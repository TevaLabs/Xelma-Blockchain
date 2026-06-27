//! Core contract implementation for the XLM Price Prediction Market.

use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    contract, contractimpl, panic_with_error, symbol_short, Address, Bytes, BytesN, Env, Map, Vec,
};

use crate::errors::ContractError;
use crate::types::{
    ArchivedRoundSummary, BetSide, ConfigChangeKind, ConfigChangePayload, DataKey,
    OracleHeartbeatRecord, OraclePayload, PendingConfigChange, PrecisionCommitment,
    PrecisionPrediction, Round, RoundArchiveStatus, RoundMode, UserPosition, UserStats,
};

// ─── Economic control limits ─────────────────────────────────────────────────
/// Minimum allowed value when setting an economic cap to prevent zero-value lockouts.
const MIN_CAP_VALUE: i128 = 1;
/// Lower bound for `min_bet` (Issue #161). Setting min_bet below this floor is
/// rejected because it would either block every bet (`min=0` is the same as
/// `None` but expressed as a number) or have no useful semantics.
const MIN_MIN_BET_AMOUNT: i128 = 1;
/// Upper bound for `min_bet` (Issue #161). Capped at the same magnitude as
/// the protocol's `MAX_START_PRICE` to keep admin-tuned floors inside the
/// same safe range as settlement-time arithmetic.
const MAX_MIN_BET_AMOUNT: i128 = 1_000_000_000_000_000_000; // 1e18, matches MAX_START_PRICE
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

const DEFAULT_BET_WINDOW_LEDGERS: u32 = 6;
const DEFAULT_RUN_WINDOW_LEDGERS: u32 = 12;
const MAX_BET_WINDOW_LEDGERS: u32 = 1_440;
const MAX_RUN_WINDOW_LEDGERS: u32 = 2_880;

// ─── Oracle deviation guardrails ─────────────────────────────────────────────
/// Maximum allowed basis points for oracle deviation is bounded to avoid absurd configs.
/// 100_000 bp = 1000% deviation (effectively "off", but still explicit).
const MAX_ORACLE_DEVIATION_BPS: u32 = 100_000;

// ─── Storage schema versioning ───────────────────────────────────────────────
const CURRENT_SCHEMA_VERSION: u32 = 2;
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

/// Maximum archived round summaries retained on-chain (FIFO pruning).
const MAX_ARCHIVED_ROUNDS: u32 = 128;
/// Ledgers to wait before a scheduled critical config change may be applied (~2 hours).
const CONFIG_TIMELOCK_LEDGERS: u32 = 1440;

#[contract]
pub struct VirtualTokenContract;

#[contractimpl]
impl VirtualTokenContract {
    /// Initializes the contract with admin and oracle addresses (one-time only)
    pub fn initialize(env: Env, admin: Address, oracle: Address) -> Result<(), ContractError> {
        admin.require_auth();

        if admin == oracle {
            return Err(ContractError::AdminIsOracle);
        }

        if env.storage().persistent().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }

        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::Oracle, &oracle);
        env.storage().persistent().set(&DataKey::Paused, &false);
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

        Self::_extend_persistent_ttl(&env, &DataKey::Admin);
        Self::_extend_persistent_ttl(&env, &DataKey::Oracle);
        Self::_extend_persistent_ttl(&env, &DataKey::Paused);
        Self::_extend_persistent_ttl(&env, &DataKey::SchemaVersion);
        Self::_extend_persistent_ttl(&env, &DataKey::BetWindowLedgers);
        Self::_extend_persistent_ttl(&env, &DataKey::RunWindowLedgers);

        Ok(())
    }

    /// Returns the stored schema version. If unset, returns legacy version 1.
    pub fn get_schema_version(env: Env) -> u32 {
        let key = DataKey::SchemaVersion;
        Self::_extend_persistent_ttl(&env, &key);
        Self::_schema_version(&env).unwrap_or(1)
    }

    /// Migrates legacy schema version 1 → current schema version 2 (admin only).
    ///
    /// Guardrails:
    /// - Must not have an active round (avoids partial state interpretation changes)
    /// - Only supports v1 → v2 in this release
    pub fn migrate_schema_v1_to_v2(env: Env) -> Result<(), ContractError> {
        let admin_key = DataKey::Admin;
        Self::_extend_persistent_ttl(&env, &admin_key);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&admin_key)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        if env.storage().persistent().has(&DataKey::ActiveRound) {
            return Err(ContractError::MigrationActiveRound);
        }

        let from = Self::_schema_version(&env).unwrap_or(1);
        if from != 1 || CURRENT_SCHEMA_VERSION != 2 {
            return Err(ContractError::InvalidMigrationPath);
        }

        let schema_key = DataKey::SchemaVersion;
        env.storage()
            .persistent()
            .set(&schema_key, &CURRENT_SCHEMA_VERSION);
        Self::_extend_persistent_ttl(&env, &schema_key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("schema"), symbol_short!("migrated")),
            (from, CURRENT_SCHEMA_VERSION),
        );

        Ok(())
    }

    /// Returns whether the contract is currently paused
    pub fn is_paused(env: Env) -> bool {
        let key = DataKey::Paused;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key).unwrap_or(false)
    }

    /// Pauses the contract for emergency recovery (admin only)
    pub fn pause_contract(env: Env) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;

        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &true);
        Self::_extend_persistent_ttl(&env, &DataKey::Paused);

        Ok(())
    }

    /// Unpauses the contract after recovery (admin only)
    pub fn unpause_contract(env: Env) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;

        admin.require_auth();
        env.storage().persistent().set(&DataKey::Paused, &false);
        Self::_extend_persistent_ttl(&env, &DataKey::Paused);

        Ok(())
    }

    /// Creates a new prediction round (admin only)
    /// mode: 0 = Up/Down (default), 1 = Precision (Legends)
    pub fn create_round(
        env: Env,
        start_price: u128,
        mode: Option<u32>,
    ) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        if start_price < MIN_START_PRICE {
            return Err(ContractError::StartPriceTooLow);
        }
        if start_price > MAX_START_PRICE {
            return Err(ContractError::StartPriceTooHigh);
        }

        // Default to Up/Down mode (0) if not specified
        let mode_value = mode.unwrap_or(0);

        // Validate mode is either 0 or 1
        if mode_value > 1 {
            return Err(ContractError::InvalidMode);
        }

        let round_mode = if mode_value == 0 {
            RoundMode::UpDown
        } else {
            RoundMode::Precision
        };

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;

        admin.require_auth();
        Self::_ensure_not_paused(&env)?;
        Self::assert_no_active_round(&env)?;

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

        let start_ledger = env.ledger().sequence();
        let bet_end_ledger = start_ledger
            .checked_add(bet_ledgers)
            .ok_or(ContractError::Overflow)?;
        let end_ledger = start_ledger
            .checked_add(run_ledgers)
            .ok_or(ContractError::Overflow)?;

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

        env.storage()
            .persistent()
            .set(&DataKey::ActiveRound, &round);
        Self::_extend_persistent_ttl(&env, &DataKey::ActiveRound);

        // Note: individual position keys (DataKey::Position / DataKey::PrecisionPosition)
        // are cleaned up at resolve time; no bulk-map clearing needed here.

        // Emit round creation event with round ID and mode
        // Topic: ("round", "created")
        // Payload: (round_id: u64, start_price: u128, start_ledger: u32, bet_end_ledger: u32, end_ledger: u32, mode: u32)
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("round"), symbol_short!("created")),
            (
                round_id,
                start_price,
                start_ledger,
                bet_end_ledger,
                end_ledger,
                mode_value,
            ),
        );

        Ok(())
    }

    /// Returns the currently active round, if any
    pub fn get_active_round(env: Env) -> Option<Round> {
        env.storage().persistent().get(&DataKey::ActiveRound)
    }

    /// Returns the ID of the last created round (0 if no rounds created yet)
    pub fn get_last_round_id(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::LastRoundId)
            .unwrap_or(0)
    }

    /// Returns a compact archived round summary by round id, if retained.
    pub fn get_archived_round(env: Env, round_id: u64) -> Option<ArchivedRoundSummary> {
        env.storage()
            .persistent()
            .get(&DataKey::ArchivedRound(round_id))
    }

    /// Returns up to `limit` most recently archived rounds (newest first).
    ///
    /// Pass `limit = 0` to receive an empty list. Values above [`MAX_ARCHIVED_ROUNDS`]
    /// are capped automatically.
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

        let fetch_cap = if limit > MAX_ARCHIVED_ROUNDS {
            MAX_ARCHIVED_ROUNDS
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

    pub fn get_admin(env: Env) -> Option<Address> {
        let key = DataKey::Admin;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    }

    pub fn get_oracle(env: Env) -> Option<Address> {
        let key = DataKey::Oracle;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    }

    /// Schedules a timelocked oracle deviation update (alias for [`Self::schedule_oracle_deviation_bps`]).
    ///
    /// - `None`: disables deviation guardrails
    /// - `Some(bps)`: enables guardrails with a threshold in basis points (1 bp = 0.01%)
    pub fn set_oracle_max_deviation_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
        Self::schedule_oracle_deviation_bps(env, bps)
    }

    /// Returns the configured oracle max deviation bps, if set.
    pub fn get_oracle_max_deviation_bps(env: Env) -> Option<u32> {
        let key = DataKey::OracleMaxDeviationBps;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    }

    /// Arms a one-shot override to bypass deviation checks for the next settlement (admin only).
    /// The flag is automatically cleared after a settlement uses it.
    pub fn arm_oracle_deviation_override(env: Env) -> Result<(), ContractError> {
        let admin_key = DataKey::Admin;
        Self::_extend_persistent_ttl(&env, &admin_key);
        let admin: Address = env
            .storage()
            .persistent()
            .get(&admin_key)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        let override_key = DataKey::OracleDeviationOverrideArmed;
        env.storage().persistent().set(&override_key, &true);
        Self::_extend_persistent_ttl(&env, &override_key);
        Ok(())
    }

    // ─── Oracle heartbeat and liveness (on-chain health tracking) ───────────

    /// Records an oracle heartbeat (oracle only).
    /// `status`: 0 = active, 1 = degraded, 2 = offline.
    /// Stores current ledger timestamp; emits `("oracle", "heartbeat")`.
    pub fn update_oracle_heartbeat(env: Env, status: u32) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        if status > 2 {
            return Err(ContractError::InvalidOracleStatus);
        }
        Self::_extend_persistent_ttl(&env, &DataKey::Oracle);
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
        Self::_extend_persistent_ttl(&env, &DataKey::OracleHeartbeat);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("oracle"), symbol_short!("heartbeat")),
            (ts, status),
        );
        Ok(())
    }

    /// Returns the most recent oracle heartbeat record, if any.
    pub fn get_oracle_heartbeat(env: Env) -> Option<OracleHeartbeatRecord> {
        let key = DataKey::OracleHeartbeat;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    }

    /// Returns `true` if the oracle has a non-stale heartbeat with status not offline (2).
    /// Uses the configured stale threshold, defaulting to 3600 seconds.
    pub fn is_oracle_live(env: Env) -> bool {
        let heartbeat_key = DataKey::OracleHeartbeat;
        Self::_extend_persistent_ttl(&env, &heartbeat_key);
        let record: OracleHeartbeatRecord = match env.storage().persistent().get(&heartbeat_key) {
            Some(r) => r,
            None => return false,
        };
        if record.status == 2 {
            return false;
        }
        let threshold_key = DataKey::OracleStaleThreshold;
        Self::_extend_persistent_ttl(&env, &threshold_key);
        let threshold: u64 = env
            .storage()
            .persistent()
            .get(&threshold_key)
            .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD);
        let current_time = env.ledger().timestamp();
        current_time <= record.timestamp.saturating_add(threshold)
    }

    /// Schedules a timelocked stale threshold update (alias for [`Self::schedule_oracle_stale_threshold`]).
    /// Allowed range: 60–86400 seconds (1 minute to 24 hours).
    pub fn set_oracle_stale_threshold(env: Env, seconds: u64) -> Result<(), ContractError> {
        Self::schedule_oracle_stale_threshold(env, seconds)
    }

    /// Returns the configured oracle stale threshold, or the default (3600 s) if not set.
    pub fn get_oracle_stale_threshold(env: Env) -> u64 {
        let key = DataKey::OracleStaleThreshold;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD)
    }

    /// Schedules a timelocked windows update (alias for [`Self::schedule_windows`]).
    /// bet_ledgers: Number of ledgers users can place bets
    /// run_ledgers: Total number of ledgers before round can be resolved
    pub fn set_windows(env: Env, bet_ledgers: u32, run_ledgers: u32) -> Result<(), ContractError> {
        Self::schedule_windows(env, bet_ledgers, run_ledgers)
    }

    // ─── Economic controls (Issue #113) ─────────────────────────────────────

    /// Schedules a timelocked max stake update (alias for [`Self::schedule_max_stake`]).
    /// Pass `None` to disable the cap.
    pub fn set_max_stake(env: Env, max_amount: Option<i128>) -> Result<(), ContractError> {
        Self::schedule_max_stake(env, max_amount)
    }

    /// Returns the current maximum stake cap, if set.
    pub fn get_max_stake(env: Env) -> Option<i128> {
        let key = DataKey::MaxStake;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    }

    /// Schedules a timelocked exposure cap update (alias for [`Self::schedule_max_user_exposure`]).
    /// Pass `None` to disable the cap.
    pub fn set_max_user_exposure(
        env: Env,
        max_exposure: Option<i128>,
    ) -> Result<(), ContractError> {
        Self::schedule_max_user_exposure(env, max_exposure)
    }

    /// Returns the current per-user round exposure cap, if set.
    pub fn get_max_user_exposure(env: Env) -> Option<i128> {
        let key = DataKey::MaxUserRoundExposure;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    }

    // ─── Accounting safety (Issue #120) ─────────────────────────────────────

    /// Schedules a timelocked pending winnings cap update (alias for [`Self::schedule_max_pending_winnings`]).
    /// Pass `None` to disable the cap.
    pub fn set_max_pending_winnings(
        env: Env,
        max_pending: Option<i128>,
    ) -> Result<(), ContractError> {
        Self::schedule_max_pending_winnings(env, max_pending)
    }

    // ─── Timelocked critical config (governance safety) ─────────────────────

    /// Schedules a timelocked update to betting and execution windows (admin only).
    /// The change is stored pending until `apply_scheduled_changes` is called after the delay.
    pub fn schedule_windows(
        env: Env,
        bet_ledgers: u32,
        run_ledgers: u32,
    ) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        Self::_validate_windows(bet_ledgers, run_ledgers)?;
        Self::_schedule_config_change(
            &env,
            ConfigChangeKind::Windows,
            ConfigChangePayload::Windows(bet_ledgers, run_ledgers),
        )
    }

    /// Schedules a timelocked update to the maximum stake cap (admin only).
    pub fn schedule_max_stake(env: Env, max_amount: Option<i128>) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        Self::_validate_max_stake(max_amount)?;
        Self::_schedule_config_change(
            &env,
            ConfigChangeKind::MaxStake,
            ConfigChangePayload::MaxStake(max_amount),
        )
    }

    /// Schedules a timelocked update to the per-user round exposure cap (admin only).
    pub fn schedule_max_user_exposure(
        env: Env,
        max_exposure: Option<i128>,
    ) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        Self::_validate_max_stake(max_exposure)?;
        Self::_schedule_config_change(
            &env,
            ConfigChangeKind::MaxUserRoundExposure,
            ConfigChangePayload::MaxUserRoundExposure(max_exposure),
        )
    }

    /// Schedules a timelocked update to the pending winnings cap (admin only).
    pub fn schedule_max_pending_winnings(
        env: Env,
        max_pending: Option<i128>,
    ) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        Self::_validate_max_stake(max_pending)?;
        Self::_schedule_config_change(
            &env,
            ConfigChangeKind::MaxPendingWinnings,
            ConfigChangePayload::MaxPendingWinnings(max_pending),
        )
    }

    /// Schedules a timelocked update to the oracle stale threshold (admin only).
    pub fn schedule_oracle_stale_threshold(env: Env, seconds: u64) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        Self::_validate_oracle_stale_threshold(seconds)?;
        Self::_schedule_config_change(
            &env,
            ConfigChangeKind::OracleStaleThreshold,
            ConfigChangePayload::OracleStaleThreshold(seconds),
        )
    }

    /// Schedules a timelocked update to the oracle max deviation threshold (admin only).
    pub fn schedule_oracle_deviation_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        Self::_validate_oracle_max_deviation_bps(bps)?;
        Self::_schedule_config_change(
            &env,
            ConfigChangeKind::OracleMaxDeviationBps,
            ConfigChangePayload::OracleMaxDeviationBps(bps),
        )
    }

    /// Returns a pending timelocked config change for the given kind, if any.
    pub fn get_pending_config_change(
        env: Env,
        kind: ConfigChangeKind,
    ) -> Option<PendingConfigChange> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingConfigChange(kind))
    }

    /// Applies a scheduled critical config change after its activation ledger (any caller).
    pub fn apply_scheduled_changes(env: Env, kind: ConfigChangeKind) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        Self::_ensure_not_paused(&env)?;

        let key = DataKey::PendingConfigChange(kind.clone());
        let pending: PendingConfigChange = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::CommitmentNotFound)?;

        let current_ledger = env.ledger().sequence();
        if current_ledger < pending.activation_ledger {
            return Err(ContractError::RoundNotEnded);
        }

        Self::_apply_config_payload(&env, &kind, &pending.payload)?;
        env.storage().persistent().remove(&key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("config"), symbol_short!("applied")),
            (kind, pending.activation_ledger),
        );

        Ok(())
    }

    /// Cancels a pending timelocked config change before activation (admin only).
    pub fn cancel_config_change(env: Env, kind: ConfigChangeKind) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        let key = DataKey::PendingConfigChange(kind.clone());
        let pending: PendingConfigChange = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::CommitmentNotFound)?;

        if env.ledger().sequence() >= pending.activation_ledger {
            return Err(ContractError::RoundNotCancellable);
        }

        let cancelled_at = env.ledger().sequence();
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("config"), symbol_short!("cancelled")),
            (kind, cancelled_at),
        );

        env.storage().persistent().remove(&key);

        Ok(())
    }

    /// Returns the current maximum pending winnings cap, if set.
    pub fn get_max_pending_winnings(env: Env) -> Option<i128> {
        let key = DataKey::MaxPendingWinnings;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    }

    // ─── Minimum participants (competitive settlement integrity) ─────────────

    /// Sets the minimum participant count required for competitive settlement (admin only).
    /// Rounds that end below this threshold are refunded to all participants.
    /// Pass `None` to disable the threshold.
    pub fn set_min_participants(env: Env, min: Option<u32>) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        let key = DataKey::MinParticipants;
        if let Some(v) = min {
            if v == 0 || v > MAX_MIN_PARTICIPANTS {
                return Err(ContractError::InvalidMinParticipants);
            }
            env.storage().persistent().set(&key, &v);
            Self::_extend_persistent_ttl(&env, &key);
        } else {
            env.storage().persistent().remove(&key);
        }
        Ok(())
    }

    /// Returns the current minimum participant threshold, if set.
    pub fn get_min_participants(env: Env) -> Option<u32> {
        let key = DataKey::MinParticipants;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    }

    /// Sets the maximum participant count for Precision rounds (admin only).
    /// The value must be in the range 1..=10_000. Unset contracts use the
    /// protocol default of 1_000 participants.
    pub fn set_max_precision_participants(env: Env, max: u32) -> Result<(), ContractError> {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        if max == 0 || max > MAX_PRECISION_PARTICIPANTS_LIMIT {
            return Err(ContractError::InvalidPrecisionParticipantCap);
        }

        let key = DataKey::MaxPrecisionParticipants;
        env.storage().persistent().set(&key, &max);
        Self::_extend_persistent_ttl(&env, &key);
        Ok(())
    }

    /// Returns the configured Precision participant cap, or the default if unset.
    pub fn get_max_precision_participants(env: Env) -> u32 {
        let key = DataKey::MaxPrecisionParticipants;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(DEFAULT_MAX_PRECISION_PARTICIPANTS)
    }

    // ─── Minimum bet floor (Issue #161) ─────────────────────────────────────

    /// Sets or clears the minimum bet/prediction amount (admin only, Issue #161).
    ///
    /// - `None`: disables the floor; any positive amount passes the
    ///   `_enforce_min_bet` check (this preserves the original v1 behaviour for
    ///   existing deployments).
    /// - `Some(v)`: any `place_bet` / `place_precision_prediction` /
    ///   `commit_prediction` with `amount < v` fails with
    ///   [`ContractError::InvalidBetAmount`]. The floor must lie in
    ///   `[MIN_MIN_BET_AMOUNT, MAX_MIN_BET_AMOUNT]`; out-of-range values are
    ///   rejected with the same error.
    ///
    /// NOTE: Both rejection paths reuse `InvalidBetAmount` because the
    /// Soroban `#[contracterror]` enum is hard-limited to 50 variants by the
    /// XDR spec (`VecM<T, 50>`); introducing a dedicated variant would push
    /// the enum past that ceiling and cause the macro to panic with
    /// `LengthExceedsMax` at compile time.
    ///
    /// Emits a `("min_bet", "updated")` event with the new value as payload so
    /// indexers can observe admin tuning without re-reading storage.
    pub fn set_min_bet(env: Env, min_bet: Option<i128>) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;
        Self::_validate_min_bet(min_bet)?;

        let key = DataKey::MinBet;
        if let Some(v) = min_bet {
            env.storage().persistent().set(&key, &v);
            Self::_extend_persistent_ttl(&env, &key);
        } else {
            env.storage().persistent().remove(&key);
        }

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("min_bet"), symbol_short!("updated")),
            min_bet,
        );

        Ok(())
    }

    /// Returns the configured minimum bet floor (Issue #161), or `None` if
    /// the floor is disabled.
    pub fn get_min_bet(env: Env) -> Option<i128> {
        let key = DataKey::MinBet;
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    }

    /// Returns user statistics (wins, losses, streaks)
    pub fn get_user_stats(env: Env, user: Address) -> UserStats {
        let key = DataKey::UserStats(user);
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key).unwrap_or(UserStats {
            total_wins: 0,
            total_losses: 0,
            current_streak: 0,
            best_streak: 0,
        })
    }

    /// Returns user's claimable winnings
    pub fn get_pending_winnings(env: Env, user: Address) -> i128 {
        let key = DataKey::PendingWinnings(user);
        Self::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    /// Places a bet on the active round (Up/Down mode only).
    ///
    /// Storage layout: each participant's position is stored under its own
    /// composite key `DataKey::Position(round_id, user)` — O(1) read/write
    /// regardless of how many other participants exist. An ordered participant
    /// list `DataKey::RoundParticipants(round_id)` is maintained for O(n)
    /// iteration at resolution time only.
    pub fn place_bet(
        env: Env,
        user: Address,
        amount: i128,
        side: BetSide,
    ) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        user.require_auth();
        Self::_ensure_not_paused(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidBetAmount);
        }

        // Enforce minimum bet floor (Issue #161) — checked early so a small
        // spam bet never costs the user a balance read or storage write.
        Self::_enforce_min_bet(&env, amount)?;

        // Enforce max stake cap (Issue #113)
        if let Some(max_stake) = env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::MaxStake)
        {
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
        if let Some(max_exposure) = env
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
        let new_balance = user_balance
            .checked_sub(amount)
            .ok_or(ContractError::Overflow)?;
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
        env.storage()
            .persistent()
            .set(&participants_key, &participants);

        // Update cached round pools and write once
        match side {
            BetSide::Up => {
                round.pool_up = round
                    .pool_up
                    .checked_add(amount)
                    .ok_or(ContractError::Overflow)?;
            }
            BetSide::Down => {
                round.pool_down = round
                    .pool_down
                    .checked_add(amount)
                    .ok_or(ContractError::Overflow)?;
            }
        }
        env.storage()
            .persistent()
            .set(&DataKey::ActiveRound, &round);

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
            (user, round.round_id, amount, side_value),
        );

        Ok(())
    }

    /// Places a precision prediction on the active round (Precision/Legends mode only)
    /// predicted_price: price scaled to 4 decimals (e.g., 0.2297 → 2297)
    ///
    /// Per-user key `DataKey::PrecisionPosition(round_id, user)` gives O(1)
    /// write cost independent of participant count.
    pub fn place_precision_prediction(
        env: Env,
        user: Address,
        amount: i128,
        predicted_price: u128,
    ) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        user.require_auth();
        Self::_ensure_not_paused(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidBetAmount);
        }

        // Enforce minimum bet floor (Issue #161)
        Self::_enforce_min_bet(&env, amount)?;

        // Enforce max stake cap (Issue #113)
        if let Some(max_stake) = env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::MaxStake)
        {
            if amount > max_stake {
                return Err(ContractError::StakeExceedsMax);
            }
        }

        // Validate price scale (must be 4 decimal places, max value 9999 for 0.9999)
        // Reasonable max: 99999999 (9999.9999 XLM)
        if predicted_price > 99_999_999 {
            return Err(ContractError::InvalidPriceScale);
        }

        // Single read of the active round — cache in call scope
        let round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .ok_or(ContractError::NoActiveRound)?;

        // Enforce per-user round exposure cap (Issue #113)
        if let Some(max_exposure) = env
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
        if env.storage().persistent().has(&pred_key) || env.storage().persistent().has(&commit_key)
        {
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
        let new_balance = user_balance
            .checked_sub(amount)
            .ok_or(ContractError::Overflow)?;
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
        env.storage()
            .persistent()
            .set(&participants_key, &participants);

        // Emit event for precision prediction
        // Topic: ("predict", "price")
        // Payload: (user: Address, round_id: u64, predicted_price: u128, amount: i128)
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("predict"), symbol_short!("price")),
            (user, round.round_id, predicted_price, amount),
        );

        Ok(())
    }

    /// Alias for place_precision_prediction - allows users to submit exact price predictions
    /// guessed_price: price scaled to 4 decimals (e.g., 0.2297 → 2297)
    pub fn predict_price(
        env: Env,
        user: Address,
        guessed_price: u128,
        amount: i128,
    ) -> Result<(), ContractError> {
        Self::place_precision_prediction(env, user, amount, guessed_price)
    }

    /// Commits a hashed prediction and stake amount (Precision mode only)
    pub fn commit_prediction(
        env: Env,
        user: Address,
        hash: BytesN<32>,
        amount: i128,
    ) -> Result<(), ContractError> {
        user.require_auth();
        Self::_ensure_not_paused(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidBetAmount);
        }

        // Enforce minimum bet floor (Issue #161)
        Self::_enforce_min_bet(&env, amount)?;

        // Enforce max stake cap
        if let Some(max_stake) = env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::MaxStake)
        {
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
        if let Some(max_exposure) = env
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
        if env.storage().persistent().has(&pred_key) || env.storage().persistent().has(&commit_key)
        {
            return Err(ContractError::AlreadyBet);
        }

        // Deduct balance
        let new_balance = user_balance
            .checked_sub(amount)
            .ok_or(ContractError::Overflow)?;
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
        env.storage()
            .persistent()
            .set(&participants_key, &participants);

        // Emit commit prediction event
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("commit"), symbol_short!("predict")),
            (user, round.round_id, hash, amount),
        );

        Ok(())
    }

    /// Reveals a previously committed prediction (Precision mode only)
    pub fn reveal_prediction(
        env: Env,
        user: Address,
        predicted_price: u128,
        salt: BytesN<32>,
    ) -> Result<(), ContractError> {
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
            (user, round.round_id, predicted_price, commitment.amount),
        );

        Ok(())
    }

    /// Returns user's position in the current round (Up/Down mode).
    ///
    /// Reads a single composite key `DataKey::Position(round_id, user)` — O(1).
    /// Falls back to legacy `UpDownPositions` / `Positions` map blobs for
    /// one-time migration compatibility.
    pub fn get_user_position(env: Env, user: Address) -> Option<UserPosition> {
        if let Some(round) = env
            .storage()
            .persistent()
            .get::<_, Round>(&DataKey::ActiveRound)
        {
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
    }

    /// Returns user's precision prediction in the current round (Precision mode).
    ///
    /// Reads a single composite key `DataKey::PrecisionPosition(round_id, user)` — O(1).
    /// Falls back to legacy `PrecisionPositions` map for migration compatibility.
    pub fn get_user_precision_prediction(env: Env, user: Address) -> Option<PrecisionPrediction> {
        if let Some(round) = env
            .storage()
            .persistent()
            .get::<_, Round>(&DataKey::ActiveRound)
        {
            let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
            if let Some(p) = env
                .storage()
                .persistent()
                .get::<_, PrecisionPrediction>(&pred_key)
            {
                return Some(p);
            }
        }
        let legacy: Map<Address, PrecisionPrediction> = env
            .storage()
            .persistent()
            .get(&DataKey::PrecisionPositions)
            .unwrap_or(Map::new(&env));
        legacy.get(user)
    }

    /// Returns all precision predictions for the current round.
    ///
    /// Reads the participant list once, then fetches each prediction individually.
    /// Total reads: 1 (participant list) + N (predictions) instead of 1 large map blob.
    pub fn get_precision_predictions(env: Env) -> Vec<PrecisionPrediction> {
        let round = match env
            .storage()
            .persistent()
            .get::<_, Round>(&DataKey::ActiveRound)
        {
            Some(r) => r,
            None => return Vec::new(&env),
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
    }

    /// Returns all Up/Down positions for the current round.
    ///
    /// Reads the participant list once, then fetches each position individually.
    pub fn get_updown_positions(env: Env) -> Map<Address, UserPosition> {
        let round = match env
            .storage()
            .persistent()
            .get::<_, Round>(&DataKey::ActiveRound)
        {
            Some(r) => r,
            None => return Map::new(&env),
        };

        let participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundParticipants(round.round_id))
            .unwrap_or(Vec::new(&env));

        let mut result: Map<Address, UserPosition> = Map::new(&env);
        for i in 0..participants.len() {
            if let Some(user) = participants.get(i) {
                let pos_key = DataKey::Position(round.round_id, user.clone());
                if let Some(pos) = env.storage().persistent().get(&pos_key) {
                    result.set(user, pos);
                }
            }
        }

        // Legacy fallback: pre-migration data lives in the bulk map
        if result.is_empty() {
            return env
                .storage()
                .persistent()
                .get(&DataKey::UpDownPositions)
                .unwrap_or(Map::new(&env));
        }
        result
    }
    // ─── Pagination (Issue #139) ─────────────────────────────────────────────

    /// Returns a deterministic slice of Precision-mode predictions for the
    /// active round, ordered by ascending participant address (the same
    /// canonical order used internally for payout-remainder assignment).
    ///
    /// `offset` is the zero-based index into the ordered participant list.
    /// `limit` is the maximum number of entries to return and is capped at
    /// `MAX_PAGE_SIZE` to bound gas/read costs regardless of caller input.
    ///
    /// Returns an empty `Vec` if there is no active round, if `offset` is
    /// beyond the number of available entries, or if `limit` is zero — this
    /// is not an error condition, matching standard pagination semantics
    /// (asking past the end of a list yields an empty page, not a fault).
    ///
    /// This does not replace [`Self::get_precision_predictions`], which
    /// remains available unchanged for full-set reads on small rounds.
    pub fn get_precision_predictions_page(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Vec<PrecisionPrediction> {
        let limit = limit.min(MAX_PAGE_SIZE);
        if limit == 0 {
            return Vec::new(&env);
        }

        let round = match env
            .storage()
            .persistent()
            .get::<_, Round>(&DataKey::ActiveRound)
        {
            Some(r) => r,
            None => return Vec::new(&env),
        };

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

        let mut result: Vec<PrecisionPrediction> = Vec::new(&env);
        for i in offset..end {
            if let Some(user) = participants.get(i) {
                let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
                if let Some(pred) = env.storage().persistent().get(&pred_key) {
                    result.push_back(pred);
                }
            }
        }

        result
    }

    /// Returns a deterministic slice of Up/Down positions for the active
    /// round, ordered by ascending participant address, as `(Address,
    /// UserPosition)` pairs.
    ///
    /// A `Vec` of pairs is used instead of a `Map` because pagination over a
    /// `Map` has no stable, caller-controllable slice semantics in Soroban —
    /// pairs preserve the exact offset/limit window the caller requested.
    ///
    /// See [`Self::get_precision_predictions_page`] for the offset/limit/empty-page
    /// contract, which is identical here. This does not replace
    /// [`Self::get_updown_positions`], which remains available unchanged.
    pub fn get_updown_positions_page(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Vec<(Address, UserPosition)> {
        let limit = limit.min(MAX_PAGE_SIZE);
        if limit == 0 {
            return Vec::new(&env);
        }

        let round = match env
            .storage()
            .persistent()
            .get::<_, Round>(&DataKey::ActiveRound)
        {
            Some(r) => r,
            None => return Vec::new(&env),
        };

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
        Self::_ensure_not_paused(&env)?;

        let round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .ok_or(ContractError::NoActiveRound)?;

        // Verify round ID matches to prevent cross-round replays
        if payload.round_id != round.start_ledger {
            return Err(ContractError::InvalidOracleRound);
        }

        // ─── Domain-context validation (Issue #143) ─────────────────────────
        // Reject payloads targeting a different network or contract deployment.
        if payload.network_id != env.ledger().network_id() {
            return Err(ContractError::OracleNetworkMismatch);
        }
        if payload.contract_addr != env.current_contract_address() {
            return Err(ContractError::OracleContractMismatch);
        }

        // Verify data freshness (max 300 seconds / 5 minutes old)
        let current_time = env.ledger().timestamp();

        // Reject future timestamps to prevent time-skew manipulation
        if payload.timestamp > current_time {
            return Err(ContractError::FutureOracleData);
        }

        if current_time > payload.timestamp + 300 {
            return Err(ContractError::StaleOracleData);
        }

        // ─── Oracle deviation guardrails (circuit-breaker) ───────────────────
        // Compare settlement price against round start price (trusted baseline).
        // If configured, reject large jumps unless an admin-armed one-shot override is set.
        Self::_extend_persistent_ttl(&env, &DataKey::OracleMaxDeviationBps);
        if let Some(max_bps) = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::OracleMaxDeviationBps)
        {
            let start_price = round.price_start;
            // start_price is validated at round creation; still guard division by zero.
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

            // Integer bps: floor(diff / start) * 10_000.
            // Use checked math so any u128 overflow maps to explicit errors.
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
                // One-shot override is consumed on use.
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

        // Per-round nonce replay guard (Issue #118).
        // Consume the nonce only after all validation passes so a rejected payload
        // doesn't permanently burn a nonce value.
        let nonce_key = DataKey::ConsumedOracleNonce(round.round_id, payload.nonce);
        if env.storage().persistent().has(&nonce_key) {
            return Err(ContractError::OracleNonceReused);
        }
        env.storage().persistent().set(&nonce_key, &true);

        // Verify round has reached end_ledger
        let current_ledger = env.ledger().sequence();
        if current_ledger < round.end_ledger {
            return Err(ContractError::RoundNotEnded);
        }

        // Store round ID before cleaning up
        let round_id = round.round_id;

        // ─── Minimum participants threshold check ────────────────────────────
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
                    (round_id, count, min),
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
                        (round_id, round.pool_up, round.pool_down),
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

        // Clean up legacy map keys if present (migration compat)
        env.storage().persistent().remove(&DataKey::ActiveRound);
        env.storage().persistent().remove(&DataKey::Positions);
        env.storage().persistent().remove(&DataKey::UpDownPositions);
        env.storage()
            .persistent()
            .remove(&DataKey::PrecisionPositions);

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
            (round_id, payload.price, mode_value),
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
        final_price: u128,
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
        let is_one_sided = (price_went_up && round.pool_down == 0 && round.pool_up > 0)
            || (price_went_down && round.pool_up == 0 && round.pool_down > 0);

        if !participants.is_empty() {
            if price_unchanged || is_one_sided {
                Self::_record_refunds_indexed(env, round.round_id, &participants)?;
            } else if price_went_up {
                Self::_record_winnings_indexed(
                    env,
                    round.round_id,
                    &participants,
                    BetSide::Up,
                    round.pool_up,
                    round.pool_down,
                )?;
            } else if price_went_down {
                Self::_record_winnings_indexed(
                    env,
                    round.round_id,
                    &participants,
                    BetSide::Down,
                    round.pool_down,
                    round.pool_up,
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
                    Self::_record_refunds_legacy(env, &positions)?;
                } else if price_went_up {
                    Self::_record_winnings_legacy(
                        env,
                        &positions,
                        BetSide::Up,
                        round.pool_up,
                        round.pool_down,
                    )?;
                } else if price_went_down {
                    Self::_record_winnings_legacy(
                        env,
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

    /// Legacy refund path — reads the bulk Map blob.
    /// Used only when migrating pre-existing rounds; new rounds use indexed keys.
    fn _record_refunds_legacy(
        env: &Env,
        positions: &Map<Address, UserPosition>,
    ) -> Result<(), ContractError> {
        let keys: Vec<Address> = positions.keys();
        for i in 0..keys.len() {
            if let Some(user) = keys.get(i) {
                if let Some(position) = positions.get(user.clone()) {
                    Self::_accumulate_pending(env, user, position.amount)?;
                }
            }
        }
        Ok(())
    }

    /// Legacy winnings path — reads the bulk Map blob.
    fn _record_winnings_legacy(
        env: &Env,
        positions: &Map<Address, UserPosition>,
        winning_side: BetSide,
        winning_pool: i128,
        losing_pool: i128,
    ) -> Result<(), ContractError> {
        if winning_pool == 0 {
            return Ok(());
        }
        let keys: Vec<Address> = positions.keys();
        for i in 0..keys.len() {
            if let Some(user) = keys.get(i) {
                if let Some(position) = positions.get(user.clone()) {
                    if position.side == winning_side {
                        let share_numerator = position
                            .amount
                            .checked_mul(losing_pool)
                            .ok_or(ContractError::Overflow)?;
                        let share = share_numerator / winning_pool;
                        let payout = position
                            .amount
                            .checked_add(share)
                            .ok_or(ContractError::Overflow)?;

                        Self::_accumulate_pending(env, user.clone(), payout)?;
                        Self::_update_stats_win(env, user)?;
                    } else {
                        Self::_update_stats_loss(env, user)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Resolves Precision/Legends mode round using indexed per-user prediction keys.
    ///
    /// Reads: 1 (participants list) + N (individual predictions).
    /// Awards full pot to closest guess(es); ties split evenly.
    /// Migration fallback: empty participant list → legacy `PrecisionPositions` map.
    fn _resolve_precision_mode(
        env: &Env,
        round_id: u64,
        final_price: u128,
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
            return Self::_resolve_precision_legacy(env, &legacy, final_price);
        }

        // Find minimum difference and collect all winners
        let mut min_diff: Option<u128> = None;
        let mut winners: Vec<PrecisionPrediction> = Vec::new(env);
        let mut total_pot: i128 = 0;

        // Single pass to build winners list and total pot
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

                // Add amount to total pot from prediction (revealed) or commitment (unrevealed)
                let amount = if let Some(ref pred) = pred_opt {
                    pred.amount
                } else if let Some(ref commit) = commitment_opt {
                    commit.amount
                } else {
                    0
                };

                total_pot = total_pot
                    .checked_add(amount)
                    .ok_or(ContractError::Overflow)?;

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
        }

        // Distribute winnings to winner(s).
        // Remainder policy: `participants` is sorted lexicographically; `winners` is built
        // in that same sorted order, so index 0 is always the winner with the lowest Address.
        // Any integer remainder from the even split is assigned to that winner, making the
        // distribution fully deterministic.
        if !winners.is_empty() && total_pot > 0 {
            let winner_count = winners.len() as i128;
            let payout_per_winner = total_pot / winner_count;
            let remainder = total_pot % winner_count;

            // Award to each winner
            for i in 0..winners.len() {
                if let Some(winner) = winners.get(i) {
                    // First winner (lowest XDR-ordered Address) absorbs the remainder.
                    let payout = if i == 0 {
                        payout_per_winner
                            .checked_add(remainder)
                            .ok_or(ContractError::Overflow)?
                    } else {
                        payout_per_winner
                    };

                    Self::_accumulate_pending(env, winner.user.clone(), payout)?;
                    Self::_update_stats_win(env, winner.user.clone())?;
                }
            }

            // Update stats for losers
            for i in 0..participants.len() {
                if let Some(user) = participants.get(i) {
                    let is_winner = winners.iter().any(|w| w.user == user);
                    if !is_winner {
                        Self::_update_stats_loss(env, user)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Legacy precision-mode resolution path — reads the bulk Map blob.
    /// Used only as a migration fallback; new rounds use indexed per-user keys.
    fn _resolve_precision_legacy(
        env: &Env,
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
                total_pot = Self::payout_add(total_pot, pred.amount)?;
            }
        }

        // Remainder policy: `predictions_map` is a `Map<Address, PrecisionPrediction>`, which
        // Soroban keeps sorted by XDR-encoded key bytes. `winners` is built by iterating
        // `predictions_map.values()` in that stable key order, so index 0 always refers to
        // the lexicographically-lowest Address. Any integer remainder from the even split is
        // assigned exclusively to that winner, making the distribution fully deterministic.
        if !winners.is_empty() && total_pot > 0 {
            let winner_count = winners.len() as i128;
            let payout_per_winner = total_pot / winner_count;
            let remainder = total_pot % winner_count;

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
                }
            }

            for i in 0..predictions.len() {
                if let Some(pred) = predictions.get(i) {
                    let is_winner = winners.iter().any(|w| w.user == pred.user);
                    if !is_winner {
                        Self::_update_stats_loss(env, pred.user.clone())?;
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
            .ok_or(ContractError::RoundNotCancellable)?;

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
                            Self::_accumulate_pending(&env, user, pos.amount)?;
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
                            Self::_accumulate_pending(&env, user.clone(), refund_amount)?;
                        }
                        env.storage().persistent().remove(&pred_key);
                        env.storage().persistent().remove(&commit_key);
                    }
                }
            }
        }

        // Clean up participant list and mark round as cancelled
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
        env.storage().persistent().remove(&DataKey::ActiveRound);

        // Emit cancellation event
        // Topic: ("round", "cancelled")
        // Payload: (round_id: u64, reason: u32, pool_up: i128, pool_down: i128)
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("round"), symbol_short!("cancelled")),
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
        Self::_set_balance(&env, user.clone(), new_balance);

        env.storage().persistent().remove(&key);

        // Emit claim event
        // Topic: ("claim", "winnings")
        // Payload: (user: Address, amount: i128)
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("claim"), symbol_short!("winnings")),
            (user, pending),
        );

        Ok(pending)
    }

    /// Records refunds when price unchanged — indexed variant.
    ///
    /// Reads N individual position keys (O(1) each); no full-map deserialisation.
    fn _record_refunds_indexed(
        env: &Env,
        round_id: u64,
        participants: &Vec<Address>,
    ) -> Result<(), ContractError> {
        for i in 0..participants.len() {
            if let Some(user) = participants.get(i) {
                let pos_key = DataKey::Position(round_id, user.clone());
                if let Some(position) = env.storage().persistent().get::<_, UserPosition>(&pos_key)
                {
                    Self::_accumulate_pending(env, user, position.amount)?;
                }
            }
        }
        Ok(())
    }

    /// Records winnings for winning side — indexed variant.
    ///
    /// Formula: payout = bet + (bet / winning_pool) * losing_pool
    /// Reads N individual position keys; no full-map deserialisation.
    fn _record_winnings_indexed(
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

        for i in 0..participants.len() {
            if let Some(user) = participants.get(i) {
                let pos_key = DataKey::Position(round_id, user.clone());
                if let Some(position) = env.storage().persistent().get::<_, UserPosition>(&pos_key)
                {
                    if position.side == winning_side {
                        // Compute all payout math before any storage write
                        let share_numerator = Self::payout_mul(position.amount, losing_pool)?;
                        let share = share_numerator / winning_pool;
                        let payout = Self::payout_add(position.amount, share)?;

                        Self::_accumulate_pending(env, user.clone(), payout)?;
                        Self::_update_stats_win(env, user)?;
                    } else {
                        Self::_update_stats_loss(env, user)?;
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

        let mut recent: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::RecentArchivedRoundIds)
            .unwrap_or(Vec::new(env));

        recent.push_back(round.round_id);

        while recent.len() > MAX_ARCHIVED_ROUNDS {
            if let Some(oldest) = recent.get(0) {
                env.storage()
                    .persistent()
                    .remove(&DataKey::ArchivedRound(oldest));
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

    /// Refunds all participant stakes when the minimum-participants threshold is not met.
    /// Performs the same key cleanup as normal resolution so the contract is left consistent.
    fn _refund_under_threshold(
        env: &Env,
        round: &Round,
        participants: &Vec<Address>,
    ) -> Result<(), ContractError> {
        let round_id = round.round_id;
        match round.mode {
            RoundMode::UpDown => {
                for i in 0..participants.len() {
                    if let Some(user) = participants.get(i) {
                        let pos_key = DataKey::Position(round_id, user.clone());
                        if let Some(pos) =
                            env.storage().persistent().get::<_, UserPosition>(&pos_key)
                        {
                            Self::_accumulate_pending(env, user, pos.amount)?;
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
                            Self::_accumulate_pending(env, user, pred.amount)?;
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

    pub(crate) fn _update_stats_win(env: &Env, user: Address) -> Result<(), ContractError> {
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

        stats.total_losses = stats
            .total_losses
            .checked_add(1)
            .ok_or(ContractError::Overflow)?;
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
        if Self::is_paused(env.clone()) {
            panic_with_error!(&env, ContractError::ContractPaused);
        }

        let key = DataKey::Balance(user.clone());

        if let Some(existing_balance) = env.storage().persistent().get(&key) {
            Self::_extend_persistent_ttl(&env, &key);
            return existing_balance;
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
            (user, initial_amount),
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
        Self::_extend_persistent_ttl(env, &DataKey::Paused);
        if Self::is_paused(env.clone()) {
            return Err(ContractError::ContractPaused);
        }

        Ok(())
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
        if let Some(cap) = env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::MaxPendingWinnings)
        {
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

    /// Validates the configured minimum bet value (Issue #161). `None` is the
    /// "disabled" sentinel and always valid; `Some(0)` is rejected because it
    /// would block every bet (use `None` instead); values above
    /// `MAX_MIN_BET_AMOUNT` are rejected to keep floors within the protocol's
    /// safe arithmetic range.
    fn _validate_min_bet(min_bet: Option<i128>) -> Result<(), ContractError> {
        if let Some(v) = min_bet {
            if !(MIN_MIN_BET_AMOUNT..=MAX_MIN_BET_AMOUNT).contains(&v) {
                return Err(ContractError::InvalidBetAmount);
            }
        }
        Ok(())
    }

    /// Enforces the configured minimum-bet floor (Issue #161).
    /// Returns `InvalidBetAmount` when a positive amount is strictly below
    /// the configured floor, and silently passes when no floor is set or
    /// when the configured floor is at or below the amount.
    fn _enforce_min_bet(env: &Env, amount: i128) -> Result<(), ContractError> {
        // Read-then-bump: only touch TTL when the floor is actually configured.
        // Avoids paying a persistent load + TTL bump on every Up/Down/Precision
        // bet when no min_bet is set (the default state for fresh deploys).
        let min_key = DataKey::MinBet;
        if let Some(min) = env.storage().persistent().get::<_, i128>(&min_key) {
            Self::_extend_persistent_ttl(env, &min_key);
            if amount < min {
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
        Self::_ensure_not_paused(env)?;

        let key = DataKey::PendingConfigChange(kind.clone());
        if env.storage().persistent().has(&key) {
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
            (symbol_short!("config"), symbol_short!("scheduled")),
            (kind, activation_ledger),
        );

        Ok(())
    }

    fn _apply_config_payload(
        env: &Env,
        kind: &ConfigChangeKind,
        payload: &ConfigChangePayload,
    ) -> Result<(), ContractError> {
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
            _ => return Err(ContractError::InvalidMode),
        }
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
    }

    fn sort_addresses(addresses: Vec<Address>) -> Vec<Address> {
        let mut sorted = Vec::new(addresses.env());
        for addr in addresses.iter() {
            let mut inserted = false;
            for i in 0..sorted.len() {
                if addr < sorted.get_unchecked(i) {
                    sorted.insert(i, addr.clone());
                    inserted = true;
                    break;
                }
            }
            if !inserted {
                sorted.push_back(addr);
            }
        }
        sorted
    }
}
