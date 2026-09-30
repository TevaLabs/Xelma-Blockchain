// SPDX-License-Identifier: MIT
//! Multi-feed oracle validation and settlement entry point.

use super::*;

/// Resolves the active round using a multi-feed oracle payload.
///
/// This is the **preferred** settlement path when `OracleQuorumConfig` is set.
/// It carries N independent feed observations, computes the median price,
/// rejects outliers beyond the configured threshold, and requires a quorum
/// of agreeing feeds before settlement proceeds.
///
/// The legacy single-oracle `resolve_round` path remains available and
/// unaffected by this function.
pub fn resolve_round_multi(env: Env, payload: MultiFeedPayload) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;

    // ── Basic payload validation ──────────────────────────────────────────
    if payload.prices.is_empty() || payload.sources.is_empty() {
        return Err(ContractError::TooFewObservations);
    }
    if payload.prices.len() != payload.sources.len() {
        return Err(ContractError::TooFewObservations);
    }

    // All prices must be non-zero
    let n = payload.prices.len() as u32;
    for i in 0..n {
        if let Some(price) = payload.prices.get(i) {
            if price == 0 {
                return Err(ContractError::InvalidPrice);
            }
        }
    }

    // ── Auth and pause check ──────────────────────────────────────────────
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

    // ── Load quorum config ────────────────────────────────────────────────
    _extend_persistent_ttl(&env, &DataKeyCore::OracleQuorum);
    let quorum_cfg: OracleQuorumConfig = env
        .storage()
        .persistent()
        .get(&DataKeyCore::OracleQuorum)
        .ok_or(ContractError::OracleNotSet)?;

    // ── Load active round ─────────────────────────────────────────────────
    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    // ── Verify round ID ───────────────────────────────────────────────────
    if payload.round_id != round.start_ledger {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::InvalidOracleRound,
        );
        return Err(ContractError::InvalidOracleRound);
    }

    // ── Cross-network / cross-contract replay protection ──────────────────
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

    // ── Timestamp window check (round-relative economic window) ───────────
    let current_time = env.ledger().timestamp();
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

    // ── Oracle heartbeat health gate (parity with single-oracle) ─────────
    let hb_config = crate::admin::_load_hb_config(&env);
    if hb_config.strict_mode {
        let hb_blocked = _check_heartbeat_health_blocked(&env, &hb_config);
        if hb_blocked {
            if hb_config.override_armed {
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

    // ── Nonce replay protection ───────────────────────────────────────────
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

    // ── Verify round end ledger ───────────────────────────────────────────
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

    // ── Check min_observations and max cap ────────────────────────────────
    if n < quorum_cfg.min_observations {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::TooFewObservations,
        );
        return Err(ContractError::TooFewObservations);
    }
    // Reject excessive observations to prevent gas abuse from O(N²) sort
    if n > MAX_ORACLE_OBSERVATIONS {
        _emit_action_rejected(
            &env,
            &oracle,
            symbol_short!("resolve"),
            ContractError::TooFewObservations,
        );
        return Err(ContractError::TooFewObservations);
    }

    // ── Check for duplicate source identifiers ────────────────────────────
    for i in 0..n {
        if let Some(src_i) = payload.sources.get(i) {
            for j in (i + 1)..n {
                if let Some(src_j) = payload.sources.get(j) {
                    if src_i == src_j {
                        _emit_action_rejected(
                            &env,
                            &oracle,
                            symbol_short!("resolve"),
                            ContractError::DuplicateOracleSource,
                        );
                        return Err(ContractError::DuplicateOracleSource);
                    }
                }
            }
        }
    }

    // ── Sort prices (insertion sort) and compute median ───────────────────
    let mut sorted_prices: Vec<u128> = Vec::new(&env);
    for i in 0..n {
        if let Some(price) = payload.prices.get(i) {
            let mut inserted = false;
            for j in 0..sorted_prices.len() {
                if price < sorted_prices.get(j).ok_or(ContractError::Overflow)? {
                    sorted_prices.insert(j, price);
                    inserted = true;
                    break;
                }
            }
            if !inserted {
                sorted_prices.push_back(price);
            }
        }
    }

    // Compute median
    let median_price: u128 = if n % 2 == 1 {
        sorted_prices.get(n / 2).ok_or(ContractError::Overflow)?
    } else {
        let mid1 = sorted_prices
            .get(n / 2 - 1)
            .ok_or(ContractError::Overflow)?;
        let mid2 = sorted_prices.get(n / 2).ok_or(ContractError::Overflow)?;
        mid1.checked_add(mid2).ok_or(ContractError::Overflow)? / 2
    };

    if median_price == 0 {
        return Err(ContractError::InvalidPrice);
    }

    // ── Deviation guardrail check against round start price ───────────────
    // The multi-feed path still respects the configured max deviation from
    // the round's start price. This prevents the oracle from using multi-feed
    // to bypass the single-feed deviation guardrail.
    _extend_persistent_ttl(&env, &DataKeyCore::OracleMaxDeviationBps);
    if let Some(max_bps) = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKeyCore::OracleMaxDeviationBps)
    {
        let start_price = round.price_start;
        if start_price == 0 {
            return Err(ContractError::InvalidPrice);
        }
        let diff = if median_price >= start_price {
            median_price
                .checked_sub(start_price)
                .ok_or(ContractError::Overflow)?
        } else {
            start_price
                .checked_sub(median_price)
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
            .get(&DataKeyCore::OracleDeviationOverrideArmed)
            .unwrap_or(false);

        if diff_bps > max_bps && !override_armed {
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("oracle"), symbol_short!("rejected")),
                (round.round_id, start_price, median_price, diff_bps, max_bps),
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
                (round.round_id, start_price, median_price, diff_bps, max_bps),
            );
        }
    }

    // ── Outlier rejection & quorum check ──────────────────────────────────
    let mut survivors: u32 = 0;
    for i in 0..n {
        if let Some(price) = payload.prices.get(i) {
            let diff = if price >= median_price {
                price
                    .checked_sub(median_price)
                    .ok_or(ContractError::Overflow)?
            } else {
                median_price
                    .checked_sub(price)
                    .ok_or(ContractError::Overflow)?
            };

            // diff_bps = diff * 10000 / median_price
            let diff_bps_u128 = diff
                .checked_mul(10_000u128)
                .ok_or(ContractError::Overflow)?
                / median_price;
            let diff_bps: u32 = diff_bps_u128
                .try_into()
                .map_err(|_| ContractError::Overflow)?;

            if diff_bps <= quorum_cfg.outlier_threshold_bps {
                survivors = survivors.checked_add(1).ok_or(ContractError::Overflow)?;
            }
        }
    }

    if survivors < quorum_cfg.quorum_threshold {
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("oracle"), symbol_short!("nofed")),
            (
                round.round_id,
                median_price,
                survivors,
                quorum_cfg.quorum_threshold,
            ),
        );
        return Err(ContractError::InsufficientOracleQuorum);
    }

    // ── Emit multi-feed summary event ─────────────────────────────────────
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("oracle"), symbol_short!("multisum")),
        (
            round.round_id,
            n,
            survivors,
            median_price,
            quorum_cfg.quorum_threshold,
        ),
    );

    // Record this validated price into the TWAP sample ring (Issue #266).
    _record_twap_sample(&env, median_price, payload.timestamp);

    // ── Settle the round using the computed median price ──────────────────
    super::resolve::_settle_round_with_price(&env, &round, median_price, None)
}
