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
    /// Invalid round mode (must be 0 or 1)
    InvalidMode = 14,
    /// Wrong prediction type for current round mode
    WrongModeForPrediction = 15,
    /// Round has not reached end_ledger yet
    RoundNotEnded = 16,
    /// Invalid price scale (must represent 4 decimal places)
    InvalidPriceScale = 17,
    /// Oracle data is too old (STALE)
    StaleOracleData = 18,
    /// Oracle payload round_id doesn't match ActiveRound
    InvalidOracleRound = 19,
    /// An active round already exists and cannot be overwritten
    RoundAlreadyActive = 20,
    /// Admin and Oracle addresses cannot be identical
    AdminIsOracle = 21,
    /// Contract is paused for emergency recovery
    ContractPaused = 22,
    /// One or more window values exceed configured maximum bounds
    WindowOutOfRange = 23,
    /// Oracle payload timestamp is in the future
    FutureOracleData = 24,
    /// Arithmetic overflow in payout accumulation — no funds moved
    PayoutOverflow = 25,
    /// Round has been cancelled and cannot be resolved
    RoundCancelled = 26,
    /// Round cannot be cancelled (no active round or already resolved)
    RoundNotCancellable = 27,
    /// Bet amount exceeds the configured maximum stake
    StakeExceedsMax = 28,
    /// User's cumulative exposure in this round exceeds the configured cap
    ExposureCapExceeded = 29,
    /// Pending winnings accumulation would exceed the configured cap
    PendingWinningsCapExceeded = 30,
    /// Oracle payload nonce was already consumed for this round (replay)
    OracleNonceReused = 31,
    /// Round has fewer participants than the configured minimum for competitive settlement
    InsufficientParticipants = 32,
    /// Minimum participants value is out of valid range (must be 1–10000)
    InvalidMinParticipants = 33,
    /// Oracle heartbeat status is out of range (must be 0, 1, or 2)
    InvalidOracleStatus = 34,
    /// Oracle stale threshold is out of valid range (must be 60–86400 seconds)
    InvalidStaleThreshold = 35,
    /// Oracle payload timestamp predates the round lifecycle window
    OracleDataBeforeRound = 36,
}
