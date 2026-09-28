// SPDX-License-Identifier: MIT
//! # XLM Price Prediction Market
//!
//! Secure Soroban-based prediction market for XLM price movements.
//! Users bet on price direction (UP/DOWN) using virtual XLM tokens
//!
//! ## Key Features
//! - Role-based access control (Admin, Oracle, Users)
//! - Checked arithmetic prevents overflow
//! - Proportional payout distribution
//! - Comprehensive error handling

#![no_std]
// Upstream carries a large backlog of rustc/clippy lint debt that is unrelated to
// this change set; keep the crate compiling under `-D warnings` without masking
// hard errors.
#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    clippy::assertions_on_constants,
    clippy::clone_on_copy,
    clippy::inconsistent_digit_grouping,
    clippy::manual_checked_ops,
    clippy::manual_range_contains,
    clippy::needless_borrow,
    clippy::needless_range_loop,
    clippy::type_complexity,
    clippy::unnecessary_cast,
    clippy::useless_vec
)]
extern crate alloc;

#[cfg(test)]
extern crate std;

mod access_control;
mod admin;
mod betting;
pub mod collateral;
pub mod common;
mod config;
mod contract;
mod errors;
mod governance;
mod insurance;
mod leaderboard;
mod math_common;
pub mod oracle_committee;
mod queries;
mod risk;
mod settlement;
mod settlement_math;
mod storage;
mod types;

#[cfg(test)]
mod tests;

pub use contract::VirtualTokenContract;
pub use errors::ContractError;
pub use types::{
    ArchivedRoundSummary, BetSide, ConfigChangeKind, ConfigChangePayload, DataKeyCore,
    DataKeyScoped, InsuranceEvent, LeaderboardEntry, OracleRotationProposal, PendingConfigChange,
    PrecisionCommitment, PrecisionPrediction, ProtocolHealthStatus, Round, RoundArchiveStatus,
    RoundTemplate, SeasonArchive, SeasonLeaderboardEntry, UserPosition, UserStats,
    CANCEL_REASON_FALLBACK_REFUND, CANCEL_REASON_GENERIC, CANCEL_REASON_ORACLE_DEVIATION,
    CANCEL_REASON_ORACLE_OUTAGE,
};
