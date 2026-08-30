use super::*;
use alloc::vec;

// ============================================================================
// GOLDEN VECTOR TESTS — Pure settlement_math verification (Issue #257)
// ============================================================================
// These tests verify settlement_math functions with known inputs and expected
// outputs. They do NOT require the Soroban test harness — only std::prelude.

// ─── Price direction golden vectors ─────────────────────────────────────────

#[test]
fn golden_price_direction_up() {
    assert_eq!(
        classify_price_direction(1_0000000, 1_5000000),
        PriceDirection::Up
    );
}

#[test]
fn golden_price_direction_down() {
    assert_eq!(
        classify_price_direction(2_0000000, 1_0000000),
        PriceDirection::Down
    );
}

#[test]
fn golden_price_direction_unchanged() {
    assert_eq!(
        classify_price_direction(1_0000000, 1_0000000),
        PriceDirection::Unchanged
    );
}

#[test]
fn golden_price_direction_large_values() {
    assert_eq!(
        classify_price_direction(100_0000000, 200_0000000),
        PriceDirection::Up
    );
    assert_eq!(
        classify_price_direction(200_0000000, 100_0000000),
        PriceDirection::Down
    );
}

// ─── One-sided pool golden vectors ──────────────────────────────────────────

#[test]
fn golden_is_one_sided_only_up() {
    assert!(is_one_sided_pool(100, 0));
}

#[test]
fn golden_is_one_sided_only_down() {
    assert!(is_one_sided_pool(0, 100));
}

#[test]
fn golden_not_one_sided_both_filled() {
    assert!(!is_one_sided_pool(100, 50));
}

#[test]
fn golden_not_one_sided_both_empty() {
    assert!(!is_one_sided_pool(0, 0));
}

// ─── Fee math golden vectors ────────────────────────────────────────────────

#[test]
fn golden_updown_fee_1pct_conservation() {
    let (dw, dl, fee) = compute_updown_fee(300, 150, Some(100)).unwrap();
    assert_eq!(fee, 4);
    assert_eq!(dl, 146);
    assert_eq!(dw, 300);
    assert_eq!(dw + dl + fee, 450);
}

#[test]
fn golden_updown_fee_spillover_from_winning() {
    let (dw, dl, fee) = compute_updown_fee(1000, 10, Some(500)).unwrap();
    assert_eq!(fee, 50);
    assert_eq!(dl, 0);
    assert_eq!(dw, 960);
    assert_eq!(dw + dl + fee, 1010);
}

#[test]
fn golden_updown_fee_zero_bps_is_noop() {
    let (dw, dl, fee) = compute_updown_fee(300, 150, Some(0)).unwrap();
    assert_eq!(dw, 300);
    assert_eq!(dl, 150);
    assert_eq!(fee, 0);
}

#[test]
fn golden_updown_fee_none_is_noop() {
    let (dw, dl, fee) = compute_updown_fee(300, 150, None).unwrap();
    assert_eq!(dw, 300);
    assert_eq!(dl, 150);
    assert_eq!(fee, 0);
}

#[test]
fn golden_precision_fee_2pct() {
    let (dist, fee) = compute_precision_fee(1000, Some(200)).unwrap();
    assert_eq!(fee, 20);
    assert_eq!(dist, 980);
    assert_eq!(dist + fee, 1000);
}

#[test]
fn golden_precision_fee_none() {
    let (dist, fee) = compute_precision_fee(500, None).unwrap();
    assert_eq!(fee, 0);
    assert_eq!(dist, 500);
}

#[test]
fn golden_precision_fee_zero_pot() {
    let (dist, fee) = compute_precision_fee(0, Some(100)).unwrap();
    assert_eq!(fee, 0);
    assert_eq!(dist, 0);
}

#[test]
fn golden_precision_fee_negative_pot() {
    let (dist, fee) = compute_precision_fee(-10, Some(100)).unwrap();
    assert_eq!(fee, 0);
    assert_eq!(dist, -10);
}

// ─── Deviation math golden vectors ──────────────────────────────────────────

#[test]
fn golden_deviation_5pct_up() {
    let bps = compute_deviation_bps(1_0500000, 1_0000000).unwrap();
    assert_eq!(bps, 500);
}

#[test]
fn golden_deviation_5pct_down() {
    let bps = compute_deviation_bps(9500000, 1_0000000).unwrap();
    assert_eq!(bps, 500);
}

#[test]
fn golden_deviation_10pct() {
    let bps = compute_deviation_bps(1_1000000, 1_0000000).unwrap();
    assert_eq!(bps, 1000);
}

#[test]
fn golden_deviation_exact_zero() {
    let bps = compute_deviation_bps(1_0000000, 1_0000000).unwrap();
    assert_eq!(bps, 0);
}

#[test]
fn golden_deviation_tiny() {
    let bps = compute_deviation_bps(1_0000100, 1_0000000).unwrap();
    assert_eq!(bps, 0);
}

// ─── Total pot golden vectors ───────────────────────────────────────────────

#[test]
fn golden_total_pot_updown() {
    assert_eq!(total_pot_updown(300, 150), 450);
    assert_eq!(total_pot_updown(0, 0), 0);
    assert_eq!(total_pot_updown(1_000_000, 500_000), 1_500_000);
}

// ─── UpDown payout golden vectors ───────────────────────────────────────────

#[test]
fn golden_updown_price_up_two_winners() {
    let positions = vec![
        UpDownPosition { index: 0, amount: 100, side_up: true },
        UpDownPosition { index: 1, amount: 200, side_up: true },
        UpDownPosition { index: 2, amount: 150, side_up: false },
    ];
    let results = compute_updown_payouts(
        &positions, 1_0000000, 1_5000000, 300, 150, None,
    )
    .unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].payout, 150);
    assert!(results[0].is_winner);
    assert!(!results[0].is_refund);
    assert_eq!(results[1].payout, 300);
    assert!(results[1].is_winner);
    assert_eq!(results[2].payout, 0);
    assert!(!results[2].is_winner);
    assert_eq!(results[0].payout + results[1].payout + results[2].payout, 450);
}

#[test]
fn golden_updown_price_down_single_winner() {
    let positions = vec![
        UpDownPosition { index: 0, amount: 200, side_up: false },
        UpDownPosition { index: 1, amount: 100, side_up: true },
    ];
    let results = compute_updown_payouts(
        &positions, 2_0000000, 1_0000000, 100, 200, None,
    )
    .unwrap();

    assert_eq!(results[0].payout, 300);
    assert!(results[0].is_winner);
    assert_eq!(results[1].payout, 0);
    assert!(!results[1].is_winner);
    assert_eq!(results[0].payout + results[1].payout, 300);
}

#[test]
fn golden_updown_price_unchanged_refunds_all() {
    let positions = vec![
        UpDownPosition { index: 0, amount: 100, side_up: true },
        UpDownPosition { index: 1, amount: 50, side_up: false },
    ];
    let results = compute_updown_payouts(
        &positions, 1_0000000, 1_0000000, 100, 50, None,
    )
    .unwrap();

    assert_eq!(results[0].payout, 100);
    assert!(results[0].is_refund);
    assert_eq!(results[1].payout, 50);
    assert!(results[1].is_refund);
    assert_eq!(results[0].payout + results[1].payout, 150);
}

#[test]
fn golden_updown_one_sided_refunds_all() {
    let positions = vec![
        UpDownPosition { index: 0, amount: 100, side_up: true },
        UpDownPosition { index: 1, amount: 200, side_up: true },
    ];
    let results = compute_updown_payouts(
        &positions, 1_0000000, 1_5000000, 300, 0, None,
    )
    .unwrap();

    assert_eq!(results[0].payout, 100);
    assert!(results[0].is_refund);
    assert_eq!(results[1].payout, 200);
    assert!(results[1].is_refund);
}

#[test]
fn golden_updown_with_1pct_fee() {
    let positions = vec![
        UpDownPosition { index: 0, amount: 100, side_up: true },
        UpDownPosition { index: 1, amount: 150, side_up: false },
    ];
    let results = compute_updown_payouts(
        &positions, 1_0000000, 1_5000000, 300, 150, Some(100),
    )
    .unwrap();

    assert_eq!(results[0].payout, 148);
    assert!(results[0].is_winner);
    assert_eq!(results[1].payout, 0);
}

#[test]
fn golden_updown_empty_winning_pool_refunds() {
    let positions = vec![
        UpDownPosition { index: 0, amount: 100, side_up: false },
        UpDownPosition { index: 1, amount: 50, side_up: false },
    ];
    let results = compute_updown_payouts(
        &positions, 1_0000000, 1_5000000, 0, 150, None,
    )
    .unwrap();

    assert_eq!(results[0].payout, 100);
    assert!(results[0].is_refund);
    assert_eq!(results[1].payout, 50);
    assert!(results[1].is_refund);
}

// ─── Precision winner determination golden vectors ──────────────────────────

#[test]
fn golden_precision_winners_single_clear_winner() {
    let entries = vec![
        PrecisionEntry { index: 0, predicted_price: 2297, amount: 100, revealed: true },
        PrecisionEntry { index: 1, predicted_price: 2300, amount: 150, revealed: true },
        PrecisionEntry { index: 2, predicted_price: 2500, amount: 50, revealed: true },
    ];
    let result = find_precision_winners(&entries, 2298);
    assert_eq!(result.winner_indices, vec![0]);
    assert_eq!(result.total_pot, 300);
    assert!(result.loser_indices.contains(&1));
    assert!(result.loser_indices.contains(&2));
}

#[test]
fn golden_precision_winners_two_way_tie() {
    let entries = vec![
        PrecisionEntry { index: 0, predicted_price: 2100, amount: 100, revealed: true },
        PrecisionEntry { index: 1, predicted_price: 2300, amount: 150, revealed: true },
    ];
    let result = find_precision_winners(&entries, 2200);
    assert_eq!(result.winner_indices.len(), 2);
    assert_eq!(result.total_pot, 250);
}

#[test]
fn golden_precision_winners_exact_match() {
    let entries = vec![
        PrecisionEntry { index: 0, predicted_price: 2250, amount: 100, revealed: true },
        PrecisionEntry { index: 1, predicted_price: 2200, amount: 100, revealed: true },
    ];
    let result = find_precision_winners(&entries, 2250);
    assert_eq!(result.winner_indices, vec![0]);
}

#[test]
fn golden_precision_winners_unrevealed_loses() {
    let entries = vec![
        PrecisionEntry { index: 0, predicted_price: 2297, amount: 100, revealed: false },
        PrecisionEntry { index: 1, predicted_price: 4000, amount: 100, revealed: true },
    ];
    let result = find_precision_winners(&entries, 2298);
    assert_eq!(result.winner_indices, vec![1]);
}

#[test]
fn golden_precision_winners_all_unrevealed() {
    let entries = vec![
        PrecisionEntry { index: 0, predicted_price: 0, amount: 100, revealed: false },
        PrecisionEntry { index: 1, predicted_price: 0, amount: 50, revealed: false },
    ];
    let result = find_precision_winners(&entries, 2298);
    assert!(result.winner_indices.is_empty());
    assert_eq!(result.total_pot, 150);
}

#[test]
fn golden_precision_winners_empty() {
    let entries: alloc::vec::Vec<PrecisionEntry> = vec![];
    let result = find_precision_winners(&entries, 2298);
    assert!(result.winner_indices.is_empty());
    assert_eq!(result.total_pot, 0);
}

// ─── Pot splitting golden vectors ───────────────────────────────────────────

#[test]
fn golden_split_pot_even() {
    let payouts = split_pot_among_winners(100, 2).unwrap();
    assert_eq!(payouts, vec![50, 50]);
}

#[test]
fn golden_split_pot_remainder_to_first() {
    let payouts = split_pot_among_winners(100, 3).unwrap();
    assert_eq!(payouts, vec![34, 33, 33]);
    assert_eq!(payouts.iter().sum::<i128>(), 100);
}

#[test]
fn golden_split_pot_large_remainder() {
    let payouts = split_pot_among_winners(103, 5).unwrap();
    assert_eq!(payouts, vec![23, 20, 20, 20, 20]);
    assert_eq!(payouts.iter().sum::<i128>(), 103);
}

#[test]
fn golden_split_pot_single_winner() {
    let payouts = split_pot_among_winners(500, 1).unwrap();
    assert_eq!(payouts, vec![500]);
}

#[test]
fn golden_split_pot_zero_pot() {
    let payouts = split_pot_among_winners(0, 3).unwrap();
    assert!(payouts.is_empty());
}

#[test]
fn golden_split_pot_zero_winners() {
    let payouts = split_pot_among_winners(100, 0).unwrap();
    assert!(payouts.is_empty());
}

// ─── Composite Precision payout golden vectors ──────────────────────────────

#[test]
fn golden_precision_payouts_single_winner_no_fee() {
    let entries = vec![
        PrecisionEntry { index: 0, predicted_price: 2297, amount: 100, revealed: true },
        PrecisionEntry { index: 1, predicted_price: 2300, amount: 150, revealed: true },
        PrecisionEntry { index: 2, predicted_price: 2500, amount: 50, revealed: true },
    ];
    let results = compute_precision_payouts(&entries, 2298, None).unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].payout, 300);
    assert!(results[0].is_winner);
    assert_eq!(results[1].payout, 0);
    assert!(!results[1].is_winner);
    assert_eq!(results[2].payout, 0);
    assert!(!results[2].is_winner);
    assert_eq!(results.iter().map(|r| r.payout).sum::<i128>(), 300);
}

#[test]
fn golden_precision_payouts_two_way_tie_no_fee() {
    let entries = vec![
        PrecisionEntry { index: 0, predicted_price: 2100, amount: 100, revealed: true },
        PrecisionEntry { index: 1, predicted_price: 2300, amount: 150, revealed: true },
        PrecisionEntry { index: 2, predicted_price: 2500, amount: 50, revealed: true },
    ];
    let results = compute_precision_payouts(&entries, 2200, None).unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].payout, 150);
    assert!(results[0].is_winner);
    assert_eq!(results[1].payout, 150);
    assert!(results[1].is_winner);
    assert_eq!(results[2].payout, 0);
    assert!(!results[2].is_winner);
    assert_eq!(
        results.iter().map(|r| r.payout).sum::<i128>(),
        300
    );
}

#[test]
fn golden_precision_payouts_all_unrevealed_refunds() {
    let entries = vec![
        PrecisionEntry { index: 0, predicted_price: 0, amount: 100, revealed: false },
        PrecisionEntry { index: 1, predicted_price: 0, amount: 50, revealed: false },
    ];
    let results = compute_precision_payouts(&entries, 2298, None).unwrap();

    assert_eq!(results[0].payout, 100);
    assert!(results[0].is_refund);
    assert_eq!(results[1].payout, 50);
    assert!(results[1].is_refund);
    assert_eq!(results.iter().map(|r| r.payout).sum::<i128>(), 150);
}

#[test]
fn golden_precision_payouts_with_1pct_fee() {
    let entries = vec![
        PrecisionEntry { index: 0, predicted_price: 2250, amount: 100, revealed: true },
        PrecisionEntry { index: 1, predicted_price: 2200, amount: 100, revealed: true },
    ];
    let results = compute_precision_payouts(&entries, 2250, Some(100)).unwrap();

    assert_eq!(results[0].payout, 198);
    assert!(results[0].is_winner);
    assert_eq!(results[1].payout, 0);
    assert_eq!(results.iter().map(|r| r.payout).sum::<i128>(), 198);
}

#[test]
fn golden_precision_payouts_empty() {
    let entries: alloc::vec::Vec<PrecisionEntry> = vec![];
    let results = compute_precision_payouts(&entries, 2298, None).unwrap();
    assert!(results.is_empty());
}

// ─── Conservation invariant: UpDown ─────────────────────────────────────────

#[test]
fn golden_updown_conservation_invariant() {
    let scenarios = vec![
        (300, 150, 1_0000000u128, 1_5000000u128, None),
        (300, 150, 1_0000000u128, 1_5000000u128, Some(100u32)),
        (100, 200, 2_0000000u128, 1_0000000u128, None),
        (100, 200, 2_0000000u128, 1_0000000u128, Some(500u32)),
        (500, 500, 1_0000000u128, 1_0000000u128, None),
        (500, 500, 1_0000000u128, 1_5000000u128, Some(50u32)),
        (0, 100, 1_0000000u128, 1_5000000u128, None),
        (100, 0, 1_0000000u128, 5000000u128, None),
        (1, 1000, 1_0000000u128, 1_5000000u128, Some(50u32)),
        (1000, 1, 1_0000000u128, 5000000u128, Some(100u32)),
    ];

    for (pool_up, pool_down, start, final_price, fee_bps) in &scenarios {
        let direction = classify_price_direction(*start, *final_price);
        let one_sided = is_one_sided_pool(*pool_up, *pool_down);

        let positions = vec![
            UpDownPosition { index: 0, amount: *pool_up, side_up: true },
            UpDownPosition { index: 1, amount: *pool_down, side_up: false },
        ];
        let results =
            compute_updown_payouts(&positions, *start, *final_price, *pool_up, *pool_down, *fee_bps)
                .unwrap();

        let sum_payouts: i128 = results.iter().map(|r| r.payout).sum();

        if direction == PriceDirection::Unchanged || one_sided || {
            let wp = if direction == PriceDirection::Up { *pool_up } else { *pool_down };
            wp == 0
        } {
            assert_eq!(
                sum_payouts,
                *pool_up + *pool_down,
                "Refund scenario: conservation failed for ({}, {}, {}, {})",
                pool_up, pool_down, start, final_price
            );
        } else {
            let (_, _, fee) = compute_updown_fee(
                if direction == PriceDirection::Up { *pool_up } else { *pool_down },
                if direction == PriceDirection::Up { *pool_down } else { *pool_up },
                *fee_bps,
            )
            .unwrap();
            assert_eq!(
                sum_payouts + fee,
                *pool_up + *pool_down,
                "Competitive scenario: conservation failed"
            );
        }
    }
}

// ─── Conservation invariant: Precision ──────────────────────────────────────

#[test]
fn golden_precision_conservation_invariant() {
    let scenarios: alloc::vec::Vec<(alloc::vec::Vec<PrecisionEntry>, u128, Option<u32>)> = vec![
        (
            vec![
                PrecisionEntry { index: 0, predicted_price: 100, amount: 200, revealed: true },
                PrecisionEntry { index: 1, predicted_price: 300, amount: 100, revealed: true },
            ],
            100, None,
        ),
        (
            vec![
                PrecisionEntry { index: 0, predicted_price: 2100, amount: 100, revealed: true },
                PrecisionEntry { index: 1, predicted_price: 2300, amount: 150, revealed: true },
            ],
            2200, Some(100),
        ),
        (
            vec![
                PrecisionEntry { index: 0, predicted_price: 0, amount: 50, revealed: false },
                PrecisionEntry { index: 1, predicted_price: 0, amount: 100, revealed: false },
            ],
            2298, None,
        ),
        (
            vec![
                PrecisionEntry { index: 0, predicted_price: 2297, amount: 100, revealed: true },
                PrecisionEntry { index: 1, predicted_price: 0, amount: 200, revealed: false },
            ],
            2298, None,
        ),
        (vec![], 2298, None),
    ];

    for (entries, final_price, fee_bps) in &scenarios {
        let results = compute_precision_payouts(entries, *final_price, *fee_bps).unwrap();
        let sum_payouts: i128 = results.iter().map(|r| r.payout).sum();
        let total_stakes: i128 = entries.iter().map(|e| e.amount).sum();

        if total_stakes > 0 {
            assert!(
                sum_payouts <= total_stakes,
                "Precision payouts exceed total stakes: {} > {}",
                sum_payouts, total_stakes
            );
            for r in &results {
                assert!(r.payout >= 0, "Negative payout detected");
            }
        } else {
            assert_eq!(sum_payouts, 0);
        }
    }
}
