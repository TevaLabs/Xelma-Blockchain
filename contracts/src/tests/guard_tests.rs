// SPDX-License-Identifier: MIT
//! Tests for the single-active-round invariant guard (assert_no_active_round).
//!
//! Success path: no active round → create_round proceeds, storage updated.
//! Failure path: active round present → RoundAlreadyActive returned, storage
//!               snapshot confirms no mutation occurred.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{DataKey, OraclePayload, Round};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

// ─── success path ────────────────────────────────────────────────────────────

/// No active round → create_round proceeds, ActiveRound and LastRoundId written.
#[test]
fn test_guard_success_path_no_active_round() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    // Pre-condition: no active round
    assert!(client.get_active_round().is_none());

    // Action: create a round
    let start_price: u128 = 1_5000000;
    client.create_round(&start_price, &None);

    // Post-condition: active round stored with correct values
    let round = client
        .get_active_round()
        .expect("round must exist after create");
    assert_eq!(round.price_start, start_price);
    assert_eq!(round.round_id, 1);
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 0);

    // LastRoundId incremented
    assert_eq!(client.get_last_round_id(), 1);
}

/// After resolving, a new round can be created (guard passes again).
#[test]
fn test_guard_passes_after_round_resolved() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();

    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    assert!(client.get_active_round().is_none());

    // Second round must succeed
    client.create_round(&2_0000000u128, &None);
    let round2 = client.get_active_round().unwrap();
    assert_eq!(round2.round_id, 2);
    assert_eq!(round2.price_start, 2_0000000);
}

// ─── failure path ────────────────────────────────────────────────────────────

/// Active round present → RoundAlreadyActive returned, storage unchanged.
#[test]
fn test_guard_failure_path_active_round_exists() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    // Create first round
    let start_price: u128 = 1_5000000;
    client.create_round(&start_price, &None);

    // Capture storage snapshot before the rejected attempt
    let existing_round: Round = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .unwrap()
    });
    let last_round_id_before = client.get_last_round_id();

    // Attempt to create a second round while first is active
    let result = client.try_create_round(&2_0000000, &None);

    // Exact error variant asserted
    assert_eq!(result, Err(Ok(ContractError::RoundAlreadyActive)));

    // Storage snapshot: ActiveRound unchanged
    let round_after: Round = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .unwrap()
    });
    assert_eq!(round_after.round_id, existing_round.round_id);
    assert_eq!(round_after.price_start, existing_round.price_start);
    assert_eq!(round_after.start_ledger, existing_round.start_ledger);
    assert_eq!(round_after.bet_end_ledger, existing_round.bet_end_ledger);
    assert_eq!(round_after.end_ledger, existing_round.end_ledger);

    // LastRoundId not incremented — no mutation occurred
    assert_eq!(client.get_last_round_id(), last_round_id_before);
}

/// Repeated rejection attempts do not corrupt state.
#[test]
fn test_guard_repeated_rejections_do_not_corrupt_state() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    client.create_round(&1_0000000u128, &None);
    let original_round = client.get_active_round().unwrap();

    // Attempt 5 times — each must fail with the same error and leave state intact
    for i in 0..5u128 {
        let result = client.try_create_round(&(2_0000000 + i), &None);
        assert_eq!(result, Err(Ok(ContractError::RoundAlreadyActive)));
    }

    let round_after = client.get_active_round().unwrap();
    assert_eq!(round_after.round_id, original_round.round_id);
    assert_eq!(round_after.price_start, original_round.price_start);
    assert_eq!(client.get_last_round_id(), 1);
}


