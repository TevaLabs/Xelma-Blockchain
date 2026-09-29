// SPDX-License-Identifier: MIT
//! Mode-agnostic position repository — unified storage helpers for UpDown and
//! Precision modes.
//!
//! The canonical cleanup functions in this module ensure that **all**
//! position storage keys are removed for every participant, regardless of
//! the active round mode.  This prevents stale data cross-contamination
//! when the protocol alternates between UpDown and Precision rounds.
//!
//! Every terminal round path (resolve, cancel, void, finalize, fallback
//! refund) MUST route through [`clear_round_storage`] or
//! [`clear_round_storage_keep_active`] instead of duplicating per-key
//! removal patterns inline (Issue #507):
//!
//! - [`clear_round_storage`] — the round being terminalized is (still) the
//!   active one: removes the `ActiveRound` marker as well. Used by
//!   `cancel_round` and immediate (dispute-window-less) resolution.
//! - [`clear_round_storage_keep_active`] — dispute-window staging paths
//!   (`void_round` / `finalize_round` on an older disputed round) where a
//!   **newer** round may already be active; removing `ActiveRound` there
//!   would kill the wrong round's marker. Also used inside
//!   `_complete_settlement`, which removes `ActiveRound` itself **only
//!   after verifying it still points at the round being settled**.

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
/// along with the shared participant list and any legacy storage keys,
/// while **leaving the `ActiveRound` marker untouched**.
///
/// This is the canonical round cleanup for **dispute-window staging
/// paths** (`void_round` / `finalize_round`): by the time an older round's
/// staged result is terminalized, a newer round may already be active, so
/// this helper deliberately does not remove `ActiveRound`. Callers that
/// own the active round (cancel, immediate resolution) must use
/// [`clear_round_storage`], which additionally removes the marker.
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
/// Shared keys preserved:
/// - `ActiveRound` — see above.
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
/// This is the **canonical round cleanup** for paths where the round being
/// terminalized is still the active one — call it once after settlement,
/// cancellation, or fallback refund. For dispute-window staging paths,
/// where a newer round may already be active, use
/// [`clear_round_storage_keep_active`] instead.
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
    clear_round_storage_keep_active(env, round_id, participants);

    // Only the path that owns the active round removes this marker.
    env.storage().persistent().remove(&DataKeyCore::ActiveRound);
}
