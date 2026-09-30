// SPDX-License-Identifier: MIT
extern crate alloc;
use crate::admin::{
    _ensure_not_paused, _load_attestation_config, _load_deviation_config, _load_hb_config,
    _require_supported_schema,
};
use crate::common::{
    _accumulate_pending, _emit_action_rejected, _extend_persistent_ttl, _extend_ttl_symbol,
    _set_balance, balance, payout_add, payout_mul, sort_addresses, DEFAULT_ORACLE_TIMESTAMP_SKEW,
    MAX_CLAIM_BATCH_SIZE, MAX_ORACLE_OBSERVATIONS, SECONDS_PER_LEDGER, TTL_BUMP_AMOUNT,
    TTL_BUMP_THRESHOLD,
};
use crate::config::{_apply_protocol_fee_precision, _apply_protocol_fee_updown, _read_fee_model};
use crate::errors::ContractError;
use crate::settlement_math::{
    classify_price_direction, compute_deviation_bps, compute_updown_winner_payout,
    is_one_sided_pool, total_pot_updown, PriceDirection,
};
use crate::storage::clear_round_storage;
use crate::types::{
    ArchivedRoundSummary, BetSide, DataKeyCore, DataKeyScoped, DeviationReferenceMode,
    HbGateConfig, LeaderboardEntry, MultiFeedPayload, OneSidedPolicy, OracleHeartbeatRecord,
    OraclePayload, OracleQuorumConfig, PendingWinningsUpdatedAtKey, PrecisionCommitment,
    PrecisionPayoutPolicy, PrecisionPrediction, PriceSample, Round, RoundArchiveStatus, RoundMode,
    TwapSamplesKey, UserOutcomeType, UserPosition, UserRoundOutcome, UserStats,
};
use alloc::vec::Vec as StdVec;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{contracttype, symbol_short, Address, Bytes, Env, Map, Symbol, Vec};

mod archive;
mod cancel;
mod multi_feed;
mod resolve;

pub use crate::common::DEFAULT_ARCHIVE_RETENTION;
pub use archive::{_archive_round, _persist_user_outcome};
pub use cancel::{cancel_round, finalize_round, is_round_cancelled, void_round};
pub use multi_feed::resolve_round_multi;
pub use resolve::{
    _apply_one_sided_policy, _build_attestation_message, _check_heartbeat_health_blocked,
    _load_twap_samples, _record_twap_sample, _refund_under_threshold, _resolve_precision_mode,
    _resolve_updown_mode, _select_one_sided_policy, _twap_reference_price, resolve_round,
};

#[contracttype]
#[derive(Clone)]
struct PendingDisputeSettlement {
    round: Round,
    final_price: u128,
    confidence: Option<u32>,
    resolved_at_ledger: u32,
    deadline_ledger: u32,
}

fn _pending_disputes_key(env: &Env) -> Symbol {
    Symbol::new(env, "PendingDisputes")
}

fn _read_pending_dispute(env: &Env, round_id: u64) -> Option<PendingDisputeSettlement> {
    let key = _pending_disputes_key(env);
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
    }
    let pending: Map<u64, PendingDisputeSettlement> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Map::new(env));
    pending.get(round_id)
}

fn _write_pending_dispute(env: &Env, settlement: &PendingDisputeSettlement) {
    let key = _pending_disputes_key(env);
    let mut pending: Map<u64, PendingDisputeSettlement> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Map::new(env));
    pending.set(settlement.round.round_id, settlement.clone());
    env.storage().persistent().set(&key, &pending);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
}

fn _remove_pending_dispute(env: &Env, round_id: u64) {
    let key = _pending_disputes_key(env);
    let mut pending: Map<u64, PendingDisputeSettlement> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Map::new(env));
    pending.remove(round_id);
    if pending.is_empty() {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &pending);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
    }
}

/// Clears only storage owned by the terminal round. A newer active round may
/// already exist while an older result is inside its dispute window, so this
/// helper deliberately does not remove `ActiveRound`.
fn _clear_dispute_round_storage(env: &Env, round_id: u64, participants: &Vec<Address>) {
    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            env.storage()
                .persistent()
                .remove(&DataKeyScoped::Position(round_id, user.clone()));
            env.storage()
                .persistent()
                .remove(&DataKeyScoped::PrecisionPosition(round_id, user.clone()));
            env.storage()
                .persistent()
                .remove(&DataKeyScoped::PrecisionCommitment(round_id, user));
        }
    }
    env.storage()
        .persistent()
        .remove(&DataKeyScoped::RoundParticipants(round_id));
    env.storage().persistent().remove(&DataKeyCore::Positions);
    env.storage()
        .persistent()
        .remove(&DataKeyCore::UpDownPositions);
    env.storage()
        .persistent()
        .remove(&DataKeyCore::PrecisionPositions);
}

/// Claims pending winnings and adds to user balance.
///
/// # CEI Ordering (Checks-Effects-Interactions)
///
/// **Checks**:
/// 1. Schema version is supported.
/// 2. Caller (`user`) authenticates.
/// 3. Contract is not in `FullyPaused` mode (Normal & ClaimsOnly are permitted).
/// 4. Pending winnings must be non-zero — early return for zero-pending (idempotent).
/// 5. `balance + pending` must not overflow i128 (guarded by `payout_add`).
///
/// Note: `balance()` internally extends the TTL of the Balance storage key
/// (a read-side persistence operation). This is benign — TTL bumping does not
/// affect state semantics and is safe to perform before the Effects phase.
///
/// **Effects** (applied in strict order):
/// 1. Remove the `PendingWinnings` slot FIRST — prevents double-claim races.
/// 2. Write the new balance to the user's `Balance` slot — committed only after
///    the pending slot is cleared.
///
/// **Interactions**:
/// 1. Emit `(claim, winnings)` event with the full claim context *after* all
///    state is finalised, so observers always see a consistent ledger state.
///
/// # Overflow Safety
///
/// Safe `i128` arithmetic via `payout_add` for the `pending → balance` transfer.
/// If `current_balance + pending` overflows i128, the function returns
/// `PayoutOverflow` and NO storage writes occur (all-or-nothing guarantee).
///
/// # Mode Compatibility
///
/// | `RuntimeMode`    | Behaviour                                                   |
/// |------------------|-------------------------------------------------------------|
/// | `Normal`    (0)  | Claim allowed (standard flow).                              |
/// | `ClaimsOnly` (1) | Claim allowed (round settled/cancelled, only claims useful).|
/// | `FullyPaused`(2) | Claim rejected with `ContractPaused`.                       |
pub fn claim_winnings(env: Env, user: Address) -> Result<i128, ContractError> {
    // ── Checks ────────────────────────────────────────────────────────────
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_not_paused(&env)?; // rejects FullyPaused; allows Normal & ClaimsOnly

    let key = DataKeyScoped::PendingWinnings(user.clone());
    let pending: i128 = env.storage().persistent().get(&key).unwrap_or(0);

    if pending == 0 {
        return Ok(0);
    }

    let current_balance = balance(env.clone(), user.clone());
    let new_balance = payout_add(current_balance, pending)?;

    // ── Effects ───────────────────────────────────────────────────────────
    // 1. Remove the pending-winnings claim slot first (prevent double-claim).
    env.storage().persistent().remove(&key);
    env.storage()
        .persistent()
        .remove(&PendingWinningsUpdatedAtKey(user.clone()));
    _set_balance(&env, user.clone(), new_balance);

    // ── Interactions ──────────────────────────────────────────────────────
    // Emit a structured event reflecting the *committed* state so indexers
    // always observe a consistent view (old balance, claimed amount, new balance).
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("claim"), symbol_short!("winnings")),
        (user.clone(), pending, current_balance, new_balance),
    );

    Ok(pending)
}

/// Claims pending winnings for a bounded batch of users in a single call
/// (Issue #277).
///
/// # CEI Ordering (Checks-Effects-Interactions)
///
/// **Checks** (before any storage mutation):
/// 1. Schema version is supported.
/// 2. Contract is not in `FullyPaused` mode (Normal & ClaimsOnly are permitted).
/// 3. `users.len()` does not exceed [`MAX_CLAIM_BATCH_SIZE`] — bounds
///    per-invocation compute/storage-op cost for the whole batch.
/// 4. `users` contains no duplicate address — a duplicate would otherwise
///    claim once and silently no-op the second time, which is surprising
///    for an operator batch API, so it is rejected outright instead.
///
/// Per user, inside the loop:
/// 5. The user authenticates (`user.require_auth()`), exactly as
///    [`claim_winnings`] requires — an operator submitting this batch must
///    bundle each user's own pre-authorized signature; this function does
///    not grant itself any elevated claim authority over admin/operator auth.
/// 6. Pending winnings must be non-zero, or the user is skipped as a no-op
///    (idempotent, matching [`claim_winnings`]'s single-claim behaviour).
/// 7. `balance + pending` must not overflow i128 (guarded by `payout_add`).
///
/// **Effects** and **Interactions** per user mirror [`claim_winnings`]
/// exactly (remove `PendingWinnings` first, then write the new balance, then
/// emit the same `(claim, winnings)` event) so downstream indexers observe
/// identical per-user events whether a claim happened individually or as
/// part of a batch.
///
/// # All-or-nothing
///
/// This function performs no manual two-phase validate/commit: Soroban
/// discards every storage write made during a host function invocation that
/// returns `Err`, so returning `Err` at any point — cap check, duplicate
/// check, a missing per-user auth, or a `payout_add` overflow — atomically
/// reverts every effect already applied earlier in the same call, including
/// balance/pending-winnings updates for users processed before the failure.
///
/// # Returns
///
/// A `Vec<i128>` of claimed amounts, one per entry in `users`, in the same
/// order (0 for a user with no pending winnings at call time).
pub fn claim_many(env: Env, users: Vec<Address>) -> Result<Vec<i128>, ContractError> {
    // ── Checks ────────────────────────────────────────────────────────────
    _require_supported_schema(&env)?;
    _ensure_not_paused(&env)?; // rejects FullyPaused; allows Normal & ClaimsOnly

    if users.len() > MAX_CLAIM_BATCH_SIZE {
        return Err(ContractError::ClaimBatchTooLarge);
    }

    let sorted = sort_addresses(users.clone());
    for i in 1..sorted.len() {
        if sorted.get(i) == sorted.get(i - 1) {
            return Err(ContractError::DuplicateClaimAddress);
        }
    }

    let mut amounts: Vec<i128> = Vec::new(&env);

    for user in users.iter() {
        // ── Checks (per user) ────────────────────────────────────────────
        user.require_auth();

        let key = DataKeyScoped::PendingWinnings(user.clone());
        let pending: i128 = env.storage().persistent().get(&key).unwrap_or(0);

        if pending == 0 {
            amounts.push_back(0);
            continue;
        }

        let current_balance = balance(env.clone(), user.clone());
        let new_balance = payout_add(current_balance, pending)?;

        // ── Effects ───────────────────────────────────────────────────────
        // 1. Remove the pending-winnings claim slot first (prevent double-claim).
        env.storage().persistent().remove(&key);
        env.storage()
            .persistent()
            .remove(&PendingWinningsUpdatedAtKey(user.clone()));
        _set_balance(&env, user.clone(), new_balance);

        // ── Interactions ──────────────────────────────────────────────────
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("claim"), symbol_short!("winnings")),
            (user.clone(), pending, current_balance, new_balance),
        );

        amounts.push_back(pending);
    }

    Ok(amounts)
}

pub fn _update_stats_win(env: &Env, user: Address) -> Result<(), ContractError> {
    let key = DataKeyScoped::UserStats(user.clone());
    let mut stats: UserStats = env.storage().persistent().get(&key).unwrap_or(UserStats {
        total_wins: 0,
        total_losses: 0,
        current_streak: 0,
        best_streak: 0,
    });

    stats.total_wins = stats
        .total_wins
        .checked_add(1)
        .ok_or(ContractError::Overflow)?;
    stats.current_streak = stats
        .current_streak
        .checked_add(1)
        .ok_or(ContractError::Overflow)?;

    if stats.current_streak > stats.best_streak {
        stats.best_streak = stats.current_streak;
    }

    env.storage().persistent().set(&key, &stats);
    _extend_persistent_ttl(env, &key);
    crate::leaderboard::_update_leaderboards(env, user.clone());
    crate::leaderboard::_update_season_stats_win(env, user)?;
    Ok(())
}

pub fn _update_stats_loss(env: &Env, user: Address) -> Result<(), ContractError> {
    let key = DataKeyScoped::UserStats(user.clone());
    let mut stats: UserStats = env.storage().persistent().get(&key).unwrap_or(UserStats {
        total_wins: 0,
        total_losses: 0,
        current_streak: 0,
        best_streak: 0,
    });

    stats.total_losses = stats
        .total_losses
        .checked_add(1)
        .ok_or(ContractError::Overflow)?;
    stats.current_streak = 0;

    env.storage().persistent().set(&key, &stats);
    _extend_persistent_ttl(env, &key);
    crate::leaderboard::_update_leaderboards(env, user.clone());
    crate::leaderboard::_update_season_stats_loss(env, user)?;
    Ok(())
}
