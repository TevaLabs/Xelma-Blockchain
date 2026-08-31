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
