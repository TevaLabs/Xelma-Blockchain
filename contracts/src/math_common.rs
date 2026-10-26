// SPDX-License-Identifier: MIT
//! Checked payout arithmetic shared by settlement math and offline replay tooling.

use crate::errors::ContractError;

pub const BPS_DENOMINATOR: i128 = 10_000;

/// Checked addition for payout accumulation.
#[inline(always)]
pub fn payout_add(a: i128, b: i128) -> Result<i128, ContractError> {
    a.checked_add(b).ok_or(ContractError::PayoutOverflow)
}

/// Checked multiplication for payout accumulation.
#[inline(always)]
pub fn payout_mul(a: i128, b: i128) -> Result<i128, ContractError> {
    a.checked_mul(b).ok_or(ContractError::PayoutOverflow)
}

/// Precision used when comparing payout ratios for closest-winner selection.
pub const PRECISION_SCALE: i128 = 1_000_000_000;

/// Scale a numerator/denominator ratio to `PRECISION_SCALE` using checked math.
///
/// Returns `None` when the denominator is zero or the scaled value overflows,
/// allowing callers to treat such candidates as non-comparable rather than
/// panicking. This mirrors the reference model's precision handling so that
/// differential replay across seeds stays in parity.
#[inline(always)]
pub fn scale_ratio(numerator: i128, denominator: i128) -> Option<i128> {
    if denominator == 0 {
        return None;
    }
    numerator
        .checked_mul(PRECISION_SCALE)
        .and_then(|scaled| scaled.checked_div(denominator))
}

/// Select the closest winner by comparing scaled ratios at fixed precision.
///
/// Ties are resolved deterministically by preferring the lower index, matching
/// the reference model's closest-winner contract. Returns `None` for an empty
/// candidate set or when no candidate yields a comparable ratio.
#[inline(always)]
pub fn closest_winner(ratios: &[(i128, i128)]) -> Option<usize> {
    let mut best: Option<(usize, i128)> = None;
    for (index, &(numerator, denominator)) in ratios.iter().enumerate() {
        let Some(scaled) = scale_ratio(numerator, denominator) else {
            continue;
        };
        match best {
            Some((_, best_scaled)) if scaled <= best_scaled => {}
            _ => best = Some((index, scaled)),
        }
    }
    best.map(|(index, _)| index)
}
