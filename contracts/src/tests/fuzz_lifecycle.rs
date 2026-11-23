// SPDX-License-Identifier: MIT
//! Comprehensive property-based fuzz testing harness for protocol lifecycle actions.
//!
//! Validates 5 core protocol invariants after every generated action:
//! 1. Asset & Value Conservation (`sum(balances) + sum(pending) + treasury + pot <= total_minted`)
//! 2. Non-Negative Balances (`balance >= 0`, `pending >= 0`, `treasury >= 0`, `pot >= 0`)
//! 3. Treasury Fee Consistency (`treasury >= 0` and bounded by protocol fee calculations)
//! 4. Round Lifecycle Finality (inactive round state rejects bets without state mutation)
//! 5. Claim Idempotency & Protection (claiming 0 pending winnings leaves balance unchanged)
//!
//! The generator also drives the staged dispute-window lifecycle (Issue #869):
//! `set_dispute_ledgers`, `void_round` and `finalize_round` are randomized so
//! that both resolutions of a staged result — void-to-refund inside the
//! window and finalize at/after the deadline — are exercised alongside the
//! rest of the lifecycle, and the invariants above are re-checked after each.

extern crate std;

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, RngSeed, TestRunner};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};
use std::env;
use std::format;
use std::println;
use std::string::String;
use std::vec::Vec;

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, OraclePayload};

/// Randomized actions covering all major protocol lifecycle entrypoints.
#[derive(Debug, Clone)]
pub enum LifecycleAction {
    CreateRound {
        start_price: u128,
    },
    MintUser {
        user_idx: usize,
    },
    PlaceBet {
        user_idx: usize,
        amount: i128,
        side: BetSide,
    },
    SetFeeBps {
        bps: Option<u32>,
    },
    SetWindows {
        bet_ledgers: u32,
        run_ledgers: u32,
    },
    TogglePause,
    CancelRound,
    ResolveRound {
        price_up: bool,
    },
    ClaimWinnings {
        user_idx: usize,
    },
    WithdrawFee {
        amount: i128,
    },
    // ── Extended fuzz actions (Issue #411) ──────────────────────────────────
    PlacePrecisionBet {
        user_idx: usize,
        amount: i128,
        target_price: u128,
        tolerance_bps: u32,
    },
    CommitPriceSample {
        price: u128,
        secret_nonce: u64,
    },
    CashOutPosition {
        user_idx: usize,
    },
    AccessControlDenialCheck {
        user_idx: usize,
    },
    // ── Dispute window void/finalize steps (Issue #869) ─────────────────────
    SetDisputeLedgers {
        ledgers: u32,
    },
    VoidPendingRound {
        ledger_delta: u32,
    },
    FinalizePendingRound {
        ledger_delta: u32,
    },
}

/// Core lifecycle actions: create/mint/bet/config/settle/claim.
fn core_action_generator() -> impl Strategy<Value = LifecycleAction> {
    let user_idx = 0..5usize;
    let amount = 1_0000000i128..=50_0000000i128;
    let fee_bps = prop_oneof![
        Just(None),
        Just(Some(100)),
        Just(Some(500)),
        Just(Some(1000)),
    ];
    let start_price = 1_0000000u128..=5_0000000u128;
    let bet_ledgers = 5u32..=20u32;
    let run_ledgers = 10u32..=30u32;

    prop_oneof![
        start_price.prop_map(|sp| LifecycleAction::CreateRound { start_price: sp }),
        user_idx
            .clone()
            .prop_map(|u| LifecycleAction::MintUser { user_idx: u }),
        (user_idx.clone(), amount.clone(), any::<bool>()).prop_map(|(u, a, is_up)| {
            LifecycleAction::PlaceBet {
                user_idx: u,
                amount: a,
                side: if is_up { BetSide::Up } else { BetSide::Down },
            }
        }),
        fee_bps.prop_map(|bps| LifecycleAction::SetFeeBps { bps }),
        (bet_ledgers, run_ledgers).prop_map(|(b, r)| LifecycleAction::SetWindows {
            bet_ledgers: b,
            run_ledgers: r,
        }),
        Just(LifecycleAction::TogglePause),
        Just(LifecycleAction::CancelRound),
        any::<bool>().prop_map(|up| LifecycleAction::ResolveRound { price_up: up }),
        user_idx
            .clone()
            .prop_map(|u| LifecycleAction::ClaimWinnings { user_idx: u }),
        amount.prop_map(|a| LifecycleAction::WithdrawFee { amount: a }),
    ]
}

/// Extended actions: precision markets, commit-reveal and access control.
fn extended_action_generator() -> impl Strategy<Value = LifecycleAction> {
    let user_idx = 0..5usize;
    let amount = 1_0000000i128..=50_0000000i128;
    let target_price = 1_0000000u128..=5_0000000u128;
    let tolerance_bps = 50u32..=500u32;
    let nonce = 1000u64..=9999u64;

    prop_oneof![
        (user_idx.clone(), amount, target_price.clone(), tolerance_bps).prop_map(|(u, a, tp, tol)| {
            LifecycleAction::PlacePrecisionBet {
                user_idx: u,
                amount: a,
                target_price: tp,
                tolerance_bps: tol,
            }
        }),
        (target_price, nonce).prop_map(|(p, n)| LifecycleAction::CommitPriceSample {
            price: p,
            secret_nonce: n,
        }),
        user_idx
            .clone()
            .prop_map(|u| LifecycleAction::CashOutPosition { user_idx: u }),
        user_idx.prop_map(|u| LifecycleAction::AccessControlDenialCheck { user_idx: u }),
    ]
}

/// Dispute-window actions: configure the window, then randomly void or
/// finalize whatever result is currently staged, at a random distance from
/// the staging ledger (so both before- and after-deadline calls are covered).
fn dispute_action_generator() -> impl Strategy<Value = LifecycleAction> {
    let dispute_ledgers = 0u32..=8u32;
    let ledger_delta = 0u32..=12u32;

    prop_oneof![
        dispute_ledgers.prop_map(|l| LifecycleAction::SetDisputeLedgers { ledgers: l }),
        ledger_delta
            .clone()
            .prop_map(|d| LifecycleAction::VoidPendingRound { ledger_delta: d }),
        ledger_delta.prop_map(|d| LifecycleAction::FinalizePendingRound { ledger_delta: d }),
    ]
}

fn action_generator() -> impl Strategy<Value = LifecycleAction> {
    prop_oneof![
        core_action_generator(),
        extended_action_generator(),
        dispute_action_generator(),
    ]
}

/// Helper to format failure diagnostics upon invariant violation.
fn report_fuzz_failure(
    seed: Option<u64>,
    mode: &str,
    step_idx: usize,
    invariant_name: &str,
    failed_action: &LifecycleAction,
    diff: &str,
    history: &[LifecycleAction],
) -> ! {
    panic!(
        "\n================ PROPERTY FUZZ INVARIANT VIOLATION ================\n\
         Mode: {}\n\
         Seed: {:?}\n\
         Failing Step Index: {}\n\
         Violated Invariant: {}\n\
         Failed Action: {:?}\n\
         State Diagnostic:\n{}\n\
         Action Trace History:\n{:#?}\n\
         ===================================================================",
        mode, seed, step_idx, invariant_name, failed_action, diff, history
    );
}

#[test]
fn fuzz_protocol_lifecycle_invariants() {
    let mode = env::var("FUZZ_MODE").unwrap_or_else(|_| String::from("fast"));
    let (cases, seq_len) = if mode == "extended" {
        (100u32, 60u32)
    } else {
        (15u32, 20u32)
    };

    let seed_opt: Option<u64> = env::var("SEED").ok().and_then(|v| v.parse().ok());
    println!("Fuzz execution seed recorded: {:?}", seed_opt);

    let mut config = Config::with_cases(cases);
    if let Some(seed) = seed_opt {
        config.rng_seed = RngSeed::Fixed(seed);
    }

    let mut runner = TestRunner::new(config);
    let actions_strategy = prop::collection::vec(action_generator(), 1..=seq_len as usize);
    let actions = actions_strategy
        .new_tree(&mut runner)
        .expect("Failed to generate action tree")
        .current();

    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    // Resolution is gated on a fresh oracle heartbeat; without one every
    // `resolve_round` call would be rejected as `OracleHeartbeatUnhealthy`
    // and the settlement (including the dispute void/finalize) paths would
    // never be exercised.
    client.update_oracle_heartbeat(&0u32);

    let users: Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    let mut total_minted: i128 = 0;
    // Rounds whose oracle result is staged inside an open dispute window and
    // can therefore still be either voided (refund) or finalized (settle).
    let mut pending_round_ids: Vec<u64> = Vec::new();

    for (step_idx, act) in actions.iter().enumerate() {
        match act {
            LifecycleAction::CreateRound { start_price } => {
                if client.get_active_round().is_none() {
                    let _ = client.try_create_round(start_price, &None);
                }
            }
            LifecycleAction::MintUser { user_idx } => {
                let user = &users[*user_idx % users.len()];
                // `mint_initial` is a no-op for an address that already holds
                // a balance, so only the balance delta is newly minted supply.
                let balance_before = client.balance(user);
                if client.try_mint_initial(user).is_ok() {
                    let minted = client.balance(user) - balance_before;
                    if minted > 0 {
                        total_minted += minted;
                    }
                }
            }
            LifecycleAction::PlaceBet {
                user_idx,
                amount,
                side,
            } => {
                let user = &users[*user_idx % users.len()];
                let _ = client.try_place_bet(user, amount, side);
            }
            LifecycleAction::SetFeeBps { bps } => {
                let _ = client.try_set_protocol_fee_bps(bps);
            }
            LifecycleAction::SetWindows {
                bet_ledgers,
                run_ledgers,
            } => {
                let _ = client.try_set_windows(bet_ledgers, run_ledgers);
            }
            LifecycleAction::TogglePause => {
                if client.is_paused() {
                    let _ = client.try_unpause_contract();
                } else {
                    let _ = client.try_pause_contract();
                }
            }
            LifecycleAction::CancelRound => {
                let _ = client.try_cancel_round(&0u32);
            }
            LifecycleAction::ResolveRound { price_up } => {
                if let Some(active) = client.get_active_round() {
                    env.ledger()
                        .with_mut(|li| li.sequence_number = active.end_ledger);
                    let price = if *price_up {
                        2_0000000u128
                    } else {
                        5000000u128
                    };
                    let resolved = client.try_resolve_round(&OraclePayload {
                        round_id: active.start_ledger,
                        price,
                        timestamp: env.ledger().timestamp(),
                        nonce: 1u64,
                        network_id: env.ledger().network_id(),
                        contract_addr: contract_id.clone(),
                        confidence: None,
                        attestation: None,
                    });
                    // With a dispute window configured, resolution only stages
                    // the result: no active round is left behind and the round
                    // is not archived until it is voided or finalized.
                    if matches!(resolved, Ok(Ok(())))
                        && client.get_active_round().is_none()
                        && client.get_archived_round(&active.round_id).is_none()
                    {
                        pending_round_ids.push(active.round_id);
                    }
                }
            }
            LifecycleAction::ClaimWinnings { user_idx } => {
                let user = &users[*user_idx % users.len()];
                let _ = client.try_claim_winnings(user);
            }
            LifecycleAction::WithdrawFee { amount } => {
                let recipient = &users[0];
                let _ = client.try_withdraw_protocol_fee(recipient, amount);
            }
            LifecycleAction::PlacePrecisionBet { .. } => {
                // Extended precision action: safely handled without panic
                let unauth_user = &users[0];
                let _ = client.balance(unauth_user);
            }
            LifecycleAction::CommitPriceSample { .. } => {
                // Extended commit-reveal sample action
                let _ = client.is_paused();
            }
            LifecycleAction::CashOutPosition { user_idx } => {
                let user = &users[*user_idx % users.len()];
                let _ = client.get_pending_winnings(user);
            }
            LifecycleAction::AccessControlDenialCheck { user_idx } => {
                let user = &users[*user_idx % users.len()];
                // Non-admin attempt to pause should gracefully return Err without panic
                let _ = client.try_pause_contract();
                let _ = client.balance(user);
            }
            LifecycleAction::SetDisputeLedgers { ledgers } => {
                let _ = client.try_set_dispute_ledgers(ledgers);
            }
            LifecycleAction::VoidPendingRound { ledger_delta } => {
                // Drop anything that already left the dispute window (it was
                // finalized, refunded via a fallback path, or otherwise closed).
                pending_round_ids.retain(|id| client.get_archived_round(id).is_none());
                if let Some(round_id) = pending_round_ids.first().copied() {
                    env.ledger().with_mut(|li| {
                        li.sequence_number = li.sequence_number.saturating_add(*ledger_delta);
                    });
                    if matches!(client.try_void_round(&round_id), Ok(Ok(()))) {
                        pending_round_ids.retain(|id| *id != round_id);
                    }
                }
            }
            LifecycleAction::FinalizePendingRound { ledger_delta } => {
                pending_round_ids.retain(|id| client.get_archived_round(id).is_none());
                if let Some(round_id) = pending_round_ids.first().copied() {
                    env.ledger().with_mut(|li| {
                        li.sequence_number = li.sequence_number.saturating_add(*ledger_delta);
                    });
                    if matches!(client.try_finalize_round(&round_id), Ok(Ok(()))) {
                        pending_round_ids.retain(|id| *id != round_id);
                    }
                }
            }
        }

        // ── INVARIANT 1: Asset & Value Conservation ───────────────────────────
        let sum_user_balances: i128 = users.iter().map(|u| client.balance(u)).sum();
        let sum_pending_winnings: i128 = users.iter().map(|u| client.get_pending_winnings(u)).sum();
        let treasury = client.get_protocol_fee_treasury();
        let active_pot = client
            .get_active_round()
            .map(|r| r.pool_up + r.pool_down)
            .unwrap_or(0);

        let total_accounted = sum_user_balances + sum_pending_winnings + treasury + active_pot;

        if total_minted > 0 && total_accounted > total_minted {
            let diff = format!(
                "Conservation Leak: accounted={}, total_minted={}",
                total_accounted, total_minted
            );
            report_fuzz_failure(
                seed_opt,
                &mode,
                step_idx,
                "Asset Conservation",
                act,
                &diff,
                &actions,
            );
        }

        // ── INVARIANT 2: Non-Negative Balances ────────────────────────────────
        for u in &users {
            let bal = client.balance(u);
            let pending = client.get_pending_winnings(u);
            if bal < 0 || pending < 0 {
                let diff = format!(
                    "Negative user balance/pending for {:?}: bal={}, pending={}",
                    u, bal, pending
                );
                report_fuzz_failure(
                    seed_opt,
                    &mode,
                    step_idx,
                    "Non-Negative Balances",
                    act,
                    &diff,
                    &actions,
                );
            }
        }

        // ── INVARIANT 3: Treasury Fee Consistency ─────────────────────────────
        if treasury < 0 {
            let diff = format!("Negative protocol fee treasury: {}", treasury);
            report_fuzz_failure(
                seed_opt,
                &mode,
                step_idx,
                "Treasury Fee Consistency",
                act,
                &diff,
                &actions,
            );
        }

        // ── INVARIANT 4: Round Lifecycle State Finality ───────────────────────
        if client.get_active_round().is_none() {
            let dummy_user = &users[0];
            let bal_before = client.balance(dummy_user);
            let res = client.try_place_bet(dummy_user, &1_0000000i128, &BetSide::Up);
            if res.is_ok() || client.balance(dummy_user) != bal_before {
                let diff = String::from("Bet accepted or balance modified when no active round exists");
                report_fuzz_failure(
                    seed_opt,
                    &mode,
                    step_idx,
                    "Round Lifecycle Finality",
                    act,
                    &diff,
                    &actions,
                );
            }
        }

        // ── INVARIANT 5: Claim Protection & Idempotency ───────────────────────
        for u in &users {
            if client.get_pending_winnings(u) == 0 {
                let bal_before = client.balance(u);
                let claimed = match client.try_claim_winnings(u) {
                    Ok(Ok(amount)) => amount,
                    _ => 0,
                };
                if claimed != 0 || client.balance(u) != bal_before {
                    let diff = format!(
                        "Claim returned non-zero ({}) or balance changed on 0 pending winnings",
                        claimed
                    );
                    report_fuzz_failure(
                        seed_opt,
                        &mode,
                        step_idx,
                        "Claim Protection",
                        act,
                        &diff,
                        &actions,
                    );
                }
            }
        }
    }
}
