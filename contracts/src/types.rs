//! Type definitions for the XLM Price Prediction Market.

use soroban_sdk::{contracttype, Address};

/// Storage keys for contract data
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Balance(Address),
    Admin,
    Oracle,
    ActiveRound,
    Positions,
    PendingWinnings(Address),
    UserStats(Address),
}

/// Represents which side a user bet on
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum BetSide {
    Up,
    Down,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct UserPosition {
    pub amount: i128,
    pub side: BetSide,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct UserStats {
    pub total_wins: u32,
    pub total_losses: u32,
    pub current_streak: u32,
    pub best_streak: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    pub price_start: u128,  // Starting XLM price in stroops
    pub end_ledger: u32,     // Ledger when round ends (~5s per ledger)
    pub pool_up: i128,       // Total vXLM bet on UP
    pub pool_down: i128,     // Total vXLM bet on DOWN
}

