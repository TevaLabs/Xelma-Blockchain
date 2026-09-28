// SPDX-License-Identifier: MIT
//! Pure settlement math functions — no `Env`, no storage, no events.
//!
//! These helpers isolate pot/payout/tie/fee arithmetic so that auditors,
//! property tests, and golden-vector tests can verify correctness without
//! touching the Soroban test harness.
//!
//! Every function is deterministic given its inputs.  Callers in
//! `settlement.rs` remain responsible for storage reads/writes, events,
//! and authorization — this module is the *engine*, not the *orchestrator*.

use alloc::vec::Vec;

use crate::errors::ContractError;
use crate::math_common::{payout_add, payout_mul, BPS_DENOMINATOR};

/// Payout policy for Precision mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PrecisionPayoutPolicy {
    Equal = 0,         // Split payout pool equally among winners (default)
    StakeWeighted = 1, // Split payout pool proportionally to winner stakes
}

/// Scoring mode for Precision winner determination
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PrecisionScoringMode {
    AbsoluteDistance = 0, // Score = |predicted_price - final_price|
    RelativeDistance = 1, // Score = |predicted_price - final_price| * 10_000 / final_price (basis points)
}

/// Scoring policy for Precision mode including scoring metric and optional confidence band tolerance
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrecisionScoringPolicy {
    pub mode: PrecisionScoringMode,
    pub confidence_band: Option<u128>,
}

// ─── Price direction ─────────────────────────────────────────────────────────

/// Outcome of comparing the oracle settlement price against the round's
/// starting price.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceDirection {
    /// `final_price > start_price`
    Up,
    /// `final_price < start_price`
    Down,
    /// `final_price == start_price`
    Unchanged,
}

/// Pure price-direction classification.
#[inline]
pub fn classify_price_direction(start_price: u128, final_price: u128) -> PriceDirection {
    if final_price > start_price {
        PriceDirection::Up
    } else if final_price < start_price {
        PriceDirection::Down
    } else {
        PriceDirection::Unchanged
    }
}

/// Returns `true` when exactly one pool is empty (XOR).
/// One-sided rounds refund all participants regardless of price movement.
#[inline]
pub fn is_one_sided_pool(pool_up: i128, pool_down: i128) -> bool {
    (pool_up == 0) != (pool_down == 0)
}

// ─── Protocol fee math (pure) ────────────────────────────────────────────────

/// Splits a `(winning_pool, losing_pool)` pair into the post-fee pools
/// and the treasury's cut.
///
/// Conservation invariant **always** holds:
///   `dist_winning + dist_losing + fee == winning + losing`
///
/// In the pathological case `fee > losing_pool` (very thin losing-side
/// liquidity near the bps cap), the spillover is deducted from
/// `winning_pool`.
pub fn compute_updown_fee(
    winning_pool: i128,
    losing_pool: i128,
    fee_bps: Option<u32>,
) -> Result<(i128, i128, i128), ContractError> {
    if fee_bps.is_none() {
        return Ok((winning_pool, losing_pool, 0));
    }
    let bps_value = fee_bps.unwrap();
    let total_pot = payout_add(winning_pool, losing_pool)?;
    let fee_amount = total_pot
        .checked_mul(bps_value as i128)
        .ok_or(ContractError::Overflow)?
        / BPS_DENOMINATOR;
    if fee_amount == 0 {
        return Ok((winning_pool, losing_pool, 0));
    }
    let fee_from_losing = fee_amount.min(losing_pool);
    let fee_from_winning = fee_amount
        .checked_sub(fee_from_losing)
        .ok_or(ContractError::Overflow)?;
    let dist_winning = winning_pool
        .checked_sub(fee_from_winning)
        .ok_or(ContractError::Overflow)?;
    let dist_losing = losing_pool
        .checked_sub(fee_from_losing)
        .ok_or(ContractError::Overflow)?;
    Ok((dist_winning, dist_losing, fee_amount))
}

/// Splits a precision-mode `total_pot` into the distributable amount and
/// the treasury's cut.  Returns `(distributable, fee_amount)`.
pub fn compute_precision_fee(
    total_pot: i128,
    fee_bps: Option<u32>,
) -> Result<(i128, i128), ContractError> {
    if fee_bps.is_none() || total_pot <= 0 {
        return Ok((total_pot, 0));
    }
    let bps_value = fee_bps.unwrap();
    let fee_amount = total_pot
        .checked_mul(bps_value as i128)
        .ok_or(ContractError::Overflow)?
        / BPS_DENOMINATOR;
    let distributable = total_pot
        .checked_sub(fee_amount)
        .ok_or(ContractError::Overflow)?;
    Ok((distributable, fee_amount))
}

// ─── UpDown payout math ──────────────────────────────────────────────────────

/// Computes the gross payout for a single UpDown winner given their stake,
/// the winning-side pool total, and the total distributable funds (post-fee
/// sum of both pools).
///
/// Formula: `payout = stake * total_distributable / winning_pool`
#[inline]
pub fn compute_updown_winner_payout(
    stake: i128,
    winning_pool: i128,
    total_distributable: i128,
) -> Result<i128, ContractError> {
    if winning_pool == 0 {
        return Ok(0);
    }
    Ok(payout_mul(stake, total_distributable)? / winning_pool)
}

// ─── Precision winner determination ──────────────────────────────────────────

/// A single prediction for the precision winner-finding logic.
#[derive(Clone, Debug, PartialEq)]
pub struct PrecisionEntry {
    /// Participant index (0-based).  Caller maps this back to an address.
    pub index: usize,
    /// The price the participant predicted (4 decimal places).
    pub predicted_price: u128,
    /// The amount the participant staked.
    pub amount: i128,
    /// Whether this entry has a revealed prediction (vs unrevealed commitment).
    pub revealed: bool,
}

/// Result of the precision winner-determination algorithm.
#[derive(Clone, Debug, PartialEq)]
pub struct PrecisionWinnersResult {
    /// Indices (into the original `entries` slice) of the winning participants.
    pub winner_indices: Vec<usize>,
    /// Total pot (sum of all staked amounts).
    pub total_pot: i128,
    /// Indices of losing participants.
    pub loser_indices: Vec<usize>,
}

/// Finds the closest prediction(s) to `final_price` under default absolute distance scoring.
pub fn find_precision_winners(
    entries: &[PrecisionEntry],
    final_price: u128,
) -> PrecisionWinnersResult {
    find_precision_winners_with_policy(
        entries,
        final_price,
        PrecisionScoringPolicy {
            mode: PrecisionScoringMode::AbsoluteDistance,
            confidence_band: None,
        },
    )
}

/// Finds precision winners given a explicit `PrecisionScoringPolicy` (Absolute vs Relative distance, optional confidence band).
pub fn find_precision_winners_with_policy(
    entries: &[PrecisionEntry],
    final_price: u128,
    policy: PrecisionScoringPolicy,
) -> PrecisionWinnersResult {
    let mut total_pot: i128 = 0;
    let mut scores: Vec<(usize, u128)> = Vec::new();
    let mut min_score: Option<u128> = None;

    for entry in entries {
        total_pot = total_pot.saturating_add(entry.amount);

        if !entry.revealed {
            continue;
        }

        let abs_diff = if entry.predicted_price >= final_price {
            entry.predicted_price - final_price
        } else {
            final_price - entry.predicted_price
        };

        let score = match policy.mode {
            PrecisionScoringMode::AbsoluteDistance => abs_diff,
            PrecisionScoringMode::RelativeDistance => {
                if final_price > 0 {
                    abs_diff.saturating_mul(10_000) / final_price
                } else {
                    abs_diff
                }
            }
        };

        scores.push((entry.index, score));

        match min_score {
            None => min_score = Some(score),
            Some(cur_min) => {
                if score < cur_min {
                    min_score = Some(score);
                }
            }
        }
    }

    let mut winner_indices: Vec<usize> = Vec::new();
    if let Some(best) = min_score {
        for &(idx, score) in &scores {
            let is_winner = match policy.confidence_band {
                None => score == best,
                Some(band) => score <= band || score <= best.saturating_add(band),
            };
            if is_winner {
                winner_indices.push(idx);
            }
        }
    }

    // Build loser list from all non-winners
    let mut loser_indices: Vec<usize> = Vec::new();
    for entry in entries {
        if !winner_indices.contains(&entry.index) {
            loser_indices.push(entry.index);
        }
    }

    PrecisionWinnersResult {
        winner_indices,
        total_pot,
        loser_indices,
    }
}

// ─── Precision pot splitting ─────────────────────────────────────────────────

/// Splits `distributable` among `winner_count` winners equally.
///
/// The remainder (distributable % winner_count) is assigned to the first
/// winner. Every winner receives at least `distributable / winner_count`.
pub fn split_pot_among_winners(
    distributable: i128,
    winner_count: usize,
) -> Result<Vec<i128>, ContractError> {
    if winner_count == 0 || distributable <= 0 {
        return Ok(Vec::new());
    }
    let count = winner_count as i128;
    let per_winner = distributable / count;
    let remainder = distributable % count;

    let mut payouts = Vec::new();
    for i in 0..winner_count {
        let payout = if i == 0 {
            per_winner
                .checked_add(remainder)
                .ok_or(ContractError::Overflow)?
        } else {
            per_winner
        };
        payouts.push(payout);
    }
    Ok(payouts)
}

/// Splits `distributable` proportionally according to winner stakes.
///
/// Integer remainder is allocated to the first winner for exact conservation.
pub fn split_pot_stake_weighted(
    distributable: i128,
    winner_stakes: &[i128],
) -> Result<Vec<i128>, ContractError> {
    if winner_stakes.is_empty() || distributable <= 0 {
        return Ok(Vec::new());
    }

    let mut total_winner_stake: i128 = 0;
    for &stake in winner_stakes {
        total_winner_stake = total_winner_stake
            .checked_add(stake)
            .ok_or(ContractError::Overflow)?;
    }

    if total_winner_stake == 0 {
        return split_pot_among_winners(distributable, winner_stakes.len());
    }

    let mut payouts = Vec::new();
    let mut total_allocated = 0i128;

    for &stake in winner_stakes {
        let payout = payout_mul(stake, distributable)? / total_winner_stake;
        payouts.push(payout);
        total_allocated = total_allocated
            .checked_add(payout)
            .ok_or(ContractError::Overflow)?;
    }

    let remainder = distributable
        .checked_sub(total_allocated)
        .ok_or(ContractError::Overflow)?;

    if remainder > 0 && !payouts.is_empty() {
        payouts[0] = payouts[0]
            .checked_add(remainder)
            .ok_or(ContractError::Overflow)?;
    }

    Ok(payouts)
}

// ─── Composite: compute full UpDown payout vector ────────────────────────────

/// A single participant's UpDown position for the payout engine.
#[derive(Clone, Debug, PartialEq)]
pub struct UpDownPosition {
    pub index: usize,
    pub amount: i128,
    pub side_up: bool, // true = Up, false = Down
}

/// Computed payout for one UpDown participant.
#[derive(Clone, Debug, PartialEq)]
pub struct UpDownPayoutEntry {
    pub index: usize,
    pub stake: i128,
    pub payout: i128,
    pub is_winner: bool,
    pub is_refund: bool,
}

/// Computes the full payout vector for an UpDown round.
///
/// Inputs are the round-level parameters and the list of participant
/// positions.  Returns one `UpDownPayoutEntry` per participant.
pub fn compute_updown_payouts(
    positions: &[UpDownPosition],
    start_price: u128,
    final_price: u128,
    pool_up: i128,
    pool_down: i128,
    fee_bps: Option<u32>,
) -> Result<Vec<UpDownPayoutEntry>, ContractError> {
    let direction = classify_price_direction(start_price, final_price);
    let one_sided = is_one_sided_pool(pool_up, pool_down);

    let mut results: Vec<UpDownPayoutEntry> = Vec::new();

    // Refund scenarios
    if direction == PriceDirection::Unchanged || one_sided {
        for pos in positions {
            results.push(UpDownPayoutEntry {
                index: pos.index,
                stake: pos.amount,
                payout: pos.amount,
                is_winner: false,
                is_refund: true,
            });
        }
        return Ok(results);
    }

    // Competitive settlement
    let (winning_side_up, winning_pool, losing_pool) = match direction {
        PriceDirection::Up => (true, pool_up, pool_down),
        PriceDirection::Down => (false, pool_down, pool_up),
        PriceDirection::Unchanged => unreachable!(),
    };

    if winning_pool == 0 {
        // No winning-side liquidity — refund everyone
        for pos in positions {
            results.push(UpDownPayoutEntry {
                index: pos.index,
                stake: pos.amount,
                payout: pos.amount,
                is_winner: false,
                is_refund: true,
            });
        }
        return Ok(results);
    }

    let (dist_winning, dist_losing, _fee_amount) =
        compute_updown_fee(winning_pool, losing_pool, fee_bps)?;
    let total_distributable = payout_add(dist_winning, dist_losing)?;

    for pos in positions {
        let is_winner = pos.side_up == winning_side_up;
        let payout = if is_winner {
            compute_updown_winner_payout(pos.amount, winning_pool, total_distributable)?
        } else {
            0
        };
        results.push(UpDownPayoutEntry {
            index: pos.index,
            stake: pos.amount,
            payout,
            is_winner,
            is_refund: false,
        });
    }

    Ok(results)
}

// ─── Composite: compute full Precision payout vector ─────────────────────────

/// Computed payout for one Precision participant.
#[derive(Clone, Debug, PartialEq)]
pub struct PrecisionPayoutEntry {
    pub index: usize,
    pub stake: i128,
    pub predicted_price: u128,
    pub payout: i128,
    pub is_winner: bool,
    pub is_refund: bool,
}

pub fn compute_precision_payouts(
    entries: &[PrecisionEntry],
    final_price: u128,
    fee_bps: Option<u32>,
) -> Result<Vec<PrecisionPayoutEntry>, ContractError> {
    compute_precision_payouts_with_policy(
        entries,
        final_price,
        fee_bps,
        PrecisionScoringPolicy {
            mode: PrecisionScoringMode::AbsoluteDistance,
            confidence_band: None,
        },
        PrecisionPayoutPolicy::Equal,
    )
}

/// Computes the full payout vector for a Precision round using explicit scoring and payout policies.
pub fn compute_precision_payouts_with_policy(
    entries: &[PrecisionEntry],
    final_price: u128,
    fee_bps: Option<u32>,
    scoring_policy: PrecisionScoringPolicy,
    payout_policy: PrecisionPayoutPolicy,
) -> Result<Vec<PrecisionPayoutEntry>, ContractError> {
    let result = find_precision_winners_with_policy(entries, final_price, scoring_policy);

    // All-unrevealed: refund everyone
    if result.winner_indices.is_empty() && result.total_pot > 0 {
        let mut payouts: Vec<PrecisionPayoutEntry> = Vec::new();
        for entry in entries {
            payouts.push(PrecisionPayoutEntry {
                index: entry.index,
                stake: entry.amount,
                predicted_price: entry.predicted_price,
                payout: entry.amount,
                is_winner: false,
                is_refund: true,
            });
        }
        return Ok(payouts);
    }

    // No pot: nothing to distribute
    if result.total_pot <= 0 || result.winner_indices.is_empty() {
        let mut payouts: Vec<PrecisionPayoutEntry> = Vec::new();
        for entry in entries {
            payouts.push(PrecisionPayoutEntry {
                index: entry.index,
                stake: entry.amount,
                predicted_price: entry.predicted_price,
                payout: 0,
                is_winner: false,
                is_refund: false,
            });
        }
        return Ok(payouts);
    }

    let (distributable, _fee_amount) = compute_precision_fee(result.total_pot, fee_bps)?;

    let winner_payouts = match payout_policy {
        PrecisionPayoutPolicy::Equal => {
            split_pot_among_winners(distributable, result.winner_indices.len())?
        }
        PrecisionPayoutPolicy::StakeWeighted => {
            let winner_stakes: Vec<i128> = result
                .winner_indices
                .iter()
                .map(|&idx| entries[idx].amount)
                .collect();
            split_pot_stake_weighted(distributable, &winner_stakes)?
        }
    };

    let mut payouts: Vec<PrecisionPayoutEntry> = Vec::new();
    for entry in entries {
        let winner_pos = result
            .winner_indices
            .iter()
            .position(|&idx| idx == entry.index);
        let (payout, is_winner, is_refund) = if let Some(pos) = winner_pos {
            (winner_payouts[pos], true, false)
        } else {
            (0, false, false)
        };
        payouts.push(PrecisionPayoutEntry {
            index: entry.index,
            stake: entry.amount,
            predicted_price: entry.predicted_price,
            payout,
            is_winner,
            is_refund,
        });
    }

    Ok(payouts)
}

// ─── Oracle deviation math ───────────────────────────────────────────────────

/// Computes the basis-point deviation of `price` from `reference`.
///
/// Returns `(diff_bps, diff_abs)` where `diff_bps = |price - ref| * 10000 / ref`.
pub fn compute_deviation_bps(price: u128, reference: u128) -> Result<u32, ContractError> {
    if reference == 0 {
        return Err(ContractError::InvalidPrice);
    }
    let diff = if price >= reference {
        price
            .checked_sub(reference)
            .ok_or(ContractError::Overflow)?
    } else {
        reference
            .checked_sub(price)
            .ok_or(ContractError::Overflow)?
    };
    let diff_bps_u128 = diff
        .checked_mul(10_000u128)
        .ok_or(ContractError::Overflow)?
        / reference;
    diff_bps_u128
        .try_into()
        .map_err(|_| ContractError::Overflow)
}

// ─── Median computation (for multi-feed) ─────────────────────────────────────

/// Sorts a slice of `u128` values in-place (insertion sort, O(N²)).
/// Acceptable because N ≤ 32 in all call-sites.
pub fn sort_prices_in_place(prices: &mut [u128]) {
    for i in 1..prices.len() {
        let key = prices[i];
        let mut j = i;
        while j > 0 && prices[j - 1] > key {
            prices[j] = prices[j - 1];
            j -= 1;
        }
        prices[j] = key;
    }
}

/// Computes the median of a **sorted** slice of `u128` values.
///
/// For odd N: returns the middle element.
/// For even N: returns the floor average of the two middle elements.
pub fn median_of_sorted(prices: &[u128]) -> Option<u128> {
    let n = prices.len();
    if n == 0 {
        return None;
    }
    if n % 2 == 1 {
        Some(prices[n / 2])
    } else {
        let mid1 = prices[n / 2 - 1];
        let mid2 = prices[n / 2];
        // Overflow impossible: both are ≤ u128::MAX, sum ≤ u128::MAX for realistic prices
        Some(mid1.saturating_add(mid2) / 2)
    }
}

// ─── Total pot computation ───────────────────────────────────────────────────

/// Computes the total pot for an UpDown round.
#[inline]
pub fn total_pot_updown(pool_up: i128, pool_down: i128) -> i128 {
    pool_up.saturating_add(pool_down)
}

/// Computes the total pot for a Precision round from a list of stake amounts.
pub fn total_pot_precision(stakes: &[i128]) -> i128 {
    stakes.iter().fold(0i128, |acc, &s| acc.saturating_add(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    // ─── Price direction tests ───────────────────────────────────────────

    #[test]
    fn test_price_direction_up() {
        assert_eq!(
            classify_price_direction(1_0000000, 1_5000000),
            PriceDirection::Up
        );
    }

    #[test]
    fn test_price_direction_down() {
        assert_eq!(
            classify_price_direction(2_0000000, 1_5000000),
            PriceDirection::Down
        );
    }

    #[test]
    fn test_price_direction_unchanged() {
        assert_eq!(
            classify_price_direction(1_0000000, 1_0000000),
            PriceDirection::Unchanged
        );
    }

    // ─── Fee math tests ──────────────────────────────────────────────────

    #[test]
    fn test_updown_fee_no_fee_config() {
        let (dw, dl, fee) = compute_updown_fee(300, 150, None).unwrap();
        assert_eq!(dw, 300);
        assert_eq!(dl, 150);
        assert_eq!(fee, 0);
    }

    #[test]
    fn test_updown_fee_with_1_percent() {
        // 1% fee on total pot 450 => fee = 4 (floor: 450*100/10000=4)
        let (dw, dl, fee) = compute_updown_fee(300, 150, Some(100)).unwrap();
        assert_eq!(fee, 4);
        // fee_from_losing = min(4, 150) = 4, fee_from_winning = 0
        assert_eq!(dl, 146);
        assert_eq!(dw, 300);
        // conservation: 300+146+4 = 450 ✓
        assert_eq!(dw + dl + fee, 450);
    }

    #[test]
    fn test_updown_fee_thin_losing_pool() {
        // Very thin losing pool: fee > losing_pool
        let (dw, dl, fee) = compute_updown_fee(1000, 10, Some(500)).unwrap();
        // total_pot = 1010, fee = 1010*500/10000 = 50
        assert_eq!(fee, 50);
        // fee_from_losing = min(50, 10) = 10, fee_from_winning = 40
        assert_eq!(dl, 0);
        assert_eq!(dw, 960);
        // conservation: 960+0+50 = 1010 ✓
        assert_eq!(dw + dl + fee, 1010);
    }

    #[test]
    fn test_precision_fee_no_config() {
        let (dist, fee) = compute_precision_fee(500, None).unwrap();
        assert_eq!(dist, 500);
        assert_eq!(fee, 0);
    }

    #[test]
    fn test_precision_fee_with_2_percent() {
        let (dist, fee) = compute_precision_fee(1000, Some(200)).unwrap();
        // fee = 1000*200/10000 = 20
        assert_eq!(fee, 20);
        assert_eq!(dist, 980);
    }

    // ─── UpDown payout tests ─────────────────────────────────────────────

    #[test]
    fn test_updown_winner_payout_proportional() {
        // Alice staked 100 of 300 pool_up. Total distributable = 450.
        // payout = 100*450/300 = 150
        let payout = compute_updown_winner_payout(100, 300, 450).unwrap();
        assert_eq!(payout, 150);
    }

    #[test]
    fn test_updown_winner_payout_zero_pool() {
        let payout = compute_updown_winner_payout(100, 0, 450).unwrap();
        assert_eq!(payout, 0);
    }

    #[test]
    fn test_updown_full_payouts_price_up() {
        let positions = vec![
            UpDownPosition {
                index: 0,
                amount: 100,
                side_up: true,
            },
            UpDownPosition {
                index: 1,
                amount: 200,
                side_up: true,
            },
            UpDownPosition {
                index: 2,
                amount: 150,
                side_up: false,
            },
        ];
        let results = compute_updown_payouts(
            &positions, 1_0000000, // start
            1_5000000, // final (up)
            300,       // pool_up
            150,       // pool_down
            None,      // no fee
        )
        .unwrap();

        // Alice (Up): 100 * 450 / 300 = 150
        assert_eq!(results[0].payout, 150);
        assert!(results[0].is_winner);
        // Bob (Up): 200 * 450 / 300 = 300
        assert_eq!(results[1].payout, 300);
        assert!(results[1].is_winner);
        // Charlie (Down): 0
        assert_eq!(results[2].payout, 0);
        assert!(!results[2].is_winner);
    }

    #[test]
    fn test_updown_full_payouts_unchanged_refunds() {
        let positions = vec![
            UpDownPosition {
                index: 0,
                amount: 100,
                side_up: true,
            },
            UpDownPosition {
                index: 1,
                amount: 50,
                side_up: false,
            },
        ];
        let results = compute_updown_payouts(
            &positions, 1_0000000, 1_0000000, // unchanged
            100, 50, None,
        )
        .unwrap();

        assert_eq!(results[0].payout, 100);
        assert!(results[0].is_refund);
        assert_eq!(results[1].payout, 50);
        assert!(results[1].is_refund);
    }

    #[test]
    fn test_updown_full_payouts_one_sided_refunds() {
        let positions = vec![
            UpDownPosition {
                index: 0,
                amount: 100,
                side_up: true,
            },
            UpDownPosition {
                index: 1,
                amount: 200,
                side_up: true,
            },
        ];
        let results = compute_updown_payouts(
            &positions, 1_0000000, 1_5000000, // up
            300, 0, // one-sided (no down pool)
            None,
        )
        .unwrap();

        // One-sided: all refunded
        assert_eq!(results[0].payout, 100);
        assert!(results[0].is_refund);
        assert_eq!(results[1].payout, 200);
        assert!(results[1].is_refund);
    }

    // ─── Precision winner determination tests ────────────────────────────

    #[test]
    fn test_find_precision_winners_single_winner() {
        let entries = vec![
            PrecisionEntry {
                index: 0,
                predicted_price: 2297,
                amount: 100,
                revealed: true,
            },
            PrecisionEntry {
                index: 1,
                predicted_price: 2300,
                amount: 150,
                revealed: true,
            },
            PrecisionEntry {
                index: 2,
                predicted_price: 2500,
                amount: 50,
                revealed: true,
            },
        ];
        let result = find_precision_winners(&entries, 2298);
        // Alice (diff 1) wins alone
        assert_eq!(result.winner_indices, vec![0]);
        assert!(result.loser_indices.contains(&1));
        assert!(result.loser_indices.contains(&2));
        assert_eq!(result.total_pot, 300);
    }

    #[test]
    fn test_find_precision_winners_tie() {
        let entries = vec![
            PrecisionEntry {
                index: 0,
                predicted_price: 2100,
                amount: 100,
                revealed: true,
            },
            PrecisionEntry {
                index: 1,
                predicted_price: 2300,
                amount: 150,
                revealed: true,
            },
        ];
        let result = find_precision_winners(&entries, 2200);
        // Both diff 100
        assert_eq!(result.winner_indices.len(), 2);
    }

    #[test]
    fn test_find_precision_winners_exact_match() {
        let entries = vec![
            PrecisionEntry {
                index: 0,
                predicted_price: 2250,
                amount: 100,
                revealed: true,
            },
            PrecisionEntry {
                index: 1,
                predicted_price: 2200,
                amount: 100,
                revealed: true,
            },
        ];
        let result = find_precision_winners(&entries, 2250);
        assert_eq!(result.winner_indices, vec![0]);
    }

    #[test]
    fn test_find_precision_winners_unrevealed_lose() {
        let entries = vec![
            PrecisionEntry {
                index: 0,
                predicted_price: 2297,
                amount: 100,
                revealed: false,
            },
            PrecisionEntry {
                index: 1,
                predicted_price: 3000,
                amount: 100,
                revealed: true,
            },
        ];
        let result = find_precision_winners(&entries, 2298);
        // Only Bob revealed, so Bob wins even though Alice was closer
        assert_eq!(result.winner_indices, vec![1]);
    }

    #[test]
    fn test_find_precision_winners_all_unrevealed() {
        let entries = vec![
            PrecisionEntry {
                index: 0,
                predicted_price: 0,
                amount: 100,
                revealed: false,
            },
            PrecisionEntry {
                index: 1,
                predicted_price: 0,
                amount: 100,
                revealed: false,
            },
        ];
        let result = find_precision_winners(&entries, 2298);
        // No winners — all refund
        assert!(result.winner_indices.is_empty());
    }

    // ─── Pot splitting tests ─────────────────────────────────────────────

    #[test]
    fn test_split_pot_even_division() {
        let payouts = split_pot_among_winners(100, 2).unwrap();
        assert_eq!(payouts, vec![50, 50]);
    }

    #[test]
    fn test_split_pot_with_remainder() {
        let payouts = split_pot_among_winners(100, 3).unwrap();
        // 100/3 = 33 remainder 1. First gets 34.
        assert_eq!(payouts, vec![34, 33, 33]);
    }

    #[test]
    fn test_split_pot_large_remainder() {
        let payouts = split_pot_among_winners(103, 5).unwrap();
        // 103/5 = 20 remainder 3. First gets 23.
        assert_eq!(payouts, vec![23, 20, 20, 20, 20]);
    }

    #[test]
    fn test_split_pot_single_winner() {
        let payouts = split_pot_among_winners(500, 1).unwrap();
        assert_eq!(payouts, vec![500]);
    }

    #[test]
    fn test_split_pot_empty() {
        let payouts = split_pot_among_winners(0, 3).unwrap();
        assert!(payouts.is_empty());
    }

    // ─── Deviation math tests ────────────────────────────────────────────

    #[test]
    fn test_deviation_bps_exact_5_percent() {
        let bps = compute_deviation_bps(1_0500000, 1_0000000).unwrap();
        assert_eq!(bps, 500);
    }

    #[test]
    fn test_deviation_bps_downward() {
        let bps = compute_deviation_bps(9500000, 1_0000000).unwrap();
        assert_eq!(bps, 500);
    }

    #[test]
    fn test_deviation_bps_no_change() {
        let bps = compute_deviation_bps(1_0000000, 1_0000000).unwrap();
        assert_eq!(bps, 0);
    }

    // ─── Median tests ────────────────────────────────────────────────────

    #[test]
    fn test_median_odd() {
        let mut prices = [300u128, 100, 200];
        sort_prices_in_place(&mut prices);
        assert_eq!(prices, [100, 200, 300]);
        assert_eq!(median_of_sorted(&prices), Some(200));
    }

    #[test]
    fn test_median_even() {
        let mut prices = [400u128, 100, 300, 200];
        sort_prices_in_place(&mut prices);
        assert_eq!(prices, [100, 200, 300, 400]);
        // median = (200+300)/2 = 250
        assert_eq!(median_of_sorted(&prices), Some(250));
    }

    #[test]
    fn test_median_single() {
        let prices = [42u128];
        assert_eq!(median_of_sorted(&prices), Some(42));
    }

    #[test]
    fn test_median_empty() {
        let prices: [u128; 0] = [];
        assert_eq!(median_of_sorted(&prices), None);
    }

    // ─── Total pot tests ─────────────────────────────────────────────────

    #[test]
    fn test_total_pot_updown() {
        assert_eq!(total_pot_updown(300, 150), 450);
    }

    #[test]
    fn test_total_pot_precision() {
        assert_eq!(total_pot_precision(&[100, 150, 50]), 300);
    }

    #[test]
    fn test_precision_scoring_mode_and_confidence_band() {
        let entries = vec![
            PrecisionEntry {
                index: 0,
                predicted_price: 10_010,
                amount: 1_000,
                revealed: true,
            },
            PrecisionEntry {
                index: 1,
                predicted_price: 10_020,
                amount: 2_000,
                revealed: true,
            },
        ];

        let abs_policy = PrecisionScoringPolicy {
            mode: PrecisionScoringMode::AbsoluteDistance,
            confidence_band: Some(15),
        };
        let res = find_precision_winners_with_policy(&entries, 10_000, abs_policy);
        assert_eq!(res.winner_indices, vec![0, 1]);

        let rel_policy = PrecisionScoringPolicy {
            mode: PrecisionScoringMode::RelativeDistance,
            confidence_band: None,
        };
        let res_rel = find_precision_winners_with_policy(&entries, 10_000, rel_policy);
        assert_eq!(res_rel.winner_indices, vec![0]);
    }

    #[test]
    fn test_split_pot_stake_weighted_math() {
        let payouts = split_pot_stake_weighted(100, &[30, 70]).unwrap();
        assert_eq!(payouts, vec![30, 70]);

        let payouts_policy = compute_precision_payouts_with_policy(
            &[
                PrecisionEntry {
                    index: 0,
                    predicted_price: 10_000,
                    amount: 30,
                    revealed: true,
                },
                PrecisionEntry {
                    index: 1,
                    predicted_price: 10_000,
                    amount: 70,
                    revealed: true,
                },
            ],
            10_000,
            None,
            PrecisionScoringPolicy {
                mode: PrecisionScoringMode::AbsoluteDistance,
                confidence_band: None,
            },
            PrecisionPayoutPolicy::StakeWeighted,
        )
        .unwrap();

        assert_eq!(payouts_policy[0].payout, 30);
        assert_eq!(payouts_policy[1].payout, 70);
    }
}
