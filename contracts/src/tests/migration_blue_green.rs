// SPDX-License-Identifier: MIT
//! Tests for the blue/green migration subsystem (Issue #366): canonical
//! encoding, Merkle commitment/proofs, export/import lifecycle, replay
//! protection, drain-mode freeze, and balance conservation.

use crate::admin::get_admin;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::migration::{
    _balance_leaf, _config_leaf, _merkle_root, _null_leaf, _read_canonical_config,
    MIGRATION_DESTINATION_VERSION,
};
use crate::types::{BetSide, MerkleProof, MigrationBalance, MigrationCommitment, MigrationKey};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{vec, Address, Bytes, BytesN, Env, Vec};

fn deploy<'a>(
    env: &'a Env,
    admin: &Address,
    oracle: &Address,
) -> (VirtualTokenContractClient<'a>, Address) {
    let cid: Address = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &cid);
    client.initialize(admin, oracle);
    (client, cid)
}

fn hash_pair(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> {
    let mut buf = Bytes::new(env);
    buf.extend_from_array(&left.to_array());
    buf.extend_from_array(&right.to_array());
    env.crypto().sha256(&buf).into()
}

/// Recomputes the padded Merkle tree and returns the siblings and height for
/// the given leaf index (test-side mirror of the contract implementation).
fn compute_proof(env: &Env, leaves: &Vec<BytesN<32>>, target: u32) -> MerkleProof {
    if leaves.is_empty() {
        panic!("no leaves");
    }
    let mut size = 1u32;
    while size < leaves.len() {
        size = size.saturating_mul(2);
    }
    let mut lvl: Vec<BytesN<32>> = Vec::new(env);
    for l in leaves.iter() {
        lvl.push_back(l);
    }
    while lvl.len() < size {
        lvl.push_back(_null_leaf(env));
    }
    let mut height = 0u32;
    let mut index = target;
    let mut siblings: Vec<BytesN<32>> = Vec::new(env);
    while lvl.len() > 1 {
        let mut next: Vec<BytesN<32>> = Vec::new(env);
        let mut i = 0u32;
        while i + 1 < lvl.len() {
            let a = lvl.get(i).unwrap();
            let b = lvl.get(i + 1).unwrap();
            next.push_back(hash_pair(env, &a, &b));
            i += 2;
        }
        if i < lvl.len() {
            let lone = lvl.get(i).unwrap();
            next.push_back(lone);
        }
        let sib_index = if index.is_multiple_of(2) {
            index + 1
        } else {
            index - 1
        };
        if sib_index < lvl.len() {
            siblings.push_back(lvl.get(sib_index).unwrap());
        } else {
            siblings.push_back(_null_leaf(env));
        }
        index /= 2;
        lvl = next;
        height += 1;
    }
    MerkleProof {
        leaf_index: target,
        tree_height: height,
        siblings,
    }
}

fn sorted_balance_leaves(env: &Env, version: u32, recs: &Vec<MigrationBalance>) -> Vec<BytesN<32>> {
    let mut std_v: alloc::vec::Vec<MigrationBalance> = alloc::vec::Vec::new();
    for r in recs.iter() {
        std_v.push(r);
    }
    std_v.sort_by_key(|a| a.user.to_string());
    let mut v: Vec<BytesN<32>> = Vec::new(env);
    for r in std_v {
        v.push_back(_balance_leaf(env, version, &r));
    }
    v
}

#[test]
fn test_canonical_merkle_commitment_matches_spec() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    let (client, cid) = deploy(&env, &admin, &oracle);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    client.mint_initial(&u1);
    client.mint_initial(&u2);

    client.migration_export_start(&false);
    client.migration_export_balances(&vec![&env, u1.clone(), u2.clone()], &false);

    let version = 3u32;
    let config = env.as_contract(&cid, || _read_canonical_config(&env));
    let mut leaves: Vec<BytesN<32>> = Vec::new(&env);
    leaves.push_back(_config_leaf(&env, version, &config));
    for l in sorted_balance_leaves(
        &env,
        version,
        &vec![
            &env,
            MigrationBalance {
                user: u1.clone(),
                amount: 1000_0000000i128,
            },
            MigrationBalance {
                user: u2.clone(),
                amount: 1000_0000000i128,
            },
        ],
    )
    .iter()
    {
        leaves.push_back(l);
    }
    let expected_root = _merkle_root(&env, &leaves);

    client.migration_export_finalize(&false);

    env.as_contract(&cid, || {
        let c: MigrationCommitment = env
            .storage()
            .persistent()
            .get(&MigrationKey::Commitment)
            .unwrap();
        assert_eq!(c.source_version, 3);
        assert_eq!(c.destination_version, MIGRATION_DESTINATION_VERSION);
        assert_eq!(c.leaf_count, leaves.len());
        assert_eq!(c.root, expected_root);
    });
}

#[test]
fn test_freeze_blocks_mutation_but_allows_reads() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    let (client, _cid) = deploy(&env, &admin, &oracle);

    client.migration_export_start(&false);
    client.migration_export_finalize(&false);

    let u = Address::generate(&env);
    let res = client.try_create_round(&1_0000000u128, &None);
    assert_eq!(res, Err(Ok(ContractError::MigrationFrozen)));
    let res = client.try_place_bet(&u, &100, &BetSide::Up);
    assert_eq!(res, Err(Ok(ContractError::MigrationFrozen)));
    let res = client.try_mint_initial(&u);
    assert!(res.is_err());
    let status = client.migration_get_status();
    assert!(status.frozen);
    assert!(status.finalized);
}

#[test]
fn test_export_finalize_rejects_duplicate() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    let (client, _cid) = deploy(&env, &admin, &oracle);

    client.migration_export_start(&false);
    client.migration_export_finalize(&false);
    let res = client.try_migration_export_finalize(&false);
    assert_eq!(res, Err(Ok(ContractError::MigrationAlreadyFinalized)));
}

#[test]
fn test_export_blocked_when_round_active() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    let (client, _cid) = deploy(&env, &admin, &oracle);

    client.create_round(&1_0000000u128, &None);
    let res = client.try_migration_export_start(&false);
    assert_eq!(res, Err(Ok(ContractError::MigrationActiveRound)));
}

#[test]
fn test_import_completeness_replay_and_conservation() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    // --- source ---
    let (src, sid) = deploy(&env, &admin, &oracle);
    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    src.mint_initial(&u1);
    src.mint_initial(&u2);
    src.migration_export_start(&false);
    src.migration_export_balances(&vec![&env, u1.clone(), u2.clone()], &false);
    src.migration_export_finalize(&false);

    let commitment: MigrationCommitment = env.as_contract(&sid, || {
        env.storage()
            .persistent()
            .get(&MigrationKey::Commitment)
            .unwrap()
    });
    let version = commitment.source_version;
    let config = env.as_contract(&sid, || _read_canonical_config(&env));

    let bal_recs = vec![
        &env,
        MigrationBalance {
            user: u1.clone(),
            amount: 1000_0000000i128,
        },
        MigrationBalance {
            user: u2.clone(),
            amount: 1000_0000000i128,
        },
    ];

    // --- destination ---
    let (dst, did) = deploy(&env, &admin, &oracle);
    dst.migration_import_init(
        &commitment.root,
        &version,
        &MIGRATION_DESTINATION_VERSION,
        &commitment.leaf_count,
    );

    let mut leaves: Vec<BytesN<32>> = Vec::new(&env);
    leaves.push_back(_config_leaf(&env, version, &config));
    for l in sorted_balance_leaves(&env, version, &bal_recs).iter() {
        leaves.push_back(l);
    }

    // Config leaf is index 0.
    let cfg_proof = compute_proof(&env, &leaves, 0);
    dst.migration_import_config(&config, &cfg_proof);

    // Incomplete -> cannot finalize yet.
    let res = dst.try_migration_import_finalize();
    assert_eq!(res, Err(Ok(ContractError::MigrationExportIncomplete)));

    // Forged amount fails proof (leaf hash mismatch with root).
    let bad_rec = MigrationBalance {
        user: u1.clone(),
        amount: 999_i128,
    };
    let u1_index = leaves.len() - 2; // leaves = [config, u1, u2] sorted
    let forged_proof = compute_proof(&env, &leaves, u1_index);
    let res = dst.try_migration_import_balance(&bad_rec, &forged_proof);
    assert_eq!(res, Err(Ok(ContractError::MigrationProofInvalid)));

    // Import real balances.
    let u1_index = leaves.len() - 2;
    let u2_index = leaves.len() - 1;
    let p1 = compute_proof(&env, &leaves, u1_index);
    let p2 = compute_proof(&env, &leaves, u2_index);
    dst.migration_import_balance(&bal_recs.get(0).unwrap(), &p1);
    // Replay: same user again fails.
    let res = dst.try_migration_import_balance(&bal_recs.get(0).unwrap(), &p1);
    assert_eq!(res, Err(Ok(ContractError::MigrationRecordAlreadyImported)));
    dst.migration_import_balance(&bal_recs.get(1).unwrap(), &p2);

    // Now complete -> finalize succeeds.
    dst.migration_import_finalize();

    // Balance conservation: destination matches source balances.
    let u1_bal = env.as_contract(&did, || crate::common::balance(env.clone(), u1.clone()));
    let u2_bal = env.as_contract(&did, || crate::common::balance(env.clone(), u2.clone()));
    assert_eq!(u1_bal, 1000_0000000i128);
    assert_eq!(u2_bal, 1000_0000000i128);

    // Admin preserved.
    let src_admin = env.as_contract(&sid, || get_admin(env.clone()));
    assert_eq!(src_admin, Some(admin.clone()));
}

#[test]
fn test_dry_run_export_does_not_mutate() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    let (client, _cid) = deploy(&env, &admin, &oracle);

    let res = client.try_migration_export_start(&true);
    assert_eq!(res, Ok(Ok(())));
    let res = client.try_migration_export_finalize(&true);
    assert_eq!(res, Ok(Ok(())));

    let status = client.migration_get_status();
    assert!(!status.finalized);
    assert!(!status.frozen);
}
