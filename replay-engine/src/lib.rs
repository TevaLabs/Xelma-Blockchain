// SPDX-License-Identifier: MIT
//! Deterministic round replay recorder for audits and disputes (Issue #369).

extern crate alloc;

pub mod diagnostics;
pub mod engine;
pub mod hash;
pub mod transcript;

mod errors;
mod fee_incidence;
mod math_common;
mod settlement_math;

pub use diagnostics::{assert_live_matches_replay, compare_replay, MismatchDiagnostic};
pub use engine::{replay_round, replay_to_expected, ReplayError, ReplayPayout, ReplayResult};
pub use fee_incidence::FeeIncidence;
pub use hash::transcript_commitment_hex;
pub use transcript::{
    ArchiveStatus, CommitRevealRecord, ExpectedOutcome, ExpectedPayout, OracleTranscript,
    OutcomeKind, RoundTranscript, TerminalAction, TranscriptError, TranscriptMode,
    TranscriptParticipant, TRANSCRIPT_SCHEMA_VERSION,
};
