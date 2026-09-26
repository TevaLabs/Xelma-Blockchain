// SPDX-License-Identifier: MIT
//! Property-based fuzz harness for protocol lifecycle actions (Issue #561).
//!
//! Validates 5 core protocol invariants after every generated action:
//! 1. Asset & Value Conservation (`sum(balances) + sum(pending) + treasury + pot <= total_minted`)
//! 2. Non-Negative Balances (`balance >= 0`, `pending >= 0`, `treasury >= 0`, `pot >= 0`)
//! 3. Treasury Fee Consistency (`treasury >= 0`)
//! 4. Round Lifecycle Finality (inactive round state rejects bets without state mutation)
//! 5. Claim Idempotency & Protection (claiming 0 pending winnings leaves balance unchanged)
//!
//! Replay a failing run with the seed printed in the diagnostic:
//! `SEED=5612026 cargo test --package xelma-contract --lib tests::fuzz_lifecycle -- --nocapture`

extern crate std;

use std::env;
use std::format;
use std::string::{String, ToString};
use std::vec;
use std::vec::Vec;

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, RngSeed, TestRunner};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, Bytes, BytesN, Env};

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, MultiFeedPayload, OraclePayload, OracleQuorumConfig};

/// Default seed used when `SEED` is unset. Pass the printed seed back via `SEED`
/// to reproduce a failing trace exactly.
pub const DEFAULT_FUZZ_SEED: u64 = 561_2026;

/// Randomized actions covering Up/Down plus precision, commit-reveal,
/// early cash-out, multi-feed resolution, and access control.
#[derive(Debug, Clone)]
pub enum LifecycleAction {
    CreateRound { start_price: u128 },
    MintUser { user_idx: usize },
    PlaceBet { user_idx: usize, amount: i128, side: BetSide },
    SetFeeBps { bps: Option<u32> },
    SetWindows { bet_ledgers: u32, run_ledgers: u32 },
    TogglePause,
    CancelRound,
    ResolveRound { price_up: bool },
    ClaimWinnings { user_idx: usize },
    WithdrawFee { amount: i128 },
    PlacePrecisionBet {
        user_idx: usize,
        amount: i128,
        target_price: u128,
    },
    CommitReveal { user_idx: usize, price: u128, salt_nonce: u64, amount: i128 },
    CashOutPosition { user_idx: usize },
    ResolveMulti { price_up: bool },
    AccessControl { user_idx: usize, deny: bool },
}

fn action_generator() -> impl Strategy<Value = LifecycleAction> {
    let user_idx = 0..5usize;
    let amount = 1_0000000i128..=40_0000000i128;
    let fee_bps = prop_oneof![
        Just(None),
        Just(Some(100u32)),
        Just(Some(500u32)),
        Just(Some(1000u32)),
    ];
    let start_price = 1_0000000u128..=5_0000000u128;
    let target_price = 1_000u128..=99_999_999u128;
    let bet_ledgers = 5u32..=12u32;
    let run_extra = 4u32..=18u32;
    let nonce = 1u64..=9_999u64;

    prop_oneof![
        start_price.clone().prop_map(|sp| LifecycleAction::CreateRound { start_price: sp }),
        user_idx.clone().prop_map(|u| LifecycleAction::MintUser { user_idx: u }),
        (user_idx.clone(), amount.clone(), any::<bool>()).prop_map(|(u, a, is_up)| {
            LifecycleAction::PlaceBet {
                user_idx: u,
                amount: a,
                side: if is_up { BetSide::Up } else { BetSide::Down },
            }
        }),
        fee_bps.prop_map(|bps| LifecycleAction::SetFeeBps { bps }),
        (bet_ledgers.clone(), run_extra.clone()).prop_map(|(b, extra)| LifecycleAction::SetWindows {
            bet_ledgers: b,
            run_ledgers: b.saturating_add(extra).max(b.saturating_add(1)),
        }),
        Just(LifecycleAction::TogglePause),
        Just(LifecycleAction::CancelRound),
        any::<bool>().prop_map(|up| LifecycleAction::ResolveRound { price_up: up }),
        user_idx.clone().prop_map(|u| LifecycleAction::ClaimWinnings { user_idx: u }),
        amount.clone().prop_map(|a| LifecycleAction::WithdrawFee { amount: a }),
        (user_idx.clone(), amount.clone(), target_price.clone()).prop_map(|(u, a, tp)| {
            LifecycleAction::PlacePrecisionBet {
                user_idx: u,
                amount: a,
                target_price: tp,
            }
        }),
        (user_idx.clone(), target_price, nonce, amount.clone()).prop_map(|(u, p, n, a)| {
            LifecycleAction::CommitReveal {
                user_idx: u,
                price: p,
                salt_nonce: n,
                amount: a,
            }
        }),
        user_idx.clone().prop_map(|u| LifecycleAction::CashOutPosition { user_idx: u }),
        any::<bool>().prop_map(|up| LifecycleAction::ResolveMulti { price_up: up }),
        (user_idx, any::<bool>()).prop_map(|(u, deny)| LifecycleAction::AccessControl {
            user_idx: u,
            deny,
        }),
    ]
}

fn sample_actions(seed: u64, seq_len: usize) -> Vec<LifecycleAction> {
    let mut config = Config::with_cases(1);
    config.rng_seed = RngSeed::Fixed(seed);
    let mut runner = TestRunner::new(config);
    let strategy = prop::collection::vec(action_generator(), seq_len..=seq_len);
    strategy
        .new_tree(&mut runner)
        .expect("Failed to generate action tree")
        .current()
}

fn salt_from_nonce(env: &Env, nonce: u64) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    let raw = nonce.to_le_bytes();
    let mut i = 0;
    while i < 32 {
        bytes[i] = raw[i % 8].wrapping_add(i as u8).wrapping_mul(17).wrapping_add(3);
        i += 1;
    }
    bytes[0] |= 0xA5;
    bytes[31] ^= 0x5A;
    BytesN::from_array(env, &bytes)
}

fn commitment_hash(env: &Env, price: u128, salt: &BytesN<32>) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&price.to_xdr(env));
    preimage.append(&salt.clone().to_xdr(env));
    env.crypto().sha256(&preimage).into()
}

fn report_fuzz_failure(
    seed: u64,
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
         Seed: {}\n\
         Failing Step Index: {}\n\
         Violated Invariant: {}\n\
         Failed Action: {:?}\n\
         State Diagnostic:\n{}\n\
         Action Trace History:\n{:#?}\n\
         Replay: SEED={} cargo test --package xelma-contract --lib tests::fuzz_lifecycle -- --nocapture\n\
         ===================================================================",
        mode, seed, step_idx, invariant_name, failed_action, diff, history, seed
    );
}

fn execute_sequence(seed: u64, mode: &str, actions: &[LifecycleAction]) {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    let users: Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    let mut total_minted: i128 = 0;
    // Unrevealed commit stakes are not in `pool_*` or precision predictions yet.
    // Stored preimage lets a later action reveal the same commitment.
    let mut open_commit: Vec<Option<(u128, u64, i128)>> = Vec::new();
    let mut denied: Vec<bool> = Vec::new();
    for _ in 0..users.len() {
        open_commit.push(None);
        denied.push(false);
    }
    let mut oracle_nonce: u64 = 1;

    for (step_idx, act) in actions.iter().enumerate() {
        match act {
            LifecycleAction::CreateRound { start_price } => {
                if client.get_active_round().is_none() {
                    let _ = client.try_create_round(start_price, &None);
                }
            }
            LifecycleAction::MintUser { user_idx } => {
                let user = &users[*user_idx % users.len()];
                if client.try_mint_initial(user).is_ok() {
                    total_minted += 1000_0000000;
                }
            }
            LifecycleAction::PlaceBet { user_idx, amount, side } => {
                let user = &users[*user_idx % users.len()];
                let _ = client.try_place_bet(user, amount, side);
            }
            LifecycleAction::SetFeeBps { bps } => {
                let _ = client.try_set_protocol_fee_bps(bps);
            }
            LifecycleAction::SetWindows { bet_ledgers, run_ledgers } => {
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
                    env.ledger().with_mut(|li| li.sequence_number = active.end_ledger);
                    let price = if *price_up {
                        active.price_start.saturating_add(1).max(1)
                    } else {
                        active.price_start.saturating_sub(1).max(1)
                    };
                    oracle_nonce += 1;
                    let _ = client.try_resolve_round(&OraclePayload {
                        price,
                        timestamp: env.ledger().timestamp(),
                        round_id: active.start_ledger,
                        nonce: oracle_nonce,
                        network_id: env.ledger().network_id(),
                        contract_addr: contract_id.clone(),
                        confidence: None,
                        attestation: None,
                    });
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
            LifecycleAction::PlacePrecisionBet {
                user_idx,
                amount,
                target_price,
            } => {
                if client.get_active_round().is_none() {
                    let _ = client.try_create_round(&1_0000000u128, &Some(1u32));
                }
                let user = &users[*user_idx % users.len()];
                let price = (*target_price).min(99_999_999);
                let _ = client.try_place_precision_prediction(user, amount, &price);
            }
            LifecycleAction::CommitReveal {
                user_idx,
                price,
                salt_nonce,
                amount,
            } => {
                let idx = *user_idx % users.len();
                let user = &users[idx];
                if client.get_active_round().is_none() {
                    let _ = client.try_create_round(&1_0000000u128, &Some(1u32));
                }
                if let Some(active) = client.get_active_round() {
                    let seq = env.ledger().sequence();
                    if seq < active.bet_end_ledger {
                        let price = (*price).min(99_999_999);
                        let salt = salt_from_nonce(&env, *salt_nonce);
                        let hash = commitment_hash(&env, price, &salt);
                        if client.try_commit_prediction(user, &hash, amount).is_ok() {
                            open_commit[idx] = Some((price, *salt_nonce, *amount));
                        }
                    } else if seq < active.end_ledger {
                        if let Some((stored_price, stored_nonce, _)) = open_commit[idx] {
                            let salt = salt_from_nonce(&env, stored_nonce);
                            if client
                                .try_reveal_prediction(user, &stored_price, &salt)
                                .is_ok()
                            {
                                open_commit[idx] = None;
                            }
                        }
                    }
                }
            }
            LifecycleAction::CashOutPosition { user_idx } => {
                let _ = client.try_set_early_cashout_bps(&Some(500u32));
                if let Some(active) = client.get_active_round() {
                    if env.ledger().sequence() < active.bet_end_ledger {
                        let bet_end = active.bet_end_ledger;
                        env.ledger().with_mut(|li| li.sequence_number = bet_end);
                    }
                }
                let user = &users[*user_idx % users.len()];
                let _ = client.try_cash_out_early(user);
            }
            LifecycleAction::ResolveMulti { price_up } => {
                let _ = client.try_set_oracle_quorum_config(&Some(OracleQuorumConfig {
                    min_observations: 3,
                    quorum_threshold: 3,
                    outlier_threshold_bps: 500,
                }));
                if let Some(active) = client.get_active_round() {
                    env.ledger().with_mut(|li| li.sequence_number = active.end_ledger);
                    let base = active.price_start.max(1);
                    let (p0, p1, p2) = if *price_up {
                        (base, base.saturating_add(1), base.saturating_add(2))
                    } else {
                        (
                            base.saturating_sub(1).max(1),
                            base,
                            base.saturating_add(1),
                        )
                    };
                    oracle_nonce += 1;
                    let _ = client.try_resolve_round_multi(&MultiFeedPayload {
                        prices: soroban_sdk::vec![&env, p0, p1, p2],
                        sources: soroban_sdk::vec![&env, 0u32, 1u32, 2u32],
                        round_id: active.start_ledger,
                        nonce: oracle_nonce,
                        network_id: env.ledger().network_id(),
                        contract_addr: contract_id.clone(),
                        timestamp: env.ledger().timestamp(),
                    });
                }
            }
            LifecycleAction::AccessControl { user_idx, deny } => {
                let idx = *user_idx % users.len();
                let user = &users[idx];
                if *deny {
                    if client.try_add_denylisted(user).is_ok() {
                        denied[idx] = true;
                    }
                } else if denied[idx] && client.try_remove_denylisted(user).is_ok() {
                    denied[idx] = false;
                } else {
                    let _ = client.try_add_allowlisted(user);
                }
            }
        }

        if client.get_active_round().is_none() {
            for slot in open_commit.iter_mut() {
                *slot = None;
            }
        }

        let sum_user_balances: i128 = users.iter().map(|u| client.balance(u)).sum();
        let sum_pending_winnings: i128 = users.iter().map(|u| client.get_pending_winnings(u)).sum();
        let treasury = client.get_protocol_fee_treasury();
        let pool = client
            .get_active_round()
            .map(|r| r.pool_up + r.pool_down)
            .unwrap_or(0);
        let precision_locked: i128 = client
            .get_precision_predictions()
            .iter()
            .map(|p| p.amount)
            .sum();
        let commit_locked: i128 = open_commit.iter().filter_map(|slot| slot.map(|(_, _, amount)| amount)).sum();
        let active_pot = pool + precision_locked + commit_locked;
        let total_accounted = sum_user_balances + sum_pending_winnings + treasury + active_pot;

        if total_minted > 0 && total_accounted > total_minted {
            let diff = format!(
                "Conservation Leak: accounted={total_accounted}, total_minted={total_minted}, pool={pool}, precision_locked={precision_locked}, commit_locked={commit_locked}, treasury={treasury}"
            );
            report_fuzz_failure(seed, mode, step_idx, "Asset Conservation", act, &diff, actions);
        }

        for u in &users {
            let bal = client.balance(u);
            let pending = client.get_pending_winnings(u);
            if bal < 0 || pending < 0 {
                let diff = format!(
                    "Negative user balance/pending for {u:?}: bal={bal}, pending={pending}"
                );
                report_fuzz_failure(seed, mode, step_idx, "Non-Negative Balances", act, &diff, actions);
            }
        }

        if treasury < 0 || active_pot < 0 {
            let diff = format!("Negative treasury or pot: treasury={treasury}, pot={active_pot}");
            report_fuzz_failure(seed, mode, step_idx, "Treasury Fee Consistency", act, &diff, actions);
        }

        if client.get_active_round().is_none() {
            let dummy_user = &users[0];
            let bal_before = client.balance(dummy_user);
            let res = client.try_place_bet(dummy_user, &1_0000000i128, &BetSide::Up);
            if res.is_ok() || client.balance(dummy_user) != bal_before {
                let diff = "Bet accepted or balance modified when no active round exists".to_string();
                report_fuzz_failure(seed, mode, step_idx, "Round Lifecycle Finality", act, &diff, actions);
            }
        }

        for u in &users {
            if client.get_pending_winnings(u) == 0 {
                let bal_before = client.balance(u);
                let claimed = match client.try_claim_winnings(u) {
                    Ok(Ok(amount)) => amount,
                    _ => 0,
                };
                if claimed != 0 || client.balance(u) != bal_before {
                    let diff = format!(
                        "Claim returned non-zero ({claimed}) or balance changed on 0 pending winnings"
                    );
                    report_fuzz_failure(seed, mode, step_idx, "Claim Protection", act, &diff, actions);
                }
            }
        }
    }
}

fn configured_seed_and_length() -> (u64, &'static str, usize) {
    let mode = env::var("FUZZ_MODE").unwrap_or_else(|_| "fast".to_string());
    let seq_len = if mode == "extended" { 40 } else { 16 };
    let seed = env::var("SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_FUZZ_SEED);
    let mode_static = if mode == "extended" { "extended" } else { "fast" };
    (seed, mode_static, seq_len)
}

#[test]
fn fuzz_protocol_lifecycle_invariants() {
    let (seed, mode, seq_len) = configured_seed_and_length();
    std::println!("Fuzz execution seed recorded: {seed}");
    let actions = sample_actions(seed, seq_len);
    execute_sequence(seed, mode, &actions);
}

/// Same seed must reproduce the same action trace; a different seed must not.
#[test]
fn fuzz_lifecycle_seeded_repro_matches_trace() {
    let seed = DEFAULT_FUZZ_SEED;
    let first = sample_actions(seed, 16);
    let second = sample_actions(seed, 16);
    assert_eq!(
        format!("{first:?}"),
        format!("{second:?}"),
        "seed {seed} did not reproduce the action trace"
    );
    let other = sample_actions(seed.wrapping_add(1), 16);
    assert_ne!(
        format!("{first:?}"),
        format!("{other:?}"),
        "distinct seeds produced the same trace"
    );
    std::println!("Seeded repro ok for SEED={seed}");
}
