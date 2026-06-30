// SPDX-License-Identifier: MIT
//! Overflow boundary tests for payout arithmetic in claim_winnings and helpers.
//!
//! Each test targets a specific arithmetic branch:
//!   - claim_winnings: balance + pending  (payout_add)
//!   - _record_refunds: existing_pending + position.amount  (payout_add)
//!   - _record_winnings: amount * losing_pool (payout_mul), then + share, then + existing_pending
//!
//! Overflow must return ContractError::PayoutOverflow — never a panic.

use super::config_helpers::apply_max_pending_winnings;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, DataKey, OraclePayload};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

// ─── helpers ────────────────────────────────────────────────────────────────

fn setup() -> (Env, Address, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    (env, contract_id, client)
}

fn resolve_updown(
    env: &Env,
    client: &VirtualTokenContractClient<'_>,
    contract_id: &Address,
    final_price: u128,
    run_ledgers: u32,
) {
    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = run_ledgers;
    });
    client.resolve_round(&OraclePayload {
        price: final_price,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
}

// ─── happy-path regression ───────────────────────────────────────────────────

/// Normal claim: pending winnings accumulate correctly, no overflow.
#[test]
fn test_claim_winnings_happy_path() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.initialize(&admin, &oracle);
    client.mint_initial(&alice); // 1_000_0000000
    client.mint_initial(&bob); //  1_000_0000000

    client.create_round(&1_0000000u128, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Down);

    resolve_updown(&env, &client, &contract_id, 2_0000000, 12); // price went UP — alice wins

    let pending = client.get_pending_winnings(&alice);
    assert!(pending > 0, "alice should have pending winnings");

    let claimed = client.claim_winnings(&alice);
    assert_eq!(claimed, pending);
    assert_eq!(client.get_pending_winnings(&alice), 0);
    // alice's balance = 900 (post-bet) + payout
    assert_eq!(client.balance(&alice), 900_0000000 + pending);
}

// ─── claim_winnings overflow: balance + pending ───────────────────────────────

/// Inject pending = i128::MAX and a non-zero balance → addition overflows.
/// Must return PayoutOverflow, not panic.
#[test]
fn test_claim_winnings_overflow_returns_payout_overflow() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &oracle);
    client.mint_initial(&user); // sets balance to 1_000_0000000

    // Inject i128::MAX as pending winnings directly into storage
    env.as_contract(&contract_id, || {
        let key = DataKey::PendingWinnings(user.clone());
        env.storage().persistent().set(&key, &i128::MAX);
    });

    // claim_winnings tries: balance (1_000_0000000) + i128::MAX → overflow
    let result = client.try_claim_winnings(&user);
    assert_eq!(result, Err(Ok(ContractError::PayoutOverflow)));

    // Storage must be unchanged — pending still i128::MAX, balance untouched
    assert_eq!(client.get_pending_winnings(&user), i128::MAX);
    assert_eq!(client.balance(&user), 10_000_000_000);
}

/// Inject pending = 1 and balance = i128::MAX → addition overflows.
#[test]
fn test_claim_winnings_overflow_balance_at_max() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &oracle);

    // Set balance to i128::MAX directly
    env.as_contract(&contract_id, || {
        let bal_key = DataKey::Balance(user.clone());
        env.storage().persistent().set(&bal_key, &i128::MAX);
        let win_key = DataKey::PendingWinnings(user.clone());
        env.storage().persistent().set(&win_key, &1i128);
    });

    let result = client.try_claim_winnings(&user);
    assert_eq!(result, Err(Ok(ContractError::PayoutOverflow)));

    // No partial write — storage unchanged
    assert_eq!(client.balance(&user), i128::MAX);
    assert_eq!(client.get_pending_winnings(&user), 1);
}

// ─── _record_winnings overflow: payout_mul branch ────────────────────────────

/// Place bets such that amount * losing_pool overflows i128.
/// The pool totals can't realistically reach i128::MAX through mint_initial
/// (max mint = 1_000_0000000), so we inject the pool via storage.
#[test]
fn test_record_winnings_mul_overflow_returns_payout_overflow() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);

    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);

    client.create_round(&1_0000000u128, &None);
    // alice bets 1 up
    client.place_bet(&alice, &1_0000000, &BetSide::Up);

    // Inject an enormous losing_pool (pool_down) into ActiveRound so that
    // alice.amount (1_0000000) * losing_pool overflows i128
    env.as_contract(&contract_id, || {
        let mut round: crate::types::Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .unwrap();
        round.pool_down = i128::MAX; // causes payout_mul overflow
        env.storage()
            .persistent()
            .set(&DataKey::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let round = client.get_active_round().unwrap();

    let result = client.try_resolve_round(&OraclePayload {
        price: 2_0000000, // price went UP — alice wins
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    assert_eq!(result, Err(Ok(ContractError::PayoutOverflow)));
}

// ─── _record_refunds overflow: existing_pending + refund ─────────────────────

/// existing_pending = i128::MAX - 1, refund amount = 2 → overflow.
#[test]
fn test_record_refunds_overflow_returns_payout_overflow() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);

    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);

    client.create_round(&1_0000000u128, &None);
    client.place_bet(&alice, &2_0000000, &BetSide::Up);

    // Inject near-max existing pending winnings for alice
    env.as_contract(&contract_id, || {
        let key = DataKey::PendingWinnings(alice.clone());
        env.storage().persistent().set(&key, &(i128::MAX - 1));
    });

    // Resolve with unchanged price → refunds triggered
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let round = client.get_active_round().unwrap();

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_0000000, // same as start_price → tie → refund
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    assert_eq!(result, Err(Ok(ContractError::PayoutOverflow)));
}

// ─── boundary: values just below overflow must succeed ───────────────────────

/// Ensure values at i128::MAX - 1 + 0 = i128::MAX - 1 (no overflow) succeed.
#[test]
fn test_claim_winnings_near_max_succeeds() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &oracle);

    // balance = 0, pending = i128::MAX  → new_balance = i128::MAX (no overflow)
    env.as_contract(&contract_id, || {
        let win_key = DataKey::PendingWinnings(user.clone());
        env.storage().persistent().set(&win_key, &i128::MAX);
    });

    let claimed = client.claim_winnings(&user);
    assert_eq!(claimed, i128::MAX);
    assert_eq!(client.balance(&user), i128::MAX);
    assert_eq!(client.get_pending_winnings(&user), 0);
}

// ─── Pending winnings cap tests (Issue #120) ─────────────────────────────────

#[test]
fn test_pending_winnings_cap_enforced_on_refund() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);

    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);

    // Set cap to 50
    apply_max_pending_winnings(&env, &client, Some(50_0000000i128));

    // Alice bets 100 — on refund (price unchanged) pending would be 100 > cap 50
    client.create_round(&1_0000000u128, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let round = client.get_active_round().unwrap();

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_0000000, // same price → refund
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::PendingWinningsCapExceeded)));

    // Balance unchanged — all-or-nothing guarantee
    assert_eq!(client.balance(&alice), 900_0000000);
}

#[test]
fn test_pending_winnings_cap_enforced_on_winnings() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Alice wins 100 + share of bob's 100 = 200; set cap to 100
    apply_max_pending_winnings(&env, &client, Some(100_0000000i128));

    client.create_round(&1_0000000u128, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &100_0000000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let round = client.get_active_round().unwrap();

    // price went UP — alice wins 200 total, but cap is 100
    let result = client.try_resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::PendingWinningsCapExceeded)));
}

#[test]
fn test_pending_winnings_cap_not_exceeded_succeeds() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Alice bets 100 UP, bob 100 DOWN → alice wins 200. Set cap to 200 (exactly at cap).
    apply_max_pending_winnings(&env, &client, Some(200_0000000i128));

    client.create_round(&1_0000000u128, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &100_0000000, &BetSide::Down);

    resolve_updown(&env, &client, &contract_id, 2_0000000, 12);

    // Alice's pending = 200 == cap → OK
    let pending = client.get_pending_winnings(&alice);
    assert_eq!(pending, 200_0000000);
}

#[test]
fn test_pending_winnings_cap_disabled_large_payout_succeeds() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Set then remove cap
    apply_max_pending_winnings(&env, &client, Some(50_0000000i128));
    apply_max_pending_winnings(&env, &client, None);

    client.create_round(&1_0000000u128, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &100_0000000, &BetSide::Down);

    resolve_updown(&env, &client, &contract_id, 2_0000000, 12);

    // Cap disabled — payout proceeds normally
    let pending = client.get_pending_winnings(&alice);
    assert!(pending > 0);
}

#[test]
fn test_get_max_pending_winnings_returns_configured_value() {
    let (env, _cid, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    client.initialize(&admin, &oracle);

    assert_eq!(client.get_max_pending_winnings(), None);
    apply_max_pending_winnings(&env, &client, Some(500_0000000i128));
    assert_eq!(client.get_max_pending_winnings(), Some(500_0000000i128));
    apply_max_pending_winnings(&env, &client, None);
    assert_eq!(client.get_max_pending_winnings(), None);
}


