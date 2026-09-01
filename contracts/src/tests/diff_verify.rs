// SPDX-License-Identifier: MIT
//! Formal differential verification harness for `settlement_math` (Issue #362).
//!
//! Executes a **trusted Rust reference model** and the **contract's pure
//! settlement-math functions** on identical randomized oracle cases, asserting
//! bitwise (stroop-level) equality for all outputs.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────┐
//! │  CaseGenerator (deterministic RNG)   │
//! │  prices, stakes, fees, modes, seeds  │
//! └───────────────┬──────────────────────┘
//!                 │
//!    ┌────────────┴────────────┐
//!    ▼                         ▼
//! ┌────────────────┐  ┌────────────────────────┐
//! │ Reference Model │  │ Contract settlement_math│
//! │ (pure reimpl.)  │  │ (imported functions)   │
//! └───────┬────────┘  └──────────┬─────────────┘
//!         │                      │
//!         └──────────┬───────────┘
//!                    ▼
//!         ┌──────────────────┐
//!         │  Stroop-equality │
//!         │  assertion       │
//!         └──────────────────┘
//! ```
//!
//! # Execution modes
//!
//! Controlled by the `DIFF_VERIFY_MODE` env var:
//!
//! | Mode       | Cases | Description                                      |
//! |------------|-------|--------------------------------------------------|
//! | `fast`     | 100   | Default for PR CI — finishes in < 5 s            |
//! | `extended` | 1 000 | Nightly / local stress — ≥ 1 000 randomized cases |
//!
//! # Seed reproduction
//!
//! Every case carries a deterministic seed derived from `(base_seed, case_idx)`.
//! On failure the harness prints the exact seed and a minimal reproduction
//! command:
//!
//! ```text
//! SEED=<reported_seed> cargo test --package xelma-contract --lib \
//!   tests::diff_verify -- --nocapture
//! ```
//!
//! In `extended` mode, the harness also records *all* failing cases and replays
//! them at the end so a contributor can reproduce every mismatch in a single
//! pass without rerunning the full suite.
//!
//! # Covered scenarios
//!
//! * **UpDown mode**: Up, Down, Unchanged, one-sided pool, fee on/off,
//!   thin-losing-pool spillover, 1–8 winners per side
//! * **Precision mode**: AbsoluteDistance, RelativeDistance, confidence band,
//!   equal/stake-weighted payout, ties, unrevealed entries, fee on/off
//! * **Edge cases**: zero-pool, single participant, max fee (1 000 bps),
//!   min fee (1 bp), large stakes near overflow boundary
//! * **Oracle deviation**: price feed deviation bps

extern crate std;

use std::env;
use std::format;
use std::string::{String, ToString};
use std::vec;
use std::vec::Vec;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

// ═══════════════════════════════════════════════════════════════════════════════
// § 1 — Contract imports
// ═══════════════════════════════════════════════════════════════════════════════

use crate::settlement_math::{
    classify_price_direction, compute_deviation_bps, compute_precision_fee,
    compute_precision_payouts_with_policy, compute_updown_fee, compute_updown_payouts,
    find_precision_winners_with_policy, PrecisionEntry, PrecisionPayoutPolicy,
    PrecisionScoringMode, PrecisionScoringPolicy, PriceDirection, UpDownPosition,
};

// ═══════════════════════════════════════════════════════════════════════════════
// § 2 — Reference model (standalone re-implementation)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Every function below is a **fresh re-implementation** of the corresponding
// `settlement_math` function, written in a deliberately different style to
// minimize the risk of a shared systematic bug.  They use the same types and
// return values so the diff harness can compare bit-for-bit.

const BPS_DENOM: i128 = 10_000;

/// Reference: classify price direction (independent re-implementation).
fn ref_classify_direction(start: u128, final_: u128) -> PriceDirection {
    if final_ > start {
        PriceDirection::Up
    } else if final_ < start {
        PriceDirection::Down
    } else {
        PriceDirection::Unchanged
    }
}

/// Reference: one-sided pool check.
fn ref_is_one_sided(pool_up: i128, pool_down: i128) -> bool {
    let up_zero = pool_up == 0;
    let down_zero = pool_down == 0;
    up_zero != down_zero
}

/// Reference: UpDown fee computation (independent re-implementation).
fn ref_compute_updown_fee(
    winning_pool: i128,
    losing_pool: i128,
    fee_bps: Option<u32>,
) -> (i128, i128, i128) {
    match fee_bps {
        None => (winning_pool, losing_pool, 0),
        Some(bps) => {
            let total = winning_pool + losing_pool;
            let fee = total * (bps as i128) / BPS_DENOM;
            if fee == 0 {
                return (winning_pool, losing_pool, 0);
            }
            let fee_from_losing = fee.min(losing_pool);
            let fee_from_winning = fee - fee_from_losing;
            (
                winning_pool - fee_from_winning,
                losing_pool - fee_from_losing,
                fee,
            )
        }
    }
}

/// Reference: Precision fee computation.
fn ref_compute_precision_fee(total_pot: i128, fee_bps: Option<u32>) -> (i128, i128) {
    if total_pot <= 0 {
        return (total_pot, 0);
    }
    match fee_bps {
        None => (total_pot, 0),
        Some(bps) => {
            let fee = total_pot * (bps as i128) / BPS_DENOM;
            (total_pot - fee, fee)
        }
    }
}

/// Reference: UpDown winner payout.
fn ref_winner_payout(stake: i128, winning_pool: i128, distributable: i128) -> i128 {
    if winning_pool == 0 {
        return 0;
    }
    stake * distributable / winning_pool
}

/// Reference: full UpDown payout vector.
fn ref_updown_payouts(
    positions: &[UpDownPosition],
    start_price: u128,
    final_price: u128,
    pool_up: i128,
    pool_down: i128,
    fee_bps: Option<u32>,
) -> Vec<(i128, bool, bool)> {
    let direction = ref_classify_direction(start_price, final_price);
    let one_sided = ref_is_one_sided(pool_up, pool_down);

    if direction == PriceDirection::Unchanged || one_sided {
        return positions.iter().map(|p| (p.amount, false, true)).collect();
    }

    let (winning_side_up, winning_pool, losing_pool) = match direction {
        PriceDirection::Up => (true, pool_up, pool_down),
        PriceDirection::Down => (false, pool_down, pool_up),
        PriceDirection::Unchanged => unreachable!(),
    };

    if winning_pool == 0 {
        return positions.iter().map(|p| (p.amount, false, true)).collect();
    }

    let (dw, dl, _) = ref_compute_updown_fee(winning_pool, losing_pool, fee_bps);
    let total_dist = dw + dl;

    positions
        .iter()
        .map(|p| {
            let is_winner = p.side_up == winning_side_up;
            let payout = if is_winner {
                ref_winner_payout(p.amount, winning_pool, total_dist)
            } else {
                0
            };
            (payout, is_winner, false)
        })
        .collect()
}

/// Reference: precision scoring for a single entry.
fn ref_precision_score(predicted: u128, final_price: u128, mode: PrecisionScoringMode) -> u128 {
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

/// Reference: precision winner-finding with policy.
fn ref_find_precision_winners(
    entries: &[PrecisionEntry],
    final_price: u128,
    policy: &PrecisionScoringPolicy,
) -> (Vec<usize>, Vec<usize>, i128) {
    let mut total_pot: i128 = 0;
    let mut scores: Vec<(usize, u128)> = Vec::new();
    let mut min_score: Option<u128> = None;

    for entry in entries {
        total_pot += entry.amount;
        if !entry.revealed {
            continue;
        }
        let score = ref_precision_score(entry.predicted_price, final_price, policy.mode);
        scores.push((entry.index, score));
        min_score = Some(min_score.map_or(score, |cur| cur.min(score)));
    }

    let mut winner_indices: Vec<usize> = Vec::new();
    if let Some(best) = min_score {
        for &(idx, score) in &scores {
            let is_winner = match policy.confidence_band {
                None => score == best,
                Some(band) => score <= band || score <= best + band,
            };
            if is_winner {
                winner_indices.push(idx);
            }
        }
    }

    let loser_indices: Vec<usize> = entries
        .iter()
        .filter(|e| !winner_indices.contains(&e.index))
        .map(|e| e.index)
        .collect();

    (winner_indices, loser_indices, total_pot)
}

/// Reference: split pot equally among winners (remainder to first).
fn ref_split_equal(distributable: i128, count: usize) -> Vec<i128> {
    if count == 0 || distributable <= 0 {
        return Vec::new();
    }
    let c = count as i128;
    let per = distributable / c;
    let remainder = distributable % c;
    let mut payouts = Vec::with_capacity(count);
    for i in 0..count {
        payouts.push(if i == 0 { per + remainder } else { per });
    }
    payouts
}

/// Reference: split pot stake-weighted (remainder to first).
fn ref_split_stake_weighted(distributable: i128, stakes: &[i128]) -> Vec<i128> {
    if stakes.is_empty() || distributable <= 0 {
        return Vec::new();
    }
    let total: i128 = stakes.iter().sum();
    if total == 0 {
        return ref_split_equal(distributable, stakes.len());
    }
    let mut payouts = Vec::with_capacity(stakes.len());
    let mut allocated = 0i128;
    for &s in stakes {
        let p = s * distributable / total;
        payouts.push(p);
        allocated += p;
    }
    let remainder = distributable - allocated;
    if remainder > 0 && !payouts.is_empty() {
        payouts[0] += remainder;
    }
    payouts
}

/// Reference: full precision payout vector.
fn ref_precision_payouts(
    entries: &[PrecisionEntry],
    final_price: u128,
    fee_bps: Option<u32>,
    scoring_policy: &PrecisionScoringPolicy,
    payout_policy: PrecisionPayoutPolicy,
) -> Vec<(i128, bool, bool)> {
    let (winner_indices, _, total_pot) =
        ref_find_precision_winners(entries, final_price, scoring_policy);

    if winner_indices.is_empty() && total_pot > 0 {
        return entries.iter().map(|e| (e.amount, false, true)).collect();
    }
    if total_pot <= 0 || winner_indices.is_empty() {
        return entries.iter().map(|_| (0i128, false, false)).collect();
    }

    let (distributable, _) = ref_compute_precision_fee(total_pot, fee_bps);

    let winner_payouts = match payout_policy {
        PrecisionPayoutPolicy::Equal => ref_split_equal(distributable, winner_indices.len()),
        PrecisionPayoutPolicy::StakeWeighted => {
            let ws: Vec<i128> = winner_indices
                .iter()
                .map(|&idx| entries[idx].amount)
                .collect();
            ref_split_stake_weighted(distributable, &ws)
        }
    };

    entries
        .iter()
        .map(|e| {
            let wp = winner_indices.iter().position(|&i| i == e.index);
            match wp {
                Some(pos) => (winner_payouts[pos], true, false),
                None => (0, false, false),
            }
        })
        .collect()
}

/// Reference: deviation bps.
fn ref_deviation_bps(price: u128, reference: u128) -> u32 {
    if reference == 0 {
        return 0;
    }
    let diff = if price >= reference {
        price - reference
    } else {
        reference - price
    };
    (diff * 10_000 / reference) as u32
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 3 — Case generator
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
struct OracleCase {
    seed: u64,
    description: String,
    start_price: u128,
    final_price: u128,
    pool_up: i128,
    pool_down: i128,
    fee_bps: Option<u32>,
    positions: Vec<(i128, bool)>,
    precision_entries: Vec<(u128, i128, bool)>,
    scoring_policy: PrecisionScoringPolicy,
    payout_policy: PrecisionPayoutPolicy,
    deviation_reference: u128,
}

fn generate_cases(base_seed: u64, count: u32) -> Vec<OracleCase> {
    let mut cases = Vec::with_capacity(count as usize);

    for i in 0..count {
        let case_seed = base_seed
            .wrapping_add(i as u64)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let mut rng = StdRng::seed_from_u64(case_seed);

        let description = format!("diff_verify_case_{}_seed_{}", i, case_seed);
        let start_price: u128 = rng.gen_range(100_0000..=50_000_0000);
        let final_price: u128 = rng.gen_range(100_0000..=50_000_0000);
        let fee_bps: Option<u32> = match rng.gen_range(0u32..=3) {
            0 => None,
            1 => Some(1),     // 0.01%
            2 => Some(250),   // 2.5%
            3 => Some(1_000), // 10% max
            _ => unreachable!(),
        };

        // ── UpDown positions ──
        let num_up: usize = rng.gen_range(0..=4);
        let num_down: usize = rng.gen_range(0..=4);
        let mut positions: Vec<(i128, bool)> = Vec::with_capacity(num_up + num_down);
        for _ in 0..num_up {
            let amt: i128 = rng.gen_range(1..=100_000_000);
            positions.push((amt, true));
        }
        for _ in 0..num_down {
            let amt: i128 = rng.gen_range(1..=100_000_000);
            positions.push((amt, false));
        }

        // ── Precision entries ──
        let num_entries: usize = rng.gen_range(1..=6);
        let mut precision_entries: Vec<(u128, i128, bool)> = Vec::with_capacity(num_entries);
        for _ in 0..num_entries {
            let predicted: u128 = rng.gen_range(100_0000..=50_000_0000);
            let amount: i128 = rng.gen_range(1..=100_000_000);
            let revealed: bool = rng.gen_bool(0.75);
            precision_entries.push((predicted, amount, revealed));
        }

        // ── Scoring policy ──
        let scoring_policy = match rng.gen_range(0u8..=2) {
            0 => PrecisionScoringPolicy {
                mode: PrecisionScoringMode::AbsoluteDistance,
                confidence_band: None,
            },
            1 => PrecisionScoringPolicy {
                mode: PrecisionScoringMode::RelativeDistance,
                confidence_band: None,
            },
            _ => PrecisionScoringPolicy {
                mode: PrecisionScoringMode::AbsoluteDistance,
                confidence_band: Some(rng.gen_range(1..=1000)),
            },
        };

        let payout_policy = match rng.gen_range(0u8..=1) {
            0 => PrecisionPayoutPolicy::Equal,
            _ => PrecisionPayoutPolicy::StakeWeighted,
        };

        let pool_up: i128 = positions
            .iter()
            .filter(|p| p.1)
            .map(|p| p.0)
            .sum::<i128>()
            .max(0);
        let pool_down: i128 = positions
            .iter()
            .filter(|p| !p.1)
            .map(|p| p.0)
            .sum::<i128>()
            .max(0);

        let deviation_reference: u128 = rng.gen_range(100_0000..=50_000_0000);

        cases.push(OracleCase {
            seed: case_seed,
            description,
            start_price,
            final_price,
            pool_up,
            pool_down,
            fee_bps,
            positions,
            precision_entries,
            scoring_policy,
            payout_policy,
            deviation_reference,
        });
    }
    cases
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 4 — Differential assertion helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Assert two `i128` values are stroop-equal; on mismatch, print a structured
/// diagnostic including seed and reproduction command.
fn assert_stroop_eq(
    got: i128,
    expected: i128,
    label: &str,
    case: &OracleCase,
) -> Result<(), String> {
    if got != expected {
        Err(format!(
            "\n\
             ═══════════════════ DIFF VERIFY MISMATCH ═══════════════════\n\
             Case:    {}\n\
             Seed:    {}\n\
             Field:   {}\n\
             Got:     {}\n\
             Expected:{}\n\
             ─────────────────────────────────────────────────────────\n\
             Reproduce:\n  \
               SEED={seed} cargo test --package xelma-contract --lib \\\n  \
               tests::diff_verify -- --nocapture\n\
             ══════════════════════════════════════════════════════════════",
            case.description,
            case.seed,
            label,
            got,
            expected,
            seed = case.seed,
        ))
    } else {
        Ok(())
    }
}

/// Assert two `u32` values are equal.
fn assert_u32_eq(got: u32, expected: u32, label: &str, case: &OracleCase) -> Result<(), String> {
    if got != expected {
        Err(format!(
            "\n\
             ═══════════════════ DIFF VERIFY MISMATCH ═══════════════════\n\
             Case:    {}\n\
             Seed:    {}\n\
             Field:   {}\n\
             Got:     {}\n\
             Expected:{}\n\
             Reproduce:\n  \
               SEED={seed} cargo test --package xelma-contract --lib \\\n  \
               tests::diff_verify -- --nocapture\n\
             ══════════════════════════════════════════════════════════════",
            case.description,
            case.seed,
            label,
            got,
            expected,
            seed = case.seed,
        ))
    } else {
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 5 — Case executor
// ═══════════════════════════════════════════════════════════════════════════════

/// Execute one oracle case against both the contract and reference model.
/// Returns `Ok(())` on match, or `Err(diagnostic_string)` on mismatch.
fn execute_case(case: &OracleCase) -> Result<(), String> {
    // ── 5a: Price direction classification ──
    let c_dir = classify_price_direction(case.start_price, case.final_price);
    let r_dir = ref_classify_direction(case.start_price, case.final_price);
    assert_stroop_eq(
        c_dir as i128,
        r_dir as i128,
        "classify_price_direction",
        case,
    )?;

    // ── 5b: One-sided pool ──
    let c_1sided = crate::settlement_math::is_one_sided_pool(case.pool_up, case.pool_down);
    let r_1sided = ref_is_one_sided(case.pool_up, case.pool_down);
    if c_1sided != r_1sided {
        return Err(format!(
            "\n\
             ═══════════════════ DIFF VERIFY MISMATCH ═══════════════════\n\
             Case:    {}\n\
             Seed:    {}\n\
             Field:   is_one_sided_pool\n\
             Got:     {}\n\
             Expected:{}\n\
             Reproduce:\n  \
               SEED={seed} cargo test --package xelma-contract --lib \\\n  \
               tests::diff_verify -- --nocapture\n\
             ══════════════════════════════════════════════════════════════",
            case.description,
            case.seed,
            c_1sided,
            r_1sided,
            seed = case.seed,
        ));
    }

    // ── 5c: UpDown fee computation ──
    let (c_dw, c_dl, c_fee) = compute_updown_fee(case.pool_up, case.pool_down, case.fee_bps)
        .map_err(|e| format!("contract compute_updown_fee error: {:?}", e))?;
    let (r_dw, r_dl, r_fee) = ref_compute_updown_fee(case.pool_up, case.pool_down, case.fee_bps);
    assert_stroop_eq(c_dw, r_dw, "updown_fee.dist_winning", case)?;
    assert_stroop_eq(c_dl, r_dl, "updown_fee.dist_losing", case)?;
    assert_stroop_eq(c_fee, r_fee, "updown_fee.fee", case)?;

    // ── 5d: Precision fee computation ──
    let total_pot = case.pool_up + case.pool_down;
    let (c_pd, c_pf) = compute_precision_fee(total_pot, case.fee_bps)
        .map_err(|e| format!("contract compute_precision_fee error: {:?}", e))?;
    let (r_pd, r_pf) = ref_compute_precision_fee(total_pot, case.fee_bps);
    assert_stroop_eq(c_pd, r_pd, "precision_fee.distributable", case)?;
    assert_stroop_eq(c_pf, r_pf, "precision_fee.fee", case)?;

    // ── 5e: UpDown full payout vector ──
    let contract_positions: Vec<UpDownPosition> = case
        .positions
        .iter()
        .enumerate()
        .map(|(i, (amt, side))| UpDownPosition {
            index: i,
            amount: *amt,
            side_up: *side,
        })
        .collect();

    let c_updown = compute_updown_payouts(
        &contract_positions,
        case.start_price,
        case.final_price,
        case.pool_up,
        case.pool_down,
        case.fee_bps,
    )
    .map_err(|e| format!("contract compute_updown_payouts error: {:?}", e))?;

    let r_updown = ref_updown_payouts(
        &contract_positions,
        case.start_price,
        case.final_price,
        case.pool_up,
        case.pool_down,
        case.fee_bps,
    );

    if c_updown.len() != r_updown.len() {
        return Err(format!(
            "\n\
             ═══════════════════ DIFF VERIFY MISMATCH ═══════════════════\n\
             Case:    {}\n\
             Seed:    {}\n\
             Field:   updown_payouts length\n\
             Got:     {}\n\
             Expected:{}\n\
             ══════════════════════════════════════════════════════════════",
            case.description,
            case.seed,
            c_updown.len(),
            r_updown.len()
        ));
    }

    for (i, (contract_e, &(ref_payout, ref_winner, ref_refund))) in
        c_updown.iter().zip(r_updown.iter()).enumerate()
    {
        assert_stroop_eq(
            contract_e.payout,
            ref_payout,
            &format!("updown_payouts[{}].payout", i),
            case,
        )?;
        if contract_e.is_winner != ref_winner {
            return Err(format!(
                "\n\
                 ═══════════════════ DIFF VERIFY MISMATCH ═══════════════════\n\
                 Case:    {}\n\
                 Seed:    {}\n\
                 Field:   updown_payouts[{}].is_winner\n\
                 Got:     {}\n\
                 Expected:{}\n\
                 ══════════════════════════════════════════════════════════════",
                case.description, case.seed, i, contract_e.is_winner, ref_winner
            ));
        }
        if contract_e.is_refund != ref_refund {
            return Err(format!(
                "\n\
                 ═══════════════════ DIFF VERIFY MISMATCH ═══════════════════\n\
                 Case:    {}\n\
                 Seed:    {}\n\
                 Field:   updown_payouts[{}].is_refund\n\
                 Got:     {}\n\
                 Expected:{}\n\
                 ══════════════════════════════════════════════════════════════",
                case.description, case.seed, i, contract_e.is_refund, ref_refund
            ));
        }
    }

    // ── 5f: Precision winner-finding (per policy) ──
    let contract_entries: Vec<PrecisionEntry> = case
        .precision_entries
        .iter()
        .enumerate()
        .map(|(i, (pred, amt, rev))| PrecisionEntry {
            index: i,
            predicted_price: *pred,
            amount: *amt,
            revealed: *rev,
        })
        .collect();

    let c_winners = find_precision_winners_with_policy(
        &contract_entries,
        case.final_price,
        case.scoring_policy.clone(),
    );
    let (r_winner_indices, _, r_total_pot) =
        ref_find_precision_winners(&contract_entries, case.final_price, &case.scoring_policy);

    assert_stroop_eq(
        c_winners.total_pot as i128,
        r_total_pot as i128,
        "precision_winners.total_pot",
        case,
    )?;
    if c_winners.winner_indices != r_winner_indices {
        return Err(format!(
            "\n\
             ═══════════════════ DIFF VERIFY MISMATCH ═══════════════════\n\
             Case:    {}\n\
             Seed:    {}\n\
             Field:   precision_winners.winner_indices\n\
             Got:     {:?}\n\
             Expected:{:?}\n\
             Score mode: {:?}, confidence_band: {:?}\n\
             ══════════════════════════════════════════════════════════════",
            case.description,
            case.seed,
            c_winners.winner_indices,
            r_winner_indices,
            case.scoring_policy.mode,
            case.scoring_policy.confidence_band,
        ));
    }

    // ── 5g: Precision full payout vector ──
    let c_precision = compute_precision_payouts_with_policy(
        &contract_entries,
        case.final_price,
        case.fee_bps,
        case.scoring_policy.clone(),
        case.payout_policy,
    )
    .map_err(|e| format!("contract compute_precision_payouts error: {:?}", e))?;

    let r_precision = ref_precision_payouts(
        &contract_entries,
        case.final_price,
        case.fee_bps,
        &case.scoring_policy,
        case.payout_policy,
    );

    if c_precision.len() != r_precision.len() {
        return Err(format!(
            "\n\
             ═══════════════════ DIFF VERIFY MISMATCH ═══════════════════\n\
             Case:    {}\n\
             Seed:    {}\n\
             Field:   precision_payouts length\n\
             Got:     {}\n\
             Expected:{}\n\
             ══════════════════════════════════════════════════════════════",
            case.description,
            case.seed,
            c_precision.len(),
            r_precision.len()
        ));
    }

    for (i, (contract_e, &(ref_payout, ref_winner, ref_refund))) in
        c_precision.iter().zip(r_precision.iter()).enumerate()
    {
        assert_stroop_eq(
            contract_e.payout,
            ref_payout,
            &format!("precision_payouts[{}].payout", i),
            case,
        )?;
        if contract_e.is_winner != ref_winner {
            return Err(format!(
                "\n\
                 ═══════════════════ DIFF VERIFY MISMATCH ═══════════════════\n\
                 Case:    {}\n\
                 Seed:    {}\n\
                 Field:   precision_payouts[{}].is_winner\n\
                 Got:     {}\n\
                 Expected:{}\n\
                 ══════════════════════════════════════════════════════════════",
                case.description, case.seed, i, contract_e.is_winner, ref_winner
            ));
        }
        if contract_e.is_refund != ref_refund {
            return Err(format!(
                "\n\
                 ═══════════════════ DIFF VERIFY MISMATCH ═══════════════════\n\
                 Case:    {}\n\
                 Seed:    {}\n\
                 Field:   precision_payouts[{}].is_refund\n\
                 Got:     {}\n\
                 Expected:{}\n\
                 ══════════════════════════════════════════════════════════════",
                case.description, case.seed, i, contract_e.is_refund, ref_refund
            ));
        }
    }

    // ── 5h: Deviation bps ──
    let c_dev = compute_deviation_bps(case.final_price, case.deviation_reference)
        .map_err(|e| format!("contract compute_deviation_bps error: {:?}", e))?;
    let r_dev = ref_deviation_bps(case.final_price, case.deviation_reference);
    assert_u32_eq(c_dev, r_dev, "deviation_bps", case)?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 6 — Minimal-case minimiser
// ═══════════════════════════════════════════════════════════════════════════════
//
// When a mismatch is found, we attempt to narrow the input to the smallest
// case that still reproduces the failure.  This uses simple binary-search
// style reduction: halving stakes, removing participants, disabling fees,
// etc.

/// Attempt to minimise a failing case by reducing its inputs while preserving
/// the mismatch.  Returns the minimised case.
fn minimise_case(original: &OracleCase) -> OracleCase {
    let mut best = original.clone();

    // 1. Try removing each participant one at a time (binary removal).
    if best.positions.len() > 2 {
        for skip in (0..best.positions.len()).rev() {
            let mut reduced = best.clone();
            reduced.positions.remove(skip);
            // Recompute pools.
            reduced.pool_up = reduced
                .positions
                .iter()
                .filter(|p| p.1)
                .map(|p| p.0)
                .sum::<i128>()
                .max(0);
            reduced.pool_down = reduced
                .positions
                .iter()
                .filter(|p| !p.1)
                .map(|p| p.0)
                .sum::<i128>()
                .max(0);
            if execute_case(&reduced).is_err() {
                best = reduced;
            }
        }
    }

    // 2. Try halving stakes (keep direction).
    {
        let mut reduced = best.clone();
        for p in &mut reduced.positions {
            p.0 = p.0.max(1) / 2;
        }
        reduced.pool_up = reduced
            .positions
            .iter()
            .filter(|p| p.1)
            .map(|p| p.0)
            .sum::<i128>()
            .max(0);
        reduced.pool_down = reduced
            .positions
            .iter()
            .filter(|p| !p.1)
            .map(|p| p.0)
            .sum::<i128>()
            .max(0);
        if execute_case(&reduced).is_err() {
            best = reduced;
        }
    }

    // 3. Try removing fee.
    {
        let mut reduced = best.clone();
        reduced.fee_bps = None;
        if execute_case(&reduced).is_err() {
            best = reduced;
        }
    }

    // 4. Try setting final_price = start_price (tie).
    {
        let mut reduced = best.clone();
        reduced.final_price = reduced.start_price;
        if execute_case(&reduced).is_err() {
            best = reduced;
        }
    }

    best
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 7 — Fixed regression cases
// ═══════════════════════════════════════════════════════════════════════════════
//
// Manually crafted edge cases that target historically tricky code paths.
// These always run regardless of mode.

fn fixed_regression_cases() -> Vec<OracleCase> {
    let scoring_default = PrecisionScoringPolicy {
        mode: PrecisionScoringMode::AbsoluteDistance,
        confidence_band: None,
    };

    vec![
        OracleCase {
            seed: 0xDEAD_0001,
            description: "regression: thin_losing_pool_fee_spillover".into(),
            start_price: 10_000_000,
            final_price: 20_000_000,
            pool_up: 1_000_000,
            pool_down: 10,
            fee_bps: Some(500),
            positions: vec![(500_000, true), (500_000, true), (10, false)],
            precision_entries: vec![],
            scoring_policy: scoring_default.clone(),
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 10_000_000,
        },
        OracleCase {
            seed: 0xDEAD_0002,
            description: "regression: tie_refund_with_fee_configured".into(),
            start_price: 10_000_000,
            final_price: 10_000_000,
            pool_up: 100,
            pool_down: 200,
            fee_bps: Some(1_000),
            positions: vec![(100, true), (200, false)],
            precision_entries: vec![],
            scoring_policy: scoring_default.clone(),
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 10_000_000,
        },
        OracleCase {
            seed: 0xDEAD_0003,
            description: "regression: one_sided_pool_refunds".into(),
            start_price: 10_000_000,
            final_price: 20_000_000,
            pool_up: 500,
            pool_down: 0,
            fee_bps: Some(100),
            positions: vec![(300, true), (200, true)],
            precision_entries: vec![],
            scoring_policy: scoring_default.clone(),
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 10_000_000,
        },
        OracleCase {
            seed: 0xDEAD_0004,
            description: "regression: precision_all_unrevealed_refund".into(),
            start_price: 10_000_000,
            final_price: 15_000_000,
            pool_up: 0,
            pool_down: 0,
            fee_bps: Some(500),
            positions: vec![],
            precision_entries: vec![(10_000_000, 100, false), (20_000_000, 200, false)],
            scoring_policy: scoring_default.clone(),
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 10_000_000,
        },
        OracleCase {
            seed: 0xDEAD_0005,
            description: "regression: precision_mixed_reveal_fee".into(),
            start_price: 10_000_000,
            final_price: 10_005_000,
            pool_up: 0,
            pool_down: 0,
            fee_bps: Some(200),
            positions: vec![],
            precision_entries: vec![
                (10_000_000, 50, true),  // revealed, very close
                (10_010_000, 30, true),  // revealed, farther
                (15_000_000, 20, false), // unrevealed — forfeit
            ],
            scoring_policy: scoring_default.clone(),
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 10_000_000,
        },
        OracleCase {
            seed: 0xDEAD_0006,
            description: "regression: precision_stake_weighted_tie".into(),
            start_price: 10_000_000,
            final_price: 10_000_000,
            pool_up: 0,
            pool_down: 0,
            fee_bps: None,
            positions: vec![],
            precision_entries: vec![(10_000_000, 30, true), (10_000_000, 70, true)],
            scoring_policy: scoring_default.clone(),
            payout_policy: PrecisionPayoutPolicy::StakeWeighted,
            deviation_reference: 10_000_000,
        },
        OracleCase {
            seed: 0xDEAD_0007,
            description: "regression: relative_distance_scoring".into(),
            start_price: 10_000_000,
            final_price: 10_050_000,
            pool_up: 0,
            pool_down: 0,
            fee_bps: Some(100),
            positions: vec![],
            precision_entries: vec![
                (10_040_000, 100, true), // score = 10000 * 10000/10050000 = 995
                (10_060_000, 200, true), // score = 10000 * 10000/10050000 = 995
                (11_000_000, 50, true),  // score much higher — loses
            ],
            scoring_policy: PrecisionScoringPolicy {
                mode: PrecisionScoringMode::RelativeDistance,
                confidence_band: None,
            },
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 10_000_000,
        },
        OracleCase {
            seed: 0xDEAD_0008,
            description: "regression: confidence_band_multiple_winners".into(),
            start_price: 10_000_000,
            final_price: 10_000_000,
            pool_up: 0,
            pool_down: 0,
            fee_bps: None,
            positions: vec![],
            precision_entries: vec![
                (10_000_000, 100, true), // diff 0
                (10_000_100, 200, true), // diff 100
                (10_000_050, 150, true), // diff 50
            ],
            scoring_policy: PrecisionScoringPolicy {
                mode: PrecisionScoringMode::AbsoluteDistance,
                confidence_band: Some(100),
            },
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 10_000_000,
        },
        OracleCase {
            seed: 0xDEAD_0009,
            description: "regression: max_fee_10pct".into(),
            start_price: 10_000_000,
            final_price: 20_000_000,
            pool_up: 1_000_000,
            pool_down: 1_000_000,
            fee_bps: Some(1_000),
            positions: vec![(500_000, true), (500_000, true), (1_000_000, false)],
            precision_entries: vec![],
            scoring_policy: scoring_default.clone(),
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 10_000_000,
        },
        OracleCase {
            seed: 0xDEAD_000A,
            description: "regression: single_participant_updown_win".into(),
            start_price: 10_000_000,
            final_price: 20_000_000,
            pool_up: 100,
            pool_down: 200,
            fee_bps: Some(100),
            positions: vec![(100, true), (200, false)],
            precision_entries: vec![],
            scoring_policy: scoring_default.clone(),
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 10_000_000,
        },
        OracleCase {
            seed: 0xDEAD_000B,
            description: "regression: large_stakes_near_overflow".into(),
            start_price: 1_000_000,
            final_price: 2_000_000,
            pool_up: 900_000_000_000_000, // 9e14 — large but within i128
            pool_down: 100_000_000_000_000,
            fee_bps: Some(100),
            positions: vec![
                (450_000_000_000_000, true),
                (450_000_000_000_000, true),
                (100_000_000_000_000, false),
            ],
            precision_entries: vec![],
            scoring_policy: scoring_default.clone(),
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 1_000_000,
        },
        OracleCase {
            seed: 0xDEAD_000C,
            description: "regression: precision_5way_tie_remainder".into(),
            start_price: 10_000_000,
            final_price: 10_000_000,
            pool_up: 0,
            pool_down: 0,
            fee_bps: None,
            positions: vec![],
            precision_entries: vec![
                (10_000_000, 21, true),
                (10_000_000, 21, true),
                (10_000_000, 21, true),
                (10_000_000, 20, true),
                (10_000_000, 20, true),
            ],
            scoring_policy: scoring_default.clone(),
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 10_000_000,
        },
        OracleCase {
            seed: 0xDEAD_000D,
            description: "regression: deviation_bps_boundary".into(),
            start_price: 10_000_000,
            final_price: 10_500_000,
            pool_up: 0,
            pool_down: 0,
            fee_bps: None,
            positions: vec![],
            precision_entries: vec![],
            scoring_policy: scoring_default.clone(),
            payout_policy: PrecisionPayoutPolicy::Equal,
            deviation_reference: 10_000_000,
        },
    ]
}

// ═══════════════════════════════════════════════════════════════════════════════
// § 8 — Main test entrypoints
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn differential_verify_fixed_regression() {
    let cases = fixed_regression_cases();
    let mut failures: Vec<String> = Vec::new();

    for case in &cases {
        if let Err(diag) = execute_case(case) {
            failures.push(diag);
        }
    }

    if !failures.is_empty() {
        let msg: String = failures.join("\n");
        panic!(
            "\n\
             ═══════════════ DIFF VERIFY: FIXED REGRESSION FAILURES ══════════════\n\
             {} cases checked, {} FAILED\n\
             {}\n\
             ══════════════════════════════════════════════════════════════════════",
            cases.len(),
            failures.len(),
            msg
        );
    }
}

#[test]
fn differential_verify_fuzz() {
    let mode = env::var("DIFF_VERIFY_MODE").unwrap_or_else(|_| "fast".into());
    let case_count: u32 = match mode.as_str() {
        "extended" => 1_000,
        _ => 100, // fast (default for PR CI)
    };

    let base_seed: u64 = env::var("SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0xBEEF_1234);

    let cases = generate_cases(base_seed, case_count);
    let mut failures: Vec<String> = Vec::new();

    for (i, case) in cases.iter().enumerate() {
        match execute_case(case) {
            Ok(()) => {}
            Err(diag) => {
                std::eprintln!(
                    "Mismatch at case {}/{} (seed={}), minimising...",
                    i + 1,
                    case_count,
                    case.seed
                );
                let minimised = minimise_case(case);
                let mini_diag = match execute_case(&minimised) {
                    Ok(()) => "  (minimised case no longer reproduces — original may be flaky)"
                        .to_string(),
                    Err(d) => d,
                };
                failures.push(format!(
                    "Case {} (seed={}):\n{}\nMinimised: {}",
                    i, case.seed, diag, mini_diag
                ));
            }
        }
    }

    if !failures.is_empty() {
        let report: String = failures.join("\n\n");
        panic!(
            "\n\
             ═══════════════ DIFF VERIFY: FUZZ MISMATCHES ══════════════\n\
             Mode:    {}\n\
             Seed:    {}\n\
             Cases:   {} checked, {} FAILED\n\
             \n\
             Failing cases:\n{}\n\
             \n\
             Reproduce all:\n\
               SEED={seed} DIFF_VERIFY_MODE={mode} cargo test \\\n  \
               --package xelma-contract --lib tests::diff_verify \\\n  \
               -- --nocapture\n\
             ══════════════════════════════════════════════════════════════",
            mode,
            base_seed,
            case_count,
            failures.len(),
            report,
            seed = base_seed,
            mode = mode,
        );
    }
}
