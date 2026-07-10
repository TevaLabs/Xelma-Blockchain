// SPDX-License-Identifier: MIT
use crate::common::{
    _derive_round_phase, _extend_persistent_ttl, sort_addresses, BPS_DENOMINATOR,
    DEFAULT_ARCHIVE_RETENTION, MAX_PAGE_SIZE,
};
use crate::errors::ContractError;
use crate::types::{
    ArchivedRoundSummary, BetSide, DataKey, PrecisionCommitment, PrecisionPrediction, Round,
    RoundMode, RoundPhase, RoundPoolStats, UserPosition, UserRoundOutcome, UserStats,
};
use soroban_sdk::{Address, Env, Map, Vec};

pub fn get_active_round(env: Env) -> Option<Round> {
    env.storage().persistent().get(&DataKey::ActiveRound)
}

/// Returns live pool-composition metrics for the currently active round.
pub fn get_round_pool_stats(env: Env) -> Option<RoundPoolStats> {
    let round: Round = env.storage().persistent().get(&DataKey::ActiveRound)?;
    let participants_key = DataKey::RoundParticipants(round.round_id);
    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&participants_key)
        .unwrap_or(Vec::new(&env));

    let mut stats = RoundPoolStats {
        round_id: round.round_id,
        mode: round.mode.clone(),
        total_up_stake: 0,
        total_down_stake: 0,
        up_participant_count: 0,
        down_participant_count: 0,
        up_stake_ratio_bps: 0,
        down_stake_ratio_bps: 0,
        precision_total_stake: 0,
        precision_participant_count: 0,
        precision_prediction_count: 0,
        precision_commitment_count: 0,
        precision_revealed_count: 0,
    };

    match round.mode {
        RoundMode::UpDown => {
            stats.total_up_stake = round.pool_up;
            stats.total_down_stake = round.pool_down;

            let mut idx = 0;
            while idx < participants.len() {
                if let Some(user) = participants.get(idx) {
                    if let Some(position) = env
                        .storage()
                        .persistent()
                        .get::<_, UserPosition>(&DataKey::Position(round.round_id, user))
                    {
                        match position.side {
                            BetSide::Up => stats.up_participant_count += 1,
                            BetSide::Down => stats.down_participant_count += 1,
                        }
                    }
                }
                idx += 1;
            }

            let total_stake = round.pool_up.checked_add(round.pool_down).unwrap_or(0);
            if total_stake > 0 {
                stats.up_stake_ratio_bps = ((round.pool_up as u128)
                    .saturating_mul(BPS_DENOMINATOR as u128)
                    / total_stake as u128) as u32;
                stats.down_stake_ratio_bps = ((round.pool_down as u128)
                    .saturating_mul(BPS_DENOMINATOR as u128)
                    / total_stake as u128) as u32;
            }
        }
        RoundMode::Precision => {
            stats.precision_participant_count = participants.len();

            let mut idx = 0;
            while idx < participants.len() {
                if let Some(user) = participants.get(idx) {
                    if let Some(prediction) =
                        env.storage().persistent().get::<_, PrecisionPrediction>(
                            &DataKey::PrecisionPosition(round.round_id, user.clone()),
                        )
                    {
                        stats.precision_prediction_count += 1;
                        stats.precision_total_stake += prediction.amount;
                    } else if let Some(commitment) =
                        env.storage().persistent().get::<_, PrecisionCommitment>(
                            &DataKey::PrecisionCommitment(round.round_id, user),
                        )
                    {
                        stats.precision_commitment_count += 1;
                        stats.precision_total_stake += commitment.amount;
                        if commitment.revealed {
                            stats.precision_revealed_count += 1;
                        }
                    }
                }
                idx += 1;
            }
        }
    }

    Some(stats)
}

/// Returns the current lifecycle phase of the active round.
pub fn get_round_phase(env: Env) -> Result<RoundPhase, ContractError> {
    let round = env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKey::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;
    Ok(_derive_round_phase(env.ledger().sequence(), &round))
}

/// Returns the ID of the last created round (0 if no rounds created yet)
pub fn get_last_round_id(env: Env) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::LastRoundId)
        .unwrap_or(0)
}

/// Returns a compact archived round summary by round id, if retained.
pub fn get_archived_round(env: Env, round_id: u64) -> Option<ArchivedRoundSummary> {
    env.storage()
        .persistent()
        .get(&DataKey::ArchivedRound(round_id))
}

/// Returns up to `limit` most recently archived rounds (newest first).
pub fn get_recent_archived_rounds(env: Env, limit: u32) -> Vec<ArchivedRoundSummary> {
    let env_ref = &env;
    let recent: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::RecentArchivedRoundIds)
        .unwrap_or(Vec::new(env_ref));

    let mut result = Vec::new(env_ref);
    if limit == 0 || recent.is_empty() {
        return result;
    }

    let retention_limit = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::ArchiveRetention)
        .unwrap_or(DEFAULT_ARCHIVE_RETENTION);

    let fetch_cap = if limit > retention_limit {
        retention_limit
    } else {
        limit
    };

    let mut fetched: u32 = 0;
    let mut idx = recent.len();
    while idx > 0 && fetched < fetch_cap {
        idx -= 1;
        if let Some(round_id) = recent.get(idx) {
            if let Some(summary) = env
                .storage()
                .persistent()
                .get(&DataKey::ArchivedRound(round_id))
            {
                result.push_back(summary);
                fetched += 1;
            }
        }
    }
    result
}

/// Returns a compact per-user outcome record for a specific archived round.
pub fn get_user_archived_participation(
    env: Env,
    user: Address,
    round_id: u64,
) -> Option<UserRoundOutcome> {
    let key = DataKey::UserRoundOutcome(round_id, user);
    env.storage().persistent().get(&key)
}

/// Returns user statistics (wins, losses, streaks)
pub fn get_user_stats(env: Env, user: Address) -> UserStats {
    let key = DataKey::UserStats(user);
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key).unwrap_or(UserStats {
        total_wins: 0,
        total_losses: 0,
        current_streak: 0,
        best_streak: 0,
    })
}

/// Returns user's unclaimed pending winnings balance
pub fn get_pending_winnings(env: Env, user: Address) -> i128 {
    let key = DataKey::PendingWinnings(user);
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key).unwrap_or(0)
}

/// Returns user's position in the current round (Up/Down mode).
pub fn get_user_position(env: Env, user: Address) -> Option<UserPosition> {
    if let Some(round) = env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKey::ActiveRound)
    {
        let pos_key = DataKey::Position(round.round_id, user.clone());
        if let Some(pos) = env.storage().persistent().get(&pos_key) {
            return Some(pos);
        }
    }

    let legacy_updown: Map<Address, UserPosition> = env
        .storage()
        .persistent()
        .get(&DataKey::UpDownPositions)
        .unwrap_or(Map::new(&env));
    if let Some(p) = legacy_updown.get(user.clone()) {
        return Some(p);
    }
    let legacy_positions: Map<Address, UserPosition> = env
        .storage()
        .persistent()
        .get(&DataKey::Positions)
        .unwrap_or(Map::new(&env));
    legacy_positions.get(user)
}

/// Returns user's precision prediction in the current round (Precision mode).
pub fn get_user_precision_prediction(env: Env, user: Address) -> Option<PrecisionPrediction> {
    if let Some(round) = env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKey::ActiveRound)
    {
        let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
        if let Some(p) = env
            .storage()
            .persistent()
            .get::<_, PrecisionPrediction>(&pred_key)
        {
            return Some(p);
        }
    }
    let legacy: Map<Address, PrecisionPrediction> = env
        .storage()
        .persistent()
        .get(&DataKey::PrecisionPositions)
        .unwrap_or(Map::new(&env));
    legacy.get(user)
}

/// Returns all precision predictions for the current round.
pub fn get_precision_predictions(env: Env) -> Vec<PrecisionPrediction> {
    let round = match env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKey::ActiveRound)
    {
        Some(r) => r,
        None => return Vec::new(&env),
    };

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::RoundParticipants(round.round_id))
        .unwrap_or(Vec::new(&env));

    let mut result: Vec<PrecisionPrediction> = Vec::new(&env);
    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
            if let Some(pred) = env.storage().persistent().get(&pred_key) {
                result.push_back(pred);
            }
        }
    }

    if result.is_empty() {
        let legacy: Map<Address, PrecisionPrediction> = env
            .storage()
            .persistent()
            .get(&DataKey::PrecisionPositions)
            .unwrap_or(Map::new(&env));
        return legacy.values();
    }
    result
}

/// Returns all Up/Down positions for the current round.
pub fn get_updown_positions(env: Env) -> Map<Address, UserPosition> {
    let round = match env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKey::ActiveRound)
    {
        Some(r) => r,
        None => return Map::new(&env),
    };

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::RoundParticipants(round.round_id))
        .unwrap_or(Vec::new(&env));

    let mut result: Map<Address, UserPosition> = Map::new(&env);
    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            let pos_key = DataKey::Position(round.round_id, user.clone());
            if let Some(pos) = env.storage().persistent().get(&pos_key) {
                result.set(user, pos);
            }
        }
    }

    if result.is_empty() {
        return env
            .storage()
            .persistent()
            .get(&DataKey::UpDownPositions)
            .unwrap_or(Map::new(&env));
    }
    result
}

/// Returns a deterministic slice of Precision-mode predictions for the active round.
pub fn get_precision_predictions_page(
    env: Env,
    offset: u32,
    limit: u32,
) -> Vec<PrecisionPrediction> {
    let limit = limit.min(MAX_PAGE_SIZE);
    if limit == 0 {
        return Vec::new(&env);
    }

    let round = match env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKey::ActiveRound)
    {
        Some(r) => r,
        None => return Vec::new(&env),
    };

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::RoundParticipants(round.round_id))
        .unwrap_or(Vec::new(&env));
    let participants = sort_addresses(participants);

    let total = participants.len();
    if offset >= total {
        return Vec::new(&env);
    }

    let end = offset.saturating_add(limit).min(total);

    let mut result: Vec<PrecisionPrediction> = Vec::new(&env);
    for i in offset..end {
        if let Some(user) = participants.get(i) {
            let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
            if let Some(pred) = env.storage().persistent().get(&pred_key) {
                result.push_back(pred);
            }
        }
    }

    result
}

/// Returns a slice of Up/Down positions for the active round as (Address, UserPosition) pairs.
pub fn get_updown_positions_page(
    env: Env,
    offset: u32,
    limit: u32,
) -> Vec<(Address, UserPosition)> {
    let limit = limit.min(MAX_PAGE_SIZE);
    if limit == 0 {
        return Vec::new(&env);
    }

    let round = match env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKey::ActiveRound)
    {
        Some(r) => r,
        None => return Vec::new(&env),
    };

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::RoundParticipants(round.round_id))
        .unwrap_or(Vec::new(&env));
    let participants = sort_addresses(participants);

    let total = participants.len();
    if offset >= total {
        return Vec::new(&env);
    }

    let end = offset.saturating_add(limit).min(total);

    let mut result: Vec<(Address, UserPosition)> = Vec::new(&env);
    for i in offset..end {
        if let Some(user) = participants.get(i) {
            let pos_key = DataKey::Position(round.round_id, user.clone());
            if let Some(pos) = env.storage().persistent().get(&pos_key) {
                result.push_back((user, pos));
            }
        }
    }

    result
}
