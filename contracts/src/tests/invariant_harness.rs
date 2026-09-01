// SPDX-License-Identifier: MIT
//! Differential invariant test harness using a reference model.

extern crate std;

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, RngSeed, TestRunner};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};
use std::env;
use std::format;

use super::reference_model::ReferenceModel;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::BetSide;

/// Represents an action performed in differential testing.
#[derive(Debug, Clone)]
enum Action {
    CreateRound,
    BetUp { user_idx: usize, amount: i128 },
    BetDown { user_idx: usize, amount: i128 },
    SetFeeBps { bps: Option<u32> },
    ResolveRound { price_up: bool },
    CancelRound,
    Claim { user_idx: usize },
    WithdrawFee { amount: i128 },
    TogglePause,
}

/// Strategy for generating randomized action sequences.
fn action_strategy() -> impl Strategy<Value = Action> {
    let user_idx = 0..5usize;
    let amount = 1_0000000i128..=100_0000000i128;
    let fee_bps = prop_oneof![
        Just(None),
        Just(Some(100)),  // 1%
        Just(Some(500)),  // 5%
        Just(Some(1000)), // 10%
    ];

    prop_oneof![
        Just(Action::CreateRound),
        (user_idx.clone(), amount.clone()).prop_map(|(u, a)| Action::BetUp {
            user_idx: u,
            amount: a
        }),
        (user_idx.clone(), amount.clone()).prop_map(|(u, a)| Action::BetDown {
            user_idx: u,
            amount: a
        }),
        fee_bps.prop_map(|bps| Action::SetFeeBps { bps }),
        any::<bool>().prop_map(|up| Action::ResolveRound { price_up: up }),
        Just(Action::CancelRound),
        user_idx.clone().prop_map(|u| Action::Claim { user_idx: u }),
        amount
            .clone()
            .prop_map(|a| Action::WithdrawFee { amount: a }),
        Just(Action::TogglePause),
    ]
}

/// Helper to format and emit failure diagnostic details when parity breaks.
fn pretty_print_failure(
    seed: Option<u64>,
    step_idx: usize,
    actions: &[Action],
    failed_action: &Action,
    diff: &str,
) -> ! {
    panic!(
        "\n================ DIFFERENTIAL PARITY MISMATCH ================\n\
         Seed: {:?}\n\
         Step Index: {}\n\
         Failed Action: {:?}\n\
         State Divergence:\n{}\n\
         Action Trace History:\n{:#?}\n\
         ==============================================================",
        seed, step_idx, failed_action, diff, actions
    );
}

#[test]
fn differential_invariant_harness() {
    // Environment configuration
    let seq_len: u32 = env::var("SEQUENCE_LENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let seed_opt: Option<u64> = env::var("SEED").ok().and_then(|v| v.parse().ok());

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
    client.update_oracle_heartbeat(&0u32);

    let users: std::vec::Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    let mut model = ReferenceModel::new();
    for u in &users {
        client.mint_initial(u);
        model.deposit(u, 1000_0000000);
    }

    let mut current_round_id = 0u64;

    for (step_idx, act) in actions.iter().enumerate() {
        match act {
            Action::CreateRound => {
                if client.get_active_round().is_none() {
                    current_round_id += 1;
                    let _ = client.try_create_round(&1_0000000u128, &None);
                    model.create_round(current_round_id);
                }
            }
            Action::BetUp { user_idx, amount } => {
                let user = &users[*user_idx % users.len()];
                let res = client.try_place_bet(user, amount, &BetSide::Up);
                if res.is_ok() {
                    model.place_bet(user, *amount, true);
                }
            }
            Action::BetDown { user_idx, amount } => {
                let user = &users[*user_idx % users.len()];
                let res = client.try_place_bet(user, amount, &BetSide::Down);
                if res.is_ok() {
                    model.place_bet(user, *amount, false);
                }
            }
            Action::SetFeeBps { bps } => {
                let _ = client.try_set_protocol_fee_bps(bps);
                model.set_fee_bps(*bps);
            }
            Action::ResolveRound { price_up } => {
                if let Some(active) = client.get_active_round() {
                    env.ledger()
                        .with_mut(|li| li.sequence_number = active.end_ledger);
                    let price = if *price_up {
                        2_0000000u128
                    } else {
                        5000000u128
                    };
                    let res = client.try_resolve_round(&crate::types::OraclePayload {
                        round_id: active.start_ledger,
                        price,
                        timestamp: env.ledger().timestamp(),
                        nonce: 1u64,
                        network_id: env.ledger().network_id(),
                        contract_addr: contract_id.clone(),
                        confidence: None,
                        attestation: None,
                    });
                    if res.is_ok() {
                        model.resolve_round(*price_up);
                    }
                }
            }
            Action::CancelRound => {
                let res = client.try_cancel_round(&0u32);
                if res.is_ok() {
                    model.cancel_round();
                }
            }
            Action::Claim { user_idx } => {
                let user = &users[*user_idx % users.len()];
                let res = client.try_claim_winnings(user);
                if res.is_ok() {
                    model.claim(user);
                }
            }
            Action::WithdrawFee { amount } => {
                let recipient = &users[0];
                let res = client.try_withdraw_protocol_fee(recipient, amount);
                if res.is_ok() {
                    model.withdraw_protocol_fee(recipient, *amount);
                }
            }
            Action::TogglePause => {
                if client.is_paused() {
                    let _ = client.try_unpause_contract();
                    model.paused = false;
                } else {
                    let _ = client.try_pause_contract();
                    model.paused = true;
                }
            }
        }

        // Verify model internal invariants
        let violations = model.check_invariants();
        if !violations.is_empty() {
            let diff = format!("Model Invariant Violations: {:?}", violations);
            pretty_print_failure(seed_opt, step_idx, &actions, act, &diff);
        }

        // Verify contract on-chain state vs reference model parity
        for user in &users {
            let actual_bal = client.balance(user);
            let expected_bal = *model.balances.get(user).unwrap_or(&0);
            if actual_bal != expected_bal {
                let diff = format!(
                    "Balance mismatch for user {:?}: actual={}, expected={}",
                    user, actual_bal, expected_bal
                );
                pretty_print_failure(seed_opt, step_idx, &actions, act, &diff);
            }

            let actual_pending = client.get_pending_winnings(user);
            let expected_pending = *model.pending_winnings.get(user).unwrap_or(&0);
            if actual_pending != expected_pending {
                let diff = format!(
                    "Pending winnings mismatch for user {:?}: actual={}, expected={}",
                    user, actual_pending, expected_pending
                );
                pretty_print_failure(seed_opt, step_idx, &actions, act, &diff);
            }
        }

        let actual_treasury = client.get_protocol_fee_treasury();
        if actual_treasury != model.protocol_fee_treasury {
            let diff = format!(
                "Protocol fee treasury mismatch: actual={}, expected={}",
                actual_treasury, model.protocol_fee_treasury
            );
            pretty_print_failure(seed_opt, step_idx, &actions, act, &diff);
        }

        let actual_paused = client.is_paused();
        if actual_paused != model.paused {
            let diff = format!(
                "Paused state mismatch: actual={}, expected={}",
                actual_paused, model.paused
            );
            pretty_print_failure(seed_opt, step_idx, &actions, act, &diff);
        }
    }
}
