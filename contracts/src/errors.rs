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
    /// Pending winnings entry exists but has not yet reached the configured
    /// expiry threshold — caller must wait before reclaiming.
    PendingWinningsNotExpired = 86,
    /// claim_many batch size exceeds MAX_CLAIM_BATCH_SIZE (Issue #277)
    ClaimBatchTooLarge = 87,
    /// claim_many batch contains the same address more than once (Issue #277)
    DuplicateClaimAddress = 88,
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
    /// Early cash-out feature is disabled or not configured
    EarlyCashoutDisabled = 95,
    /// User does not have an active position to cash out
    PositionNotFound = 96,
    /// Early cash-out attempted outside the valid running phase
    InvalidPhaseForCashout = 97,
    /// Early cash-out is only supported for UpDown rounds
    WrongModeForCashout = 98,
    /// A proposed insurance payout split does not sum to the covered balance.
    InsuranceInvalidSplit = 99,
    /// The insurance backstop fund has insufficient balance to cover the claim.
    InsuranceInsufficientFund = 100,
    /// The supplied token amount is invalid for the requested operation.
    InvalidAmount = 101,
    /// The close-buffer has frozen betting before the round's betting window ends.
    BettingClosed = 102,
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
mod tests {
    use super::ContractError;
    use std::collections::HashSet;

    const ALL_VARIANTS: &[(ContractError, u32, &str)] = &[
        (ContractError::AlreadyInitialized, 1, "AlreadyInitialized"),
        (ContractError::AdminNotSet, 2, "AdminNotSet"),
        (ContractError::OracleNotSet, 3, "OracleNotSet"),
        (ContractError::InvalidBetAmount, 6, "InvalidBetAmount"),
        (ContractError::NoActiveRound, 7, "NoActiveRound"),
        (ContractError::RoundEnded, 8, "RoundEnded"),
        (ContractError::InsufficientBalance, 9, "InsufficientBalance"),
        (ContractError::AlreadyBet, 10, "AlreadyBet"),
        (ContractError::Overflow, 11, "Overflow"),
        (ContractError::InvalidPrice, 12, "InvalidPrice"),
        (ContractError::InvalidDuration, 13, "InvalidDuration"),
        (ContractError::InvalidMode, 14, "InvalidMode"),
        (
            ContractError::WrongModeForPrediction,
            15,
            "WrongModeForPrediction",
        ),
        (ContractError::RoundNotEnded, 16, "RoundNotEnded"),
        (ContractError::StaleOracleData, 18, "StaleOracleData"),
        (ContractError::InvalidOracleRound, 19, "InvalidOracleRound"),
        (ContractError::RoundAlreadyActive, 20, "RoundAlreadyActive"),
        (ContractError::ContractPaused, 22, "ContractPaused"),
        (ContractError::WindowOutOfRange, 23, "WindowOutOfRange"),
        (ContractError::FutureOracleData, 24, "FutureOracleData"),
        (ContractError::PayoutOverflow, 25, "PayoutOverflow"),
        (
            ContractError::RoundNotCancellable,
            27,
            "RoundNotCancellable",
        ),
        (ContractError::StakeExceedsMax, 28, "StakeExceedsMax"),
        (
            ContractError::ExposureCapExceeded,
            29,
            "ExposureCapExceeded",
        ),
        (
            ContractError::PendingWinningsCapExceeded,
            30,
            "PendingWinningsCapExceeded",
        ),
        (ContractError::InvalidStartPrice, 31, "InvalidStartPrice"),
        (ContractError::OracleNonceReused, 33, "OracleNonceReused"),
        (
            ContractError::InvalidMinParticipants,
            35,
            "InvalidMinParticipants",
        ),
        (
            ContractError::InvalidPrecisionCap,
            38,
            "InvalidPrecisionCap",
        ),
        (
            ContractError::PrecisionCapExceeded,
            39,
            "PrecisionCapExceeded",
        ),
        (
            ContractError::OracleDeviationExceeded,
            41,
            "OracleDeviationExceeded",
        ),
        (
            ContractError::UnsupportedSchemaVersion,
            42,
            "UnsupportedSchemaVersion",
        ),
        (
            ContractError::MigrationActiveRound,
            44,
            "MigrationActiveRound",
        ),
        (ContractError::CommitmentNotFound, 45, "CommitmentNotFound"),
        (ContractError::AlreadyRevealed, 46, "AlreadyRevealed"),
        (
            ContractError::InvalidRevealWindow,
            47,
            "InvalidRevealWindow",
        ),
        (ContractError::HashMismatch, 48, "HashMismatch"),
        (
            ContractError::OracleNetworkMismatch,
            49,
            "OracleNetworkMismatch",
        ),
        (
            ContractError::InvalidProtocolFeeBps,
            51,
            "InvalidProtocolFeeBps",
        ),
        (ContractError::MintLimitExceeded, 53, "MintLimitExceeded"),
        (ContractError::NoPendingRotation, 54, "NoPendingRotation"),
        (
            ContractError::RotationDelayNotElapsed,
            55,
            "RotationDelayNotElapsed",
        ),
        (
            ContractError::InvalidArchiveRetention,
            62,
            "InvalidArchiveRetention",
        ),
        (ContractError::InvalidCommitment, 63, "InvalidCommitment"),
        (ContractError::InvalidSalt, 64, "InvalidSalt"),
        (ContractError::NoRoundTemplate, 65, "NoRoundTemplate"),
        (
            ContractError::OracleTimestampOutsideWindow,
            66,
            "OracleTimestampOutsideWindow",
        ),
        (
            ContractError::EpochBudgetExceeded,
            67,
            "EpochBudgetExceeded",
        ),
        (ContractError::OracleNotLive, 68, "OracleNotLive"),
        (
            ContractError::InvalidPayoutPolicy,
            69,
            "InvalidPayoutPolicy",
        ),
        (ContractError::BelowMinBet, 70, "BelowMinBet"),
        (
            ContractError::InsufficientOracleQuorum,
            71,
            "InsufficientOracleQuorum",
        ),
        (ContractError::TooFewObservations, 72, "TooFewObservations"),
        (
            ContractError::OracleOutlierRejected,
            73,
            "OracleOutlierRejected",
        ),
        (
            ContractError::DuplicateOracleSource,
            74,
            "DuplicateOracleSource",
        ),
        (
            ContractError::InvalidObservationOrder,
            75,
            "InvalidObservationOrder",
        ),
        (
            ContractError::UnsupportedDataKeyForTtlTouch,
            76,
            "UnsupportedDataKeyForTtlTouch",
        ),
        (
            ContractError::PendingWinningsNotFound,
            77,
            "PendingWinningsNotFound",
        ),
        (
            ContractError::ExpiryNotConfigured,
            78,
            "ExpiryNotConfigured",
        ),
        (ContractError::AccessDenied, 79, "AccessDenied"),
        (ContractError::ProposalNotFound, 80, "ProposalNotFound"),
        (ContractError::ProposalExpired, 81, "ProposalExpired"),
        (ContractError::GovInvalidState, 82, "GovInvalidState"),
        (ContractError::GovUnauthorized, 83, "GovUnauthorized"),
        (
            ContractError::IllegalPhaseTransition,
            84,
            "IllegalPhaseTransition",
        ),
        (
            ContractError::OracleHeartbeatUnhealthy,
            85,
            "OracleHeartbeatUnhealthy",
        ),
        (
            ContractError::PendingWinningsNotExpired,
            86,
            "PendingWinningsNotExpired",
        ),
        (ContractError::ClaimBatchTooLarge, 87, "ClaimBatchTooLarge"),
        (
            ContractError::DuplicateClaimAddress,
            88,
            "DuplicateClaimAddress",
        ),
        (
            ContractError::DisputeWindowExpired,
            91,
            "DisputeWindowExpired",
        ),
        (ContractError::ClaimLocked, 92, "ClaimLocked"),
        (
            ContractError::RoundStartLedgerReused,
            93,
            "RoundStartLedgerReused",
        ),
        (ContractError::PageSizeExceeded, 94, "PageSizeExceeded"),
        (
            ContractError::EarlyCashoutDisabled,
            95,
            "EarlyCashoutDisabled",
        ),
        (ContractError::PositionNotFound, 96, "PositionNotFound"),
        (
            ContractError::InvalidPhaseForCashout,
            97,
            "InvalidPhaseForCashout",
        ),
        (
            ContractError::WrongModeForCashout,
            98,
            "WrongModeForCashout",
        ),
        (
            ContractError::InsuranceInvalidSplit,
            99,
            "InsuranceInvalidSplit",
        ),
        (
            ContractError::InsuranceInsufficientFund,
            100,
            "InsuranceInsufficientFund",
        ),
        (ContractError::InvalidAmount, 101, "InvalidAmount"),
        (ContractError::BettingClosed, 102, "BettingClosed"),
    ];

    #[test]
    fn test_contract_error_repr_codes_are_unique_and_ordered() {
        let mut seen_codes = HashSet::new();
        let mut prev_code = 0u32;

        for &(variant, expected_code, name) in ALL_VARIANTS {
            let actual_code = variant as u32;
            assert_eq!(
                actual_code, expected_code,
                "ContractError::{name} expected repr code {expected_code}, got {actual_code}"
            );
            assert!(
                seen_codes.insert(actual_code),
                "Duplicate ContractError repr code {actual_code} for {name}"
            );
            assert!(
                actual_code > prev_code,
                "ContractError::{name} ({actual_code}) must be ordered after {prev_code}"
            );
            prev_code = actual_code;
        }
    }

    #[test]
    fn test_contract_error_code_66_disambiguation() {
        assert_eq!(ContractError::OracleTimestampOutsideWindow as u32, 66);
        assert_eq!(ContractError::EpochBudgetExceeded as u32, 67);
        assert_eq!(ContractError::OracleNotLive as u32, 68);
        assert_eq!(ContractError::PendingWinningsNotExpired as u32, 86);

        assert_ne!(
            ContractError::OracleTimestampOutsideWindow as u32,
            ContractError::PendingWinningsNotExpired as u32
        );
        assert_ne!(
            ContractError::OracleTimestampOutsideWindow as u32,
            ContractError::OracleNotLive as u32
        );
        assert_ne!(
            ContractError::OracleTimestampOutsideWindow as u32,
            ContractError::EpochBudgetExceeded as u32
        );
    }
}
