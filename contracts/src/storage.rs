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

/// Returns whether a system key can be proactively extended through the
/// admin-gated batch TTL maintenance entrypoint.
pub fn is_ttl_touch_allowed(key: &DataKeyCore) -> bool {
    matches!(
        key,
        DataKeyCore::Admin
            | DataKeyCore::Oracle
            | DataKeyCore::SchemaVersion
            | DataKeyCore::Paused
            | DataKeyCore::BetWindowLedgers
            | DataKeyCore::RunWindowLedgers
            | DataKeyCore::CloseBufferLedgers
            | DataKeyCore::LastRoundId
            | DataKeyCore::MaxStake
            | DataKeyCore::MaxUserRoundExposure
            | DataKeyCore::MaxPendingWinnings
            | DataKeyCore::MinParticipants
            | DataKeyCore::OracleHeartbeat
            | DataKeyCore::OracleStaleThreshold
            | DataKeyCore::MaxPrecisionParticipants
            | DataKeyCore::OracleMaxDeviationBps
            | DataKeyCore::OracleDeviationOverrideArmed
            | DataKeyCore::OracleMinConfidenceBps
            | DataKeyCore::OracleStrictMode
            | DataKeyCore::MigratedToV3
            | DataKeyCore::ProtocolFeeBps
            | DataKeyCore::ProtocolFeeTreasury
            | DataKeyCore::MintLimitConfig
            | DataKeyCore::OracleRotationProposal
            | DataKeyCore::ArchiveRetention
            | DataKeyCore::RoundTemplate
            | DataKeyCore::OracleQuorum
            | DataKeyCore::MinBet
            | DataKeyCore::EpochMintBudget
            | DataKeyCore::EarlyCashoutBps
            | DataKeyCore::FeeModel
            | DataKeyCore::DisputeLedgers
            | DataKeyCore::PrecisionPayoutPolicy
            | DataKeyCore::AccessControlEnabled
            | DataKeyCore::GovApprover
            | DataKeyCore::GovProposalTtlLedgers
            | DataKeyCore::NextGovProposalId
            | DataKeyCore::Ext(DataKeyExt::LeaderboardWins)
            | DataKeyCore::Ext(DataKeyExt::LeaderboardStreak)
            | DataKeyCore::Ext(DataKeyExt::SeasonId)
            | DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardWins)
            | DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardStreak)
            | DataKeyCore::Ext(DataKeyExt::ConstitutionMetadata)
            | DataKeyCore::Ext(DataKeyExt::NextAmendmentId)
    )
}

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

#[cfg(test)]
mod tests {
    use super::is_ttl_touch_allowed;
    use crate::types::{DataKeyCore, DataKeyExt};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};

    #[test]
    fn ttl_touch_allowlist_covers_every_storage_key_variant() {
        let env = Env::default();
        let address = Address::generate(&env);
        let keys = [
            DataKeyCore::Admin,
            DataKeyCore::Oracle,
            DataKeyCore::SchemaVersion,
            DataKeyCore::ActiveRound,
            DataKeyCore::Positions,
            DataKeyCore::UpDownPositions,
            DataKeyCore::PrecisionPositions,
            DataKeyCore::Paused,
            DataKeyCore::BetWindowLedgers,
            DataKeyCore::RunWindowLedgers,
            DataKeyCore::CloseBufferLedgers,
            DataKeyCore::LastRoundId,
            DataKeyCore::MaxStake,
            DataKeyCore::MaxUserRoundExposure,
            DataKeyCore::MaxPendingWinnings,
            DataKeyCore::MinParticipants,
            DataKeyCore::OracleHeartbeat,
            DataKeyCore::OracleStaleThreshold,
            DataKeyCore::MaxPrecisionParticipants,
            DataKeyCore::OracleMaxDeviationBps,
            DataKeyCore::OracleDeviationOverrideArmed,
            DataKeyCore::OracleMinConfidenceBps,
            DataKeyCore::OracleStrictMode,
            DataKeyCore::RecentArchivedRoundIds,
            DataKeyCore::MigratedToV3,
            DataKeyCore::ProtocolFeeBps,
            DataKeyCore::ProtocolFeeTreasury,
            DataKeyCore::MintLimitConfig,
            DataKeyCore::OracleRotationProposal,
            DataKeyCore::ArchiveRetention,
            DataKeyCore::RoundTemplate,
            DataKeyCore::OracleQuorum,
            DataKeyCore::NextSchemaVersion,
            DataKeyCore::MinBet,
            DataKeyCore::EpochMintBudget,
            DataKeyCore::EarlyCashoutBps,
            DataKeyCore::FeeModel,
            DataKeyCore::DisputeLedgers,
            DataKeyCore::PrecisionPayoutPolicy,
            DataKeyCore::AccessControlEnabled,
            DataKeyCore::GovApprover,
            DataKeyCore::GovProposalTtlLedgers,
            DataKeyCore::NextGovProposalId,
            DataKeyCore::Ext(DataKeyExt::LeaderboardWins),
            DataKeyCore::Ext(DataKeyExt::LeaderboardStreak),
            DataKeyCore::Ext(DataKeyExt::SeasonId),
            DataKeyCore::Ext(DataKeyExt::SeasonUserStats(0, address.clone())),
            DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardWins),
            DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardStreak),
            DataKeyCore::Ext(DataKeyExt::SeasonArchive(0)),
            DataKeyCore::Ext(DataKeyExt::ConstitutionMetadata),
            DataKeyCore::Ext(DataKeyExt::Amendment(0)),
            DataKeyCore::Ext(DataKeyExt::NextAmendmentId),
        ];

        for key in keys.iter() {
            let (name, expected) = match key {
                DataKeyCore::Admin => ("Admin", true),
                DataKeyCore::Oracle => ("Oracle", true),
                DataKeyCore::SchemaVersion => ("SchemaVersion", true),
                DataKeyCore::ActiveRound => ("ActiveRound", false),
                DataKeyCore::Positions => ("Positions", false),
                DataKeyCore::UpDownPositions => ("UpDownPositions", false),
                DataKeyCore::PrecisionPositions => ("PrecisionPositions", false),
                DataKeyCore::Paused => ("Paused", true),
                DataKeyCore::BetWindowLedgers => ("BetWindowLedgers", true),
                DataKeyCore::RunWindowLedgers => ("RunWindowLedgers", true),
                DataKeyCore::CloseBufferLedgers => ("CloseBufferLedgers", true),
                DataKeyCore::LastRoundId => ("LastRoundId", true),
                DataKeyCore::MaxStake => ("MaxStake", true),
                DataKeyCore::MaxUserRoundExposure => ("MaxUserRoundExposure", true),
                DataKeyCore::MaxPendingWinnings => ("MaxPendingWinnings", true),
                DataKeyCore::MinParticipants => ("MinParticipants", true),
                DataKeyCore::OracleHeartbeat => ("OracleHeartbeat", true),
                DataKeyCore::OracleStaleThreshold => ("OracleStaleThreshold", true),
                DataKeyCore::MaxPrecisionParticipants => ("MaxPrecisionParticipants", true),
                DataKeyCore::OracleMaxDeviationBps => ("OracleMaxDeviationBps", true),
                DataKeyCore::OracleDeviationOverrideArmed => ("OracleDeviationOverrideArmed", true),
                DataKeyCore::OracleMinConfidenceBps => ("OracleMinConfidenceBps", true),
                DataKeyCore::OracleStrictMode => ("OracleStrictMode", true),
                DataKeyCore::RecentArchivedRoundIds => ("RecentArchivedRoundIds", false),
                DataKeyCore::MigratedToV3 => ("MigratedToV3", true),
                DataKeyCore::ProtocolFeeBps => ("ProtocolFeeBps", true),
                DataKeyCore::ProtocolFeeTreasury => ("ProtocolFeeTreasury", true),
                DataKeyCore::MintLimitConfig => ("MintLimitConfig", true),
                DataKeyCore::OracleRotationProposal => ("OracleRotationProposal", true),
                DataKeyCore::ArchiveRetention => ("ArchiveRetention", true),
                DataKeyCore::RoundTemplate => ("RoundTemplate", true),
                DataKeyCore::OracleQuorum => ("OracleQuorum", true),
                DataKeyCore::NextSchemaVersion => ("NextSchemaVersion", false),
                DataKeyCore::MinBet => ("MinBet", true),
                DataKeyCore::EpochMintBudget => ("EpochMintBudget", true),
                DataKeyCore::EarlyCashoutBps => ("EarlyCashoutBps", true),
                DataKeyCore::FeeModel => ("FeeModel", true),
                DataKeyCore::DisputeLedgers => ("DisputeLedgers", true),
                DataKeyCore::PrecisionPayoutPolicy => ("PrecisionPayoutPolicy", true),
                DataKeyCore::AccessControlEnabled => ("AccessControlEnabled", true),
                DataKeyCore::GovApprover => ("GovApprover", true),
                DataKeyCore::GovProposalTtlLedgers => ("GovProposalTtlLedgers", true),
                DataKeyCore::NextGovProposalId => ("NextGovProposalId", true),
                DataKeyCore::Ext(DataKeyExt::LeaderboardWins) => ("LeaderboardWins", true),
                DataKeyCore::Ext(DataKeyExt::LeaderboardStreak) => ("LeaderboardStreak", true),
                DataKeyCore::Ext(DataKeyExt::SeasonId) => ("SeasonId", true),
                DataKeyCore::Ext(DataKeyExt::SeasonUserStats(_, _)) => ("SeasonUserStats", false),
                DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardWins) => {
                    ("SeasonLeaderboardWins", true)
                }
                DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardStreak) => {
                    ("SeasonLeaderboardStreak", true)
                }
                DataKeyCore::Ext(DataKeyExt::SeasonArchive(_)) => ("SeasonArchive", false),
                DataKeyCore::Ext(DataKeyExt::ConstitutionMetadata) => {
                    ("ConstitutionMetadata", true)
                }
                DataKeyCore::Ext(DataKeyExt::Amendment(_)) => ("Amendment", false),
                DataKeyCore::Ext(DataKeyExt::NextAmendmentId) => ("NextAmendmentId", true),
            };
            assert_eq!(
                is_ttl_touch_allowed(key),
                expected,
                "TTL touch policy for {name}"
            );
        }
    }
}
