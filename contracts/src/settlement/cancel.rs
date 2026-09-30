// SPDX-License-Identifier: MIT
//! Active-round cancellation, voiding, and dispute finalization.

use super::*;

/// Cancels the active round and deterministically refunds all participant stakes.
pub fn cancel_round(env: Env, reason: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();

    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
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
        .get(&DataKeyScoped::RoundParticipants(round_id))
        .unwrap_or(Vec::new(&env));

    // ─── Insurance coverage (Issue #367) ──────────────────────────────────
    // Collect per-participant stakes for insurance coverage calculation.
    // Coverage is distributed AFTER refunds as an additional bonus.
    let eligible = crate::insurance::is_coverage_eligible(&env, reason);
    let mut participant_stakes: Vec<i128> = Vec::new(&env);
    let mut total_stake: i128 = 0;

    match round.mode {
        RoundMode::UpDown => {
            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    let pos_key = DataKeyScoped::Position(round_id, user.clone());
                    if let Some(pos) = env.storage().persistent().get::<_, UserPosition>(&pos_key) {
                        participant_stakes.push_back(pos.amount);
                        total_stake = total_stake.checked_add(pos.amount).unwrap_or(total_stake);
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
                            UserOutcomeType::Void,
                        );
                    }
                }
            }
        }
        RoundMode::Precision => {
            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    let pred_key = DataKeyScoped::PrecisionPosition(round_id, user.clone());
                    let commit_key = DataKeyScoped::PrecisionCommitment(round_id, user.clone());

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

                    participant_stakes.push_back(refund_amount);
                    total_stake = total_stake
                        .checked_add(refund_amount)
                        .unwrap_or(total_stake);

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
                        UserOutcomeType::Void,
                    );
                }
            }
        }
    }

    // ─── Insurance coverage payout (Issue #367) ──────────────────────────
    // If the cancel reason is in the eligible-event whitelist and the
    // insurance fund has balance, distribute coverage as additional
    // pending winnings to each participant.
    if eligible && !participant_stakes.is_empty() {
        let mut total_coverage: i128 = 0;
        for i in 0..participant_stakes.len() {
            if let Some(stake) = participant_stakes.get(i) {
                let cov = crate::insurance::calculate_coverage_amount(&env, stake)?;
                total_coverage = payout_add(total_coverage, cov)?;
            }
        }
        if total_coverage > 0 {
            let distributed =
                crate::insurance::deduct_insurance_coverage(&env, round_id, total_coverage)?;
            // Distribute coverage proportionally to participants
            if distributed > 0 && total_stake > 0 {
                for i in 0..participants.len() {
                    if let Some(user) = participants.get(i) {
                        let stake = participant_stakes.get(i).unwrap_or(0);
                        let coverage = if stake > 0 {
                            distributed
                                .checked_mul(stake)
                                .ok_or(ContractError::Overflow)?
                                .checked_div(total_stake)
                                .ok_or(ContractError::Overflow)?
                        } else {
                            0
                        };
                        if coverage > 0 {
                            _accumulate_pending(&env, user, coverage)?;
                        }
                    }
                }
            }
        }
    }

    // Canonical cleanup: removes ALL position keys + shared keys + legacy keys
    clear_round_storage(&env, round_id, &participants);

    // Clean up participant list and mark round as cancelled.
    _archive_round(
        &env,
        &round,
        RoundArchiveStatus::Cancelled,
        0,
        &participants,
        0,
        None,
    );

    env.storage()
        .persistent()
        .remove(&DataKeyScoped::RoundParticipants(round_id));
    env.storage()
        .persistent()
        .set(&DataKeyScoped::CancelledRound(round_id), &true);
    env.storage().persistent().remove(&DataKeyCore::ActiveRound);

    Ok(())
}

/// Returns true if the given round_id was cancelled.
pub fn is_round_cancelled(env: Env, round_id: u64) -> bool {
    env.storage()
        .persistent()
        .get(&DataKeyScoped::CancelledRound(round_id))
        .unwrap_or(false)
}

pub fn void_round(env: Env, round_id: u64) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _ensure_not_paused(&env)?;

    let pending =
        _read_pending_dispute(&env, round_id).ok_or(ContractError::DisputeWindowExpired)?;
    if env.ledger().sequence() >= pending.deadline_ledger {
        return Err(ContractError::DisputeWindowExpired);
    }

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::RoundParticipants(round_id))
        .unwrap_or(Vec::new(&env));
    let mut total_refund = 0i128;

    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            match pending.round.mode {
                RoundMode::UpDown => {
                    let key = DataKeyScoped::Position(round_id, user.clone());
                    if let Some(position) = env.storage().persistent().get::<_, UserPosition>(&key)
                    {
                        total_refund = payout_add(total_refund, position.amount)?;
                        _accumulate_pending(&env, user.clone(), position.amount)?;
                        let side = match position.side {
                            BetSide::Up => 0,
                            BetSide::Down => 1,
                        };
                        _persist_user_outcome(
                            &env,
                            round_id,
                            0,
                            &user,
                            side,
                            0,
                            position.amount,
                            position.amount,
                            UserOutcomeType::Void,
                        );
                    }
                }
                RoundMode::Precision => {
                    let prediction_key = DataKeyScoped::PrecisionPosition(round_id, user.clone());
                    let commitment_key = DataKeyScoped::PrecisionCommitment(round_id, user.clone());
                    let (stake, predicted_price) = if let Some(prediction) = env
                        .storage()
                        .persistent()
                        .get::<_, PrecisionPrediction>(&prediction_key)
                    {
                        (prediction.amount, prediction.predicted_price)
                    } else if let Some(commitment) = env
                        .storage()
                        .persistent()
                        .get::<_, PrecisionCommitment>(&commitment_key)
                    {
                        (commitment.amount, 0)
                    } else {
                        (0, 0)
                    };
                    if stake > 0 {
                        total_refund = payout_add(total_refund, stake)?;
                        _accumulate_pending(&env, user.clone(), stake)?;
                        _persist_user_outcome(
                            &env,
                            round_id,
                            1,
                            &user,
                            2,
                            predicted_price,
                            stake,
                            stake,
                            UserOutcomeType::Void,
                        );
                    }
                }
            }
        }
    }

    _archive_round(
        &env,
        &pending.round,
        RoundArchiveStatus::Voided,
        pending.final_price,
        &participants,
        0,
        pending.confidence,
    );
    _clear_dispute_round_storage(&env, round_id, &participants);
    _remove_pending_dispute(&env, round_id);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("round"), symbol_short!("voided")),
        (round_id, participants.len() as u32, total_refund),
    );
    Ok(())
}

/// Permissionlessly finalizes the staged oracle result once the dispute window
/// has closed. Calling at the exact deadline is allowed.
pub fn finalize_round(env: Env, round_id: u64) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _ensure_not_paused(&env)?;

    let pending = _read_pending_dispute(&env, round_id).ok_or(ContractError::NoActiveRound)?;
    if env.ledger().sequence() < pending.deadline_ledger {
        return Err(ContractError::ClaimLocked);
    }

    let (fee_amount, participant_count) = super::resolve::_complete_settlement(
        &env,
        &pending.round,
        pending.final_price,
        pending.confidence,
    )?;
    _remove_pending_dispute(&env, round_id);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("round"), symbol_short!("finalized")),
        (round_id, pending.final_price, participant_count, fee_amount),
    );
    Ok(())
}
