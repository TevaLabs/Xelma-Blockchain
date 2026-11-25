// SPDX-License-Identifier: MIT
#![cfg(test)]

use crate::settlement_math::{
    compute_precision_payouts_with_policy, find_precision_winners_with_policy,
    split_pot_stake_weighted, PrecisionEntry, PrecisionPayoutPolicy, PrecisionScoringMode,
    PrecisionScoringPolicy,
};
use alloc::vec;

#[test]
fn test_absolute_vs_relative_scoring_modes() {
    // Final price = 10,000 (1.0000)
    // Entry 0: 10,200 (+200 abs, 200/10000 = 200 bps relative)
    // Entry 1: 9,700 (-300 abs, 300/10000 = 300 bps relative)
    let entries = vec![
        PrecisionEntry {
            index: 0,
            predicted_price: 10_200,
            amount: 1_000,
            revealed: true,
        },
        PrecisionEntry {
            index: 1,
            predicted_price: 9_700,
            amount: 1_000,
            revealed: true,
        },
    ];

    let abs_policy = PrecisionScoringPolicy {
        mode: PrecisionScoringMode::AbsoluteDistance,
        confidence_band: None,
    };
    let abs_res = find_precision_winners_with_policy(&entries, 10_000, abs_policy);
    assert_eq!(abs_res.winner_indices, vec![0]);

    let rel_policy = PrecisionScoringPolicy {
        mode: PrecisionScoringMode::RelativeDistance,
        confidence_band: None,
    };
    let rel_res = find_precision_winners_with_policy(&entries, 10_000, rel_policy);
    assert_eq!(rel_res.winner_indices, vec![0]);
}

#[test]
fn test_confidence_band_robust_winner_sets() {
    // Final price = 10,000
    // Entry 0: 10,010 (score = 10)
    // Entry 1: 10,015 (score = 15)
    // Entry 2: 10,050 (score = 50)
    let entries = vec![
        PrecisionEntry {
            index: 0,
            predicted_price: 10_010,
            amount: 1_000,
            revealed: true,
        },
        PrecisionEntry {
            index: 1,
            predicted_price: 10_015,
            amount: 2_000,
            revealed: true,
        },
        PrecisionEntry {
            index: 2,
            predicted_price: 10_050,
            amount: 3_000,
            revealed: true,
        },
    ];

    // Without confidence band: only Entry 0 (score=10) wins
    let no_band = PrecisionScoringPolicy {
        mode: PrecisionScoringMode::AbsoluteDistance,
        confidence_band: None,
    };
    let res_no_band = find_precision_winners_with_policy(&entries, 10_000, no_band);
    assert_eq!(res_no_band.winner_indices, vec![0]);

    // With confidence band = 10 (score <= min + 10 = 20): Entry 0 & 1 win, Entry 2 loses
    let with_band = PrecisionScoringPolicy {
        mode: PrecisionScoringMode::AbsoluteDistance,
        confidence_band: Some(10),
    };
    let res_with_band = find_precision_winners_with_policy(&entries, 10_000, with_band);
    assert_eq!(res_with_band.winner_indices, vec![0, 1]);
}

#[test]
fn test_stake_weighted_split_exact_value_conservation() {
    let distributable = 1_000i128;
    let stakes = vec![300i128, 700i128];
    let payouts = split_pot_stake_weighted(distributable, &stakes).unwrap();

    // 300/1000 * 1000 = 300, 700/1000 * 1000 = 700
    assert_eq!(payouts, vec![300, 700]);
    assert_eq!(payouts.iter().sum::<i128>(), distributable);

    // Uneven remainder test: 100 distributable, stakes 100 & 100
    // 50 + 50 = 100
    let payouts_even = split_pot_stake_weighted(100, &vec![100, 100]).unwrap();
    assert_eq!(payouts_even.iter().sum::<i128>(), 100);

    // Uneven stakes: 100 distributable, stakes 33 & 67
    let payouts_uneven = split_pot_stake_weighted(100, &vec![33, 67]).unwrap();
    assert_eq!(payouts_uneven.iter().sum::<i128>(), 100);
}

#[test]
fn test_compute_precision_payouts_with_policy_stake_weighted() {
    let entries = vec![
        PrecisionEntry {
            index: 0,
            predicted_price: 10_005,
            amount: 400,
            revealed: true,
        },
        PrecisionEntry {
            index: 1,
            predicted_price: 10_008,
            amount: 600,
            revealed: true,
        },
    ];

    let scoring_policy = PrecisionScoringPolicy {
        mode: PrecisionScoringMode::AbsoluteDistance,
        confidence_band: Some(10), // Both win
    };

    let payouts = compute_precision_payouts_with_policy(
        &entries,
        10_000,
        None, // 0% fee
        scoring_policy,
        PrecisionPayoutPolicy::StakeWeighted,
    )
    .unwrap();

    assert_eq!(payouts.len(), 2);
    assert!(payouts[0].is_winner);
    assert!(payouts[1].is_winner);
    assert_eq!(payouts[0].payout, 400);
    assert_eq!(payouts[1].payout, 600);
}
