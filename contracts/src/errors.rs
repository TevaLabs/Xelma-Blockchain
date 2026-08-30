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
    PendingWinningsNotExpired = 86,
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
    /// Participant is blocked by the active allowlist or denylist policy.
    AccessDenied = 79,
    /// Governance proposal does not exist.
    ProposalNotFound = 80,
    /// Governance proposal is past its execution deadline.
    ProposalExpired = 81,
    /// Governance proposal cannot transition from its current state.
    GovInvalidState = 82,
    /// Caller is not authorized by the configured governance policy.
    GovUnauthorized = 83,
    /// Requested action is not valid in the round's current lifecycle phase.
    IllegalPhaseTransition = 84,
    /// Oracle heartbeat failed the configured freshness or health policy.
    OracleHeartbeatUnhealthy = 85,
    /// Early cash-out feature is disabled or not configured
    EarlyCashoutDisabled = 79,
    /// User does not have an active position to cash out
    PositionNotFound = 80,
    /// Early cash-out attempted outside the valid running phase
    InvalidPhaseForCashout = 81,
    /// Early cash-out is only supported for UpDown rounds
    WrongModeForCashout = 82,
    ProposalNotFound = 83,
    ProposalExpired = 84,
    GovInvalidState = 85,
    GovUnauthorized = 86,
    /// claim_many batch size exceeds MAX_CLAIM_BATCH_SIZE (Issue #277)
    ClaimBatchTooLarge = 87,
    /// claim_many batch contains the same address more than once (Issue #277)
    DuplicateClaimAddress = 88,
    /// Caller is denylisted, or allowlist mode is enabled and caller is not
    /// allowlisted (Issue #274 access-control gate).
    AccessDenied = 89,
    /// Oracle heartbeat is not live and strict mode blocks single-feed
    /// settlement (Issue #264 sibling check for `resolve_round`).
    OracleHeartbeatUnhealthy = 90,
    /// The dispute window for `void_round` has expired, or dispute windows
    /// are not configured (`dispute_ledgers == 0`).
    DisputeWindowExpired = 91,
    /// `finalize_round` was called before the dispute window elapsed.
    ClaimLocked = 92,
    // ─── Blue/green migration (Issue #366) ──────────────────────────────────
    /// A migration commitment has not been finalized yet.
    MigrationNotFinalized = 93,
    /// The migration has already been finalized; further commits are rejected.
    MigrationAlreadyFinalized = 94,
    /// Attempted migration action failed an authorization check.
    MigrationUnauthorized = 95,
    /// The supplied proof/commitment does not match the expected migration root.
    MigrationCommitmentMismatch = 96,
    /// The migration source/destination version does not match what is in flight.
    MigrationVersionMismatch = 97,
    /// The source contract is already in the migration drain/frozen state.
    MigrationAlreadyFrozen = 98,
    /// The destination migration session has not been initialized.
    MigrationNotInitialized = 99,
    /// The destination migration session was already initialized.
    MigrationAlreadyInitialized = 100,
    /// A record has already been imported into the destination contract.
    MigrationRecordAlreadyImported = 101,
    /// The supplied Merkle proof is malformed or does not verify.
    MigrationProofInvalid = 102,
    /// The source-contract export has not produced a finalized commitment.
    MigrationExportIncomplete = 103,
    /// The source contract is frozen for migration; this action is disabled.
    MigrationFrozen = 104,
    /// The destination contract is not ready to accept imports.
    MigrationNotReady = 105,
    /// A canonical record was supplied for a value that does not match on-chain state.
    MigrationRecordMismatch = 106,
    /// Someone attempted to re-open rounds in a contract already migrated.
    MigrationAlreadyMigrated = 107,
}
