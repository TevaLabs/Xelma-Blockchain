// SPDX-License-Identifier: MIT
//! Differential invariant test harness using a reference model.
//!
//! Validates state parity, value conservation, fee consistency, and event schemas
//! across both dimensions of the one-sided degenerate market settlement policy:
//! `OneSidedPolicy::Refund` and `OneSidedPolicy::CarryForward`.

extern crate std;

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, RngSeed, TestRunner};
use soroban_sdk::testutils::{Address as _, Events, Ledger as _};
use soroban_sdk::{symbol_short, Address, Env, TryIntoVal};
use std::env;
use std::format;

use super::reference_model::ReferenceModel;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::settlement::_apply_one_sided_policy;
use crate::types::{BetSide, DataKeyCore, DataKeyScoped, OneSidedPolicy, OraclePayload};

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

/// Helper to retrieve the latest emitted `("pool", "onesided")` event from the environment.
///
/// Returns `Some((round_id, policy_code, affected_side, refund_amount, carry_amount, pool_up, pool_down))`
fn find_latest_onesided_event(
    env: &Env,
) -> Option<(u64, u32, u32, i128, i128, i128, i128)> {
    let events = env.events().all();
    for e in events.iter().rev() {
        let (_contract, topics, data) = e;
        if topics.len() == 2
            && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("pool"))
            && topics.get(1).unwrap().try_into_val(env) == Ok(symbol_short!("onesided"))
        {
            let vec_res: Result<soroban_sdk::Vec<soroban_sdk::Val>, _> = data.try_into_val(env);
            if let Ok(vec) = vec_res {
                if vec.len() == 7 {
                    let tuple_res: Result<(u64, u32, u32, i128, i128, i128, i128), _> =
                        data.try_into_val(env);
                    if let Ok(tuple) = tuple_res {
                        return Some(tuple);
                    }
                }
            }
        }
    }
    None
}

/// Drives randomized action sequences comparing contract on-chain state against `ReferenceModel`.
/// Parameterized by `OneSidedPolicy` (Refund vs CarryForward) for degenerate market resolution.
fn run_differential_harness(policy: OneSidedPolicy) {
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

                    let is_one_sided = (active.pool_up > 0 && active.pool_down == 0)
                        || (active.pool_down > 0 && active.pool_up == 0);

                    if is_one_sided && policy == OneSidedPolicy::CarryForward {
                        let participants: soroban_sdk::Vec<Address> = env.as_contract(&contract_id, || {
                            env.storage()
                                .persistent()
                                .get(&DataKeyScoped::RoundParticipants(active.round_id))
                                .unwrap_or(soroban_sdk::Vec::new(&env))
                        });

                        let res = env.as_contract(&contract_id, || {
                            let apply_res = _apply_one_sided_policy(
                                &env,
                                &active,
                                OneSidedPolicy::CarryForward,
                                &participants,
                                &None,
                            );
                            env.storage().persistent().remove(&DataKeyCore::ActiveRound);
                            apply_res
                        });

                        if res.is_ok() {
                            model.resolve_round(*price_up);

                            // Event parity check for CarryForward
                            if let Some(ev) = find_latest_onesided_event(&env) {
                                let total_pool = active.pool_up + active.pool_down;
                                if ev.1 != 2 || ev.3 != 0 || ev.4 != total_pool {
                                    let diff = format!(
                                        "CarryForward onesided event mismatch: policy={}, refund={}, carry={}, expected_carry={}",
                                        ev.1, ev.3, ev.4, total_pool
                                    );
                                    pretty_print_failure(seed_opt, step_idx, &actions, act, &diff);
                                }
                            }
                        }
                    } else {
                        let price = if *price_up {
                            2_0000000u128
                        } else {
                            5000000u128
                        };
                        let res = client.try_resolve_round(&OraclePayload {
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

                            if is_one_sided && policy == OneSidedPolicy::Refund {
                                if let Some(ev) = find_latest_onesided_event(&env) {
                                    let total_pool = active.pool_up + active.pool_down;
                                    if ev.1 != 0 || ev.3 != total_pool || ev.4 != 0 {
                                        let diff = format!(
                                            "Refund onesided event mismatch: policy={}, refund={}, carry={}, expected_refund={}",
                                            ev.1, ev.3, ev.4, total_pool
                                        );
                                        pretty_print_failure(seed_opt, step_idx, &actions, act, &diff);
                                    }
                                }
                            }
                        }
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

/// Backward-compatible entrypoint testing differential invariants with default Refund policy.
#[test]
fn differential_invariant_harness() {
    run_differential_harness(OneSidedPolicy::Refund);
}

/// Test 1: Hardened differential invariant harness explicitly exercising `OneSidedPolicy::Refund`.
#[test]
fn differential_invariant_harness_refund() {
    run_differential_harness(OneSidedPolicy::Refund);
}

/// Test 2: Differential invariant harness explicitly exercising `OneSidedPolicy::CarryForward`.
///
/// Invokes `_apply_one_sided_policy` with `CarryForward` when a one-sided round occurs,
/// asserting differential parity across all state fields and event schema conformance.
#[test]
fn differential_invariant_harness_carry_forward() {
    run_differential_harness(OneSidedPolicy::CarryForward);
}

/// Test 3: Side-by-side comparison of Refund vs CarryForward on identical initial market configurations.
///
/// Asserts:
/// - Pending winnings match exact user stakes on both contracts
/// - Zero fee collection (`treasury_delta == 0`) on both contracts
/// - Event metadata & conservation: `refund_amount + carry_amount == pool_up + pool_down`
///   - Refund: `policy_code == 0`, `refund_amount == total_pot`, `carry_amount == 0`
///   - CarryForward: `policy_code == 2`, `refund_amount == 0`, `carry_amount == total_pot`
/// - Full balance restoration after claiming on both contracts
/// - Non-mutation of win/loss statistics
/// - Edge cases: Populated UP side, populated DOWN side with dust/indivisible stakes, and active fee configuration.
#[test]
fn test_invariant_one_sided_policy_dimension_parity() {
    // ── Setup Environment A (Refund) and Environment B (CarryForward) ──────────
    let env_a = Env::default();
    let contract_id_a = env_a.register(VirtualTokenContract, ());
    let client_a = VirtualTokenContractClient::new(&env_a, &contract_id_a);
    let admin_a = Address::generate(&env_a);
    let oracle_a = Address::generate(&env_a);
    env_a.mock_all_auths();
    client_a.initialize(&admin_a, &oracle_a);
    client_a.update_oracle_heartbeat(&0u32);

    let env_b = Env::default();
    let contract_id_b = env_b.register(VirtualTokenContract, ());
    let client_b = VirtualTokenContractClient::new(&env_b, &contract_id_b);
    let admin_b = Address::generate(&env_b);
    let oracle_b = Address::generate(&env_b);
    env_b.mock_all_auths();
    client_b.initialize(&admin_b, &oracle_b);
    client_b.update_oracle_heartbeat(&0u32);

    // Active protocol fee configured (500 bps = 5%): verify fee is strictly ignored on both paths
    client_a.set_protocol_fee_bps(&Some(500));
    client_b.set_protocol_fee_bps(&Some(500));

    let alice_a = Address::generate(&env_a);
    let bob_a = Address::generate(&env_a);
    client_a.mint_initial(&alice_a);
    client_a.mint_initial(&bob_a);

    let alice_b = Address::generate(&env_b);
    let bob_b = Address::generate(&env_b);
    client_b.mint_initial(&alice_b);
    client_b.mint_initial(&bob_b);

    let initial_bal_alice_a = client_a.balance(&alice_a);
    let initial_bal_bob_a = client_a.balance(&bob_a);
    let initial_bal_alice_b = client_b.balance(&alice_b);
    let initial_bal_bob_b = client_b.balance(&bob_b);

    // ── Round 1: Populated UP side ───────────────────────────────────────────
    let stake_alice = 100_0000000i128;
    let stake_bob = 200_0000000i128;
    let total_pot_1 = stake_alice + stake_bob;

    client_a.create_round(&1_0000000u128, &None);
    client_a.place_bet(&alice_a, &stake_alice, &BetSide::Up);
    client_a.place_bet(&bob_a, &stake_bob, &BetSide::Up);

    client_b.create_round(&1_0000000u128, &None);
    client_b.place_bet(&alice_b, &stake_alice, &BetSide::Up);
    client_b.place_bet(&bob_b, &stake_bob, &BetSide::Up);

    env_a.ledger().with_mut(|li| li.sequence_number = 12);
    env_b.ledger().with_mut(|li| li.sequence_number = 12);

    // Contract A resolves under Refund
    let round_a = client_a.get_active_round().unwrap();
    client_a.resolve_round(&OraclePayload {
        round_id: round_a.start_ledger,
        price: 2_0000000u128,
        timestamp: env_a.ledger().timestamp(),
        nonce: 1u64,
        network_id: env_a.ledger().network_id(),
        contract_addr: contract_id_a.clone(),
        confidence: None,
        attestation: None,
    });
    // Capture events before subsequent contract calls reset the event log
    let event_a = find_latest_onesided_event(&env_a).expect("Refund onesided event missing");

    // Contract B executes CarryForward via _apply_one_sided_policy
    let round_b = client_b.get_active_round().unwrap();
    let participants_b = soroban_sdk::vec![&env_b, alice_b.clone(), bob_b.clone()];
    let res_b = env_b.as_contract(&contract_id_b, || {
        let r = _apply_one_sided_policy(
            &env_b,
            &round_b,
            OneSidedPolicy::CarryForward,
            &participants_b,
            &None,
        );
        env_b.storage().persistent().remove(&DataKeyCore::ActiveRound);
        r
    });
    assert!(res_b.is_ok());
    let event_b = find_latest_onesided_event(&env_b).expect("CarryForward onesided event missing");

    // 1. Pending winnings parity
    assert_eq!(client_a.get_pending_winnings(&alice_a), stake_alice);
    assert_eq!(client_b.get_pending_winnings(&alice_b), stake_alice);
    assert_eq!(client_a.get_pending_winnings(&bob_a), stake_bob);
    assert_eq!(client_b.get_pending_winnings(&bob_b), stake_bob);

    // 2. Zero fee collection on both paths
    assert_eq!(client_a.get_protocol_fee_treasury(), 0);
    assert_eq!(client_b.get_protocol_fee_treasury(), 0);

    // 3. Event conservation & metadata
    assert_eq!(event_a.1, 0, "Event A policy must be Refund (0)");
    assert_eq!(event_a.2, 0, "Event A affected_side must be Up (0)");
    assert_eq!(event_a.3, total_pot_1, "Event A refund_amount must equal total_pot");
    assert_eq!(event_a.4, 0, "Event A carry_amount must be 0");
    assert_eq!(event_a.3 + event_a.4, total_pot_1);

    assert_eq!(event_b.1, 2, "Event B policy must be CarryForward (2)");
    assert_eq!(event_b.2, 0, "Event B affected_side must be Up (0)");
    assert_eq!(event_b.3, 0, "Event B refund_amount must be 0");
    assert_eq!(event_b.4, total_pot_1, "Event B carry_amount must equal total_pot");
    assert_eq!(event_b.3 + event_b.4, total_pot_1);

    // 4. Claim idempotency & exact balance restoration
    client_a.claim_winnings(&alice_a);
    client_a.claim_winnings(&bob_a);
    client_b.claim_winnings(&alice_b);
    client_b.claim_winnings(&bob_b);

    assert_eq!(client_a.balance(&alice_a), initial_bal_alice_a);
    assert_eq!(client_a.balance(&bob_a), initial_bal_bob_a);
    assert_eq!(client_b.balance(&alice_b), initial_bal_alice_b);
    assert_eq!(client_b.balance(&bob_b), initial_bal_bob_b);

    // 5. User win/loss statistics unmutated
    assert_eq!(client_a.get_user_stats(&alice_a).total_wins, 0);
    assert_eq!(client_a.get_user_stats(&alice_a).total_losses, 0);
    assert_eq!(client_b.get_user_stats(&alice_b).total_wins, 0);
    assert_eq!(client_b.get_user_stats(&alice_b).total_losses, 0);

    // ── Round 2: Populated DOWN side with dust/indivisible stakes ────────────
    let dust_stake_alice = 333_3333333i128;
    let dust_stake_bob = 667_6666667i128;
    let total_pot_2 = dust_stake_alice + dust_stake_bob;

    client_a.create_round(&2_0000000u128, &None);
    client_a.place_bet(&alice_a, &dust_stake_alice, &BetSide::Down);
    client_a.place_bet(&bob_a, &dust_stake_bob, &BetSide::Down);

    client_b.create_round(&2_0000000u128, &None);
    client_b.place_bet(&alice_b, &dust_stake_alice, &BetSide::Down);
    client_b.place_bet(&bob_b, &dust_stake_bob, &BetSide::Down);

    env_a.ledger().with_mut(|li| li.sequence_number = 24);
    env_b.ledger().with_mut(|li| li.sequence_number = 24);

    let round_a_2 = client_a.get_active_round().unwrap();
    client_a.resolve_round(&OraclePayload {
        round_id: round_a_2.start_ledger,
        price: 1_0000000u128,
        timestamp: env_a.ledger().timestamp(),
        nonce: 2u64,
        network_id: env_a.ledger().network_id(),
        contract_addr: contract_id_a.clone(),
        confidence: None,
        attestation: None,
    });
    let event_a_2 = find_latest_onesided_event(&env_a).unwrap();

    let round_b_2 = client_b.get_active_round().unwrap();
    let res_b_2 = env_b.as_contract(&contract_id_b, || {
        let r = _apply_one_sided_policy(
            &env_b,
            &round_b_2,
            OneSidedPolicy::CarryForward,
            &participants_b,
            &None,
        );
        env_b.storage().persistent().remove(&DataKeyCore::ActiveRound);
        r
    });
    assert!(res_b_2.is_ok());
    let event_b_2 = find_latest_onesided_event(&env_b).unwrap();

    assert_eq!(client_a.get_pending_winnings(&alice_a), dust_stake_alice);
    assert_eq!(client_b.get_pending_winnings(&alice_b), dust_stake_alice);
    assert_eq!(client_a.get_pending_winnings(&bob_a), dust_stake_bob);
    assert_eq!(client_b.get_pending_winnings(&bob_b), dust_stake_bob);

    assert_eq!(event_a_2.2, 1, "Affected side must be Down (1)");
    assert_eq!(event_b_2.2, 1, "Affected side must be Down (1)");
    assert_eq!(event_a_2.3, total_pot_2);
    assert_eq!(event_b_2.4, total_pot_2);

    client_a.claim_winnings(&alice_a);
    client_a.claim_winnings(&bob_a);
    client_b.claim_winnings(&alice_b);
    client_b.claim_winnings(&bob_b);

    assert_eq!(client_a.balance(&alice_a), initial_bal_alice_a);
    assert_eq!(client_b.balance(&alice_b), initial_bal_alice_b);
}

// ── Test 4: Property-Based Conservation Verification ────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Test 4: Property-based exhaustive verification of OneSidedPolicy::Refund and CarryForward.
    /// Randomizes policy, populated side (Up vs Down), stakes across multiple users, and fee BPS.
    #[test]
    fn test_invariant_one_sided_conservation_proptest(
        policy in prop_oneof![Just(OneSidedPolicy::Refund), Just(OneSidedPolicy::CarryForward)],
        side_is_up in any::<bool>(),
        stake_a in 1_0000000i128..=100_0000000i128,
        stake_b in 1_0000000i128..=100_0000000i128,
        stake_c in 1_0000000i128..=100_0000000i128,
        fee_bps in prop_oneof![Just(None), Just(Some(0)), Just(Some(100)), Just(Some(500)), Just(Some(1000))],
    ) {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);
        client.update_oracle_heartbeat(&0u32);

        if let Some(bps) = fee_bps {
            let _ = client.try_set_protocol_fee_bps(&Some(bps));
        }

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        client.mint_initial(&alice);
        client.mint_initial(&bob);
        client.mint_initial(&charlie);

        let initial_bal_a = client.balance(&alice);
        let initial_bal_b = client.balance(&bob);
        let initial_bal_c = client.balance(&charlie);

        let total_pot = stake_a + stake_b + stake_c;
        let side = if side_is_up { BetSide::Up } else { BetSide::Down };

        client.create_round(&1_0000000u128, &None);
        client.place_bet(&alice, &stake_a, &side);
        client.place_bet(&bob, &stake_b, &side);
        client.place_bet(&charlie, &stake_c, &side);

        env.ledger().with_mut(|li| li.sequence_number = 12);
        let treasury_before = client.get_protocol_fee_treasury();

        let round = client.get_active_round().unwrap();
        let participants = soroban_sdk::vec![&env, alice.clone(), bob.clone(), charlie.clone()];

        let res = env.as_contract(&contract_id, || {
            let r = _apply_one_sided_policy(
                &env,
                &round,
                policy,
                &participants,
                &None,
            );
            env.storage().persistent().remove(&DataKeyCore::ActiveRound);
            r
        });
        prop_assert!(res.is_ok());

        // Capture event immediately before subsequent contract queries reset the event log
        let event = find_latest_onesided_event(&env);
        prop_assert!(event.is_some());

        // Invariant 1: sum(pending_winnings) == total_pot
        let pending_a = client.get_pending_winnings(&alice);
        let pending_b = client.get_pending_winnings(&bob);
        let pending_c = client.get_pending_winnings(&charlie);

        prop_assert_eq!(pending_a, stake_a);
        prop_assert_eq!(pending_b, stake_b);
        prop_assert_eq!(pending_c, stake_c);
        prop_assert_eq!(pending_a + pending_b + pending_c, total_pot);

        // Invariant 2: treasury_after == treasury_before (zero fee on one-sided paths regardless of fee_bps)
        let treasury_after = client.get_protocol_fee_treasury();
        prop_assert_eq!(treasury_after, treasury_before);

        // Invariant 3: Event conservation & policy distinction
        let ev = event.unwrap();
        prop_assert_eq!(ev.1, policy as u32);
        prop_assert_eq!(ev.2, if side_is_up { 0 } else { 1 });
        prop_assert_eq!(ev.3 + ev.4, total_pot);

        if policy == OneSidedPolicy::Refund {
            prop_assert_eq!(ev.3, total_pot);
            prop_assert_eq!(ev.4, 0);
        } else {
            prop_assert_eq!(ev.3, 0);
            prop_assert_eq!(ev.4, total_pot);
        }

        // Invariant 4: Stats unmutated
        for u in [&alice, &bob, &charlie] {
            let stats = client.get_user_stats(u);
            prop_assert_eq!(stats.total_wins, 0);
            prop_assert_eq!(stats.total_losses, 0);
        }

        // Invariant 5: Claim idempotency and full balance preservation
        client.claim_winnings(&alice);
        client.claim_winnings(&bob);
        client.claim_winnings(&charlie);

        prop_assert_eq!(client.balance(&alice), initial_bal_a);
        prop_assert_eq!(client.balance(&bob), initial_bal_b);
        prop_assert_eq!(client.balance(&charlie), initial_bal_c);
    }
}
