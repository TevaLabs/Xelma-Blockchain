// SPDX-License-Identifier: MIT
extern crate alloc;

use crate::common::{
    _emit_action_rejected, _extend_persistent_ttl, DEFAULT_ORACLE_QUORUM_MIN_OBSERVATIONS,
    DEFAULT_ORACLE_QUORUM_THRESHOLD, DEFAULT_ORACLE_STALE_THRESHOLD, DEFAULT_ORACLE_TIMESTAMP_SKEW,
    MAX_ORACLE_OBSERVATIONS, MIN_TWAP_WINDOW_SAMPLES, SECONDS_PER_LEDGER, TTL_BUMP_AMOUNT,
    TTL_BUMP_THRESHOLD,
};
use crate::errors::ContractError;
use crate::types::{
    AttestationConfig, AttestationConfigKey, DataKeyCore, DataKeyScoped, DeviationConfig,
    DeviationConfigKey, DeviationReferenceMode, HbGateConfig, HbGateKey, MultiFeedPayload,
    OracleHeartbeatRecord, OraclePayload, OracleQuorumConfig, PriceSample, Round, TwapSamplesKey,
};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, Vec};

const ATTESTATION_DOMAIN_PREFIX: &[u8] = b"XELMA_ORACLE_ATTESTATION_V1";

// ─── Attestation & Domain Binding ─────────────────────────────────────────────

/// Loads the attestation config, returning `key: None` (disabled) if unset (Issue #263).
pub fn _load_attestation_config(env: &Env) -> AttestationConfig {
    let key = AttestationConfigKey::Config;
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(AttestationConfig { key: None })
}

/// Builds the domain-separated attestation message for signed oracle payloads (Issue #263).
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

/// Validates network and contract domain binding for oracle payloads.
pub fn validate_domain_binding(
    env: &Env,
    oracle: &Address,
    payload_round_id: u32,
    round_start_ledger: u32,
    payload_network_id: &BytesN<32>,
    payload_contract_addr: &Address,
) -> Result<(), ContractError> {
    if payload_round_id != round_start_ledger {
        _emit_action_rejected(
            env,
            oracle,
            symbol_short!("resolve"),
            ContractError::InvalidOracleRound,
        );
        return Err(ContractError::InvalidOracleRound);
    }
    if *payload_network_id != env.ledger().network_id() {
        _emit_action_rejected(
            env,
            oracle,
            symbol_short!("resolve"),
            ContractError::OracleNetworkMismatch,
        );
        return Err(ContractError::OracleNetworkMismatch);
    }
    if *payload_contract_addr != env.current_contract_address() {
        _emit_action_rejected(
            env,
            oracle,
            symbol_short!("resolve"),
            ContractError::OracleNetworkMismatch,
        );
        return Err(ContractError::OracleNetworkMismatch);
    }
    Ok(())
}

/// Verifies oracle attestation signature if an attestation key is configured (Issue #263).
pub fn validate_attestation(
    env: &Env,
    oracle: &Address,
    payload: &OraclePayload,
) -> Result<(), ContractError> {
    let attestation_config = _load_attestation_config(env);
    if let Some(pubkey) = attestation_config.key {
        let signature = payload.attestation.clone().ok_or_else(|| {
            _emit_action_rejected(
                env,
                oracle,
                symbol_short!("resolve"),
                ContractError::WindowOutOfRange,
            );
            ContractError::WindowOutOfRange
        })?;

        let message = _build_attestation_message(env, payload);
        env.crypto().ed25519_verify(&pubkey, &message, &signature);
    }
    Ok(())
}

// ─── Heartbeat Health & Gate ──────────────────────────────────────────────────

/// Loads the heartbeat gate config, returning defaults if unset.
pub fn _load_hb_config(env: &Env) -> HbGateConfig {
    let key = HbGateKey::Config;
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(HbGateConfig {
            strict_mode: false,
            override_armed: false,
            grace_seconds: 0,
        })
}

/// Saves the heartbeat gate config to persistent storage.
pub fn _save_hb_config(env: &Env, config: &HbGateConfig) {
    let key = HbGateKey::Config;
    env.storage().persistent().set(&key, config);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
}

/// Consumes the heartbeat override if armed (called from settlement).
/// Returns true if the override was consumed.
pub fn _consume_hb_override(env: &Env) -> bool {
    let config = _load_hb_config(env);
    if config.override_armed {
        let mut new_config = config.clone();
        new_config.override_armed = false;
        _save_hb_config(env, &new_config);
        true
    } else {
        false
    }
}

/// Returns `true` if the oracle heartbeat health gate should block settlement (Issue #264).
pub fn _check_heartbeat_health_blocked(env: &Env, config: &HbGateConfig) -> bool {
    let heartbeat_key = DataKeyCore::OracleHeartbeat;
    _extend_persistent_ttl(env, &heartbeat_key);
    let record: OracleHeartbeatRecord = match env.storage().persistent().get(&heartbeat_key) {
        Some(r) => r,
        None => return true, // No heartbeat → blocked
    };

    if record.status == 2 {
        return true;
    }

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

    current_time > deadline
}

/// Pre-flight heartbeat health check before state mutation (Issue #264).
pub fn validate_heartbeat_pre_flight(env: &Env, oracle: &Address) -> Result<(), ContractError> {
    let hb_config = _load_hb_config(env);
    if _check_heartbeat_health_blocked(env, &hb_config) {
        _emit_action_rejected(
            env,
            oracle,
            symbol_short!("resolve"),
            ContractError::OracleHeartbeatUnhealthy,
        );
        return Err(ContractError::OracleHeartbeatUnhealthy);
    }
    Ok(())
}

/// Heartbeat strict mode gate check (Issue #264).
pub fn validate_heartbeat_strict_gate(
    env: &Env,
    oracle: &Address,
    round_id: u64,
) -> Result<(), ContractError> {
    let hb_config = _load_hb_config(env);
    if hb_config.strict_mode {
        let hb_blocked = _check_heartbeat_health_blocked(env, &hb_config);

        if hb_blocked {
            if hb_config.override_armed {
                _consume_hb_override(env);

                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("oracle"), symbol_short!("hoverride")),
                    (round_id,),
                );
            } else {
                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("oracle"), symbol_short!("hblocked")),
                    (round_id,),
                );
                _emit_action_rejected(
                    env,
                    oracle,
                    symbol_short!("resolve"),
                    ContractError::OracleNotLive,
                );
                return Err(ContractError::OracleNotLive);
            }
        }
    }
    Ok(())
}

/// Records an oracle heartbeat (oracle only).
pub fn update_oracle_heartbeat(env: Env, status: u32) -> Result<(), ContractError> {
    crate::admin::_require_supported_schema(&env)?;
    if status > 2 {
        _extend_persistent_ttl(&env, &DataKeyCore::Oracle);
        if let Some(oracle) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKeyCore::Oracle)
        {
            _emit_action_rejected(
                &env,
                &oracle,
                symbol_short!("hbeat"),
                ContractError::InvalidMode,
            );
        }
        return Err(ContractError::InvalidMode);
    }
    _extend_persistent_ttl(&env, &DataKeyCore::Oracle);
    let oracle: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Oracle)
        .ok_or(ContractError::OracleNotSet)?;
    oracle.require_auth();

    let ts = env.ledger().timestamp();
    let record = OracleHeartbeatRecord {
        timestamp: ts,
        status,
    };
    env.storage()
        .persistent()
        .set(&DataKeyCore::OracleHeartbeat, &record);
    _extend_persistent_ttl(&env, &DataKeyCore::OracleHeartbeat);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("oracle"), symbol_short!("hbeat")),
        (ts, status),
    );
    Ok(())
}

/// Returns the most recent oracle heartbeat record, if any.
pub fn get_oracle_heartbeat(env: Env) -> Option<OracleHeartbeatRecord> {
    let key = DataKeyCore::OracleHeartbeat;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Returns `true` if the oracle has a non-stale heartbeat with status not offline (2).
pub fn is_oracle_live(env: Env) -> bool {
    let heartbeat_key = DataKeyCore::OracleHeartbeat;
    _extend_persistent_ttl(&env, &heartbeat_key);
    let record: OracleHeartbeatRecord = match env.storage().persistent().get(&heartbeat_key) {
        Some(r) => r,
        None => return false,
    };
    if record.status == 2 {
        return false;
    }
    let threshold_key = DataKeyCore::OracleStaleThreshold;
    _extend_persistent_ttl(&env, &threshold_key);
    let threshold: u64 = env
        .storage()
        .persistent()
        .get(&threshold_key)
        .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD);
    let current_time = env.ledger().timestamp();
    current_time <= record.timestamp.saturating_add(threshold)
}

// ─── Deviation & TWAP Reference Calculation ───────────────────────────────────

/// Loads the deviation guardrail config, returning the `StartPrice` default if unset (Issue #266).
pub fn _load_deviation_config(env: &Env) -> DeviationConfig {
    let key = DeviationConfigKey::Config;
    if env.storage().persistent().has(&key) {
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DeviationConfig {
            reference_mode: DeviationReferenceMode::StartPrice,
            window_samples: MIN_TWAP_WINDOW_SAMPLES,
        })
}

/// Saves deviation config to persistent storage.
pub fn _save_deviation_config(env: &Env, config: &DeviationConfig) {
    let key = DeviationConfigKey::Config;
    env.storage().persistent().set(&key, config);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
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

/// Appends a settled price to the TWAP sample ring (Issue #266).
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

/// Computes the TWAP reference price from the last `window_samples` recorded settlement prices (Issue #266).
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

/// Validates price deviation against configured max bps and reference price (Issue #266).
pub fn validate_deviation(env: &Env, round: &Round, price: u128) -> Result<(), ContractError> {
    _extend_persistent_ttl(env, &DataKeyCore::OracleMaxDeviationBps);
    if let Some(max_bps) = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKeyCore::OracleMaxDeviationBps)
    {
        let deviation_config = _load_deviation_config(env);
        let reference_price = match deviation_config.reference_mode {
            DeviationReferenceMode::StartPrice => round.price_start,
            DeviationReferenceMode::Twap => {
                _twap_reference_price(env, deviation_config.window_samples)?
            }
        };
        if reference_price == 0 {
            return Err(ContractError::InvalidPrice);
        }

        let diff = if price >= reference_price {
            price
                .checked_sub(reference_price)
                .ok_or(ContractError::Overflow)?
        } else {
            reference_price
                .checked_sub(price)
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
                (round.round_id, reference_price, price, diff_bps, max_bps),
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
                (round.round_id, reference_price, price, diff_bps, max_bps),
            );
        }
    }
    Ok(())
}

// ─── Timestamp Freshness & Confidence ─────────────────────────────────────────

/// Validates timestamp freshness within the round's economic window.
pub fn validate_timestamp_freshness(
    env: &Env,
    oracle: &Address,
    round: &Round,
    timestamp: u64,
) -> Result<(), ContractError> {
    let current_time = env.ledger().timestamp();

    if timestamp > current_time {
        _emit_action_rejected(
            env,
            oracle,
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

    if timestamp < lower_bound || timestamp > upper_bound {
        _emit_action_rejected(
            env,
            oracle,
            symbol_short!("resolve"),
            ContractError::OracleTimestampOutsideWindow,
        );
        return Err(ContractError::OracleTimestampOutsideWindow);
    }

    Ok(())
}

/// Validates confidence score against minimum configured confidence and strict mode.
pub fn validate_confidence(
    env: &Env,
    round_id: u64,
    confidence: Option<u32>,
) -> Result<(), ContractError> {
    _extend_persistent_ttl(env, &DataKeyCore::OracleMinConfidenceBps);
    _extend_persistent_ttl(env, &DataKeyCore::OracleStrictMode);
    if let Some(min_confidence_bps) = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKeyCore::OracleMinConfidenceBps)
    {
        match confidence {
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
                        (round_id, confidence_bps, min_confidence_bps),
                    );
                    return Err(ContractError::InvalidPrice);
                }
            }
        }
    }
    Ok(())
}

// ─── Nonce & Replay Protection ────────────────────────────────────────────────

/// Validates and records oracle nonce consumption for replay protection.
pub fn validate_and_consume_nonce(
    env: &Env,
    oracle: &Address,
    round_id: u64,
    nonce: u64,
) -> Result<(), ContractError> {
    let nonce_key = DataKeyScoped::ConsumedOracleNonce(round_id, nonce);
    if env.storage().persistent().has(&nonce_key) {
        _emit_action_rejected(
            env,
            oracle,
            symbol_short!("resolve"),
            ContractError::OracleNonceReused,
        );
        return Err(ContractError::OracleNonceReused);
    }
    env.storage().persistent().set(&nonce_key, &true);
    Ok(())
}

// ─── Multi-Feed Consensus & Quorum ────────────────────────────────────────────

/// Validates the quorum config values are within allowed bounds.
pub fn _validate_quorum_config(cfg: &OracleQuorumConfig) -> Result<(), ContractError> {
    if cfg.min_observations < DEFAULT_ORACLE_QUORUM_MIN_OBSERVATIONS
        || cfg.min_observations > MAX_ORACLE_OBSERVATIONS
    {
        return Err(ContractError::TooFewObservations);
    }
    if cfg.quorum_threshold < DEFAULT_ORACLE_QUORUM_THRESHOLD
        || cfg.quorum_threshold > cfg.min_observations
    {
        return Err(ContractError::InsufficientOracleQuorum);
    }
    if cfg.outlier_threshold_bps == 0 || cfg.outlier_threshold_bps > 10_000 {
        return Err(ContractError::WindowOutOfRange);
    }
    Ok(())
}

/// Processes multi-feed consensus: validates bounds, duplicate sources, calculates median price,
/// checks start price deviation, and enforces quorum outlier filtering.
pub fn process_multi_feed_consensus(
    env: &Env,
    oracle: &Address,
    round: &Round,
    payload: &MultiFeedPayload,
    quorum_cfg: &OracleQuorumConfig,
) -> Result<u128, ContractError> {
    let n = payload.prices.len();

    if n < quorum_cfg.min_observations {
        _emit_action_rejected(
            env,
            oracle,
            symbol_short!("resolve"),
            ContractError::TooFewObservations,
        );
        return Err(ContractError::TooFewObservations);
    }
    if n > MAX_ORACLE_OBSERVATIONS {
        _emit_action_rejected(
            env,
            oracle,
            symbol_short!("resolve"),
            ContractError::TooFewObservations,
        );
        return Err(ContractError::TooFewObservations);
    }

    // Check for duplicate source identifiers
    for i in 0..n {
        if let Some(src_i) = payload.sources.get(i) {
            for j in (i + 1)..n {
                if let Some(src_j) = payload.sources.get(j) {
                    if src_i == src_j {
                        _emit_action_rejected(
                            env,
                            oracle,
                            symbol_short!("resolve"),
                            ContractError::DuplicateOracleSource,
                        );
                        return Err(ContractError::DuplicateOracleSource);
                    }
                }
            }
        }
    }

    // Sort prices (insertion sort) and compute median
    let mut sorted_prices: Vec<u128> = Vec::new(env);
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

    // Deviation guardrail check against round start price for multi-feed median
    _extend_persistent_ttl(env, &DataKeyCore::OracleMaxDeviationBps);
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

    // Outlier rejection & quorum check
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

    // Emit multi-feed summary event
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

    Ok(median_price)
}

// ─── Isolated Unit Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RoundMode, SymbolString};
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{symbol_short, Address, Env};

    fn dummy_oracle(env: &Env) -> Address {
        Address::generate(env)
    }

    fn dummy_round(env: &Env, start_ledger: u32, end_ledger: u32) -> Round {
        Round {
            round_id: start_ledger as u64,
            start_ledger,
            end_ledger,
            price_start: 1_000_0000,
            price_end: 0,
            total_up: 0,
            total_down: 0,
            pool_up: 0,
            pool_down: 0,
            winner: 0,
            start_timestamp: env.ledger().timestamp(),
            mode: RoundMode::UpDown,
            strike_price: None,
            precision_payout_policy: None,
            archived: false,
        }
    }

    #[test]
    fn test_isolated_heartbeat_health_blocked() {
        let env = Env::default();
        let config = HbGateConfig {
            strict_mode: true,
            override_armed: false,
            grace_seconds: 0,
        };

        // No heartbeat record -> blocked
        assert!(_check_heartbeat_health_blocked(&env, &config));

        // Record heartbeat status 2 (offline) -> blocked
        let ts = env.ledger().timestamp();
        let record = OracleHeartbeatRecord {
            timestamp: ts,
            status: 2,
        };
        env.storage()
            .persistent()
            .set(&DataKeyCore::OracleHeartbeat, &record);
        assert!(_check_heartbeat_health_blocked(&env, &config));

        // Record live heartbeat status 0 -> not blocked
        let record = OracleHeartbeatRecord {
            timestamp: ts,
            status: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKeyCore::OracleHeartbeat, &record);
        assert!(!_check_heartbeat_health_blocked(&env, &config));
    }

    #[test]
    fn test_isolated_domain_binding() {
        let env = Env::default();
        let oracle = dummy_oracle(&env);
        let valid_net = env.ledger().network_id();
        let valid_contract = env.current_contract_address();
        let invalid_contract = dummy_oracle(&env);

        // Valid domain binding -> Ok
        assert!(validate_domain_binding(&env, &valid_net, &valid_contract, &oracle).is_ok());

        // Mismatched contract address -> Err(OracleNetworkMismatch)
        assert_eq!(
            validate_domain_binding(&env, &valid_net, &invalid_contract, &oracle),
            Err(ContractError::OracleNetworkMismatch)
        );
    }

    #[test]
    fn test_isolated_nonce_validation() {
        let env = Env::default();
        let oracle = dummy_oracle(&env);
        let round_id = 100u64;
        let nonce = 12345u64;

        // First consume -> Ok
        assert!(validate_and_consume_nonce(&env, round_id, nonce, &oracle).is_ok());

        // Reused nonce -> Err(OracleNonceReused)
        assert_eq!(
            validate_and_consume_nonce(&env, round_id, nonce, &oracle),
            Err(ContractError::OracleNonceReused)
        );
    }

    #[test]
    fn test_isolated_timestamp_freshness() {
        let env = Env::default();
        let oracle = dummy_oracle(&env);
        let round = dummy_round(&env, 10, 20);

        // Future timestamp -> Err(FutureOracleData)
        let future_ts = env.ledger().timestamp() + 100;
        assert_eq!(
            validate_timestamp_freshness(&env, future_ts, &round, &oracle),
            Err(ContractError::FutureOracleData)
        );

        // Current valid timestamp -> Ok
        let current_ts = env.ledger().timestamp();
        assert!(validate_timestamp_freshness(&env, current_ts, &round, &oracle).is_ok());
    }

    #[test]
    fn test_isolated_quorum_config_validation() {
        let valid_cfg = OracleQuorumConfig {
            min_observations: 3,
            quorum_threshold: 2,
            outlier_threshold_bps: 500,
        };
        assert!(_validate_quorum_config(&valid_cfg).is_ok());

        let invalid_min = OracleQuorumConfig {
            min_observations: 1,
            quorum_threshold: 2,
            outlier_threshold_bps: 500,
        };
        assert_eq!(
            _validate_quorum_config(&invalid_min),
            Err(ContractError::TooFewObservations)
        );
    }

    #[test]
    fn test_isolated_multi_feed_consensus_duplicate_sources() {
        let env = Env::default();
        let oracle = dummy_oracle(&env);
        let round = dummy_round(&env, 10, 20);
        let quorum_cfg = OracleQuorumConfig {
            min_observations: 2,
            quorum_threshold: 2,
            outlier_threshold_bps: 500,
        };

        let mut prices = Vec::new(&env);
        prices.push_back(1_000_0000);
        prices.push_back(1_010_0000);

        let mut sources = Vec::new(&env);
        let src = SymbolString {
            symbol: symbol_short!("binance"),
        };
        sources.push_back(src.clone());
        sources.push_back(src); // Duplicate source

        let payload = MultiFeedPayload {
            prices,
            sources,
            timestamp: env.ledger().timestamp(),
            round_id: 10,
            nonce: 1,
            network_id: env.ledger().network_id(),
            contract_addr: env.current_contract_address(),
        };

        assert_eq!(
            process_multi_feed_consensus(&env, &payload, &quorum_cfg, &round, &oracle),
            Err(ContractError::DuplicateOracleSource)
        );
    }
}
