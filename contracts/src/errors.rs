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
    /// claim_many batch size exceeds MAX_CLAIM_BATCH_SIZE (Issue #277)
    ClaimBatchTooLarge = 87,
    /// claim_many batch contains the same address more than once (Issue #277)
    DuplicateClaimAddress = 88,
    /// Early cash-out feature is disabled or not configured (code 95 / 0x5f).
    EarlyCashoutDisabled = 95,
    /// User does not have an active position to cash out (code 96 / 0x60).
    PositionNotFound = 96,
    /// Early cash-out attempted outside the valid running phase (code 97 / 0x61).
    InvalidPhaseForCashout = 97,
    /// Early cash-out is only supported for UpDown rounds (code 98 / 0x62).
    WrongModeForCashout = 98,
    /// A proposed insurance payout split does not sum to the covered balance.
    InsuranceInvalidSplit = 99,
    /// The insurance backstop fund has insufficient balance to cover the claim.
    InsuranceInsufficientFund = 100,
    /// The supplied token amount is invalid for the requested operation.
    InvalidAmount = 101,
    /// The close-buffer has frozen betting before the round's betting window ends.
    BettingClosed = 102,
    /// The dispute window for `void_round` has expired, or dispute windows
    /// are not configured (`dispute_ledgers == 0`).
    DisputeWindowExpired = 91,
    /// `finalize_round` was called before the dispute window elapsed.
    ClaimLocked = 92,
    /// A round cannot be created because the current ledger sequence has
    /// already backed another round's `start_ledger`.
    ///
    /// Oracle payloads bind to `Round.start_ledger`, so reusing a ledger
    /// sequence would make a payload signed for the earlier round valid for
    /// the later one. Retry once the ledger has advanced.
    RoundStartLedgerReused = 93,
    /// Pagination limit exceeds MAX_PAGE_SIZE (Issue #430, gas guard)
    PageSizeExceeded = 94,
}

impl ContractError {
    /// Returns the unique stable numeric `u32` code for this `ContractError`.
    #[inline]
    pub const fn code(self) -> u32 {
        self as u32
    }
}

#[cfg(test)]
mod tests {
    use super::ContractError;
    use std::collections::HashSet;

    #[test]
    fn every_contract_error_has_unique_stable_code() {
        let table: &[(ContractError, u32)] = &[
            (ContractError::AlreadyInitialized, 1),
            (ContractError::AdminNotSet, 2),
            (ContractError::OracleNotSet, 3),
            (ContractError::InvalidBetAmount, 6),
            (ContractError::NoActiveRound, 7),
            (ContractError::RoundEnded, 8),
            (ContractError::InsufficientBalance, 9),
            (ContractError::AlreadyBet, 10),
            (ContractError::Overflow, 11),
            (ContractError::InvalidPrice, 12),
            (ContractError::InvalidDuration, 13),
            (ContractError::InvalidMode, 14),
            (ContractError::WrongModeForPrediction, 15),
            (ContractError::RoundNotEnded, 16),
            (ContractError::StaleOracleData, 18),
            (ContractError::InvalidOracleRound, 19),
            (ContractError::RoundAlreadyActive, 20),
            (ContractError::ContractPaused, 22),
            (ContractError::WindowOutOfRange, 23),
            (ContractError::FutureOracleData, 24),
            (ContractError::PayoutOverflow, 25),
            (ContractError::RoundNotCancellable, 27),
            (ContractError::StakeExceedsMax, 28),
            (ContractError::ExposureCapExceeded, 29),
            (ContractError::PendingWinningsCapExceeded, 30),
            (ContractError::InvalidStartPrice, 31),
            (ContractError::OracleNonceReused, 33),
            (ContractError::InvalidMinParticipants, 35),
            (ContractError::InvalidPrecisionCap, 38),
            (ContractError::PrecisionCapExceeded, 39),
            (ContractError::OracleDeviationExceeded, 41),
            (ContractError::UnsupportedSchemaVersion, 42),
            (ContractError::MigrationActiveRound, 44),
            (ContractError::CommitmentNotFound, 45),
            (ContractError::AlreadyRevealed, 46),
            (ContractError::InvalidRevealWindow, 47),
            (ContractError::HashMismatch, 48),
            (ContractError::OracleNetworkMismatch, 49),
            (ContractError::InvalidProtocolFeeBps, 51),
            (ContractError::MintLimitExceeded, 53),
            (ContractError::NoPendingRotation, 54),
            (ContractError::RotationDelayNotElapsed, 55),
            (ContractError::InvalidArchiveRetention, 62),
            (ContractError::InvalidCommitment, 63),
            (ContractError::InvalidSalt, 64),
            (ContractError::NoRoundTemplate, 65),
            (ContractError::OracleTimestampOutsideWindow, 66),
            (ContractError::PendingWinningsNotExpired, 86),
            (ContractError::EpochBudgetExceeded, 67),
            (ContractError::OracleNotLive, 68),
            (ContractError::InvalidPayoutPolicy, 69),
            (ContractError::BelowMinBet, 70),
            (ContractError::InsufficientOracleQuorum, 71),
            (ContractError::TooFewObservations, 72),
            (ContractError::OracleOutlierRejected, 73),
            (ContractError::DuplicateOracleSource, 74),
            (ContractError::InvalidObservationOrder, 75),
            (ContractError::UnsupportedDataKeyForTtlTouch, 76),
            (ContractError::PendingWinningsNotFound, 77),
            (ContractError::ExpiryNotConfigured, 78),
            (ContractError::AccessDenied, 79),
            (ContractError::ProposalNotFound, 80),
            (ContractError::ProposalExpired, 81),
            (ContractError::GovInvalidState, 82),
            (ContractError::GovUnauthorized, 83),
            (ContractError::IllegalPhaseTransition, 84),
            (ContractError::OracleHeartbeatUnhealthy, 85),
            (ContractError::ClaimBatchTooLarge, 87),
            (ContractError::DuplicateClaimAddress, 88),
            (ContractError::EarlyCashoutDisabled, 95),
            (ContractError::PositionNotFound, 96),
            (ContractError::InvalidPhaseForCashout, 97),
            (ContractError::WrongModeForCashout, 98),
            (ContractError::InsuranceInvalidSplit, 99),
            (ContractError::InsuranceInsufficientFund, 100),
            (ContractError::InvalidAmount, 101),
            (ContractError::BettingClosed, 102),
            (ContractError::DisputeWindowExpired, 91),
            (ContractError::ClaimLocked, 92),
            (ContractError::RoundStartLedgerReused, 93),
            (ContractError::PageSizeExceeded, 94),
        ];
        let mut seen = HashSet::with_capacity(table.len());
        for &(err, expected_code) in table {
            assert_eq!(err.code(), expected_code);
            assert!(seen.insert(expected_code), "duplicate ContractError code: {}", expected_code);
        }
    }
}
