// SPDX-License-Identifier: MIT
//! Comprehensive test suite for deterministic one-sided (degenerate) market settlement policies.
//! Issue #270: Policy enum, deterministic selection, event emission, value conservation, and edge cases.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::settlement::{_apply_one_sided_policy, _select_one_sided_policy};
use crate::types::{BetSide, OneSidedPolicy, OraclePayload, Policy};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, TryIntoVal,
};

fn setup_test_env() -> (Env, VirtualTokenContractClient<'static>, Address, Address) {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    (env, client, admin, oracle)
}

#[test]
fn test_one_sided_up_market() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Up);

    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 300_0000000);
    assert_eq!(round.pool_down, 0);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    attestation: None,
    });

    // Capture events before subsequent contract calls reset the log
    let events = env.events().all();
    let one_sided_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("pool"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("onesided"))
    });
    assert!(one_sided_event.is_some(), "onesided event must be emitted");

    // 100% refund of stakes
    assert_eq!(client.get_pending_winnings(&alice), 100_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 200_0000000);

    // Verify stats were NOT mutated (no fake win/loss stats on one-sided refund)
    let alice_stats = client.get_user_stats(&alice);
    assert_eq!(alice_stats.total_wins, 0);
    assert_eq!(alice_stats.total_losses, 0);
}

#[test]
fn test_one_sided_down_market() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&2_0000000, &None);
    client.place_bet(&alice, &150_0000000, &BetSide::Down);
    client.place_bet(&bob, &250_0000000, &BetSide::Down);

    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 400_0000000);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    attestation: None,
    });

    let events = env.events().all();
    let one_sided_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("pool"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("onesided"))
    });
    assert!(one_sided_event.is_some());

    assert_eq!(client.get_pending_winnings(&alice), 150_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 250_0000000);
}

#[test]
fn test_empty_market() {
    let (env, client, _admin, _oracle) = setup_test_env();

    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 0);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    attestation: None,
    });

    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_deterministic_policy_selection() {
    let (_env, client, _admin, _oracle) = setup_test_env();

    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    let selected_policy = _select_one_sided_policy(&round);
    assert_eq!(selected_policy, OneSidedPolicy::Refund);

    let contract_policy = client.get_one_sided_policy();
    assert_eq!(contract_policy, Policy::Refund);
}

#[test]
fn test_emitted_events_and_metadata() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);

    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &500_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    attestation: None,
    });

    let events = env.events().all();
    let one_sided_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("pool"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("onesided"))
    });

    assert!(one_sided_event.is_some());
}

#[test]
fn test_refund_behavior_value_preservation() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    let alice_initial = client.balance(&alice);
    let bob_initial = client.balance(&bob);
    let charlie_initial = client.balance(&charlie);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Up);
    client.place_bet(&charlie, &300_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    attestation: None,
    });

    // Claim pending winnings
    client.claim_winnings(&alice);
    client.claim_winnings(&bob);
    client.claim_winnings(&charlie);

    assert_eq!(client.balance(&alice), alice_initial);
    assert_eq!(client.balance(&bob), bob_initial);
    assert_eq!(client.balance(&charlie), charlie_initial);
}

#[test]
fn test_carry_forward_behavior_fallback() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);

    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    let round = client.get_active_round().unwrap();

    let result = env.as_contract(&client.address, || {
        _apply_one_sided_policy(
            &env,
            &round,
            OneSidedPolicy::CarryForward,
            &soroban_sdk::vec![&env, alice.clone()],
            &None,
        )
    });

    assert!(result.is_ok());
    assert_eq!(client.get_pending_winnings(&alice), 100_0000000);
}

#[test]
fn test_repeated_settlement_attempts() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);

    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let payload = OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    attestation: None,
    };

    client.resolve_round(&payload);

    // Second resolution attempt must fail with NoActiveRound
    let res = client.try_resolve_round(&OraclePayload {
        nonce: 2u64,
        ..payload
    });
    assert_eq!(res, Err(Ok(ContractError::NoActiveRound)));
}

#[test]
fn test_rounding_and_value_conservation() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    let alice_stake = 333_3333333i128;
    let bob_stake = 666_6666667i128;
    let total_pool = alice_stake + bob_stake;

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &alice_stake, &BetSide::Up);
    client.place_bet(&bob, &bob_stake, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    attestation: None,
    });

    let alice_refund = client.get_pending_winnings(&alice);
    let bob_refund = client.get_pending_winnings(&bob);

    assert_eq!(alice_refund, alice_stake);
    assert_eq!(bob_refund, bob_stake);
    assert_eq!(alice_refund + bob_refund, total_pool);
    assert_eq!(client.get_protocol_fee_treasury(), 0);
}
