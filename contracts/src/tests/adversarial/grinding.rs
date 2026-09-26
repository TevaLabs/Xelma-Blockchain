// SPDX-License-Identifier: MIT
//! Adversarial commit-reveal grinding scenarios (Issue #414).
//!
//! This module exercises grinding, griefing, and manipulation attempts against
//! the Precision-mode commit-reveal pipeline, asserting that protocol defenses
//! neutralize each attack vector.

use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Bytes, BytesN, Env,
};

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::OraclePayload;

const INITIAL_BALANCE: i128 = 1000_0000000;
const ROUND_START_PRICE: u128 = 2300;
const ORACLE_FINAL_PRICE: u128 = 2305;

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
    client.create_round(&ROUND_START_PRICE, &Some(1));
    (client, contract_id, oracle)
}

#[test]
fn test_adversarial_salt_grinding_defense() {
    let env = Env::default();
    let (client, _contract_id, _oracle) = setup(&env);
    let attacker = Address::generate(&env);
    client.mint_initial(&attacker);

    let price = 2297u128;
    let good_salt = test_salt(&env, 100);
    let hash = make_commitment(&env, price, &good_salt);
    client.commit_prediction(&attacker, &hash, &100_0000000);

    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });

    let zero_salt = BytesN::from_array(&env, &[0u8; 32]);
    assert_eq!(
        client.try_reveal_prediction(&attacker, &price, &zero_salt),
        Err(Ok(ContractError::InvalidSalt)),
        "zero-filled salt must be rejected"
    );

    let constant_salt = BytesN::from_array(&env, &[0xABu8; 32]);
    assert_eq!(
        client.try_reveal_prediction(&attacker, &price, &constant_salt),
        Err(Ok(ContractError::InvalidSalt)),
        "constant-byte salt must be rejected"
    );

    for byte_val in [1u8, 2, 4, 8, 16, 32, 64, 128, 255] {
        let mut bytes = [0u8; 32];
        bytes[0] = byte_val;
        let mismatched_salt = BytesN::from_array(&env, &bytes);
        assert_eq!(
            client.try_reveal_prediction(&attacker, &price, &mismatched_salt),
            Err(Ok(ContractError::HashMismatch)),
            "non-matching salt that passes entropy must still fail the hash check"
        );
    }

    client.reveal_prediction(&attacker, &price, &good_salt);
    let prediction = client.get_user_precision_prediction(&attacker);
    assert!(prediction.is_some(), "legitimate reveal must succeed");
    assert_eq!(prediction.unwrap().predicted_price, price);

    assert_eq!(client.balance(&attacker), INITIAL_BALANCE - 100_0000000);
}

#[test]
fn test_adversarial_commit_and_grief_non_reveal() {
    let env = Env::default();
    let (client, _contract_id, _oracle) = setup(&env);

    let griefer = Address::generate(&env);
    let honest = Address::generate(&env);
    client.mint_initial(&griefer);
    client.mint_initial(&honest);

    let griefer_bet: i128 = 500_0000000;
    let honest_bet: i128 = 100_0000000;

    let griefer_salt = test_salt(&env, 50);
    let honest_salt = test_salt(&env, 51);
    let griefer_price: u128 = 2200;
    let honest_price: u128 = 2310;

    client.commit_prediction(
        &griefer,
        &make_commitment(&env, griefer_price, &griefer_salt),
        &griefer_bet,
    );
    client.commit_prediction(
        &honest,
        &make_commitment(&env, honest_price, &honest_salt),
        &honest_bet,
    );

    assert_eq!(client.balance(&griefer), INITIAL_BALANCE - griefer_bet);
    assert_eq!(client.balance(&honest), INITIAL_BALANCE - honest_bet);

    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });
    client.reveal_prediction(&honest, &honest_price, &honest_salt);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    let round = client.get_active_round().unwrap();
    client.resolve_round(&OraclePayload {
        price: ORACLE_FINAL_PRICE,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: _contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    let total_pot = griefer_bet + honest_bet;
    assert_eq!(
        client.get_pending_winnings(&honest),
        total_pot,
        "honest user must win the full pot including griefer's forfeited stake"
    );
    assert_eq!(
        client.get_pending_winnings(&griefer),
        0,
        "griefer must have zero pending — their stake is forfeited"
    );

    client.claim_winnings(&honest);
    assert_eq!(
        client.balance(&honest),
        INITIAL_BALANCE - honest_bet + total_pot
    );
    assert_eq!(client.balance(&griefer), INITIAL_BALANCE - griefer_bet);
    assert_eq!(
        client.balance(&griefer)
            + client.balance(&honest)
            + client.get_pending_winnings(&griefer)
            + client.get_pending_winnings(&honest),
        INITIAL_BALANCE * 2,
        "conservation invariant must hold after grief resolution"
    );
}

#[test]
fn test_adversarial_cross_round_commitment_replay() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&Address::generate(&env));

    let attacker = Address::generate(&env);
    client.mint_initial(&attacker);

    client.create_round(&ROUND_START_PRICE, &Some(1));
    let price_r1: u128 = 2280;
    let salt_r1 = test_salt(&env, 70);
    let hash_r1 = make_commitment(&env, price_r1, &salt_r1);
    client.commit_prediction(&attacker, &hash_r1, &50_0000000);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    let round1 = client.get_active_round().unwrap();
    client.resolve_round(&OraclePayload {
        price: ORACLE_FINAL_PRICE,
        timestamp: env.ledger().timestamp(),
        round_id: round1.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert_eq!(
        client.get_pending_winnings(&attacker),
        50_0000000,
        "unrevealed-only round must refund"
    );
    client.claim_winnings(&attacker);

    env.ledger().with_mut(|li| {
        li.sequence_number = 14;
    });
    client.create_round(&(ROUND_START_PRICE + 100), &Some(1));

    env.ledger().with_mut(|li| {
        li.sequence_number = 20;
    });

    let result = client.try_reveal_prediction(&attacker, &price_r1, &salt_r1);
    assert_eq!(
        result,
        Err(Ok(ContractError::CommitmentNotFound)),
        "cross-round replay must be rejected — no commitment exists in round 2"
    );
}

#[test]
fn test_adversarial_double_commit_rejected() {
    let env = Env::default();
    let (client, _contract_id, _oracle) = setup(&env);

    let attacker = Address::generate(&env);
    client.mint_initial(&attacker);

    let salt1 = test_salt(&env, 80);
    let salt2 = test_salt(&env, 81);
    let price1: u128 = 2200;
    let price2: u128 = 2400;

    client.commit_prediction(
        &attacker,
        &make_commitment(&env, price1, &salt1),
        &100_0000000,
    );

    let result = client.try_commit_prediction(
        &attacker,
        &make_commitment(&env, price2, &salt2),
        &100_0000000,
    );
    assert_eq!(
        result,
        Err(Ok(ContractError::AlreadyBet)),
        "double commit must be rejected"
    );

    // Try to place direct prediction while betting window is still open
    let result2 = client.try_place_precision_prediction(&attacker, &50_0000000, &2500u128);
    assert_eq!(
        result2,
        Err(Ok(ContractError::AlreadyBet)),
        "commit-then-direct-prediction bypass must also be rejected"
    );

    // Now advance to reveal window and test reveal works
    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });
    client.reveal_prediction(&attacker, &price1, &salt1);
    let prediction = client.get_user_precision_prediction(&attacker).unwrap();
    assert_eq!(prediction.predicted_price, price1);
    assert_eq!(prediction.amount, 100_0000000);
}
