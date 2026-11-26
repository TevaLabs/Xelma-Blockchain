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

use crate::common::{TTL_BUMP_AMOUNT, TTL_BUMP_THRESHOLD};
use crate::types::{DataKeyCore, DataKeyScoped, PendingOutcome};
use soroban_sdk::{Address, Env, Vec};

/// Loads a deferred outcome and refreshes its persistent TTL when present.
pub fn read_pending_outcome(env: &Env, round_id: u64) -> Option<PendingOutcome> {
    let key = DataKeyScoped::PendingOutcome(round_id);
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
    }
    env.storage().persistent().get(&key)
}

/// Stores a deferred outcome under its round-scoped key and applies the
/// standard persistent-storage TTL policy.
pub fn write_pending_outcome(env: &Env, outcome: &PendingOutcome) {
    let key = DataKeyScoped::PendingOutcome(outcome.round.round_id);
    env.storage().persistent().set(&key, outcome);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
}

/// Removes a deferred outcome after finalization or an accepted dispute.
pub fn remove_pending_outcome(env: &Env, round_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKeyScoped::PendingOutcome(round_id));
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
