// SPDX-License-Identifier: MIT
//! Loads the checked-in golden vectors from `contracts/test_vectors/` and
//! verifies `settlement_math` against them (Issue #412, Issue #404).
//!
//! Unlike `tests::resolution::golden` (inline literal expectations, useful
//! for reviewing the math alongside its test), this module treats
//! `contracts/test_vectors/settlement_math.json` as the source of truth: the
//! file is checked into the repo, diffable in review, and can be regenerated
//! from the current implementation with one command (see
//! `contracts/test_vectors/README.md`). CI runs this module on every
//! `cargo test`, so any unintentional drift in `settlement_math`'s output
//! fails the build.
//!
//! The `precision_remainder_ordering` section specifically pins the rule
//! that the indivisible remainder of a Precision-mode payout split always
//! goes to the first winner in canonical (address-sorted) order — see
//! `PROTOCOL_SPEC.md` §"Precision mode" and
//! `tests::resolution::precision::test_precision_remainder_goes_to_lexicographically_lowest_winner`
//! for the end-to-end contract-level proof of the same rule.

use crate::settlement_math::{
    compute_deviation_bps, compute_precision_fee, compute_precision_payouts, compute_updown_fee,
    compute_updown_payouts, find_precision_winners, split_pot_among_winners, PrecisionEntry,
    UpDownPosition,
};
use serde::{Deserialize, Serialize};
use std::string::String;
use std::vec::Vec as StdVec;

const VECTORS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/test_vectors/settlement_math.json"
);
const VECTORS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/test_vectors/settlement_math.json"
));

#[derive(Deserialize, Serialize)]
struct VectorFile {
    #[serde(rename = "$schema_note")]
    schema_note: String,
    updown_fee: StdVec<UpdownFeeCase>,
    precision_fee: StdVec<PrecisionFeeCase>,
    updown_payouts: StdVec<UpdownPayoutsCase>,
    precision_winners: StdVec<PrecisionWinnersCase>,
    split_pot_among_winners: StdVec<SplitPotCase>,
    deviation_bps: StdVec<DeviationCase>,
    precision_remainder_ordering: StdVec<RemainderOrderingCase>,
}

#[derive(Deserialize, Serialize)]
struct UpdownFeeCase {
    name: String,
    winning_pool: i128,
    losing_pool: i128,
    fee_bps: Option<u32>,
    expected_dist_winning: i128,
    expected_dist_losing: i128,
    expected_fee: i128,
}

#[derive(Deserialize, Serialize)]
struct PrecisionFeeCase {
    name: String,
    total_pot: i128,
    fee_bps: Option<u32>,
    expected_distributable: i128,
    expected_fee: i128,
}

#[derive(Deserialize, Serialize)]
struct JsonPosition {
    amount: i128,
    side_up: bool,
}

#[derive(Deserialize, Serialize)]
struct JsonUpDownExpectation {
    payout: i128,
    is_winner: bool,
    is_refund: bool,
}

#[derive(Deserialize, Serialize)]
struct UpdownPayoutsCase {
    name: String,
    start_price: u128,
    final_price: u128,
    pool_up: i128,
    pool_down: i128,
    fee_bps: Option<u32>,
    positions: StdVec<JsonPosition>,
    expected: StdVec<JsonUpDownExpectation>,
}

#[derive(Deserialize, Serialize, Clone)]
struct JsonPrecisionEntry {
    predicted_price: u128,
    amount: i128,
    revealed: bool,
}

#[derive(Deserialize, Serialize)]
struct PrecisionWinnersCase {
    name: String,
    final_price: u128,
    entries: StdVec<JsonPrecisionEntry>,
    expected_winner_indices: StdVec<usize>,
    expected_total_pot: i128,
}

#[derive(Deserialize, Serialize)]
struct SplitPotCase {
    name: String,
    distributable: i128,
    winner_count: usize,
    expected: StdVec<i128>,
}

#[derive(Deserialize, Serialize)]
struct DeviationCase {
    name: String,
    price: u128,
    reference: u128,
    expected_bps: u32,
}

#[derive(Deserialize, Serialize)]
struct RemainderOrderingCase {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    final_price: u128,
    entries: StdVec<JsonPrecisionEntry>,
    expected_payouts: StdVec<i128>,
}

fn load_vectors() -> VectorFile {
    serde_json::from_str(VECTORS_JSON)
        .expect("contracts/test_vectors/settlement_math.json must parse as VectorFile")
}

fn to_precision_entries(entries: &[JsonPrecisionEntry]) -> StdVec<PrecisionEntry> {
    entries
        .iter()
        .enumerate()
        .map(|(index, e)| PrecisionEntry {
            index,
            predicted_price: e.predicted_price,
            amount: e.amount,
            revealed: e.revealed,
        })
        .collect()
}

#[test]
fn golden_vectors_updown_fee() {
    for case in load_vectors().updown_fee {
        let (dw, dl, fee) = compute_updown_fee(case.winning_pool, case.losing_pool, case.fee_bps)
            .unwrap_or_else(|e| {
                panic!("case '{}': compute_updown_fee errored: {:?}", case.name, e)
            });
        assert_eq!(
            dw, case.expected_dist_winning,
            "case '{}': dist_winning",
            case.name
        );
        assert_eq!(
            dl, case.expected_dist_losing,
            "case '{}': dist_losing",
            case.name
        );
        assert_eq!(fee, case.expected_fee, "case '{}': fee", case.name);
    }
}

#[test]
fn golden_vectors_precision_fee() {
    for case in load_vectors().precision_fee {
        let (dist, fee) = compute_precision_fee(case.total_pot, case.fee_bps).unwrap_or_else(|e| {
            panic!(
                "case '{}': compute_precision_fee errored: {:?}",
                case.name, e
            )
        });
        assert_eq!(
            dist, case.expected_distributable,
            "case '{}': distributable",
            case.name
        );
        assert_eq!(fee, case.expected_fee, "case '{}': fee", case.name);
    }
}

#[test]
fn golden_vectors_updown_payouts() {
    for case in load_vectors().updown_payouts {
        let positions: StdVec<UpDownPosition> = case
            .positions
            .iter()
            .enumerate()
            .map(|(index, p)| UpDownPosition {
                index,
                amount: p.amount,
                side_up: p.side_up,
            })
            .collect();
        let results = compute_updown_payouts(
            &positions,
            case.start_price,
            case.final_price,
            case.pool_up,
            case.pool_down,
            case.fee_bps,
        )
        .unwrap_or_else(|e| {
            panic!(
                "case '{}': compute_updown_payouts errored: {:?}",
                case.name, e
            )
        });

        assert_eq!(
            results.len(),
            case.expected.len(),
            "case '{}': result count",
            case.name
        );
        for (i, (got, want)) in results.iter().zip(case.expected.iter()).enumerate() {
            assert_eq!(
                got.payout, want.payout,
                "case '{}' entry {}: payout",
                case.name, i
            );
            assert_eq!(
                got.is_winner, want.is_winner,
                "case '{}' entry {}: is_winner",
                case.name, i
            );
            assert_eq!(
                got.is_refund, want.is_refund,
                "case '{}' entry {}: is_refund",
                case.name, i
            );
        }
    }
}

#[test]
fn golden_vectors_precision_winners() {
    for case in load_vectors().precision_winners {
        let entries = to_precision_entries(&case.entries);
        let result = find_precision_winners(&entries, case.final_price);
        assert_eq!(
            result.winner_indices, case.expected_winner_indices,
            "case '{}': winner_indices",
            case.name
        );
        assert_eq!(
            result.total_pot, case.expected_total_pot,
            "case '{}': total_pot",
            case.name
        );
    }
}

#[test]
fn golden_vectors_split_pot_among_winners() {
    for case in load_vectors().split_pot_among_winners {
        let payouts = split_pot_among_winners(case.distributable, case.winner_count)
            .unwrap_or_else(|e| {
                panic!(
                    "case '{}': split_pot_among_winners errored: {:?}",
                    case.name, e
                )
            });
        assert_eq!(payouts, case.expected, "case '{}': payouts", case.name);
    }
}

#[test]
fn golden_vectors_deviation_bps() {
    for case in load_vectors().deviation_bps {
        let bps = compute_deviation_bps(case.price, case.reference).unwrap_or_else(|e| {
            panic!(
                "case '{}': compute_deviation_bps errored: {:?}",
                case.name, e
            )
        });
        assert_eq!(bps, case.expected_bps, "case '{}': bps", case.name);
    }
}

/// Proves the remainder-to-first-winner rule from `split_pot_among_winners`
/// composes correctly through the full `compute_precision_payouts` pipeline
/// (winner-finding + fee + split), given entries in canonical
/// (address-sorted) order — see the module doc comment.
#[test]
fn golden_vectors_precision_remainder_ordering() {
    for case in load_vectors().precision_remainder_ordering {
        let entries = to_precision_entries(&case.entries);
        let results =
            compute_precision_payouts(&entries, case.final_price, None).unwrap_or_else(|e| {
                panic!(
                    "case '{}': compute_precision_payouts errored: {:?}",
                    case.name, e
                )
            });

        let got_payouts: StdVec<i128> = results.iter().map(|r| r.payout).collect();
        assert_eq!(
            got_payouts, case.expected_payouts,
            "case '{}': payouts",
            case.name
        );

        // Conservation: every stroop staked is accounted for exactly once.
        let total_staked: i128 = case.entries.iter().map(|e| e.amount).sum();
        let total_paid: i128 = got_payouts.iter().sum();
        assert_eq!(
            total_paid, total_staked,
            "case '{}': conservation (paid must equal staked, no fee configured)",
            case.name
        );
    }
}

/// Not a check — overwrites `contracts/test_vectors/settlement_math.json`
/// with every `expected_*`/`expected`/`expected_payouts` field recomputed
/// from the *current* `settlement_math` implementation, keeping each case's
/// declared inputs (`name`, `note`, and arguments) unchanged. Run explicitly
/// and deliberately:
///
/// ```sh
/// cargo test --package xelma-contract --lib \
///   tests::settlement_math_vectors::regenerate_vectors_file \
///   --features testutils -- --ignored --nocapture
/// ```
///
/// Then review the resulting `git diff` carefully before committing — an
/// unreviewed regenerate silently launders a real behavior change into
/// "expected".
#[test]
#[ignore = "overwrites contracts/test_vectors/settlement_math.json; run deliberately, see doc comment"]
fn regenerate_vectors_file() {
    let mut vectors = load_vectors();

    for c in &mut vectors.updown_fee {
        let (dw, dl, fee) = compute_updown_fee(c.winning_pool, c.losing_pool, c.fee_bps).unwrap();
        c.expected_dist_winning = dw;
        c.expected_dist_losing = dl;
        c.expected_fee = fee;
    }

    for c in &mut vectors.precision_fee {
        let (dist, fee) = compute_precision_fee(c.total_pot, c.fee_bps).unwrap();
        c.expected_distributable = dist;
        c.expected_fee = fee;
    }

    for c in &mut vectors.updown_payouts {
        let positions: StdVec<UpDownPosition> = c
            .positions
            .iter()
            .enumerate()
            .map(|(index, p)| UpDownPosition {
                index,
                amount: p.amount,
                side_up: p.side_up,
            })
            .collect();
        let results = compute_updown_payouts(
            &positions,
            c.start_price,
            c.final_price,
            c.pool_up,
            c.pool_down,
            c.fee_bps,
        )
        .unwrap();
        c.expected = results
            .into_iter()
            .map(|r| JsonUpDownExpectation {
                payout: r.payout,
                is_winner: r.is_winner,
                is_refund: r.is_refund,
            })
            .collect();
    }

    for c in &mut vectors.precision_winners {
        let entries = to_precision_entries(&c.entries);
        let result = find_precision_winners(&entries, c.final_price);
        c.expected_winner_indices = result.winner_indices;
        c.expected_total_pot = result.total_pot;
    }

    for c in &mut vectors.split_pot_among_winners {
        c.expected = split_pot_among_winners(c.distributable, c.winner_count).unwrap();
    }

    for c in &mut vectors.deviation_bps {
        c.expected_bps = compute_deviation_bps(c.price, c.reference).unwrap();
    }

    for c in &mut vectors.precision_remainder_ordering {
        let entries = to_precision_entries(&c.entries);
        let results = compute_precision_payouts(&entries, c.final_price, None).unwrap();
        c.expected_payouts = results.into_iter().map(|r| r.payout).collect();
    }

    let rendered = serde_json::to_string_pretty(&vectors).expect("VectorFile must serialize");
    std::fs::write(VECTORS_PATH, rendered + "\n")
        .unwrap_or_else(|e| panic!("failed to write {}: {}", VECTORS_PATH, e));
    std::eprintln!("Regenerated {}", VECTORS_PATH);
}
