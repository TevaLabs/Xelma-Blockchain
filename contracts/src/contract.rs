// SPDX-License-Identifier: MIT
//! Core contract implementation for the XLM Price Prediction Market.

use soroban_sdk::{
<<<<<<< HEAD
    contract,
    contractimpl,
    panic_with_error,
    symbol_short,
    Address,
    Bytes,
    BytesN,
    Env,
    Map,
    Symbol,
    Vec,
=======
    contract, contractimpl, Address, BytesN, Env, Map, Symbol, Vec,
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
};

use crate::errors::ContractError;
use crate::types::{
<<<<<<< HEAD
    BetSide,
    DataKey,
    OracleHeartbeatRecord,
    OraclePayload,
    PrecisionCommitment,
    PrecisionPrediction,
    Round,
    RoundMode,
    UserPosition,
    UserStats,
=======
    ArchivedRoundSummary, BetSide, ConfigChangeKind, ConfigChangePayload, DataKey,
    OracleHeartbeatRecord, OraclePayload, OracleRotationProposal, PendingConfigChange,
    PrecisionCommitment, PrecisionPrediction, ProtocolHealthStatus, Round, RoundArchiveStatus,
    RoundMode, UserPosition, UserStats,
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
};

// ─── Economic control limits ─────────────────────────────────────────────────
/// Minimum allowed value when setting an economic cap to prevent zero-value lockouts.
const MIN_CAP_VALUE: i128 = 1;
/// Upper bound on the minimum-participants config to prevent unbounded gas in resolution.
const MAX_MIN_PARTICIPANTS: u32 = 10_000;
const DEFAULT_MAX_PRECISION_PARTICIPANTS: u32 = 1_000;
const MAX_PRECISION_PARTICIPANTS_LIMIT: u32 = 10_000;
/// Maximum number of entries returned per page by paginated query methods,
/// regardless of the caller-requested `limit` (Issue #139).
const MAX_PAGE_SIZE: u32 = 100;

// ─── Oracle heartbeat limits ──────────────────────────────────────────────────
const DEFAULT_ORACLE_STALE_THRESHOLD: u64 = 3_600; // 1 hour
const MIN_ORACLE_STALE_THRESHOLD: u64 = 60; // 1 minute
const MAX_ORACLE_STALE_THRESHOLD: u64 = 86_400; // 24 hours
const ORACLE_MAX_DATA_AGE_SECONDS: u64 = 300;

// ─── Oracle rotation expiry ───────────────────────────────────────────────────
const MIN_ROTATION_EXPIRY_SECONDS: u64 = 60; // 1 minute minimum

const DEFAULT_BET_WINDOW_LEDGERS: u32 = 6;
const DEFAULT_RUN_WINDOW_LEDGERS: u32 = 12;
const MAX_BET_WINDOW_LEDGERS: u32 = 1_440;
const MAX_RUN_WINDOW_LEDGERS: u32 = 2_880;

// ─── Oracle deviation guardrails ─────────────────────────────────────────────
/// Maximum allowed basis points for oracle deviation is bounded to avoid absurd configs.
/// 100_000 bp = 1000% deviation (effectively "off", but still explicit).
const MAX_ORACLE_DEVIATION_BPS: u32 = 100_000;

// ─── Protocol fee (Issue #162) ────────────────────────────────────────────────
/// Hard cap on the optional protocol settlement fee, in basis points
/// (1 bp = 0.01%). 1_000 bp = 10% of the round's total pot — the maximum an
/// admin may ever schedule via timelock. Larger values would risk turning
/// the protocol into a de-facto extraction mechanism and are explicitly
/// disallowed to preserve user trust and the conservation invariant.
const MAX_PROTOCOL_FEE_BPS: u32 = 1_000;
/// Denominator for bps math: `fee = total_pot * bps / BPS_DENOMINATOR`.
/// Pinned to 10_000 to match the universal "1 bp = 0.01%" convention.
const BPS_DENOMINATOR: i128 = 10_000;

// ─── Storage schema versioning ───────────────────────────────────────────────
const CURRENT_SCHEMA_VERSION: u32 = 3;
// ─── Start-price bounds (Issue #119) ─────────────────────────────────────────
/// Minimum start price in protocol units — prevents zero-value and dust rounds.
const MIN_START_PRICE: u128 = 1;
/// Maximum start price in protocol units — guards against overflow in payout math.
const MAX_START_PRICE: u128 = 1_000_000_000_000_000_000;
// ─── Storage TTL Lifecycle Limits (Issue #142) ──────────────────────────────
/// Minimum remaining ledgers before a persistent entry is extended.
const TTL_BUMP_THRESHOLD: u32 = 17_280; // ~1 day at 5-second ledgers
/// Amount of ledgers to extend a persistent entry to when below threshold.
const TTL_BUMP_AMOUNT: u32 = 518_400; // ~30 days at 5-second ledgers

/// Default archived round summaries retained on-chain (FIFO pruning).
const DEFAULT_ARCHIVE_RETENTION: u32 = 128;
/// Minimum archive retention limit — prevents accidental pruning of all history.
const MIN_ARCHIVE_RETENTION: u32 = 1;
/// Maximum archive retention limit — prevents unbounded storage growth.
const MAX_ARCHIVE_RETENTION: u32 = 10_000;
/// Ledgers to wait before a scheduled critical config change may be applied (~2 hours).
const CONFIG_TIMELOCK_LEDGERS: u32 = 1440;
    ArchivedRoundSummary, BetSide, ConfigChangeKind, ConfigChangePayload,
    OracleHeartbeatRecord, OraclePayload, PendingConfigChange,
    PrecisionPrediction, ProtocolHealthStatus, Round, RoundPhase, RoundPoolStats,
    UserPosition, UserRoundOutcome, UserStats,
};

use crate::admin;
use crate::config;
use crate::betting;
use crate::settlement;
use crate::queries;
use crate::common;

#[contract]
pub struct VirtualTokenContract;

#[contractimpl]
impl VirtualTokenContract {
    /// Initializes the contract with admin and oracle addresses (one-time only)
    pub fn initialize(env: Env, admin: Address, oracle: Address) -> Result<(), ContractError> {
        admin.require_auth();

        if admin == oracle {
            return Err(ContractError::InvalidMode);
        }

        if env.storage().persistent().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }

        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::Oracle, &oracle);
<<<<<<< HEAD
        env.storage().persistent().set(&DataKey::Paused, &false);
        env.storage().persistent().set(&DataKey::SchemaVersion, &CURRENT_SCHEMA_VERSION);
=======
        env.storage()
            .persistent()
            .set(&DataKey::Paused, &RuntimeMode::Normal);
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersion, &CURRENT_SCHEMA_VERSION);
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b

        // Set default window values
        env.storage().persistent().set(&DataKey::BetWindowLedgers, &DEFAULT_BET_WINDOW_LEDGERS);
        env.storage().persistent().set(&DataKey::RunWindowLedgers, &DEFAULT_RUN_WINDOW_LEDGERS);

        Self::_extend_persistent_ttl(&env, &DataKey::Admin);
        Self::_extend_persistent_ttl(&env, &DataKey::Oracle);
        Self::_extend_persistent_ttl(&env, &DataKey::Paused);
        Self::_extend_persistent_ttl(&env, &DataKey::SchemaVersion);
        Self::_extend_persistent_ttl(&env, &DataKey::BetWindowLedgers);
        Self::_extend_persistent_ttl(&env, &DataKey::RunWindowLedgers);

        Ok(())
        admin::initialize(env, admin, oracle)
    }

    /// Returns the stored schema version. If unset, returns legacy version 1.
    pub fn get_schema_version(env: Env) -> u32 {
        admin::get_schema_version(env)
    }

    /// Migrates legacy schema version 1 → version 2 (admin only).
    pub fn migrate_schema_v1_to_v2(env: Env) -> Result<(), ContractError> {
        let admin_key = DataKey::Admin;
        Self::_extend_persistent_ttl(&env, &admin_key);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&admin_key)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env).map_err(|e| {
            Self::_emit_action_rejected(&env, &admin, symbol_short!("migrate"), e);
            e
        })?;

        if env.storage().persistent().has(&DataKey::ActiveRound) {
            Self::_emit_action_rejected(
                &env,
                &admin,
                symbol_short!("migrate"),
                ContractError::MigrationActiveRound,
            );
            return Err(ContractError::MigrationActiveRound);
        }

        let from = Self::_schema_version(&env).unwrap_or(1);
        if from != 1 || CURRENT_SCHEMA_VERSION != 2 {
            return Err(ContractError::UnsupportedSchemaVersion);
        const TARGET_VERSION: u32 = 2;
        if from != 1 {
            Self::_emit_action_rejected(
                &env,
                &admin,
                symbol_short!("migrate"),
                ContractError::InvalidMigrationPath,
            );
            return Err(ContractError::InvalidMigrationPath);
        }

<<<<<<< HEAD
        env.storage().persistent().set(&DataKey::SchemaVersion, &CURRENT_SCHEMA_VERSION);
=======
        let schema_key = DataKey::SchemaVersion;
        env.storage().persistent().set(&schema_key, &TARGET_VERSION);
        Self::_extend_persistent_ttl(&env, &schema_key);
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("schema"), symbol_short!("migrated")),
<<<<<<< HEAD
            (from, CURRENT_SCHEMA_VERSION)
=======
            (from, TARGET_VERSION),
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
        );

        Ok(())
        admin::migrate_schema_v1_to_v2(env)
    }

    /// Migrates schema version 2 → version 3 (admin only).
    pub fn migrate_schema_v2_to_v3(env: Env) -> Result<(), ContractError> {
        admin::migrate_schema_v2_to_v3(env)
    }

    /// Returns whether the contract is currently paused
    pub fn is_paused(env: Env) -> bool {
<<<<<<< HEAD
        env.storage().persistent().get(&DataKey::Paused).unwrap_or(false)
=======
        admin::is_paused(env)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    /// Pauses the contract for emergency recovery (admin only)
    pub fn pause_contract(env: Env) -> Result<(), ContractError> {
        admin::pause_contract(env)
    }

    /// Unpauses the contract after recovery (admin only)
    pub fn unpause_contract(env: Env) -> Result<(), ContractError> {
        admin::unpause_contract(env)
    }

    /// Returns the current runtime mode (0 = Normal, 1 = ClaimsOnly, 2 = FullyPaused)
    pub fn get_runtime_mode(env: Env) -> u32 {
        admin::get_runtime_mode(env)
    }

    /// Sets the runtime mode of the contract (admin only)
    pub fn set_runtime_mode(env: Env, mode: u32) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
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

        Self::_set_mode(&env, new_mode)?;

        Ok(())
    }

    /// Creates a new prediction round (admin only)
    /// mode: 0 = Up/Down (default), 1 = Precision (Legends)
    pub fn create_round(
        env: Env,
        start_price: u128,
        mode: Option<u32>
    ) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        if start_price < MIN_START_PRICE {
            return Err(ContractError::InvalidStartPrice);
        }
        if start_price > MAX_START_PRICE {
            return Err(ContractError::InvalidStartPrice);
        }

        // Default to Up/Down mode (0) if not specified
        let mode_value = mode.unwrap_or(0);

        // Validate mode is either 0 or 1
        if mode_value > 1 {
            return Err(ContractError::InvalidMode);
        }

        let round_mode = if mode_value == 0 { RoundMode::UpDown } else { RoundMode::Precision };

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;

        admin.require_auth();
        Self::_ensure_not_paused(&env).map_err(|e| {
            Self::_emit_action_rejected(&env, &admin, symbol_short!("create"), e);
            e
        })?;
        Self::assert_no_active_round(&env).map_err(|e| {
            Self::_emit_action_rejected(&env, &admin, symbol_short!("create"), e);
            e
        })?;

        // Get configured windows (with defaults)
        Self::_extend_persistent_ttl(&env, &DataKey::BetWindowLedgers);
        let bet_ledgers: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::BetWindowLedgers)
            .unwrap_or(DEFAULT_BET_WINDOW_LEDGERS);
        Self::_extend_persistent_ttl(&env, &DataKey::RunWindowLedgers);
        let run_ledgers: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RunWindowLedgers)
            .unwrap_or(DEFAULT_RUN_WINDOW_LEDGERS);

        // Generate unique round ID
<<<<<<< HEAD
        let last_round_id: u64 = env.storage().persistent().get(&DataKey::LastRoundId).unwrap_or(0);
        let round_id = last_round_id.checked_add(1).ok_or(ContractError::Overflow)?;
        env.storage().persistent().set(&DataKey::LastRoundId, &round_id);
=======
        Self::_extend_persistent_ttl(&env, &DataKey::LastRoundId);
        let last_round_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::LastRoundId)
            .unwrap_or(0);
        let round_id = last_round_id
            .checked_add(1)
            .ok_or(ContractError::Overflow)?;
        env.storage()
            .persistent()
            .set(&DataKey::LastRoundId, &round_id);
        Self::_extend_persistent_ttl(&env, &DataKey::LastRoundId);
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b

        let start_ledger = env.ledger().sequence();
        let bet_end_ledger = start_ledger.checked_add(bet_ledgers).ok_or(ContractError::Overflow)?;
        let end_ledger = start_ledger.checked_add(run_ledgers).ok_or(ContractError::Overflow)?;

        let round = Round {
            round_id,
            price_start: start_price,
            start_ledger,
            bet_end_ledger,
            end_ledger,
            pool_up: 0,
            pool_down: 0,
            mode: round_mode.clone(),
        };

        env.storage().persistent().set(&DataKey::ActiveRound, &round);
        env.storage()
            .persistent()
<<<<<<< HEAD
            .set(&Self::round_start_timestamp_key(round_id), &env.ledger().timestamp());
=======
            .set(&DataKey::ActiveRound, &round);
        Self::_extend_persistent_ttl(&env, &DataKey::ActiveRound);
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b

        // Note: individual position keys (DataKey::Position / DataKey::PrecisionPosition)
        // are cleaned up at resolve time; no bulk-map clearing needed here.

        // Emit round creation event with round ID and mode
        // Topic: ("round", "created")
        // Payload: (round_id: u64, start_price: u128, start_ledger: u32, bet_end_ledger: u32, end_ledger: u32, mode: u32)
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("round"), symbol_short!("created")),
            (round_id, start_price, start_ledger, bet_end_ledger, end_ledger, mode_value)
        );

        Ok(())
    }

    /// Returns the currently active round, if any
    pub fn get_active_round(env: Env) -> Option<Round> {
        env.storage().persistent().get(&DataKey::ActiveRound)
    }
    /// Returns live pool-composition metrics for the currently active round.
    pub fn get_round_pool_stats(env: Env) -> Option<RoundPoolStats> {
        let round: Round = env.storage().persistent().get(&DataKey::ActiveRound)?;
        let participants_key = DataKey::RoundParticipants(round.round_id);
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
                            .get::<_, UserPosition>(&DataKey::Position(round.round_id, user))
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
                                &DataKey::PrecisionPosition(round.round_id, user.clone()),
                            )
                        {
                            stats.precision_prediction_count += 1;
                            stats.precision_total_stake += prediction.amount;
                        } else if let Some(commitment) =
                            env.storage().persistent().get::<_, PrecisionCommitment>(
                                &DataKey::PrecisionCommitment(round.round_id, user),
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
    ///
    /// Phase boundaries are deterministic:
    /// - `Betting` while `ledger < bet_end_ledger`
    /// - `Running` while `bet_end_ledger ≤ ledger < end_ledger`
    /// - `Resolvable` when `ledger ≥ end_ledger`
    ///
    /// Returns [`ContractError::NoActiveRound`] when no round is active.
    pub fn get_round_phase(env: Env) -> Result<RoundPhase, ContractError> {
        let round = env
            .storage()
            .persistent()
            .get::<_, Round>(&DataKey::ActiveRound)
            .ok_or(ContractError::NoActiveRound)?;
        Ok(Self::_derive_round_phase(env.ledger().sequence(), &round))
    }

    /// Returns the ID of the last created round (0 if no rounds created yet)
    pub fn get_last_round_id(env: Env) -> u64 {
        env.storage().persistent().get(&DataKey::LastRoundId).unwrap_or(0)
    }

    /// Returns a compact archived round summary by round id, if retained.
    pub fn get_archived_round(env: Env, round_id: u64) -> Option<ArchivedRoundSummary> {
        env.storage()
            .persistent()
            .get(&DataKey::ArchivedRound(round_id))
    }

    /// Returns up to `limit` most recently archived rounds (newest first).
    ///
    /// Pass `limit = 0` to receive an empty list. Values above the configured
    /// archive retention limit are capped automatically.
    pub fn get_recent_archived_rounds(env: Env, limit: u32) -> Vec<ArchivedRoundSummary> {
        let env_ref = &env;
        let recent: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::RecentArchivedRoundIds)
            .unwrap_or(Vec::new(env_ref));

        let mut result = Vec::new(env_ref);
        if limit == 0 || recent.is_empty() {
            return result;
        }

        let retention_limit = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::ArchiveRetention)
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
                    .get(&DataKey::ArchivedRound(round_id))
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
    /// Missing data returns `None` cleanly — the user either did not participate
    /// in the requested round or the round has not yet been archived.
    pub fn get_user_archived_participation(
        env: Env,
        user: Address,
        round_id: u64,
    ) -> Option<UserRoundOutcome> {
        let key = DataKey::UserRoundOutcome(round_id, user);
        env.storage().persistent().get(&key)
        admin::set_runtime_mode(env, mode)
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        admin::get_admin(env)
    }

    pub fn get_oracle(env: Env) -> Option<Address> {
        admin::get_oracle(env)
    }

    /// Schedules a timelocked oracle deviation update
    pub fn set_oracle_max_deviation_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
<<<<<<< HEAD
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        if let Some(v) = bps {
            if v == 0 || v > MAX_ORACLE_DEVIATION_BPS {
                return Err(ContractError::InvalidOracleDeviationBps);
            }
            env.storage().persistent().set(&DataKey::OracleMaxDeviationBps, &v);
        } else {
            env.storage().persistent().remove(&DataKey::OracleMaxDeviationBps);
        }
        Ok(())
=======
        admin::set_oracle_max_deviation_bps(env, bps)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    /// Returns the configured oracle max deviation bps, if set.
    pub fn get_oracle_max_deviation_bps(env: Env) -> Option<u32> {
<<<<<<< HEAD
        env.storage().persistent().get(&DataKey::OracleMaxDeviationBps)
=======
        admin::get_oracle_max_deviation_bps(env)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    /// Arms a one-shot override to bypass deviation checks for the next settlement (admin only).
    pub fn arm_oracle_deviation_override(env: Env) -> Result<(), ContractError> {
<<<<<<< HEAD
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        env.storage().persistent().set(&DataKey::OracleDeviationOverrideArmed, &true);
        Ok(())
=======
        admin::arm_oracle_deviation_override(env)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    /// Sets the minimum oracle confidence threshold in basis points (admin only).
    pub fn set_oracle_min_confidence_bps(
        env: Env,
        min_bps: Option<u32>,
    ) -> Result<(), ContractError> {
        admin::set_oracle_min_confidence_bps(env, min_bps)
    }

    /// Enables or disables strict mode for oracle confidence (admin only).
    pub fn set_oracle_strict_mode(env: Env, enabled: bool) -> Result<(), ContractError> {
        admin::set_oracle_strict_mode(env, enabled)
    }

    /// Returns the configured minimum oracle confidence bps, if set.
    pub fn get_oracle_min_confidence_bps(env: Env) -> Option<u32> {
        admin::get_oracle_min_confidence_bps(env)
    }

    /// Returns whether oracle strict mode is enabled.
    pub fn get_oracle_strict_mode(env: Env) -> bool {
        admin::get_oracle_strict_mode(env)
    }

    /// Records an oracle heartbeat (oracle only).
    pub fn update_oracle_heartbeat(env: Env, status: u32) -> Result<(), ContractError> {
<<<<<<< HEAD
        Self::_require_supported_schema(&env)?;
        if status > 2 {
            return Err(ContractError::InvalidOracleStatus);
        }
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
        env.storage().persistent().set(&DataKey::OracleHeartbeat, &record);

        #[allow(deprecated)]
        env.events().publish((symbol_short!("oracle"), symbol_short!("heartbeat")), (ts, status));
        Ok(())
=======
        admin::update_oracle_heartbeat(env, status)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    /// Returns the most recent oracle heartbeat record, if any.
    pub fn get_oracle_heartbeat(env: Env) -> Option<OracleHeartbeatRecord> {
        admin::get_oracle_heartbeat(env)
    }

    /// Returns `true` if the oracle has a non-stale heartbeat with status not offline (2).
    pub fn is_oracle_live(env: Env) -> bool {
<<<<<<< HEAD
        let record: OracleHeartbeatRecord = match
            env.storage().persistent().get(&DataKey::OracleHeartbeat)
        {
            Some(r) => r,
            None => {
                return false;
            }
        };
        if record.status == 2 {
            return false;
        }
        let threshold: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::OracleStaleThreshold)
            .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD);
        let current_time = env.ledger().timestamp();
        current_time <= record.timestamp.saturating_add(threshold)
=======
        admin::is_oracle_live(env)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    /// Schedules a timelocked stale threshold update
    pub fn set_oracle_stale_threshold(env: Env, seconds: u64) -> Result<(), ContractError> {
        admin::set_oracle_stale_threshold(env, seconds)
    }

    /// Returns a composite protocol health status
    pub fn get_protocol_health(env: Env) -> ProtocolHealthStatus {
        admin::get_protocol_health(env)
    }

    /// Returns the configured oracle stale threshold, or the default if not set.
    /// Returns the global status of the protocol.
    ///
    /// This is the canonical single-call status endpoint for frontends and
    /// monitoring dashboards. The returned [`ProtocolStatus`] maps directly to
    /// the three mutually-exclusive states visible to end users:
    ///
    /// | return value      | meaning                                             |
    /// |-------------------|-----------------------------------------------------|
    /// | `Active`      (0) | A round is live; bets or reveals are accepted.      |
    /// | `Paused`      (1) | Emergency pause active; mutations rejected.          |
    /// | `ClaimsOnly`  (2) | No active round; only `claim_winnings` is useful.   |
    ///
    /// **Priority**: `Paused` is always returned first when the contract is
    /// paused, regardless of whether an active round exists.
    pub fn get_protocol_status(env: Env) -> ProtocolStatus {
        if Self::is_paused(env.clone()) {
            ProtocolStatus::Paused
        } else if env.storage().persistent().has(&DataKey::ActiveRound) {
            ProtocolStatus::Active
        } else {
            ProtocolStatus::ClaimsOnly
        }
    }

    /// Returns the status of a specific round identified by `round_id`.
    ///
    /// Lookup strategy (in priority order):
    /// 1. If the round is the **current active round**, derive status from
    ///    ledger position relative to `bet_end_ledger` / `end_ledger`.
    /// 2. If the round appears in the **on-chain archive**, map its
    ///    [`RoundArchiveStatus`] to the corresponding terminal [`RoundStatus`].
    /// 3. If a `CancelledRound` marker exists (archive may be pruned),
    ///    return `Cancelled`.
    /// 4. Otherwise, return `Unknown`.
    ///
    /// | return value          | meaning                                                       |
    /// |-----------------------|---------------------------------------------------------------|
    /// | `Unknown`        (0)  | Round not found; never created or pruned from archive.       |
    /// | `Betting`        (1)  | Active; `ledger < bet_end_ledger`.                           |
    /// | `Running`        (2)  | Active; `bet_end_ledger ≤ ledger < end_ledger`.              |
    /// | `AwaitingResolve`(3)  | Active; `ledger ≥ end_ledger`, oracle not yet called.        |
    /// | `Resolved`       (4)  | Settled normally; pot distributed.                           |
    /// | `Cancelled`      (5)  | Admin-cancelled; stakes refunded.                            |
    /// | `FallbackRefund` (6)  | Settled with insufficient participants; stakes refunded.     |
    ///
    /// Note: `Betting`, `Running`, and `AwaitingResolve` are **derived** from
    /// ledger sequence — they do not involve additional storage writes.
    pub fn get_round_status(env: Env, round_id: u64) -> RoundStatus {
        // First check if it is the active round
        if let Some(active_round) = env
            .storage()
            .persistent()
            .get::<_, Round>(&DataKey::ActiveRound)
        {
            if active_round.round_id == round_id {
                let phase = Self::_derive_round_phase(env.ledger().sequence(), &active_round);
                return match phase {
                    RoundPhase::Betting => RoundStatus::Betting,
                    RoundPhase::Running => RoundStatus::Running,
                    RoundPhase::Resolvable => RoundStatus::AwaitingResolve,
                };
            }
        }
<<<<<<< HEAD
        env.storage().persistent().set(&DataKey::OracleStaleThreshold, &seconds);
        Ok(())
=======

        // Second, check the archived rounds summary
        let archive_key = DataKey::ArchivedRound(round_id);
        if let Some(archive) = env
            .storage()
            .persistent()
            .get::<_, ArchivedRoundSummary>(&archive_key)
        {
            return match archive.status {
                RoundArchiveStatus::Resolved => RoundStatus::Resolved,
                RoundArchiveStatus::Cancelled => RoundStatus::Cancelled,
                RoundArchiveStatus::FallbackRefund => RoundStatus::FallbackRefund,
            };
        }

        // Third, fallback check for cancelled rounds (in case it was pruned but CancelledRound flag remains)
        if Self::is_round_cancelled(env.clone(), round_id) {
            return RoundStatus::Cancelled;
        }

        // Otherwise, it's not active, not in archive, not cancelled.
        RoundStatus::Unknown
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    /// Returns the configured oracle stale threshold, or the default (3600 s) if not set.
    pub fn get_oracle_stale_threshold(env: Env) -> u64 {
        admin::get_oracle_stale_threshold(env)
    }

    // ─── Oracle rotation (two-step with expiry) ─────────────────────────────

    /// Proposes a new oracle address with an expiry window (admin only).
    ///
    /// The proposal must be accepted via [`Self::accept_oracle_rotation`] before
    /// `expires_in_seconds` elapses, otherwise acceptance is rejected.
    /// Minimum expiry is 60 seconds.
    ///
    /// Emits `("oracle", "propose")`.
    pub fn propose_oracle_rotation(
        env: Env,
        new_oracle: Address,
        expires_in_seconds: u64,
    ) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        if expires_in_seconds < MIN_ROTATION_EXPIRY_SECONDS {
            return Err(ContractError::InvalidStaleThreshold);
        }

        let proposed_at = env.ledger().timestamp();
        let expires_at = proposed_at
            .checked_add(expires_in_seconds)
            .ok_or(ContractError::Overflow)?;

        let proposal = OracleRotationProposal {
            new_oracle: new_oracle.clone(),
            proposed_at,
            expires_at,
        };

<<<<<<< HEAD
        env.storage().persistent().set(&DataKey::BetWindowLedgers, &bet_ledgers);
        env.storage().persistent().set(&DataKey::RunWindowLedgers, &run_ledgers);
=======
        let key = DataKey::OracleRotationProposal;
        env.storage().persistent().set(&key, &proposal);
        Self::_extend_persistent_ttl(&env, &key);
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b

        #[allow(deprecated)]
        env.events().publish(
<<<<<<< HEAD
            (symbol_short!("windows"), symbol_short!("updated")),
            (bet_ledgers, run_ledgers)
=======
            (symbol_short!("oracle"), symbol_short!("propose")),
            (new_oracle, expires_at),
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
        );

        Ok(())
    }

    /// Accepts a pending oracle rotation proposal before expiry (any caller).
    ///
    /// If the proposal has expired the call returns `RotationExpired` and the
    /// stale proposal is removed after emitting `("oracle", "expired")`.
    /// On success the stored oracle address is updated and
    /// `("oracle", "accept")` is emitted.
    pub fn accept_oracle_rotation(env: Env) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        Self::_ensure_not_paused(&env)?;

        let key = DataKey::OracleRotationProposal;
        let proposal: OracleRotationProposal = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::NoPendingRotation)?;

        let current_ts = env.ledger().timestamp();

        if current_ts > proposal.expires_at {
            env.storage().persistent().remove(&key);
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("oracle"), symbol_short!("expired")),
                (
                    proposal.new_oracle,
                    proposal.proposed_at,
                    proposal.expires_at,
                ),
            );
            return Err(ContractError::RotationExpired);
        }

        let oracle_key = DataKey::Oracle;
        let previous: Address = env
            .storage()
            .persistent()
            .get(&oracle_key)
            .ok_or(ContractError::OracleNotSet)?;

        env.storage()
            .persistent()
            .set(&oracle_key, &proposal.new_oracle);
        Self::_extend_persistent_ttl(&env, &oracle_key);
        env.storage().persistent().remove(&key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("oracle"), symbol_short!("accept")),
            (previous, proposal.new_oracle),
        );

        Ok(())
    }

    /// Cancels a pending oracle rotation proposal before it expires (admin only).
    ///
    /// Emits `("oracle", "cancel")` on success.
    pub fn cancel_oracle_rotation(env: Env) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        let key = DataKey::OracleRotationProposal;
        let proposal: OracleRotationProposal = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::NoPendingRotation)?;

        env.storage().persistent().remove(&key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("oracle"), symbol_short!("cancel")),
            (proposal.new_oracle,),
        );

        Ok(())
    }

    /// Returns the pending oracle rotation proposal, if any.
    pub fn get_oracle_rotation_proposal(env: Env) -> Option<OracleRotationProposal> {
        let key = DataKey::OracleRotationProposal;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    }

    /// Schedules a timelocked windows update (alias for [`Self::schedule_windows`]).
    /// bet_ledgers: Number of ledgers users can place bets
    /// run_ledgers: Total number of ledgers before round can be resolved
    pub fn set_windows(env: Env, bet_ledgers: u32, run_ledgers: u32) -> Result<(), ContractError> {
        config::set_windows(env, bet_ledgers, run_ledgers)
    }

    pub fn set_max_stake(env: Env, max_amount: Option<i128>) -> Result<(), ContractError> {
        config::set_max_stake(env, max_amount)
    }

    pub fn get_max_stake(env: Env) -> Option<i128> {
        config::get_max_stake(env)
    }

    pub fn set_max_user_exposure(
        env: Env,
        max_exposure: Option<i128>
    ) -> Result<(), ContractError> {
<<<<<<< HEAD
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        if let Some(v) = max_exposure {
            if v < MIN_CAP_VALUE {
                return Err(ContractError::InvalidBetAmount);
            }
            env.storage().persistent().set(&DataKey::MaxUserRoundExposure, &v);
        } else {
            env.storage().persistent().remove(&DataKey::MaxUserRoundExposure);
        }
        Ok(())
=======
        config::set_max_user_exposure(env, max_exposure)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    pub fn get_max_user_exposure(env: Env) -> Option<i128> {
<<<<<<< HEAD
        env.storage().persistent().get(&DataKey::MaxUserRoundExposure)
=======
        config::get_max_user_exposure(env)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    pub fn set_max_pending_winnings(
        env: Env,
        max_pending: Option<i128>
    ) -> Result<(), ContractError> {
        config::set_max_pending_winnings(env, max_pending)
    }

    pub fn schedule_windows(
        env: Env,
        bet_ledgers: u32,
        run_ledgers: u32,
    ) -> Result<(), ContractError> {
        config::schedule_windows(env, bet_ledgers, run_ledgers)
    }

    pub fn schedule_max_stake(env: Env, max_amount: Option<i128>) -> Result<(), ContractError> {
        config::schedule_max_stake(env, max_amount)
    }

    pub fn schedule_max_user_exposure(
        env: Env,
        max_exposure: Option<i128>,
    ) -> Result<(), ContractError> {
        config::schedule_max_user_exposure(env, max_exposure)
    }

    pub fn schedule_max_pending_winnings(
        env: Env,
        max_pending: Option<i128>,
    ) -> Result<(), ContractError> {
        config::schedule_max_pending_winnings(env, max_pending)
    }

    pub fn schedule_oracle_stale_threshold(env: Env, seconds: u64) -> Result<(), ContractError> {
        config::schedule_oracle_stale_threshold(env, seconds)
    }

    pub fn schedule_oracle_deviation_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
        config::schedule_oracle_deviation_bps(env, bps)
    }

    pub fn schedule_protocol_fee_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
        config::schedule_protocol_fee_bps(env, bps)
    }

    pub fn set_protocol_fee_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
        config::set_protocol_fee_bps(env, bps)
    }

    pub fn get_protocol_fee_bps(env: Env) -> Option<u32> {
        config::get_protocol_fee_bps(env)
    }

    pub fn get_protocol_fee_treasury(env: Env) -> i128 {
        config::get_protocol_fee_treasury(env)
    }

    pub fn withdraw_protocol_fee(
        env: Env,
        recipient: Address,
        amount: i128,
    ) -> Result<i128, ContractError> {
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env).map_err(|e| {
            Self::_emit_action_rejected(&env, &admin, symbol_short!("withdraw"), e);
            e
        })?;

<<<<<<< HEAD
        if let Some(v) = max_pending {
            if v < MIN_CAP_VALUE {
                return Err(ContractError::InvalidBetAmount);
            }
            env.storage().persistent().set(&DataKey::MaxPendingWinnings, &v);
        } else {
            env.storage().persistent().remove(&DataKey::MaxPendingWinnings);
=======
        if amount <= 0 {
            return Err(ContractError::InvalidBetAmount);
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
        }

        let treasury_key = DataKey::ProtocolFeeTreasury;
        let current: i128 = env.storage().persistent().get(&treasury_key).unwrap_or(0);
        let new_treasury = current
            .checked_sub(amount)
            .ok_or(ContractError::Overflow)?;
        env.storage().persistent().set(&treasury_key, &new_treasury);
        Self::_extend_persistent_ttl(&env, &treasury_key);

        // Credit recipient — reuse the existing balance helper. create a
        // balance row if recipient has none yet (treasury recipient may
        // not have minted).
        let recipient_bal: i128 = Self::balance(env.clone(), recipient.clone());
        let new_bal = Self::payout_add(recipient_bal, amount)?;
        Self::_set_balance(&env, recipient.clone(), new_bal);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("protocol"), symbol_short!("withdrawn")),
            (recipient, amount, new_treasury),
        );

        Ok(amount)
        config::withdraw_protocol_fee(env, recipient, amount)
    }

    pub fn get_pending_config_change(
        env: Env,
        kind: ConfigChangeKind,
    ) -> Option<PendingConfigChange> {
        config::get_pending_config_change(env, kind)
    }

    pub fn apply_scheduled_changes(env: Env, kind: ConfigChangeKind) -> Result<(), ContractError> {
        config::apply_scheduled_changes(env, kind)
    }

    pub fn cancel_config_change(env: Env, kind: ConfigChangeKind) -> Result<(), ContractError> {
        config::cancel_config_change(env, kind)
    }

    pub fn get_max_pending_winnings(env: Env) -> Option<i128> {
        config::get_max_pending_winnings(env)
    }

    pub fn set_min_participants(env: Env, min: Option<u32>) -> Result<(), ContractError> {
<<<<<<< HEAD
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        if let Some(v) = min {
            if v == 0 || v > MAX_MIN_PARTICIPANTS {
                return Err(ContractError::InvalidMinParticipants);
            }
            env.storage().persistent().set(&DataKey::MinParticipants, &v);
        } else {
            env.storage().persistent().remove(&DataKey::MinParticipants);
        }
        Ok(())
=======
        config::set_min_participants(env, min)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    pub fn get_min_participants(env: Env) -> Option<u32> {
        config::get_min_participants(env)
    }

    pub fn set_max_precision_participants(env: Env, max: u32) -> Result<(), ContractError> {
        config::set_max_precision_participants(env, max)
    }

    pub fn get_max_precision_participants(env: Env) -> u32 {
        config::get_max_precision_participants(env)
    }

    pub fn set_mint_limit(env: Env, limit: u32) -> Result<(), ContractError> {
        config::set_mint_limit(env, limit)
    }

    pub fn get_mint_limit(env: Env) -> u32 {
        config::get_mint_limit(env)
    }

    pub fn set_archive_retention(env: Env, limit: u32) -> Result<(), ContractError> {
        config::set_archive_retention(env, limit)
    }

    pub fn get_archive_retention(env: Env) -> u32 {
        config::get_archive_retention(env)
    }

    /// Creates a new prediction round (admin only)
    pub fn create_round(
        env: Env,
        start_price: u128,
        mode: Option<u32>,
    ) -> Result<(), ContractError> {
        betting::create_round(env, start_price, mode)
    }

    pub fn place_bet(
        env: Env,
        user: Address,
        amount: i128,
        side: BetSide
    ) -> Result<(), ContractError> {
<<<<<<< HEAD
        Self::_require_supported_schema(&env)?;
        user.require_auth();
        Self::_ensure_not_paused(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidBetAmount);
        }

        // Enforce max stake cap (Issue #113)
        if let Some(max_stake) = env.storage().persistent().get::<_, i128>(&DataKey::MaxStake) {
            if amount > max_stake {
                return Err(ContractError::StakeExceedsMax);
            }
        }

        // Single read of the active round — cache in call scope
        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .ok_or(ContractError::NoActiveRound)?;

        // Enforce per-user round exposure cap (Issue #113)
        if
            let Some(max_exposure) = env
                .storage()
                .persistent()
                .get::<_, i128>(&DataKey::MaxUserRoundExposure)
        {
            if amount > max_exposure {
                return Err(ContractError::ExposureCapExceeded);
            }
        }

        // Verify round is in Up/Down mode
        if round.mode != RoundMode::UpDown {
            return Err(ContractError::WrongModeForPrediction);
        }

        let current_ledger = env.ledger().sequence();
        if current_ledger >= round.bet_end_ledger {
            return Err(ContractError::RoundEnded);
        }

        let user_balance = Self::balance(env.clone(), user.clone());
        if user_balance < amount {
            return Err(ContractError::InsufficientBalance);
        }

        // O(1) duplicate-bet check — read one small key, not the full map
        let pos_key = DataKey::Position(round.round_id, user.clone());
        if env.storage().persistent().has(&pos_key) {
            return Err(ContractError::AlreadyBet);
        }

        // Deduct balance
        let new_balance = user_balance.checked_sub(amount).ok_or(ContractError::Overflow)?;
        Self::_set_balance(&env, user.clone(), new_balance);

        // Write single-user position key — O(1), constant-size entry
        let position = UserPosition {
            amount,
            side: side.clone(),
        };
        env.storage().persistent().set(&pos_key, &position);

        // Append to participant list (needed for O(n) resolution iteration)
        let participants_key = DataKey::RoundParticipants(round.round_id);
        let mut participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&participants_key)
            .unwrap_or(Vec::new(&env));
        participants.push_back(user.clone());
        env.storage().persistent().set(&participants_key, &participants);

        // Update cached round pools and write once
        match side {
            BetSide::Up => {
                round.pool_up = round.pool_up.checked_add(amount).ok_or(ContractError::Overflow)?;
            }
            BetSide::Down => {
                round.pool_down = round.pool_down
                    .checked_add(amount)
                    .ok_or(ContractError::Overflow)?;
            }
        }
        env.storage().persistent().set(&DataKey::ActiveRound, &round);

        // Emit bet placed event
        // Topic: ("bet", "placed")
        // Payload: (user: Address, round_id: u64, amount: i128, side: u32 where 0=Up, 1=Down)
        let side_value: u32 = match side {
            BetSide::Up => 0,
            BetSide::Down => 1,
        };
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("bet"), symbol_short!("placed")),
            (user, round.round_id, amount, side_value)
        );

        Ok(())
=======
        betting::place_bet(env, user, amount, side)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    pub fn place_precision_prediction(
        env: Env,
        user: Address,
        amount: i128,
        predicted_price: u128
    ) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        user.require_auth();
        Self::_ensure_normal_mode(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidBetAmount);
        }

        // Enforce max stake cap (Issue #113)
        if let Some(max_stake) = env.storage().persistent().get::<_, i128>(&DataKey::MaxStake) {
            if amount > max_stake {
                return Err(ContractError::StakeExceedsMax);
            }
        }

        // Validate price scale (must be 4 decimal places, max value 9999 for 0.9999)
        // Reasonable max: 99999999 (9999.9999 XLM)
        if predicted_price > 99_999_999 {
            return Err(ContractError::InvalidPrice);
        }

        // Single read of the active round — cache in call scope
        let round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .ok_or(ContractError::NoActiveRound)?;

        // Enforce per-user round exposure cap (Issue #113)
        if
            let Some(max_exposure) = env
                .storage()
                .persistent()
                .get::<_, i128>(&DataKey::MaxUserRoundExposure)
        {
            if amount > max_exposure {
                return Err(ContractError::ExposureCapExceeded);
            }
        }

        // Verify round is in Precision mode
        if round.mode != RoundMode::Precision {
            return Err(ContractError::WrongModeForPrediction);
        }

        let current_ledger = env.ledger().sequence();
        if current_ledger >= round.bet_end_ledger {
            return Err(ContractError::RoundEnded);
        }

        // O(1) duplicate-prediction check — single composite key read
        let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
        let commit_key = DataKey::PrecisionCommitment(round.round_id, user.clone());
        if env.storage().persistent().has(&pred_key) || env.storage().persistent().has(&commit_key) {
            return Err(ContractError::AlreadyBet);
        }

        let participants_key = DataKey::RoundParticipants(round.round_id);
        let mut participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&participants_key)
            .unwrap_or(Vec::new(&env));
        let max_precision_participants = Self::get_max_precision_participants(env.clone());
        if participants.len() >= max_precision_participants {
            return Err(ContractError::PrecisionParticipantCapExceeded);
        }

        let user_balance = Self::balance(env.clone(), user.clone());
        if user_balance < amount {
            return Err(ContractError::InsufficientBalance);
        }

        // Deduct balance
        let new_balance = user_balance.checked_sub(amount).ok_or(ContractError::Overflow)?;
        Self::_set_balance(&env, user.clone(), new_balance);

        // Write single-user prediction key — O(1), constant-size entry
        let prediction = PrecisionPrediction {
            user: user.clone(),
            predicted_price,
            amount,
        };
        env.storage().persistent().set(&pred_key, &prediction);

        // Append to shared participant list
        participants.push_back(user.clone());
        env.storage().persistent().set(&participants_key, &participants);

        // Emit event for precision prediction
        // Topic: ("predict", "price")
        // Payload: (user: Address, round_id: u64, predicted_price: u128, amount: i128)
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("predict"), symbol_short!("price")),
            (user, round.round_id, predicted_price, amount)
        );

        Ok(())
        betting::place_precision_prediction(env, user, amount, predicted_price)
    }

    pub fn predict_price(
        env: Env,
        user: Address,
        guessed_price: u128,
        amount: i128
    ) -> Result<(), ContractError> {
        betting::predict_price(env, user, guessed_price, amount)
    }

    pub fn commit_prediction(
        env: Env,
        user: Address,
        hash: BytesN<32>,
        amount: i128
    ) -> Result<(), ContractError> {
<<<<<<< HEAD
        user.require_auth();
        Self::_ensure_not_paused(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidBetAmount);
        }

        // Enforce max stake cap
        if let Some(max_stake) = env.storage().persistent().get::<_, i128>(&DataKey::MaxStake) {
            if amount > max_stake {
                return Err(ContractError::StakeExceedsMax);
            }
        }

        // Single read of the active round
        let round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .ok_or(ContractError::NoActiveRound)?;

        // Enforce per-user round exposure cap
        if
            let Some(max_exposure) = env
                .storage()
                .persistent()
                .get::<_, i128>(&DataKey::MaxUserRoundExposure)
        {
            if amount > max_exposure {
                return Err(ContractError::ExposureCapExceeded);
            }
        }

        // Verify round is in Precision mode
        if round.mode != RoundMode::Precision {
            return Err(ContractError::WrongModeForPrediction);
        }

        let current_ledger = env.ledger().sequence();
        if current_ledger >= round.bet_end_ledger {
            return Err(ContractError::RoundEnded);
        }

        let user_balance = Self::balance(env.clone(), user.clone());
        if user_balance < amount {
            return Err(ContractError::InsufficientBalance);
        }

        // Check duplicate bet or commitment
        let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
        let commit_key = DataKey::PrecisionCommitment(round.round_id, user.clone());
        if env.storage().persistent().has(&pred_key) || env.storage().persistent().has(&commit_key) {
            return Err(ContractError::AlreadyBet);
        }

        // Deduct balance
        let new_balance = user_balance.checked_sub(amount).ok_or(ContractError::Overflow)?;
        Self::_set_balance(&env, user.clone(), new_balance);

        // Store commitment
        let commitment = PrecisionCommitment {
            hash: hash.clone(),
            amount,
            revealed: false,
        };
        env.storage().persistent().set(&commit_key, &commitment);

        // Append to shared participant list
        let participants_key = DataKey::RoundParticipants(round.round_id);
        let mut participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&participants_key)
            .unwrap_or(Vec::new(&env));
        participants.push_back(user.clone());
        env.storage().persistent().set(&participants_key, &participants);

        // Emit commit prediction event
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("commit"), symbol_short!("predict")),
            (user, round.round_id, hash, amount)
        );

        Ok(())
=======
        betting::commit_prediction(env, user, hash, amount)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    pub fn reveal_prediction(
        env: Env,
        user: Address,
        predicted_price: u128,
        salt: BytesN<32>
    ) -> Result<(), ContractError> {
<<<<<<< HEAD
        user.require_auth();
        Self::_ensure_not_paused(&env)?;

        // Single read of the active round
        let round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .ok_or(ContractError::NoActiveRound)?;

        // Verify round is in Precision mode
        if round.mode != RoundMode::Precision {
            return Err(ContractError::WrongModeForPrediction);
        }

        // Enforce reveal window: bet_end_ledger <= ledger < end_ledger
        let current_ledger = env.ledger().sequence();
        if current_ledger < round.bet_end_ledger || current_ledger >= round.end_ledger {
            return Err(ContractError::InvalidRevealWindow);
        }

        // Retrieve commitment
        let commit_key = DataKey::PrecisionCommitment(round.round_id, user.clone());
        let mut commitment: PrecisionCommitment = env
            .storage()
            .persistent()
            .get(&commit_key)
            .ok_or(ContractError::CommitmentNotFound)?;

        if commitment.revealed {
            return Err(ContractError::AlreadyRevealed);
        }

        // Verify hash
        let mut preimage = Bytes::new(&env);
        preimage.append(&predicted_price.to_xdr(&env));
        preimage.append(&salt.to_xdr(&env));
        let computed_hash = env.crypto().sha256(&preimage);
        let computed_hash_bytes: BytesN<32> = computed_hash.into();

        if computed_hash_bytes != commitment.hash {
            return Err(ContractError::HashMismatch);
        }

        // Mark revealed and write
        commitment.revealed = true;
        env.storage().persistent().set(&commit_key, &commitment);

        // Store prediction for resolution
        let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
        let prediction = PrecisionPrediction {
            user: user.clone(),
            predicted_price,
            amount: commitment.amount,
        };
        env.storage().persistent().set(&pred_key, &prediction);

        // Emit reveal prediction event
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("reveal"), symbol_short!("predict")),
            (user, round.round_id, predicted_price, commitment.amount)
        );

        Ok(())
=======
        betting::reveal_prediction(env, user, predicted_price, salt)
    }

    /// Mints 1000 vXLM for new users (one-time only)
    pub fn mint_initial(env: Env, user: Address) -> i128 {
        betting::mint_initial(env, user)
    }

    pub fn resolve_round(env: Env, payload: OraclePayload) -> Result<(), ContractError> {
        settlement::resolve_round(env, payload)
    }

    pub fn cancel_round(env: Env, reason: u32) -> Result<(), ContractError> {
        settlement::cancel_round(env, reason)
    }

    pub fn is_round_cancelled(env: Env, round_id: u64) -> bool {
        settlement::is_round_cancelled(env, round_id)
    }

    pub fn claim_winnings(env: Env, user: Address) -> Result<i128, ContractError> {
        settlement::claim_winnings(env, user)
    }

    pub fn get_active_round(env: Env) -> Option<Round> {
        queries::get_active_round(env)
    }

    pub fn get_round_pool_stats(env: Env) -> Option<RoundPoolStats> {
        queries::get_round_pool_stats(env)
    }

    pub fn get_round_phase(env: Env) -> Result<RoundPhase, ContractError> {
        queries::get_round_phase(env)
    }

    pub fn get_last_round_id(env: Env) -> u64 {
        queries::get_last_round_id(env)
    }

    pub fn get_archived_round(env: Env, round_id: u64) -> Option<ArchivedRoundSummary> {
        queries::get_archived_round(env, round_id)
    }

    pub fn get_recent_archived_rounds(env: Env, limit: u32) -> Vec<ArchivedRoundSummary> {
        queries::get_recent_archived_rounds(env, limit)
    }

    pub fn get_user_archived_participation(
        env: Env,
        user: Address,
        round_id: u64,
    ) -> Option<UserRoundOutcome> {
        queries::get_user_archived_participation(env, user, round_id)
    }

    pub fn get_user_stats(env: Env, user: Address) -> UserStats {
        queries::get_user_stats(env, user)
    }

    pub fn get_pending_winnings(env: Env, user: Address) -> i128 {
        queries::get_pending_winnings(env, user)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    pub fn get_user_position(env: Env, user: Address) -> Option<UserPosition> {
<<<<<<< HEAD
        if let Some(round) = env.storage().persistent().get::<_, Round>(&DataKey::ActiveRound) {
            let pos_key = DataKey::Position(round.round_id, user.clone());
            if let Some(pos) = env.storage().persistent().get(&pos_key) {
                return Some(pos);
            }
        }

        // Legacy read-only fallback for migration data
        let legacy_updown: Map<Address, UserPosition> = env
            .storage()
            .persistent()
            .get(&DataKey::UpDownPositions)
            .unwrap_or(Map::new(&env));
        if let Some(p) = legacy_updown.get(user.clone()) {
            return Some(p);
        }
        let legacy_positions: Map<Address, UserPosition> = env
            .storage()
            .persistent()
            .get(&DataKey::Positions)
            .unwrap_or(Map::new(&env));
        legacy_positions.get(user)
=======
        queries::get_user_position(env, user)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    pub fn get_user_precision_prediction(env: Env, user: Address) -> Option<PrecisionPrediction> {
<<<<<<< HEAD
        if let Some(round) = env.storage().persistent().get::<_, Round>(&DataKey::ActiveRound) {
            let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
            if let Some(p) = env.storage().persistent().get::<_, PrecisionPrediction>(&pred_key) {
                return Some(p);
            }
        }
        let legacy: Map<Address, PrecisionPrediction> = env
            .storage()
            .persistent()
            .get(&DataKey::PrecisionPositions)
            .unwrap_or(Map::new(&env));
        legacy.get(user)
=======
        queries::get_user_precision_prediction(env, user)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    pub fn get_precision_predictions(env: Env) -> Vec<PrecisionPrediction> {
<<<<<<< HEAD
        let round = match env.storage().persistent().get::<_, Round>(&DataKey::ActiveRound) {
            Some(r) => r,
            None => {
                return Vec::new(&env);
            }
        };

        let participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundParticipants(round.round_id))
            .unwrap_or(Vec::new(&env));

        let mut result: Vec<PrecisionPrediction> = Vec::new(&env);
        for i in 0..participants.len() {
            if let Some(user) = participants.get(i) {
                let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
                if let Some(pred) = env.storage().persistent().get(&pred_key) {
                    result.push_back(pred);
                }
            }
        }

        // Legacy fallback: pre-migration data lives in the bulk map
        if result.is_empty() {
            let legacy: Map<Address, PrecisionPrediction> = env
                .storage()
                .persistent()
                .get(&DataKey::PrecisionPositions)
                .unwrap_or(Map::new(&env));
            return legacy.values();
        }
        result
=======
        queries::get_precision_predictions(env)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    }

    pub fn get_updown_positions(env: Env) -> Map<Address, UserPosition> {
<<<<<<< HEAD
        let round = match env.storage().persistent().get::<_, Round>(&DataKey::ActiveRound) {
            Some(r) => r,
            None => {
                return Map::new(&env);
            }
        };
=======
        queries::get_updown_positions(env)
    }

    pub fn get_precision_predictions_page(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Vec<PrecisionPrediction> {
        queries::get_precision_predictions_page(env, offset, limit)
    }

    pub fn get_updown_positions_page(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Vec<(Address, UserPosition)> {
        queries::get_updown_positions_page(env, offset, limit)
    }

    /// Returns user's vXLM balance
    pub fn balance(env: Env, user: Address) -> i128 {
        common::balance(env, user)
    }
}
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b

        let participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundParticipants(round.round_id))
            .unwrap_or(Vec::new(&env));
        let participants = Self::sort_addresses(participants);

        let total = participants.len();
        if offset >= total {
            return Vec::new(&env);
        }

        let end = offset.saturating_add(limit).min(total);

        let mut result: Vec<(Address, UserPosition)> = Vec::new(&env);
        for i in offset..end {
            if let Some(user) = participants.get(i) {
                let pos_key = DataKey::Position(round.round_id, user.clone());
                if let Some(pos) = env.storage().persistent().get(&pos_key) {
                    result.push_back((user, pos));
                }
            }
        }

        result
    }

    /// Resolves the round with oracle payload (oracle only)
    /// Mode 0 (Up/Down): Winners split losers' pool proportionally; ties get refunds
    /// Mode 1 (Precision/Legends): Closest guess wins full pot; ties split evenly
    pub fn resolve_round(env: Env, payload: OraclePayload) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        if payload.price == 0 {
            return Err(ContractError::InvalidPrice);
        }

        Self::_extend_persistent_ttl(&env, &DataKey::Oracle);
        let oracle: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Oracle)
            .ok_or(ContractError::OracleNotSet)?;

        oracle.require_auth();
        Self::_ensure_not_paused(&env).map_err(|e| {
            Self::_emit_action_rejected(&env, &oracle, symbol_short!("resolve"), e);
            e
        })?;

        let round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .ok_or(ContractError::NoActiveRound)?;

        // Verify round ID matches to prevent cross-round replays
        if payload.round_id != round.start_ledger {
            Self::_emit_action_rejected(
                &env,
                &oracle,
                symbol_short!("resolve"),
                ContractError::InvalidOracleRound,
            );
            return Err(ContractError::InvalidOracleRound);
        }

<<<<<<< HEAD
        // Per-round nonce replay guard (Issue #118).
        // Round-ID checks already block cross-round replays; this additionally
        // makes resolution idempotent against accidental duplicate submissions
        // of the same payload within a round.
        let nonce_key = DataKey::ConsumedOracleNonce(round.round_id, payload.nonce);
        if env.storage().persistent().has(&nonce_key) {
            return Err(ContractError::OracleNonceReused);
        }

        // Verify the oracle timestamp is inside the deterministic round-safe window:
        // [round_start_timestamp, current_ledger_timestamp], with a 300 second max age.
=======
        // ─── Domain-context validation (Issue #143) ─────────────────────────
        // Reject payloads targeting a different network or contract deployment.
        if payload.network_id != env.ledger().network_id() {
            Self::_emit_action_rejected(
                &env,
                &oracle,
                symbol_short!("resolve"),
                ContractError::OracleNetworkMismatch,
            );
            return Err(ContractError::OracleNetworkMismatch);
        }
        if payload.contract_addr != env.current_contract_address() {
            Self::_emit_action_rejected(
                &env,
                &oracle,
                symbol_short!("resolve"),
                ContractError::OracleContractMismatch,
            );
            return Err(ContractError::OracleContractMismatch);
        }

>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
        // Verify data freshness (max 300 seconds / 5 minutes old)
        let current_time = env.ledger().timestamp();
        let round_start_timestamp: u64 = env
            .storage()
            .persistent()
            .get(&Self::round_start_timestamp_key(round.round_id))
            .unwrap_or(0);

        if payload.timestamp < round_start_timestamp {
            return Err(ContractError::OracleDataBeforeRound);
        }

        // Reject future timestamps to prevent time-skew manipulation
        if payload.timestamp > current_time {
            Self::_emit_action_rejected(
                &env,
                &oracle,
                symbol_short!("resolve"),
                ContractError::FutureOracleData,
            );
            return Err(ContractError::FutureOracleData);
        }

<<<<<<< HEAD
        if current_time.saturating_sub(payload.timestamp) > ORACLE_MAX_DATA_AGE_SECONDS {
=======
        if current_time > payload.timestamp + 300 {
            Self::_emit_action_rejected(
                &env,
                &oracle,
                symbol_short!("resolve"),
                ContractError::StaleOracleData,
            );
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
            return Err(ContractError::StaleOracleData);
        }

        // ─── Oracle deviation guardrails (circuit-breaker) ───────────────────
        // Compare settlement price against round start price (trusted baseline).
        // If configured, reject large jumps unless an admin-armed one-shot override is set.
<<<<<<< HEAD
        if
            let Some(max_bps) = env
                .storage()
                .persistent()
                .get::<_, u32>(&DataKey::OracleMaxDeviationBps)
=======
        Self::_extend_persistent_ttl(&env, &DataKey::OracleMaxDeviationBps);
        if let Some(max_bps) = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::OracleMaxDeviationBps)
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
        {
            let start_price = round.price_start;
            // start_price is validated at round creation; still guard division by zero.
            if start_price == 0 {
                return Err(ContractError::InvalidPrice);
            }

            let diff = if payload.price >= start_price {
                payload.price.checked_sub(start_price).ok_or(ContractError::Overflow)?
            } else {
                start_price.checked_sub(payload.price).ok_or(ContractError::Overflow)?
            };

            // Integer bps: floor(diff / start) * 10_000.
            // Use checked math so any u128 overflow maps to explicit errors.
            let diff_bps_u128 =
                diff.checked_mul(10_000u128).ok_or(ContractError::Overflow)? / start_price;
            let diff_bps: u32 = diff_bps_u128.try_into().map_err(|_| ContractError::Overflow)?;

            let override_armed: bool = env
                .storage()
                .persistent()
                .get(&DataKey::OracleDeviationOverrideArmed)
                .unwrap_or(false);

            if diff_bps > max_bps && !override_armed {
                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("oracle"), symbol_short!("rejected")),
                    (round.round_id, start_price, payload.price, diff_bps, max_bps)
                );
                return Err(ContractError::OracleDeviationExceeded);
            }

            if diff_bps > max_bps && override_armed {
                // One-shot override is consumed on use.
                env.storage().persistent().remove(&DataKey::OracleDeviationOverrideArmed);

                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("oracle"), symbol_short!("override")),
                    (round.round_id, start_price, payload.price, diff_bps, max_bps)
                );
            }
        }

        // ─── Oracle confidence guardrails ────────────────────────────────────────
        Self::_extend_persistent_ttl(&env, &DataKey::OracleMinConfidenceBps);
        Self::_extend_persistent_ttl(&env, &DataKey::OracleStrictMode);
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
        // Per-round nonce replay guard (Issue #118).
        // Consume the nonce only after all validation passes so a rejected payload
        // doesn't permanently burn a nonce value.
        let nonce_key = DataKey::ConsumedOracleNonce(round.round_id, payload.nonce);
        if env.storage().persistent().has(&nonce_key) {
            Self::_emit_action_rejected(
                &env,
                &oracle,
                symbol_short!("resolve"),
                ContractError::OracleNonceReused,
            );
            return Err(ContractError::OracleNonceReused);
        }
        env.storage().persistent().set(&nonce_key, &true);

        // Verify round has reached end_ledger
        let current_ledger = env.ledger().sequence();
        if current_ledger < round.end_ledger {
            Self::_emit_action_rejected(
                &env,
                &oracle,
                symbol_short!("resolve"),
                ContractError::RoundNotEnded,
            );
            return Err(ContractError::RoundNotEnded);
        }

        // Record the consumed nonce only after all pre-settlement validation succeeds.
        env.storage().persistent().set(&nonce_key, &true);

        // Store round ID before cleaning up
        let round_id = round.round_id;

        // ─── Minimum participants threshold check ────────────────────────────
        if let Some(min) = env.storage().persistent().get::<_, u32>(&DataKey::MinParticipants) {
            let threshold_participants: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::RoundParticipants(round_id))
                .unwrap_or(Vec::new(&env));
            let count = threshold_participants.len();
            if count < min {
                Self::_archive_round(
                    &env,
                    &round,
                    RoundArchiveStatus::FallbackRefund,
                    payload.price,
                    count,
                );
                Self::_refund_under_threshold(&env, &round, &threshold_participants)?;
                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("round"), symbol_short!("fallback")),
                    (round_id, count, min)
                );
                return Ok(());
            }
        }

        // Branch based on round mode
        match round.mode {
            RoundMode::UpDown => {
                let one_sided = Self::_resolve_updown_mode(&env, &round, payload.price)?;
                if one_sided {
                    // Emit here (public scope, env: Env) so the event is captured in tests.
                    #[allow(deprecated)]
                    env.events().publish(
                        (symbol_short!("pool"), symbol_short!("onesided")),
                        (round_id, round.pool_up, round.pool_down)
                    );
                }
            }
            RoundMode::Precision => {
                Self::_resolve_precision_mode(&env, round_id, payload.price)?;
            }
        }

        // Clean up indexed position keys and participant list
        let participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundParticipants(round_id))
            .unwrap_or(Vec::new(&env));
        let participant_count = participants.len();

        Self::_archive_round(
            &env,
            &round,
            RoundArchiveStatus::Resolved,
            payload.price,
            participant_count,
        );

        for i in 0..participants.len() {
            if let Some(user) = participants.get(i) {
                env.storage().persistent().remove(&DataKey::Position(round_id, user.clone()));
                env.storage()
                    .persistent()
                    .remove(&DataKey::PrecisionPosition(round_id, user.clone()));
                env.storage().persistent().remove(&DataKey::PrecisionCommitment(round_id, user));
            }
        }
        env.storage().persistent().remove(&DataKey::RoundParticipants(round_id));

        // Clean up legacy map keys if present (migration compat)
        env.storage().persistent().remove(&DataKey::ActiveRound);
        env.storage().persistent().remove(&Self::round_start_timestamp_key(round_id));
        env.storage().persistent().remove(&DataKey::Positions);
        env.storage().persistent().remove(&DataKey::UpDownPositions);
        env.storage().persistent().remove(&DataKey::PrecisionPositions);

        // Emit resolution event with round ID, price, and mode
        // Topic: ("round", "resolved")
        // Payload: (round_id: u64, final_price: u128, mode: u32 where 0=UpDown, 1=Precision)
        let mode_value: u32 = match round.mode {
            RoundMode::UpDown => 0,
            RoundMode::Precision => 1,
        };
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("round"), symbol_short!("resolved")),
<<<<<<< HEAD
            (round_id, payload.price, mode_value)
=======
            (round_id, payload.price, mode_value, payload.confidence),
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
        );

        Ok(())
    }

    /// Resolves Up/Down mode round using indexed per-user position keys.
    ///
    /// Reads: 1 (participants list) + N (individual positions).
    /// Migration fallback: if the participant list is empty but the legacy
    /// `UpDownPositions` map is present, the resolver iterates the legacy map
    /// — preserves correctness for any in-flight pre-migration round.
    /// Returns `true` when a one-sided pool was detected (winning side exists but
    /// losing pool is empty). The caller is responsible for emitting the event.
    fn _resolve_updown_mode(
        env: &Env,
        round: &Round,
        final_price: u128
    ) -> Result<bool, ContractError> {
        let participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundParticipants(round.round_id))
            .unwrap_or(Vec::new(env));
        let participants = Self::sort_addresses(participants);

        let price_went_up = final_price > round.price_start;
        let price_went_down = final_price < round.price_start;
        let price_unchanged = final_price == round.price_start;

        // One-sided liquidity: winning side exists but losing pool is empty.
        // Policy: refund all participants — no fund loss, transparent outcome.
        let is_one_sided =
            (price_went_up && round.pool_down == 0 && round.pool_up > 0) ||
            (price_went_down && round.pool_up == 0 && round.pool_down > 0);

        if !participants.is_empty() {
            if price_unchanged || is_one_sided {
                Self::_record_refunds_indexed(env, round.round_id, 0, &participants)?;
            } else if price_went_up {
                Self::_record_winnings_indexed(
                    env,
                    round.round_id,
                    &participants,
                    BetSide::Up,
                    round.pool_up,
                    round.pool_down
                )?;
            } else if price_went_down {
                Self::_record_winnings_indexed(
                    env,
                    round.round_id,
                    &participants,
                    BetSide::Down,
                    round.pool_down,
                    round.pool_up
                )?;
            }
        } else {
            // Migration fallback: legacy single-map layout
            let positions: Map<Address, UserPosition> = env
                .storage()
                .persistent()
                .get(&DataKey::UpDownPositions)
                .unwrap_or(Map::new(env));
            if !positions.is_empty() {
                if price_unchanged {
                    Self::_record_refunds_legacy(env, round.round_id, &positions)?;
                } else if price_went_up {
                    Self::_record_winnings_legacy(
                        env,
                        round.round_id,
                        &positions,
                        BetSide::Up,
                        round.pool_up,
                        round.pool_down
                    )?;
                } else if price_went_down {
                    Self::_record_winnings_legacy(
                        env,
                        round.round_id,
                        &positions,
                        BetSide::Down,
                        round.pool_down,
                        round.pool_up
                    )?;
                }
            }
        }

        Ok(is_one_sided)
    }

    /// Legacy refund path — reads the bulk Map blob.
    /// Used only when migrating pre-existing rounds; new rounds use indexed keys.
    fn _record_refunds_legacy(
        env: &Env,
<<<<<<< HEAD
        positions: &Map<Address, UserPosition>
=======
        round_id: u64,
        positions: &Map<Address, UserPosition>,
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    ) -> Result<(), ContractError> {
        let keys: Vec<Address> = positions.keys();
        for i in 0..keys.len() {
            if let Some(user) = keys.get(i) {
                if let Some(position) = positions.get(user.clone()) {
                    Self::_accumulate_pending(env, user.clone(), position.amount)?;
                    let prediction_side = match position.side {
                        BetSide::Up => 0,
                        BetSide::Down => 1,
                    };
                    Self::_persist_user_outcome(
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

    /// Legacy winnings path — reads the bulk Map blob.
    ///
    /// `round_id` is threaded in so that the per-loser `("outcome", "loss")`
    /// observability event (Issue #168) emitted in the loser branch can carry
    /// the correct round identifier without an extra storage read.
    fn _record_winnings_legacy(
        env: &Env,
        round_id: u64,
        positions: &Map<Address, UserPosition>,
        winning_side: BetSide,
        winning_pool: i128,
        losing_pool: i128
    ) -> Result<(), ContractError> {
        if winning_pool == 0 {
            return Ok(());
        }

        // Apply protocol fee (Issue #162); see `_record_winnings_indexed`.
        let (winning_pool, losing_pool, _fee_amount) =
            Self::_apply_protocol_fee_updown(env, round_id, winning_pool, losing_pool)?;

        let keys: Vec<Address> = positions.keys();
        for i in 0..keys.len() {
            if let Some(user) = keys.get(i) {
                if let Some(position) = positions.get(user.clone()) {
                    if position.side == winning_side {
<<<<<<< HEAD
                        let share_numerator = position.amount
                            .checked_mul(losing_pool)
                            .ok_or(ContractError::Overflow)?;
                        let share = share_numerator / winning_pool;
                        let payout = position.amount
                            .checked_add(share)
                            .ok_or(ContractError::Overflow)?;
=======
                        // Compute all payout math before any storage write
                        let share_numerator = Self::payout_mul(position.amount, losing_pool)?;
                        let share = share_numerator / winning_pool;
                        let payout = Self::payout_add(position.amount, share)?;
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b

                        Self::_accumulate_pending(env, user.clone(), payout)?;
                        Self::_update_stats_win(env, user.clone())?;

                        let side_value = match position.side {
                            BetSide::Up => 0,
                            BetSide::Down => 1,
                        };
                        Self::_persist_user_outcome(
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
                        // Emit outcome loss event for UpDown loser (Issue #168).
                        // Topic: ("outcome", "loss")
                        // Payload: (user, round_id, mode=0=UpDown, amount, side, predicted_price=0)
                        // `predicted_price` is fixed at 0 for UpDown since this
                        // field is only meaningful in Precision mode.
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
                        Self::_update_stats_loss(env, user.clone())?;

                        Self::_persist_user_outcome(
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

    fn _resolve_precision_mode(
        env: &Env,
        round_id: u64,
        final_price: u128
    ) -> Result<(), ContractError> {
        let mut participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundParticipants(round_id))
            .unwrap_or(Vec::new(env));
        participants = Self::sort_addresses(participants);

        if participants.is_empty() {
            // Migration fallback to legacy bulk map
            let legacy: Map<Address, PrecisionPrediction> = env
                .storage()
                .persistent()
                .get(&DataKey::PrecisionPositions)
                .unwrap_or(Map::new(env));
            if legacy.is_empty() {
                return Ok(());
            }
            return Self::_resolve_precision_legacy(env, round_id, &legacy, final_price);
        }

        // Find minimum difference and collect all winners.
        //
        // We also cache `(amount, predicted_price)` per participant during this
        // first pass so the loser branch (which emits the `("outcome", "loss")`
        // observability event introduced by Issue #168) does not need to fetch
        // the same composite keys a second time. Halving the per-loser read
        // count is material for rounds at the participant cap (1 000).
        let mut min_diff: Option<u128> = None;
        let mut winners: Vec<PrecisionPrediction> = Vec::new(env);
        let mut total_pot: i128 = 0;
        // Per-participant snapshot of (amount, predicted_price-or-0-for-unrevealed).
        // Indexed by the same order as `participants`; each loser can be looked
        // up directly by its position without any extra storage access.
        let mut participant_amounts: Vec<i128> = Vec::new(env);
        let mut participant_prices: Vec<u128> = Vec::new(env);
        // Per-participant winner flag, populated in lockstep with the amount /
        // price snapshots above. Used by the loser branch to do O(N) winner
        // detection (instead of the O(N^2) `winners.iter().any(...)` lookup).
        // Index `i` corresponds to `participants[i]` by construction.
        let mut is_winner_mask: Vec<bool> = Vec::new(env);

        for i in 0..participants.len() {
            if let Some(user) = participants.get(i) {
                let pred_key = DataKey::PrecisionPosition(round_id, user.clone());
                let commit_key = DataKey::PrecisionCommitment(round_id, user.clone());

                let pred_opt = env.storage().persistent().get::<_, PrecisionPrediction>(&pred_key);

                let commitment_opt = env
                    .storage()
                    .persistent()
                    .get::<_, PrecisionCommitment>(&commit_key);

                // Add amount to total pot from prediction (revealed) or commitment (unrevealed).
                // Also snapshot the amount and (revealed-or-zero) price so the
                // loser branch below can emit the loss event without re-reading.
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

<<<<<<< HEAD
                total_pot = total_pot.checked_add(amount).ok_or(ContractError::Overflow)?;
=======
                total_pot = total_pot
                    .checked_add(amount)
                    .ok_or(ContractError::Overflow)?;
                participant_amounts.push_back(amount);
                participant_prices.push_back(cached_price);
                // Default to loser; flipped to `true` only when this
                // participant holds the currently smallest diff, and reset
                // to `false` if a tighter minimum is found later in the pass.
                is_winner_mask.push_back(false);
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b

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
                                // Reset any prior winners; index `i` now holds
                                // the sole winning prediction.
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

        // Distribute winnings to winner(s).
        // Remainder policy: `participants` is sorted lexicographically; `winners` is built
        // in that same sorted order, so index 0 is always the winner with the lowest Address.
        // Any integer remainder from the even split is assigned to that winner, making the
        // distribution fully deterministic.
        if !winners.is_empty() && total_pot > 0 {
            // Apply protocol fee (Issue #162) before splitting the pot.
            // Conservation invariant `distributable + fee == total_pot`.
            let (payout_pool, _fee_amount) =
                Self::_apply_protocol_fee_precision(env, round_id, total_pot)?;
            let winner_count = winners.len() as i128;
            let payout_per_winner = payout_pool / winner_count;
            let remainder = payout_pool % winner_count;

            // Award to each winner
            for i in 0..winners.len() {
                if let Some(winner) = winners.get(i) {
                    // First winner (lowest XDR-ordered Address) absorbs the remainder.
                    let payout = if i == 0 {
                        payout_per_winner.checked_add(remainder).ok_or(ContractError::Overflow)?
                    } else {
                        payout_per_winner
                    };

                    Self::_accumulate_pending(env, winner.user.clone(), payout)?;
                    Self::_update_stats_win(env, winner.user.clone())?;

                    Self::_persist_user_outcome(
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

            // Update stats and emit loss events for losers (Issue #168).
            //
            // The loss event payload mirrors [`Self::_record_winnings_indexed`]:
            // for Precision mode the relevant metadata is `predicted_price`,
            // so `side` is fixed at 0 while `mode = 1`.
            //
            // `stake` and `predicted_price` are read from the cached snapshot
            // populated during the winner-detection pass above, so this loop
            // does not need to re-read the per-user precision/commitment keys.
            //
            // Ordering note: the per-loser `("outcome", "loss")` event is
            // emitted BEFORE `_update_stats_loss` is called. Tests and indexers
            // rely on this sequence — flipping the order would change the
            // observable on-chain event stream for a loss-less winner-loss
            // pair.
            //
            // For unrevealed commitments the guess is unknowable on-chain
            // until reveal, so `predicted_price` is published as 0 to keep the
            // payload shape uniform across all losers.
            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    // O(1) winner lookup; the mask is filled in lockstep with
                    // `participants` so the index is always valid.
                    let was_winner = is_winner_mask.get(i).unwrap_or(false);
                    if !was_winner {
                        // Snapshot-driven. By construction `participants`,
                        // `participant_amounts`, `participant_prices` and
                        // `is_winner_mask` are all pushed exactly once per
                        // iteration of the winner-detection pass above, so
                        // index drift between the two passes is impossible.
                        // `unwrap` would surface any future drift instead of
                        // silently publishing a 0-stake loss event.
                        let stake = participant_amounts.get(i).unwrap();
                        let predicted_price = participant_prices.get(i).unwrap();

                        // Emit outcome loss event for Precision loser (Issue #168).
                        // Topic: ("outcome", "loss")
                        // Payload: (user, round_id, mode=1=Precision, amount, side=0, predicted_price)
                        #[allow(deprecated)]
                        env.events().publish(
                            (symbol_short!("outcome"), symbol_short!("loss")),
                            (user.clone(), round_id, 1u32, stake, 0u32, predicted_price),
                        );
                        Self::_update_stats_loss(env, user.clone())?;

                        Self::_persist_user_outcome(
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

    /// Legacy precision-mode resolution path — reads the bulk Map blob.
    /// Used only as a migration fallback; new rounds use indexed per-user keys.
    ///
    /// `round_id` is threaded in so the per-loser `("outcome", "loss")`
    /// observability event (Issue #168) carries the correct round id.
    fn _resolve_precision_legacy(
        env: &Env,
        round_id: u64,
        predictions_map: &Map<Address, PrecisionPrediction>,
        final_price: u128
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
                    pred.predicted_price.checked_sub(final_price).ok_or(ContractError::Overflow)?
                } else {
                    final_price.checked_sub(pred.predicted_price).ok_or(ContractError::Overflow)?
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
                total_pot = Self::payout_add(total_pot, pred.amount)?;
            }
        }

        // Remainder policy: `predictions_map` is a `Map<Address, PrecisionPrediction>`, which
        // Soroban keeps sorted by XDR-encoded key bytes. `winners` is built by iterating
        // `predictions_map.values()` in that stable key order, so index 0 always refers to
        // the lexicographically-lowest Address. Any integer remainder from the even split is
        // assigned exclusively to that winner, making the distribution fully deterministic.
        if !winners.is_empty() && total_pot > 0 {
            // Apply protocol fee (Issue #162) before splitting the pot.
            let (payout_pool, _fee_amount) =
                Self::_apply_protocol_fee_precision(env, round_id, total_pot)?;
            let winner_count = winners.len() as i128;
            let payout_per_winner = payout_pool / winner_count;
            let remainder = payout_pool % winner_count;

            // Award to each winner — all arithmetic checked before writing
            for i in 0..winners.len() {
                if let Some(winner) = winners.get(i) {
                    let payout = if i == 0 {
                        Self::payout_add(payout_per_winner, remainder)?
                    } else {
                        payout_per_winner
                    };
                    Self::_accumulate_pending(env, winner.user.clone(), payout)?;
                    Self::_update_stats_win(env, winner.user.clone())?;

                    Self::_persist_user_outcome(
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
                        // Emit outcome loss event for Precision loser (Issue #168).
                        // Topic: ("outcome", "loss")
                        // Payload: (user, round_id, mode=1=Precision, amount, side=0, predicted_price)
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
                        Self::_update_stats_loss(env, pred.user.clone())?;

                        Self::_persist_user_outcome(
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

    // ─── Lifecycle resilience (Issue #111) ──────────────────────────────────

    /// Cancels the active round and deterministically refunds all participant stakes.
    ///
    /// Only admin may cancel. Intended for oracle-unavailable or emergency recovery
    /// scenarios. After cancellation:
    ///  - All participant stakes are moved to their pending winnings.
    ///  - The active round is removed; no future settlement is possible.
    ///  - The round ID is marked cancelled to prevent any replay.
    pub fn cancel_round(env: Env, reason: u32) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
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
                Self::_emit_action_rejected(
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
                        if
                            let Some(pos) = env
                                .storage()
                                .persistent()
                                .get::<_, UserPosition>(&pos_key)
                        {
                            Self::_accumulate_pending(&env, user.clone(), pos.amount)?;
                            let prediction_side = match pos.side {
                                BetSide::Up => 0,
                                BetSide::Down => 1,
                            };
                            Self::_persist_user_outcome(
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
                        if
                            let Some(pred) = env
                                .storage()
                                .persistent()
                                .get::<_, PrecisionPrediction>(&pred_key)
                        {
                            refund_amount = pred.amount;
                        } else if
                            let Some(commit) = env
                                .storage()
                                .persistent()
                                .get::<_, PrecisionCommitment>(&commit_key)
                        {
                            refund_amount = commit.amount;
                        }

                        if refund_amount > 0 {
                            Self::_accumulate_pending(&env, user.clone(), refund_amount)?;
                        }
                        Self::_persist_user_outcome(
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
<<<<<<< HEAD
        env.storage().persistent().remove(&DataKey::RoundParticipants(round_id));
        env.storage().persistent().set(&DataKey::CancelledRound(round_id), &true);
=======
        let participant_count = participants.len();
        Self::_archive_round(
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
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
        env.storage().persistent().remove(&DataKey::ActiveRound);
        env.storage().persistent().remove(&Self::round_start_timestamp_key(round_id));

        // Emit cancellation event
        // Topic: ("round", "cancelled")
        // Payload: (round_id: u64, reason: u32, pool_up: i128, pool_down: i128)
        #[allow(deprecated)]
        env.events().publish(
<<<<<<< HEAD
            (symbol_short!("round"), symbol_short!("cancelled")),
            (round_id, reason, round.pool_up, round.pool_down)
=======
            (symbol_short!("round"), symbol_short!("cancel")),
            (round_id, reason, round.pool_up, round.pool_down),
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
        );

        Ok(())
    }

    /// Returns true if the given round_id was cancelled.
    pub fn is_round_cancelled(env: Env, round_id: u64) -> bool {
        env.storage().persistent().get(&DataKey::CancelledRound(round_id)).unwrap_or(false)
    }

    /// Claims pending winnings and adds to balance
    pub fn claim_winnings(env: Env, user: Address) -> Result<i128, ContractError> {
        Self::_require_supported_schema(&env)?;
        user.require_auth();
        Self::_ensure_not_paused(&env)?;

        let key = DataKey::PendingWinnings(user.clone());
        let pending: i128 = env.storage().persistent().get(&key).unwrap_or(0);

        if pending == 0 {
            return Ok(0);
        }

        let current_balance = Self::balance(env.clone(), user.clone());
        // Compute new balance before writing — all-or-nothing guarantee
        let new_balance = Self::payout_add(current_balance, pending)?;

        // Remove pending winnings before increasing balance (CEI pattern)
        env.storage().persistent().remove(&key);
        Self::_set_balance(&env, user.clone(), new_balance);

        // Emit claim event
        // Topic: ("claim", "winnings")
        // Payload: (user: Address, amount: i128)
        #[allow(deprecated)]
        env.events().publish((symbol_short!("claim"), symbol_short!("winnings")), (user, pending));

        Ok(pending)
    }

    /// Records refunds when price unchanged — indexed variant.
    ///
    /// Reads N individual position keys (O(1) each); no full-map deserialisation.
    fn _record_refunds_indexed(
        env: &Env,
        round_id: u64,
<<<<<<< HEAD
        participants: &Vec<Address>
=======
        round_mode: u32,
        participants: &Vec<Address>,
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
    ) -> Result<(), ContractError> {
        for i in 0..participants.len() {
            if let Some(user) = participants.get(i) {
                let pos_key = DataKey::Position(round_id, user.clone());
<<<<<<< HEAD
                if let Some(position) = env.storage().persistent().get::<_, UserPosition>(&pos_key) {
                    Self::_accumulate_pending(env, user, position.amount)?;
=======
                if let Some(position) = env.storage().persistent().get::<_, UserPosition>(&pos_key)
                {
                    Self::_accumulate_pending(env, user.clone(), position.amount)?;
                    let prediction_side = match position.side {
                        BetSide::Up => 0,
                        BetSide::Down => 1,
                    };
                    Self::_persist_user_outcome(
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
>>>>>>> 8a83d4239681978a127d37d86ddf69d1d5c78a9b
                }
            }
        }
        Ok(())
    }

    /// Records winnings for winning side — indexed variant.
    ///
    /// Formula: payout = bet + (bet / winning_pool) * losing_pool
    /// Reads N individual position keys; no full-map deserialisation.
    ///
    /// Also emits a per-loser `("outcome", "loss")` event (Issue #168) so
    /// indexers no longer need to infer losses from absence of payout
    /// events.
    fn _record_winnings_indexed(
        env: &Env,
        round_id: u64,
        participants: &Vec<Address>,
        winning_side: BetSide,
        winning_pool: i128,
        losing_pool: i128
    ) -> Result<(), ContractError> {
        if winning_pool == 0 {
            return Ok(());
        }

        // Apply protocol fee (Issue #162). Conservation invariant
        // `dist_winning + dist_losing + fee == winning + losing` always
        // holds; in the pathological case `fee > losing_pool` the spillover
        // is taken from `winning_pool`. Fee event already emitted inside
        // the helper.
        let (winning_pool, losing_pool, _fee_amount) =
            Self::_apply_protocol_fee_updown(env, round_id, winning_pool, losing_pool)?;

        for i in 0..participants.len() {
            if let Some(user) = participants.get(i) {
                let pos_key = DataKey::Position(round_id, user.clone());
                if let Some(position) = env.storage().persistent().get::<_, UserPosition>(&pos_key) {
                    if position.side == winning_side {
                        // Compute all payout math before any storage write
                        let share_numerator = Self::payout_mul(position.amount, losing_pool)?;
                        let share = share_numerator / winning_pool;
                        let payout = Self::payout_add(position.amount, share)?;

                        Self::_accumulate_pending(env, user.clone(), payout)?;
                        Self::_update_stats_win(env, user.clone())?;

                        let side_value = match position.side {
                            BetSide::Up => 0,
                            BetSide::Down => 1,
                        };
                        Self::_persist_user_outcome(
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
                        // Emit outcome loss event for UpDown loser (Issue #168).
                        // Topic: ("outcome", "loss")
                        // Payload: (user, round_id, mode=0=UpDown, amount, side, predicted_price=0)
                        // `predicted_price` is fixed at 0 for UpDown since this
                        // field is only meaningful in Precision mode.
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
                        Self::_update_stats_loss(env, user.clone())?;

                        Self::_persist_user_outcome(
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

    /// Persists a compact round summary and enforces FIFO archive retention.
    fn _archive_round(
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

        // Emit forensic round summary event
        // Topic: ("round", "summary")
        // Payload: (round_id, mode, price_start, price_final, participant_count, total_pot, status)
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

    fn _persist_user_outcome(
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
        Self::_extend_persistent_ttl(env, &key);
    }

    /// Refunds all participant stakes when the minimum-participants threshold is not met.
    /// Performs the same key cleanup as normal resolution so the contract is left consistent.
    fn _refund_under_threshold(
        env: &Env,
        round: &Round,
        participants: &Vec<Address>
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
                        if
                            let Some(pos) = env
                                .storage()
                                .persistent()
                                .get::<_, UserPosition>(&pos_key)
                        {
                            Self::_accumulate_pending(env, user.clone(), pos.amount)?;
                            let prediction_side = match pos.side {
                                BetSide::Up => 0,
                                BetSide::Down => 1,
                            };
                            Self::_persist_user_outcome(
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
                        if
                            let Some(pred) = env
                                .storage()
                                .persistent()
                                .get::<_, PrecisionPrediction>(&pred_key)
                        {
                            Self::_accumulate_pending(env, user.clone(), pred.amount)?;
                            Self::_persist_user_outcome(
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
                env.storage().persistent().remove(&DataKey::Position(round_id, user.clone()));
                env.storage().persistent().remove(&DataKey::PrecisionPosition(round_id, user));
            }
        }
        env.storage().persistent().remove(&DataKey::RoundParticipants(round_id));
        env.storage().persistent().remove(&DataKey::ActiveRound);
        env.storage().persistent().remove(&Self::round_start_timestamp_key(round_id));
        env.storage().persistent().remove(&DataKey::Positions);
        env.storage().persistent().remove(&DataKey::UpDownPositions);
        env.storage().persistent().remove(&DataKey::PrecisionPositions);
        Ok(())
    }

    pub(crate) fn _update_stats_win(env: &Env, user: Address) -> Result<(), ContractError> {
        let key = DataKey::UserStats(user);
        let mut stats: UserStats = env.storage().persistent().get(&key).unwrap_or(UserStats {
            total_wins: 0,
            total_losses: 0,
            current_streak: 0,
            best_streak: 0,
        });

        stats.total_wins = stats.total_wins.checked_add(1).ok_or(ContractError::Overflow)?;
        stats.current_streak = stats.current_streak.checked_add(1).ok_or(ContractError::Overflow)?;

        if stats.current_streak > stats.best_streak {
            stats.best_streak = stats.current_streak;
        }

        env.storage().persistent().set(&key, &stats);
        Self::_extend_persistent_ttl(env, &key);
        Ok(())
    }

    pub(crate) fn _update_stats_loss(env: &Env, user: Address) -> Result<(), ContractError> {
        let key = DataKey::UserStats(user);
        let mut stats: UserStats = env.storage().persistent().get(&key).unwrap_or(UserStats {
            total_wins: 0,
            total_losses: 0,
            current_streak: 0,
            best_streak: 0,
        });

        stats.total_losses = stats.total_losses.checked_add(1).ok_or(ContractError::Overflow)?;
        stats.current_streak = 0;

        env.storage().persistent().set(&key, &stats);
        Self::_extend_persistent_ttl(env, &key);
        Ok(())
    }

    /// Mints 1000 vXLM for new users (one-time only)
    pub fn mint_initial(env: Env, user: Address) -> i128 {
        user.require_auth();
        if let Err(e) = Self::_require_supported_schema(&env) {
            panic_with_error!(&env, e);
        }
        if let Err(e) = Self::_ensure_normal_mode(&env) {
            panic_with_error!(&env, e);
        }

        let key = DataKey::Balance(user.clone());

        if let Some(existing_balance) = env.storage().persistent().get(&key) {
            Self::_extend_persistent_ttl(&env, &key);
            return existing_balance;
        }

        // Rate limit check: retrieve current ledger height and check against limit config.
        let sequence = env.ledger().sequence();
        if let Some(limit) = env
            .storage()
            .instance()
            .get::<_, u32>(&DataKey::MintLimitConfig)
        {
            if limit > 0 {
                let counter_key = DataKey::LedgerMintCounter(sequence);
                let current_count = env
                    .storage()
                    .temporary()
                    .get::<_, u32>(&counter_key)
                    .unwrap_or(0);
                if current_count >= limit {
                    panic_with_error!(&env, ContractError::MintLimitExceeded);
                }
                env.storage()
                    .temporary()
                    .set(&counter_key, &(current_count + 1));
            }
        }

        let initial_amount: i128 = 1000_0000000;
        env.storage().persistent().set(&key, &initial_amount);
        Self::_extend_persistent_ttl(&env, &key);

        // Emit mint event
        // Topic: ("mint", "initial")
        // Payload: (user: Address, amount: i128)
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("mint"), symbol_short!("initial")),
            (user, initial_amount)
        );

        initial_amount
    }

    /// Returns user's vXLM balance
    pub fn balance(env: Env, user: Address) -> i128 {
        let key = DataKey::Balance(user);
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    pub(crate) fn _set_balance(env: &Env, user: Address, amount: i128) {
        let key = DataKey::Balance(user);
        env.storage().persistent().set(&key, &amount);
        Self::_extend_persistent_ttl(env, &key);
    }

    fn _ensure_not_paused(env: &Env) -> Result<(), ContractError> {
        let key = DataKey::Paused;
        Self::_extend_persistent_ttl(env, &key);
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

    fn _ensure_normal_mode(env: &Env) -> Result<(), ContractError> {
        let key = DataKey::Paused;
        Self::_extend_persistent_ttl(env, &key);
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

    fn _set_mode(env: &Env, new_mode: RuntimeMode) -> Result<(), ContractError> {
        let key = DataKey::Paused;
        let old_mode = env
            .storage()
            .persistent()
            .get::<_, RuntimeMode>(&key)
            .unwrap_or(RuntimeMode::Normal);
        if old_mode != new_mode {
            env.storage().persistent().set(&key, &new_mode);
            Self::_extend_persistent_ttl(env, &key);
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("mode"), Symbol::new(env, "transition")),
                (old_mode as u32, new_mode as u32),
            );
        }
        Ok(())
    }

    /// Derives the round lifecycle phase for `round` at `ledger_sequence`.
    fn _derive_round_phase(ledger_sequence: u32, round: &Round) -> RoundPhase {
        if ledger_sequence < round.bet_end_ledger {
            RoundPhase::Betting
        } else if ledger_sequence < round.end_ledger {
            RoundPhase::Running
        } else {
            RoundPhase::Resolvable
        }
    }

    fn _schema_version(env: &Env) -> Option<u32> {
        env.storage().persistent().get(&DataKey::SchemaVersion)
    }

    fn _require_supported_schema(env: &Env) -> Result<u32, ContractError> {
        Self::_extend_persistent_ttl(env, &DataKey::SchemaVersion);
        if env.storage().persistent().has(&DataKey::Admin) {
            Self::_extend_persistent_ttl(env, &DataKey::Admin);
        }
        let v = Self::_schema_version(env).unwrap_or(1);
        if v == 0 || v > CURRENT_SCHEMA_VERSION {
            return Err(ContractError::UnsupportedSchemaVersion);
        }
        Ok(v)
    }

    fn assert_no_active_round(env: &Env) -> Result<(), ContractError> {
        if env.storage().persistent().has(&DataKey::ActiveRound) {
            return Err(ContractError::RoundAlreadyActive);
        }

        Ok(())
    }

    fn round_start_timestamp_key(round_id: u64) -> (Symbol, u64) {
        (symbol_short!("rstart"), round_id)
    }

    /// Checked addition for payout accumulation.
    ///
    /// All payout aggregation (refunds, winnings, precision payouts) routes
    /// through this helper so overflow always maps to the stable
    /// `PayoutOverflow` variant rather than a generic `Overflow`. This makes
    /// the failure mode auditable and distinguishable from non-financial
    /// overflow (e.g. round-ID counter, ledger arithmetic).
    ///
    /// All-or-nothing guarantee: callers must not mutate storage before all
    /// payout math is complete and checked. The functions below enforce this
    /// by computing the new value first and only writing it afterward.
    #[inline(always)]
    fn payout_add(a: i128, b: i128) -> Result<i128, ContractError> {
        a.checked_add(b).ok_or(ContractError::PayoutOverflow)
    }

    #[inline(always)]
    fn payout_mul(a: i128, b: i128) -> Result<i128, ContractError> {
        a.checked_mul(b).ok_or(ContractError::PayoutOverflow)
    }

    /// Accumulates `amount` into a user's pending winnings, enforcing the cap if set (Issue #120).
    ///
    /// Reads and writes `DataKey::PendingWinnings(user)` in one place, ensuring the cap
    /// check and overflow protection are applied consistently across all payout paths.
    fn _accumulate_pending(env: &Env, user: Address, amount: i128) -> Result<(), ContractError> {
        let key = DataKey::PendingWinnings(user);
        let existing: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_pending = Self::payout_add(existing, amount)?;

        // Enforce pending winnings cap if configured
        if let Some(cap) = env.storage().persistent().get::<_, i128>(&DataKey::MaxPendingWinnings) {
            if new_pending > cap {
                return Err(ContractError::PendingWinningsCapExceeded);
            }
        }

        env.storage().persistent().set(&key, &new_pending);
        Self::_extend_persistent_ttl(env, &key);
        Ok(())
    }

    fn _validate_windows(bet_ledgers: u32, run_ledgers: u32) -> Result<(), ContractError> {
        if bet_ledgers == 0 || run_ledgers == 0 {
            return Err(ContractError::InvalidDuration);
        }
        if bet_ledgers > MAX_BET_WINDOW_LEDGERS || run_ledgers > MAX_RUN_WINDOW_LEDGERS {
            return Err(ContractError::WindowOutOfRange);
        }
        if bet_ledgers >= run_ledgers {
            return Err(ContractError::InvalidDuration);
        }
        Ok(())
    }

    fn _validate_max_stake(max_amount: Option<i128>) -> Result<(), ContractError> {
        if let Some(v) = max_amount {
            if v < MIN_CAP_VALUE {
                return Err(ContractError::InvalidBetAmount);
            }
        }
        Ok(())
    }

    fn _validate_oracle_stale_threshold(seconds: u64) -> Result<(), ContractError> {
        if !(MIN_ORACLE_STALE_THRESHOLD..=MAX_ORACLE_STALE_THRESHOLD).contains(&seconds) {
            return Err(ContractError::InvalidStaleThreshold);
        }
        Ok(())
    }

    fn _validate_oracle_max_deviation_bps(bps: Option<u32>) -> Result<(), ContractError> {
        if let Some(v) = bps {
            if v == 0 || v > MAX_ORACLE_DEVIATION_BPS {
                return Err(ContractError::InvalidOracleDeviationBps);
            }
        }
        Ok(())
    }

    /// Validates a requested protocol-fee bps (Issue #162).
    /// `None` always allowed (disables fee entirely, restoring pre-#162
    /// byte-for-byte behaviour). `Some(0)` is rejected — only explicit `None`
    /// is the legitimate way to express "fee disabled". `Some(bps)` must
    /// satisfy `1 <= bps <= MAX_PROTOCOL_FEE_BPS`.
    fn _validate_protocol_fee_bps(bps: Option<u32>) -> Result<(), ContractError> {
        if let Some(v) = bps {
            if v == 0 || v > MAX_PROTOCOL_FEE_BPS {
                return Err(ContractError::InvalidProtocolFeeBps);
            }
        }
        Ok(())
    }

    /// Reads the currently-configured protocol fee in bps (Issue #162).
    /// Bumps TTL only when the key is present (avoids extra storage writes
    /// on the hot "fee disabled" path through every competitive settlement).
    fn _read_protocol_fee_bps(env: &Env) -> Option<u32> {
        let key = DataKey::ProtocolFeeBps;
        let v: Option<u32> = env.storage().persistent().get(&key);
        if v.is_some() {
            Self::_extend_persistent_ttl(env, &key);
        }
        v
    }

    /// Credits `fee_amount` stroops to the protocol fee treasury and emits
    /// `("protocol", "fee_collected")` (Issue #162). TTL on the treasury
    /// key is extended on every write so the cumulative balance never
    /// falls into archival. Payload mirrors the active bps so indexers
    /// do not need an extra storage read.
    fn _collect_protocol_fee(
        env: &Env,
        round_id: u64,
        fee_amount: i128,
        bps_active: Option<u32>,
    ) -> Result<(), ContractError> {
        if fee_amount <= 0 {
            return Ok(());
        }
        let treasury_key = DataKey::ProtocolFeeTreasury;
        let current: i128 = env.storage().persistent().get(&treasury_key).unwrap_or(0);
        let new_treasury = current
            .checked_add(fee_amount)
            .ok_or(ContractError::Overflow)?;
        env.storage().persistent().set(&treasury_key, &new_treasury);
        Self::_extend_persistent_ttl(env, &treasury_key);

        let bps_value: u32 = bps_active.unwrap_or(0);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("protocol"), symbol_short!("collected")),
            (round_id, fee_amount, new_treasury, bps_value),
        );

        Ok(())
    }

    /// Splits a `(winning_pool, losing_pool)` pair into the post-fee pools
    /// and the treasury's cut, used by both UpDown settlement paths
    /// (Issue #162). Conservation invariant
    ///   dist_winning + dist_losing + fee == winning + losing
    /// holds ALWAYS, even in the pathological case `fee > losing_pool`
    /// (very thin losing-side liquidity near the bps cap): the spillover
    /// is then deducted from `winning_pool`, so winners lose a portion
    /// of their principal rather than the fee being silently dropped.
    /// Behaviour is documented in `docs/EVENT_SCHEMA.md` and exercised
    /// by `test_protocol_fee_thin_losing_pool`.
    fn _apply_protocol_fee_updown(
        env: &Env,
        round_id: u64,
        winning_pool: i128,
        losing_pool: i128,
    ) -> Result<(i128, i128, i128), ContractError> {
        let bps = Self::_read_protocol_fee_bps(env);
        if bps.is_none() {
            return Ok((winning_pool, losing_pool, 0));
        }
        let bps_value = bps.unwrap();
        let total_pot = Self::payout_add(winning_pool, losing_pool)?;
        let fee_amount = total_pot
            .checked_mul(bps_value as i128)
            .ok_or(ContractError::Overflow)?
            / BPS_DENOMINATOR;
        if fee_amount == 0 {
            return Ok((winning_pool, losing_pool, 0));
        }
        let fee_from_losing = fee_amount.min(losing_pool);
        let fee_from_winning = fee_amount
            .checked_sub(fee_from_losing)
            .ok_or(ContractError::Overflow)?;
        let dist_winning = winning_pool
            .checked_sub(fee_from_winning)
            .ok_or(ContractError::Overflow)?;
        let dist_losing = losing_pool
            .checked_sub(fee_from_losing)
            .ok_or(ContractError::Overflow)?;
        Self::_collect_protocol_fee(env, round_id, fee_amount, Some(bps_value))?;
        Ok((dist_winning, dist_losing, fee_amount))
    }

    /// Splits a precision-mode `total_pot` into the distributable amount
    /// (split among winners per the existing remainder policy) and the
    /// treasury's cut (Issue #162). Returns `(distributable, fee_amount)`.
    fn _apply_protocol_fee_precision(
        env: &Env,
        round_id: u64,
        total_pot: i128,
    ) -> Result<(i128, i128), ContractError> {
        let bps = Self::_read_protocol_fee_bps(env);
        if bps.is_none() || total_pot <= 0 {
            return Ok((total_pot, 0));
        }
        let bps_value = bps.unwrap();
        let fee_amount = total_pot
            .checked_mul(bps_value as i128)
            .ok_or(ContractError::Overflow)?
            / BPS_DENOMINATOR;
        let distributable = total_pot
            .checked_sub(fee_amount)
            .ok_or(ContractError::Overflow)?;
        if fee_amount > 0 {
            Self::_collect_protocol_fee(env, round_id, fee_amount, Some(bps_value))?;
        }
        Ok((distributable, fee_amount))
    }

    fn _emit_action_rejected(env: &Env, actor: &Address, action: Symbol, reason: ContractError) {
        // Privacy: event payload contains only the actor Address, an action
        // symbol, and a numeric reason code. No personally identifiable
        // information, financial amounts, or internal state is exposed.
        // Operators can match reason codes against ContractError variants.
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("action"), symbol_short!("rejct")),
            (actor.clone(), action, reason as u32),
        );
    }

    fn _emit_config_updated(
        env: &Env,
        kind: ConfigChangeKind,
        old_value: ConfigChangePayload,
        new_value: ConfigChangePayload,
    ) {
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("config"), symbol_short!("updated")),
            (kind, old_value, new_value),
        );
    }

    fn _current_config_payload(env: &Env, kind: &ConfigChangeKind) -> ConfigChangePayload {
        match kind {
            ConfigChangeKind::Windows => {
                let bet: u32 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::BetWindowLedgers)
                    .unwrap_or(DEFAULT_BET_WINDOW_LEDGERS);
                let run: u32 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::RunWindowLedgers)
                    .unwrap_or(DEFAULT_RUN_WINDOW_LEDGERS);
                ConfigChangePayload::Windows(bet, run)
            }
            ConfigChangeKind::MaxStake => {
                ConfigChangePayload::MaxStake(env.storage().persistent().get(&DataKey::MaxStake))
            }
            ConfigChangeKind::MaxUserRoundExposure => ConfigChangePayload::MaxUserRoundExposure(
                env.storage()
                    .persistent()
                    .get(&DataKey::MaxUserRoundExposure),
            ),
            ConfigChangeKind::MaxPendingWinnings => ConfigChangePayload::MaxPendingWinnings(
                env.storage().persistent().get(&DataKey::MaxPendingWinnings),
            ),
            ConfigChangeKind::OracleStaleThreshold => ConfigChangePayload::OracleStaleThreshold(
                env.storage()
                    .persistent()
                    .get(&DataKey::OracleStaleThreshold)
                    .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD),
            ),
            ConfigChangeKind::OracleMaxDeviationBps => ConfigChangePayload::OracleMaxDeviationBps(
                env.storage()
                    .persistent()
                    .get(&DataKey::OracleMaxDeviationBps),
            ),
            ConfigChangeKind::ProtocolFeeBps => ConfigChangePayload::ProtocolFeeBps(
                env.storage().persistent().get(&DataKey::ProtocolFeeBps),
            ),
            ConfigChangeKind::MinParticipants => ConfigChangePayload::MinParticipants(
                env.storage().persistent().get(&DataKey::MinParticipants),
            ),
            ConfigChangeKind::MaxPrecisionParticipants => {
                ConfigChangePayload::MaxPrecisionParticipants(
                    env.storage()
                        .persistent()
                        .get(&DataKey::MaxPrecisionParticipants)
                        .unwrap_or(DEFAULT_MAX_PRECISION_PARTICIPANTS),
                )
            }
            ConfigChangeKind::MintLimit => ConfigChangePayload::MintLimit(
                env.storage()
                    .instance()
                    .get(&DataKey::MintLimitConfig)
                    .unwrap_or(0),
            ),
            ConfigChangeKind::ArchiveRetention => ConfigChangePayload::ArchiveRetention(
                env.storage()
                    .persistent()
                    .get(&DataKey::ArchiveRetention)
                    .unwrap_or(DEFAULT_ARCHIVE_RETENTION),
            ),
        }
    }

    fn _schedule_config_change(
        env: &Env,
        kind: ConfigChangeKind,
        payload: ConfigChangePayload,
    ) -> Result<(), ContractError> {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(env).map_err(|e| {
            Self::_emit_action_rejected(env, &admin, symbol_short!("sched"), e);
            e
        })?;

        let key = DataKey::PendingConfigChange(kind.clone());
        if env.storage().persistent().has(&key) {
            Self::_emit_action_rejected(
                env,
                &admin,
                symbol_short!("sched"),
                ContractError::RoundAlreadyActive,
            );
            return Err(ContractError::RoundAlreadyActive);
        }

        let scheduled_at_ledger = env.ledger().sequence();
        let activation_ledger = scheduled_at_ledger
            .checked_add(CONFIG_TIMELOCK_LEDGERS)
            .ok_or(ContractError::Overflow)?;

        let pending = PendingConfigChange {
            payload,
            activation_ledger,
            scheduled_at_ledger,
        };
        env.storage().persistent().set(&key, &pending);
        Self::_extend_persistent_ttl(env, &key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("config"), symbol_short!("sched")),
            (kind, activation_ledger),
        );

        Ok(())
    }

    fn _apply_config_payload(
        env: &Env,
        kind: &ConfigChangeKind,
        payload: &ConfigChangePayload,
    ) -> Result<(), ContractError> {
        let old_value = Self::_current_config_payload(env, kind);
        match (kind, payload) {
            (ConfigChangeKind::Windows, ConfigChangePayload::Windows(bet, run)) => {
                Self::_validate_windows(*bet, *run)?;
                env.storage()
                    .persistent()
                    .set(&DataKey::BetWindowLedgers, bet);
                Self::_extend_persistent_ttl(env, &DataKey::BetWindowLedgers);
                env.storage()
                    .persistent()
                    .set(&DataKey::RunWindowLedgers, run);
                Self::_extend_persistent_ttl(env, &DataKey::RunWindowLedgers);
                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("windows"), symbol_short!("updated")),
                    (*bet, *run),
                );
            }
            (ConfigChangeKind::MaxStake, ConfigChangePayload::MaxStake(max)) => {
                Self::_validate_max_stake(*max)?;
                let key = DataKey::MaxStake;
                if let Some(v) = max {
                    env.storage().persistent().set(&key, v);
                    Self::_extend_persistent_ttl(env, &key);
                } else {
                    env.storage().persistent().remove(&key);
                }
            }
            (
                ConfigChangeKind::MaxUserRoundExposure,
                ConfigChangePayload::MaxUserRoundExposure(max),
            ) => {
                Self::_validate_max_stake(*max)?;
                let key = DataKey::MaxUserRoundExposure;
                if let Some(v) = max {
                    env.storage().persistent().set(&key, v);
                    Self::_extend_persistent_ttl(env, &key);
                } else {
                    env.storage().persistent().remove(&key);
                }
            }
            (
                ConfigChangeKind::MaxPendingWinnings,
                ConfigChangePayload::MaxPendingWinnings(max),
            ) => {
                Self::_validate_max_stake(*max)?;
                let key = DataKey::MaxPendingWinnings;
                if let Some(v) = max {
                    env.storage().persistent().set(&key, v);
                    Self::_extend_persistent_ttl(env, &key);
                } else {
                    env.storage().persistent().remove(&key);
                }
            }
            (
                ConfigChangeKind::OracleStaleThreshold,
                ConfigChangePayload::OracleStaleThreshold(seconds),
            ) => {
                Self::_validate_oracle_stale_threshold(*seconds)?;
                let key = DataKey::OracleStaleThreshold;
                env.storage().persistent().set(&key, seconds);
                Self::_extend_persistent_ttl(env, &key);
            }
            (
                ConfigChangeKind::OracleMaxDeviationBps,
                ConfigChangePayload::OracleMaxDeviationBps(bps),
            ) => {
                Self::_validate_oracle_max_deviation_bps(*bps)?;
                let key = DataKey::OracleMaxDeviationBps;
                if let Some(v) = bps {
                    env.storage().persistent().set(&key, v);
                    Self::_extend_persistent_ttl(env, &key);
                } else {
                    env.storage().persistent().remove(&key);
                }
            }

            (ConfigChangeKind::ProtocolFeeBps, ConfigChangePayload::ProtocolFeeBps(bps)) => {
                Self::_validate_protocol_fee_bps(*bps)?;
                let key = DataKey::ProtocolFeeBps;
                if let Some(v) = bps {
                    env.storage().persistent().set(&key, v);
                    Self::_extend_persistent_ttl(env, &key);
                } else {
                    env.storage().persistent().remove(&key);
                }
                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("protocol"), symbol_short!("bps_set")),
                    (bps.clone(),),
                );
            }
            _ => return Err(ContractError::InvalidMode),
        }
        Self::_emit_config_updated(env, kind.clone(), old_value, payload.clone());
        Ok(())
    }

    /// Bumps/extends the TTL of the given persistent storage key if its remaining TTL
    /// is less than the threshold. Enforces rent policy (Issue #142).
    fn _extend_persistent_ttl(env: &Env, key: &DataKey) {
        if env.storage().persistent().has(key) {
            env.storage()
                .persistent()
                .extend_ttl(key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
        }
impl VirtualTokenContract {
    pub fn _update_stats_win(env: &Env, user: Address) -> Result<(), ContractError> {
        settlement::_update_stats_win(env, user)
    }

    pub fn _update_stats_loss(env: &Env, user: Address) -> Result<(), ContractError> {
        settlement::_update_stats_loss(env, user)
    }
}
