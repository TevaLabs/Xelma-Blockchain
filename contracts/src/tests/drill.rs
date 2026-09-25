// SPDX-License-Identifier: MIT
//! Automated emergency "claims-only" drill tests for protocol pause/incident behavior.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, DataKey, OraclePayload, ProtocolStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, BytesN,
};

fn setup_contract(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    (client, contract_id, admin, oracle)
}

/// Verifies the complete operational matrix when the protocol is in ClaimsOnly mode (mode 1).
#[test]
fn test_claims_only_matrix_verification() {
    let env = Env::default();
    let (client, contract_id, admin, oracle) = setup_contract(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Setup initial user balance and pending winnings before emergency mode
    client.mint_initial(&user1);
    client.mint_initial(&user2);

    // Round 0
    client.create_round(&1_0000000, &None);
    client.place_bet(&user1, &100_0000000, &BetSide::Up);
    client.place_bet(&user2, &100_0000000, &BetSide::Down);

    env.ledger().with_mut(|li| {
        li.sequence_number = 65;
        li.timestamp = 1000;
    });

    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 100,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    attestation: None,
    });

    assert!(client.get_pending_winnings(&user1) > 0);

    // Seed protocol fee treasury for test fee withdrawal validation
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::ProtocolFeeTreasury, &5000_0000000i128);
    });

    // ─── ENTER CLAIMS-ONLY MODE ────────────────────────────────────────────────
    client.set_runtime_mode(&1u32);
    assert_eq!(client.get_runtime_mode(), 1u32);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);

    let health = client.get_protocol_health();
    assert_eq!(health.status_code, 6u32); // CLAIMS_ONLY
    assert!(!health.paused);

    // 1. Deposits / Initial Mint -> BLOCKED
    let user3 = Address::generate(&env);
    let mint_res = client.try_mint_initial(&user3);
    assert!(mint_res.is_err());

    // 2. Trades / Bets / Predictions -> BLOCKED
    // Create Round 1
    client.create_round(&1_5000000, &None);

    let bet_res = client.try_place_bet(&user1, &10_0000000, &BetSide::Up);
    assert_eq!(bet_res, Err(Ok(ContractError::ContractPaused)));

    let predict_res = client.try_place_precision_prediction(&user1, &10_0000000, &1550);
    assert_eq!(predict_res, Err(Ok(ContractError::ContractPaused)));

    let dummy_hash = BytesN::from_array(&env, &[0u8; 32]);
    let commit_res = client.try_commit_prediction(&user1, &dummy_hash, &10_0000000);
    assert_eq!(commit_res, Err(Ok(ContractError::ContractPaused)));

    let dummy_salt = BytesN::from_array(&env, &[1u8; 32]);
    let reveal_res = client.try_reveal_prediction(&user1, &1550, &dummy_salt);
    assert_eq!(reveal_res, Err(Ok(ContractError::ContractPaused)));

    // 3. Withdrawals / Claims -> ALLOWED
    let pending_before = client.get_pending_winnings(&user1);
    assert!(pending_before > 0);
    let bal_before = client.balance(&user1);

    let claimed = client.claim_winnings(&user1);
    assert_eq!(claimed, pending_before);
    assert_eq!(client.balance(&user1), bal_before + claimed);
    assert_eq!(client.get_pending_winnings(&user1), 0);

    // Zero winnings claim is idempotent and succeeds
    let zero_claim = client.claim_winnings(&user3);
    assert_eq!(zero_claim, 0);

    // 4. Market Creation & Cancellation -> ALLOWED
    // Active Round 1 exists, admin cancels it
    client.cancel_round(&1);

    // Create Round 2 in ClaimsOnly mode
    client.create_round(&2_0000000, &None);

    // 5. Resolution -> ALLOWED
    env.ledger().with_mut(|li| {
        li.sequence_number = 130;
        li.timestamp = 2000;
    });

    let resolve_res = client.try_resolve_round(&OraclePayload {
        price: 2_1000000,
        timestamp: env.ledger().timestamp(),
        round_id: 65,
        nonce: 101,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    attestation: None,
    });
    assert_eq!(resolve_res, Ok(Ok(())));

    // 6. Withdraw Protocol Fee -> ALLOWED
    let withdraw_res = client.try_withdraw_protocol_fee(&admin, &1000_0000000i128);
    assert!(withdraw_res.is_ok());

    // 7. Administrative Config -> ALLOWED
    let set_win_res = client.try_set_windows(&10, &20);
    assert!(set_win_res.is_ok());

    // 8. Read-only Queries -> ALLOWED
    assert_eq!(client.get_admin(), Some(admin));
    assert_eq!(client.get_oracle(), Some(oracle));
    assert!(!client.is_paused()); // is_paused checks FullyPaused specifically
}

/// Verifies operational matrix when in FullyPaused mode (mode 2).
#[test]
fn test_fully_paused_matrix_verification() {
    let env = Env::default();
    let (client, contract_id, admin, oracle) = setup_contract(&env);
    let user1 = Address::generate(&env);

    client.mint_initial(&user1);
    client.pause_contract();

    assert!(client.is_paused());
    assert_eq!(client.get_runtime_mode(), 2u32);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Paused);

    // 1. Claims -> BLOCKED
    let claim_res = client.try_claim_winnings(&user1);
    assert_eq!(claim_res, Err(Ok(ContractError::ContractPaused)));

    // 2. Deposits / Mint -> BLOCKED
    let mint_res = client.try_mint_initial(&user1);
    assert!(mint_res.is_err());

    // 3. Bets / Trades -> BLOCKED
    let bet_res = client.try_place_bet(&user1, &10_0000000, &BetSide::Up);
    assert_eq!(bet_res, Err(Ok(ContractError::ContractPaused)));

    // 4. Market Creation -> BLOCKED
    let create_res = client.try_create_round(&1_0000000, &None);
    assert_eq!(create_res, Err(Ok(ContractError::ContractPaused)));

    // 5. Settlement -> BLOCKED
    let resolve_res = client.try_resolve_round(&OraclePayload {
        price: 1_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 102,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    attestation: None,
    });
    assert_eq!(resolve_res, Err(Ok(ContractError::ContractPaused)));

    // 6. Admin Config -> BLOCKED
    let win_res = client.try_set_windows(&5, &10);
    assert_eq!(win_res, Err(Ok(ContractError::ContractPaused)));

    // 7. Queries -> ALLOWED
    assert_eq!(client.get_admin(), Some(admin));
    assert_eq!(client.get_oracle(), Some(oracle));
    assert_eq!(client.balance(&user1), 1000_0000000);

    // 8. Unpause / Mode reset -> ALLOWED
    client.unpause_contract();
    assert!(!client.is_paused());
    assert_eq!(client.get_runtime_mode(), 0u32);
}

/// End-to-end simulation of an emergency incident lifecycle:
/// Normal -> Incident (ClaimsOnly) -> Resolution & Claiming -> Full Pause Escalation -> Recovery to Normal.
#[test]
fn test_emergency_incident_simulation_lifecycle() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    // Step A: Normal Operation
    client.mint_initial(&user1);
    client.mint_initial(&user2);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user1, &100_0000000, &BetSide::Up);
    client.place_bet(&user2, &100_0000000, &BetSide::Down);

    // Step B: INCIDENT DETECTED - Transition to ClaimsOnly Mode
    client.set_runtime_mode(&1u32);
    assert_eq!(client.get_runtime_mode(), 1u32);
    let health_incident = client.get_protocol_health();
    assert_eq!(health_incident.status_code, 6u32);

    // Step C: Verify Incident Protections Active
    // New users cannot join
    assert!(client.try_mint_initial(&user3).is_err());
    // Bettors cannot place new bets
    assert_eq!(
        client.try_place_bet(&user1, &50_0000000, &BetSide::Up),
        Err(Ok(ContractError::ContractPaused))
    );

    // Step D: In-Flight Round Resolution in Claims-Only Mode
    env.ledger().with_mut(|li| {
        li.sequence_number = 65;
        li.timestamp = 1000;
    });

    client.resolve_round(&OraclePayload {
        price: 1_2000000, // Up side wins
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 200,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    attestation: None,
    });

    // Step E: Claim Winnings Executed Successfully During Emergency Mode
    let user1_pending = client.get_pending_winnings(&user1);
    assert!(user1_pending > 0);

    let claimed_amount = client.claim_winnings(&user1);
    assert_eq!(claimed_amount, user1_pending);

    // Step F: Escalation to Fully Paused Mode
    client.pause_contract();
    assert!(client.is_paused());

    // In Full Pause, even claims are locked down
    assert_eq!(
        client.try_claim_winnings(&user2),
        Err(Ok(ContractError::ContractPaused))
    );

    // Step G: Recovery - Unpause Protocol
    client.unpause_contract();
    assert!(!client.is_paused());
    assert_eq!(client.get_runtime_mode(), 0u32);

    // Step H: Post-Incident Verification
    // Minting re-enabled
    client.mint_initial(&user3);
    assert_eq!(client.balance(&user3), 1000_0000000);

    // Market creation and betting re-enabled
    client.create_round(&1_2000000, &None);
    let bet_res = client.try_place_bet(&user3, &50_0000000, &BetSide::Up);
    assert!(bet_res.is_ok());

    let health = client.get_protocol_health();
    assert!(!health.paused);
    assert!(health.has_active_round);
}

// ─── Issue #417: chaos recovery across migrate + active round + pause ───────
//
// The full emergency interleaving the ops runbook cares about:
//   create round → pause → migration dry-run → claims-only → resume/cancel.
// Both drills prove the central invariant — **no funds are ever stuck** —
// regardless of how the emergency states are interleaved, and that migration
// dry-runs are atomic (they never move funds or mutate storage, even when
// refused).

/// Chaos recovery drill, resume path (Issue #417):
/// create round → pause → migration dry-run → claims-only → resolve → claim.
#[test]
fn test_chaos_recovery_migrate_active_round_pause_resume() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    // ── 1. Normal operation: create round and place bets ──────────────────
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&carol);
    let initial_total = 3 * 1000_0000000i128;

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Down);
    let total_staked = 300_0000000i128;

    // Migration dry-run with an active round is refused atomically: no
    // storage is touched and no funds move.
    assert_eq!(
        client.try_migrate_schema_v1_to_v2(&true),
        Err(Ok(ContractError::MigrationActiveRound))
    );
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);

    // ── 2. Emergency pause (FullyPaused mode 2) ───────────────────────────
    client.pause_contract();
    assert!(client.is_paused());

    // Trading and claiming are locked.
    assert_eq!(
        client.try_place_bet(&carol, &50_0000000, &BetSide::Up),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(
        client.try_claim_winnings(&alice),
        Err(Ok(ContractError::ContractPaused))
    );

    // Migration dry-run while paused is refused by the pause gate and leaves
    // balances untouched.
    assert_eq!(
        client.try_migrate_schema_v1_to_v2(&true),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);

    // ── 3. Claims-only (mode 1): claims/resolution allowed, bets blocked ──
    client.set_runtime_mode(&1u32);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
    assert!(!client.is_paused());
    assert_eq!(
        client.try_place_bet(&carol, &50_0000000, &BetSide::Up),
        Err(Ok(ContractError::ContractPaused))
    );

    // Migration dry-run is still refused (active round) during claims-only.
    assert_eq!(
        client.try_migrate_schema_v1_to_v2(&true),
        Err(Ok(ContractError::MigrationActiveRound))
    );

    // ── 4. Resume path: resolve the in-flight round during claims-only ────
    env.ledger().with_mut(|li| {
        li.sequence_number = 20;
    });
    let round = client.get_active_round().unwrap();
    client.resolve_round(&OraclePayload {
        price: 2_0000000, // price went UP → alice wins
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 900,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });
    assert_eq!(client.get_active_round(), None);

    // No funds stuck: every staked stroop is pending (or already claimed).
    let pending_sum = client.get_pending_winnings(&alice)
        + client.get_pending_winnings(&bob)
        + client.get_pending_winnings(&carol);
    assert_eq!(pending_sum, total_staked);

    // Claim everything; balances fully reconcile to the initial mints.
    client.claim_winnings(&alice);
    client.claim_winnings(&bob);
    client.claim_winnings(&carol);
    let balance_sum = client.balance(&alice) + client.balance(&bob) + client.balance(&carol);
    assert_eq!(balance_sum, initial_total);

    // ── 5. Recovery to Normal ─────────────────────────────────────────────
    client.set_runtime_mode(&0u32);
    assert_eq!(client.get_runtime_mode(), 0u32);
    let health = client.get_protocol_health();
    assert!(!health.paused);
}

/// Chaos recovery drill, cancel path (Issue #417):
/// create round → pause → migration dry-run → claims-only → cancel → claim.
#[test]
fn test_chaos_recovery_migrate_active_round_pause_cancel() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // ── 1. Normal operation: create round and place bets ──────────────────
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    let initial_total = 2 * 1000_0000000i128;

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Down);
    let total_staked = 300_0000000i128;

    // ── 2. Emergency pause ────────────────────────────────────────────────
    client.pause_contract();
    assert!(client.is_paused());

    // ── 3. Migration dry-run while paused: refused, no funds touched ──────
    assert_eq!(
        client.try_migrate_schema_v1_to_v2(&true),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);

    // ── 4. Claims-only ────────────────────────────────────────────────────
    client.set_runtime_mode(&1u32);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);

    // Migration dry-run in claims-only with an active round is refused
    // atomically by the active-round gate.
    assert_eq!(
        client.try_migrate_schema_v1_to_v2(&true),
        Err(Ok(ContractError::MigrationActiveRound))
    );
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);

    // ── 5. Cancel path: refunds every stake in full during claims-only ────
    client.cancel_round(&3u32);
    assert_eq!(client.get_active_round(), None);
    assert_eq!(client.get_pending_winnings(&alice), 100_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 200_0000000);
    assert_eq!(
        client.get_pending_winnings(&alice) + client.get_pending_winnings(&bob),
        total_staked
    );

    client.claim_winnings(&alice);
    client.claim_winnings(&bob);
    assert_eq!(client.balance(&alice) + client.balance(&bob), initial_total);

    // ── 6. Recovery to Normal: a fresh round can be created and traded ────
    client.set_runtime_mode(&0u32);
    assert_eq!(client.get_runtime_mode(), 0u32);
    env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });
    client.create_round(&1_0000000, &None);
    assert!(client.get_active_round().is_some());
}
