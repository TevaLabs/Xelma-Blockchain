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
    InvalidBetAmount = 4,
    NoActiveRound = 5,
    RoundEnded = 6,
    InsufficientBalance = 7,
    AlreadyBet = 8,
    Overflow = 9,
    InvalidPrice = 10,
    InvalidDuration = 11,
    InvalidMode = 12,
    WrongModeForPrediction = 13,
    RoundNotEnded = 14,
    StaleOracleData = 15,
    InvalidOracleRound = 16,
    RoundAlreadyActive = 17,
    ContractPaused = 18,
    WindowOutOfRange = 19,
    FutureOracleData = 20,
    PayoutOverflow = 21,
    RoundNotCancellable = 22,
    StakeExceedsMax = 23,
    ExposureCapExceeded = 24,
    PendingWinningsCapExceeded = 25,
    InvalidStartPrice = 26,
    OracleNonceReused = 27,
    InvalidMinParticipants = 28,
    InvalidPrecisionCap = 29,
    PrecisionCapExceeded = 30,
    OracleDeviationExceeded = 31,
    UnsupportedSchemaVersion = 32,
    MigrationActiveRound = 33,
    CommitmentNotFound = 34,
    AlreadyRevealed = 35,
    InvalidRevealWindow = 36,
    HashMismatch = 37,
    OracleNetworkMismatch = 38,
    InvalidProtocolFeeBps = 39,
    MintLimitExceeded = 40,
    NoPendingRotation = 41,
    /// Oracle rotation delay has not elapsed yet (must wait MIN_ROTATION_DELAY_SECONDS)
    RotationDelayNotElapsed = 42,
    /// Invalid archive retention limit
    InvalidArchiveRetention = 43,
    InvalidCommitment = 44,
    InvalidSalt = 45,
    NoRoundTemplate = 46,
    /// Oracle payload timestamp is outside the round-relative economic window
    OracleTimestampOutsideWindow = 47,
    /// Pending winnings entry exists but has not yet reached the configured
    /// expiry threshold — caller must wait before reclaiming.
    PendingWinningsNotExpired = 48,
    /// Epoch mint budget has been fully consumed
    EpochBudgetExceeded = 49,
    /// Oracle heartbeat is not live and strict mode blocks settlement (Issue #264)
    OracleNotLive = 50,
    /// Invalid precision payout policy
    InvalidPayoutPolicy = 51,
    /// Stake amount is below the configured minimum bet (dust protection, Issue #269)
    BelowMinBet = 52,
    /// Multi-feed resolution: fewer observations survived outlier rejection
    /// than the configured quorum threshold.
    InsufficientOracleQuorum = 53,
    /// Multi-feed resolution: payload contains fewer observations than the
    /// configured minimum.
    TooFewObservations = 54,
    /// Multi-feed resolution: outlier observations would dominate the result
    /// (too many rejected, cannot form quorum).
    OracleOutlierRejected = 55,
    /// Multi-feed payload contains duplicate source identifiers.
    DuplicateOracleSource = 56,
    /// Multi-feed payload has observations that are not sorted or sources
    /// are out of expected range.
    InvalidObservationOrder = 57,
    /// The requested data key is not allowed for batch TTL touch operations.
    UnsupportedDataKeyForTtlTouch = 58,
    /// Pending winnings entry does not exist or expiry is not configured.
    PendingWinningsNotFound = 59,
    /// Pending winnings expiry is not configured (value is 0).
    ExpiryNotConfigured = 60,
    /// Participant is blocked by the active allowlist or denylist policy.
    AccessDenied = 61,
    /// Governance proposal does not exist.
    ProposalNotFound = 62,
    /// Governance proposal is past its execution deadline.
    ProposalExpired = 63,
    /// Governance proposal cannot transition from its current state.
    GovInvalidState = 64,
    /// Caller is not authorized by the configured governance policy.
    GovUnauthorized = 65,
    /// Requested action is not valid in the round's current lifecycle phase.
    IllegalPhaseTransition = 66,
    /// Oracle heartbeat failed the configured freshness or health policy.
    OracleHeartbeatUnhealthy = 67,
    /// claim_many batch size exceeds MAX_CLAIM_BATCH_SIZE (Issue #277)
    ClaimBatchTooLarge = 68,
    /// claim_many batch contains the same address more than once (Issue #277)
    DuplicateClaimAddress = 69,
    /// Early cash-out feature is disabled or not configured
    EarlyCashoutDisabled = 70,
    /// User does not have an active position to cash out
    PositionNotFound = 71,
    /// Early cash-out attempted outside the valid running phase
    InvalidPhaseForCashout = 72,
    /// Early cash-out is only supported for UpDown rounds
    WrongModeForCashout = 73,
    /// A proposed insurance payout split does not sum to the covered balance.
    InsuranceInvalidSplit = 74,
    /// The insurance backstop fund has insufficient balance to cover the claim.
    InsuranceInsufficientFund = 75,
    /// The supplied token amount is invalid for the requested operation.
    InvalidAmount = 76,
    /// The dispute window for `void_round` has expired, or dispute windows
    /// are not configured (`dispute_ledgers == 0`).
    DisputeWindowExpired = 77,
    /// `finalize_round` was called before the dispute window elapsed.
    ClaimLocked = 78,
    /// A round cannot be created because the current ledger sequence has
    /// already backed another round's `start_ledger`.
    ///
    /// Oracle payloads bind to `Round.start_ledger`, so reusing a ledger
    /// sequence would make a payload signed for the earlier round valid for
    /// the later one. Retry once the ledger has advanced.
    RoundStartLedgerReused = 79,
    /// Pagination limit exceeds MAX_PAGE_SIZE (Issue #430, gas guard)
    PageSizeExceeded = 80,
}

