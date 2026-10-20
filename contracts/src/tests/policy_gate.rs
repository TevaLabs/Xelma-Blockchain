// SPDX-License-Identifier: MIT
//! Exhaustive mode x action tests for the central PolicyGate (Issue #261).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, OraclePayload, PolicyAction};
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

/// Regression guard (Issue #701): ClaimsOnly mode must still allow
/// `claim_winnings` to succeed for a resolved round with pending winnings.
/// Only `RoundMutation` is blocked in ClaimsOnly; claims remain open.
#[test]
fn test_policy_gate_claims_only_allows_claim_winnings() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
        li.timestamp = round.end_ledger as u64 * 5;
    });
    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert!(client.get_pending_winnings(&user) > 0);

    // Transition to ClaimsOnly — claims must still be allowed.
    client.set_runtime_mode(&1u32);
    assert_eq!(client.get_runtime_mode(), 1u32);

    let claimed = client.claim_winnings(&user);
    assert!(claimed > 0);
    assert_eq!(client.get_pending_winnings(&user), 0);
}
