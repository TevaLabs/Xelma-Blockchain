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
/// along with the shared participant list and any legacy storage keys.
/// Deliberately leaves `ActiveRound` untouched.
///
/// This is the **canonical terminal cleanup** for round data. A round may be
/// terminalized (resolved/cancelled/voided) while a *newer* round is already
/// active — e.g. the staged oracle dispute-window flow lets a fresh round
/// start while an older result is still pending finalization/void — so
/// clearing `ActiveRound` unconditionally here would risk wiping out that
/// newer round's marker. Callers that know the round being cleaned up *is*
/// still the current active round should follow up with
/// [`clear_round_storage`], which also removes `ActiveRound`.
///
/// Keys removed (per participant):
/// - `Position`, `PrecisionPosition`, `PrecisionCommitment`
///
/// Shared keys removed:
/// - `RoundParticipants(round_id)`
/// - `Positions` (legacy)
/// - `UpDownPositions` (legacy)
/// - `PrecisionPositions` (legacy)
///
/// # TTL / archive interaction
///
/// This function only removes *live* position/participant keys — it never
/// touches `ArchivedRound(round_id)`, `UserRoundOutcome`, or
/// `UserArchivedRoundIds`, which are the durable historical record and are
/// expected to persist (and have their own TTL managed via
/// [`crate::common::_extend_persistent_ttl`] / archive-retention pruning)
/// independently of live-round cleanup. Call the archival helper
/// (`_archive_round`) *before* invoking either cleanup function in this
/// module, so the historical snapshot is captured while the live keys still
/// exist.
pub fn clear_round_storage_keep_active(env: &Env, round_id: u64, participants: &Vec<Address>) {
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

    // Legacy keys — safe no-op when absent
    env.storage().persistent().remove(&DataKeyCore::Positions);
    env.storage()
        .persistent()
        .remove(&DataKeyCore::UpDownPositions);
    env.storage()
        .persistent()
        .remove(&DataKeyCore::PrecisionPositions);
}

/// Removes all position storage keys for every participant in a round,
/// along with the shared participant list, the active round marker, and
/// any legacy storage keys.
///
/// This is the **canonical round cleanup** — call it once after
/// settlement, cancellation, or fallback refund, when the round being
/// cleaned up is still `ActiveRound` (i.e. no newer round has started).
/// If a dispute window may have let a newer round become active in the
/// meantime, use [`clear_round_storage_keep_active`] instead and remove
/// `ActiveRound` yourself only after confirming it still refers to this
/// `round_id`.
///
/// Keys removed: everything [`clear_round_storage_keep_active`] removes,
/// plus `ActiveRound`.
pub fn clear_round_storage(env: &Env, round_id: u64, participants: &Vec<Address>) {
    clear_round_storage_keep_active(env, round_id, participants);
    env.storage().persistent().remove(&DataKeyCore::ActiveRound);
}
