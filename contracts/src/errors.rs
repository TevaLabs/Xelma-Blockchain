// SPDX-License-Identifier: MIT
//! Contract error types for the XLM Price Prediction Market.

use soroban_sdk::contracterror;

/// Contract error types
#[contracterror(export = false)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    AdminNotSet = 2,
    OracleNotSet = 3,
    InvalidBetAmount = 6,
    NoActiveRound = 7,
    RoundEnded = 8,
    InsufficientBalance = 9,
    AlreadyBet = 10,
    Overflow = 11,
    InvalidPrice = 12,
    InvalidDuration = 13,
    InvalidMode = 14,
    WrongModeForPrediction = 15,
    RoundNotEnded = 16,
    StaleOracleData = 18,
    InvalidOracleRound = 19,
    RoundAlreadyActive = 20,
    ContractPaused = 22,
    WindowOutOfRange = 23,
    FutureOracleData = 24,
    PayoutOverflow = 25,
    RoundNotCancellable = 27,
    StakeExceedsMax = 28,
    ExposureCapExceeded = 29,
    PendingWinningsCapExceeded = 30,
    InvalidStartPrice = 31,
    OracleNonceReused = 33,
    InvalidMinParticipants = 35,
    InvalidPrecisionCap = 38,
    PrecisionCapExceeded = 39,
    OracleDeviationExceeded = 41,
    UnsupportedSchemaVersion = 42,
    MigrationActiveRound = 44,
    CommitmentNotFound = 45,
    AlreadyRevealed = 46,
    InvalidRevealWindow = 47,
    HashMismatch = 48,
    OracleNetworkMismatch = 49,
    InvalidProtocolFeeBps = 51,
    MintLimitExceeded = 53,
    NoPendingRotation = 54,
    /// Oracle rotation delay has not elapsed yet (must wait MIN_ROTATION_DELAY_SECONDS)
    RotationDelayNotElapsed = 55,
    /// Invalid archive retention limit
    InvalidArchiveRetention = 62,
    InvalidCommitment = 63,
    InvalidSalt = 64,
    NoRoundTemplate = 65,
    /// Oracle payload timestamp is outside the round-relative economic window
    OracleTimestampOutsideWindow = 66,
    /// Pending winnings entry exists but has not yet reached the configured
    /// expiry threshold — caller must wait before reclaiming.
    PendingWinningsNotExpired = 61,
    /// Epoch mint budget has been fully consumed
    EpochBudgetExceeded = 67,
    /// Oracle heartbeat is not live and strict mode blocks settlement (Issue #264)
    OracleNotLive = 68,
    /// Invalid precision payout policy
    InvalidPayoutPolicy = 69,
    /// Stake amount is below the configured minimum bet (dust protection, Issue #269)
    BelowMinBet = 70,
    /// Multi-feed resolution: fewer observations survived outlier rejection
    /// than the configured quorum threshold.
    InsufficientOracleQuorum = 71,
    /// Multi-feed resolution: payload contains fewer observations than the
    /// configured minimum.
    TooFewObservations = 72,
    /// Multi-feed resolution: outlier observations would dominate the result
    /// (too many rejected, cannot form quorum).
    OracleOutlierRejected = 73,
    /// Multi-feed payload contains duplicate source identifiers.
    DuplicateOracleSource = 74,
    /// Multi-feed payload has observations that are not sorted or sources
    /// are out of expected range.
    InvalidObservationOrder = 75,
    /// The requested data key is not allowed for batch TTL touch operations.
    UnsupportedDataKeyForTtlTouch = 76,
    /// Pending winnings entry does not exist or expiry is not configured.
    PendingWinningsNotFound = 77,
    /// Pending winnings expiry is not configured (value is 0).
    ExpiryNotConfigured = 78,
    /// Early cash-out feature is disabled or not configured
    EarlyCashoutDisabled = 79,
    /// User does not have an active position to cash out
    PositionNotFound = 80,
    /// Early cash-out attempted outside the valid running phase
    InvalidPhaseForCashout = 81,
    /// Early cash-out is only supported for UpDown rounds
    WrongModeForCashout = 82,
    /// Oracle heartbeat is unhealthy and blocks settlement (Issue #264).
    /// Returned by the always-on heartbeat gate (`_enforce_heartbeat_health`)
    /// regardless of strict-mode setting — covers no heartbeat, offline status,
    /// and stale-beyond-grace conditions.
    OracleHeartbeatUnhealthy = 83,
    /// Access denied — caller does not have the required role.
    AccessDenied = 84,
    /// Governance action is unauthorized — caller is not a governance authority.
    GovUnauthorized = 85,
    /// Proposal not found.
    ProposalNotFound = 86,
    /// Proposal has expired.
    ProposalExpired = 87,
    /// Governance action is in an invalid state for the requested operation.
    GovInvalidState = 88,
    /// Dispute window has expired.
    DisputeWindowExpired = 89,
    /// Claim is locked.
    ClaimLocked = 90,
}
