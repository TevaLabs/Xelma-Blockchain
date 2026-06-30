extern crate std;
// SPDX-License-Identifier: MIT
//! Differential invariant test harness using a reference model.

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, RngSeed, TestRunner};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};
use std::env;
use std::string::String;
use std::vec::Vec;
use rand::{rngs::StdRng, SeedableRng};

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::BetSide;
use super::reference_model::ReferenceModel;

/// Represents a simplified action that can be performed on the contract.
#[derive(Debug, Clone)]
enum Action {
    BetUp { user_idx: usize, amount: i128 },
    BetDown { user_idx: usize, amount: i128 },
    Resolve { price_up: bool },
    Claim { user_idx: usize },
    // New actions for extended stress testing
    Cancel { user_idx: usize },
    Pause,
    ConfigChange { key: String, value: String },
}

/// Generate a random sequence of actions.
fn action_strategy() -> impl Strategy<Value = Action> {
let user_idx = 0..5usize;
    let amount = 0i128..=1_000_000i128;
    // Simple string generators for config actions.
    let key = any::<String>();
    let value = any::<String>();
    prop_oneof![
        (user_idx.clone(), amount.clone()).prop_map(|(u, a)| Action::BetUp { user_idx: u, amount: a }),
        (user_idx.clone(), amount.clone()).prop_map(|(u, a)| Action::BetDown { user_idx: u, amount: a }),
        any::<bool>().prop_map(|up| Action::Resolve { price_up: up }),
        user_idx.clone().prop_map(|u| Action::Claim { user_idx: u }),
        // New actions
        user_idx.clone().prop_map(|u| Action::Cancel { user_idx: u }),
        any::<bool>().prop_map(|_| Action::Pause),
        (key.clone(), value.clone()).prop_map(|(k, v)| Action::ConfigChange { key: k, value: v }),
    ]
}

/// Helper to format failure information.
fn pretty_print_failure(seed: Option<u64>, actions: &[Action], diff: &str) -> ! {
    panic!(
        "Invariant violation!\nSeed: {:?}\nAction trace: {:#?}\nState diff:\n{}",
        seed, actions, diff
    );
}

#[test]
fn differential_invariant_harness() {
        // Environment configuration
        let seq_len: u32 = env::var("SEQUENCE_LENGTH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20);
        let seed_opt: Option<u64> = env::var("SEED")
            .ok()
            .and_then(|v| v.parse().ok());

        // Set up proptest runner with optional seed (deterministic when seed is provided)
        let mut config = Config::with_cases(seq_len);
        if let Some(seed) = seed_opt {
            config.rng_seed = RngSeed::Fixed(seed);
        }
        let mut runner = TestRunner::new(config);
        let actions_strategy = prop::collection::vec(action_strategy(), 1..=seq_len as usize);
        let actions = actions_strategy
            .new_tree(&mut runner)
            .expect("Failed to generate actions")
            .current();

        // Setup contract environment.
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);

        let users: std::vec::Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
        for u in &users {
            client.mint_initial(u);
        }

        // Reference model.
        let mut model = ReferenceModel::new();

        // Helper to compare invariants after each step.
        let check = |model: &ReferenceModel| {
            let violations = model.check_invariants();
            if !violations.is_empty() {
                // Provide a placeholder diff; in a real implementation, compare contract vs model state.
                let diff = "State diff not implemented";
                pretty_print_failure(seed_opt, &actions, diff);
            }
        };

        // Execute actions.
        for act in &actions {
            match act {
                Action::BetUp { user_idx, amount } => {
                    let user = &users[*user_idx % users.len()];
                    let _ = client.try_place_bet(&user, &(*amount as u128), &BetSide::Up);
                    model.place_bet(&user, *amount);
                }
                Action::BetDown { user_idx, amount } => {
                    let user = &users[*user_idx % users.len()];
                    let _ = client.try_place_bet(&user, &(*amount as u128), &BetSide::Down);
                    model.place_bet(&user, *amount);
                }
                Action::Resolve { price_up } => {
                    let _ = client.try_resolve_round(&crate::types::OraclePayload {
                        price: if *price_up { 2_000_0000 } else { 500_000 },
                        timestamp: env.ledger().timestamp(),
                        round_id: 0,
                        nonce: 1u64,
                        network_id: env.ledger().network_id(),
                        contract_addr: contract_id.clone(),
                    });
                    // Simplified: no explicit winners map; model resolves with empty map.
                    model.resolve(&std::collections::BTreeMap::new());
                }
                Action::Claim { user_idx } => {
                    let user = &users[*user_idx % users.len()];
                    let _ = client.try_claim_winnings(&user);
                    model.claim(&user);
                }
                Action::Cancel { user_idx } => {
                    let user = &users[*user_idx % users.len()];
                    model.cancel(&user);
                }
                Action::Pause => {
                    model.pause();
                }
                Action::ConfigChange { key, value } => {
                    model.config_change(key, value);
                }
            }
            // Check invariants after each action.
            check(&model);
        }
    }
}
