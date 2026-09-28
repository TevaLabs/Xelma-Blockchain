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
    client.update_oracle_heartbeat(&0u32);

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
        confidence: None,
        attestation: None,    };
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
        confidence: None,
        attestation: None,    };
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
        confidence: None,
        attestation: None,    };
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

// ─── RuntimeMode consistency (Issue #567) ────────────────────────────────────
//
// `get_protocol_status`, `get_protocol_health` and `get_runtime_mode` must
// never disagree about what "paused" means. These tests pin every
// (RuntimeMode × active-round) cell of the table in docs/STATUS_CODES.md and
// check each reported status against what the policy gate actually allows.

use crate::errors::ContractError;

/// Probes the real policy gate: can `bettor` (pre-funded) bet, and can a
/// fresh user claim? Fresh users keep the probes from disturbing the round.
fn probe_gate(
    env: &Env,
    client: &VirtualTokenContractClient<'_>,
    bettor: &Address,
) -> (bool, bool) {
    let can_bet = client
        .try_place_bet(bettor, &10_0000000, &BetSide::Up)
        .map(|r| r.is_ok())
        .unwrap_or(false);
    let claimer = Address::generate(env);
    let can_claim = match client.try_claim_winnings(&claimer) {
        Ok(_) => true,
        Err(Ok(ContractError::ContractPaused)) => false,
        Err(other) => panic!("unexpected claim error: {:?}", other),
    };
    (can_bet, can_claim)
}

#[test]
fn test_status_matrix_matches_runtime_mode_and_policy_gate() {
    // (mode, with_round, expected ProtocolStatus, bets allowed, claims allowed)
    let cases = [
        (0u32, false, ProtocolStatus::ClaimsOnly, false, true),
        (0u32, true, ProtocolStatus::Active, true, true),
        (1u32, false, ProtocolStatus::ClaimsOnly, false, true),
        (1u32, true, ProtocolStatus::ClaimsOnly, false, true),
        (2u32, false, ProtocolStatus::Paused, false, false),
        (2u32, true, ProtocolStatus::Paused, false, false),
    ];

    for (mode, with_round, expected, bets, claims) in cases {
        let env = Env::default();
        let (client, _admin, _oracle) = setup_contract(&env);
        if with_round {
            client.create_round(&10_0000u128, &None);
        }
        // Fund the probe bettor while minting is still allowed.
        let bettor = Address::generate(&env);
        client.mint_initial(&bettor);
        client.set_runtime_mode(&mode);

        let status = client.get_protocol_status();
        assert_eq!(status, expected, "mode={} round={}", mode, with_round);
        assert_eq!(client.get_runtime_mode(), mode);
        assert_eq!(client.is_paused(), mode == 2);

        let health = client.get_protocol_health();
        assert_eq!(
            health.paused,
            mode == 2,
            "health.paused must mean FullyPaused only"
        );
        assert_eq!(health.has_active_round, with_round);

        // `Paused` <=> claims blocked; `Active` <=> bets accepted.
        let (can_bet, can_claim) = probe_gate(&env, &client, &bettor);
        assert_eq!(can_bet, bets, "bet gate mode={} round={}", mode, with_round);
        assert_eq!(
            can_claim, claims,
            "claim gate mode={} round={}",
            mode, with_round
        );
        assert_eq!(status == ProtocolStatus::Active, can_bet);
        assert_eq!(status == ProtocolStatus::Paused, !can_claim);
    }
}

/// Regression: ClaimsOnly with a live round used to report `Active` even
/// though every bet was rejected with `ContractPaused`.
#[test]
fn test_protocol_status_claims_only_mode_with_active_round() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);

    client.create_round(&10_0000u128, &None);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);

    client.set_runtime_mode(&1u32);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
    // The round's own lifecycle phase is independent of RuntimeMode.
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);
    assert_eq!(client.get_protocol_health().status_code, 6); // CLAIMS_ONLY

    client.set_runtime_mode(&0u32);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);
    assert_eq!(client.get_protocol_health().status_code, 0); // HEALTHY
}

/// `set_runtime_mode(2)` and `pause_contract()` are the same state and must
/// be reported identically by every status surface.
#[test]
fn test_pause_contract_and_mode_two_are_indistinguishable() {
    let env_a = Env::default();
    let (a, _, _) = setup_contract(&env_a);
    a.create_round(&10_0000u128, &None);
    a.pause_contract();

    let env_b = Env::default();
    let (b, _, _) = setup_contract(&env_b);
    b.create_round(&10_0000u128, &None);
    b.set_runtime_mode(&2u32);

    assert_eq!(a.get_runtime_mode(), b.get_runtime_mode());
    assert_eq!(a.get_protocol_status(), b.get_protocol_status());
    assert_eq!(a.get_round_status(&1), b.get_round_status(&1));
    let (ha, hb) = (a.get_protocol_health(), b.get_protocol_health());
    assert_eq!(ha.paused, hb.paused);
    assert_eq!(ha.status_code, hb.status_code);
    assert_eq!(ha.status_code, 1); // PAUSED
}

/// ClaimsOnly counts as one degradation: combined with a stale oracle it must
/// surface MULTIPLE_ISSUES instead of hiding the oracle problem.
#[test]
fn test_health_claims_only_does_not_mask_stale_oracle() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup_contract(&env);
    client.create_round(&10_0000u128, &None);
    client.set_runtime_mode(&1u32);
    assert_eq!(client.get_protocol_health().status_code, 6); // CLAIMS_ONLY alone

    // Push the ledger clock past the oracle stale threshold.
    let threshold = client.get_oracle_stale_threshold();
    env.ledger().with_mut(|li| li.timestamp += threshold + 1);
    let health = client.get_protocol_health();
    assert!(!health.oracle_live);
    assert_eq!(health.status_code, 5); // MULTIPLE_ISSUES

    // Escalating to FullyPaused always wins.
    client.set_runtime_mode(&2u32);
    assert_eq!(client.get_protocol_health().status_code, 1); // PAUSED
}
