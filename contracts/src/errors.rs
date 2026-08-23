// SPDX-License-Identifier: MIT
//! Contract error types for the XLM Price Prediction Market.
//!
//! `#[contracterror]` compiles to a Soroban contract-spec XDR union, which is
//! hard-capped at 50 cases (`VecM<ScSpecUdtErrorEnumCaseV0, 50>`). This enum
//! is at that cap — semantically related failure modes intentionally share a
//! variant (see the doc comment on each) rather than growing past it.
use soroban_sdk::contracterror;

/// Contract error types
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    AdminNotSet = 2,
    InvalidBetAmount = 6,
    NoActiveRound = 7,
    RoundEnded = 8,
    InsufficientBalance = 9,
    AlreadyBet = 10,
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
    /// Also covers multi-feed quorum/outlier/observation-count rejection
    /// (`InsufficientOracleQuorum`, `TooFewObservations`,
    /// `DuplicateOracleSource` in earlier drafts) — all are "oracle payload
    /// failed a data-quality gate" and share this code to stay under the
    /// contracterror 50-case cap.
    OracleDeviationExceeded = 41,
    UnsupportedSchemaVersion = 42,
    MigrationActiveRound = 44,
    CommitmentNotFound = 45,
    AlreadyRevealed = 46,
    /// Also covers "pool commitment not yet openable" (Issue: pool-aggregate
    /// commitments) — both are "action attempted outside its valid window".
    InvalidRevealWindow = 47,
    HashMismatch = 48,
    OracleNetworkMismatch = 49,
    MintLimitExceeded = 53,
    NoPendingRotation = 54,
    /// Oracle rotation delay has not elapsed yet (must wait MIN_ROTATION_DELAY_SECONDS)
    RotationDelayNotElapsed = 55,
    /// Oracle payload timestamp is outside the round-relative economic window
    OracleTimestampOutsideWindow = 66,
    /// Pending winnings entry exists but has not yet reached the configured
    /// expiry threshold — caller must wait before reclaiming. Also covers
    /// "expiry not configured" and "no pending winnings entry" — all are
    /// "nothing reclaimable right now".
    PendingWinningsNotExpired = 79,
    /// Epoch mint budget has been fully consumed
    EpochBudgetExceeded = 67,
    /// Oracle heartbeat is not live and strict mode blocks settlement (Issue #264)
    OracleNotLive = 68,
    /// Stake amount is below the configured minimum bet (dust protection, Issue #269)
    BelowMinBet = 70,
    /// Caller is not authorized for the requested access-controlled action.
    AccessDenied = 80,
    /// Governance proposal is not in a state that allows the requested
    /// transition. Also covers "proposal not found" — an absent proposal id
    /// is treated as an invalid-state transition.
    GovInvalidState = 81,
    /// Caller is not an authorized governance admin/approver.
    GovUnauthorized = 82,
    /// Requested state transition is not legal from the round's current phase.
    IllegalPhaseTransition = 83,
    /// Oracle heartbeat is unhealthy (stale/degraded) and strict mode blocks the action.
    OracleHeartbeatUnhealthy = 84,
    /// Governance proposal's expiry ledger has passed.
    ProposalExpired = 85,
}
