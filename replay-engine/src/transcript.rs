// SPDX-License-Identifier: MIT
//! Versioned round transcript schema for deterministic replay.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Current transcript schema version. Bump when breaking fields or ordering rules change.
pub const TRANSCRIPT_SCHEMA_VERSION: u32 = 1;

fn serialize_u128<S: Serializer>(value: &u128, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}

fn deserialize_u128<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u128, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum U128Rep {
        Num(u64),
        Str(String),
    }
    match U128Rep::deserialize(deserializer)? {
        U128Rep::Num(n) => Ok(n as u128),
        U128Rep::Str(s) => s.parse::<u128>().map_err(serde::de::Error::custom),
    }
}

/// Round mode encoded in transcripts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptMode {
    #[serde(rename = "updown")]
    UpDown,
    Precision,
}

/// Terminal settlement path recorded in the transcript.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalAction {
    /// Normal oracle settlement.
    Resolve,
    /// Admin cancel before/during round.
    Cancel,
    /// Dispute window void-to-refund.
    Void,
    /// `min_participants` threshold not met at settlement.
    FallbackRefund,
}

/// Archived round status mirrored from on-chain settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveStatus {
    Resolved,
    Cancelled,
    FallbackRefund,
    Voided,
}

/// Per-participant outcome classification for diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeKind {
    Win,
    Loss,
    Refund,
    Void,
}

/// Oracle payload fields required to justify settlement price selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OracleTranscript {
    #[serde(with = "u128_str")]
    pub price: u128,
    pub timestamp: u64,
    pub round_id: u64,
    pub nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<u32>,
}

/// Commit/reveal metadata for precision rounds.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitRevealRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_hash_hex: Option<String>,
    pub revealed: bool,
    #[serde(default, with = "u128_str")]
    pub predicted_price: u128,
}

mod u128_str {
    use super::{deserialize_u128, serialize_u128};
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u128, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_u128(value, serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u128, D::Error> {
        deserialize_u128(deserializer)
    }
}

/// Canonical participant row — sorted by `index` before hashing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TranscriptParticipant {
    pub index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    pub amount: i128,
    /// UpDown side (`true` = Up). Omitted for precision-only rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side_up: Option<bool>,
    #[serde(flatten)]
    pub commit_reveal: CommitRevealRecord,
}

/// Expected live outcome captured at record time for parity checks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExpectedOutcome {
    pub archive_status: ArchiveStatus,
    pub total_fee: i128,
    pub payouts: Vec<ExpectedPayout>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExpectedPayout {
    pub index: usize,
    pub payout: i128,
    pub outcome: OutcomeKind,
}

/// Full round input transcript: bets, commits/reveals, oracle payload, terminal path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoundTranscript {
    pub schema_version: u32,
    pub round_id: u64,
    pub mode: TranscriptMode,
    pub terminal: TerminalAction,
    #[serde(with = "u128_str")]
    pub price_start: u128,
    #[serde(with = "u128_str")]
    pub final_price: u128,
    pub pool_up: i128,
    pub pool_down: i128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_bps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_participants: Option<u32>,
    pub participant_count: u32,
    pub oracle: OracleTranscript,
    /// Must be sorted by ascending `index` (canonical ordering rule).
    pub participants: Vec<TranscriptParticipant>,
    pub expected: ExpectedOutcome,
}

impl RoundTranscript {
    pub fn validate_schema(&self) -> Result<(), TranscriptError> {
        if self.schema_version != TRANSCRIPT_SCHEMA_VERSION {
            return Err(TranscriptError::UnsupportedSchema(self.schema_version));
        }
        if self.participants.is_empty() {
            return Err(TranscriptError::EmptyParticipants);
        }
        for (i, p) in self.participants.iter().enumerate() {
            if p.index != i {
                return Err(TranscriptError::ParticipantOrder {
                    expected_index: i,
                    found_index: p.index,
                });
            }
        }
        if self.participant_count as usize != self.participants.len() {
            return Err(TranscriptError::ParticipantCountMismatch {
                declared: self.participant_count,
                actual: self.participants.len() as u32,
            });
        }
        Ok(())
    }

    /// Sort participants by index (canonical ordering before hashing or replay).
    pub fn canonicalize_participants(&mut self) {
        self.participants.sort_by_key(|p| p.index);
        self.participant_count = self.participants.len() as u32;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TranscriptError {
    UnsupportedSchema(u32),
    EmptyParticipants,
    ParticipantOrder {
        expected_index: usize,
        found_index: usize,
    },
    ParticipantCountMismatch {
        declared: u32,
        actual: u32,
    },
}

impl std::fmt::Display for TranscriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchema(v) => write!(f, "unsupported transcript schema version {v}"),
            Self::EmptyParticipants => write!(f, "transcript has no participants"),
            Self::ParticipantOrder {
                expected_index,
                found_index,
            } => {
                write!(
                    f,
                    "participants must be sorted by index: expected {expected_index}, found {found_index}"
                )
            }
            Self::ParticipantCountMismatch { declared, actual } => write!(
                f,
                "participant_count {declared} does not match participants.len() {actual}"
            ),
        }
    }
}

impl std::error::Error for TranscriptError {}
