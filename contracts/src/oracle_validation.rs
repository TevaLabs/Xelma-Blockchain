// SPDX-License-Identifier: MIT
//! Oracle Validation Module (Issue #511)
//!
//! Single-owner module for all oracle payload validation helpers.
//! Extracted from `settlement.rs` so that both `resolve_round` and
//! `resolve_round_multi` call a shared, auditable set of functions rather
//! than maintaining duplicate inline logic.
//!
//! ## Responsibilities
//! - Attestation message construction and domain-binding (`_build_attestation_message`)
//! - Heartbeat health gate (`_check_heartbeat_health_blocked`)
//! - TWAP sample ring management (`_load_twap_samples`, `_record_twap_sample`,
//!   `_twap_reference_price`)
//!
//! ## Not responsible for
//! - Oracle config setters/getters (live in `admin.rs`)
//! - Full settlement dispatch (live in `settlement.rs`)

use crate::common::{
    _extend_persistent_ttl, DEFAULT_ORACLE_STALE_THRESHOLD, MAX_TWAP_WINDOW_SAMPLES,
    TTL_BUMP_AMOUNT, TTL_BUMP_THRESHOLD,
};
use crate::errors::ContractError;
use crate::types::{
    DataKeyCore, HbGateConfig, OracleHeartbeatRecord, OraclePayload, PriceSample, TwapSamplesKey,
};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Bytes, Env, Vec};

// ─── Attestation ─────────────────────────────────────────────────────────────

/// Domain-separation prefix for oracle attestation messages (Issue #263).
///
/// Ensures an attestation signature can never be replayed against another
/// message type that happens to XDR-encode to the same bytes (e.g. a
/// different contract's signed struct), independent of the on-chain
/// `network_id`/`contract_addr` equality checks performed separately.
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

// ─── Heartbeat health gate ────────────────────────────────────────────────────

/// Returns `true` if the oracle heartbeat health gate should block settlement
/// (Issue #264).
///
/// Checks (in order):
/// 1. No heartbeat recorded → blocked.
/// 2. `record.status == 2` (offline) → blocked.
/// 3. Current time exceeds `record.timestamp + stale_threshold + grace_seconds` → blocked.
/// 4. Otherwise → not blocked (settlement allowed).
pub fn _check_heartbeat_health_blocked(env: &Env, config: &HbGateConfig) -> bool {
    let heartbeat_key = DataKeyCore::OracleHeartbeat;
    _extend_persistent_ttl(env, &heartbeat_key);
    let record: OracleHeartbeatRecord = match env.storage().persistent().get(&heartbeat_key) {
        Some(r) => r,
        None => return true, // No heartbeat on record → blocked
    };

    // Offline status always blocks regardless of timestamp.
    if record.status == 2 {
        return true;
    }

    // Check staleness: stale_threshold + grace_seconds must not have elapsed.
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

    // Blocked if the current time is strictly past the deadline.
    current_time > deadline
}

// ─── TWAP sample ring ─────────────────────────────────────────────────────────

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

/// Appends a settled price to the TWAP sample ring, evicting the oldest entry
/// once the ring exceeds [`MAX_TWAP_WINDOW_SAMPLES`] (Issue #266).
pub fn _record_twap_sample(env: &Env, price: u128, timestamp: u64) {
    let key = TwapSamplesKey::Samples;
    let mut samples = _load_twap_samples(env);
    samples.push_back(PriceSample { price, timestamp });
    while samples.len() > MAX_TWAP_WINDOW_SAMPLES {
        samples.remove(0);
    }
    env.storage().persistent().set(&key, &samples);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
}

/// Computes the TWAP reference price from the last `window_samples` recorded
/// settlement prices (Issue #266).
///
/// Uses a simple arithmetic mean — samples are recorded once per settled round,
/// not on a continuous clock, so duration-weighted averaging would weight every
/// sample equally anyway.
///
/// Returns [`ContractError::WindowOutOfRange`] if fewer than `window_samples`
/// have been recorded yet, preventing silent settlement against a thin or empty
/// window early in a deployment's life.
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

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HbGateConfig;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::Env;

    fn make_env() -> Env {
        Env::default()
    }

    // ── _check_heartbeat_health_blocked ──────────────────────────────────

    #[test]
    fn heartbeat_blocked_when_no_record() {
        let env = make_env();
        let config = HbGateConfig {
            strict_mode: false,
            override_armed: false,
            grace_seconds: 0,
        };
        // No heartbeat written → must be blocked.
        assert!(_check_heartbeat_health_blocked(&env, &config));
    }

    #[test]
    fn heartbeat_blocked_when_offline() {
        let env = make_env();
        env.ledger().set_timestamp(1_000);
        env.storage().persistent().set(
            &DataKeyCore::OracleHeartbeat,
            &OracleHeartbeatRecord {
                timestamp: 999,
                status: 2, // offline
            },
        );
        let config = HbGateConfig {
            strict_mode: false,
            override_armed: false,
            grace_seconds: 0,
        };
        assert!(_check_heartbeat_health_blocked(&env, &config));
    }

    #[test]
    fn heartbeat_not_blocked_when_live() {
        let env = make_env();
        let now: u64 = 1_000;
        env.ledger().set_timestamp(now);
        env.storage().persistent().set(
            &DataKeyCore::OracleHeartbeat,
            &OracleHeartbeatRecord {
                timestamp: now - 10, // fresh, well within threshold
                status: 1,           // online
            },
        );
        // stale threshold defaults to 3600 s — 10 s old is well within range.
        let config = HbGateConfig {
            strict_mode: false,
            override_armed: false,
            grace_seconds: 0,
        };
        assert!(!_check_heartbeat_health_blocked(&env, &config));
    }

    #[test]
    fn heartbeat_blocked_when_stale_past_grace() {
        let env = make_env();
        let threshold = DEFAULT_ORACLE_STALE_THRESHOLD; // 3600
        let grace = 60u64;
        let now: u64 = 10_000;
        env.ledger().set_timestamp(now);
        // Heartbeat at now - threshold - grace - 1 → deadline already passed.
        let ts = now - threshold - grace - 1;
        env.storage().persistent().set(
            &DataKeyCore::OracleHeartbeat,
            &OracleHeartbeatRecord {
                timestamp: ts,
                status: 1,
            },
        );
        let config = HbGateConfig {
            strict_mode: false,
            override_armed: false,
            grace_seconds: grace,
        };
        assert!(_check_heartbeat_health_blocked(&env, &config));
    }

    // ── TWAP helpers ─────────────────────────────────────────────────────

    #[test]
    fn twap_insufficient_samples_returns_error() {
        let env = make_env();
        // No samples stored yet → requesting any window should fail.
        let result = _twap_reference_price(&env, 2);
        assert_eq!(result, Err(ContractError::WindowOutOfRange));
    }

    #[test]
    fn twap_single_sample_mean() {
        let env = make_env();
        _record_twap_sample(&env, 1_000, 100);
        // Window of 1 → mean of [1_000] = 1_000.
        assert_eq!(_twap_reference_price(&env, 1), Ok(1_000));
    }

    #[test]
    fn twap_three_samples_mean() {
        let env = make_env();
        _record_twap_sample(&env, 100, 1);
        _record_twap_sample(&env, 200, 2);
        _record_twap_sample(&env, 300, 3);
        // Mean of [100, 200, 300] = 200.
        assert_eq!(_twap_reference_price(&env, 3), Ok(200));
    }

    #[test]
    fn twap_window_slides_to_latest() {
        let env = make_env();
        _record_twap_sample(&env, 100, 1);
        _record_twap_sample(&env, 200, 2);
        _record_twap_sample(&env, 300, 3);
        // Window of 2 uses last 2 samples: [200, 300] → mean = 250.
        assert_eq!(_twap_reference_price(&env, 2), Ok(250));
    }
}
