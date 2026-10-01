// SPDX-License-Identifier: MIT
//! Oracle validation, payout calculation, and round resolution.

use super::*;

pub fn resolve_round(env: Env, payload: OraclePayload) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    if payload.price == 0 {
        return Err(ContractError::InvalidPrice);
    }

    _extend_persistent_ttl(&env, &DataKeyCore::Oracle);
    let oracle: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Oracle)
        .ok_or(ContractError::OracleNotSet)?;

    oracle.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &oracle, symbol_short!("resolve"), e);
    })?;

    // Heartbeat health enforcement (Issue #264) — must come before any
    // state mutation (nonce consumption) so a stale oracle cannot race
    // the admin override. An armed one-shot override bypasses the block
    // and is consumed here; the `hoverride` event is published once the
    // round id is known.
    let hb_config = _load_hb_config(&env);
    let hb_blocked = _check_heartbeat_health_blocked(&env, &hb_config);
    let consumed_hb_override = hb_blocked && hb_config.override_armed;
    if hb_blocked && !hb_config.override_armed {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::OracleHeartbeatUnhealthy,
        );
        return Err(ContractError::OracleHeartbeatUnhealthy);
    }

    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
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
            ContractError::OracleNetworkMismatch,
        );
        return Err(ContractError::OracleNetworkMismatch);
    }

    if consumed_hb_override {
        crate::admin::_consume_hb_override(&env);
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("oracle"), symbol_short!("hoverride")),
            (round.round_id,),
        );
    }

    // Verify timestamp is inside the round-relative economic window.
    // This replaces the old absolute-freshness (300 s) check which could
    // accept wrong-phase prices from outside the round's active period.
    // ─── Oracle attestation (Issue #263) ────────────────────────────────────
    //
    // When an attestation key is configured, every payload must carry a
    // detached ed25519 signature over a domain-separated message binding
    // network/contract/round/price/time/nonce — stronger than
    // `oracle.require_auth()` alone across environments where the oracle's
    // Soroban account key and its off-chain signing key may differ (e.g. an
    // HSM-backed signer publishing through a relayer account). When no key
    // is configured, this block is a no-op and behaviour is identical to
    // pre-#263 (account auth only).
    let attestation_config = _load_attestation_config(&env);
    if let Some(pubkey) = attestation_config.key {
        let signature = payload.attestation.clone().ok_or_else(|| {
            _emit_action_rejected(
                &env,
                &oracle,
                symbol_short!("resolve"),
                ContractError::WindowOutOfRange,
            );
            ContractError::WindowOutOfRange
        })?;

        let message = _build_attestation_message(&env, &payload);
        // `ed25519_verify` panics on a bad signature rather than returning a
        // Result, so validity is checked host-side first via a try/catch-free
        // approach: Soroban's crypto host function traps the transaction on
        // failure, which is the correct "fail closed" behaviour for a
        // security check — an invalid signature must never let execution
        // continue past this point.
        env.crypto().ed25519_verify(&pubkey, &message, &signature);
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

    let skew: u64 = env
        .storage()
        .instance()
        .get(&symbol_short!("otskew"))
        .unwrap_or(DEFAULT_ORACLE_TIMESTAMP_SKEW);

    let round_start = round.start_timestamp;
    let round_duration_ledgers = (round.end_ledger)
        .checked_sub(round.start_ledger)
        .ok_or(ContractError::Overflow)?;
    let round_end_estimate = round_start
        .checked_add(
            (round_duration_ledgers as u64)
                .checked_mul(SECONDS_PER_LEDGER)
                .ok_or(ContractError::Overflow)?,
        )
        .ok_or(ContractError::Overflow)?;

    let lower_bound = round_start.saturating_sub(skew);
    let upper_bound = round_end_estimate.saturating_add(skew);

    if payload.timestamp < lower_bound || payload.timestamp > upper_bound {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::OracleTimestampOutsideWindow,
        );
        return Err(ContractError::OracleTimestampOutsideWindow);
    }

    // Oracle deviation guardrails (Issue #266: reference price is either the
    // round's fixed start price, or a trailing-sample TWAP average, per the
    // configured `DeviationReferenceMode`; default is `StartPrice`, matching
    // pre-#266 behaviour exactly when no config has ever been set).
    _extend_persistent_ttl(&env, &DataKeyCore::OracleMaxDeviationBps);
    if let Some(max_bps) = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKeyCore::OracleMaxDeviationBps)
    {
        let deviation_config = _load_deviation_config(&env);
        let reference_price = match deviation_config.reference_mode {
            DeviationReferenceMode::StartPrice => round.price_start,
            DeviationReferenceMode::Twap => {
                _twap_reference_price(&env, deviation_config.window_samples)?
            }
        };
        if reference_price == 0 {
            return Err(ContractError::InvalidPrice);
        }

        let diff = if payload.price >= reference_price {
            payload
                .price
                .checked_sub(reference_price)
                .ok_or(ContractError::Overflow)?
        } else {
            reference_price
                .checked_sub(payload.price)
                .ok_or(ContractError::Overflow)?
        };

        let diff_bps_u128 = diff
            .checked_mul(10_000u128)
            .ok_or(ContractError::Overflow)?
            / reference_price;
        let diff_bps: u32 = diff_bps_u128
            .try_into()
            .map_err(|_| ContractError::Overflow)?;

        let override_armed: bool = env
            .storage()
            .persistent()
            .get(&DataKeyCore::OracleDeviationOverrideArmed)
            .unwrap_or(false);

        if diff_bps > max_bps && !override_armed {
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("oracle"), symbol_short!("rejected")),
                (
                    round.round_id,
                    reference_price,
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
                .remove(&DataKeyCore::OracleDeviationOverrideArmed);

            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("oracle"), symbol_short!("override")),
                (
                    round.round_id,
                    reference_price,
                    payload.price,
                    diff_bps,
                    max_bps,
                ),
            );
        }
    }

    // Oracle confidence guardrails
    _extend_persistent_ttl(&env, &DataKeyCore::OracleMinConfidenceBps);
    _extend_persistent_ttl(&env, &DataKeyCore::OracleStrictMode);
    if let Some(min_confidence_bps) = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKeyCore::OracleMinConfidenceBps)
    {
        match payload.confidence {
            None => {
                let strict_mode: bool = env
                    .storage()
                    .persistent()
                    .get(&DataKeyCore::OracleStrictMode)
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

    let nonce_key = DataKeyScoped::ConsumedOracleNonce(round.round_id, payload.nonce);
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

    // ─── Oracle heartbeat health gate (Issue #264) ──────────────────────────
    //
    // When `HbGateConfig.strict_mode` is enabled, `resolve_round` verifies
    // that the oracle heartbeat is live before allowing settlement.
    let hb_config = crate::admin::_load_hb_config(&env);

    if hb_config.strict_mode && !consumed_hb_override {
        let hb_blocked = _check_heartbeat_health_blocked(&env, &hb_config);

        if hb_blocked {
            if hb_config.override_armed {
                // Consume the one-shot override
                crate::admin::_consume_hb_override(&env);

                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("oracle"), symbol_short!("hoverride")),
                    (round.round_id,),
                );
            } else {
                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("oracle"), symbol_short!("hblocked")),
                    (round.round_id,),
                );
                _emit_action_rejected(
                    &env,
                    &oracle,
                    symbol_short!("resolve"),
                    ContractError::OracleNotLive,
                );
                return Err(ContractError::OracleNotLive);
            }
        }
    }

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

    // Record this validated price into the TWAP sample ring (Issue #266).
    // Runs once the payload has cleared every validity gate above (deviation,
    // confidence, nonce, freshness, heartbeat) regardless of which
    // reference mode is active, so a later switch to `Twap` mode has
    // historical samples to draw from instead of starting empty.
    _record_twap_sample(&env, payload.price, payload.timestamp);

    // Delegate to the shared settlement helper (passes confidence from legacy payload).
    _settle_round_with_price(&env, &round, payload.price, payload.confidence)
}

/// Dispatches settlement after the price has been determined by either oracle path.
pub(super) fn _settle_round_with_price(
    env: &Env,
    round: &Round,
    final_price: u128,
    confidence: Option<u32>,
) -> Result<(), ContractError> {
    let round_id = round.round_id;

    // Minimum participants threshold check
    if let Some(min) = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKeyCore::MinParticipants)
    {
        let threshold_participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKeyScoped::RoundParticipants(round_id))
            .unwrap_or(Vec::new(env));
        let count = threshold_participants.len() as u32;
        if count < min {
            _archive_round(
                env,
                round,
                RoundArchiveStatus::FallbackRefund,
                final_price,
                &threshold_participants,
                0,
                confidence,
            );
            _refund_under_threshold(env, round, &threshold_participants)?;
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("round"), symbol_short!("fallback")),
                (round_id, count, min),
            );
            return Ok(());
        }
    }

    let dispute_ledgers = crate::config::get_dispute_ledgers(env);
    if dispute_ledgers > 0 {
        let resolved_at_ledger = env.ledger().sequence();
        let deadline_ledger = resolved_at_ledger
            .checked_add(dispute_ledgers)
            .ok_or(ContractError::Overflow)?;
        _write_pending_dispute(
            env,
            &PendingDisputeSettlement {
                round: round.clone(),
                final_price,
                confidence,
                resolved_at_ledger,
                deadline_ledger,
            },
        );

        env.storage().persistent().remove(&DataKeyCore::ActiveRound);
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("round"), symbol_short!("pending")),
            (round_id, final_price, resolved_at_ledger, deadline_ledger),
        );
        return Ok(());
    }

    _complete_settlement(env, round, final_price, confidence).map(|_| ())
}

pub(super) fn _complete_settlement(
    env: &Env,
    round: &Round,
    final_price: u128,
    confidence: Option<u32>,
) -> Result<(i128, u32), ContractError> {
    let round_id = round.round_id;
    let fee_amount = match round.mode {
        RoundMode::UpDown => {
            let (one_sided, fee) = _resolve_updown_mode(env, round, final_price, false)?;
            if one_sided {
                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("pool"), symbol_short!("onesided")),
                    (round_id, round.pool_up, round.pool_down),
                );
            }
            fee
        }
        RoundMode::Precision => _resolve_precision_mode(env, round_id, final_price, false)?.0,
    };

    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::RoundParticipants(round_id))
        .unwrap_or(Vec::new(env));
    let participant_count = participants.len() as u32;

    _archive_round(
        env,
        round,
        RoundArchiveStatus::Resolved,
        final_price,
        &participants,
        fee_amount,
        confidence,
    );

    _clear_dispute_round_storage(env, round_id, &participants);

    // Mode-scoped position cleanup (eliminates redundant storage delete lookups)
    match round.mode {
        RoundMode::UpDown => {
            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    env.storage()
                        .persistent()
                        .remove(&DataKeyScoped::Position(round_id, user));
                }
            }
        }
        RoundMode::Precision => {
            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    env.storage()
                        .persistent()
                        .remove(&DataKeyScoped::PrecisionPosition(round_id, user.clone()));
                    env.storage()
                        .persistent()
                        .remove(&DataKeyScoped::PrecisionCommitment(round_id, user));
                }
            }
        }
    }
    env.storage()
        .persistent()
        .remove(&DataKeyScoped::RoundParticipants(round_id));

    env.storage().persistent().remove(&DataKeyCore::ActiveRound);
    env.storage().persistent().remove(&DataKeyCore::Positions);
    env.storage()
        .persistent()
        .remove(&DataKeyCore::UpDownPositions);
    // A merge left this guarded re-remove of the active round split across
    // two fragments (the `if` keyword and its condition were separated from
    // the body). Rejoined here so the file parses again.
    if env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKeyCore::ActiveRound)
        .map(|active| active.round_id == round_id)
        .unwrap_or(false)
    {
        env.storage().persistent().remove(&DataKeyCore::ActiveRound);
    }

    let mode_value: u32 = match round.mode {
        RoundMode::UpDown => 0,
        RoundMode::Precision => 1,
    };
    let policy: u32 = if round.mode == RoundMode::Precision {
        crate::config::get_precision_payout_policy(env.clone())
    } else {
        0
    };
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("round"), symbol_short!("resolved")),
        (round_id, final_price, mode_value, confidence, policy),
    );

    Ok((fee_amount, participant_count))
}

// ─── Internal helpers ────────────────────────────────────────────────────────

/// Permissionlessly voids a resolved round while its dispute window is open.
/// Payouts and fees remain deferred, so each participant receives exactly the
/// stake recorded for this round.

/// Deterministically selects the active one-sided settlement policy for a round.
pub fn _select_one_sided_policy(_round: &Round) -> OneSidedPolicy {
    OneSidedPolicy::Refund
}

/// Applies deterministic one-sided settlement policy for degenerate markets.
pub fn _apply_one_sided_policy(
    env: &Env,
    round: &Round,
    policy: OneSidedPolicy,
    participants: &Vec<Address>,
    positions: &Option<Map<Address, UserPosition>>,
) -> Result<i128, ContractError> {
    let affected_side: u32 = if round.pool_up > 0 {
        0
    } else if round.pool_down > 0 {
        1
    } else {
        2
    };

    let (refund_amount, carry_amount) = match policy {
        OneSidedPolicy::Refund | OneSidedPolicy::Void => {
            if !participants.is_empty() {
                _record_refunds_indexed(env, round.round_id, 0, participants)?;
            } else if let Some(pos_map) = positions {
                #[cfg(any(feature = "legacy-map-settlement", test))]
                {
                    _record_refunds_legacy(env, round.round_id, pos_map)?;
                }
            }
            (round.pool_up.saturating_add(round.pool_down), 0i128)
        }
        OneSidedPolicy::CarryForward => {
            if !participants.is_empty() {
                _record_refunds_indexed(env, round.round_id, 0, participants)?;
            } else if let Some(pos_map) = positions {
                #[cfg(any(feature = "legacy-map-settlement", test))]
                {
                    _record_refunds_legacy(env, round.round_id, pos_map)?;
                }
            }
            (0i128, round.pool_up.saturating_add(round.pool_down))
        }
    };

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("pool"), symbol_short!("onesided")),
        (
            round.round_id,
            policy as u32,
            affected_side,
            refund_amount,
            carry_amount,
            round.pool_up,
            round.pool_down,
        ),
    );

    Ok(0)
}

#[allow(clippy::too_many_arguments)]
pub fn _resolve_updown_mode(
    env: &Env,
    round: &Round,
    final_price: u128,
    _skip_payout: bool,
) -> Result<(bool, i128), ContractError> {
    let raw_participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::RoundParticipants(round.round_id))
        .unwrap_or(Vec::new(env));
    let participants = sort_addresses(raw_participants);

    // Pure price-direction classification and one-sided check delegated to
    // settlement_math for auditability and golden-vector coverage.
    let direction = classify_price_direction(round.price_start, final_price);
    let price_unchanged = direction == PriceDirection::Unchanged;
    let price_went_up = direction == PriceDirection::Up;
    let price_went_down = direction == PriceDirection::Down;

    // One-sided: exactly one pool is empty (XOR).  Regardless of which way
    // price moved, if the winning-side pool is 0 there are no winners to pay,
    // and if the losing-side pool is 0 there is nothing to distribute — in
    // both cases every participant gets a full refund.
    let is_one_sided = is_one_sided_pool(round.pool_up, round.pool_down);

    let mut fee_amount = 0;

    if is_one_sided {
        let policy = _select_one_sided_policy(round);
        let positions: Map<Address, UserPosition> = if participants.is_empty() {
            env.storage()
                .persistent()
                .get(&DataKeyCore::UpDownPositions)
                .unwrap_or(Map::new(env))
        } else {
            Map::new(env)
        };
        fee_amount = _apply_one_sided_policy(
            env,
            round,
            policy,
            &participants,
            &if participants.is_empty() {
                Some(positions)
            } else {
                None
            },
        )?;
    } else if !participants.is_empty() {
        if price_unchanged {
            _record_refunds_indexed(env, round.round_id, 0, &participants)?;
        } else if price_went_up {
            fee_amount = _record_winnings_indexed(
                env,
                round.round_id,
                &participants,
                BetSide::Up,
                round.pool_up,
                round.pool_down,
            )?;
        } else if price_went_down {
            fee_amount = _record_winnings_indexed(
                env,
                round.round_id,
                &participants,
                BetSide::Down,
                round.pool_down,
                round.pool_up,
            )?;
        }
    } else {
        #[cfg(any(feature = "legacy-map-settlement", test))]
        {
            let positions: Map<Address, UserPosition> = env
                .storage()
                .persistent()
                .get(&DataKeyCore::UpDownPositions)
                .unwrap_or(Map::new(env));
            if !positions.is_empty() {
                if price_unchanged {
                    _record_refunds_legacy(env, round.round_id, &positions)?;
                } else if price_went_up {
                    fee_amount = _record_winnings_legacy(
                        env,
                        round.round_id,
                        &positions,
                        BetSide::Up,
                        round.pool_up,
                        round.pool_down,
                    )?;
                } else if price_went_down {
                    fee_amount = _record_winnings_legacy(
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
    }

    Ok((is_one_sided, fee_amount))
}

#[cfg(any(feature = "legacy-map-settlement", test))]
#[deprecated(
    note = "Legacy map settlement is disabled by default and must be removed no later than 2026-12-31; enable the legacy-map-settlement feature only for migration proofs."
)]
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

#[cfg(any(feature = "legacy-map-settlement", test))]
#[deprecated(
    note = "Legacy map settlement is disabled by default and must be removed no later than 2026-12-31; enable the legacy-map-settlement feature only for migration proofs."
)]
pub fn _record_winnings_legacy(
    env: &Env,
    round_id: u64,
    positions: &Map<Address, UserPosition>,
    winning_side: BetSide,
    winning_pool: i128,
    losing_pool: i128,
) -> Result<i128, ContractError> {
    if winning_pool == 0 {
        return Ok(0);
    }

    let original_winning_pool = winning_pool;
    let (dist_winning, dist_losing, fee_amount) =
        _apply_protocol_fee_updown(env, round_id, winning_pool, losing_pool)?;
    // Proportional share of ALL distributable funds (handles fee spillover from winning pool).
    let total_distributable = payout_add(dist_winning, dist_losing)?;

    let keys: Vec<Address> = positions.keys();
    for i in 0..keys.len() {
        if let Some(user) = keys.get(i) {
            if let Some(position) = positions.get(user.clone()) {
                if position.side == winning_side {
                    let payout = compute_updown_winner_payout(
                        position.amount,
                        original_winning_pool,
                        total_distributable,
                    )?;

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

    Ok(fee_amount)
}

fn _calculate_precision_payouts(
    env: &Env,
    winners: &Vec<PrecisionPrediction>,
    payout_pool: i128,
) -> Result<Vec<i128>, ContractError> {
    let policy = crate::config::_read_precision_payout_policy(env);
    let mut payouts = Vec::new(env);
    let mut total_paid = 0i128;

    match policy {
        PrecisionPayoutPolicy::Equal => {
            let winner_count = winners.len() as i128;
            if winner_count > 0 {
                let payout_per_winner = payout_pool / winner_count;
                for _ in 0..winners.len() {
                    payouts.push_back(payout_per_winner);
                    total_paid = payout_add(total_paid, payout_per_winner)?;
                }
            }
        }
        PrecisionPayoutPolicy::StakeWeighted => {
            let mut total_winner_stakes = 0i128;
            for i in 0..winners.len() {
                if let Some(winner) = winners.get(i) {
                    total_winner_stakes = payout_add(total_winner_stakes, winner.amount)?;
                }
            }

            if total_winner_stakes > 0 {
                for i in 0..winners.len() {
                    if let Some(winner) = winners.get(i) {
                        let payout = payout_mul(winner.amount, payout_pool)? / total_winner_stakes;
                        payouts.push_back(payout);
                        total_paid = payout_add(total_paid, payout)?;
                    }
                }
            } else {
                for _ in 0..winners.len() {
                    payouts.push_back(0);
                }
            }
        }
    }

    let remainder = payout_pool
        .checked_sub(total_paid)
        .ok_or(ContractError::PayoutOverflow)?;

    if !winners.is_empty() {
        if let Some(base_payout_0) = payouts.get(0) {
            let payout_0 = payout_add(base_payout_0, remainder)?;
            payouts.set(0, payout_0);
        }
    }

    Ok(payouts)
}

pub fn _resolve_precision_mode(
    env: &Env,
    round_id: u64,
    final_price: u128,
    skip_payout: bool,
) -> Result<(i128, i128), ContractError> {
    let mut participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::RoundParticipants(round_id))
        .unwrap_or(Vec::new(env));
    participants = sort_addresses(participants);

    #[cfg(any(feature = "legacy-map-settlement", test))]
    if participants.is_empty() {
        let legacy: Map<Address, PrecisionPrediction> = env
            .storage()
            .persistent()
            .get(&DataKeyCore::PrecisionPositions)
            .unwrap_or(Map::new(env));
        if legacy.is_empty() {
            return Ok((0, 0));
        }
        return _resolve_precision_legacy(env, round_id, &legacy, final_price);
    }

    #[cfg(not(any(feature = "legacy-map-settlement", test)))]
    if participants.is_empty() {
        return Ok((0, 0));
    }

    let mut min_diff: Option<u128> = None;
    let mut winners: Vec<PrecisionPrediction> = Vec::new(env);
    let mut total_pot: i128 = 0;
    let p_len = participants.len() as usize;
    let mut participant_amounts: StdVec<i128> = StdVec::with_capacity(p_len);
    let mut participant_prices: StdVec<u128> = StdVec::with_capacity(p_len);
    let mut participant_revealed: StdVec<bool> = StdVec::with_capacity(p_len);
    let mut is_winner_mask: StdVec<bool> = StdVec::with_capacity(p_len);

    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            let pred_key = DataKeyScoped::PrecisionPosition(round_id, user.clone());
            let commit_key = DataKeyScoped::PrecisionCommitment(round_id, user.clone());

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
            let revealed = pred_opt.is_some();

            // Precision pot accumulation is payout arithmetic: an overflow here
            // (Issue #405) must surface as `PayoutOverflow`, not a generic
            // `Overflow`, so clients can unambiguously detect a payout failure.
            total_pot = payout_add(total_pot, amount)?;
            participant_amounts.push(amount);
            participant_prices.push(cached_price);
            participant_revealed.push(revealed);
            is_winner_mask.push(false);

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

                let idx = i as usize;
                match min_diff {
                    None => {
                        min_diff = Some(diff);
                        winners.push_back(pred.clone());
                        is_winner_mask[idx] = true;
                    }
                    Some(current_min) => {
                        if diff < current_min {
                            min_diff = Some(diff);
                            winners = Vec::new(env);
                            winners.push_back(pred.clone());
                            for j in 0..idx {
                                is_winner_mask[j] = false;
                            }
                            is_winner_mask[idx] = true;
                        } else if diff == current_min {
                            winners.push_back(pred.clone());
                            is_winner_mask[idx] = true;
                        }
                    }
                }
            }
        }
    }

    let mut fee_amount = 0;
    if !winners.is_empty() && total_pot > 0 {
        // Sum winner stakes for fee-on-winnings calculation.
        // Must propagate overflow error rather than silently truncating.
        let mut winner_stakes: i128 = 0;
        for i in 0..winners.len() {
            if let Some(w) = winners.get(i) {
                // Payout arithmetic (Issue #405): overflow must map to
                // `PayoutOverflow`.
                winner_stakes = payout_add(winner_stakes, w.amount)?;
            }
        }
        let (payout_pool, fee) =
            _apply_protocol_fee_precision(env, round_id, total_pot, winner_stakes)?;
        fee_amount = fee;
        let payouts = _calculate_precision_payouts(env, &winners, payout_pool)?;

        for i in 0..winners.len() {
            if let Some(winner) = winners.get(i) {
                let payout = payouts.get(i).unwrap_or(0);

                if !skip_payout {
                    _accumulate_pending(env, winner.user.clone(), payout)?;
                    _update_stats_win(env, winner.user.clone())?;
                }

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
                let idx = i as usize;
                let was_winner = is_winner_mask.get(idx).copied().unwrap_or(false);
                if !was_winner {
                    let stake = participant_amounts[idx];
                    let predicted_price = participant_prices[idx];

                    if !participant_revealed[idx] {
                        #[allow(deprecated)]
                        env.events().publish(
                            (symbol_short!("forfeit"), symbol_short!("predict")),
                            (user.clone(), round_id, stake),
                        );
                    }

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
    } else if total_pot > 0 {
        // Nobody revealed a prediction (all-commitment round). There is no
        // closest-guess winner to forfeit the pot to, so — unlike the
        // mixed-reveal case — every participant's stake must be refunded in
        // full rather than silently vanishing from circulation (conservation:
        // sum_refunds == total_pot, fee == 0, no stats mutation).
        for i in 0..participants.len() {
            if let Some(user) = participants.get(i) {
                let idx = i as usize;
                let stake = participant_amounts.get(idx).copied().unwrap_or(0);
                if stake > 0 {
                    _accumulate_pending(env, user.clone(), stake)?;
                    _persist_user_outcome(
                        env,
                        round_id,
                        1,
                        &user,
                        2,
                        0,
                        stake,
                        stake,
                        UserOutcomeType::Refund,
                    );
                }
            }
        }
    }

    Ok((fee_amount, total_pot))
}

#[cfg(any(feature = "legacy-map-settlement", test))]
#[deprecated(
    note = "Legacy map settlement is disabled by default and must be removed no later than 2026-12-31; enable the legacy-map-settlement feature only for migration proofs."
)]
pub fn _resolve_precision_legacy(
    env: &Env,
    round_id: u64,
    predictions_map: &Map<Address, PrecisionPrediction>,
    final_price: u128,
) -> Result<(i128, i128), ContractError> {
    let predictions = predictions_map.values();
    if predictions.is_empty() {
        return Ok((0, 0));
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

    let mut fee_amount = 0;
    if !winners.is_empty() && total_pot > 0 {
        // Sum winner stakes for fee-on-winnings calculation.
        // Must propagate overflow error rather than silently truncating.
        let mut winner_stakes: i128 = 0;
        for i in 0..winners.len() {
            if let Some(w) = winners.get(i) {
                // Payout arithmetic (Issue #405): overflow must map to
                // `PayoutOverflow`.
                winner_stakes = payout_add(winner_stakes, w.amount)?;
            }
        }
        let (payout_pool, fee) =
            _apply_protocol_fee_precision(env, round_id, total_pot, winner_stakes)?;
        fee_amount = fee;
        let payouts = _calculate_precision_payouts(env, &winners, payout_pool)?;

        for i in 0..winners.len() {
            if let Some(winner) = winners.get(i) {
                let payout = payouts.get(i).unwrap_or(0);
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

    Ok((fee_amount, total_pot))
}

pub fn _record_refunds_indexed(
    env: &Env,
    round_id: u64,
    round_mode: u32,
    participants: &Vec<Address>,
) -> Result<(), ContractError> {
    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            let pos_key = DataKeyScoped::Position(round_id, user.clone());
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
) -> Result<i128, ContractError> {
    if winning_pool == 0 {
        return Ok(0);
    }

    let original_winning_pool = winning_pool;
    let (dist_winning, dist_losing, fee_amount) =
        _apply_protocol_fee_updown(env, round_id, winning_pool, losing_pool)?;
    // Proportional share of ALL distributable funds (handles fee spillover from winning pool).
    let total_distributable = payout_add(dist_winning, dist_losing)?;

    for i in 0..participants.len() {
        if let Some(user) = participants.get(i) {
            let pos_key = DataKeyScoped::Position(round_id, user.clone());
            if let Some(position) = env.storage().persistent().get::<_, UserPosition>(&pos_key) {
                if position.side == winning_side {
                    let payout = compute_updown_winner_payout(
                        position.amount,
                        original_winning_pool,
                        total_distributable,
                    )?;

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

    Ok(fee_amount)
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
                    let pos_key = DataKeyScoped::Position(round_id, user.clone());
                    if let Some(pos) = env.storage().persistent().get::<_, UserPosition>(&pos_key) {
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
                    let pred_key = DataKeyScoped::PrecisionPosition(round_id, user.clone());
                    let commit_key = DataKeyScoped::PrecisionCommitment(round_id, user.clone());
                    let mut refund_amount = 0i128;
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
                        // Unrevealed commitments must be refunded on the
                        // insufficient-participants fallback path (same
                        // conservation rule as cancel_round).
                        refund_amount = commit.amount;
                    }
                    if refund_amount > 0 {
                        _accumulate_pending(env, user.clone(), refund_amount)?;
                        _persist_user_outcome(
                            env,
                            round_id,
                            round_mode,
                            &user,
                            2,
                            0,
                            refund_amount,
                            refund_amount,
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
    env.storage().persistent().remove(&DataKeyCore::ActiveRound);
    env.storage().persistent().remove(&DataKeyCore::Positions);
    env.storage()
        .persistent()
        .remove(&DataKeyCore::UpDownPositions);
    env.storage()
        .persistent()
        .remove(&DataKeyCore::PrecisionPositions);
    Ok(())
}

/// Domain-separation prefix for oracle attestation messages (Issue #263).
/// Ensures an attestation signature can never be replayed against another
/// message type that happens to XDR-encode to the same bytes (e.g. a
/// different contract's signed struct), independent of the on-chain
/// network_id/contract_addr equality checks performed separately.
const ATTESTATION_DOMAIN_PREFIX: &[u8] = b"XELMA_ORACLE_ATTESTATION_V1";

/// Builds the canonical message an oracle operator signs off-chain
/// (Issue #263): a fixed domain prefix followed by the XDR encoding of
/// every field that binds this payload to a specific network, contract,
/// round, price, timestamp, and nonce. Verified on-chain via
/// `env.crypto().ed25519_verify()` against the configured attestation key.
///
/// Deliberately excludes `confidence` and `attestation` itself — the former
/// is advisory metadata, the latter is the signature being verified.
pub fn _build_attestation_message(env: &Env, payload: &OraclePayload) -> Bytes {
    let mut message = Bytes::from_slice(env, ATTESTATION_DOMAIN_PREFIX);
    message.append(&payload.network_id.clone().into());
    message.append(&payload.contract_addr.clone().to_xdr(env));
    message.append(&payload.round_id.to_xdr(env));
    message.append(&payload.price.to_xdr(env));
    message.append(&payload.timestamp.to_xdr(env));
    message.append(&payload.nonce.to_xdr(env));
    message
}

/// Computes the TWAP reference price from the last `window_samples` recorded
/// settlement prices (Issue #266). Simple arithmetic mean — samples are
/// recorded once per settled round (not on a continuous clock), so a
/// duration-weighted average would weight every sample equally anyway.
///
/// Returns `InsufficientTwapSamples` if fewer than `window_samples` have
/// been recorded yet, so an admin can't silently settle against a thin or
/// empty window early in a deployment's life.
pub fn _twap_reference_price(env: &Env, window_samples: u32) -> Result<u128, ContractError> {
    let samples = _load_twap_samples(env);
    if samples.len() < window_samples {
        return Err(ContractError::WindowOutOfRange);
    }

    let start = samples.len() - window_samples;
    let mut sum: u128 = 0;
    let mut count: u128 = 0;
    for i in start..samples.len() {
        if let Some(sample) = samples.get(i) {
            sum = sum
                .checked_add(sample.price)
                .ok_or(ContractError::Overflow)?;
            count += 1;
        }
    }
    if count == 0 {
        return Err(ContractError::WindowOutOfRange);
    }
    Ok(sum / count)
}

/// Loads the bounded ring of recent settlement price samples (Issue #266).
pub fn _load_twap_samples(env: &Env) -> Vec<PriceSample> {
    let key = TwapSamplesKey::Samples;
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env))
}

/// Appends a settled price to the TWAP sample ring, evicting the oldest
/// entry once the ring exceeds `MAX_TWAP_WINDOW_SAMPLES` (Issue #266).
pub fn _record_twap_sample(env: &Env, price: u128, timestamp: u64) {
    let key = TwapSamplesKey::Samples;
    let mut samples = _load_twap_samples(env);
    samples.push_back(PriceSample { price, timestamp });
    while samples.len() > crate::common::MAX_TWAP_WINDOW_SAMPLES {
        samples.remove(0);
    }
    env.storage().persistent().set(&key, &samples);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
}

/// Returns `true` if the oracle heartbeat health gate should block settlement (Issue #264).
///
/// Checks:
/// 1. No heartbeat recorded → blocked
/// 2. Status = 2 (offline) → blocked
/// 3. Heartbeat stale beyond (threshold + grace) → blocked
/// 4. Otherwise → not blocked (allowed)
pub fn _check_heartbeat_health_blocked(env: &Env, config: &HbGateConfig) -> bool {
    use crate::common::DEFAULT_ORACLE_STALE_THRESHOLD;

    let heartbeat_key = DataKeyCore::OracleHeartbeat;
    _extend_persistent_ttl(env, &heartbeat_key);
    let record: OracleHeartbeatRecord = match env.storage().persistent().get(&heartbeat_key) {
        Some(r) => r,
        None => return true, // No heartbeat → blocked
    };

    // Offline status always blocks
    if record.status == 2 {
        return true;
    }

    // Check staleness with grace period
    let threshold_key = DataKeyCore::OracleStaleThreshold;
    _extend_persistent_ttl(env, &threshold_key);
    let threshold: u64 = env
        .storage()
        .persistent()
        .get(&threshold_key)
        .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD);

    let grace: u64 = config.grace_seconds;

    let current_time = env.ledger().timestamp();
    let deadline = record
        .timestamp
        .saturating_add(threshold)
        .saturating_add(grace);

    // Blocked if past the stale threshold + grace period
    current_time > deadline
}
