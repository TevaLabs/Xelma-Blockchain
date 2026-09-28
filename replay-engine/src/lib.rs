// SPDX-License-Identifier: MIT
//! Deterministic round replay recorder for audits and disputes (Issue #369).

// Shares contract sources (settlement_math) whose pre-existing lint debt is
// unrelated to this change set.
#![allow(
    dead_code,
    clippy::inconsistent_digit_grouping,
    clippy::manual_abs_diff,
    clippy::manual_checked_ops,
    clippy::needless_borrow,
    clippy::type_complexity,
    clippy::unnecessary_cast,
    clippy::useless_vec
)]

extern crate alloc;

pub mod diagnostics;
pub mod engine;
pub mod hash;
pub mod transcript;

mod errors;
mod math_common;
mod settlement_math;

pub use diagnostics::{assert_live_matches_replay, compare_replay, MismatchDiagnostic};
pub use engine::{replay_round, replay_to_expected, ReplayError, ReplayPayout, ReplayResult};
pub use hash::transcript_commitment_hex;
pub use transcript::{
    ArchiveStatus, CommitRevealRecord, ExpectedOutcome, ExpectedPayout, OracleTranscript,
    OutcomeKind, RoundTranscript, TerminalAction, TranscriptError, TranscriptMode,
    TranscriptParticipant, TRANSCRIPT_SCHEMA_VERSION,
};
