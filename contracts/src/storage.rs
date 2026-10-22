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

use crate::common::TTL_BUMP_AMOUNT;
use crate::types::{DataKeyCore, DataKeyExt, DataKeyScoped};
use soroban_sdk::{Address, Env, IntoVal, Val, Vec};

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

// ─── Pending outcome (dispute window) — Issue #685 ──────────────────────────

/// Key of the staged oracle result for `round_id` while its dispute window is
/// open (`dispute_ledgers > 0`). One entry per round, so reads and writes never
/// touch other rounds' staged results.
#[inline]
pub fn pending_outcome_key(round_id: u64) -> DataKeyCore {
    DataKeyCore::Ext(DataKeyExt::PendingOutcome(round_id))
}

/// Minimum remaining TTL (in ledgers) every entry needed to settle a staged
/// round must have: the whole dispute window plus the normal rent bump, so a
/// keeper can still finalize or void after the deadline.
#[inline]
pub fn dispute_ttl_target(dispute_ledgers: u32) -> u32 {
    dispute_ledgers.saturating_add(TTL_BUMP_AMOUNT)
}

#[inline]
fn extend_to_at_least<K: IntoVal<Env, Val>>(env: &Env, key: &K, target: u32) {
    if env.storage().persistent().has(key) {
        env.storage().persistent().extend_ttl(key, target, target);
    }
}

/// Extends the TTL of the staged outcome and of everything `finalize_round`
/// or `void_round` will read (participant list and every position) to at least
/// `target` ledgers. No-op for entries that already live longer.
pub fn extend_staged_round_ttl(env: &Env, round_id: u64, participants: &Vec<Address>, target: u32) {
    extend_to_at_least(env, &pending_outcome_key(round_id), target);
    extend_to_at_least(env, &DataKeyScoped::RoundParticipants(round_id), target);
    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            extend_to_at_least(
                env,
                &DataKeyScoped::Position(round_id, user.clone()),
                target,
            );
            extend_to_at_least(
                env,
                &DataKeyScoped::PrecisionPosition(round_id, user.clone()),
                target,
            );
            extend_to_at_least(
                env,
                &DataKeyScoped::PrecisionCommitment(round_id, user),
                target,
            );
        }
    }
}
