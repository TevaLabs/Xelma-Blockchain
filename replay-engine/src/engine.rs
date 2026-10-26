// SPDX-License-Identifier: MIT
//! Deterministic replay entrypoint — uses the same settlement math as live settlement.

use crate::errors::ContractError;
use crate::fee_incidence::FeeIncidence;
use crate::settlement_math::{
    compute_precision_payouts_with_policy_and_model, compute_updown_payouts_with_model,
    PrecisionEntry, PrecisionPayoutPolicy, PrecisionScoringMode, PrecisionScoringPolicy,
    UpDownPosition,
};
use crate::transcript::{
    ArchiveStatus, OutcomeKind, RoundTranscript, TerminalAction, TranscriptError, TranscriptMode,
};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ReplayPayout {
    pub index: usize,
    pub payout: i128,
    pub outcome: OutcomeKind,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ReplayResult {
    pub archive_status: ArchiveStatus,
    pub total_fee: i128,
    pub payouts: Vec<ReplayPayout>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplayError {
    Transcript(TranscriptError),
    Settlement(ContractError),
    OracleRoundMismatch { transcript: u64, oracle: u64 },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transcript(e) => write!(f, "{e}"),
            Self::Settlement(e) => write!(f, "settlement math error: {e:?}"),
            Self::OracleRoundMismatch { transcript, oracle } => write!(
                f,
                "oracle.round_id {oracle} != transcript.round_id {transcript}"
            ),
        }
    }
}

impl std::error::Error for ReplayError {}

impl From<TranscriptError> for ReplayError {
    fn from(value: TranscriptError) -> Self {
        Self::Transcript(value)
    }
}

impl From<ContractError> for ReplayError {
    fn from(value: ContractError) -> Self {
        Self::Settlement(value)
    }
}

/// Recompute round outcomes from a canonical transcript.
pub fn replay_round(transcript: &RoundTranscript) -> Result<ReplayResult, ReplayError> {
    transcript.validate_schema()?;

    if transcript.oracle.round_id != transcript.round_id {
        return Err(ReplayError::OracleRoundMismatch {
            transcript: transcript.round_id,
            oracle: transcript.oracle.round_id,
        });
    }

    match transcript.terminal {
        TerminalAction::Cancel => {
            replay_full_refund(transcript, ArchiveStatus::Cancelled, OutcomeKind::Void)
        }
        TerminalAction::Void => {
            replay_full_refund(transcript, ArchiveStatus::Voided, OutcomeKind::Void)
        }
        TerminalAction::FallbackRefund => replay_full_refund(
            transcript,
            ArchiveStatus::FallbackRefund,
            OutcomeKind::Refund,
        ),
        TerminalAction::Resolve => {
            if let Some(min) = transcript.min_participants {
                if transcript.participant_count < min {
                    return replay_full_refund(
                        transcript,
                        ArchiveStatus::FallbackRefund,
                        OutcomeKind::Refund,
                    );
                }
            }
            replay_resolve(transcript)
        }
    }
}

fn replay_full_refund(
    transcript: &RoundTranscript,
    status: ArchiveStatus,
    outcome: OutcomeKind,
) -> Result<ReplayResult, ReplayError> {
    let payouts = transcript
        .participants
        .iter()
        .map(|p| ReplayPayout {
            index: p.index,
            payout: p.amount,
            outcome,
        })
        .collect();
    Ok(ReplayResult {
        archive_status: status,
        total_fee: 0,
        payouts,
    })
}

fn replay_resolve(transcript: &RoundTranscript) -> Result<ReplayResult, ReplayError> {
    let final_price = transcript.final_price;
    // Transcripts recorded before #531 omit `fee_model`; absent means the
    // pre-#268 behaviour, which was always fee-on-pot.
    let fee_model = FeeIncidence::from_code(transcript.fee_model.unwrap_or(0));

    match transcript.mode {
        TranscriptMode::UpDown => {
            let positions: Vec<UpDownPosition> = transcript
                .participants
                .iter()
                .map(|p| UpDownPosition {
                    index: p.index,
                    amount: p.amount,
                    side_up: p.side_up.unwrap_or(true),
                })
                .collect();

            // The engine derives the fee itself and applies the price
            // direction. Recomputing it here from `(pool_up, pool_down)`
            // would tax the wrong pool under `FeeOnWinnings`, whose base is
            // the *losing* side (Issue #531).
            let settlement = compute_updown_payouts_with_model(
                &positions,
                transcript.price_start,
                final_price,
                transcript.pool_up,
                transcript.pool_down,
                transcript.fee_bps,
                fee_model,
            )?;
            let entries = settlement.payouts;
            let total_fee = settlement.fee_amount;

            let payouts = entries
                .iter()
                .map(|e| ReplayPayout {
                    index: e.index,
                    payout: e.payout,
                    outcome: if e.is_refund {
                        OutcomeKind::Refund
                    } else if e.is_winner {
                        OutcomeKind::Win
                    } else {
                        OutcomeKind::Loss
                    },
                })
                .collect();

            Ok(ReplayResult {
                archive_status: ArchiveStatus::Resolved,
                total_fee,
                payouts,
            })
        }
        TranscriptMode::Precision => {
            let entries: Vec<PrecisionEntry> = transcript
                .participants
                .iter()
                .map(|p| PrecisionEntry {
                    index: p.index,
                    predicted_price: p.commit_reveal.predicted_price,
                    amount: p.amount,
                    revealed: p.commit_reveal.revealed,
                })
                .collect();

            let settlement = compute_precision_payouts_with_policy_and_model(
                &entries,
                final_price,
                transcript.fee_bps,
                PrecisionScoringPolicy {
                    mode: PrecisionScoringMode::AbsoluteDistance,
                    confidence_band: None,
                },
                PrecisionPayoutPolicy::Equal,
                fee_model,
            )?;

            let math = settlement.payouts;
            let total_fee = settlement.fee_amount;

            let payouts = math
                .iter()
                .map(|e| ReplayPayout {
                    index: e.index,
                    payout: e.payout,
                    outcome: if e.is_refund {
                        OutcomeKind::Refund
                    } else if e.is_winner {
                        OutcomeKind::Win
                    } else {
                        OutcomeKind::Loss
                    },
                })
                .collect();

            Ok(ReplayResult {
                archive_status: ArchiveStatus::Resolved,
                total_fee,
                payouts,
            })
        }
    }
}

/// Build expected outcome block from a replay result (for recording live transcripts).
pub fn replay_to_expected(replay: &ReplayResult) -> crate::transcript::ExpectedOutcome {
    crate::transcript::ExpectedOutcome {
        archive_status: replay.archive_status,
        total_fee: replay.total_fee,
        payouts: replay
            .payouts
            .iter()
            .map(|p| crate::transcript::ExpectedPayout {
                index: p.index,
                payout: p.payout,
                outcome: p.outcome,
            })
            .collect(),
    }
}
