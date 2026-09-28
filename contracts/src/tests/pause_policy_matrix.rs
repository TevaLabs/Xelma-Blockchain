// SPDX-License-Identifier: MIT
//! Exhaustive action × mode matrix for the `AdminConfig` policy class (Issue #402).
//!
//! `admin::_policy_gate`'s doc comment inventories every entrypoint dispatched
//! through each `PolicyAction` class (see `admin.rs`). `policy_gate.rs` and
//! `drill.rs` already exercise the four `PolicyAction` classes end-to-end
//! through a representative sample of entrypoints. This module goes further
//! for the `AdminConfig` class specifically (blocked only by `FullyPaused`,
//! allowed in `ClaimsOnly`) by driving *every* entrypoint the docstring lists
//! under that class through both cells:
//!
//! - **`FullyPaused` → blocked**: every call must fail with exactly
//!   `ContractError::ContractPaused`.
//! - **`ClaimsOnly` → allowed**: every call must NOT fail with
//!   `ContractPaused` — it may still fail for an orthogonal reason (a missing
//!   precondition like "no pending rotation"), which is fine: the property
//!   under test is that the *policy gate* does not block it, not that the
//!   call fully succeeds.
//!
//! `create_round`/`create_next_from_template` are intentionally included:
//! per the docstring they are `AdminConfig`-gated (not `RoundMutation`)
//! specifically so they stay callable in `ClaimsOnly` — that's the entrypoint
//! that transitions the protocol back to `Active`.
//!
//! Two functions the docstring originally miscategorized are covered by
//! dedicated tests below instead of the shared matrix helper, since they
//! don't fit the "blocked only by FullyPaused" AdminConfig shape:
//! - `set_runtime_mode` (along with `pause_contract`/`unpause_contract`) is a
//!   mode-transition control that bypasses `_policy_gate` entirely — it must
//!   stay callable in every mode, including `FullyPaused`, or there would be
//!   no way to escape an incident.
//! - `apply_scheduled_changes` is actually `RoundMutation`-gated
//!   (`_ensure_normal_mode`), not `AdminConfig` — it is blocked in
//!   `ClaimsOnly` too, unlike the rest of the config surface.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::ConfigChangeKind;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    (client, admin)
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
