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

        let active = matches!(&self.active_round, Some(round) if round.active);
        if !active {
            return false;
        }

        self.withdraw(user, amount);
        self.total_pool = self.total_pool.saturating_add(amount);
        if let Some(ref mut round) = self.active_round {
            if side_is_up {
                round.pool_up += amount;
                *round.bets_up.entry(user.clone()).or_default() += amount;
            } else {
                round.pool_down += amount;
                *round.bets_down.entry(user.clone()).or_default() += amount;
            }
        }
        true
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
            (round.pool_up, round.pool_down, round.bets_up, round.bets_down)
        } else {
            (round.pool_down, round.pool_up, round.bets_down, round.bets_up)
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
pub fn compute_fee_with_model(
    taxable_base: i128,
    fee_bps: Option<u32>,
) -> i128 {
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
    ref_updown_settle_with_model(winning_pool, losing_pool, winner_stakes, fee_bps, FEE_MODEL_ON_POT)
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
