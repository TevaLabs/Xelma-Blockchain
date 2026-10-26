// SPDX-License-Identifier: MIT
//! Golden and property tests proving live settlement == replay.

use std::fs;
use std::path::PathBuf;

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use xelma_replay::{
    assert_live_matches_replay, replay_round, replay_to_expected, transcript_commitment_hex,
    ArchiveStatus, CommitRevealRecord, OracleTranscript, OutcomeKind, RoundTranscript,
    TerminalAction, TranscriptMode, TranscriptParticipant, TRANSCRIPT_SCHEMA_VERSION,
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

/// Builds a small UpDown transcript: 100 Up against 300 Down, price up, so
/// the winning (Up) pool is 100 and the losing (Down) pool is 300. Unequal on
/// purpose — the two fee models tax different bases here (400 vs 300), so a
/// model-blind replay is detectable.
fn updown_transcript(fee_bps: Option<u32>, fee_model: Option<u32>) -> RoundTranscript {
    RoundTranscript {
        schema_version: TRANSCRIPT_SCHEMA_VERSION,
        round_id: 77,
        mode: TranscriptMode::UpDown,
        terminal: TerminalAction::Resolve,
        price_start: 10_000,
        final_price: 11_000, // price up => Up wins
        pool_up: 100,
        pool_down: 300,
        fee_bps,
        fee_model,
        min_participants: None,
        participant_count: 2,
        oracle: OracleTranscript {
            price: 11_000,
            timestamp: 1_700_000_500,
            round_id: 77,
            nonce: 1,
            confidence: None,
        },
        participants: vec![
            TranscriptParticipant {
                index: 0,
                address: None,
                amount: 100,
                side_up: Some(true),
                commit_reveal: CommitRevealRecord {
                    commit_hash_hex: None,
                    revealed: true,
                    predicted_price: 0,
                },
            },
            TranscriptParticipant {
                index: 1,
                address: None,
                amount: 300,
                side_up: Some(false),
                commit_reveal: CommitRevealRecord {
                    commit_hash_hex: None,
                    revealed: true,
                    predicted_price: 0,
                },
            },
        ],
        expected: xelma_replay::ExpectedOutcome {
            archive_status: ArchiveStatus::Resolved,
            total_fee: 0,
            payouts: vec![],
        },
    }
}

/// Replay must honour the transcript's fee incidence model (Issue #531).
/// Before this, the engine hard-coded fee-on-pot, so a `FeeOnWinnings` round
/// replayed a fee the chain never charged.
#[test]
fn replay_honours_the_transcript_fee_model() {
    let bps = Some(1_000u32); // 10%

    // FeeOnPot taxes the whole 400 pot => 40.
    let on_pot = replay_round(&updown_transcript(bps, Some(0))).expect("replay fee-on-pot");
    assert_eq!(on_pot.total_fee, 40, "FeeOnPot must tax the total pot");

    // FeeOnWinnings taxes only the 300 losing pool => 30.
    let on_winnings =
        replay_round(&updown_transcript(bps, Some(1))).expect("replay fee-on-winnings");
    assert_eq!(
        on_winnings.total_fee, 30,
        "FeeOnWinnings must tax only the losing pool"
    );
    assert_ne!(on_pot.total_fee, on_winnings.total_fee);

    // Winner payouts must track the difference: the whole distributable pool
    // goes to the single winning participant.
    assert_eq!(on_pot.payouts[0].payout, 360, "400 pot - 40 fee");
    assert_eq!(on_winnings.payouts[0].payout, 370, "400 pot - 30 fee");

    // Conservation holds for both models.
    for (label, result, fee) in [
        ("FeeOnPot", &on_pot, 40i128),
        ("FeeOnWinnings", &on_winnings, 30i128),
    ] {
        let sum: i128 = result.payouts.iter().map(|p| p.payout).sum();
        assert_eq!(sum + fee, 400, "{label}: conservation");
    }
}

/// An absent `fee_model` means the pre-#268 behaviour, which was always
/// fee-on-pot. Transcripts recorded before the field existed must keep
/// replaying, and — because the field is `skip_serializing_if` — keep their
/// transcript commitment hash too.
#[test]
fn transcripts_without_a_fee_model_default_to_fee_on_pot() {
    let absent = updown_transcript(Some(1_000), None);
    let explicit_pot = updown_transcript(Some(1_000), Some(0));

    let a = replay_round(&absent).expect("replay absent");
    let b = replay_round(&explicit_pot).expect("replay explicit");
    assert_eq!(
        a.total_fee, b.total_fee,
        "an omitted fee_model must behave as FeeOnPot"
    );

    // Backward compatibility for the audit commitment: because the field is
    // `skip_serializing_if = "Option::is_none"`, a transcript without a fee
    // model serialises byte-for-byte as it did before the field existed, so
    // its SHA-256 commitment is unchanged. If this ever regresses, every
    // pre-#531 dispute case would stop verifying.
    let json = serde_json::to_string(&absent).expect("serialise");
    assert!(
        !json.contains("fee_model"),
        "an absent fee model must be omitted from the canonical JSON: {json}"
    );
    let with_model =
        serde_json::to_string(&updown_transcript(Some(1_000), Some(1))).expect("serialise");
    assert!(
        with_model.contains("\"fee_model\":1"),
        "an explicit fee model must appear in the canonical JSON: {with_model}"
    );
    assert_ne!(
        transcript_commitment_hex(&absent).expect("commitment absent"),
        transcript_commitment_hex(&updown_transcript(Some(1_000), Some(1)))
            .expect("commitment winnings"),
        "an explicit FeeOnWinnings transcript must commit differently"
    );
}

fn arb_updown_transcript() -> impl Strategy<Value = RoundTranscript> {
    (
        1u64..10_000,
        prop::collection::vec((1i128..500, any::<bool>()), 1..8),
        1_000_000u128..5_000_000,
        1_000_000u128..5_000_000,
        prop::option::of(0u32..500),
        prop::option::of(0u32..=1),
    )
        .prop_map(
            |(round_id, stakes, start, final_price, fee_bps, fee_model)| {
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
                    fee_model,
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
            },
        )
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
        prop::option::of(0u32..=1),
    )
        .prop_map(|(round_id, rows, final_price, fee_model)| {
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
                price_start: 2_000_0000,
                final_price,
                pool_up: 0,
                pool_down: 0,
                fee_bps: Some(100),
                fee_model,
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
            price_start: 1_000_0000,
            final_price: 1_500_0000,
            pool_up: amount,
            pool_down: 0,
            fee_bps: None,
            fee_model: None,
            min_participants: None,
            participant_count: 1,
            oracle: OracleTranscript {
                price: 1_500_0000,
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
