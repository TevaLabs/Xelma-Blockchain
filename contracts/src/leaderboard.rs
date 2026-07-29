// SPDX-License-Identifier: MIT
//! Leaderboard storage and queries: an all-time (lifetime) leaderboard plus
//! seasonal leaderboards that can be reset without losing history.
//!
//! ## Lifetime leaderboard
//! Two bounded (top [`LEADERBOARD_LIMIT`]) indexes — `LeaderboardWins` and
//! `LeaderboardStreak` — are maintained incrementally on every win/loss via
//! [`_update_leaderboards`], ranked by the same lifetime `UserStats` totals
//! `get_user_stats` already exposes. This scope is untouched by seasons.
//!
//! ## Seasons
//! A season is a scope-limited leaderboard layered on top of the lifetime
//! one: `SeasonId` names the *active* season (default 1), and
//! `SeasonUserStats(season_id, user)` tracks win/loss/streak counters scoped
//! to that season only — independent of, and never mutating, the lifetime
//! totals above ("hack campaigns need seasons without wiping history").
//!
//! [`reset_leaderboard_season`] (admin only) freezes the active season's
//! bounded rankings into a permanent `SeasonArchive(season_id)` snapshot,
//! then advances `SeasonId`. Nothing is deleted: `SeasonUserStats` entries
//! for old seasons stay addressable forever via [`get_season_user_stats`],
//! and [`get_season_leaderboard_by_wins`] / [`get_season_leaderboard_by_streak`]
//! transparently serve the live index for the current season or the frozen
//! archive for any past one — callers never need to know which.

use crate::admin::{_ensure_not_paused, _require_supported_schema};
use crate::common::{
    _emit_action_rejected, _extend_persistent_ttl, LEADERBOARD_LIMIT, MAX_PAGE_SIZE,
};
use crate::errors::ContractError;
use crate::types::{DataKeyCore, DataKeyScoped, LeaderboardEntry, SeasonArchive, SeasonLeaderboardEntry, UserStats};
use crate::types::{
    DataKey, DataKeyExt, LeaderboardEntry, SeasonArchive, SeasonLeaderboardEntry, UserStats,
};
use soroban_sdk::{symbol_short, Address, Env, Vec};

fn lifetime_user_stats(env: &Env, user: &Address) -> UserStats {
    env.storage()
        .persistent()
        .get(&DataKeyScoped::UserStats(user.clone()))
        .unwrap_or(UserStats {
            total_wins: 0,
            total_losses: 0,
            current_streak: 0,
            best_streak: 0,
        })
}

fn season_user_stats_raw(env: &Env, season_id: u32, user: &Address) -> UserStats {
    env.storage()
        .persistent()
        .get(&DataKeyScoped::SeasonUserStats(season_id, user.clone()))
        .get(&DataKey::Ext(DataKeyExt::SeasonUserStats(
            season_id,
            user.clone(),
        )))
        .unwrap_or(UserStats {
            total_wins: 0,
            total_losses: 0,
            current_streak: 0,
            best_streak: 0,
        })
}

// ─── Bounded-index maintenance ────────────────────────────────────────────────
//
// Every bounded index (lifetime wins/streak, active-season wins/streak) is
// maintained the same way: drop the user if already present, re-insert in
// sorted order (metric descending, address ascending as a deterministic
// tie-breaker), then truncate to LEADERBOARD_LIMIT. Cost is O(n) per update,
// bounded by LEADERBOARD_LIMIT — acceptable for a top-N ranking structure.

fn reinsert_sorted_by_wins(
    env: &Env,
    addresses: Vec<Address>,
    stats_of: impl Fn(&Address) -> UserStats,
) -> Vec<Address> {
    let mut sorted: Vec<Address> = Vec::new(env);
    for addr in addresses.iter() {
        let addr_stats = stats_of(&addr);
        let mut inserted = false;
        for i in 0..sorted.len() {
            let other = sorted.get_unchecked(i);
            let other_stats = stats_of(&other);
            if addr_stats.total_wins > other_stats.total_wins
                || (addr_stats.total_wins == other_stats.total_wins && addr < other)
            {
                sorted.insert(i, addr.clone());
                inserted = true;
                break;
            }
        }
        if !inserted {
            sorted.push_back(addr);
        }
    }
    sorted
}

fn reinsert_sorted_by_streak(
    env: &Env,
    addresses: Vec<Address>,
    stats_of: impl Fn(&Address) -> UserStats,
) -> Vec<Address> {
    let mut sorted: Vec<Address> = Vec::new(env);
    for addr in addresses.iter() {
        let addr_stats = stats_of(&addr);
        let mut inserted = false;
        for i in 0..sorted.len() {
            let other = sorted.get_unchecked(i);
            let other_stats = stats_of(&other);
            if addr_stats.best_streak > other_stats.best_streak
                || (addr_stats.best_streak == other_stats.best_streak && addr < other)
            {
                sorted.insert(i, addr.clone());
                inserted = true;
                break;
            }
        }
        if !inserted {
            sorted.push_back(addr);
        }
    }
    sorted
}

fn upsert_bounded_index(env: &Env, key: &DataKeyCore, sorted: Vec<Address>) {
    let limit = LEADERBOARD_LIMIT.min(sorted.len());
    let mut bounded = Vec::new(env);
    for i in 0..limit {
        bounded.push_back(sorted.get_unchecked(i));
    }
    env.storage().persistent().set(key, &bounded);
    _extend_persistent_ttl(env, key);
}

fn without_user(env: &Env, list: &Vec<Address>, user: &Address) -> Vec<Address> {
    let mut filtered = Vec::new(env);
    for addr in list.iter() {
        if addr != *user {
            filtered.push_back(addr);
        }
    }
    filtered
}

// ─── Lifetime leaderboard ─────────────────────────────────────────────────────

/// Re-ranks `user` into both lifetime bounded indexes after a win/loss.
/// Called from `settlement::_update_stats_win` / `_update_stats_loss`,
/// **after** the lifetime `UserStats` write, so the freshly-updated totals
/// are what gets ranked.
pub fn _update_leaderboards(env: &Env, user: Address) {
    let wins_key = DataKeyCore::LeaderboardWins;
    let wins_list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&wins_key)
        .unwrap_or(Vec::new(env));
    let mut candidates = without_user(env, &wins_list, &user);
    candidates.push_back(user.clone());
    let sorted = reinsert_sorted_by_wins(env, candidates, |addr| lifetime_user_stats(env, addr));
    upsert_bounded_index(env, &wins_key, sorted);

    let streak_key = DataKeyCore::LeaderboardStreak;
    let streak_list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&streak_key)
        .unwrap_or(Vec::new(env));
    let mut candidates = without_user(env, &streak_list, &user);
    candidates.push_back(user);
    let sorted = reinsert_sorted_by_streak(env, candidates, |addr| lifetime_user_stats(env, addr));
    upsert_bounded_index(env, &streak_key, sorted);
}

/// Returns a paginated slice of the lifetime wins leaderboard, ordered by
/// total wins descending (address ascending as a tie-breaker).
pub fn get_leaderboard_by_wins(env: Env, offset: u32, limit: u32) -> Vec<LeaderboardEntry> {
    let limit = limit.min(MAX_PAGE_SIZE);
    if limit == 0 {
        return Vec::new(&env);
    }
    let key = DataKeyCore::LeaderboardWins;
    _extend_persistent_ttl(&env, &key);
    let list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(&env));

    let total = list.len();
    if offset >= total {
        return Vec::new(&env);
    }
    let end = offset.saturating_add(limit).min(total);

    let mut result = Vec::new(&env);
    for i in offset..end {
        if let Some(user) = list.get(i) {
            let stats = lifetime_user_stats(&env, &user);
            result.push_back(LeaderboardEntry { user, stats });
        }
    }
    result
}

/// Returns a paginated slice of the lifetime best-streak leaderboard,
/// ordered by best streak descending (address ascending as a tie-breaker).
pub fn get_leaderboard_by_streak(env: Env, offset: u32, limit: u32) -> Vec<LeaderboardEntry> {
    let limit = limit.min(MAX_PAGE_SIZE);
    if limit == 0 {
        return Vec::new(&env);
    }
    let key = DataKeyCore::LeaderboardStreak;
    _extend_persistent_ttl(&env, &key);
    let list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(&env));

    let total = list.len();
    if offset >= total {
        return Vec::new(&env);
    }
    let end = offset.saturating_add(limit).min(total);

    let mut result = Vec::new(&env);
    for i in offset..end {
        if let Some(user) = list.get(i) {
            let stats = lifetime_user_stats(&env, &user);
            result.push_back(LeaderboardEntry { user, stats });
        }
    }
    result
}

// ─── Seasons ───────────────────────────────────────────────────────────────

pub fn _current_season_id(env: &Env) -> u32 {
    env.storage().persistent().get(&DataKeyCore::SeasonId).unwrap_or(1)
    env.storage()
        .persistent()
        .get(&DataKey::Ext(DataKeyExt::SeasonId))
        .unwrap_or(1)
}

/// Returns the id of the currently-active leaderboard season (default 1).
pub fn get_current_season_id(env: Env) -> u32 {
    let key = DataKeyCore::SeasonId;
    _extend_persistent_ttl(&env, &key);
    _current_season_id(&env)
}

/// Returns `user`'s season-scoped stats for `season_id` — works uniformly
/// for the active season or any past (archived) one, since per-season stats
/// are never deleted.
pub fn get_season_user_stats(env: Env, season_id: u32, user: Address) -> UserStats {
    let key = DataKeyScoped::SeasonUserStats(season_id, user);
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key).unwrap_or(UserStats {
        total_wins: 0,
        total_losses: 0,
        current_streak: 0,
        best_streak: 0,
    })
}

fn _update_season_leaderboards(env: &Env, season_id: u32, user: Address) {
    let wins_key = DataKeyCore::SeasonLeaderboardWins;
    let wins_list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&wins_key)
        .unwrap_or(Vec::new(env));
    let mut candidates = without_user(env, &wins_list, &user);
    candidates.push_back(user.clone());
    let sorted = reinsert_sorted_by_wins(env, candidates, |addr| {
        season_user_stats_raw(env, season_id, addr)
    });
    upsert_bounded_index(env, &wins_key, sorted);

    let streak_key = DataKeyCore::SeasonLeaderboardStreak;
    let streak_list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&streak_key)
        .unwrap_or(Vec::new(env));
    let mut candidates = without_user(env, &streak_list, &user);
    candidates.push_back(user);
    let sorted = reinsert_sorted_by_streak(env, candidates, |addr| {
        season_user_stats_raw(env, season_id, addr)
    });
    upsert_bounded_index(env, &streak_key, sorted);
}

/// Records a season-scoped win for `user` in the active season. Mirrors
/// `settlement::_update_stats_win` but scoped to `SeasonUserStats`, entirely
/// independent of the lifetime totals.
pub fn _update_season_stats_win(env: &Env, user: Address) -> Result<(), ContractError> {
    let season_id = _current_season_id(env);
    let key = DataKeyScoped::SeasonUserStats(season_id, user.clone());
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
    _update_season_leaderboards(env, season_id, user);
    Ok(())
}

/// Records a season-scoped loss for `user` in the active season.
pub fn _update_season_stats_loss(env: &Env, user: Address) -> Result<(), ContractError> {
    let season_id = _current_season_id(env);
    let key = DataKeyScoped::SeasonUserStats(season_id, user.clone());
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
    _update_season_leaderboards(env, season_id, user);
    Ok(())
}

/// Freezes the active season's bounded rankings into a permanent archive
/// and advances to the next season (admin only).
///
/// This is the "archive on reset": nothing is deleted. `SeasonUserStats`
/// entries for the ending season remain queryable forever via
/// [`get_season_user_stats`], and the frozen top-N snapshot written here is
/// what [`get_season_leaderboard_by_wins`] / [`get_season_leaderboard_by_streak`]
/// serve once the season is no longer active. Only the *active*-season
/// bounded indexes are cleared, so the new season starts with an empty
/// ranking rather than inheriting stale entries.
pub fn reset_leaderboard_season(env: Env) -> Result<u32, ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("rst_seas"), e);
    })?;

    let season_id = _current_season_id(&env);

    let wins_list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyCore::SeasonLeaderboardWins)
        .unwrap_or(Vec::new(&env));
    let streak_list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyCore::SeasonLeaderboardStreak)
        .unwrap_or(Vec::new(&env));

    let mut wins_entries: Vec<SeasonLeaderboardEntry> = Vec::new(&env);
    for addr in wins_list.iter() {
        let stats = season_user_stats_raw(&env, season_id, &addr);
        wins_entries.push_back(SeasonLeaderboardEntry {
            user: addr,
            wins: stats.total_wins,
            best_streak: stats.best_streak,
        });
    }

    let mut streak_entries: Vec<SeasonLeaderboardEntry> = Vec::new(&env);
    for addr in streak_list.iter() {
        let stats = season_user_stats_raw(&env, season_id, &addr);
        streak_entries.push_back(SeasonLeaderboardEntry {
            user: addr,
            wins: stats.total_wins,
            best_streak: stats.best_streak,
        });
    }

    // Distinct participants across both bounded indexes (a user may appear
    // in both). Bounded by 2*LEADERBOARD_LIMIT, so the O(n^2) dedupe is cheap.
    let mut distinct_participants: Vec<Address> = Vec::new(&env);
    for addr in wins_list.iter() {
        distinct_participants.push_back(addr);
    }
    for addr in streak_list.iter() {
        let mut seen = false;
        for existing in distinct_participants.iter() {
            if existing == addr {
                seen = true;
                break;
            }
        }
        if !seen {
            distinct_participants.push_back(addr);
        }
    }
    let participant_count = distinct_participants.len();
    let ended_at_ledger = env.ledger().sequence();

    let archive = SeasonArchive {
        season_id,
        ended_at_ledger,
        wins: wins_entries,
        streak: streak_entries,
        participant_count,
    };
    let archive_key = DataKeyScoped::SeasonArchive(season_id);
    env.storage().persistent().set(&archive_key, &archive);
    _extend_persistent_ttl(&env, &archive_key);

    let new_season_id = season_id.checked_add(1).ok_or(ContractError::Overflow)?;
    let season_key = DataKeyCore::SeasonId;
    env.storage().persistent().set(&season_key, &new_season_id);
    _extend_persistent_ttl(&env, &season_key);

    env.storage()
        .persistent()
        .remove(&DataKeyCore::SeasonLeaderboardWins);
    env.storage()
        .persistent()
        .remove(&DataKeyCore::SeasonLeaderboardStreak);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("season"), symbol_short!("reset")),
        (season_id, new_season_id, ended_at_ledger, participant_count),
    );

    Ok(new_season_id)
}

/// Returns the frozen archive for a past season, if one exists (i.e. the
/// season has been reset at least once since).
pub fn get_season_archive(env: Env, season_id: u32) -> Option<SeasonArchive> {
    let key = DataKeyScoped::SeasonArchive(season_id);
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Returns a paginated slice of `season_id`'s wins leaderboard. Transparently
/// serves the live bounded index if `season_id` is the active season, or the
/// frozen archive snapshot otherwise. Unknown/future season ids return an
/// empty page rather than erroring, matching the other paginated queries.
pub fn get_season_leaderboard_by_wins(
    env: Env,
    season_id: u32,
    offset: u32,
    limit: u32,
) -> Vec<SeasonLeaderboardEntry> {
    let limit = limit.min(MAX_PAGE_SIZE);
    if limit == 0 {
        return Vec::new(&env);
    }

    if season_id == _current_season_id(&env) {
        let key = DataKeyCore::SeasonLeaderboardWins;
        _extend_persistent_ttl(&env, &key);
        let list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        let total = list.len();
        if offset >= total {
            return Vec::new(&env);
        }
        let end = offset.saturating_add(limit).min(total);
        let mut result = Vec::new(&env);
        for i in offset..end {
            if let Some(user) = list.get(i) {
                let stats = season_user_stats_raw(&env, season_id, &user);
                result.push_back(SeasonLeaderboardEntry {
                    user,
                    wins: stats.total_wins,
                    best_streak: stats.best_streak,
                });
            }
        }
        return result;
    }

    match get_season_archive(env.clone(), season_id) {
        None => Vec::new(&env),
        Some(archive) => page_entries(&env, &archive.wins, offset, limit),
    }
}

/// Returns a paginated slice of `season_id`'s best-streak leaderboard. Same
/// live-vs-archive semantics as [`get_season_leaderboard_by_wins`].
pub fn get_season_leaderboard_by_streak(
    env: Env,
    season_id: u32,
    offset: u32,
    limit: u32,
) -> Vec<SeasonLeaderboardEntry> {
    let limit = limit.min(MAX_PAGE_SIZE);
    if limit == 0 {
        return Vec::new(&env);
    }

    if season_id == _current_season_id(&env) {
        let key = DataKeyCore::SeasonLeaderboardStreak;
        _extend_persistent_ttl(&env, &key);
        let list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        let total = list.len();
        if offset >= total {
            return Vec::new(&env);
        }
        let end = offset.saturating_add(limit).min(total);
        let mut result = Vec::new(&env);
        for i in offset..end {
            if let Some(user) = list.get(i) {
                let stats = season_user_stats_raw(&env, season_id, &user);
                result.push_back(SeasonLeaderboardEntry {
                    user,
                    wins: stats.total_wins,
                    best_streak: stats.best_streak,
                });
            }
        }
        return result;
    }

    match get_season_archive(env.clone(), season_id) {
        None => Vec::new(&env),
        Some(archive) => page_entries(&env, &archive.streak, offset, limit),
    }
}

fn page_entries(
    env: &Env,
    entries: &Vec<SeasonLeaderboardEntry>,
    offset: u32,
    limit: u32,
) -> Vec<SeasonLeaderboardEntry> {
    let total = entries.len();
    if offset >= total {
        return Vec::new(env);
    }
    let end = offset.saturating_add(limit).min(total);
    let mut result = Vec::new(env);
    for i in offset..end {
        if let Some(entry) = entries.get(i) {
            result.push_back(entry);
        }
    }
    result
}
