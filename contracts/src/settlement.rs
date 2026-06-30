// SPDX-License-Identifier: MIT
use soroban_sdk::{
    symbol_short, Address, Env, Symbol, Vec, Map,
};
use crate::errors::ContractError;
use crate::types::{
    DataKey, Round, RoundMode, BetSide, UserPosition, PrecisionPrediction, PrecisionCommitment,
    RoundArchiveStatus, UserOutcomeType, UserRoundOutcome, ArchivedRoundSummary, OraclePayload, UserStats,
};
use crate::common::{
    _extend_persistent_ttl, payout_add, payout_mul, _accumulate_pending, _emit_action_rejected,
    balance, _set_balance, sort_addresses,
    DEFAULT_ARCHIVE_RETENTION, BPS_DENOMINATOR,
};
use crate::admin::{_require_supported_schema, _ensure_not_paused, _ensure_normal_mode, _set_mode};
use crate::config::{_apply_protocol_fee_updown, _apply_protocol_fee_precision};

/// Cancels the active round and deterministically refunds all participant stakes.
pub fn cancel_round(env: Env, reason: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();

    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveRound)
        .ok_or_else(|| {
            _emit_action_rejected(
                &env,
                &admin,
                symbol_short!("cancel"),
                ContractError::RoundNotCancellable,
            );
            ContractError::RoundNotCancellable
        })?;

    let round_id = round.round_id;

    // Refund all participants based on round mode
    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::RoundParticipants(round_id))
        .unwrap_or(Vec::new(&env));

    match round.mode {
        RoundMode::UpDown => {
            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    let pos_key = DataKey::Position(round_id, user.clone());
                    if let Some(pos) =
                        env.storage().persistent().get::<_, UserPosition>(&pos_key)
                    {
                        _accumulate_pending(&env, user.clone(), pos.amount)?;
                        let prediction_side = match pos.side {
                            BetSide::Up => 0,
                            BetSide::Down => 1,
                        };
                        _persist_user_outcome(
                            &env,
                            round_id,
                            0,
                            &user,
                            prediction_side,
                            0,
                            pos.amount,
                            pos.amount,
                            UserOutcomeType::Cancel,
                        );
                        env.storage().persistent().remove(&pos_key);
                    }
                }
            }
        }
        RoundMode::Precision => {
            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    let pred_key = DataKey::PrecisionPosition(round_id, user.clone());
                    let commit_key = DataKey::PrecisionCommitment(round_id, user.clone());

                    let mut refund_amount = 0;
                    if let Some(pred) = env
                        .storage()
                        .persistent()
                        .get::<_, PrecisionPrediction>(&pred_key)
                    {
                        refund_amount = pred.amount;
                    } else if let Some(commit) = env
                        .storage()
                        .persistent()
                        .get::<_, PrecisionCommitment>(&commit_key)
                    {
                        refund_amount = commit.amount;
                    }

                    if refund_amount > 0 {
                        _accumulate_pending(&env, user.clone(), refund_amount)?;
                    }
                    _persist_user_outcome(
                        &env,
                        round_id,
                        1,
                        &user,
                        2,
                        0,
                        refund_amount,
                        refund_amount,
                        UserOutcomeType::Cancel,
                    );
                    env.storage().persistent().remove(&pred_key);
                    env.storage().persistent().remove(&commit_key);
                }
            }
        }
    }

    // Clean up participant list and mark round as cancelled
    let participant_count = participants.len();
    _archive_round(
        &env,
        &round,
        RoundArchiveStatus::Cancelled,
        0,
        participant_count,
    );

    env.storage()
        .persistent()
        .remove(&DataKey::RoundParticipants(round_id));
    env.storage()
        .persistent()
        .set(&DataKey::CancelledRound(round_id), &true);
    env.storage().persistent().remove(&DataKey::ActiveRound);

    // Emit cancellation event
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("round"), symbol_short!("cancel")),
        (round_id, reason, round.pool_up, round.pool_down),
    );

    Ok(())
}

/// Returns true if the given round_id was cancelled.
pub fn is_round_cancelled(env: Env, round_id: u64) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::CancelledRound(round_id))
        .unwrap_or(false)
}

/// Claims pending winnings and adds to balance
pub fn claim_winnings(env: Env, user: Address) -> Result<i128, ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_not_paused(&env)?;

    let key = DataKey::PendingWinnings(user.clone());
    let pending: i128 = env.storage().persistent().get(&key).unwrap_or(0);

    if pending == 0 {
        return Ok(0);
    }

    let current_balance = balance(env.clone(), user.clone());
    let new_balance = payout_add(current_balance, pending)?;

    env.storage().persistent().remove(&key);
    _set_balance(&env, user.clone(), new_balance);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("claim"), symbol_short!("winnings")),
        (user, pending),
    );

    Ok(pending)
}

pub fn resolve_round(env: Env, payload: OraclePayload) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    if payload.price == 0 {
        return Err(ContractError::InvalidPrice);
    }

    _extend_persistent_ttl(&env, &DataKey::Oracle);
    let oracle: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Oracle)
        .ok_or(ContractError::OracleNotSet)?;

    oracle.require_auth();
    _ensure_not_paused(&env).map_err(|e| {
        _emit_action_rejected(&env, &oracle, symbol_short!("resolve"), e);
        e
    })?;

    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    // Verify round ID matches to prevent cross-round replays
    if payload.round_id != round.start_ledger {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::InvalidOracleRound,
        );
        return Err(ContractError::InvalidOracleRound);
    }

    // Reject payloads targeting a different network or contract deployment.
    if payload.network_id != env.ledger().network_id() {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::OracleNetworkMismatch,
        );
        return Err(ContractError::OracleNetworkMismatch);
    }
    if payload.contract_addr != env.current_contract_address() {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::OracleContractMismatch,
        );
        return Err(ContractError::OracleContractMismatch);
    }

    // Verify data freshness (max 300 seconds / 5 minutes old)
    let current_time = env.ledger().timestamp();

    // Reject future timestamps to prevent time-skew manipulation
    if payload.timestamp > current_time {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::FutureOracleData,
        );
        return Err(ContractError::FutureOracleData);
    }

    if current_time > payload.timestamp + 300 {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::StaleOracleData,
        );
        return Err(ContractError::StaleOracleData);
    }

    // Oracle deviation guardrails
    _extend_persistent_ttl(&env, &DataKey::OracleMaxDeviationBps);
    if let Some(max_bps) = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::OracleMaxDeviationBps)
    {
        let start_price = round.price_start;
        if start_price == 0 {
            return Err(ContractError::InvalidPrice);
        }

        let diff = if payload.price >= start_price {
            payload
                .price
                .checked_sub(start_price)
                .ok_or(ContractError::Overflow)?
        } else {
            start_price
                .checked_sub(payload.price)
                .ok_or(ContractError::Overflow)?
        };

        let diff_bps_u128 = diff
            .checked_mul(10_000u128)
            .ok_or(ContractError::Overflow)?
            / start_price;
        let diff_bps: u32 = diff_bps_u128
            .try_into()
            .map_err(|_| ContractError::Overflow)?;

        let override_armed: bool = env
            .storage()
            .persistent()
            .get(&DataKey::OracleDeviationOverrideArmed)
            .unwrap_or(false);

        if diff_bps > max_bps && !override_armed {
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("oracle"), symbol_short!("rejected")),
                (
                    round.round_id,
                    start_price,
                    payload.price,
                    diff_bps,
                    max_bps,
                ),
            );
            return Err(ContractError::OracleDeviationExceeded);
        }

        if diff_bps > max_bps && override_armed {
            env.storage()
                .persistent()
                .remove(&DataKey::OracleDeviationOverrideArmed);

            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("oracle"), symbol_short!("override")),
                (
                    round.round_id,
                    start_price,
                    payload.price,
                    diff_bps,
                    max_bps,
                ),
            );
        }
    }

    // Oracle confidence guardrails
    _extend_persistent_ttl(&env, &DataKey::OracleMinConfidenceBps);
    _extend_persistent_ttl(&env, &DataKey::OracleStrictMode);
    if let Some(min_confidence_bps) = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::OracleMinConfidenceBps)
    {
        match payload.confidence {
            None => {
                let strict_mode: bool = env
                    .storage()
                    .persistent()
                    .get(&DataKey::OracleStrictMode)
                    .unwrap_or(false);
                if strict_mode {
                    return Err(ContractError::InvalidPrice);
                }
            }
            Some(confidence_bps) => {
                if confidence_bps > 10_000 || confidence_bps < min_confidence_bps {
                    #[allow(deprecated)]
                    env.events().publish(
                        (symbol_short!("oracle"), symbol_short!("lowconf")),
                        (round.round_id, confidence_bps, min_confidence_bps),
                    );
                    return Err(ContractError::InvalidPrice);
                }
            }
        }
    }

    let nonce_key = DataKey::ConsumedOracleNonce(round.round_id, payload.nonce);
    if env.storage().persistent().has(&nonce_key) {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::OracleNonceReused,
        );
        return Err(ContractError::OracleNonceReused);
    }
    env.storage().persistent().set(&nonce_key, &true);

    let current_ledger = env.ledger().sequence();
    if current_ledger < round.end_ledger {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::RoundNotEnded,
        );
        return Err(ContractError::RoundNotEnded);
    }

    let round_id = round.round_id;

    // Minimum participants threshold check
    if let Some(min) = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKey::MinParticipants)
    {
        let threshold_participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundParticipants(round_id))
            .unwrap_or(Vec::new(&env));
        let count = threshold_participants.len();
        if count < min {
            _archive_round(
                &env,
                &round,
                RoundArchiveStatus::FallbackRefund,
                payload.price,
                count,
            );
            _refund_under_threshold(&env, &round, &threshold_participants)?;
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("round"), symbol_short!("fallback")),
                (round_id, count, min),
            );
            return Ok(());
        }
    }

    match round.mode {
        RoundMode::UpDown => {
            let one_sided = _resolve_updown_mode(&env, &round, payload.price)?;
            if one_sided {
                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("pool"), symbol_short!("onesided")),
                    (round_id, round.pool_up, round.pool_down),
                );
            }
        }
        RoundMode::Precision => {
            _resolve_precision_mode(&env, round_id, payload.price)?;
        }
    }

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::RoundParticipants(round_id))
        .unwrap_or(Vec::new(&env));
    let participant_count = participants.len();

    _archive_round(
        &env,
        &round,
        RoundArchiveStatus::Resolved,
        payload.price,
        participant_count,
    );

    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            env.storage()
                .persistent()
                .remove(&DataKey::Position(round_id, user.clone()));
            env.storage()
                .persistent()
                .remove(&DataKey::PrecisionPosition(round_id, user.clone()));
            env.storage()
                .persistent()
                .remove(&DataKey::PrecisionCommitment(round_id, user));
        }
    }
    env.storage()
        .persistent()
        .remove(&DataKey::RoundParticipants(round_id));

    env.storage().persistent().remove(&DataKey::ActiveRound);
    env.storage().persistent().remove(&DataKey::Positions);
    env.storage().persistent().remove(&DataKey::UpDownPositions);
    env.storage()
        .persistent()
        .remove(&DataKey::PrecisionPositions);

    let mode_value: u32 = match round.mode {
        RoundMode::UpDown => 0,
        RoundMode::Precision => 1,
    };
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("round"), symbol_short!("resolved")),
        (round_id, payload.price, mode_value, payload.confidence),
    );

    Ok(())
}

// ─── Internal helpers ────────────────────────────────────────────────────────

pub fn _resolve_updown_mode(
    env: &Env,
    round: &Round,
    final_price: u128,
) -> Result<bool, ContractError> {
    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::RoundParticipants(round.round_id))
        .unwrap_or(Vec::new(env));
    let participants = sort_addresses(participants);

    let price_went_up = final_price > round.price_start;
    let price_went_down = final_price < round.price_start;
    let price_unchanged = final_price == round.price_start;

    let is_one_sided = (price_went_up && round.pool_down == 0 && round.pool_up > 0)
        || (price_went_down && round.pool_up == 0 && round.pool_down > 0);

    if !participants.is_empty() {
        if price_unchanged || is_one_sided {
            _record_refunds_indexed(env, round.round_id, 0, &participants)?;
        } else if price_went_up {
            _record_winnings_indexed(
                env,
                round.round_id,
                &participants,
                BetSide::Up,
                round.pool_up,
                round.pool_down,
            )?;
        } else if price_went_down {
            _record_winnings_indexed(
                env,
                round.round_id,
                &participants,
                BetSide::Down,
                round.pool_down,
                round.pool_up,
            )?;
        }
    } else {
        let positions: Map<Address, UserPosition> = env
            .storage()
            .persistent()
            .get(&DataKey::UpDownPositions)
            .unwrap_or(Map::new(env));
        if !positions.is_empty() {
            if price_unchanged {
                _record_refunds_legacy(env, round.round_id, &positions)?;
            } else if price_went_up {
                _record_winnings_legacy(
                    env,
                    round.round_id,
                    &positions,
                    BetSide::Up,
                    round.pool_up,
                    round.pool_down,
                )?;
            } else if price_went_down {
                _record_winnings_legacy(
                    env,
                    round.round_id,
                    &positions,
                    BetSide::Down,
                    round.pool_down,
                    round.pool_up,
                )?;
            }
        }
    }

    Ok(is_one_sided)
}

pub fn _record_refunds_legacy(
    env: &Env,
    round_id: u64,
    positions: &Map<Address, UserPosition>,
) -> Result<(), ContractError> {
    let keys: Vec<Address> = positions.keys();
    for i in 0..keys.len() {
        if let Some(user) = keys.get(i) {
            if let Some(position) = positions.get(user.clone()) {
                _accumulate_pending(env, user.clone(), position.amount)?;
                let prediction_side = match position.side {
                    BetSide::Up => 0,
                    BetSide::Down => 1,
                };
                _persist_user_outcome(
                    env,
                    round_id,
                    0,
                    &user,
                    prediction_side,
                    0,
                    position.amount,
                    position.amount,
                    UserOutcomeType::Refund,
                );
            }
        }
    }
    Ok(())
}

pub fn _record_winnings_legacy(
    env: &Env,
    round_id: u64,
    positions: &Map<Address, UserPosition>,
    winning_side: BetSide,
    winning_pool: i128,
    losing_pool: i128,
) -> Result<(), ContractError> {
    if winning_pool == 0 {
        return Ok(());
    }

    let (winning_pool, losing_pool, _fee_amount) =
        _apply_protocol_fee_updown(env, round_id, winning_pool, losing_pool)?;

    let keys: Vec<Address> = positions.keys();
    for i in 0..keys.len() {
        if let Some(user) = keys.get(i) {
            if let Some(position) = positions.get(user.clone()) {
                if position.side == winning_side {
                    let share_numerator = payout_mul(position.amount, losing_pool)?;
                    let share = share_numerator / winning_pool;
                    let payout = payout_add(position.amount, share)?;

                    _accumulate_pending(env, user.clone(), payout)?;
                    _update_stats_win(env, user.clone())?;

                    let side_value = match position.side {
                        BetSide::Up => 0,
                        BetSide::Down => 1,
                    };
                    _persist_user_outcome(
                        env,
                        round_id,
                        0,
                        &user,
                        side_value,
                        0,
                        position.amount,
                        payout,
                        UserOutcomeType::Win,
                    );
                } else {
                    let side_value: u32 = match position.side {
                        BetSide::Up => 0,
                        BetSide::Down => 1,
                    };
                    #[allow(deprecated)]
                    env.events().publish(
                        (symbol_short!("outcome"), symbol_short!("loss")),
                        (
                            user.clone(),
                            round_id,
                            0u32,
                            position.amount,
                            side_value,
                            0u128,
                        ),
                    );
                    _update_stats_loss(env, user.clone())?;

                    _persist_user_outcome(
                        env,
                        round_id,
                        0,
                        &user,
                        side_value,
                        0,
                        position.amount,
                        0,
                        UserOutcomeType::Loss,
                    );
                }
            }
        }
    }

    Ok(())
}

pub fn _resolve_precision_mode(
    env: &Env,
    round_id: u64,
    final_price: u128,
) -> Result<(), ContractError> {
    let mut participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::RoundParticipants(round_id))
        .unwrap_or(Vec::new(env));
    participants = sort_addresses(participants);

    if participants.is_empty() {
        let legacy: Map<Address, PrecisionPrediction> = env
            .storage()
            .persistent()
            .get(&DataKey::PrecisionPositions)
            .unwrap_or(Map::new(env));
        if legacy.is_empty() {
            return Ok(());
        }
        return _resolve_precision_legacy(env, round_id, &legacy, final_price);
    }

    let mut min_diff: Option<u128> = None;
    let mut winners: Vec<PrecisionPrediction> = Vec::new(env);
    let mut total_pot: i128 = 0;
    let mut participant_amounts: Vec<i128> = Vec::new(env);
    let mut participant_prices: Vec<u128> = Vec::new(env);
    let mut is_winner_mask: Vec<bool> = Vec::new(env);

    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            let pred_key = DataKey::PrecisionPosition(round_id, user.clone());
            let commit_key = DataKey::PrecisionCommitment(round_id, user.clone());

            let pred_opt = env
                .storage()
                .persistent()
                .get::<_, PrecisionPrediction>(&pred_key);

            let commitment_opt = env
                .storage()
                .persistent()
                .get::<_, PrecisionCommitment>(&commit_key);

            let amount = if let Some(ref pred) = pred_opt {
                pred.amount
            } else if let Some(ref commit) = commitment_opt {
                commit.amount
            } else {
                0
            };
            let cached_price = pred_opt
                .as_ref()
                .map(|p| p.predicted_price)
                .unwrap_or(0u128);

            total_pot = total_pot
                .checked_add(amount)
                .ok_or(ContractError::Overflow)?;
            participant_amounts.push_back(amount);
            participant_prices.push_back(cached_price);
            is_winner_mask.push_back(false);

            if let Some(pred) = pred_opt {
                let diff = if pred.predicted_price >= final_price {
                    pred.predicted_price
                        .checked_sub(final_price)
                        .ok_or(ContractError::Overflow)?
                } else {
                    final_price
                        .checked_sub(pred.predicted_price)
                        .ok_or(ContractError::Overflow)?
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
                            winners = Vec::new(env);
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

    if !winners.is_empty() && total_pot > 0 {
        let (payout_pool, _fee_amount) =
            _apply_protocol_fee_precision(env, round_id, total_pot)?;
        let winner_count = winners.len() as i128;
        let payout_per_winner = payout_pool / winner_count;
        let remainder = payout_pool % winner_count;

        for i in 0..winners.len() {
            if let Some(winner) = winners.get(i) {
                let payout = if i == 0 {
                    payout_per_winner
                        .checked_add(remainder)
                        .ok_or(ContractError::Overflow)?
                } else {
                    payout_per_winner
                };

                _accumulate_pending(env, winner.user.clone(), payout)?;
                _update_stats_win(env, winner.user.clone())?;

                _persist_user_outcome(
                    env,
                    round_id,
                    1,
                    &winner.user,
                    2,
                    winner.predicted_price,
                    winner.amount,
                    payout,
                    UserOutcomeType::Win,
                );
            }
        }

        for i in 0..participants.len() {
            if let Some(user) = participants.get(i) {
                let was_winner = is_winner_mask.get(i).unwrap_or(false);
                if !was_winner {
                    let stake = participant_amounts.get(i).unwrap();
                    let predicted_price = participant_prices.get(i).unwrap();

                    #[allow(deprecated)]
                    env.events().publish(
                        (symbol_short!("outcome"), symbol_short!("loss")),
                        (user.clone(), round_id, 1u32, stake, 0u32, predicted_price),
                    );
                    _update_stats_loss(env, user.clone())?;

                    _persist_user_outcome(
                        env,
                        round_id,
                        1,
                        &user,
                        2,
                        predicted_price,
                        stake,
                        0,
                        UserOutcomeType::Loss,
                    );
                }
            }
        }
    }

    Ok(())
}

pub fn _resolve_precision_legacy(
    env: &Env,
    round_id: u64,
    predictions_map: &Map<Address, PrecisionPrediction>,
    final_price: u128,
) -> Result<(), ContractError> {
    let predictions = predictions_map.values();
    if predictions.is_empty() {
        return Ok(());
    }

    let mut min_diff: Option<u128> = None;
    let mut winners: Vec<PrecisionPrediction> = Vec::new(env);

    for i in 0..predictions.len() {
        if let Some(pred) = predictions.get(i) {
            let diff = if pred.predicted_price >= final_price {
                pred.predicted_price
                    .checked_sub(final_price)
                    .ok_or(ContractError::Overflow)?
            } else {
                final_price
                    .checked_sub(pred.predicted_price)
                    .ok_or(ContractError::Overflow)?
            };

            match min_diff {
                None => {
                    min_diff = Some(diff);
                    winners.push_back(pred.clone());
                }
                Some(current_min) => {
                    if diff < current_min {
                        min_diff = Some(diff);
                        winners = Vec::new(env);
                        winners.push_back(pred.clone());
                    } else if diff == current_min {
                        winners.push_back(pred.clone());
                    }
                }
            }
        }
    }

    let mut total_pot: i128 = 0;
    for i in 0..predictions.len() {
        if let Some(pred) = predictions.get(i) {
            total_pot = payout_add(total_pot, pred.amount)?;
        }
    }

    if !winners.is_empty() && total_pot > 0 {
        let (payout_pool, _fee_amount) =
            _apply_protocol_fee_precision(env, round_id, total_pot)?;
        let winner_count = winners.len() as i128;
        let payout_per_winner = payout_pool / winner_count;
        let remainder = payout_pool % winner_count;

        for i in 0..winners.len() {
            if let Some(winner) = winners.get(i) {
                let payout = if i == 0 {
                    payout_add(payout_per_winner, remainder)?
                } else {
                    payout_per_winner
                };
                _accumulate_pending(env, winner.user.clone(), payout)?;
                _update_stats_win(env, winner.user.clone())?;

                _persist_user_outcome(
                    env,
                    round_id,
                    1,
                    &winner.user,
                    2,
                    winner.predicted_price,
                    winner.amount,
                    payout,
                    UserOutcomeType::Win,
                );
            }
        }

        for i in 0..predictions.len() {
            if let Some(pred) = predictions.get(i) {
                let is_winner = winners.iter().any(|w| w.user == pred.user);
                if !is_winner {
                    #[allow(deprecated)]
                    env.events().publish(
                        (symbol_short!("outcome"), symbol_short!("loss")),
                        (
                            pred.user.clone(),
                            round_id,
                            1u32,
                            pred.amount,
                            0u32,
                            pred.predicted_price,
                        ),
                    );
                    _update_stats_loss(env, pred.user.clone())?;

                    _persist_user_outcome(
                        env,
                        round_id,
                        1,
                        &pred.user,
                        2,
                        pred.predicted_price,
                        pred.amount,
                        0,
                        UserOutcomeType::Loss,
                    );
                }
            }
        }
    }

    Ok(())
}

pub fn _record_refunds_indexed(
    env: &Env,
    round_id: u64,
    round_mode: u32,
    participants: &Vec<Address>,
) -> Result<(), ContractError> {
    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            let pos_key = DataKey::Position(round_id, user.clone());
            if let Some(position) = env.storage().persistent().get::<_, UserPosition>(&pos_key) {
                _accumulate_pending(env, user.clone(), position.amount)?;
                let prediction_side = match position.side {
                    BetSide::Up => 0,
                    BetSide::Down => 1,
                };
                _persist_user_outcome(
                    env,
                    round_id,
                    round_mode,
                    &user,
                    prediction_side,
                    0,
                    position.amount,
                    position.amount,
                    UserOutcomeType::Refund,
                );
            }
        }
    }
    Ok(())
}

pub fn _record_winnings_indexed(
    env: &Env,
    round_id: u64,
    participants: &Vec<Address>,
    winning_side: BetSide,
    winning_pool: i128,
    losing_pool: i128,
) -> Result<(), ContractError> {
    if winning_pool == 0 {
        return Ok(());
    }

    let (winning_pool, losing_pool, _fee_amount) =
        _apply_protocol_fee_updown(env, round_id, winning_pool, losing_pool)?;

    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            let pos_key = DataKey::Position(round_id, user.clone());
            if let Some(position) = env.storage().persistent().get::<_, UserPosition>(&pos_key) {
                if position.side == winning_side {
                    let share_numerator = payout_mul(position.amount, losing_pool)?;
                    let share = share_numerator / winning_pool;
                    let payout = payout_add(position.amount, share)?;

                    _accumulate_pending(env, user.clone(), payout)?;
                    _update_stats_win(env, user.clone())?;

                    let side_value = match position.side {
                        BetSide::Up => 0,
                        BetSide::Down => 1,
                    };
                    _persist_user_outcome(
                        env,
                        round_id,
                        0,
                        &user,
                        side_value,
                        0,
                        position.amount,
                        payout,
                        UserOutcomeType::Win,
                    );
                } else {
                    let side_value: u32 = match position.side {
                        BetSide::Up => 0,
                        BetSide::Down => 1,
                    };
                    #[allow(deprecated)]
                    env.events().publish(
                        (symbol_short!("outcome"), symbol_short!("loss")),
                        (
                            user.clone(),
                            round_id,
                            0u32,
                            position.amount,
                            side_value,
                            0u128,
                        ),
                    );
                    _update_stats_loss(env, user.clone())?;

                    _persist_user_outcome(
                        env,
                        round_id,
                        0,
                        &user,
                        side_value,
                        0,
                        position.amount,
                        0,
                        UserOutcomeType::Loss,
                    );
                }
            }
        }
    }

    Ok(())
}

pub fn _archive_round(
    env: &Env,
    round: &Round,
    status: RoundArchiveStatus,
    final_price: u128,
    participant_count: u32,
) {
    let status_val = status.clone() as u32;
    let summary = ArchivedRoundSummary {
        round_id: round.round_id,
        price_start: round.price_start,
        price_final: final_price,
        mode: round.mode.clone(),
        status,
        pool_up: round.pool_up,
        pool_down: round.pool_down,
        participant_count,
        settled_at_ledger: env.ledger().sequence(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::ArchivedRound(round.round_id), &summary);

    let mut total_pot: i128 = 0;
    match round.mode {
        RoundMode::UpDown => {
            total_pot = round.pool_up.checked_add(round.pool_down).unwrap_or(0);
        }
        RoundMode::Precision => {
            let participants: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::RoundParticipants(round.round_id))
                .unwrap_or(Vec::new(env));
            if participants.is_empty() {
                let legacy: Map<Address, PrecisionPrediction> = env
                    .storage()
                    .persistent()
                    .get(&DataKey::PrecisionPositions)
                    .unwrap_or(Map::new(env));
                for entry in legacy.iter() {
                    total_pot = total_pot.checked_add(entry.1.amount).unwrap_or(total_pot);
                }
            } else {
                for i in 0..participants.len() {
                    if let Some(user) = participants.get(i) {
                        let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
                        let commit_key =
                            DataKey::PrecisionCommitment(round.round_id, user.clone());

                        let pred_opt = env
                            .storage()
                            .persistent()
                            .get::<_, PrecisionPrediction>(&pred_key);

                        let commitment_opt = env
                            .storage()
                            .persistent()
                            .get::<_, PrecisionCommitment>(&commit_key);

                        let amount = if let Some(ref pred) = pred_opt {
                            pred.amount
                        } else if let Some(ref commit) = commitment_opt {
                            commit.amount
                        } else {
                            0
                        };
                        total_pot = total_pot.checked_add(amount).unwrap_or(total_pot);
                    }
                }
            }
        }
    }

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("round"), Symbol::new(env, "summary")),
        (
            round.round_id,
            round.mode.clone() as u32,
            round.price_start,
            final_price,
            participant_count,
            total_pot,
            status_val,
        ),
    );

    let mut recent: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::RecentArchivedRoundIds)
        .unwrap_or(Vec::new(env));

    recent.push_back(round.round_id);

    let retention_limit: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::ArchiveRetention)
        .unwrap_or(DEFAULT_ARCHIVE_RETENTION);

    while recent.len() > retention_limit {
        if let Some(oldest) = recent.get(0) {
            env.storage()
                .persistent()
                .remove(&DataKey::ArchivedRound(oldest));

            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("archive"), symbol_short!("pruned")),
                (oldest, retention_limit),
            );
        }
        let mut trimmed = Vec::new(env);
        for i in 1..recent.len() {
            if let Some(id) = recent.get(i) {
                trimmed.push_back(id);
            }
        }
        recent = trimmed;
    }

    env.storage()
        .persistent()
        .set(&DataKey::RecentArchivedRoundIds, &recent);
}

pub fn _persist_user_outcome(
    env: &Env,
    round_id: u64,
    round_mode: u32,
    user: &Address,
    prediction_side: u32,
    predicted_price: u128,
    stake: i128,
    payout: i128,
    outcome: UserOutcomeType,
) {
    let key = DataKey::UserRoundOutcome(round_id, user.clone());
    if env.storage().persistent().has(&key) {
        return;
    }
    let record = UserRoundOutcome {
        user: user.clone(),
        round_mode,
        prediction_side,
        predicted_price,
        stake,
        payout,
        outcome,
    };
    env.storage().persistent().set(&key, &record);
    _extend_persistent_ttl(env, &key);
}

pub fn _refund_under_threshold(
    env: &Env,
    round: &Round,
    participants: &Vec<Address>,
) -> Result<(), ContractError> {
    let round_id = round.round_id;
    let round_mode = match round.mode {
        RoundMode::UpDown => 0,
        RoundMode::Precision => 1,
    };
    match round.mode {
        RoundMode::UpDown => {
            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    let pos_key = DataKey::Position(round_id, user.clone());
                    if let Some(pos) =
                        env.storage().persistent().get::<_, UserPosition>(&pos_key)
                    {
                        _accumulate_pending(env, user.clone(), pos.amount)?;
                        let prediction_side = match pos.side {
                            BetSide::Up => 0,
                            BetSide::Down => 1,
                        };
                        _persist_user_outcome(
                            env,
                            round_id,
                            round_mode,
                            &user,
                            prediction_side,
                            0,
                            pos.amount,
                            pos.amount,
                            UserOutcomeType::Refund,
                        );
                    }
                }
            }
        }
        RoundMode::Precision => {
            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    let pred_key = DataKey::PrecisionPosition(round_id, user.clone());
                    if let Some(pred) = env
                        .storage()
                        .persistent()
                        .get::<_, PrecisionPrediction>(&pred_key)
                    {
                        _accumulate_pending(env, user.clone(), pred.amount)?;
                        _persist_user_outcome(
                            env,
                            round_id,
                            round_mode,
                            &user,
                            2,
                            0,
                            pred.amount,
                            pred.amount,
                            UserOutcomeType::Refund,
                        );
                    }
                }
            }
        }
    }
    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            env.storage()
                .persistent()
                .remove(&DataKey::Position(round_id, user.clone()));
            env.storage()
                .persistent()
                .remove(&DataKey::PrecisionPosition(round_id, user));
        }
    }
    env.storage()
        .persistent()
        .remove(&DataKey::RoundParticipants(round_id));
    env.storage().persistent().remove(&DataKey::ActiveRound);
    env.storage().persistent().remove(&DataKey::Positions);
    env.storage().persistent().remove(&DataKey::UpDownPositions);
    env.storage()
        .persistent()
        .remove(&DataKey::PrecisionPositions);
    Ok(())
}

pub fn _update_stats_win(env: &Env, user: Address) -> Result<(), ContractError> {
    let key = DataKey::UserStats(user);
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
    Ok(())
}

pub fn _update_stats_loss(env: &Env, user: Address) -> Result<(), ContractError> {
    let key = DataKey::UserStats(user);
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
    Ok(())
}
