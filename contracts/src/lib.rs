// SPDX-License-Identifier: MIT
#![no_std]
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

#[cfg(test)]
extern crate std;

mod admin;
mod betting;
pub mod common;
mod config;
mod contract;
mod errors;
mod queries;
mod settlement;
mod types;

#[cfg(test)]
mod tests;

pub use contract::VirtualTokenContract;
pub use errors::ContractError;
pub use types::{
    ArchivedRoundSummary, BetSide, ConfigChangeKind, ConfigChangePayload, DataKey,
    OracleRotationProposal, PendingConfigChange, PrecisionCommitment, PrecisionPrediction,
    ProtocolHealthStatus, Round, RoundArchiveStatus, UserPosition, UserStats,
};
