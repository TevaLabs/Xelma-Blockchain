// SPDX-License-Identifier: MIT
//! Golden and property tests proving live settlement == replay.

use std::fs;
use std::path::PathBuf;

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use xelma_replay::{
    assert_live_matches_replay, replay_round, replay_to_expected, ArchiveStatus,
    CommitRevealRecord, OracleTranscript, OutcomeKind, RoundTranscript, TerminalAction,
    TranscriptMode, TranscriptParticipant, TRANSCRIPT_SCHEMA_VERSION,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn load_fixture(name: &str) -> RoundTranscript {
    let raw = fs::read_to_string(fixture_path(name)).expect("read fixture");
    serde_json::from_str(&raw).expect("parse fixture")
}

#[test]
fn golden_updown_resolve_live_equals_replay() {
    let t = load_fixture("updown_resolve_golden.json");
    let replay = replay_round(&t).expect("replay");
    assert_live_matches_replay(&t, &replay).expect("parity");
}

#[test]
fn golden_cancel_path_live_equals_replay() {
    let t = load_fixture("updown_cancel.json");
    let replay = replay_round(&t).expect("replay");
    assert_live_matches_replay(&t, &replay).expect("parity");
}

#[test]
fn golden_fallback_refund_live_equals_replay() {
    let t = load_fixture("precision_fallback_refund.json");
    let replay = replay_round(&t).expect("replay");
    assert_live_matches_replay(&t, &replay).expect("parity");
}

#[test]
fn golden_void_dispute_live_equals_replay() {
    let t = load_fixture("precision_void_dispute.json");
    let replay = replay_round(&t).expect("replay");
    assert_live_matches_replay(&t, &replay).expect("parity");
}

#[test]
fn replay_is_deterministic_for_golden_fixtures() {
    for name in [
        "updown_resolve_golden.json",
        "updown_cancel.json",
        "precision_fallback_refund.json",
        "precision_void_dispute.json",
    ] {
        let t = load_fixture(name);
        let a = replay_round(&t).expect("replay a");
        let b = replay_round(&t).expect("replay b");
        assert_eq!(a, b, "deterministic replay failed for {name}");
    }
}

fn arb_updown_transcript() -> impl Strategy<Value = RoundTranscript> {
    (
        1u64..10_000,
        prop::collection::vec((1i128..500, any::<bool>()), 1..8),
        1_000_000u128..5_000_000,
        1_000_000u128..5_000_000,
        prop::option::of(0u32..500),
    )
        .prop_map(|(round_id, stakes, start, final_price, fee_bps)| {
            let mut pool_up = 0i128;
            let mut pool_down = 0i128;
            let participants: Vec<TranscriptParticipant> = stakes
                .into_iter()
                .enumerate()
                .map(|(index, (amount, side_up))| {
                    if side_up {
                        pool_up = pool_up.saturating_add(amount);
                    } else {
                        pool_down = pool_down.saturating_add(amount);
                    }
                    TranscriptParticipant {
                        index,
                        address: None,
                        amount,
                        side_up: Some(side_up),
                        commit_reveal: CommitRevealRecord {
                            commit_hash_hex: None,
                            revealed: true,
                            predicted_price: 0,
                        },
                    }
                })
                .collect();

            let mut t = RoundTranscript {
                schema_version: TRANSCRIPT_SCHEMA_VERSION,
                round_id,
                mode: TranscriptMode::UpDown,
                terminal: TerminalAction::Resolve,
                price_start: start,
                final_price,
                pool_up,
                pool_down,
                fee_bps,
                min_participants: None,
                participant_count: participants.len() as u32,
                oracle: OracleTranscript {
                    price: final_price,
                    timestamp: 1_700_000_000,
                    round_id,
                    nonce: 1,
                    confidence: None,
                },
                participants,
                expected: xelma_replay::ExpectedOutcome {
                    archive_status: ArchiveStatus::Resolved,
                    total_fee: 0,
                    payouts: vec![],
                },
            };

            let replay = replay_round(&t).expect("random replay");
            t.expected = replay_to_expected(&replay);
            t
        })
}

proptest! {
    #[test]
    fn random_updown_live_equals_replay(t in arb_updown_transcript()) {
        let replay = replay_round(&t)?;
        assert_live_matches_replay(&t, &replay).map_err(|m| TestCaseError::fail(format!("{m:?}")))?;
    }
}

fn arb_precision_transcript() -> impl Strategy<Value = RoundTranscript> {
    (
        1u64..10_000,
        prop::collection::vec((1i128..300, 1_000_000u128..5_000_000, any::<bool>()), 1..6),
        2_000_000u128..3_000_000,
    )
        .prop_map(|(round_id, rows, final_price)| {
            let participants: Vec<TranscriptParticipant> = rows
                .into_iter()
                .enumerate()
                .map(
                    |(index, (amount, predicted_price, revealed))| TranscriptParticipant {
                        index,
                        address: None,
                        amount,
                        side_up: None,
                        commit_reveal: CommitRevealRecord {
                            commit_hash_hex: None,
                            revealed,
                            predicted_price,
                        },
                    },
                )
                .collect();

            let mut t = RoundTranscript {
                schema_version: TRANSCRIPT_SCHEMA_VERSION,
                round_id,
                mode: TranscriptMode::Precision,
                terminal: TerminalAction::Resolve,
                price_start: 20_000_000,
                final_price,
                pool_up: 0,
                pool_down: 0,
                fee_bps: Some(100),
                min_participants: None,
                participant_count: participants.len() as u32,
                oracle: OracleTranscript {
                    price: final_price,
                    timestamp: 1_700_000_100,
                    round_id,
                    nonce: 1,
                    confidence: None,
                },
                participants,
                expected: xelma_replay::ExpectedOutcome {
                    archive_status: ArchiveStatus::Resolved,
                    total_fee: 0,
                    payouts: vec![],
                },
            };

            let replay = replay_round(&t).expect("random precision replay");
            t.expected = replay_to_expected(&replay);
            t
        })
}

proptest! {
    #[test]
    fn random_precision_live_equals_replay(t in arb_precision_transcript()) {
        let replay = replay_round(&t)?;
        assert_live_matches_replay(&t, &replay).map_err(|m| TestCaseError::fail(format!("{m:?}")))?;
    }
}

proptest! {
    #[test]
    fn cancel_and_void_always_refund_full_stake(
        amount in 1i128..10_000,
        terminal in prop_oneof![Just(TerminalAction::Cancel), Just(TerminalAction::Void)]
    ) {
        let t = RoundTranscript {
            schema_version: TRANSCRIPT_SCHEMA_VERSION,
            round_id: 42,
            mode: TranscriptMode::UpDown,
            terminal,
            price_start: 10_000_000,
            final_price: 15_000_000,
            pool_up: amount,
            pool_down: 0,
            fee_bps: None,
            min_participants: None,
            participant_count: 1,
            oracle: OracleTranscript {
                price: 15_000_000,
                timestamp: 1,
                round_id: 42,
                nonce: 1,
                confidence: None,
            },
            participants: vec![TranscriptParticipant {
                index: 0,
                address: None,
                amount,
                side_up: Some(true),
                commit_reveal: CommitRevealRecord {
                    commit_hash_hex: None,
                    revealed: true,
                    predicted_price: 0,
                },
            }],
            expected: xelma_replay::ExpectedOutcome {
                archive_status: ArchiveStatus::Cancelled,
                total_fee: 0,
                payouts: vec![xelma_replay::ExpectedPayout {
                    index: 0,
                    payout: amount,
                    outcome: OutcomeKind::Void,
                }],
            },
        };

        let replay = replay_round(&t)?;
        prop_assert_eq!(replay.payouts[0].payout, amount);
    }
}
