// SPDX-License-Identifier: MIT
use crate::common::{
    _derive_round_phase, _extend_persistent_ttl, _legacy_positions_key, payout_add, payout_mul,
    sort_addresses, BPS_DENOMINATOR, DEFAULT_ARCHIVE_RETENTION, MAX_PAGE_SIZE,
};
use crate::config::{
    _read_fee_model, _read_precision_payout_policy, _read_protocol_fee_bps,
    calculate_protocol_fee_precision, calculate_protocol_fee_updown, get_bet_window_ledgers,
    get_close_buffer_ledgers, get_run_window_ledgers,
};
use crate::errors::ContractError;
use crate::types::{
    ArchivedRoundSummary, BetSide, DataKey, DataKeyCore, DataKeyScoped, LeaderboardEntry,
    MarketSnapshot, PendingWinningsUpdatedAtKey, PrecisionCommitment, PrecisionPayoutPolicy,
    PrecisionPrediction, Round, RoundMode, RoundPhase, RoundPoolStats, RoundTemplate,
    SeasonArchive, SimulationResult, UserOutcomeType, UserPosition, UserRoundOutcome, UserStats,
};
use soroban_sdk::{Address, Env, Map, Vec};

pub fn get_active_round(env: Env) -> Option<Round> {
    env.storage().persistent().get(&DataKeyCore::ActiveRound)
}

/// Returns live pool-composition metrics for the currently active round.
pub fn get_round_pool_stats(env: Env) -> Option<RoundPoolStats> {
    let round: Round = env.storage().persistent().get(&DataKeyCore::ActiveRound)?;
    let participants_key = DataKeyScoped::RoundParticipants(round.round_id);
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
                        .get::<_, UserPosition>(&DataKeyScoped::Position(round.round_id, user))
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
                            &DataKeyScoped::PrecisionPosition(round.round_id, user.clone()),
                        )
                    {
                        stats.precision_prediction_count += 1;
                        stats.precision_total_stake += prediction.amount;
                    } else if let Some(commitment) =
                        env.storage().persistent().get::<_, PrecisionCommitment>(
                            &DataKeyScoped::PrecisionCommitment(round.round_id, user),
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
        .get::<_, Round>(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;
    Ok(_derive_round_phase(env.ledger().sequence(), &round))
}

/// Returns a single-read composite snapshot of current market state: round
/// phase, pool composition, ledger timing buffers, and fee configuration
/// (Issue #280). See [`MarketSnapshot`] for empty-round semantics and the
/// exact getters each field is sourced from.
pub fn get_market_snapshot(env: Env) -> MarketSnapshot {
    let mut phase: Vec<RoundPhase> = Vec::new(&env);
    if let Ok(p) = get_round_phase(env.clone()) {
        phase.push_back(p);
    }
    let mut pool_stats: Vec<RoundPoolStats> = Vec::new(&env);
    if let Some(s) = get_round_pool_stats(env.clone()) {
        pool_stats.push_back(s);
    }

    let bet_window_ledgers = get_bet_window_ledgers(env.clone());
    let run_window_ledgers = get_run_window_ledgers(env.clone());
    let close_buffer_ledgers = get_close_buffer_ledgers(env.clone());

    let protocol_fee_bps = _read_protocol_fee_bps(&env);
    let fee_model = _read_fee_model(&env);

    MarketSnapshot {
        phase,
        pool_stats,
        bet_window_ledgers,
        run_window_ledgers,
        close_buffer_ledgers,
        protocol_fee_bps,
        fee_model,
    }
}

/// Returns the ID of the last created round (0 if no rounds created yet)
pub fn get_last_round_id(env: Env) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKeyCore::LastRoundId)
        .unwrap_or(0)
}

/// Returns a compact archived round summary by round id, if retained.
pub fn get_archived_round(env: Env, round_id: u64) -> Option<ArchivedRoundSummary> {
    env.storage()
        .persistent()
        .get(&DataKeyScoped::ArchivedRound(round_id))
}

/// Returns up to `limit` most recently archived rounds (newest first).
pub fn get_recent_archived_rounds(env: Env, limit: u32) -> Vec<ArchivedRoundSummary> {
    let env_ref = &env;
    let recent: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKeyCore::RecentArchivedRoundIds)
        .unwrap_or(Vec::new(env_ref));

    let mut result = Vec::new(env_ref);
    if limit == 0 || recent.is_empty() {
        return result;
    }

    let retention_limit = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKeyCore::ArchiveRetention)
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
                .get(&DataKeyScoped::ArchivedRound(round_id))
            {
                result.push_back(summary);
                fetched += 1;
            }
        }
    }
    result
}

/// Returns a compact per-user outcome record for a specific archived round.
///
/// Returns `None` if the round does not exist or has been pruned from the
/// on-chain archive, even if a `UserRoundOutcome` record still exists in
/// storage. This ensures consistent missing-id semantics: any query for a
/// pruned round_id returns `None` regardless of which query is used.
pub fn get_user_archived_participation(
    env: Env,
    user: Address,
    round_id: u64,
) -> Option<UserRoundOutcome> {
    if !env
        .storage()
        .persistent()
        .has(&DataKeyScoped::ArchivedRound(round_id))
    {
        return None;
    }
    let key = DataKeyScoped::UserRoundOutcome(round_id, user);
    env.storage().persistent().get(&key)
}

/// Returns paginated archived participation history for a user (newest first).
/// Rejects if `limit` exceeds `MAX_PAGE_SIZE` (100).
pub fn get_user_archive_history(
    env: Env,
    user: Address,
    offset: u32,
    limit: u32,
) -> Result<Vec<ArchivedRoundSummary>, ContractError> {
    let env_ref = &env;
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(ContractError::PageSizeExceeded);
    }

    let user_rounds: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::UserArchivedRoundIds(user))
        .unwrap_or(Vec::new(env_ref));

    let total = user_rounds.len();
    if offset >= total {
        return Vec::new(env_ref);
    }

    let start = total.saturating_sub(offset + 1);
    let end = start.saturating_sub(limit.saturating_sub(1));
    let mut idx = start;
    let mut result: Vec<ArchivedRoundSummary> = Vec::new(env_ref);
    loop {
        if let Some(round_id) = user_rounds.get(idx) {
            if let Some(summary) = env
                .storage()
                .persistent()
                .get(&DataKeyScoped::ArchivedRound(round_id))
            {
                result.push_back(summary);
            }
        }
        if idx == end || result.len() as u32 == limit {
            break;
        }
        idx = idx.saturating_sub(1);
    }

    Ok(result)
}

/// Returns user statistics (wins, losses, streaks)
pub fn get_user_stats(env: Env, user: Address) -> UserStats {
    let key = DataKeyScoped::UserStats(user);
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
    let key = DataKeyScoped::PendingWinnings(user.clone());
    _extend_persistent_ttl(&env, &key);
    let updated_key = PendingWinningsUpdatedAtKey(user);
    _extend_persistent_ttl(&env, &updated_key);
    env.storage().persistent().get(&key).unwrap_or(0)
}

/// Returns user's position in the current round (Up/Down mode).
pub fn get_user_position(env: Env, user: Address) -> Option<UserPosition> {
    if let Some(round) = env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKeyCore::ActiveRound)
    {
        let pos_key = DataKeyScoped::Position(round.round_id, user.clone());
        if let Some(pos) = env.storage().persistent().get(&pos_key) {
            return Some(pos);
        }
    }

    let legacy_updown: Map<Address, UserPosition> = env
        .storage()
        .persistent()
        .get(&DataKeyCore::UpDownPositions)
        .unwrap_or(Map::new(&env));
    if let Some(p) = legacy_updown.get(user.clone()) {
        return Some(p);
    }
    let legacy_positions: Map<Address, UserPosition> = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Positions)
        .unwrap_or(Map::new(&env));
    legacy_positions.get(user)
}

/// Returns user's precision prediction in the current round (Precision mode).
pub fn get_user_precision_prediction(env: Env, user: Address) -> Option<PrecisionPrediction> {
    if let Some(round) = env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKeyCore::ActiveRound)
    {
        let pred_key = DataKeyScoped::PrecisionPosition(round.round_id, user.clone());
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
        .get(&DataKeyCore::PrecisionPositions)
        .unwrap_or(Map::new(&env));
    legacy.get(user)
}

/// Returns all precision predictions for the current round.
pub fn get_precision_predictions(env: Env) -> Vec<PrecisionPrediction> {
    let round = match env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKeyCore::ActiveRound)
    {
        Some(r) => r,
        None => return Vec::new(&env),
    };

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::RoundParticipants(round.round_id))
        .unwrap_or(Vec::new(&env));

    let mut result: Vec<PrecisionPrediction> = Vec::new(&env);
    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            let pred_key = DataKeyScoped::PrecisionPosition(round.round_id, user.clone());
            if let Some(pred) = env.storage().persistent().get(&pred_key) {
                result.push_back(pred);
            }
        }
    }

    if result.is_empty() {
        let legacy: Map<Address, PrecisionPrediction> = env
            .storage()
            .persistent()
            .get(&DataKeyCore::PrecisionPositions)
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
        .get::<_, Round>(&DataKeyCore::ActiveRound)
    {
        Some(r) => r,
        None => return Map::new(&env),
    };

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::RoundParticipants(round.round_id))
        .unwrap_or(Vec::new(&env));

    let mut result: Map<Address, UserPosition> = Map::new(&env);
    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            let pos_key = DataKeyScoped::Position(round.round_id, user.clone());
            if let Some(pos) = env.storage().persistent().get(&pos_key) {
                result.set(user, pos);
            }
        }
    }

    if result.is_empty() {
        return env
            .storage()
            .persistent()
            .get(&DataKeyCore::UpDownPositions)
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
        .get::<_, Round>(&DataKeyCore::ActiveRound)
    {
        Some(r) => r,
        None => return Vec::new(&env),
    };

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::RoundParticipants(round.round_id))
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
            let pred_key = DataKeyScoped::PrecisionPosition(round.round_id, user.clone());
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
        .get::<_, Round>(&DataKeyCore::ActiveRound)
    {
        Some(r) => r,
        None => return Vec::new(&env),
    };

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::RoundParticipants(round.round_id))
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
            let pos_key = DataKeyScoped::Position(round.round_id, user.clone());
            if let Some(pos) = env.storage().persistent().get(&pos_key) {
                result.push_back((user, pos));
            }
        }
    }

    result
}

/// Estimates payouts for the active round given a hypothetical final price, without mutating storage.
pub fn simulate_payout(env: Env, final_price: u128) -> Result<SimulationResult, ContractError> {
    let round = env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::RoundParticipants(round.round_id))
        .unwrap_or(Vec::new(&env));
    let participants = sort_addresses(participants);

    let mut outcomes: Vec<UserRoundOutcome> = Vec::new(&env);
    let mut total_fee: i128 = 0;
    let mut precision_total_stake: i128 = 0;

    let bps = _read_protocol_fee_bps(&env);
    let fee_model = _read_fee_model(&env);

    match round.mode {
        RoundMode::UpDown => {
            let price_went_up = final_price > round.price_start;
            let price_went_down = final_price < round.price_start;
            let price_unchanged = final_price == round.price_start;
            let is_one_sided = (round.pool_up == 0) != (round.pool_down == 0);

            let mut dist_winning: i128 = 0;
            let mut dist_losing: i128 = 0;
            let mut winning_side = BetSide::Up;
            let mut winning_pool: i128 = 0;

            if !price_unchanged && !is_one_sided {
                if price_went_up {
                    winning_side = BetSide::Up;
                    winning_pool = round.pool_up;
                    let (dw, dl, fee) = calculate_protocol_fee_updown(
                        bps,
                        fee_model,
                        round.pool_up,
                        round.pool_down,
                    )?;
                    dist_winning = dw;
                    dist_losing = dl;
                    total_fee = fee;
                } else if price_went_down {
                    winning_side = BetSide::Down;
                    winning_pool = round.pool_down;
                    let (dw, dl, fee) = calculate_protocol_fee_updown(
                        bps,
                        fee_model,
                        round.pool_down,
                        round.pool_up,
                    )?;
                    dist_winning = dw;
                    dist_losing = dl;
                    total_fee = fee;
                }
            }

            let total_distributable = payout_add(dist_winning, dist_losing)?;

            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    if let Some(pos) =
                        env.storage()
                            .persistent()
                            .get::<_, UserPosition>(&DataKeyScoped::Position(
                                round.round_id,
                                user.clone(),
                            ))
                    {
                        let prediction_side = match pos.side {
                            BetSide::Up => 0,
                            BetSide::Down => 1,
                        };

                        let payout: i128;
                        let outcome_type: UserOutcomeType;

                        if price_unchanged || is_one_sided {
                            payout = pos.amount;
                            outcome_type = UserOutcomeType::Refund;
                        } else {
                            if pos.side == winning_side {
                                payout =
                                    payout_mul(pos.amount, total_distributable)? / winning_pool;
                                outcome_type = UserOutcomeType::Win;
                            } else {
                                payout = 0;
                                outcome_type = UserOutcomeType::Loss;
                            }
                        }

                        outcomes.push_back(UserRoundOutcome {
                            user,
                            round_mode: 0,
                            prediction_side,
                            predicted_price: 0,
                            stake: pos.amount,
                            payout,
                            outcome: outcome_type,
                        });
                    }
                }
            }
        }
        RoundMode::Precision => {
            let mut min_diff: Option<u128> = None;
            let mut winners: Vec<PrecisionPrediction> = Vec::new(&env);
            let mut total_pot: i128 = 0;
            let mut is_winner_mask: Vec<bool> = Vec::new(&env);
            let mut preds: Vec<PrecisionPrediction> = Vec::new(&env);
            let mut user_amounts: Vec<i128> = Vec::new(&env);

            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    let pred_opt = env.storage().persistent().get::<_, PrecisionPrediction>(
                        &DataKeyScoped::PrecisionPosition(round.round_id, user.clone()),
                    );
                    let commit_opt = env.storage().persistent().get::<_, PrecisionCommitment>(
                        &DataKeyScoped::PrecisionCommitment(round.round_id, user.clone()),
                    );

                    let mut amt = 0;
                    if let Some(ref p) = pred_opt {
                        amt = p.amount;
                        preds.push_back(p.clone());
                    } else if let Some(ref c) = commit_opt {
                        amt = c.amount;
                        preds.push_back(PrecisionPrediction {
                            user: user.clone(),
                            predicted_price: 0,
                            amount: amt,
                        });
                    } else {
                        preds.push_back(PrecisionPrediction {
                            user: user.clone(),
                            predicted_price: 0,
                            amount: 0,
                        });
                    }
                    total_pot = total_pot.checked_add(amt).ok_or(ContractError::Overflow)?;
                    user_amounts.push_back(amt);
                    is_winner_mask.push_back(false);

                    if let Some(pred) = pred_opt {
                        let diff = if pred.predicted_price >= final_price {
                            pred.predicted_price.checked_sub(final_price).unwrap()
                        } else {
                            final_price.checked_sub(pred.predicted_price).unwrap()
                        };

                        match min_diff {
                            None => {
                                min_diff = Some(diff);
                                winners.push_back(pred.clone());
                                is_winner_mask.set(i, true);
                            }
                            Some(current_min) => {
                                if diff < current_min {
                                    min_diff = Some(diff);
                                    winners = Vec::new(&env);
                                    winners.push_back(pred.clone());
                                    for j in 0..i {
                                        is_winner_mask.set(j, false);
                                    }
                                    is_winner_mask.set(i, true);
                                } else if diff == current_min {
                                    winners.push_back(pred.clone());
                                    is_winner_mask.set(i, true);
                                }
                            }
                        }
                    }
                }
            }

            precision_total_stake = total_pot;
            let mut payout_pool: i128 = 0;
            if !winners.is_empty() && total_pot > 0 {
                // Sum winner stakes for fee-on-winnings model
                let winner_stakes: i128 = winners
                    .iter()
                    .fold(0, |acc, w| acc.checked_add(w.amount).unwrap_or(acc));
                let (dist, fee) =
                    calculate_protocol_fee_precision(bps, fee_model, total_pot, winner_stakes)?;
                total_fee = fee;
                payout_pool = dist;
            }

            // Mirrors `_calculate_precision_payouts` in settlement.rs so the preview
            // never drifts from the round's configured payout policy (Equal vs
            // StakeWeighted) — a hardcoded equal split here would silently diverge
            // from the real settlement outcome for StakeWeighted rounds.
            let policy = _read_precision_payout_policy(&env);
            let winner_count = winners.len();
            let mut winner_payouts: Vec<i128> = Vec::new(&env);
            let mut total_paid: i128 = 0;

            match policy {
                PrecisionPayoutPolicy::Equal => {
                    if winner_count > 0 {
                        let payout_per_winner = payout_pool / winner_count as i128;
                        for _ in 0..winner_count {
                            winner_payouts.push_back(payout_per_winner);
                            total_paid = payout_add(total_paid, payout_per_winner)?;
                        }
                    }
                }
                PrecisionPayoutPolicy::StakeWeighted => {
                    let mut total_winner_stakes: i128 = 0;
                    for i in 0..winners.len() {
                        if let Some(winner) = winners.get(i) {
                            total_winner_stakes = payout_add(total_winner_stakes, winner.amount)?;
                        }
                    }
                    if total_winner_stakes > 0 {
                        for i in 0..winners.len() {
                            if let Some(winner) = winners.get(i) {
                                let payout =
                                    payout_mul(winner.amount, payout_pool)? / total_winner_stakes;
                                winner_payouts.push_back(payout);
                                total_paid = payout_add(total_paid, payout)?;
                            }
                        }
                    } else {
                        for _ in 0..winner_count {
                            winner_payouts.push_back(0);
                        }
                    }
                }
            }

            if winner_count > 0 {
                let remainder = payout_pool
                    .checked_sub(total_paid)
                    .ok_or(ContractError::PayoutOverflow)?;
                if let Some(base) = winner_payouts.get(0) {
                    let adjusted = payout_add(base, remainder)?;
                    winner_payouts.set(0, adjusted);
                }
            }

            let mut winner_idx: u32 = 0;
            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    let mut payout = 0;
                    let mut outcome_type = UserOutcomeType::Loss;
                    let is_winner = is_winner_mask.get(i).unwrap_or(false);
                    let amt = user_amounts.get(i).unwrap_or(0);
                    let pred_price = if i < preds.len() {
                        preds.get(i).unwrap().predicted_price
                    } else {
                        0
                    };

                    if winners.is_empty() {
                        payout = amt; // refund
                        outcome_type = UserOutcomeType::Refund;
                    } else if is_winner {
                        payout = winner_payouts.get(winner_idx).unwrap_or(0);
                        outcome_type = UserOutcomeType::Win;
                        winner_idx += 1;
                    }

                    outcomes.push_back(UserRoundOutcome {
                        user,
                        round_mode: 1,
                        prediction_side: 2, // arbitrary
                        predicted_price: pred_price,
                        stake: amt,
                        payout,
                        outcome: outcome_type,
                    });
                }
            }
        }
    }

    Ok(SimulationResult {
        mode: round.mode,
        pool_up: round.pool_up,
        pool_down: round.pool_down,
        precision_total_stake,
        fee_amount: total_fee,
        fee_model: fee_model as u32,
        outcomes,
    })
}

// ─── Cursor helpers ──────────────────────────────────────────────────────────

/// Finds the index of the first address in a sorted Vec that is strictly greater
/// than `cursor`. Returns 0 when `cursor` is `None` (start from beginning).
/// When the cursor is past the last element, returns `sorted.len()`.
fn _find_cursor_position(sorted: &Vec<Address>, cursor: &Option<Address>) -> u32 {
    let cursor_addr = match cursor {
        Some(a) => a,
        None => return 0,
    };

    for i in 0..sorted.len() {
        if let Some(addr) = sorted.get(i) {
            if &addr > cursor_addr {
                return i;
            }
        }
    }
    sorted.len()
}

/// Finds the index of the first entry in a sorted leaderboard Vec whose user
/// address is strictly greater than `cursor`. Returns 0 when `cursor` is `None`.
fn _find_cursor_in_leaderboard(sorted: &Vec<LeaderboardEntry>, cursor: &Option<Address>) -> u32 {
    let cursor_addr = match cursor {
        Some(a) => a,
        None => return 0,
    };

    for i in 0..sorted.len() {
        if let Some(entry) = sorted.get(i) {
            if &entry.user > cursor_addr {
                return i;
            }
        }
    }
    sorted.len()
}

// ─── Cursor-based participant/prediction pagination ──────────────────────────

/// Returns a cursor-based page of Precision-mode predictions for the active round.
///
/// `cursor`: The last user address from the previous page, or `None` to start
/// from the beginning. Returns error if `limit` exceeds `MAX_PAGE_SIZE` (100).
///
/// The returned `next_cursor` is the last address included in this page, or
/// `None` when the page is empty or the dataset is exhausted.
pub fn get_precision_predictions_cursor(
    env: Env,
    cursor: Option<Address>,
    limit: u32,
) -> Result<(Vec<PrecisionPrediction>, Option<Address>), ContractError> {
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(ContractError::PageSizeExceeded);
    }

    let round = match env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKeyCore::ActiveRound)
    {
        Some(r) => r,
        None => return Ok((Vec::new(&env), None)),
    };

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::RoundParticipants(round.round_id))
        .unwrap_or(Vec::new(&env));
    let participants = sort_addresses(participants);

    let total = participants.len();
    let start = _find_cursor_position(&participants, &cursor);
    if start >= total {
        return Ok((Vec::new(&env), None));
    }

    let end = start.saturating_add(limit).min(total);

    let mut items: Vec<PrecisionPrediction> = Vec::new(&env);
    let mut last_addr: Option<Address> = None;
    for i in start..end {
        if let Some(user) = participants.get(i) {
            let pred_key = DataKeyScoped::PrecisionPosition(round.round_id, user.clone());
            if let Some(pred) = env.storage().persistent().get(&pred_key) {
                last_addr = Some(user.clone());
                items.push_back(pred);
            }
        }
    }

    Ok((items, last_addr))
}

/// Returns a cursor-based page of Up/Down positions for the active round.
///
/// Each item is a `(Address, UserPosition)` pair sorted by address ascending.
/// Returns error if `limit` exceeds `MAX_PAGE_SIZE` (100).
pub fn get_updown_positions_cursor(
    env: Env,
    cursor: Option<Address>,
    limit: u32,
) -> Result<(Vec<(Address, UserPosition)>, Option<Address>), ContractError> {
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(ContractError::PageSizeExceeded);
    }

    let round = match env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKeyCore::ActiveRound)
    {
        Some(r) => r,
        None => return Ok((Vec::new(&env), None)),
    };

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::RoundParticipants(round.round_id))
        .unwrap_or(Vec::new(&env));
    let participants = sort_addresses(participants);

    let total = participants.len();
    let start = _find_cursor_position(&participants, &cursor);
    if start >= total {
        return Ok((Vec::new(&env), None));
    }

    let end = start.saturating_add(limit).min(total);

    let mut items: Vec<(Address, UserPosition)> = Vec::new(&env);
    let mut last_addr: Option<Address> = None;
    for i in start..end {
        if let Some(user) = participants.get(i) {
            let pos_key = DataKeyScoped::Position(round.round_id, user.clone());
            if let Some(pos) = env.storage().persistent().get(&pos_key) {
                last_addr = Some(user.clone());
                items.push_back((user, pos));
            }
        }
    }

    Ok((items, last_addr))
}

// ─── Leaderboard queries ─────────────────────────────────────────────────────

/// Collects all on-chain user stats into a deterministic sorted Vec of
/// `LeaderboardEntry` by scanning known participants from the active round
/// and recent archived rounds.
///
/// This is a best-effort snapshot: users with stats but no recent round
/// participation may not appear. A complete solution would maintain a
/// dedicated leaderboard index (future enhancement).
fn _collect_leaderboard_entries(env: &Env) -> Vec<LeaderboardEntry> {
    let mut seen: Vec<Address> = Vec::new(env);

    // Collect from the active round participant list if any.
    if let Some(round) = env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKeyCore::ActiveRound)
    {
        let participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKeyScoped::RoundParticipants(round.round_id))
            .unwrap_or(Vec::new(env));
        for i in 0..participants.len() {
            if let Some(user) = participants.get(i) {
                let mut found = false;
                for j in 0..seen.len() {
                    if let Some(s) = seen.get(j) {
                        if s == user {
                            found = true;
                            break;
                        }
                    }
                }
                if !found {
                    seen.push_back(user);
                }
            }
        }
    }

    // Build leaderboard entries from seen users.
    let mut entries = Vec::new(env);
    for i in 0..seen.len() {
        if let Some(user) = seen.get(i) {
            let stats = get_user_stats(env.clone(), user.clone());
            entries.push_back(LeaderboardEntry {
                user: user.clone(),
                stats,
            });
        }
    }

    entries
}

/// Returns a cursor-based page of the global leaderboard ordered by total wins
/// descending, with address ascending as tiebreaker.
///
/// `cursor`: The last user address from the previous page, or `None` to start
/// from the beginning. Returns error if `limit` exceeds `MAX_PAGE_SIZE` (100).
///
/// The result is a snapshot built from on-chain `UserStats` collected from
/// active and recent round participants.
pub fn get_leaderboard_by_wins(
    env: Env,
    cursor: Option<Address>,
    limit: u32,
) -> Result<(Vec<LeaderboardEntry>, Option<Address>), ContractError> {
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(ContractError::PageSizeExceeded);
    }

    let entries = _collect_leaderboard_entries(&env);

    // Sort: wins descending, then address ascending (tiebreaker).
    let mut sorted: Vec<LeaderboardEntry> = Vec::new(&env);
    for i in 0..entries.len() {
        if let Some(e) = entries.get(i) {
            let mut inserted = false;
            let mut j: u32 = 0;
            while j < sorted.len() {
                if let Some(cur) = sorted.get(j) {
                    if e.stats.total_wins > cur.stats.total_wins
                        || (e.stats.total_wins == cur.stats.total_wins && e.user < cur.user)
                    {
                        sorted.insert(j, e.clone());
                        inserted = true;
                        break;
                    }
                }
                j += 1;
            }
            if !inserted {
                sorted.push_back(e);
            }
        }
    }

    let total = sorted.len();
    let start = _find_cursor_in_leaderboard(&sorted, &cursor);
    if start >= total {
        return Ok((Vec::new(&env), None));
    }

    let end = start.saturating_add(limit).min(total);
    let mut items = Vec::new(&env);
    let mut last_addr: Option<Address> = None;
    for i in start..end {
        if let Some(e) = sorted.get(i) {
            last_addr = Some(e.user.clone());
            items.push_back(e.clone());
        }
    }

    Ok((items, last_addr))
}

/// Returns a cursor-based page of the global leaderboard ordered by best streak
/// descending, with address ascending as tiebreaker. Returns error if `limit`
/// exceeds `MAX_PAGE_SIZE` (100).
pub fn get_leaderboard_by_streak(
    env: Env,
    cursor: Option<Address>,
    limit: u32,
) -> Result<(Vec<LeaderboardEntry>, Option<Address>), ContractError> {
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(ContractError::PageSizeExceeded);
    }

    let entries = _collect_leaderboard_entries(&env);

    // Sort: best_streak descending, then address ascending (tiebreaker).
    let mut sorted: Vec<LeaderboardEntry> = Vec::new(&env);
    for i in 0..entries.len() {
        if let Some(e) = entries.get(i) {
            let mut inserted = false;
            let mut j: u32 = 0;
            while j < sorted.len() {
                if let Some(cur) = sorted.get(j) {
                    if e.stats.best_streak > cur.stats.best_streak
                        || (e.stats.best_streak == cur.stats.best_streak && e.user < cur.user)
                    {
                        sorted.insert(j, e.clone());
                        inserted = true;
                        break;
                    }
                }
                j += 1;
            }
            if !inserted {
                sorted.push_back(e);
            }
        }
    }

    let total = sorted.len();
    let start = _find_cursor_in_leaderboard(&sorted, &cursor);
    if start >= total {
        return Ok((Vec::new(&env), None));
    }

    let end = start.saturating_add(limit).min(total);
    let mut items = Vec::new(&env);
    let mut last_addr: Option<Address> = None;
    for i in start..end {
        if let Some(e) = sorted.get(i) {
            last_addr = Some(e.user.clone());
            items.push_back(e.clone());
        }
    }

    (items, last_addr)
}
    Ok((items, last_addr))
}
