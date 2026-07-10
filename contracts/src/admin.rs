// SPDX-License-Identifier: MIT
use crate::common::{
    _derive_round_phase, _emit_action_rejected, _extend_persistent_ttl, CURRENT_SCHEMA_VERSION,
    DEFAULT_BET_WINDOW_LEDGERS, DEFAULT_ORACLE_STALE_THRESHOLD, DEFAULT_RUN_WINDOW_LEDGERS,
};
use crate::errors::ContractError;
use crate::types::{DataKey, OracleHeartbeatRecord, ProtocolHealthStatus, Round, RuntimeMode};
use soroban_sdk::{symbol_short, Address, Env, Symbol};

/// Initializes the contract with admin and oracle addresses (one-time only)
pub fn initialize(env: Env, admin: Address, oracle: Address) -> Result<(), ContractError> {
    admin.require_auth();

    if admin == oracle {
        return Err(ContractError::OracleNetworkMismatch);
    }

    if env.storage().persistent().has(&DataKey::Admin) {
        return Err(ContractError::AlreadyInitialized);
    }

    env.storage().persistent().set(&DataKey::Admin, &admin);
    env.storage().persistent().set(&DataKey::Oracle, &oracle);
    env.storage()
        .persistent()
        .set(&DataKey::Paused, &RuntimeMode::Normal);
    env.storage()
        .persistent()
        .set(&DataKey::SchemaVersion, &CURRENT_SCHEMA_VERSION);

    // Set default window values
    env.storage()
        .persistent()
        .set(&DataKey::BetWindowLedgers, &DEFAULT_BET_WINDOW_LEDGERS);
    env.storage()
        .persistent()
        .set(&DataKey::RunWindowLedgers, &DEFAULT_RUN_WINDOW_LEDGERS);

    _extend_persistent_ttl(&env, &DataKey::Admin);
    _extend_persistent_ttl(&env, &DataKey::Oracle);
    _extend_persistent_ttl(&env, &DataKey::Paused);
    _extend_persistent_ttl(&env, &DataKey::SchemaVersion);
    _extend_persistent_ttl(&env, &DataKey::BetWindowLedgers);
    _extend_persistent_ttl(&env, &DataKey::RunWindowLedgers);

    Ok(())
}

/// Returns the stored schema version. If unset, returns legacy version 1.
pub fn get_schema_version(env: Env) -> u32 {
    let key = DataKey::SchemaVersion;
    _extend_persistent_ttl(&env, &key);
    _schema_version(&env).unwrap_or(1)
}

/// Migrates legacy schema version 1 → version 2 (admin only).
pub fn migrate_schema_v1_to_v2(env: Env) -> Result<(), ContractError> {
    let admin_key = DataKey::Admin;
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

    if env.storage().persistent().has(&DataKey::ActiveRound) {
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

    let schema_key = DataKey::SchemaVersion;
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
pub fn migrate_schema_v2_to_v3(env: Env) -> Result<(), ContractError> {
    let admin_key = DataKey::Admin;
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

    if env.storage().persistent().has(&DataKey::ActiveRound) {
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

    let schema_key = DataKey::SchemaVersion;
    env.storage().persistent().set(&schema_key, &TARGET_VERSION);
    _extend_persistent_ttl(&env, &schema_key);

    env.storage()
        .persistent()
        .set(&DataKey::MigratedToV3, &true);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("schema"), symbol_short!("migrated")),
        (from, TARGET_VERSION),
    );

    Ok(())
}

/// Returns whether the contract is currently paused
pub fn is_paused(env: Env) -> bool {
    let key = DataKey::Paused;
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
        .get(&DataKey::Admin)
        .ok_or(ContractError::AdminNotSet)?;

    admin.require_auth();
    _set_mode(&env, RuntimeMode::FullyPaused)?;

    Ok(())
}

/// Unpauses the contract after recovery (admin only)
pub fn unpause_contract(env: Env) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .ok_or(ContractError::AdminNotSet)?;

    admin.require_auth();
    _set_mode(&env, RuntimeMode::Normal)?;

    Ok(())
}

/// Returns the current runtime mode (0 = Normal, 1 = ClaimsOnly, 2 = FullyPaused)
pub fn get_runtime_mode(env: Env) -> u32 {
    let key = DataKey::Paused;
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
        .get(&DataKey::Admin)
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
    let key = DataKey::Admin;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

pub fn get_oracle(env: Env) -> Option<Address> {
    let key = DataKey::Oracle;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Schedules a timelocked oracle deviation update
pub fn set_oracle_max_deviation_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
    crate::config::schedule_oracle_deviation_bps(env, bps)
}

/// Returns the configured oracle max deviation bps, if set.
pub fn get_oracle_max_deviation_bps(env: Env) -> Option<u32> {
    let key = DataKey::OracleMaxDeviationBps;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Arms a one-shot override to bypass deviation checks for the next settlement (admin only).
pub fn arm_oracle_deviation_override(env: Env) -> Result<(), ContractError> {
    let admin_key = DataKey::Admin;
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

    let override_key = DataKey::OracleDeviationOverrideArmed;
    env.storage().persistent().set(&override_key, &true);
    _extend_persistent_ttl(&env, &override_key);
    Ok(())
}

/// Sets the minimum oracle confidence threshold in basis points (admin only).
pub fn set_oracle_min_confidence_bps(env: Env, min_bps: Option<u32>) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
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
            .remove(&DataKey::OracleMinConfidenceBps),
        Some(bps) => {
            env.storage()
                .persistent()
                .set(&DataKey::OracleMinConfidenceBps, &bps);
            _extend_persistent_ttl(&env, &DataKey::OracleMinConfidenceBps);
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
        .get(&DataKey::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    env.storage()
        .persistent()
        .set(&DataKey::OracleStrictMode, &enabled);
    _extend_persistent_ttl(&env, &DataKey::OracleStrictMode);
    Ok(())
}

/// Returns the configured minimum oracle confidence bps, if set.
pub fn get_oracle_min_confidence_bps(env: Env) -> Option<u32> {
    let key = DataKey::OracleMinConfidenceBps;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Returns whether oracle strict mode is enabled.
pub fn get_oracle_strict_mode(env: Env) -> bool {
    let key = DataKey::OracleStrictMode;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key).unwrap_or(false)
}

/// Records an oracle heartbeat (oracle only).
pub fn update_oracle_heartbeat(env: Env, status: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    if status > 2 {
        _extend_persistent_ttl(&env, &DataKey::Oracle);
        if let Some(oracle) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Oracle)
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
    _extend_persistent_ttl(&env, &DataKey::Oracle);
    let oracle: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Oracle)
        .ok_or(ContractError::OracleNotSet)?;
    oracle.require_auth();

    let ts = env.ledger().timestamp();
    let record = OracleHeartbeatRecord {
        timestamp: ts,
        status,
    };
    env.storage()
        .persistent()
        .set(&DataKey::OracleHeartbeat, &record);
    _extend_persistent_ttl(&env, &DataKey::OracleHeartbeat);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("oracle"), symbol_short!("hbeat")),
        (ts, status),
    );
    Ok(())
}

/// Returns the most recent oracle heartbeat record, if any.
pub fn get_oracle_heartbeat(env: Env) -> Option<OracleHeartbeatRecord> {
    let key = DataKey::OracleHeartbeat;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Returns `true` if the oracle has a non-stale heartbeat with status not offline (2).
pub fn is_oracle_live(env: Env) -> bool {
    let heartbeat_key = DataKey::OracleHeartbeat;
    _extend_persistent_ttl(&env, &heartbeat_key);
    let record: OracleHeartbeatRecord = match env.storage().persistent().get(&heartbeat_key) {
        Some(r) => r,
        None => return false,
    };
    if record.status == 2 {
        return false;
    }
    let threshold_key = DataKey::OracleStaleThreshold;
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

    let heartbeat_key = DataKey::OracleHeartbeat;
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
                let threshold_key = DataKey::OracleStaleThreshold;
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
        .get::<_, Round>(&DataKey::ActiveRound)
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
    let key = DataKey::OracleStaleThreshold;
    _extend_persistent_ttl(&env, &key);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD)
}

pub fn _ensure_not_paused(env: &Env) -> Result<(), ContractError> {
    let key = DataKey::Paused;
    _extend_persistent_ttl(env, &key);
    let mode = env
        .storage()
        .persistent()
        .get::<_, RuntimeMode>(&key)
        .unwrap_or(RuntimeMode::Normal);
    if mode == RuntimeMode::FullyPaused {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

pub fn _ensure_normal_mode(env: &Env) -> Result<(), ContractError> {
    let key = DataKey::Paused;
    _extend_persistent_ttl(env, &key);
    let mode = env
        .storage()
        .persistent()
        .get::<_, RuntimeMode>(&key)
        .unwrap_or(RuntimeMode::Normal);
    if mode != RuntimeMode::Normal {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

pub fn _set_mode(env: &Env, new_mode: RuntimeMode) -> Result<(), ContractError> {
    let key = DataKey::Paused;
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
    env.storage().persistent().get(&DataKey::SchemaVersion)
}

pub fn _require_supported_schema(env: &Env) -> Result<u32, ContractError> {
    _extend_persistent_ttl(env, &DataKey::SchemaVersion);
    if env.storage().persistent().has(&DataKey::Admin) {
        _extend_persistent_ttl(env, &DataKey::Admin);
    }
    let v = _schema_version(env).unwrap_or(1);
    if v == 0 || v > CURRENT_SCHEMA_VERSION {
        return Err(ContractError::UnsupportedSchemaVersion);
    }
    Ok(v)
}
