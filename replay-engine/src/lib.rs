// SPDX-License-Identifier: MIT
//! Deterministic round replay recorder for audits and disputes (Issue #369).

// Shared settlement-math helpers are included verbatim from the contract crate
// for parity replay; some of them are intentionally not called by the engine.
#![allow(dead_code)]
#![allow(clippy::manual_abs_diff)]
#![allow(clippy::manual_checked_ops)]
#![allow(clippy::inconsistent_digit_grouping)]

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
