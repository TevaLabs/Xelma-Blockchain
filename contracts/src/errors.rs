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
    /// Start price is below the minimum allowed value
    StartPriceTooLow = 31,
    /// Start price exceeds the maximum allowed value
    StartPriceTooHigh = 32,
    /// Oracle payload nonce was already consumed for this round (replay)
    OracleNonceReused = 33,
    /// Round has fewer participants than the configured minimum for competitive settlement
    InsufficientParticipants = 34,
    /// Minimum participants value is out of valid range (must be 1–10000)
    InvalidMinParticipants = 35,
    /// Oracle heartbeat status is out of range (must be 0, 1, or 2)
    InvalidOracleStatus = 36,
    /// Oracle stale threshold is out of valid range (must be 60–86400 seconds)
    InvalidStaleThreshold = 37,
    /// Precision participant cap is out of range (must be 1–10000)
    InvalidPrecisionParticipantCap = 38,
    /// Precision round has reached the configured participant cap
    PrecisionParticipantCapExceeded = 39,
    /// Oracle max deviation bps is invalid (must be > 0)
    InvalidOracleDeviationBps = 40,
    /// Oracle final price deviates beyond configured threshold
    OracleDeviationExceeded = 41,
    /// Stored schema version is unknown or unsupported by this contract build
    UnsupportedSchemaVersion = 42,
    /// Migration path is invalid for the stored schema version
    InvalidMigrationPath = 43,
    /// Migration cannot run while a round is active
    MigrationActiveRound = 44,
    /// Commitment for precision prediction not found
    CommitmentNotFound = 45,
    /// Precision prediction has already been revealed
    AlreadyRevealed = 46,
    /// Attempted to reveal prediction outside the valid window
    InvalidRevealWindow = 47,
    /// Revealed prediction hash does not match committed hash
    HashMismatch = 48,
    /// Oracle payload network_id does not match the runtime network
    OracleNetworkMismatch = 49,
    /// Oracle payload contract_addr does not match the current contract
    OracleContractMismatch = 50,
    /// Protocol fee bps is outside the allowed range (must be in `1..=MAX_PROTOCOL_FEE_BPS`)
    InvalidProtocolFeeBps = 51,
    /// Treasury withdrawal would underflow the accumulated treasury balance
    FeeTreasuryUnderflow = 52,
}
