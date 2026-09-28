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
// Public settlement-math / query helpers are kept on purpose (golden vectors,
// parity replay, operator docs) even when the contract does not call them yet.
#![allow(dead_code)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::manual_abs_diff)]
#![allow(clippy::manual_checked_ops)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::useless_vec)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::inconsistent_digit_grouping)]
extern crate alloc;

#[cfg(test)]
extern crate std;

mod access_control;
mod admin;
mod betting;
pub mod common;
mod config;
mod contract;
mod errors;
mod governance;
mod leaderboard;
mod math_common;
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
    DataKeyScoped, LeaderboardEntry, OracleRotationProposal, PendingConfigChange,
    PrecisionCommitment, PrecisionPrediction, ProtocolHealthStatus, Round, RoundArchiveStatus,
    RoundTemplate, SeasonArchive, SeasonLeaderboardEntry, UserPosition, UserStats,
};
