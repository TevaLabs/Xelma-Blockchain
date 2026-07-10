// SPDX-License-Identifier: MIT
//! Tests for explicit global and round status codes (Issue #199).
//!
//! Validates that `get_protocol_status` and `get_round_status` return correct
//! stable codes at every lifecycle stage, so that frontend state machines can
//! rely on a single endpoint instead of stitching together multiple flags.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, OraclePayload, ProtocolStatus, RoundStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

fn setup_contract(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address) {
    env.ledger().with_mut(|li| {
        li.sequence_number = 1;
    });
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    (client, admin, oracle)
}

// ─── ProtocolStatus tests ────────────────────────────────────────────────────

/// After initialization the contract has no active round: ClaimsOnly.
#[test]
fn test_protocol_status_initial_is_claims_only() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
}

/// Creating a round transitions the protocol to Active.
#[test]
fn test_protocol_status_active_when_round_exists() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    client.create_round(&10_0000u128, &None);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);
}

/// Pausing an idle (no active round) contract yields Paused.
/// Unpausing yields ClaimsOnly (not Active — no round was started).
#[test]
fn test_protocol_status_paused_idle() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    client.pause_contract();
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Paused);

    client.unpause_contract();
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
}

/// Pausing while a round is active yields Paused (takes priority over Active).
/// Unpausing with an active round still present yields Active.
#[test]
fn test_protocol_status_paused_with_active_round() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    client.create_round(&10_0000u128, &None);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);

    // Pause takes priority
    client.pause_contract();
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Paused);

    // Unpause restores Active because the round is still live
    client.unpause_contract();
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);
}

/// After resolve_round completes, there is no active round: ClaimsOnly.
#[test]
fn test_protocol_status_claims_only_after_resolve() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);
    let user = Address::generate(&env);

    client.mint_initial(&user);
    client.create_round(&10_0000u128, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| li.sequence_number = 15);

    let payload = OraclePayload {
        price: 11_0000,
        timestamp: env.ledger().timestamp(),
        round_id: 1,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
    };
    client.resolve_round(&payload);

    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
}

/// After cancel_round, no active round remains: ClaimsOnly.
#[test]
fn test_protocol_status_claims_only_after_cancel() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    client.create_round(&10_0000u128, &None);
    client.cancel_round(&1);

    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
}

// ─── RoundStatus tests ───────────────────────────────────────────────────────

/// Querying a round_id that was never created returns Unknown.
#[test]
fn test_round_status_unknown_for_nonexistent_round() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    // No round has ever been created
    assert_eq!(client.get_round_status(&1), RoundStatus::Unknown);
    assert_eq!(client.get_round_status(&42), RoundStatus::Unknown);
    assert_eq!(client.get_round_status(&999), RoundStatus::Unknown);
}

/// After creating a round it starts in Betting phase.
/// Round id queries for other ids remain Unknown.
#[test]
fn test_round_status_unknown_for_other_ids() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    client.create_round(&10_0000u128, &None);
    // round 1 is active
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);
    // round 2 doesn't exist
    assert_eq!(client.get_round_status(&2), RoundStatus::Unknown);
}

/// Full happy-path lifecycle: Unknown → Betting → Running → AwaitingResolve → Resolved.
#[test]
fn test_round_status_full_lifecycle() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);
    let user = Address::generate(&env);

    // 1. Before any round: Unknown
    assert_eq!(client.get_round_status(&1), RoundStatus::Unknown);

    // 2. Create round → Betting
    client.mint_initial(&user);
    client.create_round(&10_0000u128, &None);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);

    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // 3. Advance past bet window (default: 6 ledgers) → Running
    // start_ledger=1, bet_end_ledger=1+6=7, end_ledger=1+6+12=19
    env.ledger().with_mut(|li| li.sequence_number = 8);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);
    assert_eq!(client.get_round_status(&1), RoundStatus::Running);

    // 4. Advance past run window (default: 12 ledgers) → AwaitingResolve
    env.ledger().with_mut(|li| li.sequence_number = 20);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);
    assert_eq!(client.get_round_status(&1), RoundStatus::AwaitingResolve);

    // 5. Resolve → Resolved; protocol returns to ClaimsOnly
    let payload = OraclePayload {
        price: 11_0000,
        timestamp: env.ledger().timestamp(),
        round_id: 1,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
    };
    client.resolve_round(&payload);

    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
    assert_eq!(client.get_round_status(&1), RoundStatus::Resolved);
}

/// cancel_round yields Cancelled regardless of which sub-phase the round was in.
#[test]
fn test_round_status_cancelled() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    client.create_round(&10_0000u128, &None);

    // Cancel from Betting phase
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);
    client.cancel_round(&1);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
    assert_eq!(client.get_round_status(&1), RoundStatus::Cancelled);
}

/// Cancelling a round from the Running phase also yields Cancelled.
#[test]
fn test_round_status_cancelled_from_running() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    client.create_round(&10_0000u128, &None);

    // Advance into Running phase
    env.ledger().with_mut(|li| li.sequence_number = 8);
    assert_eq!(client.get_round_status(&1), RoundStatus::Running);

    client.cancel_round(&1);
    assert_eq!(client.get_round_status(&1), RoundStatus::Cancelled);
}

/// Cancelling a round from AwaitingResolve also yields Cancelled.
#[test]
fn test_round_status_cancelled_from_awaiting_resolve() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    client.create_round(&10_0000u128, &None);

    env.ledger().with_mut(|li| li.sequence_number = 20);
    assert_eq!(client.get_round_status(&1), RoundStatus::AwaitingResolve);

    client.cancel_round(&1);
    assert_eq!(client.get_round_status(&1), RoundStatus::Cancelled);
}

/// When min_participants is set and not met, resolve yields FallbackRefund.
#[test]
fn test_round_status_fallback_refund() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);
    let user = Address::generate(&env);

    // Require 2 participants but only 1 bets
    client.set_min_participants(&Some(2u32));
    client.mint_initial(&user);

    client.create_round(&10_0000u128, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| li.sequence_number = 20);
    assert_eq!(client.get_round_status(&1), RoundStatus::AwaitingResolve);

    let payload = OraclePayload {
        price: 11_0000,
        timestamp: env.ledger().timestamp(),
        round_id: 1,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
    };
    client.resolve_round(&payload);

    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
    assert_eq!(client.get_round_status(&1), RoundStatus::FallbackRefund);
}

/// Pausing the contract does NOT change the round's own temporal status —
/// the phase is purely derived from ledger sequence and round ledger bounds.
#[test]
fn test_round_status_unaffected_by_pause() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    client.create_round(&10_0000u128, &None);
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);

    client.pause_contract();
    // Protocol is paused but the round's temporal phase is unchanged
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Paused);
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);

    // Advance ledger — phase still advances even while paused
    env.ledger().with_mut(|li| li.sequence_number = 8);
    assert_eq!(client.get_round_status(&1), RoundStatus::Running);

    client.unpause_contract();
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);
    assert_eq!(client.get_round_status(&1), RoundStatus::Running);
}
