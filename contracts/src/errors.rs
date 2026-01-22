//! Contract error types for the XLM Price Prediction Market.

use soroban_sdk::contracterror;

/// Contract error types
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    /// Contract has already been initialized
    AlreadyInitialized = 1,
    /// Admin address not set - call initialize first
    AdminNotSet = 2,
    /// Oracle address not set - call initialize first
    OracleNotSet = 3,
    /// Only admin can perform this action
    UnauthorizedAdmin = 4,
    /// Only oracle can perform this action
    UnauthorizedOracle = 5,
    /// Bet amount must be greater than zero
    InvalidBetAmount = 6,
    /// No active round exists
    NoActiveRound = 7,
    /// Round has already ended
    RoundEnded = 8,
    /// User has insufficient balance
    InsufficientBalance = 9,
    /// User has already placed a bet in this round
    AlreadyBet = 10,
    /// Arithmetic overflow occurred
    Overflow = 11,
    /// Invalid price value
    InvalidPrice = 12,
    /// Invalid duration value
    InvalidDuration = 13,
}

