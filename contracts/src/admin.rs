// SPDX-License-Identifier: MIT
use crate::common::{
    _derive_round_phase, _emit_action_rejected, _extend_persistent_ttl, _set_balance, balance,
    payout_add, CURRENT_SCHEMA_VERSION, DEFAULT_BET_WINDOW_LEDGERS,
    DEFAULT_ORACLE_STALE_THRESHOLD, DEFAULT_RUN_WINDOW_LEDGERS, MAX_TWAP_WINDOW_SAMPLES,
    MIN_TWAP_WINDOW_SAMPLES, TTL_BUMP_AMOUNT, TTL_BUMP_THRESHOLD,
};
use crate::errors::ContractError;
use crate::types::{
    AttestationConfig, AttestationConfigKey, DataKey, DataKeyCore, DataKeyScoped,
    DeviationConfig, DeviationConfigKey, DeviationReferenceMode, HbGateConfig, HbGateKey,
    OracleHeartbeatRecord, OracleQuorumConfig, PolicyAction, ProtocolHealthStatus, Round,
    RuntimeMode, PENDING_WINNINGS_EXPIRY_KEY, PendingWinningsUpdatedAtKey,
};
use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol, Vec};

/// Initializes the contract with admin and oracle addresses (one-time only)
pub fn initialize(env: Env, admin: Address, oracle: Address) -> Result<(), ContractError> {
    admin.require_auth();

    if admin == oracle {
        return Err(ContractError::OracleNetworkMismatch);
    }

    if env.storage().persistent().has(&DataKeyCore::Admin) {
        return Err(ContractError::AlreadyInitialized);
    }

    env.storage().persistent().set(&DataKeyCore::Admin, &admin);
    env.storage().persistent().set(&DataKeyCore::Oracle, &oracle);
    env.storage()
        .persistent()
        .set(&DataKeyCore::Paused, &RuntimeMode::Normal);
    env.storage()
        .persistent()
        .set(&DataKeyCore::SchemaVersion, &CURRENT_SCHEMA_VERSION);

    // Set default window values
    env.storage()
        .persistent()
        .set(&DataKeyCore::BetWindowLedgers, &DEFAULT_BET_WINDOW_LEDGERS);
    env.storage()
        .persistent()
        .set(&DataKeyCore::RunWindowLedgers, &DEFAULT_RUN_WINDOW_LEDGERS);

    _extend_persistent_ttl(&env, &DataKeyCore::Admin);
    _extend_persistent_ttl(&env, &DataKeyCore::Oracle);
    _extend_persistent_ttl(&env, &DataKeyCore::Paused);
    _extend_persistent_ttl(&env, &DataKeyCore::SchemaVersion);
    _extend_persistent_ttl(&env, &DataKeyCore::BetWindowLedgers);
    _extend_persistent_ttl(&env, &DataKeyCore::RunWindowLedgers);

    Ok(())
}

/// Returns the stored schema version. If unset, returns legacy version 1.
pub fn get_schema_version(env: Env) -> u32 {
    let key = DataKeyCore::SchemaVersion;
    _extend_persistent_ttl(&env, &key);
    _schema_version(&env).unwrap_or(1)
}

/// Migrates legacy schema version 1 → version 2 (admin only).
///
/// When `dry_run` is `true`, all validation checks are performed but no storage
/// writes or events are emitted. This lets operators verify that a migration
/// would succeed before committing to it.
pub fn migrate_schema_v1_to_v2(env: Env, dry_run: bool) -> Result<(), ContractError> {
    let admin_key = DataKeyCore::Admin;
    _extend_persistent_ttl(&env, &admin_key);
    let admin: Address = env
        .storage()
        .persistent()
        .get(&admin_key)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("migrate"), e);
    })?;

    if env.storage().persistent().has(&DataKeyCore::ActiveRound) {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("migrate"),
            ContractError::MigrationActiveRound,
        );
        return Err(ContractError::MigrationActiveRound);
    }

    let from = _schema_version(&env).unwrap_or(1);
    const TARGET_VERSION: u32 = 2;
    if from != 1 {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("migrate"),
            ContractError::UnsupportedSchemaVersion,
        );
        return Err(ContractError::UnsupportedSchemaVersion);
    }

    if dry_run {
        return Ok(());
    }

    let schema_key = DataKeyCore::SchemaVersion;
    env.storage().persistent().set(&schema_key, &TARGET_VERSION);
    _extend_persistent_ttl(&env, &schema_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("schema"), symbol_short!("migrated")),
        (from, TARGET_VERSION),
    );

    Ok(())
}

/// Migrates schema version 2 → version 3 (admin only).
///
/// When `dry_run` is `true`, all validation checks are performed but no storage
/// writes or events are emitted. This lets operators verify that a migration
/// would succeed before committing to it.
pub fn migrate_schema_v2_to_v3(env: Env, dry_run: bool) -> Result<(), ContractError> {
    let admin_key = DataKeyCore::Admin;
    _extend_persistent_ttl(&env, &admin_key);
    let admin: Address = env
        .storage()
        .persistent()
        .get(&admin_key)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("migrate"), e);
    })?;

    if env.storage().persistent().has(&DataKeyCore::ActiveRound) {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("migrate"),
            ContractError::MigrationActiveRound,
        );
        return Err(ContractError::MigrationActiveRound);
    }

    let from = _schema_version(&env).unwrap_or(1);
    const TARGET_VERSION: u32 = 3;
    if from != 2 {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("migrate"),
            ContractError::UnsupportedSchemaVersion,
        );
        return Err(ContractError::UnsupportedSchemaVersion);
    }

    if dry_run {
        return Ok(());
    }

    let schema_key = DataKeyCore::SchemaVersion;
    env.storage().persistent().set(&schema_key, &TARGET_VERSION);
    _extend_persistent_ttl(&env, &schema_key);

    env.storage()
        .persistent()
        .set(&DataKeyCore::MigratedToV3, &true);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("schema"), symbol_short!("migrated")),
        (from, TARGET_VERSION),
    );

    Ok(())
}

/// Announces a target schema version for the next planned migration (admin only).
///
/// This sets a "v-next schema template" that operators can inspect before the
/// real migration executes. It is purely informational — it does NOT change
/// the active schema or gate any entrypoints. Use `get_next_schema` to read
/// the announced version and `clear_next_schema` to unset it.
pub fn announce_next_schema(env: Env, target_version: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();

    if target_version == 0 || target_version <= CURRENT_SCHEMA_VERSION {
        return Err(ContractError::UnsupportedSchemaVersion);
    }

    let key = DataKeyCore::NextSchemaVersion;
    env.storage().persistent().set(&key, &target_version);
    _extend_persistent_ttl(&env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("schema"), symbol_short!("next_ann")),
        (CURRENT_SCHEMA_VERSION, target_version),
    );

    Ok(())
}

/// Returns the announced next schema version, if any.
pub fn get_next_schema(env: Env) -> Option<u32> {
    let key = DataKeyCore::NextSchemaVersion;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Clears a previously announced next schema version (admin only).
pub fn clear_next_schema(env: Env) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();

    let key = DataKeyCore::NextSchemaVersion;
    if !env.storage().persistent().has(&key) {
        return Err(ContractError::UnsupportedSchemaVersion);
    }
    let previous: u32 = env.storage().persistent().get(&key).unwrap();
    env.storage().persistent().remove(&key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("schema"), symbol_short!("next_clr")),
        (previous,),
    );

    Ok(())
}

/// Returns whether the contract is currently paused
pub fn is_paused(env: Env) -> bool {
    let key = DataKeyCore::Paused;
    _extend_persistent_ttl(&env, &key);
    let mode = env
        .storage()
        .persistent()
        .get::<_, RuntimeMode>(&key)
        .unwrap_or(RuntimeMode::Normal);
    mode == RuntimeMode::FullyPaused
}

/// Pauses the contract for emergency recovery (admin only)
pub fn pause_contract(env: Env) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;

    admin.require_auth();
    if crate::governance::_is_gov_approver_set(&env) {
        return Err(ContractError::GovUnauthorized);
    }
    _set_mode(&env, RuntimeMode::FullyPaused)?;

    Ok(())
}

/// Unpauses the contract after recovery (admin only)
pub fn unpause_contract(env: Env) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;

    admin.require_auth();
    if crate::governance::_is_gov_approver_set(&env) {
        return Err(ContractError::GovUnauthorized);
    }
    _set_mode(&env, RuntimeMode::Normal)?;

    Ok(())
}

/// Returns the current runtime mode (0 = Normal, 1 = ClaimsOnly, 2 = FullyPaused)
pub fn get_runtime_mode(env: Env) -> u32 {
    let key = DataKeyCore::Paused;
    _extend_persistent_ttl(&env, &key);
    let mode = env
        .storage()
        .persistent()
        .get::<_, RuntimeMode>(&key)
        .unwrap_or(RuntimeMode::Normal);
    mode as u32
}

/// Sets the runtime mode of the contract (admin only)
pub fn set_runtime_mode(env: Env, mode: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;

    admin.require_auth();

    let new_mode = match mode {
        0 => RuntimeMode::Normal,
        1 => RuntimeMode::ClaimsOnly,
        2 => RuntimeMode::FullyPaused,
        _ => return Err(ContractError::InvalidMode),
    };

    _set_mode(&env, new_mode)?;

    Ok(())
}

pub fn get_admin(env: Env) -> Option<Address> {
    let key = DataKeyCore::Admin;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

pub fn get_oracle(env: Env) -> Option<Address> {
    let key = DataKeyCore::Oracle;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Schedules a timelocked oracle deviation update
pub fn set_oracle_max_deviation_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
    crate::config::schedule_oracle_deviation_bps(env, bps)
}

/// Returns the configured oracle max deviation bps, if set.
pub fn get_oracle_max_deviation_bps(env: Env) -> Option<u32> {
    let key = DataKeyCore::OracleMaxDeviationBps;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Arms a one-shot override to bypass deviation checks for the next settlement (admin only).
pub fn arm_oracle_deviation_override(env: Env) -> Result<(), ContractError> {
    let admin_key = DataKeyCore::Admin;
    _extend_persistent_ttl(&env, &admin_key);
    let admin: Address = env
        .storage()
        .persistent()
        .get(&admin_key)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("arm_ovr"), e);
    })?;

    let override_key = DataKeyCore::OracleDeviationOverrideArmed;
    env.storage().persistent().set(&override_key, &true);
    _extend_persistent_ttl(&env, &override_key);
    Ok(())
}

/// Loads the deviation guardrail config, returning the `StartPrice` default if unset (Issue #266).
pub fn _load_deviation_config(env: &Env) -> DeviationConfig {
    let key = DeviationConfigKey::Config;
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            TTL_BUMP_THRESHOLD,
            TTL_BUMP_AMOUNT,
        );
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DeviationConfig {
            reference_mode: DeviationReferenceMode::StartPrice,
            window_samples: MIN_TWAP_WINDOW_SAMPLES,
        })
}

fn _save_deviation_config(env: &Env, config: &DeviationConfig) {
    let key = DeviationConfigKey::Config;
    env.storage().persistent().set(&key, config);
    env.storage().persistent().extend_ttl(
        &key,
        TTL_BUMP_THRESHOLD,
        TTL_BUMP_AMOUNT,
    );
}

/// Sets the oracle deviation reference mode and (for `Twap`) the trailing
/// sample window size (admin only, Issue #266). Defaults to `StartPrice`
/// with no config stored, preserving pre-#266 behaviour exactly.
pub fn set_deviation_ref_mode(
    env: Env,
    mode: DeviationReferenceMode,
    window_samples: u32,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("devref"), e);
    })?;

    if mode == DeviationReferenceMode::Twap
        && !(MIN_TWAP_WINDOW_SAMPLES..=MAX_TWAP_WINDOW_SAMPLES).contains(&window_samples)
    {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("devref"),
            ContractError::WindowOutOfRange,
        );
        return Err(ContractError::WindowOutOfRange);
    }

    _save_deviation_config(
        &env,
        &DeviationConfig {
            reference_mode: mode,
            window_samples,
        },
    );

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("oracle"), symbol_short!("devref")),
        (mode as u32, window_samples),
    );

    Ok(())
}

/// Returns the configured deviation reference mode (default `StartPrice`, Issue #266).
pub fn get_deviation_ref_mode(env: Env) -> DeviationReferenceMode {
    _load_deviation_config(&env).reference_mode
}

/// Returns the configured TWAP window size in samples (default `MIN_TWAP_WINDOW_SAMPLES`, Issue #266).
pub fn get_deviation_window_samples(env: Env) -> u32 {
    _load_deviation_config(&env).window_samples
}

/// Loads the attestation config, returning `key: None` (disabled) if unset (Issue #263).
pub fn _load_attestation_config(env: &Env) -> AttestationConfig {
    let key = AttestationConfigKey::Config;
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            TTL_BUMP_THRESHOLD,
            TTL_BUMP_AMOUNT,
        );
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(AttestationConfig { key: None })
}

/// Sets (or clears) the ed25519 public key used to verify oracle attestation
/// signatures (admin only, Issue #263). Passing `None` disables attestation
/// verification entirely, restoring pre-#263 behaviour (account auth only).
pub fn set_attestation_key(env: Env, key: Option<BytesN<32>>) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("attkey"), e);
    })?;

    let storage_key = AttestationConfigKey::Config;
    env.storage()
        .persistent()
        .set(&storage_key, &AttestationConfig { key: key.clone() });
    env.storage().persistent().extend_ttl(
        &storage_key,
        TTL_BUMP_THRESHOLD,
        TTL_BUMP_AMOUNT,
    );

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("oracle"), symbol_short!("attkey")),
        (key.is_some(),),
    );

    Ok(())
}

/// Returns the configured attestation signing key, if attestation is enabled (Issue #263).
pub fn get_attestation_key(env: Env) -> Option<BytesN<32>> {
    _load_attestation_config(&env).key
}

/// Sets the minimum oracle confidence threshold in basis points (admin only).
pub fn set_oracle_min_confidence_bps(env: Env, min_bps: Option<u32>) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    if let Some(bps) = min_bps {
        if bps > 10_000 {
            return Err(ContractError::WindowOutOfRange);
        }
    }
    match min_bps {
        None => env
            .storage()
            .persistent()
            .remove(&DataKeyCore::OracleMinConfidenceBps),
        Some(bps) => {
            env.storage()
                .persistent()
                .set(&DataKeyCore::OracleMinConfidenceBps, &bps);
            _extend_persistent_ttl(&env, &DataKeyCore::OracleMinConfidenceBps);
        }
    }
    Ok(())
}

/// Enables or disables strict mode for oracle confidence (admin only).
pub fn set_oracle_strict_mode(env: Env, enabled: bool) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    env.storage()
        .persistent()
        .set(&DataKeyCore::OracleStrictMode, &enabled);
    _extend_persistent_ttl(&env, &DataKeyCore::OracleStrictMode);
    Ok(())
}

/// Returns the configured minimum oracle confidence bps, if set.
pub fn get_oracle_min_confidence_bps(env: Env) -> Option<u32> {
    let key = DataKeyCore::OracleMinConfidenceBps;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Returns whether oracle strict mode is enabled.
pub fn get_oracle_strict_mode(env: Env) -> bool {
    let key = DataKeyCore::OracleStrictMode;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key).unwrap_or(false)
}

/// Enables or disables strict mode for oracle heartbeat health at settlement (admin only, Issue #264).
///
/// When enabled, `resolve_round` will reject settlement if the oracle heartbeat is not live,
/// unless an admin override is armed or the heartbeat is within the configured grace period.
pub fn set_hb_strict_mode(env: Env, enabled: bool) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    let mut config = _load_hb_config(&env);
    config.strict_mode = enabled;
    _save_hb_config(&env, &config);
    Ok(())
}

/// Returns whether oracle heartbeat strict mode is enabled.
pub fn get_hb_strict_mode(env: Env) -> bool {
    _load_hb_config(&env).strict_mode
}

/// Arms a one-shot override to bypass the heartbeat health gate for the next settlement (admin only, Issue #264).
///
/// Consumed automatically when the next `resolve_round` call passes the heartbeat health gate
/// while the override is armed. Does not persist across rounds.
pub fn arm_hb_override(env: Env) -> Result<(), ContractError> {
    let admin_key = DataKeyCore::Admin;
    _extend_persistent_ttl(&env, &admin_key);
    let admin: Address = env
        .storage()
        .persistent()
        .get(&admin_key)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("arm_hovr"), e);
    })?;

    let mut config = _load_hb_config(&env);
    config.override_armed = true;
    _save_hb_config(&env, &config);
    Ok(())
}

/// Returns whether the oracle heartbeat override is currently armed.
pub fn get_hb_override_armed(env: Env) -> bool {
    _load_hb_config(&env).override_armed
}

/// Sets the grace period in seconds between heartbeat staleness and settlement block (admin only, Issue #264).
///
/// When the oracle heartbeat is stale (past `OracleStaleThreshold`), the contract allows an additional
/// `grace_seconds` window before the heartbeat health gate blocks settlement in strict mode.
/// Default is 0 (no grace period).
pub fn set_hb_grace_seconds(env: Env, seconds: u64) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    let mut config = _load_hb_config(&env);
    config.grace_seconds = seconds;
    _save_hb_config(&env, &config);
    Ok(())
}

/// Returns the configured heartbeat grace period in seconds (default 0).
pub fn get_hb_grace_seconds(env: Env) -> u64 {
    _load_hb_config(&env).grace_seconds
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

/// Loads the heartbeat gate config, returning defaults if unset.
pub fn _load_hb_config(env: &Env) -> HbGateConfig {
    let key = HbGateKey::Config;
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            TTL_BUMP_THRESHOLD,
            TTL_BUMP_AMOUNT,
        );
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
    env.storage().persistent().extend_ttl(
        &key,
        TTL_BUMP_THRESHOLD,
        TTL_BUMP_AMOUNT,
    );
}

pub fn _enforce_heartbeat_health(env: &Env, oracle: &Address) -> Result<(), ContractError> {
    let config = _load_hb_config(env);
    if config.override_armed {
        let mut new_config = config.clone();
        new_config.override_armed = false;
        _save_hb_config(env, &new_config);
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("oracle"), Symbol::new(env, "hb_override")),
            (oracle.clone(),),
        );
        return Ok(());
    }

    let heartbeat_key = DataKeyCore::OracleHeartbeat;
    _extend_persistent_ttl(env, &heartbeat_key);
    let record: OracleHeartbeatRecord = match env.storage().persistent().get(&heartbeat_key) {
        Some(r) => r,
        None => {
            _emit_action_rejected(
                env,
                oracle,
                symbol_short!("resolve"),
                ContractError::OracleHeartbeatUnhealthy,
            );
            return Err(ContractError::OracleHeartbeatUnhealthy);
        }
    };

    let threshold_key = DataKeyCore::OracleStaleThreshold;
    _extend_persistent_ttl(env, &threshold_key);
    let stale_threshold: u64 = env
        .storage()
        .persistent()
        .get(&threshold_key)
        .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD);

    let current_time = env.ledger().timestamp();
    let is_fresh = current_time <= record.timestamp.saturating_add(stale_threshold);

    match record.status {
        0 => {
            if is_fresh {
                return Ok(());
            }
        }
        1 => {
            if is_fresh && !config.strict_mode {
                return Ok(());
            }
        }
        _ => {}
    }

    if (record.status == 0 || record.status == 1) && !config.strict_mode {
        let within_grace = current_time
            <= record
                .timestamp
                .saturating_add(stale_threshold)
                .saturating_add(config.grace_seconds);

        if within_grace {
            return Ok(());
        }
    }

    _emit_action_rejected(
        env,
        oracle,
        symbol_short!("resolve"),
        ContractError::OracleHeartbeatUnhealthy,
    );
    Err(ContractError::OracleHeartbeatUnhealthy)
}

/// Records an oracle heartbeat (oracle only).
pub fn update_oracle_heartbeat(env: Env, status: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
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

/// Schedules a timelocked stale threshold update
pub fn set_oracle_stale_threshold(env: Env, seconds: u64) -> Result<(), ContractError> {
    crate::config::schedule_oracle_stale_threshold(env, seconds)
}

/// Returns a composite protocol health status
pub fn get_protocol_health(env: Env) -> ProtocolHealthStatus {
    let ledger_sequence = env.ledger().sequence();
    let ledger_timestamp = env.ledger().timestamp();

    let paused = is_paused(env.clone());

    let heartbeat_key = DataKeyCore::OracleHeartbeat;
    _extend_persistent_ttl(&env, &heartbeat_key);
    let (oracle_live, oracle_status) = match env
        .storage()
        .persistent()
        .get::<_, OracleHeartbeatRecord>(&heartbeat_key)
    {
        None => (false, 3u32),
        Some(record) => {
            if record.status == 2 {
                (false, record.status)
            } else {
                let threshold_key = DataKeyCore::OracleStaleThreshold;
                _extend_persistent_ttl(&env, &threshold_key);
                let threshold: u64 = env
                    .storage()
                    .persistent()
                    .get(&threshold_key)
                    .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD);
                let live = ledger_timestamp <= record.timestamp.saturating_add(threshold);
                (live, record.status)
            }
        }
    };

    let (has_active_round, active_round_phase) = match env
        .storage()
        .persistent()
        .get::<_, Round>(&DataKeyCore::ActiveRound)
    {
        None => (false, 0u32),
        Some(round) => {
            let phase = _derive_round_phase(ledger_sequence, &round);
            (true, phase as u32)
        }
    };

    let schema_version = _schema_version(&env).unwrap_or(1);

    let mut issues: u32 = 0;
    if paused {
        issues += 1;
    }
    if !oracle_live {
        issues += 1;
    }
    if has_active_round && active_round_phase == 3 {
        issues += 1;
    }

    let status_code = if paused {
        1u32 // PAUSED
    } else if issues > 1 {
        5u32 // MULTIPLE_ISSUES
    } else if !oracle_live {
        2u32 // ORACLE_STALE
    } else if has_active_round && active_round_phase == 3 {
        3u32 // ROUND_STALE
    } else if !has_active_round {
        4u32 // NO_ACTIVE_ROUND
    } else {
        0u32 // HEALTHY
    };

    ProtocolHealthStatus {
        paused,
        oracle_live,
        oracle_status,
        has_active_round,
        active_round_phase,
        schema_version,
        ledger_sequence,
        ledger_timestamp,
        status_code,
    }
}

pub fn get_oracle_stale_threshold(env: Env) -> u64 {
    let key = DataKeyCore::OracleStaleThreshold;
    _extend_persistent_ttl(&env, &key);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD)
}

/// Reads the current [`RuntimeMode`], defaulting to `Normal` if unset.
fn _current_mode(env: &Env) -> RuntimeMode {
    let key = DataKeyCore::Paused;
    _extend_persistent_ttl(env, &key);
    env.storage()
        .persistent()
        .get::<_, RuntimeMode>(&key)
        .unwrap_or(RuntimeMode::Normal)
}

/// Single policy gate for every mutating entrypoint (Issue #261).
///
/// Every state-changing method in the contract funnels its mode check
/// through this one function instead of hand-rolling `if mode == X` logic
/// per-entrypoint. New methods are onboarded by picking the right
/// [`PolicyAction`] variant rather than re-deriving the mode rules, so the
/// gate cannot drift out of sync with individual call sites.
///
/// ## Mode × action matrix
///
/// | action          | `Normal` | `ClaimsOnly` | `FullyPaused` |
/// |------------------|:--------:|:------------:|:-------------:|
/// | `RoundMutation`  | ✅       | ❌           | ❌            |
/// | `Claim`          | ✅       | ✅           | ❌            |
/// | `AdminConfig`    | ✅       | ✅           | ❌            |
/// | `Settlement`     | ✅       | ✅           | ❌            |
///
/// `RoundMutation` is the only class blocked by `ClaimsOnly` — betting and
/// new rounds must stop once the protocol has no active round to bet into,
/// while admins can still reconfigure, the oracle can still settle/cancel a
/// round already in flight, and users can still withdraw what they're owed.
/// `FullyPaused` blocks every class uniformly (emergency stop).
///
/// ## Entrypoint inventory (action → dispatcher, `contract.rs`)
///
/// - `RoundMutation`: `place_bet`, `place_precision_prediction`,
///   `predict_price`, `commit_prediction`, `reveal_prediction`,
///   `mint_initial`.
/// - `Claim`: `claim_winnings`.
/// - `Settlement`: `resolve_round`, `cancel_round`.
/// - `AdminConfig`: `pause_contract`, `unpause_contract`, `set_runtime_mode`,
///   `migrate_schema_v1_to_v2`, `migrate_schema_v2_to_v3`,
///   `set_oracle_max_deviation_bps`, `arm_oracle_deviation_override`,
///   `set_oracle_min_confidence_bps`, `set_oracle_strict_mode`,
///   `set_hb_strict_mode`, `arm_hb_override`, `set_hb_grace_seconds`,
///   `propose_oracle_rotation`, `accept_oracle_rotation`,
///   `cancel_oracle_rotation`, `set_windows`, `set_max_stake`,
///   `set_max_user_exposure`, `set_max_pending_winnings`, `set_min_bet`,
///   `schedule_*` variants, `apply_scheduled_changes`, `cancel_config_change`,
///   `set_protocol_fee_bps`, `withdraw_protocol_fee`, `set_min_participants`,
///   `set_max_precision_participants`, `set_mint_limit`,
///   `set_archive_retention`, `set_close_buffer_ledgers`,
///   `set_round_template`, `clear_round_template`,
///   `reset_leaderboard_season`, `create_round`, `create_next_from_template`
///   (admin-gated, not `RoundMutation` — must stay callable in `ClaimsOnly`
///   since it is the entrypoint that transitions the protocol back to
///   `Active`; it is blocked only by `FullyPaused`).
///
/// `update_oracle_heartbeat` and `is_*`/`get_*` read-only queries are
/// intentionally not gated: heartbeat recording must keep flowing even while
/// paused so `get_protocol_health` reflects live oracle status during an
/// incident, and reads never mutate state.
pub fn _policy_gate(env: &Env, action: PolicyAction) -> Result<(), ContractError> {
    let mode = _current_mode(env);
    let blocked = match action {
        PolicyAction::RoundMutation => mode != RuntimeMode::Normal,
        PolicyAction::Claim | PolicyAction::AdminConfig | PolicyAction::Settlement => {
            mode == RuntimeMode::FullyPaused
        }
    };
    if blocked {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

/// Blocked only by `FullyPaused` — equivalent to `_policy_gate(env, PolicyAction::AdminConfig)`.
pub fn _ensure_not_paused(env: &Env) -> Result<(), ContractError> {
    _policy_gate(env, PolicyAction::AdminConfig)
}

/// Blocked by any non-`Normal` mode — equivalent to `_policy_gate(env, PolicyAction::RoundMutation)`.
pub fn _ensure_normal_mode(env: &Env) -> Result<(), ContractError> {
    _policy_gate(env, PolicyAction::RoundMutation)
}

pub fn _set_mode(env: &Env, new_mode: RuntimeMode) -> Result<(), ContractError> {
    let key = DataKeyCore::Paused;
    let old_mode = env
        .storage()
        .persistent()
        .get::<_, RuntimeMode>(&key)
        .unwrap_or(RuntimeMode::Normal);
    if old_mode != new_mode {
        env.storage().persistent().set(&key, &new_mode);
        _extend_persistent_ttl(env, &key);
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("mode"), Symbol::new(env, "transition")),
            (old_mode as u32, new_mode as u32),
        );
    }
    Ok(())
}

pub fn _schema_version(env: &Env) -> Option<u32> {
    env.storage().persistent().get(&DataKeyCore::SchemaVersion)
}

/// Returns `true` if a `DataKeyCore` variant is eligible for batch TTL touch
/// by a maintainer. Only system-critical, long-lived keys are allowlisted.
pub fn _is_ttl_touch_allowed(key: &DataKeyCore) -> bool {
    matches!(
        key,
        DataKeyCore::Admin
            | DataKeyCore::Oracle
            | DataKeyCore::SchemaVersion
            | DataKeyCore::Paused
            | DataKeyCore::BetWindowLedgers
            | DataKeyCore::RunWindowLedgers
            | DataKeyCore::CloseBufferLedgers
            | DataKeyCore::MaxStake
            | DataKeyCore::MaxUserRoundExposure
            | DataKeyCore::MaxPendingWinnings
            | DataKeyCore::MinParticipants
            | DataKeyCore::MaxPrecisionParticipants
            | DataKeyCore::OracleHeartbeat
            | DataKeyCore::OracleStaleThreshold
            | DataKeyCore::OracleMaxDeviationBps
            | DataKeyCore::OracleDeviationOverrideArmed
            | DataKeyCore::OracleMinConfidenceBps
            | DataKeyCore::OracleStrictMode
            | DataKeyCore::ProtocolFeeBps
            | DataKeyCore::ProtocolFeeTreasury
            | DataKeyCore::MigratedToV3
            | DataKeyCore::ArchiveRetention
            | DataKeyCore::RoundTemplate
            | DataKeyCore::LeaderboardWins
            | DataKeyCore::LeaderboardStreak
            | DataKeyCore::SeasonId
            | DataKeyCore::SeasonLeaderboardWins
            | DataKeyCore::SeasonLeaderboardStreak
            | DataKeyCore::LastRoundId
            | DataKeyCore::OracleRotationProposal
            | DataKeyCore::MintLimitConfig
    )
}

/// Auth-gated batch TTL extension for allowlisted storage keys (admin only).
///
/// Accepts a vector of `DataKeyCore` variants. Each key is validated against the
/// TTL-touch allowlist. Keys that exist in storage have their TTL extended to
/// `TTL_BUMP_AMOUNT` (~30 days). Keys not in the allowlist cause the entire
/// call to fail with `UnsupportedDataKeyForTtlTouch`. Keys that are in the
/// allowlist but absent from storage are silently skipped.
///
/// Returns the number of keys whose TTL was actually extended.
///
/// Emits `("storage", "touch")` with `(touched, skipped)` counts.
pub fn batch_touch_ttl(env: Env, keys: Vec<DataKeyCore>) -> Result<u32, ContractError> {
    let admin_key = DataKeyCore::Admin;
    _extend_persistent_ttl(&env, &admin_key);
    let admin: Address = env
        .storage()
        .persistent()
        .get(&admin_key)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("batch_t"), e);
    })?;

    let mut touched: u32 = 0;
    let mut skipped: u32 = 0;

    for key in keys.iter() {
        if !_is_ttl_touch_allowed(&key) {
            _emit_action_rejected(
                &env,
                &admin,
                symbol_short!("batch_t"),
                ContractError::UnsupportedDataKeyForTtlTouch,
            );
            return Err(ContractError::UnsupportedDataKeyForTtlTouch);
        }
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
            touched += 1;
        } else {
            skipped += 1;
        }
    }

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("storage"), symbol_short!("touch")),
        (touched, skipped),
    );

    Ok(touched)
}

/// Sets the multi-feed oracle quorum configuration (admin only).
///
/// When `Some(config)`, `resolve_round_multi` is enabled and the legacy
/// single-oracle path is unaffected.  When `None`, multi-feed resolution is
/// disabled and calling `resolve_round_multi` returns `OracleNotSet`.
pub fn set_oracle_quorum_config(
    env: Env,
    config: Option<OracleQuorumConfig>,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("set_quor"), e);
    })?;

    if let Some(ref cfg) = config {
        _validate_quorum_config(cfg).inspect_err(|&e| {
            _emit_action_rejected(&env, &admin, symbol_short!("set_quor"), e);
        })?;
    }

    let key = DataKeyCore::OracleQuorum;
    match config {
        Some(cfg) => {
            env.storage().persistent().set(&key, &cfg);
            _extend_persistent_ttl(&env, &key);
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("oracle"), symbol_short!("quorum")),
                (
                    cfg.min_observations,
                    cfg.quorum_threshold,
                    cfg.outlier_threshold_bps,
                ),
            );
        }
        None => {
            env.storage().persistent().remove(&key);
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("oracle"), symbol_short!("quorum")),
                (0u32, 0u32, 0u32),
            );
        }
    }
    Ok(())
}

/// Returns the configured multi-feed oracle quorum config, if set.
pub fn get_oracle_quorum_config(env: Env) -> Option<OracleQuorumConfig> {
    let key = DataKeyCore::OracleQuorum;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Validates the quorum config values are within allowed bounds.
pub(crate) fn _validate_quorum_config(cfg: &OracleQuorumConfig) -> Result<(), ContractError> {
    use crate::common::{
        DEFAULT_ORACLE_QUORUM_MIN_OBSERVATIONS, DEFAULT_ORACLE_QUORUM_THRESHOLD,
        MAX_ORACLE_OBSERVATIONS,
    };
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

pub fn _require_supported_schema(env: &Env) -> Result<u32, ContractError> {
    _extend_persistent_ttl(env, &DataKeyCore::SchemaVersion);
    if env.storage().persistent().has(&DataKeyCore::Admin) {
        _extend_persistent_ttl(env, &DataKeyCore::Admin);
    }
    let v = _schema_version(env).unwrap_or(1);
    if v == 0 || v > CURRENT_SCHEMA_VERSION {
        return Err(ContractError::UnsupportedSchemaVersion);
    }
    Ok(v)
}

/// Reclaims expired pending winnings from `user` and credits them to the admin.
///
/// # Policy
/// Unclaimed pending winnings older than the configured `PendingWinningsExpiry`
/// (in ledgers) may be administratively reclaimed. The funds are credited to
/// the admin's balance as a temporary sink, preserving the conservation
/// invariant — no value is destroyed.
///
/// Emits `("claim", "expired")` on success.
///
/// # Errors
/// - `AdminNotSet` — contract not initialized.
/// - `ContractPaused` — contract is fully paused.
/// - `PendingWinningsNotExpired` — entry exists but hasn't reached the expiry threshold.
/// - `NoActiveRound` — used as a generic "no pending winnings" signal when
///   the entry doesn't exist or expiry is disabled (0).
pub fn reclaim_expired_pending_winnings(env: Env, user: Address) -> Result<i128, ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("reclaim"), e);
    })?;

    // Read the expiry config. 0 or absent means expiry is disabled.
    let expiry_key = PENDING_WINNINGS_EXPIRY_KEY;
    let expiry_ledgers: u32 = env
        .storage()
        .persistent()
        .get(&expiry_key)
        .unwrap_or(0);
    if expiry_ledgers == 0 {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("reclaim"),
            ContractError::ExpiryNotConfigured,
        );
        return Err(ContractError::ExpiryNotConfigured);
    }

    // Read pending winnings.
    let pending_key = DataKeyScoped::PendingWinnings(user.clone());
    let pending: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
    if pending == 0 {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("reclaim"),
            ContractError::PendingWinningsNotFound,
        );
        return Err(ContractError::PendingWinningsNotFound);
    }

    // Read the ledger when this entry was last updated.
    let updated_key = PendingWinningsUpdatedAtKey(user.clone());
    let updated_at: u32 = env
        .storage()
        .persistent()
        .get(&updated_key)
        .ok_or(ContractError::PendingWinningsNotFound)?;

    let current_ledger = env.ledger().sequence();
    let age = current_ledger.saturating_sub(updated_at);

    if age < expiry_ledgers {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("reclaim"),
            ContractError::PendingWinningsNotExpired,
        );
        return Err(ContractError::PendingWinningsNotExpired);
    }

    // CEI: remove storage keys before transferring.
    env.storage().persistent().remove(&pending_key);
    env.storage().persistent().remove(&updated_key);

    // Credit the admin's balance (conservation: funds are not destroyed).
    let admin_bal = balance(env.clone(), admin.clone());
    let new_admin_bal = payout_add(admin_bal, pending)?;
    _set_balance(&env, admin.clone(), new_admin_bal);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("claim"), symbol_short!("expired")),
        (user, pending, admin),
    );

    Ok(pending)
}