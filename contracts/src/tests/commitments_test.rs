// SPDX-License-Identifier: MIT
//! Unit and integration test suite for the commitments module, rolling aggregate tracking,
//! open path after betting close, and settlement hooks.

use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger as _},
    Address, Bytes, BytesN, Env, TryIntoVal,
};

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, OraclePayload, RoundMode, RoundPhase};

const ROUND_START_PRICE: u128 = 2300;
const ORACLE_FINAL_PRICE: u128 = 2310;

fn make_commitment(env: &Env, price: u128, salt: &BytesN<32>) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&price.to_xdr(env));
    preimage.append(&salt.clone().to_xdr(env));
    let hash = env.crypto().sha256(&preimage);
    hash.into()
}

fn test_salt(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        bytes[i] = seed.wrapping_add(i as u8).wrapping_mul(17).wrapping_add(3);
        i += 1;
    }
    bytes[0] = seed | 0x80;
    bytes[31] = seed ^ 0x5A;
    BytesN::from_array(env, &bytes)
}

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    (client, contract_id, oracle)
}

#[test]
fn test_rolling_commitment_lifecycle_and_blinding() {
    let env = Env::default();
    let (client, _contract_id, _oracle) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Create round at ledger 0 (bet window [0, 6), run window [6, 12))
    client.create_round(&ROUND_START_PRICE, &Some(0)); // UpDown mode
    let round_id = client.get_active_round().unwrap().round_id;

    // 1. Initial rolling commitment state
    let rolling_init = client
        .get_rolling_commitment(&round_id)
        .expect("rolling commitment must be initialized");
    assert_eq!(rolling_init.total_commitments, 0);
    assert_eq!(rolling_init.total_stake, 0);
    assert!(!rolling_init.is_opened);

    // 2. Place bets during betting phase
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    let bet_event = env
        .events()
        .all()
        .iter()
        .rev()
        .find(|(_, topics, _)| {
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(&env)
                    == Ok(soroban_sdk::symbol_short!("bet"))
                && topics.get(1).unwrap().try_into_val(&env)
                    == Ok(soroban_sdk::symbol_short!("placed"))
        })
        .expect("bet event should be emitted");
    let (_, _, event_data) = bet_event;
    let event_payload: (Address, u64, BytesN<32>, u32) = event_data
        .try_into_val(&env)
        .expect("aggregate-only event payload");
    assert_eq!(event_payload.0, alice);
    assert_eq!(event_payload.1, round_id);
    assert_eq!(event_payload.3, 1);
    let legacy_payload: Result<(Address, u64, i128, u32), _> = event_data.try_into_val(&env);
    assert!(legacy_payload.is_err());

    client.place_bet(&bob, &200_0000000, &BetSide::Down);

    let rolling_after_bets = client.get_rolling_commitment(&round_id).unwrap();
    assert_eq!(rolling_after_bets.total_commitments, 2);
    assert_eq!(rolling_after_bets.total_stake, 300_0000000);
    assert_ne!(
        rolling_after_bets.commitment_hash,
        rolling_init.commitment_hash
    );

    // 3. During betting phase: public pool stats must blind raw side breakdown to avoid pool leaks
    let stats_betting = client.get_round_pool_stats().unwrap();
    assert_eq!(
        stats_betting.total_up_stake, 0,
        "total_up_stake must be blinded during betting phase"
    );
    assert_eq!(
        stats_betting.total_down_stake, 0,
        "total_down_stake must be blinded during betting phase"
    );
    assert_eq!(stats_betting.up_stake_ratio_bps, 0);
    assert_eq!(stats_betting.down_stake_ratio_bps, 0);

    // 4. Advance ledger past bet_end_ledger (phase is now Running / open path)
    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });

    let phase = client.get_round_phase();
    assert_ne!(phase, RoundPhase::Betting);
    assert!(client.get_rolling_commitment(&round_id).unwrap().is_opened);

    // Open path: raw side pools are now revealed in pool stats
    let stats_running = client.get_round_pool_stats().unwrap();
    assert_eq!(
        stats_running.total_up_stake, 100_0000000,
        "open path must reveal total_up_stake after close"
    );
    assert_eq!(
        stats_running.total_down_stake, 200_0000000,
        "open path must reveal total_down_stake after close"
    );

    // 5. Settlement hook: resolve round and verify rolling commitment is opened
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    let active_round = client.get_active_round().unwrap();
    client.resolve_round(&OraclePayload {
        price: ORACLE_FINAL_PRICE,
        timestamp: env.ledger().timestamp(),
        round_id: active_round.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: _contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    // Settlement hook opened the commitment aggregate
    let rolling_settled = client.get_rolling_commitment(&round_id).unwrap();
    assert!(
        rolling_settled.is_opened,
        "settlement hook must set is_opened to true"
    );
}

#[test]
fn test_precision_mode_rolling_commitment_accumulation() {
    let env = Env::default();
    let (client, _contract_id, _oracle) = setup(&env);

    let committer = Address::generate(&env);
    client.mint_initial(&committer);

    client.create_round(&ROUND_START_PRICE, &Some(1)); // Precision mode
    let round_id = client.get_active_round().unwrap().round_id;

    let salt = test_salt(&env, 42);
    let hash = make_commitment(&env, 2305, &salt);

    client.commit_prediction(&committer, &hash, &150_0000000);

    let rolling = client.get_rolling_commitment(&round_id).unwrap();
    assert_eq!(rolling.total_commitments, 1);
    assert_eq!(rolling.total_stake, 150_0000000);
    assert!(!rolling.is_opened);

    let stats = client.get_round_pool_stats().unwrap();
    assert_eq!(stats.precision_total_stake, rolling.total_stake);
    assert_eq!(stats.precision_participant_count, rolling.total_commitments);
    assert_eq!(stats.precision_commitment_count, rolling.total_commitments);
    assert_eq!(stats.precision_prediction_count, 0);
    assert_eq!(stats.precision_revealed_count, 0);
}
