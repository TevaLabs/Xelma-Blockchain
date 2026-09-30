// SPDX-License-Identifier: MIT
//! Exhaustive action × mode matrix for every policy-gated class
//! (Issue #402 for `AdminConfig`, Issue #551 for the rest).
//!
//! The canonical, human-readable version of this matrix — including the full
//! per-class entrypoint inventory and the deliberate exemptions — is
//! `docs/PAUSE_POLICY.md`. This module is the executable half of that
//! document: each section below drives every entrypoint of one class through
//! the modes that are supposed to allow and the mode that is supposed to
//! block it, so the doc and the contract cannot drift apart silently.
//!
//! ## Assertion shapes
//!
//! - **blocked**: the call must fail with exactly
//!   `ContractError::ContractPaused`. Asserting the *specific* error (rather
//!   than merely "is an error") is what proves the policy gate is the cause
//!   and not some unrelated precondition.
//! - **allowed**: the call must NOT fail with `ContractPaused`. It may still
//!   fail for an orthogonal reason (a missing precondition like "no pending
//!   rotation"), which is fine: the property under test is that the *policy
//!   gate* does not block it, not that the call fully succeeds. The one
//!   exception is the claims test, which asserts real funds move.
//!
//! ## Classes
//!
//! - `AdminConfig` — blocked only by `FullyPaused`, allowed in `ClaimsOnly`.
//!   `create_round`/`create_next_from_template` are intentionally included:
//!   per the matrix they are `AdminConfig`-gated (not `RoundMutation`)
//!   specifically so they stay callable in `ClaimsOnly` — that's the
//!   entrypoint that transitions the protocol back to `Active`.
//! - `RoundMutation` — the only class blocked by `ClaimsOnly` as well, since
//!   betting into a protocol that is winding down must stop.
//! - `Claim` / `Settlement` — blocked only by `FullyPaused`.
//!
//! Two entrypoints don't fit any class shape and get dedicated tests:
//! - `set_runtime_mode` (along with `pause_contract`/`unpause_contract`) is a
//!   mode-transition control that bypasses `_policy_gate` entirely — it must
//!   stay callable in every mode, including `FullyPaused`, or there would be
//!   no way to escape an incident.
//! - `apply_scheduled_changes` is `RoundMutation`-gated
//!   (`_ensure_normal_mode`), not `AdminConfig` — it is blocked in
//!   `ClaimsOnly` too, unlike the rest of the config surface.
//!
//! `cancel_round` is listed in the matrix under `Settlement` but has no gate
//! call at all; see the dedicated divergence test at the end of this file.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{
    BetSide, ConfigChangeKind, MultiFeedPayload, OraclePayload, CANCEL_REASON_GENERIC,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, Vec,
};

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    (client, admin)
}

/// Like [`setup`], but also returns the contract id — needed by the
/// oracle-payload helpers, which must name the contract they are resolving.
fn setup_with_id(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    // Keep the oracle heartbeat fresh so `resolve_round` is not rejected with
    // `OracleHeartbeatUnhealthy` before it ever reaches the policy gate.
    client.update_oracle_heartbeat(&0u32);
    (client, contract_id, admin)
}

/// A single-feed oracle payload bound to `round_id`.
///
/// The `timestamp` is taken from the current ledger so the payload stays
/// inside the oracle's freshness window; the tests below advance the ledger
/// in 5-second steps (matching `drill.rs`) to keep it valid.
fn oracle_payload(
    env: &Env,
    contract_id: &Address,
    price: u128,
    round_id: u32,
    nonce: u64,
) -> OraclePayload {
    OraclePayload {
        price,
        timestamp: env.ledger().timestamp(),
        round_id,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    }
}

/// A three-feed oracle payload, for the quorum/median resolution path.
fn multi_feed_payload(
    env: &Env,
    contract_id: &Address,
    price: u128,
    round_id: u32,
    nonce: u64,
) -> MultiFeedPayload {
    MultiFeedPayload {
        prices: soroban_sdk::vec![env, price, price, price],
        sources: soroban_sdk::vec![env, 0u32, 1u32, 2u32],
        round_id,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        timestamp: env.ledger().timestamp(),
    }
}

/// Runs a real round end-to-end so `winner` holds pending winnings, then
/// leaves the protocol in `Normal` mode ready to be escalated.
///
/// `winner` bets `Up` and `loser` bets `Down`; resolving above the start price
/// pays the winner. The ledger is advanced past the betting window first so
/// resolution is legal, exactly as `drill.rs` does it.
fn round_settled_in_favour_of(
    env: &Env,
    client: &VirtualTokenContractClient,
    contract_id: &Address,
    winner: &Address,
    loser: &Address,
) {
    client.mint_initial(winner);
    client.mint_initial(loser);
    client.create_round(&1_0000000, &None);
    client.place_bet(winner, &100_0000000, &BetSide::Up);
    client.place_bet(loser, &100_0000000, &BetSide::Down);

    env.ledger().with_mut(|li| {
        li.sequence_number = 65;
        li.timestamp = 65 * 5; // 5 s per ledger; stays inside the oracle window
    });

    client.resolve_round(&oracle_payload(
        env,
        contract_id,
        1_5000000,
        0, // round_id binds to start_ledger of the first round
        100,
    ));
}

/// Calls every `AdminConfig`-gated entrypoint (excluding the mode-transition
/// controls and `apply_scheduled_changes` — see module docs) with
/// valid-shaped arguments, returning `(name, is_contract_paused)` pairs so a
/// single test can assert the whole matrix row at once with a readable
/// failure message.
fn call_every_admin_config_entrypoint(
    client: &VirtualTokenContractClient,
    admin: &Address,
) -> alloc::vec::Vec<(&'static str, bool)> {
    use alloc::vec;

    let is_paused = |name: &'static str, res: bool| (name, res);

    vec![
        is_paused(
            "migrate_schema_v1_to_v2",
            client.try_migrate_schema_v1_to_v2(&true) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "migrate_schema_v2_to_v3",
            client.try_migrate_schema_v2_to_v3(&true) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_oracle_max_deviation_bps",
            client.try_set_oracle_max_deviation_bps(&Some(500u32))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "arm_oracle_deviation_override",
            client.try_arm_oracle_deviation_override() == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_oracle_min_confidence_bps",
            client.try_set_oracle_min_confidence_bps(&Some(500u32))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_oracle_strict_mode",
            client.try_set_oracle_strict_mode(&true) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_hb_strict_mode",
            client.try_set_hb_strict_mode(&true) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "arm_hb_override",
            client.try_arm_hb_override() == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_hb_grace_seconds",
            client.try_set_hb_grace_seconds(&600u64) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "propose_oracle_rotation",
            client.try_propose_oracle_rotation(&Address::generate(&client.env), &100_000u64)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "accept_oracle_rotation",
            client.try_accept_oracle_rotation() == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "cancel_oracle_rotation",
            client.try_cancel_oracle_rotation() == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_windows",
            client.try_set_windows(&10u32, &20u32) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_max_stake",
            client.try_set_max_stake(&Some(1_000_0000000i128))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_max_user_exposure",
            client.try_set_max_user_exposure(&Some(1_000_0000000i128))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_max_pending_winnings",
            client.try_set_max_pending_winnings(&Some(1_000_0000000i128))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_min_bet",
            client.try_set_min_bet(&Some(1_0000000i128)) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "schedule_min_bet",
            client.try_schedule_min_bet(&Some(1_0000000i128))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "schedule_windows",
            client.try_schedule_windows(&10u32, &20u32) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "schedule_max_stake",
            client.try_schedule_max_stake(&Some(1_000_0000000i128))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "schedule_max_user_exposure",
            client.try_schedule_max_user_exposure(&Some(1_000_0000000i128))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "schedule_max_pending_winnings",
            client.try_schedule_max_pending_winnings(&Some(1_000_0000000i128))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "schedule_oracle_stale_threshold",
            client.try_schedule_oracle_stale_threshold(&300u64)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "schedule_oracle_deviation_bps",
            client.try_schedule_oracle_deviation_bps(&Some(500u32))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "schedule_oracle_timestamp_skew",
            client.try_schedule_oracle_timestamp_skew(&300u64)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "schedule_protocol_fee_bps",
            client.try_schedule_protocol_fee_bps(&Some(100u32))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_protocol_fee_bps",
            client.try_set_protocol_fee_bps(&Some(100u32))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "cancel_config_change",
            client.try_cancel_config_change(&ConfigChangeKind::MinBet)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_min_participants",
            client.try_set_min_participants(&Some(2u32)) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_max_precision_participants",
            client.try_set_max_precision_participants(&50u32)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_mint_limit",
            client.try_set_mint_limit(&10u32) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_epoch_mint_budget",
            client.try_set_epoch_mint_budget(&1_000_0000000i128)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_archive_retention",
            client.try_set_archive_retention(&50u32) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "schedule_pending_winnings_expiry",
            client.try_schedule_pending_winnings_expiry(&86_400u32)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_close_buffer_ledgers",
            client.try_set_close_buffer_ledgers(&2u32) == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "set_round_template",
            client.try_set_round_template(&1_0000000u128, &None)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "clear_round_template",
            client.try_clear_round_template() == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "reset_leaderboard_season",
            client.try_reset_leaderboard_season() == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "create_round",
            client.try_create_round(&1_0000000u128, &None)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "create_next_from_template",
            client.try_create_next_from_template() == Err(Ok(ContractError::ContractPaused)),
        ),
        is_paused(
            "withdraw_protocol_fee",
            client.try_withdraw_protocol_fee(admin, &1i128)
                == Err(Ok(ContractError::ContractPaused)),
        ),
    ]
}

#[test]
fn test_fully_paused_blocks_every_admin_config_entrypoint() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.pause_contract();
    assert!(client.is_paused());

    let results = call_every_admin_config_entrypoint(&client, &admin);
    let failed: alloc::vec::Vec<&str> = results
        .iter()
        .filter(|(_, blocked)| !*blocked)
        .map(|(name, _)| *name)
        .collect();
    assert!(
        failed.is_empty(),
        "FullyPaused must block every AdminConfig entrypoint with ContractPaused; \
         these did not: {:?}",
        failed
    );
}

#[test]
fn test_claims_only_does_not_block_any_admin_config_entrypoint() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.set_runtime_mode(&1u32); // ClaimsOnly
    assert_eq!(client.get_runtime_mode(), 1u32);

    let results = call_every_admin_config_entrypoint(&client, &admin);
    let wrongly_blocked: alloc::vec::Vec<&str> = results
        .iter()
        .filter(|(_, is_contract_paused)| *is_contract_paused)
        .map(|(name, _)| *name)
        .collect();
    assert!(
        wrongly_blocked.is_empty(),
        "ClaimsOnly must not block any AdminConfig entrypoint via the policy \
         gate (a call may still fail for an unrelated precondition, but never \
         with ContractPaused); these were wrongly gated: {:?}",
        wrongly_blocked
    );
}

/// `set_runtime_mode` bypasses `_policy_gate` entirely (see `_policy_gate`'s
/// doc comment) — it must remain callable in every mode, `FullyPaused`
/// included, since it's part of the only path out of an incident.
#[test]
fn test_set_runtime_mode_is_never_blocked_by_its_own_gate() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    client.pause_contract();
    assert_eq!(client.get_runtime_mode(), 2u32); // FullyPaused

    // Moving straight from FullyPaused to ClaimsOnly must not be rejected by
    // the policy gate (there is no gate on this entrypoint to reject it).
    let res = client.try_set_runtime_mode(&1u32);
    assert_ne!(res, Err(Ok(ContractError::ContractPaused)));
    assert_eq!(client.get_runtime_mode(), 1u32);

    // And back to FullyPaused, then straight to Normal — every transition is
    // unconditionally allowed regardless of the mode being left.
    client.set_runtime_mode(&2u32);
    assert_eq!(client.get_runtime_mode(), 2u32);
    client.set_runtime_mode(&0u32);
    assert_eq!(client.get_runtime_mode(), 0u32);
}

/// `apply_scheduled_changes` is `RoundMutation`-gated (`_ensure_normal_mode`),
/// not `AdminConfig` — unlike every other entry in
/// `call_every_admin_config_entrypoint`, it must be blocked in `ClaimsOnly`
/// too, not just `FullyPaused`.
#[test]
fn test_apply_scheduled_changes_is_blocked_in_claims_only_and_fully_paused() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    client.set_runtime_mode(&1u32); // ClaimsOnly
    assert_eq!(
        client.try_apply_scheduled_changes(&ConfigChangeKind::MinBet),
        Err(Ok(ContractError::ContractPaused))
    );

    client.set_runtime_mode(&2u32); // FullyPaused
    assert_eq!(
        client.try_apply_scheduled_changes(&ConfigChangeKind::MinBet),
        Err(Ok(ContractError::ContractPaused))
    );
}

// ─── `RoundMutation` (Issue #551) ───────────────────────────────────────────

/// Drives every `RoundMutation`-gated entrypoint, returning
/// `(name, is_contract_paused)` pairs.
///
/// `_ensure_normal_mode` is the first substantive check in each of these, so a
/// correct implementation rejects with `ContractPaused` before the
/// round/mint preconditions would otherwise matter. The caller still sets up a
/// live round and a funded user, so a regression that *reorders* the gate
/// behind a precondition shows up as a failure rather than a false pass.
fn call_every_round_mutation_entrypoint(
    client: &VirtualTokenContractClient,
    env: &Env,
    contract_id: &Address,
    user: &Address,
) -> alloc::vec::Vec<(&'static str, bool)> {
    use alloc::vec;

    let gated = |name: &'static str, res: bool| (name, res);
    let fresh_user = Address::generate(env);
    let start_ledger = client
        .get_active_round()
        .map(|r| r.start_ledger)
        .unwrap_or(0);

    vec![
        gated(
            "place_bet",
            client.try_place_bet(user, &10_0000000, &BetSide::Up)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        gated(
            "place_precision_prediction",
            client.try_place_precision_prediction(user, &10_0000000, &1550)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        // `predict_price` is a thin alias that delegates to
        // `place_precision_prediction`, so it is gated transitively.
        gated(
            "predict_price",
            client.try_predict_price(user, &1550, &10_0000000)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        gated(
            "commit_prediction",
            client.try_commit_prediction(user, &BytesN::from_array(env, &[0u8; 32]), &10_0000000)
                == Err(Ok(ContractError::ContractPaused)),
        ),
        gated(
            "reveal_prediction",
            client.try_reveal_prediction(user, &1550, &BytesN::from_array(env, &[1u8; 32]))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        gated(
            "cash_out_early",
            client.try_cash_out_early(user) == Err(Ok(ContractError::ContractPaused)),
        ),
        gated(
            "mint_initial",
            // `mint_initial` returns `i128` and reports failures with
            // `panic_with_error!`, so the contract error reaches the client as a
            // host `Error::Contract(22)` rather than a typed `ContractError`.
            client.try_mint_initial(&fresh_user)
                == Err(Ok(soroban_sdk::Error::from_contract_error(PAUSED_CODE))),
        ),
        gated(
            "apply_scheduled_changes",
            client.try_apply_scheduled_changes(&ConfigChangeKind::MinBet)
                == Err(Ok(ContractError::ContractPaused)),
        ),
    ]
}

/// `ContractError::ContractPaused` as the numeric code that reaches the client
/// through the `panic_with_error!` path used by `mint_initial`.
const PAUSED_CODE: u32 = 22;

#[test]
fn test_claims_only_blocks_every_round_mutation_entrypoint() {
    let env = Env::default();
    let (client, contract_id, _admin) = setup_with_id(&env);
    let user = Address::generate(&env);

    client.create_round(&1_0000000, &None);
    client.mint_initial(&user);

    client.set_runtime_mode(&1u32); // ClaimsOnly
    assert_eq!(client.get_runtime_mode(), 1u32);

    let results = call_every_round_mutation_entrypoint(&client, &env, &contract_id, &user);
    let not_blocked: alloc::vec::Vec<&str> = results
        .iter()
        .filter(|(_, blocked)| !*blocked)
        .map(|(name, _)| *name)
        .collect();
    assert!(
        not_blocked.is_empty(),
        "ClaimsOnly must block every RoundMutation entrypoint with ContractPaused; \
         these were not blocked: {:?}",
        not_blocked
    );
}

#[test]
fn test_fully_paused_blocks_every_round_mutation_entrypoint() {
    let env = Env::default();
    let (client, contract_id, _admin) = setup_with_id(&env);
    let user = Address::generate(&env);

    client.create_round(&1_0000000, &None);
    client.mint_initial(&user);

    client.pause_contract();
    assert!(client.is_paused());

    let results = call_every_round_mutation_entrypoint(&client, &env, &contract_id, &user);
    let not_blocked: alloc::vec::Vec<&str> = results
        .iter()
        .filter(|(_, blocked)| !*blocked)
        .map(|(name, _)| *name)
        .collect();
    assert!(
        not_blocked.is_empty(),
        "FullyPaused must block every RoundMutation entrypoint with ContractPaused; \
         these were not blocked: {:?}",
        not_blocked
    );
}

/// The headline row of the matrix: `ClaimsOnly` permits exactly three of the
/// four classes and denies the fourth.
#[test]
fn test_claims_only_does_not_block_the_rest_of_the_matrix() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    client.set_runtime_mode(&1u32); // ClaimsOnly

    assert!(client.is_action_allowed(&crate::types::PolicyAction::Claim));
    assert!(client.is_action_allowed(&crate::types::PolicyAction::Settlement));
    assert!(client.is_action_allowed(&crate::types::PolicyAction::AdminConfig));
    assert!(!client.is_action_allowed(&crate::types::PolicyAction::RoundMutation));
}

// ─── `Claim` (Issue #551) ───────────────────────────────────────────────────

/// Drives every `Claim`-gated entrypoint, returning
/// `(name, is_contract_paused)` pairs.
fn call_every_claim_entrypoint(
    client: &VirtualTokenContractClient,
    users: &Vec<Address>,
) -> alloc::vec::Vec<(&'static str, bool)> {
    use alloc::vec;

    vec![
        (
            "claim_winnings",
            client.try_claim_winnings(&users.get(0).unwrap())
                == Err(Ok(ContractError::ContractPaused)),
        ),
        (
            "claim_many",
            client.try_claim_many(users) == Err(Ok(ContractError::ContractPaused)),
        ),
    ]
}

/// Acceptance criterion for #551: **ClaimsOnly allows claims.**
///
/// This is the one cell that is not a "does the gate fire?" check — it proves
/// the money actually moves. A real round is settled, so the winner holds
/// pending winnings, and the protocol is escalated to `ClaimsOnly` *before* the
/// claim. The claim must succeed and the balance must grow by exactly the
/// pending amount; the same claims must then be rejected under `FullyPaused`.
#[test]
fn test_claims_only_allows_claims_and_fully_paused_denies_them() {
    let env = Env::default();
    let (client, contract_id, _admin) = setup_with_id(&env);

    let winner = Address::generate(&env);
    let loser = Address::generate(&env);
    round_settled_in_favour_of(&env, &client, &contract_id, &winner, &loser);

    let owed = client.get_pending_winnings(&winner);
    assert!(owed > 0, "winner must be owed winnings before the incident");
    assert_eq!(client.get_pending_winnings(&loser), 0);

    // ── ClaimsOnly: the claim goes through and the funds land ───────────────
    client.set_runtime_mode(&1u32);
    assert_eq!(client.get_runtime_mode(), 1u32);
    assert!(!client.is_paused(), "ClaimsOnly is not FullyPaused");

    let balance_before = client.balance(&winner);
    let claimed = client.claim_winnings(&winner);
    assert_eq!(
        claimed, owed,
        "claims-only must pay out the full pending amount"
    );
    assert_eq!(client.balance(&winner), balance_before + owed);
    assert_eq!(client.get_pending_winnings(&winner), 0);

    // A zero-pending claim stays a no-op success, not an error.
    assert_eq!(client.claim_winnings(&loser), 0);

    // The batch sibling behaves identically.
    let mut batch = Vec::new(&env);
    batch.push_back(winner.clone());
    batch.push_back(loser.clone());
    let amounts = client.claim_many(&batch);
    let mut expected_zeroes = Vec::new(&env);
    expected_zeroes.push_back(0i128);
    expected_zeroes.push_back(0i128);
    assert_eq!(
        amounts, expected_zeroes,
        "re-claiming an already-settled winner yields 0, not an error"
    );

    let gated = call_every_claim_entrypoint(&client, &batch);
    let wrongly_blocked: alloc::vec::Vec<&str> = gated
        .iter()
        .filter(|(_, blocked)| *blocked)
        .map(|(name, _)| *name)
        .collect();
    assert!(
        wrongly_blocked.is_empty(),
        "ClaimsOnly must not block any Claim entrypoint; these were wrongly gated: {:?}",
        wrongly_blocked
    );

    // ── FullyPaused: the same claims are refused ───────────────────────────
    client.set_runtime_mode(&2u32);
    assert!(client.is_paused());

    let gated = call_every_claim_entrypoint(&client, &batch);
    let not_blocked: alloc::vec::Vec<&str> = gated
        .iter()
        .filter(|(_, blocked)| !*blocked)
        .map(|(name, _)| *name)
        .collect();
    assert!(
        not_blocked.is_empty(),
        "FullyPaused must block every Claim entrypoint with ContractPaused; \
         these were not blocked: {:?}",
        not_blocked
    );
}

// ─── `Settlement` (Issue #551) ──────────────────────────────────────────────

/// Drives every `Settlement`-gated entrypoint listed in
/// `docs/PAUSE_POLICY.md` §3.3.
///
/// `cancel_round` is included but is *expected* to come back un-gated — it has
/// no gate call at all, which is a live divergence from the documented matrix.
/// `test_cancel_round_is_ungated_and_diverges_from_the_matrix` pins that
/// divergence explicitly; here it is filtered out of the FullyPaused assertion
/// so the rest of the class is still enforced.
fn call_every_settlement_entrypoint(
    client: &VirtualTokenContractClient,
    env: &Env,
    contract_id: &Address,
) -> alloc::vec::Vec<(&'static str, bool)> {
    use alloc::vec;

    let gated = |name: &'static str, res: bool| (name, res);
    let start_ledger = client
        .get_active_round()
        .map(|r| r.start_ledger)
        .unwrap_or(0);

    vec![
        gated(
            "resolve_round",
            client.try_resolve_round(&oracle_payload(env, contract_id, 1_2000000, 0, 1))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        gated(
            "resolve_round_multi",
            client.try_resolve_round_multi(&multi_feed_payload(env, contract_id, 1_2000000, 0, 1))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        gated(
            "void_round",
            client.try_void_round(&u64::from(start_ledger))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        gated(
            "finalize_round",
            client.try_finalize_round(&u64::from(start_ledger))
                == Err(Ok(ContractError::ContractPaused)),
        ),
        gated(
            "cancel_round",
            client.try_cancel_round(&CANCEL_REASON_GENERIC)
                == Err(Ok(ContractError::ContractPaused)),
        ),
    ]
}

#[test]
fn test_claims_only_does_not_block_settlement() {
    let env = Env::default();
    let (client, contract_id, _admin) = setup_with_id(&env);

    client.create_round(&1_0000000, &None);
    client.set_runtime_mode(&1u32); // ClaimsOnly

    let results = call_every_settlement_entrypoint(&client, &env, &contract_id);
    let wrongly_blocked: alloc::vec::Vec<&str> = results
        .iter()
        .filter(|(_, blocked)| *blocked)
        .map(|(name, _)| *name)
        .collect();
    assert!(
        wrongly_blocked.is_empty(),
        "ClaimsOnly must not block any Settlement entrypoint via the policy gate; \
         these were wrongly gated: {:?}",
        wrongly_blocked
    );
}

#[test]
fn test_fully_paused_blocks_settlement() {
    let env = Env::default();
    let (client, contract_id, _admin) = setup_with_id(&env);

    client.create_round(&1_0000000, &None);
    client.pause_contract();
    assert!(client.is_paused());

    let results = call_every_settlement_entrypoint(&client, &env, &contract_id);
    let not_blocked: alloc::vec::Vec<&str> = results
        .iter()
        // `cancel_round` has no policy gate at all — see the dedicated test.
        .filter(|(name, blocked)| !*blocked && *name != "cancel_round")
        .map(|(name, _)| *name)
        .collect();
    assert!(
        not_blocked.is_empty(),
        "FullyPaused must block every gated Settlement entrypoint with ContractPaused; \
         these were not blocked: {:?}",
        not_blocked
    );
}

/// Pins a real divergence between the documented matrix and the implementation.
///
/// `docs/PAUSE_POLICY.md` and `_policy_gate`'s doc comment both list
/// `cancel_round` under the `Settlement` class, which is blocked in
/// `FullyPaused`. The implementation has no gate call in
/// `settlement::cancel_round`, so an emergency stop does **not** stop a
/// cancellation.
///
/// Cancelling only returns stakes, so the current behaviour is not obviously
/// harmful — but "the emergency stop does not stop this" is a decision that
/// belongs to maintainers, not to whatever the code happens to do today. This
/// test records the current behaviour so that closing the gap is a deliberate,
/// reviewable change to an assertion rather than a silent drift.
#[test]
fn test_cancel_round_is_ungated_and_diverges_from_the_matrix() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    client.create_round(&1_0000000, &None);
    client.pause_contract();
    assert!(client.is_paused());

    assert_ne!(
        client.try_cancel_round(&CANCEL_REASON_GENERIC),
        Err(Ok(ContractError::ContractPaused)),
        "cancel_round currently has no policy gate: the documented Settlement \
         class says FullyPaused must block it. If this now fails, the gate was \
         added — update docs/PAUSE_POLICY.md and the doc comment on \
         admin::_policy_gate to drop the divergence note."
    );
    // The round is gone, so the call really did execute rather than fail on
    // some unrelated precondition.
    assert!(client.get_active_round().is_none());
}
