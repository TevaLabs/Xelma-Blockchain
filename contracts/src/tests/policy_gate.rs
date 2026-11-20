// SPDX-License-Identifier: MIT
//! Exhaustive mode x action tests for the central PolicyGate (Issues #261, #880).
//!
//! ## Runtime Pause Policy Matrix (`RuntimeMode` × `PolicyAction`)
//!
//! | RuntimeMode | `RoundMutation` (bet / predict / cash-out) | `Claim` (`claim_winnings` / `claim_many`) | `AdminConfig` (admin & parameter config) | `Settlement` (`resolve_round` / `cancel_round`) |
//! |-------------|:------------------------------------------:|:-----------------------------------------:|:----------------------------------------:|:-----------------------------------------------:|
//! | `0` (`Normal` / `Active`) | Allowed (`true`) | Allowed (`true`) | Allowed (`true`) | Allowed (`true`) |
//! | `1` (`ClaimsOnly`) | **Blocked (`false`)** | Allowed (`true`) | Allowed (`true`) | Allowed (`true`) |
//! | `2` (`FullyPaused`) | **Blocked (`false`)** | **Blocked (`false`)** | **Blocked (`false`)** | **Blocked (`false`)** |

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::PolicyAction;
use soroban_sdk::{testutils::Address as _, Address, Env};

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

#[test]
fn test_policy_gate_exhaustive_mode_x_action_matrix() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    // Table of (mode_u32, expected_round_mutation, expected_claim, expected_admin_config, expected_settlement)
    let matrix: [(u32, bool, bool, bool, bool); 3] = [
        (0u32, true, true, true, true),    // Normal / Active
        (1u32, false, true, true, true),   // ClaimsOnly
        (2u32, false, false, false, false), // FullyPaused
    ];

    for (mode, exp_round, exp_claim, exp_admin, exp_settle) in matrix {
        client.set_runtime_mode(&mode);
        assert_eq!(client.get_runtime_mode(), mode);
        assert_eq!(
            client.is_action_allowed(&PolicyAction::RoundMutation),
            exp_round,
            "mode {} x RoundMutation mismatch",
            mode
        );
        assert_eq!(
            client.is_action_allowed(&PolicyAction::Claim),
            exp_claim,
            "mode {} x Claim mismatch",
            mode
        );
        assert_eq!(
            client.is_action_allowed(&PolicyAction::AdminConfig),
            exp_admin,
            "mode {} x AdminConfig mismatch",
            mode
        );
        assert_eq!(
            client.is_action_allowed(&PolicyAction::Settlement),
            exp_settle,
            "mode {} x Settlement mismatch",
            mode
        );
    }
}
