// SPDX-License-Identifier: MIT
#![allow(dead_code)]
#![allow(unused)]
#![allow(clippy::mutable_key_type)]
#![allow(clippy::integer_arithmetic)]
#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]
//! Differential verification harness for settlement math (Issues #520 / #362).
//!
//! A **trusted Rust reference model** and the **contract under test** are
//! executed on identical randomized oracle cases, asserting bitwise
//! (stroop-level) equality on every outcome. Any mismatch fails the test
//! with the seed, a full case dump, a minimized failing case, and an exact
//! reproduction command.
//!
//! # Tiers
//!
//! | Tier | Subject under test                          | Comparator                     |
//! |------|---------------------------------------------|--------------------------------|
//! | A    | `settlement_math` pure engine functions      | `ref_a_*` reference functions  |
//! | B    | Full native contract settlement (testutils)  | `ref_prod_*` reference model   |
//! | C    | The **compiled WASM binary** (`WASM_PATH`)   | Tier B native run + reference  |
//!
//! Tier A catches arithmetic regressions in the audited pure engine. Tier B
//! catches regressions in the production settlement orchestration (fee
//! models, participant ordering, unrevealed-forfeit policy, remainder
//! assignment). Tier C catches any divergence introduced by the
//! `wasm32v1-none` compilation pipeline itself (optimizations, target
//! codegen) — the same WASM that gets deployed on-chain is executed in the
//! Soroban test environment and must produce stroop-identical results.
//!
//! # Execution modes
//!
//! `DIFF_VERIFY_MODE` selects the randomized case count:
//!
//! | Mode       | Tier A cases | Tier B cases | Typical runtime |
//! |------------|--------------|--------------|-----------------|
//! | `fast`     | 400          | 100          | seconds         |
//! | `extended` | 2 000        | 1 000        | ~1 minute       |
//!
//! The default (`fast`) runs in every PR via `.github/workflows/diff-verify.yml`.
//! `extended` runs nightly (schedule trigger) and can be run manually via
//! `workflow_dispatch` or locally (see `docs/DIFF_VERIFY.md`).
//!
//! # Seed reproduction
//!
//! Every case derives its RNG stream from `(base_seed, case_index)`, so a
//! failing case is bit-for-bit reproducible:
//!
//! ```text
//! SEED=<reported_seed> cargo test --package xelma-contract --lib \
//!   tests::diff_verify -- --nocapture
//! ```

extern crate std;

use std::env;
use std::format;
use std::string::{String, ToString};
use std::vec;
use std::vec::Vec;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::settlement_math::{
    classify_price_direction, compute_deviation_bps, compute_precision_fee,
    compute_precision_payouts_with_policy, compute_updown_fee, compute_updown_winner_payout,
    find_precision_winners_with_policy, is_one_sided_pool, PrecisionEntry,
    PrecisionPayoutPolicy as EnginePayoutPolicy, PrecisionScoringMode, PrecisionScoringPolicy,
    PriceDirection, UpDownPosition,
};
use crate::types::{BetSide, DataKeyCore, FeeModel, OraclePayload};
use soroban_sdk::{
    testutils::Address as _, testutils::Ledger as _, xdr::ToXdr, Address, Bytes, BytesN, Env,
};

// ═══════════════════════════════════════════════════════════════════════════════
// § 0 — Constants and configuration
// ═══════════════════════════════════════════════════════════════════════════════

/// Basis-point denominator (mirrors `math_common::BPS_DENOMINATOR`).
const BPS_DENOM: i128 = 10_000;

/// Default base seed (fixed for deterministic CI runs).
const DEFAULT_SEED: u64 = 0xBEEF_1234;

/// Outcome classes used by the production reference model.
const CLASS_WIN: u8 = 0;
const CLASS_LOSS: u8 = 1;
const CLASS_REFUND: u8 = 2;

/// Fee incidence models mirrored from `types::FeeModel`.
const FEE_MODEL_POT: u8 = 0;
const FEE_MODEL_WINNINGS: u8 = 1;

// ═══════════════════════════════════════════════════════════════════════════════
// § 1 — Shared canonical case generator
// ═══════════════════════════════════════════════════════════════════════════════

/// One randomized oracle settlement case, shared by every tier.
///
/// `positions` drives the Up/Down mode; `precision` drives Precision mode.
/// Both are generated for every case so tier A can exercise both engines
/// with identical inputs, while tiers B/C pick the mode matching the case.
#[derive(Clone, Debug)]
struct OracleCase {
    /// RNG seed for this specific case (reproducible from `base_seed`).
    seed: u64,
    /// Case index within the run.
    case_index: u32,
    /// Round start price.
    start_price: u128,
    /// Oracle settlement price.
    final_price: u128,
    /// Protocol fee bps (`None` = fee disabled).
    fee_bps: Option<u32>,
    /// `FEE_MODEL_POT` or `FEE_MODEL_WINNINGS`.
    fee_model: u8,
    /// Precision payout policy: `true` = stake-weighted, `false` = equal.
    payout_stake_weighted: bool,
    /// Up/Down positions: `(amount, side_up)` in placement order.
    positions: Vec<(i128, bool)>,
    /// Precision entries: `(predicted_price, amount, revealed_direct)`.
    /// `revealed_direct == false` entries are placed as commit-only
    /// (unrevealed) predictions in tier B, mirroring the anti-griefing
    /// forfeit path.
    precision: Vec<(u128, i128, bool)>,
    /// Reference price for deviation-bps computation (always > 0).
    deviation_reference: u128,
}

impl OracleCase {
    /// Human-readable one-line summary of the case's round-level knobs.
    fn summary(&self) -> String {
        let mode = if self.precision.is_empty() {
            "UpDown"
        } else {
            "Precision"
        };
        format!(
            "mode={} start={} final={} fee={:?} fee_model={} payout_sw={} \
             positions={} precision={} dev_ref={}",
            mode,
            self.start_price,
            self.final_price,
            self.fee_bps,
            self.fee_model,
            self.payout_stake_weighted,
            self.positions.len(),
            self.precision.len(),
            self.deviation_reference
        )
    }

    /// Full parameter dump printed on failure so the case can be replayed
    /// by hand or turned into a fixed regression entry.
    fn dump(&self) -> String {
        format!(
            "case dump:\n  summary: {}\n  positions (amount, side_up): {:?}\n  \
             precision (predicted, amount, revealed): {:?}",
            self.summary(),
            self.positions,
            self.precision
        )
    }
}

/// Builds all cases for a run. Deterministic in `(base_seed, count)`.
fn generate_cases(base_seed: u64, count: u32) -> Vec<OracleCase> {
    let mut cases = Vec::with_capacity(count as usize);
    for i in 0..count {
        // Per-case stream: LCG-mixed so adjacent indices produce
        // well-separated streams.
        let case_seed = base_seed
            .wrapping_add(i as u64)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let mut rng = StdRng::seed_from_u64(case_seed);

        let start_price: u128 = rng.gen_range(1_000_000u128..=10_000_000_000u128);

        // 20% unchanged-price (tie/refund) cases, rest random direction.
        let final_price: u128 = if rng.gen_bool(0.2) {
            start_price
        } else {
            let delta = rng.gen_range(1u128..=start_price / 2);
            if rng.gen_bool(0.5) {
                start_price + delta
            } else {
                start_price - delta
            }
        };

        let fee_bps: Option<u32> = match rng.gen_range(0u32..=4) {
            0 => None,
            1 => Some(1),
            2 => Some(250),
            _ => Some(1_000), // protocol maximum (10%)
        };

        let fee_model: u8 = if rng.gen_bool(0.3) {
            FEE_MODEL_WINNINGS
        } else {
            FEE_MODEL_POT
        };

        let payout_stake_weighted = rng.gen_bool(0.3);

        // Up/Down: 2–8 participants, stakes sized so tier B always passes
        // every validation gate (min-bet unset, max-stake unset, mint
        // balance ample). Small losing stakes keep the thin-pool fee
        // spillover branch reachable.
        let num_positions: usize = rng.gen_range(2..=8);
        let mut positions = Vec::with_capacity(num_positions);
        for _ in 0..num_positions {
            let amount: i128 = rng.gen_range(1i128..=10_000i128);
            let side_up = rng.gen_bool(0.5);
            positions.push((amount, side_up));
        }

        // Precision: 1–8 entries; ~25% commit-only (unrevealed) so the
        // unrevealed-forfeit and all-unrevealed-refund branches are hit.
        let num_entries: usize = rng.gen_range(1..=8);
        let mut precision = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            let offset = rng.gen_range(1u128..=start_price / 2);
            let predicted = if rng.gen_bool(0.5) {
                start_price + offset
            } else {
                start_price - offset
            };
            let amount: i128 = rng.gen_range(1i128..=10_000i128);
            let revealed_direct = rng.gen_bool(0.75);
            precision.push((predicted, amount, revealed_direct));
        }

        let deviation_reference: u128 = rng.gen_range(1u128..=10_000_000_000u128);

        cases.push(OracleCase {
            seed: case_seed,
            case_index: i,
            start_price,
            final_price,
            fee_bps,
            fee_model,
            payout_stake_weighted,
            positions,
            precision,
            deviation_reference,
        });
    }
    cases
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 2 — Tier A reference model (mirrors the pure settlement_math engine)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Independent re-implementation of the audited pure engine, written in a
// deliberately different style, so a systematic copy-paste bug cannot hide.

fn ref_a_classify(start: u128, final_: u128) -> u8 {
    match final_.cmp(&start) {
        std::cmp::Ordering::Greater => 0,
        std::cmp::Ordering::Less => 1,
        std::cmp::Ordering::Equal => 2,
    }
}

fn ref_a_one_sided(pool_up: i128, pool_down: i128) -> bool {
    (pool_up == 0) != (pool_down == 0)
}

/// Mirrors `settlement_math::compute_updown_fee` (pot model, spillover).
fn ref_a_updown_fee(winning: i128, losing: i128, fee_bps: Option<u32>) -> (i128, i128, i128) {
    let Some(bps) = fee_bps else {
        return (winning, losing, 0);
    };
    let fee = (winning + losing) * (bps as i128) / BPS_DENOM;
    if fee == 0 {
        return (winning, losing, 0);
    }
    let from_losing = if fee < losing { fee } else { losing };
    let from_winning = fee - from_losing;
    (winning - from_winning, losing - from_losing, fee)
}

/// Mirrors `settlement_math::compute_precision_fee` (pot model).
fn ref_a_precision_fee(total_pot: i128, fee_bps: Option<u32>) -> (i128, i128) {
    let Some(bps) = fee_bps else {
        return (total_pot, 0);
    };
    if total_pot <= 0 {
        return (total_pot, 0);
    }
    let fee = total_pot * (bps as i128) / BPS_DENOM;
    (total_pot - fee, fee)
}

/// Mirrors `settlement_math::compute_updown_winner_payout`.
fn ref_a_winner_payout(stake: i128, winning_pool: i128, distributable: i128) -> i128 {
    if winning_pool == 0 {
        return 0;
    }
    stake * distributable / winning_pool
}

fn ref_a_precision_score(predicted: u128, final_price: u128, mode: PrecisionScoringMode) -> u128 {
    let abs_diff = if predicted >= final_price {
        predicted - final_price
    } else {
        final_price - predicted
    };
    match mode {
        PrecisionScoringMode::AbsoluteDistance => abs_diff,
        PrecisionScoringMode::RelativeDistance => {
            if final_price > 0 {
                abs_diff * 10_000 / final_price
            } else {
                abs_diff
            }
        }
    }
}

/// Mirrors `settlement_math::find_precision_winners_with_policy`.
fn ref_a_find_winners(
    entries: &[PrecisionEntry],
    final_price: u128,
    policy: &PrecisionScoringPolicy,
) -> (Vec<usize>, Vec<usize>, i128) {
    let mut total_pot: i128 = 0;
    let mut scored: Vec<(usize, u128)> = Vec::new();
    let mut best: Option<u128> = None;

    for entry in entries {
        total_pot += entry.amount;
        if !entry.revealed {
            continue;
        }
        let score = ref_a_precision_score(entry.predicted_price, final_price, policy.mode);
        scored.push((entry.index, score));
        best = Some(match best {
            None => score,
            Some(b) if score < b => score,
            Some(b) => b,
        });
    }

    let mut winners = Vec::new();
    if let Some(best) = best {
        for &(idx, score) in &scored {
            let in_band = match policy.confidence_band {
                None => score == best,
                Some(band) => score <= band || score <= best.saturating_add(band),
            };
            if in_band {
                winners.push(idx);
            }
        }
    }
    let losers: Vec<usize> = entries
        .iter()
        .filter(|e| !winners.contains(&e.index))
        .map(|e| e.index)
        .collect();
    (winners, losers, total_pot)
}

/// Mirrors `settlement_math::split_pot_among_winners` (remainder → first).
fn ref_a_split_equal(distributable: i128, count: usize) -> Vec<i128> {
    if count == 0 || distributable <= 0 {
        return Vec::new();
    }
    let per = distributable / (count as i128);
    let remainder = distributable % (count as i128);
    (0..count)
        .map(|i| if i == 0 { per + remainder } else { per })
        .collect()
}

/// Mirrors `settlement_math::split_pot_stake_weighted` (remainder → first).
fn ref_a_split_stake_weighted(distributable: i128, stakes: &[i128]) -> Vec<i128> {
    if stakes.is_empty() || distributable <= 0 {
        return Vec::new();
    }
    let total: i128 = stakes.iter().sum();
    if total == 0 {
        return ref_a_split_equal(distributable, stakes.len());
    }
    let mut payouts: Vec<i128> = stakes.iter().map(|s| s * distributable / total).collect();
    let allocated: i128 = payouts.iter().sum();
    let remainder = distributable - allocated;
    if remainder > 0 {
        payouts[0] += remainder;
    }
    payouts
}

/// Mirrors `settlement_math::compute_precision_payouts_with_policy`.
fn ref_a_precision_payouts(
    entries: &[PrecisionEntry],
    final_price: u128,
    fee_bps: Option<u32>,
    scoring: &PrecisionScoringPolicy,
    payout_policy: EnginePayoutPolicy,
) -> Vec<(i128, bool, bool)> {
    let (winner_indices, _, total_pot) = ref_a_find_winners(entries, final_price, scoring);

    if winner_indices.is_empty() && total_pot > 0 {
        return entries.iter().map(|e| (e.amount, false, true)).collect();
    }
    if total_pot <= 0 || winner_indices.is_empty() {
        return entries.iter().map(|_| (0i128, false, false)).collect();
    }

    let (distributable, _) = ref_a_precision_fee(total_pot, fee_bps);
    let winner_payouts = match payout_policy {
        EnginePayoutPolicy::Equal => ref_a_split_equal(distributable, winner_indices.len()),
        EnginePayoutPolicy::StakeWeighted => {
            let stakes: Vec<i128> = winner_indices
                .iter()
                .map(|&idx| entries[idx].amount)
                .collect();
            ref_a_split_stake_weighted(distributable, &stakes)
        }
    };

    entries
        .iter()
        .map(|e| match winner_indices.iter().position(|&i| i == e.index) {
            Some(pos) => (winner_payouts[pos], true, false),
            None => (0i128, false, false),
        })
        .collect()
}

/// Mirrors `settlement_math::compute_deviation_bps` (reference > 0).
fn ref_a_deviation_bps(price: u128, reference: u128) -> u32 {
    let diff = if price >= reference {
        price - reference
    } else {
        reference - price
    };
    (diff * 10_000 / reference) as u32
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 3 — Production reference model (mirrors settlement.rs orchestration)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Semantics sourced from `_resolve_updown_mode`, `_record_winnings_indexed`,
// `_resolve_precision_mode`, `_calculate_precision_payouts`,
// `calculate_protocol_fee_updown` and `calculate_protocol_fee_precision`.
// Notably different from the pure engine: FeeOnWinnings fee incidence,
// sorted-participant iteration order, and the unrevealed-forfeit policy.

/// Up/Down settlement across randomized pools, fee bps, both fee-incidence
/// models, and all three price directions (win, tie-refund, one-sided
/// refund, and the thin-pool fee spillover path).
/// Up/Down settlement per production semantics.
///
/// `positions` must be ordered like the contract's address-sorted
/// participant list (the tier-B/C drivers guarantee this). Returns one
/// `(payout, class)` per position in that order.
fn ref_prod_updown(
    positions: &[(i128, bool)],
    start_price: u128,
    final_price: u128,
    pool_up: i128,
    pool_down: i128,
    fee_bps: Option<u32>,
    fee_model: u8,
) -> Vec<(i128, u8)> {
    let direction = match final_price.cmp(&start_price) {
        std::cmp::Ordering::Greater => PriceDirection::Up,
        std::cmp::Ordering::Less => PriceDirection::Down,
        std::cmp::Ordering::Equal => PriceDirection::Unchanged,
    };

    // One-sided (exactly one pool empty): everything is refunded and no fee
    // is collected.
    if (pool_up == 0) != (pool_down == 0) {
        return positions.iter().map(|p| (p.0, CLASS_REFUND)).collect();
    }

    // Unchanged price: full refund for everyone, no fee.
    if direction == PriceDirection::Unchanged {
        return positions.iter().map(|p| (p.0, CLASS_REFUND)).collect();
    }

    let (winning_side_up, winning_pool, losing_pool) = match direction {
        PriceDirection::Up => (true, pool_up, pool_down),
        PriceDirection::Down => (false, pool_down, pool_up),
        PriceDirection::Unchanged => unreachable!(),
    };

    // No winning-side liquidity: nothing is distributed at all (neither
    // payouts nor refunds) — mirrors `_record_winnings_indexed`'s early
    // return.
    if winning_pool == 0 {
        return positions.iter().map(|_| (0i128, CLASS_LOSS)).collect();
    }

    // Fee split per incidence model (mirrors `calculate_protocol_fee_updown`).
    let (dist_winning, dist_losing, fee_amount) = match fee_bps {
        None => (winning_pool, losing_pool, 0),
        Some(bps) => {
            let fee_amount = match fee_model {
                FEE_MODEL_WINNINGS => losing_pool * (bps as i128) / BPS_DENOM,
                _ => (winning_pool + losing_pool) * (bps as i128) / BPS_DENOM,
            };
            if fee_amount == 0 {
                (winning_pool, losing_pool, 0)
            } else if fee_model == FEE_MODEL_WINNINGS {
                (winning_pool, losing_pool - fee_amount, fee_amount)
            } else {
                let from_losing = if fee_amount < losing_pool {
                    fee_amount
                } else {
                    losing_pool
                };
                let from_winning = fee_amount - from_losing;
                (
                    winning_pool - from_winning,
                    losing_pool - from_losing,
                    fee_amount,
                )
            }
        }
    };

    // Winners receive a proportional share of ALL distributable funds
    // (mirrors `_record_winnings_indexed`: multiply first, then floor-divide
    // by the ORIGINAL winning pool — this is what makes fee spillover land
    // on winners instead of leaking).
    let total_distributable = dist_winning + dist_losing;
    positions
        .iter()
        .map(|p| {
            if p.1 == winning_side_up {
                (p.0 * total_distributable / winning_pool, CLASS_WIN)
            } else {
                (0i128, CLASS_LOSS)
            }
        })
        .collect()
}

/// Precision settlement per production semantics.
///
/// `entries` must be in the contract's address-sorted participant order.
/// Returns `(fee_amount, per-entry (payout, class))`.
fn ref_prod_precision(
    entries: &[(u128, i128, bool)],
    final_price: u128,
    fee_bps: Option<u32>,
    fee_model: u8,
    stake_weighted: bool,
) -> (i128, Vec<(i128, u8)>) {
    let total_pot: i128 = entries.iter().map(|e| e.1).sum();

    // Closest-revealed winner determination in participant order: strictly
    // better resets the winner list, equal appends (mirrors
    // `_resolve_precision_mode`).
    let mut min_diff: Option<u128> = None;
    let mut winner_indices: Vec<usize> = Vec::new();
    for (idx, entry) in entries.iter().enumerate() {
        if !entry.2 {
            continue; // unrevealed commitments never win
        }
        let diff = if entry.0 >= final_price {
            entry.0 - final_price
        } else {
            final_price - entry.0
        };
        match min_diff {
            None => {
                min_diff = Some(diff);
                winner_indices = vec![idx];
            }
            Some(cur) if diff < cur => {
                min_diff = Some(diff);
                winner_indices = vec![idx];
            }
            Some(cur) if diff == cur => winner_indices.push(idx),
            Some(_) => {}
        }
    }

    // Nobody revealed: every stake is refunded, no fee (conservation path).
    if winner_indices.is_empty() {
        let payouts = entries
            .iter()
            .map(|e| (e.1, CLASS_REFUND))
            .collect();
        return (0, payouts);
    }

    // Fee per incidence model (mirrors `calculate_protocol_fee_precision`):
    // FeeOnWinnings taxes only the net profit (pot − winner stakes).
    let winner_stakes: i128 = winner_indices.iter().map(|&i| entries[i].1).sum();
    let (distributable, fee_amount) = match fee_bps {
        None => (total_pot, 0),
        Some(bps) => {
            let taxable = match fee_model {
                FEE_MODEL_WINNINGS => {
                    let profit = total_pot - winner_stakes;
                    if profit <= 0 {
                        // No profit → no fee.
                        return (0, {
                            let mut payouts: Vec<(i128, u8)> =
                                entries.iter().map(|_| (0i128, CLASS_LOSS)).collect();
                            for &w in &winner_indices {
                                payouts[w] = (entries[w].1, CLASS_WIN);
                            }
                            payouts
                        });
                    }
                    profit
                }
                _ => total_pot,
            };
            let fee = taxable * (bps as i128) / BPS_DENOM;
            if fee == 0 {
                (total_pot, 0)
            } else {
                (total_pot - fee, fee)
            }
        }
    };

    // Split distributable among winners (mirrors `_calculate_precision_payouts`):
    // Equal → floor share each, remainder to winners[0]; StakeWeighted →
    // proportional, remainder to winners[0]. Winners are iterated in
    // participant (address-sorted) order in both the contract and here.
    let mut winner_payouts: Vec<i128> = if !stake_weighted {
        let count = winner_indices.len() as i128;
        let per = if count > 0 { distributable / count } else { 0 };
        vec![per; winner_indices.len()]
    } else if winner_stakes > 0 {
        winner_indices
            .iter()
            .map(|&i| entries[i].1 * distributable / winner_stakes)
            .collect()
    } else {
        vec![0; winner_indices.len()]
    };
    if !winner_payouts.is_empty() {
        let allocated: i128 = winner_payouts.iter().sum();
        let remainder = distributable - allocated;
        winner_payouts[0] += remainder;
    }

    let mut payouts: Vec<(i128, u8)> = entries.iter().map(|_| (0i128, CLASS_LOSS)).collect();
    for (pos, &w) in winner_indices.iter().enumerate() {
        payouts[w] = (winner_payouts[pos], CLASS_WIN);
    }
    (fee_amount, payouts)
}

/// Oracle deviation per production semantics (`compute_deviation_bps`
/// rejects a zero reference; the generator guarantees `reference > 0`).
fn ref_prod_deviation(price: u128, reference: u128) -> u32 {
    let diff = if price >= reference {
        price - reference
    } else {
        reference - price
    };
    (diff * 10_000 / reference) as u32
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 4 — Diagnostics: mismatch reports, case dumps, minimization
// ═══════════════════════════════════════════════════════════════════════════════

/// Formats a structured mismatch report with everything needed to replay.
fn mismatch_report(
    tier: &str,
    case: &OracleCase,
    field: &str,
    got: String,
    expected: String,
) -> String {
    format!(
        "\n\
         ═══════════════════ DIFF VERIFY MISMATCH ═══════════════════\n\
         Tier:      {tier}\n\
         Case:      case#{idx} seed={seed}\n\
         Field:     {field}\n\
         Got:       {got}\n\
         Expected:  {expected}\n\
         {dump}\n\
         Reproduce:\n\
         \x20 SEED={seed} cargo test --package xelma-contract --lib tests::diff_verify -- --nocapture\n\
         ════════════════════════════════════════════════════════════",
        tier = tier,
        idx = case.case_index,
        seed = case.seed,
        field = field,
        got = got,
        expected = expected,
        dump = case.dump(),
    )
}

/// Shrinks a failing tier-A case toward a minimal reproducer. Each step is
/// kept only if the mismatch still reproduces; the original case and seed
/// are always reported alongside the minimized one.
fn minimise_tier_a(original: &OracleCase, check: &dyn Fn(&OracleCase) -> Option<String>) -> OracleCase {
    let mut best = original.clone();
    // Halve prices and pools.
    let mut reduced = best.clone();
    reduced.start_price /= 2;
    reduced.final_price /= 2;
    reduced.deviation_reference /= 2;
    if check(&reduced).is_some() {
        best = reduced;
    }
    // Remove fee.
    let mut reduced = best.clone();
    reduced.fee_bps = None;
    if check(&reduced).is_some() {
        best = reduced;
    }
    // Tie the price.
    let mut reduced = best.clone();
    reduced.final_price = reduced.start_price;
    if check(&reduced).is_some() {
        best = reduced;
    }
    // Drop half the Up/Down positions (recomputing pools is unnecessary at
    // the engine tier: pools are passed explicitly).
    if best.positions.len() > 2 {
        let mut reduced = best.clone();
        let keep = reduced.positions.len() / 2;
        reduced.positions.truncate(keep);
        if check(&reduced).is_some() {
            best = reduced;
        }
    }
    best
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 5 — Tier A executor: settlement_math engine vs reference
// ═══════════════════════════════════════════════════════════════════════════════

/// Executes one case against the pure engine and its reference model.
fn execute_tier_a_case(case: &OracleCase) -> Option<String> {
    let tier = "A (settlement_math engine)";

    // 5a. Price direction.
    let c_dir = classify_price_direction(case.start_price, case.final_price);
    let r_dir = ref_a_classify(case.start_price, case.final_price);
    if c_dir as u8 != r_dir {
        return Some(mismatch_report(
            tier,
            case,
            "classify_price_direction",
            format!("{:?}", c_dir),
            format!("{}", r_dir),
        ));
    }

    // 5b. One-sided pool.
    let c_1s = is_one_sided_pool(case.positions.iter().map(|p| p.0 * 0 + 0).sum::<i128>(), 0);
    let _ = c_1s; // pools are derived below; keep the call-site shape simple
    let pool_up: i128 = case.positions.iter().filter(|p| p.1).map(|p| p.0).sum();
    let pool_down: i128 = case.positions.iter().filter(|p| !p.1).map(|p| p.0).sum();
    let c_1s = is_one_sided_pool(pool_up, pool_down);
    let r_1s = ref_a_one_sided(pool_up, pool_down);
    if c_1s != r_1s {
        return Some(mismatch_report(
            tier,
            case,
            "is_one_sided_pool",
            format!("{}", c_1s),
            format!("{}", r_1s),
        ));
    }

    // 5c. Up/Down fee split (both pool orientations).
    for (label, w, l) in [
        ("updown_fee(up,down)", pool_up, pool_down),
        ("updown_fee(down,up)", pool_down, pool_up),
    ] {
        let (c_w, c_l, c_fee) = match compute_updown_fee(w, l, case.fee_bps) {
            Ok(v) => v,
            Err(e) => {
                return Some(mismatch_report(
                    tier,
                    case,
                    label,
                    format!("Err({:?})", e),
                    "Ok((..))".to_string(),
                ))
            }
        };
        let (r_w, r_l, r_fee) = ref_a_updown_fee(w, l, case.fee_bps);
        if (c_w, c_l, c_fee) != (r_w, r_l, r_fee) {
            return Some(mismatch_report(
                tier,
                case,
                label,
                format!("({}, {}, {})", c_w, c_l, c_fee),
                format!("({}, {}, {})", r_w, r_l, r_fee),
            ));
        }
    }

    // 5d. Precision fee.
    let total_pot: i128 = case.precision.iter().map(|e| e.1).sum();
    let (c_dist, c_fee) = match compute_precision_fee(total_pot, case.fee_bps) {
        Ok(v) => v,
        Err(e) => {
            return Some(mismatch_report(
                tier,
                case,
                "precision_fee",
                format!("Err({:?})", e),
                "Ok((..))".to_string(),
            ))
        }
    };
    let (r_dist, r_fee) = ref_a_precision_fee(total_pot, case.fee_bps);
    if (c_dist, c_fee) != (r_dist, r_fee) {
        return Some(mismatch_report(
            tier,
            case,
            "precision_fee",
            format!("({}, {})", c_dist, c_fee),
            format!("({}, {})", r_dist, r_fee),
        ));
    }

    // 5e. Winner payout primitive.
    if pool_up > 0 {
        let stake = case.positions.first().map(|p| p.0).unwrap_or(1);
        let c_pay = match compute_updown_winner_payout(stake, pool_up, pool_up + pool_down) {
            Ok(v) => v,
            Err(e) => {
                return Some(mismatch_report(
                    tier,
                    case,
                    "updown_winner_payout",
                    format!("Err({:?})", e),
                    "Ok(..)".to_string(),
                ))
            }
        };
        let r_pay = ref_a_winner_payout(stake, pool_up, pool_up + pool_down);
        if c_pay != r_pay {
            return Some(mismatch_report(
                tier,
                case,
                "updown_winner_payout",
                format!("{}", c_pay),
                format!("{}", r_pay),
            ));
        }
    }

    // 5f. Precision winner determination + full payout vector, for every
    // scoring/payout policy combination.
    let engine_entries: Vec<PrecisionEntry> = case
        .precision
        .iter()
        .enumerate()
        .map(|(i, (pred, amt, rev))| PrecisionEntry {
            index: i,
            predicted_price: *pred,
            amount: *amt,
            revealed: *rev,
        })
        .collect();

    for mode in [
        PrecisionScoringMode::AbsoluteDistance,
        PrecisionScoringMode::RelativeDistance,
    ] {
        for band in [None, Some(250u128)] {
            let policy = PrecisionScoringPolicy {
                mode,
                confidence_band: band,
            };
            let c_winners = find_precision_winners_with_policy(&engine_entries, case.final_price, policy);
            let (r_winners, r_losers, r_pot) = ref_a_find_winners(&engine_entries, case.final_price, &policy);
            if c_winners.total_pot != r_pot {
                return Some(mismatch_report(
                    tier,
                    case,
                    "precision_winners.total_pot",
                    format!("{}", c_winners.total_pot),
                    format!("{}", r_pot),
                ));
            }
            if c_winners.winner_indices != r_winners {
                return Some(mismatch_report(
                    tier,
                    case,
                    "precision_winners.winner_indices",
                    format!("{:?}", c_winners.winner_indices),
                    format!("{:?} (band={:?}, mode={:?})", r_winners, band, mode),
                ));
            }
            if c_winners.loser_indices != r_losers {
                return Some(mismatch_report(
                    tier,
                    case,
                    "precision_winners.loser_indices",
                    format!("{:?}", c_winners.loser_indices),
                    format!("{:?}", r_losers),
                ));
            }

            for payout_policy in [EnginePayoutPolicy::Equal, EnginePayoutPolicy::StakeWeighted] {
                let c_vec = match compute_precision_payouts_with_policy(
                    &engine_entries,
                    case.final_price,
                    case.fee_bps,
                    policy,
                    payout_policy,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        return Some(mismatch_report(
                            tier,
                            case,
                            "precision_payouts_with_policy",
                            format!("Err({:?})", e),
                            "Ok(vec)".to_string(),
                        ))
                    }
                };
                let r_vec = ref_a_precision_payouts(
                    &engine_entries,
                    case.final_price,
                    case.fee_bps,
                    &policy,
                    payout_policy,
                );
                for (i, (c, r)) in c_vec.iter().zip(r_vec.iter()).enumerate() {
                    if (c.payout, c.is_winner, c.is_refund) != (*r, r.1, r.2) {
                        return Some(mismatch_report(
                            tier,
                            case,
                            &format!("precision_payouts[{}] (policy band={:?} sw={})", i, band, payout_policy as u8),
                            format!("({}, {}, {})", c.payout, c.is_winner, c.is_refund),
                            format!("({}, {}, {})", r.0, r.1, r.2),
                        ));
                    }
                }
            }
        }
    }

    // 5g. Deviation bps (reference is guaranteed > 0 by the generator).
    let c_dev = match compute_deviation_bps(case.final_price, case.deviation_reference) {
        Ok(v) => v,
        Err(e) => {
            return Some(mismatch_report(
                tier,
                case,
                "deviation_bps",
                format!("Err({:?})", e),
                format!("{}", ref_a_deviation_bps(case.final_price, case.deviation_reference)),
            ))
        }
    };
    let r_dev = ref_a_deviation_bps(case.final_price, case.deviation_reference);
    if c_dev != r_dev {
        return Some(mismatch_report(
            tier,
            case,
            "deviation_bps",
            format!("{}", c_dev),
            format!("{}", r_dev),
        ));
    }

    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 6 — Tier B/C shared driver: full contract settlement vs reference
// ═══════════════════════════════════════════════════════════════════════════════

/// Configures a freshly-initialized contract on `env` (native registration).
fn setup_contract_on_env(env: &Env) -> (Address, VirtualTokenContractClient<'_>) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    (contract_id, client)
}

/// Writes the protocol fee bps directly into storage, bypassing the
/// timelocked scheduler (mirrors the convention in `tests/conservation.rs`).
fn set_fee_bps_now(env: &Env, contract_id: &Address, bps: u32) {
    env.as_contract(contract_id, || {
        env.storage().persistent().set(&DataKeyCore::ProtocolFeeBps, &bps);
    });
}

/// Writes the fee incidence model directly into storage.
fn set_fee_model_now(env: &Env, contract_id: &Address, model: FeeModel) {
    env.as_contract(contract_id, || {
        env.storage().persistent().set(&DataKeyCore::FeeModel, &model);
    });
}

/// `sha256(price.to_xdr() || salt.to_xdr())` — matches `reveal_prediction`.
fn make_commitment(env: &Env, price: u128, salt: &BytesN<32>) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&price.to_xdr(env));
    preimage.append(&salt.clone().to_xdr(env));
    env.crypto().sha256(&preimage).into()
}

/// Salt satisfying the on-chain minimum-entropy checks on reveal.
fn test_salt(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        bytes[i] = seed.wrapping_add(i as u8).wrapping_mul(17).wrapping_add(3);
        i += 1;
    }
    bytes[0] = seed | 0x80;
    bytes[31] = seed ^ 0x5A;
    BytesN::from_array(env, &bytes)
}

/// A single stake in a tier-B round with its owning participant address.
#[derive(Clone)]
struct TierBStake {
    user: Address,
    amount: i128,
    /// Up/Down: `true` = Up. Unused for Precision stakes.
    side_up: bool,
    /// Precision predicted price (0 for Up/Down stakes).
    predicted: u128,
    /// Whether this stake is a direct (revealed) precision prediction.
    revealed_direct: bool,
}

/// Drives one full round on the given environment (native or WASM) and
/// returns the observed outcome: per-stake `(stake, pending delta, outcome
/// class)`, treasury delta, and total pot.
///
/// Used unchanged by tier B (native registration) and tier C (WASM
/// registration) so both are exercised through exactly the same call
/// sequence.
fn drive_round_on_env(
    env: &Env,
    contract_id: &Address,
    client: &VirtualTokenContractClient<'_>,
    case: &OracleCase,
    tier: &str,
) -> Result<(Vec<TierBStake>, Vec<(i128, i128, u8)>, i128, i128), String> {
    let is_precision = !case.precision.is_empty();
    let stake_count = if is_precision {
        case.precision.len()
    } else {
        case.positions.len()
    };

    // Mint and generate one user per stake, in placement order.
    let mut stakes: Vec<TierBStake> = Vec::with_capacity(stake_count);
    for i in 0..stake_count {
        let user = Address::generate(env);
        client.mint_initial(&user);
        let (amount, side_up, predicted, revealed_direct) = if is_precision {
            let (predicted, amount, revealed_direct) = case.precision[i];
            (amount, true, predicted, revealed_direct)
        } else {
            let (amount, side_up) = case.positions[i];
            (amount, side_up, 0u128, true)
        };
        stakes.push(TierBStake {
            user,
            amount,
            side_up,
            predicted,
            revealed_direct,
        });
    }

    if let Some(bps) = case.fee_bps {
        set_fee_bps_now(env, contract_id, bps);
    }
    if case.fee_model == FEE_MODEL_WINNINGS {
        set_fee_model_now(env, contract_id, FeeModel::FeeOnWinnings);
    }

    client.create_round(&case.start_price, &if is_precision { Some(1u32) } else { None });
    if is_precision {
        let policy: u32 = if case.payout_stake_weighted { 1 } else { 0 };
        client.set_precision_payout_policy(&policy);
    }

    // Place stakes in generation order. The contract resolves in
    // address-sorted order, which the reference model replicates from the
    // actual addresses, so placement order only fixes which user holds
    // which stake — not the settlement math.
    for s in &stakes {
        if !is_precision {
            let side = if s.side_up { BetSide::Up } else { BetSide::Down };
            client.place_bet(&s.user, &s.amount, &side);
        } else if s.revealed_direct {
            client.place_precision_prediction(&s.user, &s.amount, &s.predicted);
        } else {
            let salt = test_salt(env, ((s.amount % 251) as u8).max(1));
            let commitment = make_commitment(env, s.predicted, &salt);
            client.commit_prediction(&s.user, &commitment, &s.amount);
            // Deliberately never revealed: exercises the anti-griefing
            // forfeit path and the all-unrevealed refund path.
        }
    }

    let round = client
        .get_active_round()
        .unwrap_or_else(|| panic!("{}: active round missing after creation", tier));
    let treasury_before = client.get_protocol_fee_treasury();

    env.ledger().with_mut(|li| li.sequence_number = 12);

    client.resolve_round(&OraclePayload {
        price: case.final_price,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    // Per-stake outcome read back via the archive API — keyed by
    // (round_id, user), independent of iteration order.
    let round_id = round.round_id;
    let mut observed: Vec<(i128, i128, u8)> = Vec::with_capacity(stakes.len());
    for s in &stakes {
        let outcome = client
            .get_user_archived_participation(&s.user, &round_id)
            .unwrap_or_else(|| {
                panic!(
                    "{}: missing archived outcome for round {}",
                    tier, round_id
                )
            });
        observed.push((s.amount, outcome.payout, outcome.outcome as u8));
    }

    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
    let total_pot = if is_precision {
        case.precision.iter().map(|e| e.1).sum()
    } else {
        case.positions.iter().map(|p| p.0).sum()
    };
    Ok((stakes, observed, treasury_delta, total_pot))
}

/// Builds the tier-B/C reference expectation for a case.
///
/// The reference is evaluated over address-sorted (stake, side/price, flag)
/// tuples — replicating the contract's `sort_addresses(participants)`
/// resolution order, using the *actual* participant addresses the contract
/// driver generated — and the per-winner results are permuted back into
/// placement order, so the comparison to observed results is strictly
/// per-stake (index-aligned with the stake list).
fn reference_expectation(stakes: &[TierBStake], case: &OracleCase) -> (Vec<(i128, u8)>, i128) {
    let is_precision = !case.precision.is_empty();
    let stake_count = stakes.len();

    // Sort stake indices by their real address — mirrors `sort_addresses`.
    let mut order: Vec<usize> = (0..stake_count).collect();
    order.sort_by(|a, b| stakes[*a].user.cmp(&stakes[*b].user));

    let mut expected_by_stake: Vec<(i128, u8)> = vec![(0, CLASS_LOSS); stake_count];

    let fee_amount;
    if !is_precision {
        let ordered: Vec<(i128, bool)> = order
            .iter()
            .map(|&i| (stakes[i].amount, stakes[i].side_up))
            .collect();
        let pool_up: i128 = ordered.iter().filter(|p| p.1).map(|p| p.0).sum();
        let pool_down: i128 = ordered.iter().filter(|p| !p.1).map(|p| p.0).sum();
        let out = ref_prod_updown(
            &ordered,
            case.start_price,
            case.final_price,
            pool_up,
            pool_down,
            case.fee_bps,
            case.fee_model,
        );
        for (slot, res) in order.iter().zip(out.iter()) {
            expected_by_stake[*slot] = *res;
        }
        let paid: i128 = out.iter().map(|o| o.0).sum();
        let pot: i128 = ordered.iter().map(|p| p.0).sum();
        fee_amount = pot - paid;
    } else {
        let ordered: Vec<(u128, i128, bool)> = order
            .iter()
            .map(|&i| (stakes[i].predicted, stakes[i].amount, stakes[i].revealed_direct))
            .collect();
        let (fee, out) = ref_prod_precision(
            &ordered,
            case.final_price,
            case.fee_bps,
            case.fee_model,
            case.payout_stake_weighted,
        );
        for (slot, res) in order.iter().zip(out.iter()) {
            expected_by_stake[*slot] = *res;
        }
        fee_amount = fee;
    }

    (expected_by_stake, fee_amount)
}

/// Compares observed tier-B/C results against the reference expectation,
/// strictly per stake (index-aligned).
fn compare_tier_b_results(
    tier: &str,
    case: &OracleCase,
    observed: (Vec<(i128, i128, u8)>, i128, i128),
    expected: (Vec<(i128, u8)>, i128),
) -> Option<String> {
    let (observed_stakes, treasury_delta, total_pot) = observed;
    let (expected_by_stake, expected_fee) = expected;

    if observed_stakes.len() != expected_by_stake.len() {
        let report = mismatch_report(
            tier,
            case,
            "participant_count",
            format!("contract archived {} outcomes", observed_stakes.len()),
            format!("reference expects {} outcomes", expected_by_stake.len()),
        );
        return Some(report);
    }

    for (i, ((stake, payout, class), (exp_payout, exp_class))) in
        observed_stakes.iter().zip(expected_by_stake.iter()).enumerate()
    {
        if payout != exp_payout {
            return Some(mismatch_report(
                tier,
                case,
                &format!("stake[{}] payout (stake={})", i, stake),
                format!("{}", payout),
                format!("{}", exp_payout),
            ));
        }
        if class != *exp_class {
            return Some(mismatch_report(
                tier,
                case,
                &format!(
                    "stake[{}] outcome class (stake={}, payout={})",
                    i, stake, payout
                ),
                format!("{}", class),
                format!("{}", exp_class),
            ));
        }
    }

    if treasury_delta != expected_fee {
        return Some(mismatch_report(
            tier,
            case,
            "protocol_fee_treasury_delta",
            format!("{}", treasury_delta),
            format!("{}", expected_fee),
        ));
    }

    // Conservation: payouts + fee == pot (exact for Precision; exact for
    // Up/Down as well because the reference computes the same per-winner
    // floors the contract does).
    let paid: i128 = observed_stakes.iter().map(|o| o.1).sum();
    if paid + treasury_delta != total_pot {
        return Some(mismatch_report(
            tier,
            case,
            "conservation (payouts + fee == pot)",
            format!("{} + {}", paid, treasury_delta),
            format!("{}", total_pot),
        ));
    }

    None
}

/// Runs one tier-B case end to end on a fresh native environment.
fn run_tier_b_case(case: &OracleCase) -> Option<String> {
    let env = Env::default();
    let (contract_id, client) = setup_contract_on_env(&env);
    let tier = "B (native contract)";
    let (stakes, observed, _treasury, _pot) =
        match drive_round_on_env(&env, &contract_id, &client, case, tier) {
            Ok(v) => v,
            Err(e) => return Some(e),
        };
    let expected = reference_expectation(&stakes, case);
    compare_tier_b_results(tier, case, (observed, _treasury, _pot), expected)
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 7 — Tier C: differential against the compiled WASM binary
// ═══════════════════════════════════════════════════════════════════════════════

/// The canonical tier-C case: a competitive 8-participant Up/Down round with
/// an uneven winning split (exercises per-winner floor division) and the
/// maximum 10% fee (exercises the spillover distribution), resolved with a
/// 2× price move.
fn tier_c_case() -> OracleCase {
    OracleCase {
        seed: 0xDEAD_C0DE,
        case_index: 0,
        start_price: 1_000_000_000,
        final_price: 2_000_000_000,
        fee_bps: Some(1_000),
        fee_model: FEE_MODEL_POT,
        payout_stake_weighted: false,
        positions: vec![
            (7, true),
            (13, true),
            (29, true),
            (41, true),
            (3, false),
            (17, false),
            (53, false),
            (2, false),
        ],
        precision: vec![],
        deviation_reference: 1_000_000_000,
    }
}

/// Fails loudly (never silently passes) when the WASM binary is absent.
fn load_wasm_bytes() -> Vec<u8> {
    let path = match env::var("WASM_PATH") {
        Ok(p) if !p.is_empty() => p,
        _ => panic!(
            "\n\
             ═══════════════ DIFF VERIFY: WASM PATH MISSING ═══════════════\n\
             Tier C compares the compiled contract WASM against the native\n\
             contract and the reference model. Set WASM_PATH to the built\n\
             contract binary, e.g.:\n\n\
             \x20 cargo rustc --manifest-path=contracts/Cargo.toml --crate-type=cdylib \\\n\
             \x20   --target=wasm32v1-none --release --locked\n\
             \x20 WASM_PATH=target/wasm32v1-none/release/xelma_contract.wasm \\\n\
             \x20   cargo test --package xelma-contract --lib \\\n\
             \x20   tests::diff_verify::differential_tier_c_wasm_binary -- --nocapture\n\n\
             CI runs this automatically in the diff-verify workflow.\n\
             ════════════════════════════════════════════════════════════"
        ),
    };
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("tier C: cannot read WASM at {}: {}", path, e))
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 8 — Fixed regression cases (always run, every mode)
// ═══════════════════════════════════════════════════════════════════════════════

/// Historically tricky shapes, each targeting one branch of the settlement
/// engine. Expected values for the tier-B entries are cross-checked against
/// the pinned conservation matrix in `tests/conservation.rs`.
fn fixed_cases() -> Vec<OracleCase> {
    fn base(seed: u64, idx: u32, desc_roles: ()) -> OracleCase {
        let _ = desc_roles;
        OracleCase {
            seed,
            case_index: idx,
            start_price: 1_000_000,
            final_price: 2_000_000,
            fee_bps: None,
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: false,
            positions: vec![(60, true), (40, true), (100, false)],
            precision: vec![],
            deviation_reference: 1_000_000,
        }
    }

    let mut cases = vec![
        // Thin losing pool with 10% fee — spillover path.
        OracleCase {
            seed: 0xDEAD_0001,
            case_index: 900,
            start_price: 1_000_000,
            final_price: 2_000_000,
            fee_bps: Some(1_000),
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: false,
            positions: vec![(500, true), (500, true), (10, false)],
            precision: vec![],
            deviation_reference: 1_000_000,
        },
        // Tie with fee configured — refund path must ignore the fee.
        OracleCase {
            seed: 0xDEAD_0002,
            case_index: 901,
            start_price: 1_000_000,
            final_price: 1_000_000,
            fee_bps: Some(1_000),
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: false,
            positions: vec![(100, true), (200, false)],
            precision: vec![],
            deviation_reference: 1_000_000,
        },
        // One-sided pool — refund regardless of direction.
        OracleCase {
            seed: 0xDEAD_0003,
            case_index: 902,
            start_price: 1_000_000,
            final_price: 2_000_000,
            fee_bps: Some(500),
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: false,
            positions: vec![(300, true), (200, true)],
            precision: vec![],
            deviation_reference: 1_000_000,
        },
        // FeeOnWinnings incidence — only profit is taxed.
        OracleCase {
            seed: 0xDEAD_0004,
            case_index: 903,
            start_price: 1_000_000,
            final_price: 2_000_000,
            fee_bps: Some(1_000),
            fee_model: FEE_MODEL_WINNINGS,
            payout_stake_weighted: false,
            positions: vec![(60, true), (40, true), (100, false)],
            precision: vec![],
            deviation_reference: 1_000_000,
        },
        // Precision: 3-way tie, equal split, remainder to first winner.
        OracleCase {
            seed: 0xDEAD_0005,
            case_index: 904,
            start_price: 1_000_000,
            final_price: 1_000_000,
            fee_bps: None,
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: false,
            positions: vec![],
            precision: vec![
                (1_005_000, 50, true),
                (995_000, 50, true),
                (1_200_000, 50, true),
            ],
            deviation_reference: 1_000_000,
        },
        // Precision: stake-weighted tie.
        OracleCase {
            seed: 0xDEAD_0006,
            case_index: 905,
            start_price: 1_000_000,
            final_price: 1_000_000,
            fee_bps: None,
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: true,
            positions: vec![],
            precision: vec![(1_000_000, 30, true), (1_000_000, 70, true)],
            deviation_reference: 1_000_000,
        },
        // Precision: all-unrevealed → full refund, no fee.
        OracleCase {
            seed: 0xDEAD_0007,
            case_index: 906,
            start_price: 1_000_000,
            final_price: 1_500_000,
            fee_bps: Some(500),
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: false,
            positions: vec![],
            precision: vec![(900_000, 100, false), (1_100_000, 200, false)],
            deviation_reference: 1_000_000,
        },
        // Precision: mixed reveal — unrevealed stake forfeits to the pot.
        OracleCase {
            seed: 0xDEAD_0008,
            case_index: 907,
            start_price: 1_000_000,
            final_price: 1_005_000,
            fee_bps: Some(200),
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: false,
            positions: vec![],
            precision: vec![
                (1_000_000, 50, true),
                (1_010_000, 30, true),
                (1_500_000, 20, false),
            ],
            deviation_reference: 1_000_000,
        },
        // Deviation boundary: exactly 500 bps.
        OracleCase {
            seed: 0xDEAD_0009,
            case_index: 908,
            start_price: 1_000_000,
            final_price: 1_050_000,
            fee_bps: None,
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: false,
            positions: vec![(10, true), (10, false)],
            precision: vec![],
            deviation_reference: 1_000_000,
        },
    ];
    cases.push(base(0xDEAD_000A, 909, ()));
    cases
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 9 — Test entrypoints
// ═══════════════════════════════════════════════════════════════════════════════

fn mode_case_counts() -> (u32, u32) {
    match env::var("DIFF_VERIFY_MODE").unwrap_or_else(|_| "fast".to_string()).as_str() {
        "extended" => (2_000, 1_000),
        _ => (400, 100),
    }
}

fn base_seed() -> u64 {
    env::var("SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SEED)
}

fn run_all_or_fail(tier: &str, results: Vec<Option<String>>, total: usize) {
    let failures: Vec<String> = results.into_iter().flatten().collect();
    if !failures.is_empty() {
        let report = failures.join("\n\n");
        panic!(
            "\n\
             ═══════════════ DIFF VERIFY: {} FAILURES ═══════════════\n\
             {} case(s) executed, {} FAILED\n\n\
             {}\n\
             ════════════════════════════════════════════════════════════",
            tier,
            total,
            failures.len(),
            report,
        );
    }
}

/// Tier A — pure engine differential: fixed regressions + randomized cases.
#[test]
fn differential_tier_a_pure_math() {
    let (a_count, _) = mode_case_counts();
    let seed = base_seed();

    // Fixed regressions first.
    let mut results: Vec<Option<String>> = fixed_cases()
        .iter()
        .map(execute_tier_a_case)
        .collect();

    // Randomized cases, with minimization on failure.
    let cases = generate_cases(seed, a_count);
    for case in &cases {
        if let Some(diag) = execute_tier_a_case(case) {
            std::eprintln!(
                "tier A mismatch at case#{} (seed={}); minimizing...",
                case.case_index, case.seed
            );
            let minimized = minimise_tier_a(case, &|c| execute_tier_a_case(c));
            let mini = if minimized.seed == case.seed {
                "  (no smaller case found; original is minimal)".to_string()
            } else {
                match execute_tier_a_case(&minimized) {
                    Some(d) => format!("  minimized case (seed={}):\n{}", minimized.seed, d),
                    None => "  (minimized case no longer reproduces)".to_string(),
                }
            };
            results.push(Some(format!("{}\n{}", diag, mini)));
        }
    }

    run_all_or_fail("TIER A (settlement_math engine)", results, a_count as usize);
}

/// Tier B — full native contract settlement vs reference model.
#[test]
fn differential_tier_b_native_contract() {
    let (_, b_count) = mode_case_counts();
    let seed = base_seed();

    // Tier-B fixed regressions: cross-validated against the pinned values
    // in tests/conservation.rs.
    let mut results: Vec<Option<String>> = vec![
        run_tier_b_case(&OracleCase {
            seed: 0xDEAD_0101,
            case_index: 950,
            start_price: 1_000,
            final_price: 2_000,
            fee_bps: Some(1_000),
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: false,
            positions: vec![(60, true), (40, true), (100, false)],
            precision: vec![],
            deviation_reference: 1_000,
        }),
        run_tier_b_case(&OracleCase {
            seed: 0xDEAD_0102,
            case_index: 951,
            start_price: 1_000,
            final_price: 1_000,
            fee_bps: Some(1_000),
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: false,
            positions: vec![],
            precision: vec![(1_005, 50, true), (995, 50, true), (1_200, 50, true)],
            deviation_reference: 1_000,
        }),
        run_tier_b_case(&OracleCase {
            seed: 0xDEAD_0103,
            case_index: 952,
            start_price: 1_000,
            final_price: 1_000,
            fee_bps: Some(500),
            fee_model: FEE_MODEL_POT,
            payout_stake_weighted: false,
            positions: vec![],
            precision: vec![(1_005, 40, false), (995, 60, false)],
            deviation_reference: 1_000,
        }),
    ];

    let cases = generate_cases(seed, b_count);
    for case in &cases {
        results.push(run_tier_b_case(case));
    }

    run_all_or_fail("TIER B (native contract)", results, b_count as usize + 3);
}

/// Tier C — differential against the compiled WASM binary (`WASM_PATH`).
///
/// The same canonical round is driven on (1) the natively registered
/// contract and (2) the deployed-shape WASM binary in the Soroban test
/// environment. Both must agree with each other AND with the reference
/// model, stroop for stroop.
#[test]
fn differential_tier_c_wasm_binary() {
    let wasm_bytes = load_wasm_bytes();

    let case = tier_c_case();

    // Native run.
    let env_native = Env::default();
    let (cid_native, client_native) = setup_contract_on_env(&env_native);
    let (stakes_native, native_obs, native_treasury, _native_pot) = drive_round_on_env(
        &env_native,
        &cid_native,
        &client_native,
        &case,
        "C (wasm vs native)",
    )
    .expect("tier C: native round failed");

    // WASM run (same case, fresh environment, WASM registration).
    let env_wasm = Env::default();
    let wasm_id = env_wasm.register(wasm_bytes.as_slice(), ());
    let client_wasm = VirtualTokenContractClient::new(&env_wasm, &wasm_id);
    let admin = Address::generate(&env_wasm);
    let oracle = Address::generate(&env_wasm);
    env_wasm.mock_all_auths();
    client_wasm.initialize(&admin, &oracle);
    client_wasm.update_oracle_heartbeat(&0u32);
    let (_stakes_wasm, wasm_obs, wasm_treasury, _wasm_pot) = drive_round_on_env(
        &env_wasm,
        &wasm_id,
        &client_wasm,
        &case,
        "C (wasm vs native)",
    )
    .expect("tier C: wasm round failed");

    // Reference expectation over the native run's actual participant
    // addresses (the WASM run is contract-behaviour-identical by the
    // check below).
    let expected = reference_expectation(&stakes_native, &case);

    // 1. Native vs WASM: stroop-exact per stake and treasury.
    if native_obs.len() != wasm_obs.len() {
        let report = mismatch_report(
            "C (wasm vs native)",
            &case,
            "participant_count",
            format!("wasm archived {} outcomes", wasm_obs.len()),
            format!("native archived {} outcomes", native_obs.len()),
        );
        panic!("{}", report);
    }
    for (i, ((s_n, p_n, c_n), (s_w, p_w, c_w))) in
        native_obs.iter().zip(wasm_obs.iter()).enumerate()
    {
        if s_n != s_w || p_n != p_w || c_n != c_w {
            let report = mismatch_report(
                "C (wasm vs native)",
                &case,
                &format!("wasm_vs_native_stake[{}]", i),
                format!("wasm=({}, {}, {})", s_w, p_w, c_w),
                format!("native=({}, {}, {})", s_n, p_n, c_n),
            );
            panic!("{}", report);
        }
    }
    if native_treasury != wasm_treasury {
        let report = mismatch_report(
            "C (wasm vs native)",
            &case,
            "wasm_vs_native_treasury",
            format!("wasm={}", wasm_treasury),
            format!("native={}", native_treasury),
        );
        panic!("{}", report);
    }

    // 2. WASM against the reference model.
    if let Some(diag) = compare_tier_b_results(
        "C (wasm/reference)",
        &case,
        (wasm_obs, wasm_treasury, _wasm_pot),
        expected,
    ) {
        panic!("{}", diag);
    }
}
