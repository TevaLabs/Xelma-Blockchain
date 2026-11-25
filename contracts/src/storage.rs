// SPDX-License-Identifier: MIT
//! Mode-agnostic position repository — unified storage helpers for UpDown and
//! Precision modes.
//!
//! The canonical cleanup functions in this module ensure that **all**
//! position storage keys are removed for every participant, regardless of
//! the active round mode.  This prevents stale data cross-contamination
//! when the protocol alternates between UpDown and Precision rounds.
//!
//! Every call-site that previously duplicated the three-key removal pattern
//! (`Position` + `PrecisionPosition` + `PrecisionCommitment`) should route
//! through `clear_user_positions` or `clear_round_storage`.

use crate::types::DataKeyCore;
use crate::types::DataKeyExt;
use crate::types::DataKeyScoped;
use soroban_sdk::{Address, Env, Vec};

/// Removes **all** position storage keys for a single participant,
/// regardless of the round's mode.
///
/// Keys removed:
/// - `Position(round_id, user)`       — UpDown
/// - `PrecisionPosition(round_id, user)` — Precision (revealed)
/// - `PrecisionCommitment(round_id, user)` — Precision (unrevealed)
///
/// Safe to call even when some keys don't exist (`.remove()` is a no-op
/// on a missing key).
#[inline]
pub fn clear_user_positions(env: &Env, round_id: u64, user: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKeyScoped::Position(round_id, user.clone()));
    env.storage()
        .persistent()
        .remove(&DataKeyScoped::PrecisionPosition(round_id, user.clone()));
    env.storage()
        .persistent()
        .remove(&DataKeyScoped::PrecisionCommitment(round_id, user.clone()));
}

/// Removes all position storage keys for every participant in a round,
/// along with the shared participant list, the active round marker, and
/// any legacy storage keys.
///
/// This is the **canonical round cleanup** — call it once after
/// settlement, cancellation, or fallback refund.
///
/// Keys removed (per participant):
/// - `Position`, `PrecisionPosition`, `PrecisionCommitment`
///
/// Shared keys removed:
/// - `RoundParticipants(round_id)`
/// - `ActiveRound`
/// - `Positions` (legacy)
/// - `UpDownPositions` (legacy)
/// - `PrecisionPositions` (legacy)
pub fn clear_round_storage(env: &Env, round_id: u64, participants: &Vec<Address>) {
    // Clear per-user position keys (both modes — no stale data)
    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            clear_user_positions(env, round_id, &user);
        }
    }

    // Clear shared keys
    env.storage()
        .persistent()
        .remove(&DataKeyScoped::RoundParticipants(round_id));
    env.storage().persistent().remove(&DataKeyCore::ActiveRound);

    // Legacy keys — safe no-op when absent
    env.storage().persistent().remove(&DataKeyCore::Positions);
    env.storage()
        .persistent()
        .remove(&DataKeyCore::UpDownPositions);
    env.storage()
        .persistent()
        .remove(&DataKeyCore::PrecisionPositions);
}

// ─── TTL touch allowlist (Issue #862) ──────────────────────────────────────

/// How a `DataKeyCore` variant is treated by `batch_touch_ttl`.
///
/// Every variant is classified explicitly in [`ttl_touch_policy`] with no
/// wildcard arm, so adding a new `DataKeyCore` or `DataKeyExt` variant fails
/// to compile until someone decides whether maintainers may batch-touch it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtlTouchPolicy {
    /// Long-lived, contract-wide persistent singleton: accepted by
    /// `batch_touch_ttl` and extended when present.
    Allowed,
    /// Accepted for backward compatibility, but the value lives in
    /// **instance** storage, so the persistent entry never exists and the key
    /// is always counted as skipped. Instance storage is kept alive with the
    /// contract instance, not by this entrypoint.
    AllowedInstanceTier,
    /// Round-scoped state that is created and deleted per round and extended
    /// on access (`ActiveRound`).
    RoundScoped,
    /// Read-only legacy keys kept for migration compatibility; removed during
    /// round cleanup.
    Legacy,
    /// Variant exists in the enum but its value is stored under a different
    /// key (a `Symbol`), so there is never an entry to touch.
    NotStoredUnderVariant,
    /// Parameterised (per-season / per-user / per-id) entries, extended on
    /// access rather than in maintenance batches.
    Indexed,
}

/// Exhaustive TTL-touch classification for every `DataKeyCore` variant.
pub fn ttl_touch_policy(key: &DataKeyCore) -> TtlTouchPolicy {
    use TtlTouchPolicy::*;
    match key {
        // Core config
        DataKeyCore::Admin
        | DataKeyCore::Oracle
        | DataKeyCore::SchemaVersion
        | DataKeyCore::NextSchemaVersion
        | DataKeyCore::Paused
        | DataKeyCore::MigratedToV3 => Allowed,
        // Windows
        DataKeyCore::BetWindowLedgers
        | DataKeyCore::RunWindowLedgers
        | DataKeyCore::CloseBufferLedgers => Allowed,
        // Risk limits and matchmaking
        DataKeyCore::MaxStake
        | DataKeyCore::MaxUserRoundExposure
        | DataKeyCore::MaxPendingWinnings
        | DataKeyCore::MinBet
        | DataKeyCore::MinParticipants
        | DataKeyCore::MaxPrecisionParticipants => Allowed,
        // Oracle safety
        DataKeyCore::OracleHeartbeat
        | DataKeyCore::OracleStaleThreshold
        | DataKeyCore::OracleMaxDeviationBps
        | DataKeyCore::OracleDeviationOverrideArmed
        | DataKeyCore::OracleMinConfidenceBps
        | DataKeyCore::OracleStrictMode
        | DataKeyCore::OracleQuorum
        | DataKeyCore::OracleRotationProposal => Allowed,
        // Fees and payout policy
        DataKeyCore::ProtocolFeeBps
        | DataKeyCore::ProtocolFeeTreasury
        | DataKeyCore::FeeModel
        | DataKeyCore::EarlyCashoutBps
        | DataKeyCore::PrecisionPayoutPolicy => Allowed,
        // Archive / keeper / round counter
        DataKeyCore::ArchiveRetention
        | DataKeyCore::RecentArchivedRoundIds
        | DataKeyCore::RoundTemplate
        | DataKeyCore::LastRoundId => Allowed,
        // Access control and governance
        DataKeyCore::AccessControlEnabled
        | DataKeyCore::GovApprover
        | DataKeyCore::GovProposalTtlLedgers
        | DataKeyCore::NextGovProposalId => Allowed,
        // Instance-tier value (see `set_mint_limit_config`)
        DataKeyCore::MintLimitConfig => AllowedInstanceTier,
        // Round-scoped
        DataKeyCore::ActiveRound => RoundScoped,
        // Legacy migration keys
        DataKeyCore::Positions | DataKeyCore::UpDownPositions | DataKeyCore::PrecisionPositions => {
            Legacy
        }
        // Stored under Symbol keys ("EpMintBgt" in instance, "DisputeLedgers")
        DataKeyCore::EpochMintBudget | DataKeyCore::DisputeLedgers => NotStoredUnderVariant,
        DataKeyCore::Ext(ext) => match ext {
            DataKeyExt::LeaderboardWins
            | DataKeyExt::LeaderboardStreak
            | DataKeyExt::SeasonId
            | DataKeyExt::SeasonLeaderboardWins
            | DataKeyExt::SeasonLeaderboardStreak
            | DataKeyExt::ConstitutionMetadata
            | DataKeyExt::NextAmendmentId => Allowed,
            DataKeyExt::SeasonUserStats(_, _)
            | DataKeyExt::SeasonArchive(_)
            | DataKeyExt::Amendment(_) => Indexed,
        },
    }
}

/// Returns `true` when `batch_touch_ttl` accepts `key`.
#[inline]
pub fn is_ttl_touch_allowed(key: &DataKeyCore) -> bool {
    matches!(
        ttl_touch_policy(key),
        TtlTouchPolicy::Allowed | TtlTouchPolicy::AllowedInstanceTier
    )
}
