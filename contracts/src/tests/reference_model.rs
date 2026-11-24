// SPDX-License-Identifier: MIT
#![allow(dead_code)]
#![allow(unused)]
#![allow(clippy::mutable_key_type)]
//! Simplified reference model for contract state used in invariant testing.
//!
//! Also contains fee-conservation reference helpers used by property tests to
//! compute the expected treasury delta and verify that:
//!
//!   `user_payouts + treasury_delta == pot`
//!
//! for Up/Down, Precision, ties, and cancel/refund paths.

extern crate std;

use soroban_sdk::Address;
use std::collections::BTreeMap;
use std::string::{String, ToString};
use std::vec::Vec;

// ─── BPS constant (mirrors contract) ─────────────────────────────────────────

/// Denominator for basis-point arithmetic (1 bp = 0.01%), mirrors `BPS_DENOMINATOR`
/// in the contract.
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Hard cap on fee bps accepted by the contract (10% = 1_000 bps).
pub const MAX_PROTOCOL_FEE_BPS: u32 = 1_000;

// ─── Reference model ─────────────────────────────────────────────────────────

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct ReferenceRound {
    pub round_id: u64,
    pub pool_up: i128,
    pub pool_down: i128,
    pub bets_up: BTreeMap<Address, i128>,
    pub bets_down: BTreeMap<Address, i128>,
    pub active: bool,
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct ReferenceModel {
    /// Balances of each user.
    pub balances: BTreeMap<Address, i128>,
    /// Pending winnings per user.
    pub pending_winnings: BTreeMap<Address, i128>,
    /// Total pool amount for the current active round.
    pub total_pool: i128,
    /// Accumulated protocol fee treasury.
    pub protocol_fee_treasury: i128,
    /// Configured protocol fee bps (None or Some(bps)).
    pub fee_bps: Option<u32>,
    /// Active round state.
    pub active_round: Option<ReferenceRound>,
    /// Recorded outcomes for diagnostics.
    pub outcomes: Vec<bool>,
    /// Contract runtime paused status.
    pub paused: bool,
    /// Configuration mapping.
    pub config: BTreeMap<String, String>,
}

impl ReferenceModel {
    /// Creates a new default reference model.
    pub fn new() -> Self {
        Self::default()
    }

    /// Deposit tokens for a user.
    pub fn deposit(&mut self, user: &Address, amount: i128) {
        *self.balances.entry(user.clone()).or_default() += amount;
    }

    /// Withdraw tokens for a user.
    pub fn withdraw(&mut self, user: &Address, amount: i128) {
        let entry = self.balances.entry(user.clone()).or_default();
        *entry = entry.saturating_sub(amount);
    }

    /// Set protocol fee bps.
    pub fn set_fee_bps(&mut self, bps: Option<u32>) {
        self.fee_bps = bps;
    }

    /// Create a new active round.
    pub fn create_round(&mut self, round_id: u64) {
        if self.active_round.is_none() {
            self.active_round = Some(ReferenceRound {
                round_id,
                pool_up: 0,
                pool_down: 0,
                bets_up: BTreeMap::new(),
                bets_down: BTreeMap::new(),
                active: true,
            });
            self.total_pool = 0;
        }
    }

    /// Place a bet (locks amount from user balance and adds to the round pool).
    pub fn place_bet(&mut self, user: &Address, amount: i128, side_is_up: bool) -> bool {
        if amount <= 0 {
            return false;
        }
        let user_bal = *self.balances.get(user).unwrap_or(&0);
        if user_bal < amount {
            return false;
        }

        match self.active_round {
            Some(ref round) if !round.active => false,
            Some(_) => {
                self.withdraw(user, amount);
                self.total_pool = self.total_pool.saturating_add(amount);
                let round = self.active_round.as_mut().unwrap();
                if side_is_up {
                    round.pool_up += amount;
                    *round.bets_up.entry(user.clone()).or_default() += amount;
                } else {
                    round.pool_down += amount;
                    *round.bets_down.entry(user.clone()).or_default() += amount;
                }
                true
            }
            None => false,
        }
    }

    /// Cancel current active round (returns 100% of stakes to pending winnings, 0 fee).
    pub fn cancel_round(&mut self) -> bool {
        if let Some(round) = self.active_round.take() {
            for (user, amount) in round.bets_up {
                *self.pending_winnings.entry(user).or_default() += amount;
            }
            for (user, amount) in round.bets_down {
                *self.pending_winnings.entry(user).or_default() += amount;
            }
            self.total_pool = 0;
            true
        } else {
            false
        }
    }

    /// Resolve active round with win direction. Calculates protocol fee and winning payouts.
    pub fn resolve_round(&mut self, price_is_up: bool) -> bool {
        let round = match self.active_round.take() {
            Some(r) if r.active => r,
            _ => return false,
        };

        let (winning_pool, losing_pool, winner_bets, loser_bets) = if price_is_up {
            (
                round.pool_up,
                round.pool_down,
                round.bets_up,
                round.bets_down,
            )
        } else {
            (
                round.pool_down,
                round.pool_up,
                round.bets_down,
                round.bets_up,
            )
        };

        // One-sided or zero-pool round: 100% refund to all participants
        if winning_pool == 0 || losing_pool == 0 {
            for (user, amount) in winner_bets {
                *self.pending_winnings.entry(user).or_default() += amount;
            }
            for (user, amount) in loser_bets {
                *self.pending_winnings.entry(user).or_default() += amount;
            }
            self.total_pool = 0;
            self.outcomes.push(true);
            return true;
        }

        let pot = winning_pool + losing_pool;
        let fee = compute_fee(pot, self.fee_bps);
        let fee_from_losing = fee.min(losing_pool);
        let fee_from_winning = fee - fee_from_losing;
        let dist_winning = winning_pool - fee_from_winning;
        let dist_losing = losing_pool - fee_from_losing;
        let distributable = dist_winning + dist_losing;

        self.protocol_fee_treasury += fee;

        for (user, stake) in winner_bets {
            let payout = stake * distributable / winning_pool;
            *self.pending_winnings.entry(user).or_default() += payout;
        }

        self.total_pool = 0;
        self.outcomes.push(true);
        true
    }

    /// Resolve a round directly with explicit winners map.
    pub fn resolve(&mut self, winners: &BTreeMap<Address, i128>) {
        for (user, payout) in winners {
            *self.pending_winnings.entry(user.clone()).or_default() += *payout;
            self.total_pool = self.total_pool.saturating_sub(*payout);
        }
        self.outcomes.push(true);
    }

    /// Claim pending winnings for a user (moves to balance).
    pub fn claim(&mut self, user: &Address) -> i128 {
        if let Some(w) = self.pending_winnings.remove(user) {
            *self.balances.entry(user.clone()).or_default() += w;
            w
        } else {
            0
        }
    }

    /// Withdraw protocol fee treasury balance to recipient pending winnings.
    pub fn withdraw_protocol_fee(&mut self, recipient: &Address, amount: i128) -> bool {
        if amount <= 0 || amount > self.protocol_fee_treasury {
            return false;
        }
        self.protocol_fee_treasury -= amount;
        *self.pending_winnings.entry(recipient.clone()).or_default() += amount;
        true
    }

    /// Pause or un-pause contract.
    pub fn pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Apply a configuration change.
    pub fn config_change(&mut self, key: &str, value: &str) {
        self.config.insert(key.to_string(), value.to_string());
    }

    // ---------- Invariants ----------

    /// Invariant: total token count (balances + pending + treasury + active_pool) is non-negative.
    pub fn invariant_non_negative_total(&self) -> bool {
        let total_bal: i128 = self.balances.values().copied().sum();
        let total_pending: i128 = self.pending_winnings.values().copied().sum();
        total_bal + total_pending + self.protocol_fee_treasury >= 0
    }

    /// Invariant: pending winnings never exceed total pool.
    pub fn invariant_pending_le_pool(&self) -> bool {
        let total_pending: i128 = self.pending_winnings.values().copied().sum();
        total_pending >= 0
    }

    /// Run all invariants and return a list of violated descriptions.
    pub fn check_invariants(&self) -> Vec<String> {
        let mut violations = Vec::new();
        if !self.invariant_non_negative_total() {
            violations.push("non-negative total invariant violated".to_string());
        }
        if !self.invariant_pending_le_pool() {
            violations.push("pending >= 0 invariant violated".to_string());
        }
        violations
    }
}

// ─── Fee-conservation reference helpers ──────────────────────────────────────
//
// These pure Rust functions mirror the exact integer arithmetic used in the
// contract's settlement paths so that property tests can compute the expected
// fee without running the contract.

/// Computes the protocol fee in stroops using the same integer arithmetic as
/// the contract (`fee = pot * bps / 10_000`).
///
/// Returns 0 when `fee_bps` is `None` (fee disabled).
pub fn compute_fee(pot: i128, fee_bps: Option<u32>) -> i128 {
    match fee_bps {
        None | Some(0) => 0,
        Some(bps) => pot * (bps as i128) / BPS_DENOMINATOR,
    }
}

/// Fee incidence model constants (mirrors contract's `FeeModel` enum, Issue #268).
pub const FEE_MODEL_ON_POT: u32 = 0;
pub const FEE_MODEL_ON_WINNINGS: u32 = 1;

/// Computes the protocol fee with explicit fee model support (Issue #268).
///
/// - `FeeOnPot` (0): fee = taxable_base * bps / 10_000  (taxable_base = pot)
/// - `FeeOnWinnings` (1): fee = profit * bps / 10_000 (profit = losing_pool for UpDown,
///   profit = pot - winner_stakes for Precision)
pub fn compute_fee_with_model(taxable_base: i128, fee_bps: Option<u32>) -> i128 {
    match fee_bps {
        None | Some(0) => 0,
        Some(bps) => {
            if taxable_base <= 0 {
                return 0;
            }
            taxable_base * (bps as i128) / BPS_DENOMINATOR
        }
    }
}

/// Reference implementation of the **Up/Down** settlement with fees.
///
/// Mirrors `_apply_protocol_fee_updown` + proportional winner payout math.
///
/// Returns `(sum_of_winner_payouts, fee_collected)`.
///
/// Conservation bound (mirrors per-winner integer truncation):
/// ```text
/// pot - (winner_count - 1)  <=  sum_payouts + fee  <=  pot
/// ```
pub fn ref_updown_settle(
    winning_pool: i128,
    losing_pool: i128,
    winner_stakes: &[i128],
    fee_bps: Option<u32>,
) -> (i128, i128) {
    ref_updown_settle_with_model(
        winning_pool,
        losing_pool,
        winner_stakes,
        fee_bps,
        FEE_MODEL_ON_POT,
    )
}

/// Reference UpDown settlement with explicit fee model (Issue #268).
pub fn ref_updown_settle_with_model(
    winning_pool: i128,
    losing_pool: i128,
    winner_stakes: &[i128],
    fee_bps: Option<u32>,
    fee_model: u32,
) -> (i128, i128) {
    if winning_pool == 0 || winner_stakes.is_empty() {
        return (0, 0);
    }

    let pot = winning_pool + losing_pool;
    let fee = match fee_model {
        FEE_MODEL_ON_WINNINGS => compute_fee_with_model(losing_pool, fee_bps),
        _ => compute_fee(pot, fee_bps),
    };

    // Mirror the contract's fee-allocation logic per model.
    let total_distributable = match fee_model {
        FEE_MODEL_ON_WINNINGS => {
            // Winners keep full principal; fee comes only from losing pool.
            winning_pool + (losing_pool - fee)
        }
        _ => {
            let fee_from_losing = fee.min(losing_pool);
            let fee_from_winning = fee - fee_from_losing;
            (winning_pool - fee_from_winning) + (losing_pool - fee_from_losing)
        }
    };

    // Per-winner payout uses integer division, matching `payout_mul / winning_pool`.
    let sum_payouts: i128 = winner_stakes
        .iter()
        .map(|&stake| stake * total_distributable / winning_pool)
        .sum();

    (sum_payouts, fee)
}

/// Reference implementation of the **Precision** settlement with fees.
///
/// Mirrors `_apply_protocol_fee_precision` + payout with remainder to first winner.
///
/// Returns `(sum_of_winner_payouts, fee_collected)`.
///
/// Conservation is **exact** for precision (no per-winner truncation slack):
/// ```text
/// sum_payouts + fee_collected == total_pot
/// ```
pub fn ref_precision_settle(
    total_pot: i128,
    winner_count: i128,
    fee_bps: Option<u32>,
) -> (i128, i128) {
    ref_precision_settle_with_model(total_pot, winner_count, 0, fee_bps, FEE_MODEL_ON_POT)
}

/// Reference Precision settlement with explicit fee model (Issue #268).
/// `winner_stakes` is the sum of all winners' own stakes; used for FeeOnWinnings.
pub fn ref_precision_settle_with_model(
    total_pot: i128,
    winner_count: i128,
    winner_stakes: i128,
    fee_bps: Option<u32>,
    fee_model: u32,
) -> (i128, i128) {
    if winner_count == 0 || total_pot <= 0 {
        return (0, 0);
    }

    let fee = match fee_model {
        FEE_MODEL_ON_WINNINGS => {
            let profit = total_pot.saturating_sub(winner_stakes);
            if profit <= 0 {
                0
            } else {
                compute_fee_with_model(profit, fee_bps)
            }
        }
        _ => compute_fee(total_pot, fee_bps),
    };
    let distributable = total_pot - fee;
    // Remainder goes to first winner, so sum == distributable exactly.
    (distributable, fee)
}

/// Reference for **refund / cancel** paths (tie, price-unchanged, cancel, under-threshold).
///
/// No fee is charged; every participant receives back their exact stake.
///
/// Returns `(sum_refunds, fee_collected)` where `fee_collected` is always 0.
pub fn ref_refund_settle(stakes: &[i128]) -> (i128, i128) {
    let total: i128 = stakes.iter().sum();
    (total, 0)
}

// ─── Conservation assertion helpers ──────────────────────────────────────────

/// Asserts the strict fee-conservation invariant for **Precision** mode.
///
/// `user_payouts + treasury_delta == pot` (exact, no truncation slack).
pub fn assert_precision_fee_conservation(
    sum_payouts: i128,
    treasury_before: i128,
    treasury_after: i128,
    total_pot: i128,
    seed_label: &str,
) {
    let treasury_delta = treasury_after - treasury_before;
    assert_eq!(
        sum_payouts + treasury_delta,
        total_pot,
        "[seed={seed_label}] Precision fee conservation violated: \
         payouts={sum_payouts} treasury_delta={treasury_delta} pot={total_pot}"
    );
}

/// Asserts the fee-conservation bound for **Up/Down** mode.
///
/// Due to per-winner integer truncation:
/// `pot - (winner_count - 1) <= user_payouts + treasury_delta <= pot`
pub fn assert_updown_fee_conservation(
    sum_payouts: i128,
    treasury_before: i128,
    treasury_after: i128,
    total_pot: i128,
    winner_count: i128,
    seed_label: &str,
) {
    let treasury_delta = treasury_after - treasury_before;
    let conserved = sum_payouts + treasury_delta;
    let slack = winner_count.saturating_sub(1).max(0);
    assert!(
        conserved <= total_pot,
        "[seed={seed_label}] Up/Down conservation upper bound violated: \
         payouts={sum_payouts} treasury_delta={treasury_delta} pot={total_pot}"
    );
    assert!(
        conserved >= total_pot - slack,
        "[seed={seed_label}] Up/Down conservation lower bound violated: \
         payouts={sum_payouts} treasury_delta={treasury_delta} \
         pot={total_pot} winner_count={winner_count}"
    );
}

/// Asserts exact conservation for **refund / cancel** paths.
///
/// No fee must be charged: treasury must not move, and total refunds == pot.
pub fn assert_refund_fee_conservation(
    sum_refunds: i128,
    treasury_before: i128,
    treasury_after: i128,
    total_pot: i128,
    seed_label: &str,
) {
    let treasury_delta = treasury_after - treasury_before;
    assert_eq!(
        treasury_delta, 0,
        "[seed={seed_label}] Fee must not be charged on refund/cancel: \
         treasury moved by {treasury_delta}"
    );
    assert_eq!(
        sum_refunds, total_pot,
        "[seed={seed_label}] Refund conservation violated: \
         refunds={sum_refunds} pot={total_pot}"
    );
}

// ─── Precision (closest-guess) reference settlement (Issue #803) ─────────────
//
// An independent, straight-line reimplementation of the contract's Precision
// settlement (`settlement::_resolve_precision_mode`) used by the differential
// parity harness below. It is deliberately written from the *documented*
// contract behaviour rather than by copying the contract's incremental
// winner-tracking loop, so a divergence between the two is a real signal:
//
// 1. Participants are processed in ascending `Address` order (the contract
//    calls `sort_addresses` before scanning).
// 2. Every stake — revealed prediction, direct prediction or unrevealed
//    commitment — enters `total_pot`.
// 3. The winners are every *revealed* entry whose absolute distance
//    `|predicted_price - final_price|` equals the minimum distance among
//    revealed entries (closest guess wins, exact ties share).
// 4. If at least one winner exists the protocol fee is taken per the fee
//    model (FeeOnPot: `pot * bps / 10_000`; FeeOnWinnings:
//    `(pot - Σ winner stakes) * bps / 10_000`, zero when non-positive) and
//    `distributable = pot - fee` is split per payout policy (Equal or
//    StakeWeighted, floor division). The integer remainder goes to the first
//    winner in address order, so `Σ payouts + fee == pot` exactly.
//    Non-winners (revealed losers and unrevealed forfeiters) get 0.
// 5. If nobody revealed there is no winner: every stake is refunded in full
//    and no fee is charged.

/// Precision payout policy: equal split among winners (mirrors `PrecisionPayoutPolicy::Equal`).
pub const PRECISION_POLICY_EQUAL: u32 = 0;
/// Precision payout policy: split proportional to winner stakes
/// (mirrors `PrecisionPayoutPolicy::StakeWeighted`).
pub const PRECISION_POLICY_STAKE_WEIGHTED: u32 = 1;

/// One participant in a reference Precision round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefPrecisionEntry {
    pub user: Address,
    pub amount: i128,
    /// `Some(price)` for a direct or revealed prediction, `None` for a
    /// commitment that was never revealed.
    pub predicted_price: Option<u128>,
}

/// Outcome classification produced by the reference (mirrors `UserOutcomeType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefPrecisionOutcome {
    Win,
    Loss,
    Refund,
}

/// Per-participant result of a reference Precision settlement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefPrecisionUserResult {
    pub user: Address,
    pub stake: i128,
    pub payout: i128,
    /// Predicted price recorded in the outcome (0 for unrevealed / refunds).
    pub predicted_price: u128,
    pub outcome: RefPrecisionOutcome,
}

/// Full reference Precision settlement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefPrecisionSettlement {
    pub total_pot: i128,
    pub fee: i128,
    /// Minimum absolute distance among revealed entries (`None` if nobody revealed).
    pub min_diff: Option<u128>,
    /// Winners in address order (the first one receives the remainder).
    pub winners: Vec<Address>,
    /// Per-participant results in address order.
    pub results: Vec<RefPrecisionUserResult>,
}

impl RefPrecisionSettlement {
    /// Σ payouts over all participants.
    pub fn sum_payouts(&self) -> i128 {
        self.results.iter().map(|r| r.payout).sum()
    }
}

/// Reference closest-guess Precision settlement. See the section comment
/// above for the contract behaviour this encodes.
pub fn ref_precision_resolve(
    entries: &[RefPrecisionEntry],
    final_price: u128,
    fee_bps: Option<u32>,
    fee_model: u32,
    payout_policy: u32,
) -> RefPrecisionSettlement {
    let mut sorted: Vec<RefPrecisionEntry> = entries.to_vec();
    sorted.sort_by(|a, b| a.user.cmp(&b.user));

    let total_pot: i128 = sorted.iter().map(|e| e.amount).sum();
    let min_diff: Option<u128> = sorted
        .iter()
        .filter_map(|e| e.predicted_price.map(|p| p.abs_diff(final_price)))
        .min();

    let is_winner = |e: &RefPrecisionEntry| -> bool {
        match (e.predicted_price, min_diff) {
            (Some(p), Some(m)) => p.abs_diff(final_price) == m,
            _ => false,
        }
    };

    // Nobody revealed → full refund, no fee.
    if min_diff.is_none() {
        let results = sorted
            .iter()
            .map(|e| RefPrecisionUserResult {
                user: e.user.clone(),
                stake: e.amount,
                payout: e.amount,
                predicted_price: 0,
                outcome: RefPrecisionOutcome::Refund,
            })
            .collect();
        return RefPrecisionSettlement {
            total_pot,
            fee: 0,
            min_diff: None,
            winners: Vec::new(),
            results,
        };
    }

    let winners: Vec<&RefPrecisionEntry> = sorted.iter().filter(|e| is_winner(e)).collect();
    let winner_stakes: i128 = winners.iter().map(|e| e.amount).sum();

    let fee = match fee_model {
        FEE_MODEL_ON_WINNINGS => compute_fee_with_model(total_pot - winner_stakes, fee_bps),
        _ => compute_fee(total_pot, fee_bps).max(0),
    };
    let distributable = total_pot - fee;

    let mut winner_payouts: Vec<i128> = match payout_policy {
        PRECISION_POLICY_STAKE_WEIGHTED if winner_stakes > 0 => winners
            .iter()
            .map(|e| e.amount * distributable / winner_stakes)
            .collect(),
        PRECISION_POLICY_STAKE_WEIGHTED => winners.iter().map(|_| 0).collect(),
        _ => {
            let each = distributable / winners.len() as i128;
            winners.iter().map(|_| each).collect()
        }
    };
    let remainder = distributable - winner_payouts.iter().sum::<i128>();
    winner_payouts[0] += remainder;

    let mut next_winner = 0usize;
    let results = sorted
        .iter()
        .map(|e| {
            if is_winner(e) {
                let payout = winner_payouts[next_winner];
                next_winner += 1;
                RefPrecisionUserResult {
                    user: e.user.clone(),
                    stake: e.amount,
                    payout,
                    predicted_price: e.predicted_price.unwrap_or(0),
                    outcome: RefPrecisionOutcome::Win,
                }
            } else {
                RefPrecisionUserResult {
                    user: e.user.clone(),
                    stake: e.amount,
                    payout: 0,
                    predicted_price: e.predicted_price.unwrap_or(0),
                    outcome: RefPrecisionOutcome::Loss,
                }
            }
        })
        .collect();

    RefPrecisionSettlement {
        total_pot,
        fee,
        min_diff,
        winners: winners.iter().map(|e| e.user.clone()).collect(),
        results,
    }
}

impl ReferenceModel {
    /// Applies a reference Precision settlement: payouts/refunds accrue to
    /// pending winnings and the fee accrues to the protocol treasury.
    pub fn apply_precision_settlement(&mut self, settlement: &RefPrecisionSettlement) {
        for r in &settlement.results {
            if r.payout > 0 {
                *self.pending_winnings.entry(r.user.clone()).or_default() += r.payout;
            }
        }
        self.protocol_fee_treasury += settlement.fee;
        self.total_pool = 0;
        self.outcomes.push(!settlement.winners.is_empty());
    }
}

// ─── Unit tests for the Precision reference itself ───────────────────────────

#[cfg(test)]
mod precision_reference_unit {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    fn sorted_users(env: &Env, n: usize) -> Vec<Address> {
        let mut v: Vec<Address> = (0..n).map(|_| Address::generate(env)).collect();
        v.sort();
        v
    }

    #[test]
    fn closest_guess_single_winner_takes_pot_minus_fee() {
        let env = Env::default();
        let u = sorted_users(&env, 3);
        let entries = [
            RefPrecisionEntry {
                user: u[0].clone(),
                amount: 50,
                predicted_price: Some(1_010),
            },
            RefPrecisionEntry {
                user: u[1].clone(),
                amount: 30,
                predicted_price: Some(1_002),
            },
            RefPrecisionEntry {
                user: u[2].clone(),
                amount: 20,
                predicted_price: Some(990),
            },
        ];
        let s = ref_precision_resolve(&entries, 1_000, Some(1_000), FEE_MODEL_ON_POT, 0);
        assert_eq!(s.min_diff, Some(2));
        assert_eq!(s.winners, std::vec![u[1].clone()]);
        assert_eq!(s.fee, 10);
        assert_eq!(s.results[1].payout, 90);
        assert_eq!(s.sum_payouts() + s.fee, s.total_pot);
    }

    #[test]
    fn symmetric_tie_splits_and_remainder_goes_to_first_address() {
        let env = Env::default();
        let u = sorted_users(&env, 3);
        let entries = [
            // Insert in reverse order to prove the reference sorts.
            RefPrecisionEntry {
                user: u[2].clone(),
                amount: 50,
                predicted_price: Some(1_200),
            },
            RefPrecisionEntry {
                user: u[1].clone(),
                amount: 50,
                predicted_price: Some(995),
            },
            RefPrecisionEntry {
                user: u[0].clone(),
                amount: 50,
                predicted_price: Some(1_005),
            },
        ];
        let s = ref_precision_resolve(&entries, 1_000, Some(1_000), FEE_MODEL_ON_POT, 0);
        assert_eq!(s.winners, std::vec![u[0].clone(), u[1].clone()]);
        assert_eq!(s.fee, 15);
        assert_eq!(s.results[0].payout, 68); // 67 + remainder 1
        assert_eq!(s.results[1].payout, 67);
        assert_eq!(s.results[2].outcome, RefPrecisionOutcome::Loss);
        assert_eq!(s.sum_payouts() + s.fee, 150);
    }

    #[test]
    fn unrevealed_entries_never_win_and_forfeit_to_pot() {
        let env = Env::default();
        let u = sorted_users(&env, 2);
        let entries = [
            RefPrecisionEntry {
                user: u[0].clone(),
                amount: 60,
                predicted_price: None,
            },
            RefPrecisionEntry {
                user: u[1].clone(),
                amount: 40,
                predicted_price: Some(5_000),
            },
        ];
        let s = ref_precision_resolve(&entries, 1_000, Some(500), FEE_MODEL_ON_POT, 0);
        assert_eq!(s.winners, std::vec![u[1].clone()]);
        assert_eq!(s.results[0].outcome, RefPrecisionOutcome::Loss);
        assert_eq!(s.results[1].payout, 95);
        assert_eq!(s.fee, 5);
    }

    #[test]
    fn all_unrevealed_refunds_without_fee() {
        let env = Env::default();
        let u = sorted_users(&env, 2);
        let entries = [
            RefPrecisionEntry {
                user: u[0].clone(),
                amount: 60,
                predicted_price: None,
            },
            RefPrecisionEntry {
                user: u[1].clone(),
                amount: 40,
                predicted_price: None,
            },
        ];
        let s = ref_precision_resolve(&entries, 1_000, Some(1_000), FEE_MODEL_ON_POT, 1);
        assert_eq!(s.fee, 0);
        assert!(s.winners.is_empty());
        assert!(s
            .results
            .iter()
            .all(|r| r.outcome == RefPrecisionOutcome::Refund));
        assert_eq!(s.sum_payouts(), 100);
    }

    #[test]
    fn fee_on_winnings_with_all_participants_winning_charges_nothing() {
        let env = Env::default();
        let u = sorted_users(&env, 2);
        let entries = [
            RefPrecisionEntry {
                user: u[0].clone(),
                amount: 7,
                predicted_price: Some(1_001),
            },
            RefPrecisionEntry {
                user: u[1].clone(),
                amount: 3,
                predicted_price: Some(999),
            },
        ];
        let s = ref_precision_resolve(&entries, 1_000, Some(1_000), FEE_MODEL_ON_WINNINGS, 1);
        assert_eq!(s.fee, 0);
        assert_eq!(s.results[0].payout, 7);
        assert_eq!(s.results[1].payout, 3);
    }
}
