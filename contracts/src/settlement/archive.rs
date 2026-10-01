// SPDX-License-Identifier: MIT
//! Round archive persistence and user outcome records.

use super::*;

pub fn _archive_round(
    env: &Env,
    round: &Round,
    status: RoundArchiveStatus,
    final_price: u128,
    participants: &Vec<Address>,
    fee_amount: i128,
    confidence: Option<u32>,
) {
    let status_val = status.clone() as u32;
    let participant_count = participants.len() as u32;
    let settled_at_ledger = env.ledger().sequence();
    let summary = ArchivedRoundSummary {
        round_id: round.round_id,
        price_start: round.price_start,
        price_final: final_price,
        mode: round.mode.clone(),
        status,
        pool_up: round.pool_up,
        pool_down: round.pool_down,
        participant_count,
        settled_at_ledger,
    };

    // Record per-user participation index for paginated history queries.
    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            let index_key = DataKeyScoped::UserArchivedRoundIds(user.clone());
            let mut user_rounds: Vec<u64> = env
                .storage()
                .persistent()
                .get(&index_key)
                .unwrap_or(Vec::new(env));
            user_rounds.push_back(round.round_id);
            env.storage().persistent().set(&index_key, &user_rounds);
        }
    }

    env.storage()
        .persistent()
        .set(&DataKeyScoped::ArchivedRound(round.round_id), &summary);

    let mut total_pot: i128 = 0;
    match round.mode {
        RoundMode::UpDown => {
            total_pot = total_pot_updown(round.pool_up, round.pool_down);
        }
        RoundMode::Precision => {
            let participants: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKeyScoped::RoundParticipants(round.round_id))
                .unwrap_or(Vec::new(env));
            #[cfg(any(feature = "legacy-map-settlement", test))]
            if participants.is_empty() {
                let legacy: Map<Address, PrecisionPrediction> = env
                    .storage()
                    .persistent()
                    .get(&DataKeyCore::PrecisionPositions)
                    .unwrap_or(Map::new(env));
                for entry in legacy.iter() {
                    total_pot = total_pot.checked_add(entry.1.amount).unwrap_or(total_pot);
                }
            } else {
                for i in 0..participants.len() {
                    if let Some(user) = participants.get(i) {
                        let pred_key =
                            DataKeyScoped::PrecisionPosition(round.round_id, user.clone());
                        let commit_key =
                            DataKeyScoped::PrecisionCommitment(round.round_id, user.clone());

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
            #[cfg(not(any(feature = "legacy-map-settlement", test)))]
            {
                for i in 0..participants.len() {
                    if let Some(user) = participants.get(i) {
                        let pred_key =
                            DataKeyScoped::PrecisionPosition(round.round_id, user.clone());
                        let commit_key =
                            DataKeyScoped::PrecisionCommitment(round.round_id, user.clone());

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

    let fee_model_value: u32 = _read_fee_model(env) as u32;

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("round"), symbol_short!("summary")),
        (
            round.round_id,
            status_val,
            round.mode.clone() as u32,
            final_price,
            participant_count,
            total_pot,
            fee_amount,
            settled_at_ledger,
            confidence.unwrap_or(0u32),
            fee_model_value,
        ),
    );

    let mut recent: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKeyCore::RecentArchivedRoundIds)
        .unwrap_or(Vec::new(env));

    recent.push_back(round.round_id);

    let retention_limit: u32 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ArchiveRetention)
        .unwrap_or(DEFAULT_ARCHIVE_RETENTION);

    while recent.len() > retention_limit {
        if let Some(oldest) = recent.get(0) {
            env.storage()
                .persistent()
                .remove(&DataKeyScoped::ArchivedRound(oldest));

            // Clean up associated markers so the prune is complete:
            // cancelled-round flag, if present.
            if env
                .storage()
                .persistent()
                .has(&DataKeyScoped::CancelledRound(oldest))
            {
                env.storage()
                    .persistent()
                    .remove(&DataKeyScoped::CancelledRound(oldest));
            }

            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("archive"), symbol_short!("pruned")),
                (oldest, retention_limit),
            );
            // Drop exactly the id that was just pruned. (A second
            // `remove(0)` here used to discard the next id without deleting
            // its `ArchivedRound`, orphaning it and keeping only N-1 rounds.)
            recent.remove(0);
        } else {
            break;
        }
    }

    env.storage()
        .persistent()
        .set(&DataKeyCore::RecentArchivedRoundIds, &recent);
}

#[allow(clippy::too_many_arguments)]
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
    let key = DataKeyScoped::UserRoundOutcome(round_id, user.clone());
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
        outcome: outcome.clone(),
    };
    env.storage().persistent().set(&key, &record);
    _extend_persistent_ttl(env, &key);

    let outcome_type_u32 = outcome.clone() as u32;
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("payout"), symbol_short!("outcome")),
        (round_id, round_mode, user.clone(), payout, outcome_type_u32),
    );
}
