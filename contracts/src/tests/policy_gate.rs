// SPDX-License-Identifier: MIT
//! Exhaustive mode x action tests for the central PolicyGate (Issue #261).
//!
//! `cash_out_early` is `RoundMutation`-gated (`_ensure_normal_mode`), so the
//! pause policy must block it in both `ClaimsOnly` and `FullyPaused` and let it
//! through in `Normal` (Issue #832).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, PolicyAction};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    (client, admin, oracle)
}

#[test]
fn test_policy_gate_normal_mode_allows_everything() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    assert!(client.is_action_allowed(&PolicyAction::RoundMutation));
    assert!(client.is_action_allowed(&PolicyAction::Claim));
    assert!(client.is_action_allowed(&PolicyAction::AdminConfig));
    assert!(client.is_action_allowed(&PolicyAction::Settlement));
}

#[test]
fn test_policy_gate_claims_only_blocks_only_round_mutation() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.set_runtime_mode(&1u32); // ClaimsOnly

    assert!(!client.is_action_allowed(&PolicyAction::RoundMutation));
    assert!(client.is_action_allowed(&PolicyAction::Claim));
    assert!(client.is_action_allowed(&PolicyAction::AdminConfig));
    assert!(client.is_action_allowed(&PolicyAction::Settlement));
}

#[test]
fn test_policy_gate_fully_paused_blocks_everything() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.pause_contract();

    assert!(!client.is_action_allowed(&PolicyAction::RoundMutation));
    assert!(!client.is_action_allowed(&PolicyAction::Claim));
    assert!(!client.is_action_allowed(&PolicyAction::AdminConfig));
    assert!(!client.is_action_allowed(&PolicyAction::Settlement));
}

#[test]
fn test_policy_gate_matches_actual_entrypoint_behaviour_round_mutation() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.set_runtime_mode(&1u32); // ClaimsOnly: no active round
    assert!(!client.is_action_allowed(&PolicyAction::RoundMutation));

    // place_bet is RoundMutation-gated and requires an active round anyway —
    // ClaimsOnly blocks it via the policy gate before NoActiveRound is reached.
    let result = client.try_place_bet(&user, &10_0000000, &crate::types::BetSide::Up);
    assert!(result.is_err());

    // create_round itself is admin-gated (AdminConfig class, blocked only by
    // FullyPaused) since it is the entrypoint that transitions ClaimsOnly
    // back to Active — it must remain callable while in ClaimsOnly.
    assert!(client.is_action_allowed(&PolicyAction::AdminConfig));
    client.create_round(&1_0000000, &None);
    assert!(client.get_active_round().is_some());
}

#[test]
fn test_policy_gate_admin_config_still_allowed_in_claims_only() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.set_runtime_mode(&1u32); // ClaimsOnly
                                    // Admin can still reconfigure — e.g. pause_contract itself is AdminConfig-gated.
    client.pause_contract();
    assert!(client.is_paused());
}

/// Creates a round in the `Running` phase with one open position and early
/// cash-out enabled, i.e. the policy gate is the only thing that can block the
/// call. Returns the client and the bettor.
fn setup_running_early_cashout(env: &Env) -> (VirtualTokenContractClient<'_>, Address) {
    let (client, _admin, _oracle) = setup(env);
    let user = Address::generate(env);

    client.mint_initial(&user);
    client.set_early_cashout_bps(&Some(500u32)); // 5% penalty
    client.create_round(&1_000_0000000u128, &None);
    client.place_bet(&user, &100i128, &BetSide::Up);

    // Default windows: betting closes at ledger 6, the round ends at 12, so
    // ledger 7 is the Running phase where cash-out is legal.
    env.ledger().with_mut(|li| li.sequence_number = 7);

    (client, user)
}

/// Issue #832: the pause policy must block `cash_out_early` appropriately.
///
/// `cash_out_early` is classified as `RoundMutation`, so `ClaimsOnly` and
/// `FullyPaused` must both reject it with `ContractPaused` before any state is
/// touched, and `Normal` must let the very same call succeed.
#[test]
fn test_policy_gate_blocks_cash_out_early_in_claims_only_and_fully_paused() {
    let env = Env::default();
    let (client, user) = setup_running_early_cashout(&env);

    let pending_before = client.get_pending_winnings(&user);

    // ClaimsOnly: settlement and claims stay open, round mutation does not.
    client.set_runtime_mode(&1u32);
    assert!(!client.is_action_allowed(&PolicyAction::RoundMutation));
    assert_eq!(
        client.try_cash_out_early(&user),
        Err(Ok(ContractError::ContractPaused))
    );
    assert!(
        client.get_user_position(&user).is_some(),
        "a gated call must not consume the position"
    );
    assert_eq!(client.get_pending_winnings(&user), pending_before);

    // FullyPaused: everything except the mode-transition controls is blocked.
    client.pause_contract();
    assert!(!client.is_action_allowed(&PolicyAction::RoundMutation));
    assert_eq!(
        client.try_cash_out_early(&user),
        Err(Ok(ContractError::ContractPaused))
    );
    assert!(client.get_user_position(&user).is_some());
    assert_eq!(client.get_pending_winnings(&user), pending_before);

    // Normal: the identical call now goes through, proving the gate (and not a
    // missing precondition) was what blocked it above.
    client.set_runtime_mode(&0u32);
    assert!(client.is_action_allowed(&PolicyAction::RoundMutation));
    client.cash_out_early(&user);
    assert!(client.get_user_position(&user).is_none());
    assert!(client.get_pending_winnings(&user) > pending_before);
}

/// The policy gate runs before the feature/precondition checks inside
/// `cash_out_early`, so a paused protocol reports `ContractPaused` even when
/// early cash-out was never configured — wallets must not mistake an
/// operational pause for the feature being switched off.
#[test]
fn test_policy_gate_paused_outranks_early_cashout_configuration() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);

    client.mint_initial(&user);
    client.create_round(&1_000_0000000u128, &None);
    client.place_bet(&user, &100i128, &BetSide::Up);
    env.ledger().with_mut(|li| li.sequence_number = 7);

    // Early cash-out is deliberately left unconfigured (`EarlyCashoutBps` unset).
    assert_eq!(client.get_early_cashout_bps(), None);
    client.pause_contract();

    assert_eq!(
        client.try_cash_out_early(&user),
        Err(Ok(ContractError::ContractPaused))
    );
}

#[test]
fn test_policy_gate_transitions_round_trip() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    assert_eq!(client.get_runtime_mode(), 0); // Normal
    client.set_runtime_mode(&1u32);
    assert_eq!(client.get_runtime_mode(), 1); // ClaimsOnly
    client.set_runtime_mode(&2u32);
    assert_eq!(client.get_runtime_mode(), 2); // FullyPaused
    client.set_runtime_mode(&0u32);
    assert_eq!(client.get_runtime_mode(), 0); // Normal
}
