// SPDX-License-Identifier: MIT
//! Chaos and recovery tests for interrupted lifecycle actions (Issue #122).
//!
//! Each scenario models a failure-like condition or unusual execution sequence and
//! verifies that:
//!   - No funds are locked or frozen after the disruption.
//!   - Protocol invariants hold (pending == stakes, no double-spend, etc.).
//!   - Safe / retriable calls are idempotent.
//!
//! ## Modeled failure assumptions
//! - "Oracle unavailable" → admin must cancel; full refunds issued.
//! - "Double-resolve" → second call finds no active round (NoActiveRound).
//! - "Bet after round expired" → RoundEnded; user balance unchanged.
//! - "Pause during active round" → bets rejected; unpause restores normal flow.
//! - "Round with no participants resolved" → clean state, no errors.
//! - "Double-cancel" → second cancel finds no active round (RoundNotCancellable).
//! - "Claim with zero pending" → returns 0, balance unchanged (idempotent).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

// ─── helper ──────────────────────────────────────────────────────────────────

fn setup_contract() -> (
    Env,
    Address,
    Address,
    Address,
    VirtualTokenContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, contract_id, admin, oracle, client)
}

// ─── Scenario 1: Oracle unavailable — admin cancels, full refunds ─────────────

#[test]
fn test_chaos_oracle_unavailable_cancel_and_refund() {
    let (env, _cid, _admin, _oracle, client) = setup_contract();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &200_0000000, &BetSide::Up);
    client.place_bet(&bob, &300_0000000, &BetSide::Down);

    let total_staked = 200_0000000i128 + 300_0000000i128;

    // Simulate: oracle service is down → admin cancels to protect users
    client.cancel_round(&1u32); // reason=1: "oracle_unavailable"

    // Invariant: sum of refunds == total_staked
    let alice_refund = client.get_pending_winnings(&alice);
    let bob_refund = client.get_pending_winnings(&bob);
    assert_eq!(alice_refund + bob_refund, total_staked);
    assert_eq!(alice_refund, 200_0000000);
    assert_eq!(bob_refund, 300_0000000);

    // Invariant: no active round remains
    assert_eq!(client.get_active_round(), None);

    // Recovery: users claim refunds and balances are restored
    client.claim_winnings(&alice);
    client.claim_winnings(&bob);
    assert_eq!(client.balance(&alice), 1000_0000000);
    assert_eq!(client.balance(&bob), 1000_0000000);
}

// ─── Scenario 2: Double-resolve attempt ──────────────────────────────────────

#[test]
fn test_chaos_double_resolve_returns_no_active_round() {
    let (env, contract_id, _admin, _oracle, client) = setup_contract();

    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let round = client.get_active_round().unwrap();
    let payload = OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    };

    // First resolve succeeds
    client.resolve_round(&payload);
    assert_eq!(client.get_active_round(), None);

    // Second resolve attempt — no active round
    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

// ─── Scenario 3: Bet placed after betting window closes ──────────────────────

#[test]
fn test_chaos_bet_after_window_balance_unchanged() {
    let (env, _cid, _admin, _oracle, client) = setup_contract();
    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.create_round(&1_0000000, &None);

    // Advance past bet_end_ledger (default = 6)
    env.ledger().with_mut(|li| {
        li.sequence_number = 10;
    });

    let balance_before = client.balance(&user);
    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));

    // Invariant: user balance is unchanged — no funds locked
    assert_eq!(client.balance(&user), balance_before);
}

// ─── Scenario 4: Pause during active round, unpause, then resolve ─────────────

#[test]
fn test_chaos_pause_mid_round_then_unpause_resolve() {
    let (env, contract_id, _admin, _oracle, client) = setup_contract();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    // Mint user2 tokens before pausing (mint_initial is also pause-gated)
    let user2 = Address::generate(&env);
    client.mint_initial(&user2);

    // Admin pauses mid-round
    client.pause_contract();
    let result = client.try_place_bet(&user2, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));

    // Unpause and resolve normally
    client.unpause_contract();

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let round = client.get_active_round().unwrap();
    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    // Invariant: alice gets her stake back (only winner, no losers)
    assert_eq!(client.get_pending_winnings(&alice), 100_0000000);
    // Invariant: user2 balance unchanged
    assert_eq!(client.balance(&user2), 1000_0000000);
}

// ─── Scenario 5: Round with no participants resolved cleanly ──────────────────

#[test]
fn test_chaos_resolve_empty_round_clean_state() {
    let (env, contract_id, _admin, _oracle, client) = setup_contract();

    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let round = client.get_active_round().unwrap();
    // No participants — resolution should succeed with no-op payout
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    // Invariant: clean state
    assert_eq!(client.get_active_round(), None);
}

// ─── Scenario 6: Double cancel — second attempt fails gracefully ──────────────

#[test]
fn test_chaos_double_cancel_returns_not_cancellable() {
    let (_env, _cid, _admin, _oracle, client) = setup_contract();

    client.create_round(&1_0000000, &None);
    client.cancel_round(&0u32);

    // Second cancel — no active round
    let result = client.try_cancel_round(&0u32);
    assert_eq!(result, Err(Ok(ContractError::RoundNotCancellable)));
}

// ─── Scenario 7: Claim winnings with zero pending is idempotent ───────────────

#[test]
fn test_chaos_claim_zero_pending_is_idempotent() {
    let (env, _cid, _admin, _oracle, client) = setup_contract();
    let user = Address::generate(&env);
    client.mint_initial(&user);

    let balance_before = client.balance(&user);

    // No pending winnings — claim returns 0 without error
    let claimed = client.claim_winnings(&user);
    assert_eq!(claimed, 0);

    // Idempotent: balance unchanged, second claim also returns 0
    assert_eq!(client.balance(&user), balance_before);
    let claimed2 = client.claim_winnings(&user);
    assert_eq!(claimed2, 0);
}

// ─── Scenario 8: Cancel then create new round — fresh state ──────────────────

#[test]
fn test_chaos_cancel_and_restart_round_no_state_bleed() {
    let (_env, _cid, _admin, _oracle, client) = setup_contract();
    let env = &_env;
    let alice = Address::generate(env);
    let bob = Address::generate(env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Round 1: cancelled
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.cancel_round(&0u32);

    // Round 2: started fresh; bob bets, alice does NOT carry over her round-1 position
    client.create_round(&1_2000000, &None);
    client.place_bet(&bob, &50_0000000, &BetSide::Down);

    let round2 = client.get_active_round().unwrap();
    assert_eq!(round2.price_start, 1_2000000);
    assert_eq!(round2.pool_down, 50_0000000);
    // Alice's round-1 position is gone; she can place a fresh bet in round 2
    assert_eq!(client.get_user_position(&alice), None);
}


