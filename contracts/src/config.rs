// SPDX-License-Identifier: MIT
use crate::admin::{_ensure_normal_mode, _ensure_not_paused, _require_supported_schema};
use crate::common::{
    _emit_action_rejected, _emit_config_updated, _extend_persistent_ttl, _extend_ttl_symbol,
    _set_balance, balance, payout_add, BPS_DENOMINATOR, CONFIG_TIMELOCK_LEDGERS,
    DEFAULT_ARCHIVE_RETENTION, DEFAULT_BET_WINDOW_LEDGERS, DEFAULT_CLOSE_BUFFER_LEDGERS,
    DEFAULT_DISPUTE_LEDGERS, DEFAULT_MAX_PRECISION_PARTICIPANTS, DEFAULT_ORACLE_STALE_THRESHOLD,
    DEFAULT_ORACLE_TIMESTAMP_SKEW, DEFAULT_PENDING_WINNINGS_EXPIRY, DEFAULT_RUN_WINDOW_LEDGERS,
    MAX_ARCHIVE_RETENTION, MAX_BET_WINDOW_LEDGERS, MAX_CLOSE_BUFFER_LEDGERS, MAX_DISPUTE_LEDGERS,
    MAX_MIN_PARTICIPANTS, MAX_ORACLE_DEVIATION_BPS, MAX_ORACLE_STALE_THRESHOLD,
    MAX_ORACLE_TIMESTAMP_SKEW, MAX_PENDING_WINNINGS_EXPIRY, MAX_PRECISION_PARTICIPANTS_LIMIT,
    MAX_PROTOCOL_FEE_BPS, MAX_RUN_WINDOW_LEDGERS, MAX_START_PRICE, MIN_ARCHIVE_RETENTION,
    MIN_CAP_VALUE, MIN_ORACLE_STALE_THRESHOLD, MIN_ORACLE_TIMESTAMP_SKEW,
    MIN_PENDING_WINNINGS_EXPIRY, MIN_START_PRICE,
};
use crate::errors::ContractError;
use crate::types::{
    ConfigChangeKind, ConfigChangePayload, DataKey, DataKeyCore, DataKeyScoped, FeeModel,
    PendingConfigChange, PrecisionPayoutPolicy, RoundTemplate, PENDING_WINNINGS_EXPIRY_KEY,
};
use soroban_sdk::{symbol_short, Address, Env, Symbol};

pub fn set_windows(env: Env, bet_ledgers: u32, run_ledgers: u32) -> Result<(), ContractError> {
    schedule_windows(env, bet_ledgers, run_ledgers)
}

pub fn set_max_stake(env: Env, max_amount: Option<i128>) -> Result<(), ContractError> {
    schedule_max_stake(env, max_amount)
}

pub fn get_max_stake(env: Env) -> Option<i128> {
    let key = DataKeyCore::MaxStake;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Schedules a timelocked minimum-bet (dust protection) update (Issue #269).
pub fn set_min_bet(env: Env, min_amount: Option<i128>) -> Result<(), ContractError> {
    schedule_min_bet(env, min_amount)
}

pub fn schedule_min_bet(env: Env, min_amount: Option<i128>) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _validate_min_bet(min_amount)?;
    _schedule_config_change(
        &env,
        ConfigChangeKind::MinBet,
        ConfigChangePayload::MinBet(min_amount),
    )
}

/// Returns the configured minimum bet, if enabled. `None` disables the check
/// entirely, preserving pre-#269 behaviour (any positive amount accepted).
pub fn get_min_bet(env: Env) -> Option<i128> {
    let key = DataKeyCore::MinBet;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

pub fn set_max_user_exposure(env: Env, max_exposure: Option<i128>) -> Result<(), ContractError> {
    schedule_max_user_exposure(env, max_exposure)
}

pub fn get_max_user_exposure(env: Env) -> Option<i128> {
    let key = DataKeyCore::MaxUserRoundExposure;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

pub fn set_max_pending_winnings(env: Env, max_pending: Option<i128>) -> Result<(), ContractError> {
    schedule_max_pending_winnings(env, max_pending)
}

pub fn schedule_windows(env: Env, bet_ledgers: u32, run_ledgers: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _validate_windows(bet_ledgers, run_ledgers)?;
    _schedule_config_change(
        &env,
        ConfigChangeKind::Windows,
        ConfigChangePayload::Windows(bet_ledgers, run_ledgers),
    )
}

pub fn schedule_max_stake(env: Env, max_amount: Option<i128>) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _validate_max_stake(max_amount)?;
    _schedule_config_change(
        &env,
        ConfigChangeKind::MaxStake,
        ConfigChangePayload::MaxStake(max_amount),
    )
}

pub fn schedule_max_user_exposure(
    env: Env,
    max_exposure: Option<i128>,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _validate_max_stake(max_exposure)?;
    _schedule_config_change(
        &env,
        ConfigChangeKind::MaxUserRoundExposure,
        ConfigChangePayload::MaxUserRoundExposure(max_exposure),
    )
}

pub fn schedule_max_pending_winnings(
    env: Env,
    max_pending: Option<i128>,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _validate_max_stake(max_pending)?;
    _schedule_config_change(
        &env,
        ConfigChangeKind::MaxPendingWinnings,
        ConfigChangePayload::MaxPendingWinnings(max_pending),
    )
}

pub fn schedule_oracle_stale_threshold(env: Env, seconds: u64) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _validate_oracle_stale_threshold(seconds)?;
    _schedule_config_change(
        &env,
        ConfigChangeKind::OracleStaleThreshold,
        ConfigChangePayload::OracleStaleThreshold(seconds),
    )
}

pub fn schedule_oracle_deviation_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _validate_oracle_max_deviation_bps(bps)?;
    _schedule_config_change(
        &env,
        ConfigChangeKind::OracleMaxDeviationBps,
        ConfigChangePayload::OracleMaxDeviationBps(bps),
    )
}

pub fn schedule_oracle_timestamp_skew(env: Env, seconds: u64) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _validate_oracle_timestamp_skew(seconds)?;
    _schedule_config_change(
        &env,
        ConfigChangeKind::OracleTimestampSkew,
        ConfigChangePayload::OracleTimestampSkew(seconds),
    )
}

pub fn get_oracle_timestamp_skew(env: Env) -> u64 {
    env.storage()
        .instance()
        .get(&symbol_short!("otskew"))
        .unwrap_or(DEFAULT_ORACLE_TIMESTAMP_SKEW)
}

pub fn schedule_protocol_fee_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _validate_protocol_fee_bps(bps)?;
    _schedule_config_change(
        &env,
        ConfigChangeKind::ProtocolFeeBps,
        ConfigChangePayload::ProtocolFeeBps(bps),
    )
}

pub fn set_protocol_fee_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
    schedule_protocol_fee_bps(env, bps)
}

pub fn get_protocol_fee_bps(env: Env) -> Option<u32> {
    let key = DataKeyCore::ProtocolFeeBps;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

pub fn get_protocol_fee_treasury(env: Env) -> i128 {
    let key = DataKeyCore::ProtocolFeeTreasury;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key).unwrap_or(0)
}

// ─── Fee incidence model (Issue #268) ────────────────────────────────────────

/// Sets the fee incidence model directly (admin only, no timelock).
///
/// The model determines whether the protocol fee is calculated on the total pot
/// (`FeeOnPot`, default) or only on net winnings / profit (`FeeOnWinnings`).
pub fn set_fee_model(env: Env, model: FeeModel) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("fee_mod"), e);
    })?;

    let key = DataKeyCore::FeeModel;
    let old_model = _read_fee_model(&env);
    env.storage().persistent().set(&key, &model);
    _extend_persistent_ttl(&env, &key);

    _emit_config_updated(
        &env,
        ConfigChangeKind::FeeModel,
        ConfigChangePayload::FeeModel(old_model),
        ConfigChangePayload::FeeModel(model),
    );
    Ok(())
}

/// Returns the configured fee incidence model, defaulting to `FeeOnPot`.
pub fn get_fee_model(env: Env) -> FeeModel {
    _read_fee_model(&env)
}

pub fn withdraw_protocol_fee(
    env: Env,
    recipient: Address,
    amount: i128,
) -> Result<i128, ContractError> {
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
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("withdraw"), e);
    })?;

    if amount <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }

    let treasury_key = DataKeyCore::ProtocolFeeTreasury;
    let current: i128 = env.storage().persistent().get(&treasury_key).unwrap_or(0);
    if amount > current {
        return Err(ContractError::InsufficientBalance);
    }
    let new_treasury = current
        .checked_sub(amount)
        .ok_or(ContractError::InsufficientBalance)?;
    env.storage().persistent().set(&treasury_key, &new_treasury);
    _extend_persistent_ttl(&env, &treasury_key);

    let recipient_bal: i128 = balance(env.clone(), recipient.clone());
    let new_bal = payout_add(recipient_bal, amount)?;
    _set_balance(&env, recipient.clone(), new_bal);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("protocol"), symbol_short!("fee_with")),
        (recipient, amount, new_treasury),
    );

    Ok(amount)
}

pub fn get_pending_config_change(env: Env, kind: ConfigChangeKind) -> Option<PendingConfigChange> {
    env.storage()
        .persistent()
        .get(&DataKeyScoped::PendingConfigChange(kind))
}

pub fn apply_scheduled_changes(env: Env, kind: ConfigChangeKind) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _ensure_normal_mode(&env)?;

    let key = DataKeyScoped::PendingConfigChange(kind.clone());
    let pending: PendingConfigChange = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(ContractError::CommitmentNotFound)?;

    let current_ledger = env.ledger().sequence();
    if current_ledger < pending.activation_ledger {
        return Err(ContractError::RoundNotEnded);
    }

    _apply_config_payload(&env, &kind, &pending.payload)?;
    env.storage().persistent().remove(&key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("config"), symbol_short!("applied")),
        (kind, pending.activation_ledger),
    );

    Ok(())
}

pub fn cancel_config_change(env: Env, kind: ConfigChangeKind) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("cncl_cfg"), e);
    })?;

    let key = DataKeyScoped::PendingConfigChange(kind.clone());
    let pending: PendingConfigChange = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(ContractError::CommitmentNotFound)?;

    if env.ledger().sequence() >= pending.activation_ledger {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("cncl_cfg"),
            ContractError::RoundNotCancellable,
        );
        return Err(ContractError::RoundNotCancellable);
    }

    let cancelled_at = env.ledger().sequence();
    env.storage().persistent().remove(&key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("config"), symbol_short!("cancel")),
        (kind, cancelled_at),
    );

    Ok(())
}

pub fn get_max_pending_winnings(env: Env) -> Option<i128> {
    let key = DataKeyCore::MaxPendingWinnings;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

pub fn set_close_buffer_ledgers(env: Env, buffer_ledgers: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("closebuf"), e);
    })?;

    _validate_close_buffer_ledgers(buffer_ledgers)?;

    let key = DataKeyCore::CloseBufferLedgers;
    let old_buffer: u32 = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_CLOSE_BUFFER_LEDGERS);
    env.storage().persistent().set(&key, &buffer_ledgers);
    _extend_persistent_ttl(&env, &key);

    _emit_config_updated(
        &env,
        ConfigChangeKind::CloseBufferLedgers,
        ConfigChangePayload::CloseBufferLedgers(old_buffer),
        ConfigChangePayload::CloseBufferLedgers(buffer_ledgers),
    );
    Ok(())
}

pub fn get_close_buffer_ledgers(env: Env) -> u32 {
    let key = DataKeyCore::CloseBufferLedgers;
    _extend_persistent_ttl(&env, &key);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_CLOSE_BUFFER_LEDGERS)
}

/// Returns the configured betting-window length in ledgers (Issue #280).
pub fn get_bet_window_ledgers(env: Env) -> u32 {
    let key = DataKeyCore::BetWindowLedgers;
    _extend_persistent_ttl(&env, &key);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_BET_WINDOW_LEDGERS)
}

/// Returns the configured run-window length in ledgers (Issue #280).
pub fn get_run_window_ledgers(env: Env) -> u32 {
    let key = DataKeyCore::RunWindowLedgers;
    _extend_persistent_ttl(&env, &key);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_RUN_WINDOW_LEDGERS)
}

pub fn set_min_participants(env: Env, min: Option<u32>) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("min_par"), e);
    })?;

    let key = DataKeyCore::MinParticipants;
    let old_min: Option<u32> = env.storage().persistent().get(&key);
    if let Some(v) = min {
        if v == 0 || v > MAX_MIN_PARTICIPANTS {
            _emit_action_rejected(
                &env,
                &admin,
                symbol_short!("min_par"),
                ContractError::InvalidMinParticipants,
            );
            return Err(ContractError::InvalidMinParticipants);
        }
        env.storage().persistent().set(&key, &v);
        _extend_persistent_ttl(&env, &key);
    } else {
        env.storage().persistent().remove(&key);
    }
    _emit_config_updated(
        &env,
        ConfigChangeKind::MinParticipants,
        ConfigChangePayload::MinParticipants(old_min),
        ConfigChangePayload::MinParticipants(min),
    );
    Ok(())
}

pub fn get_min_participants(env: Env) -> Option<u32> {
    let key = DataKeyCore::MinParticipants;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

pub fn set_max_precision_participants(env: Env, max: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("max_prec"), e);
    })?;

    if max == 0 || max > MAX_PRECISION_PARTICIPANTS_LIMIT {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("max_prec"),
            ContractError::InvalidPrecisionCap,
        );
        return Err(ContractError::InvalidPrecisionCap);
    }

    let key = DataKeyCore::MaxPrecisionParticipants;
    let old_max: u32 = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_MAX_PRECISION_PARTICIPANTS);
    env.storage().persistent().set(&key, &max);
    _extend_persistent_ttl(&env, &key);
    _emit_config_updated(
        &env,
        ConfigChangeKind::MaxPrecisionParticipants,
        ConfigChangePayload::MaxPrecisionParticipants(old_max),
        ConfigChangePayload::MaxPrecisionParticipants(max),
    );
    Ok(())
}

pub fn get_max_precision_participants(env: Env) -> u32 {
    let key = DataKeyCore::MaxPrecisionParticipants;
    _extend_persistent_ttl(&env, &key);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_MAX_PRECISION_PARTICIPANTS)
}

pub fn set_precision_payout_policy(env: Env, policy: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("prec_pol"), e);
    })?;

    if policy > 1 {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("prec_pol"),
            ContractError::InvalidMode,
        );
        return Err(ContractError::InvalidMode);
    }

    let key = DataKeyCore::PrecisionPayoutPolicy;
    let old_policy: u32 = env.storage().persistent().get(&key).unwrap_or(0); // Default to 0 = Equal
    env.storage().persistent().set(&key, &policy);
    _extend_persistent_ttl(&env, &key);
    _emit_config_updated(
        &env,
        ConfigChangeKind::PrecisionPayoutPolicy,
        ConfigChangePayload::PrecisionPayoutPolicy(old_policy),
        ConfigChangePayload::PrecisionPayoutPolicy(policy),
    );
    Ok(())
}

pub fn get_precision_payout_policy(env: Env) -> u32 {
    let key = DataKeyCore::PrecisionPayoutPolicy;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key).unwrap_or(0) // Default to 0 = Equal
}

pub fn _read_precision_payout_policy(env: &Env) -> PrecisionPayoutPolicy {
    let key = DataKeyCore::PrecisionPayoutPolicy;
    _extend_persistent_ttl(env, &key);
    let policy_val: u32 = env.storage().persistent().get(&key).unwrap_or(0);
    if policy_val == 1 {
        PrecisionPayoutPolicy::StakeWeighted
    } else {
        PrecisionPayoutPolicy::Equal
    }
}

pub fn set_mint_limit(env: Env, limit: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("mint_lim"), e);
    })?;

    let old_limit: u32 = env
        .storage()
        .instance()
        .get(&DataKeyCore::MintLimitConfig)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKeyCore::MintLimitConfig, &limit);
    _emit_config_updated(
        &env,
        ConfigChangeKind::MintLimit,
        ConfigChangePayload::MintLimit(old_limit),
        ConfigChangePayload::MintLimit(limit),
    );
    Ok(())
}

pub fn get_mint_limit(env: Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKeyCore::MintLimitConfig)
        .unwrap_or(0)
}

const EPOCH_MINT_BUDGET_KEY: Symbol = symbol_short!("EpMintBgt");

pub fn set_epoch_mint_budget(env: Env, budget: i128) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("epch_bgt"), e);
    })?;

    if budget < 0 {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("epch_bgt"),
            ContractError::InvalidBetAmount,
        );
        return Err(ContractError::InvalidBetAmount);
    }

    let old_budget: i128 = env
        .storage()
        .instance()
        .get(&EPOCH_MINT_BUDGET_KEY)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&EPOCH_MINT_BUDGET_KEY, &budget);
    _emit_config_updated(
        &env,
        ConfigChangeKind::EpochMintBudget,
        ConfigChangePayload::EpochMintBudget(old_budget),
        ConfigChangePayload::EpochMintBudget(budget),
    );
    Ok(())
}

pub fn get_epoch_mint_budget(env: Env) -> i128 {
    env.storage()
        .instance()
        .get(&EPOCH_MINT_BUDGET_KEY)
        .unwrap_or(0)
}

pub fn set_archive_retention(env: Env, limit: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("set_arch"), e);
    })?;

    if !(MIN_ARCHIVE_RETENTION..=MAX_ARCHIVE_RETENTION).contains(&limit) {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("set_arch"),
            ContractError::WindowOutOfRange,
        );
        return Err(ContractError::WindowOutOfRange);
    }

    let key = DataKeyCore::ArchiveRetention;
    let old_limit: u32 = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_ARCHIVE_RETENTION);
    env.storage().persistent().set(&key, &limit);
    _extend_persistent_ttl(&env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("archive"), symbol_short!("retention")),
        (limit,),
    );
    _emit_config_updated(
        &env,
        ConfigChangeKind::ArchiveRetention,
        ConfigChangePayload::ArchiveRetention(old_limit),
        ConfigChangePayload::ArchiveRetention(limit),
    );

    Ok(())
}

pub fn get_archive_retention(env: Env) -> u32 {
    let key = DataKeyCore::ArchiveRetention;
    _extend_persistent_ttl(&env, &key);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_ARCHIVE_RETENTION)
}

// ─── Round templates (create-next keeper) ────────────────────────────────────

/// Stores the admin's blueprint for `create_next_from_template` (admin only).
///
/// Validated with the exact same rules `create_round` applies, so a template
/// can never produce a round that `create_round` itself would reject.
pub fn set_round_template(
    env: Env,
    start_price: u128,
    mode: Option<u32>,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("set_tmpl"), e);
    })?;

    _validate_round_template(start_price, mode).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("set_tmpl"), e);
    })?;

    let key = DataKeyCore::RoundTemplate;
    env.storage()
        .persistent()
        .set(&key, &RoundTemplate { start_price, mode });
    _extend_persistent_ttl(&env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("template"), symbol_short!("set")),
        (start_price, mode.unwrap_or(0)),
    );
    Ok(())
}

/// Removes the configured round template (admin only).
pub fn clear_round_template(env: Env) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("clr_tmpl"), e);
    })?;

    let key = DataKeyCore::RoundTemplate;
    if !env.storage().persistent().has(&key) {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("clr_tmpl"),
            ContractError::CommitmentNotFound,
        );
        return Err(ContractError::CommitmentNotFound);
    }
    env.storage().persistent().remove(&key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("template"), symbol_short!("cleared")),
        (env.ledger().sequence(),),
    );
    Ok(())
}

/// Returns the configured round template, if any.
pub fn get_round_template(env: Env) -> Option<RoundTemplate> {
    let key = DataKeyCore::RoundTemplate;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

// ─── Dispute window (Issue #276) ──────────────────────────────────────────────

fn _dispute_ledgers_key(env: &Env) -> Symbol {
    Symbol::new(env, "DisputeLedgers")
}

/// Sets the dispute window length in ledgers (admin only, immediate).
/// `0` preserves current behaviour — no dispute window, immediate settlement.
pub fn set_dispute_ledgers(env: Env, ledgers: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("set_dsp"), e);
    })?;

    if ledgers > MAX_DISPUTE_LEDGERS {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("set_dsp"),
            ContractError::WindowOutOfRange,
        );
        return Err(ContractError::WindowOutOfRange);
    }

    let key = _dispute_ledgers_key(&env);
    let old: u32 = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_DISPUTE_LEDGERS);
    env.storage().persistent().set(&key, &ledgers);
    _extend_ttl_symbol(&env, &key);

    _emit_config_updated(
        &env,
        ConfigChangeKind::DisputeLedgers,
        ConfigChangePayload::DisputeLedgers(old),
        ConfigChangePayload::DisputeLedgers(ledgers),
    );
    Ok(())
}

pub fn get_dispute_ledgers(env: &Env) -> u32 {
    let key = _dispute_ledgers_key(env);
    let v: u32 = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_DISPUTE_LEDGERS);
    if v > 0 {
        _extend_ttl_symbol(env, &key);
    }
    v
}

// ─── Early cash-out (Issue #271) ────────────────────────────────────────────

/// Sets the early cash-out penalty rate in basis points (admin only).
/// `None` disables early cash-out entirely (default).
pub fn set_early_cashout_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("ec_bps"), e);
    })?;

    if let Some(v) = bps {
        if v == 0 || v > MAX_PROTOCOL_FEE_BPS {
            _emit_action_rejected(
                &env,
                &admin,
                symbol_short!("ec_bps"),
                ContractError::InvalidProtocolFeeBps,
            );
            return Err(ContractError::InvalidProtocolFeeBps);
        }
    }

    let key = DataKeyCore::EarlyCashoutBps;
    let old_bps: Option<u32> = env.storage().persistent().get(&key);
    if let Some(v) = bps {
        env.storage().persistent().set(&key, &v);
        _extend_persistent_ttl(&env, &key);
    } else {
        env.storage().persistent().remove(&key);
    }

    #[allow(deprecated)]
    env.events()
        .publish((symbol_short!("config"), symbol_short!("ec_bps")), (bps,));
    _emit_config_updated(
        &env,
        ConfigChangeKind::EarlyCashoutBps,
        ConfigChangePayload::EarlyCashoutBps(old_bps),
        ConfigChangePayload::EarlyCashoutBps(bps),
    );
    Ok(())
}

/// Returns the configured early cash-out penalty bps, if enabled.
pub fn get_early_cashout_bps(env: Env) -> Option<u32> {
    let key = DataKeyCore::EarlyCashoutBps;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

// ─── Pending winnings expiry (Issue #269) ────────────────────────────────────

/// Schedules a timelocked pending winnings expiry update.
pub fn schedule_pending_winnings_expiry(env: Env, ledgers: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _validate_pending_winnings_expiry(ledgers)?;
    _schedule_config_change(
        &env,
        ConfigChangeKind::PendingWinningsExpiry,
        ConfigChangePayload::PendingWinningsExpiry(ledgers),
    )
}

pub fn set_pending_winnings_expiry(env: Env, ledgers: u32) -> Result<(), ContractError> {
    schedule_pending_winnings_expiry(env, ledgers)
}

pub fn get_pending_winnings_expiry(env: Env) -> u32 {
    _extend_persistent_ttl(&env, &PENDING_WINNINGS_EXPIRY_KEY);
    env.storage()
        .persistent()
        .get(&PENDING_WINNINGS_EXPIRY_KEY)
        .unwrap_or(DEFAULT_PENDING_WINNINGS_EXPIRY)
}

// ─── Validation helpers ─────────────────────────────────────────────────────

pub fn _validate_pending_winnings_expiry(ledgers: u32) -> Result<(), ContractError> {
    if ledgers != 0
        && (ledgers < MIN_PENDING_WINNINGS_EXPIRY || ledgers > MAX_PENDING_WINNINGS_EXPIRY)
    {
        return Err(ContractError::InvalidDuration);
    }
    Ok(())
}

pub fn _validate_round_template(start_price: u128, mode: Option<u32>) -> Result<(), ContractError> {
    if start_price < MIN_START_PRICE || start_price > MAX_START_PRICE {
        return Err(ContractError::InvalidStartPrice);
    }
    if let Some(m) = mode {
        if m > 1 {
            return Err(ContractError::InvalidMode);
        }
    }
    Ok(())
}

pub fn _validate_windows(bet_ledgers: u32, run_ledgers: u32) -> Result<(), ContractError> {
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

pub fn _validate_close_buffer_ledgers(buffer_ledgers: u32) -> Result<(), ContractError> {
    if buffer_ledgers > MAX_CLOSE_BUFFER_LEDGERS {
        return Err(ContractError::WindowOutOfRange);
    }
    Ok(())
}

pub fn _validate_max_stake(max_amount: Option<i128>) -> Result<(), ContractError> {
    if let Some(v) = max_amount {
        if v < MIN_CAP_VALUE {
            return Err(ContractError::InvalidBetAmount);
        }
    }
    Ok(())
}

pub fn _validate_min_bet(min_amount: Option<i128>) -> Result<(), ContractError> {
    if let Some(v) = min_amount {
        if v < MIN_CAP_VALUE {
            return Err(ContractError::InvalidBetAmount);
        }
    }
    Ok(())
}

pub fn _validate_oracle_stale_threshold(seconds: u64) -> Result<(), ContractError> {
    if !(MIN_ORACLE_STALE_THRESHOLD..=MAX_ORACLE_STALE_THRESHOLD).contains(&seconds) {
        return Err(ContractError::InvalidDuration);
    }
    Ok(())
}

fn _validate_oracle_timestamp_skew(seconds: u64) -> Result<(), ContractError> {
    if !(MIN_ORACLE_TIMESTAMP_SKEW..=MAX_ORACLE_TIMESTAMP_SKEW).contains(&seconds) {
        return Err(ContractError::InvalidDuration);
    }
    Ok(())
}

pub fn _validate_oracle_max_deviation_bps(bps: Option<u32>) -> Result<(), ContractError> {
    if let Some(v) = bps {
        if v == 0 || v > MAX_ORACLE_DEVIATION_BPS {
            return Err(ContractError::WindowOutOfRange);
        }
    }
    Ok(())
}

pub fn _validate_protocol_fee_bps(bps: Option<u32>) -> Result<(), ContractError> {
    if let Some(v) = bps {
        if v == 0 || v > MAX_PROTOCOL_FEE_BPS {
            return Err(ContractError::InvalidProtocolFeeBps);
        }
    }
    Ok(())
}

// ─── Fee helpers ─────────────────────────────────────────────────────────────

/// Default fee incidence model: fee-on-pot for backward compatibility.
pub const DEFAULT_FEE_MODEL: FeeModel = FeeModel::FeeOnPot;

pub fn _read_fee_model(env: &Env) -> FeeModel {
    let key = DataKeyCore::FeeModel;
    let v: Option<FeeModel> = env.storage().persistent().get(&key);
    if v.is_some() {
        _extend_persistent_ttl(env, &key);
    }
    v.unwrap_or(DEFAULT_FEE_MODEL)
}

pub fn _read_protocol_fee_bps(env: &Env) -> Option<u32> {
    let key = DataKeyCore::ProtocolFeeBps;
    let v: Option<u32> = env.storage().persistent().get(&key);
    if v.is_some() {
        _extend_persistent_ttl(env, &key);
    }
    v
}

pub fn _collect_protocol_fee(
    env: &Env,
    round_id: u64,
    fee_amount: i128,
    bps_active: Option<u32>,
    model: FeeModel,
) -> Result<(), ContractError> {
    if fee_amount <= 0 {
        return Ok(());
    }

    // Insurance fund split (Issue #367): a configurable portion of the
    // fee goes to the segregated insurance fund, the remainder to ops.
    let insurance_amount = crate::insurance::collect_insurance_fee(env, round_id, fee_amount)?;
    let ops_amount = fee_amount
        .checked_sub(insurance_amount)
        .ok_or(ContractError::Overflow)?;

    let treasury_key = DataKeyCore::ProtocolFeeTreasury;
    let current: i128 = env.storage().persistent().get(&treasury_key).unwrap_or(0);
    let new_treasury = current
        .checked_add(ops_amount)
        .ok_or(ContractError::Overflow)?;
    env.storage().persistent().set(&treasury_key, &new_treasury);
    _extend_persistent_ttl(env, &treasury_key);

    let bps_value: u32 = bps_active.unwrap_or(0);
    let model_value: u32 = model as u32;

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("protocol"), symbol_short!("fee_coll")),
        (round_id, fee_amount, new_treasury, bps_value, model_value),
    );

    Ok(())
}

pub fn calculate_protocol_fee_updown(
    bps: Option<u32>,
    model: FeeModel,
    winning_pool: i128,
    losing_pool: i128,
) -> Result<(i128, i128, i128), ContractError> {
    if bps.is_none() {
        return Ok((winning_pool, losing_pool, 0));
    }
    let bps_value = bps.unwrap();

    let fee_amount = match model {
        FeeModel::FeeOnPot => {
            let total_pot = payout_add(winning_pool, losing_pool)?;
            total_pot
                .checked_mul(bps_value as i128)
                .ok_or(ContractError::Overflow)?
                / BPS_DENOMINATOR
        }
        FeeModel::FeeOnWinnings => {
            losing_pool
                .checked_mul(bps_value as i128)
                .ok_or(ContractError::Overflow)?
                / BPS_DENOMINATOR
        }
    };

    if fee_amount == 0 {
        return Ok((winning_pool, losing_pool, 0));
    }

    match model {
        FeeModel::FeeOnPot => {
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
            Ok((dist_winning, dist_losing, fee_amount))
        }
        FeeModel::FeeOnWinnings => {
            let dist_losing = losing_pool
                .checked_sub(fee_amount)
                .ok_or(ContractError::Overflow)?;
            Ok((winning_pool, dist_losing, fee_amount))
        }
    }
}

pub fn _apply_protocol_fee_updown(
    env: &Env,
    round_id: u64,
    winning_pool: i128,
    losing_pool: i128,
) -> Result<(i128, i128, i128), ContractError> {
    let bps = _read_protocol_fee_bps(env);
    let model = _read_fee_model(env);
    let (dist_winning, dist_losing, fee_amount) =
        calculate_protocol_fee_updown(bps, model, winning_pool, losing_pool)?;
    if fee_amount > 0 {
        _collect_protocol_fee(env, round_id, fee_amount, bps, model)?;
    }
    Ok((dist_winning, dist_losing, fee_amount))
}

pub fn calculate_protocol_fee_precision(
    bps: Option<u32>,
    model: FeeModel,
    total_pot: i128,
    winner_stakes: i128,
) -> Result<(i128, i128), ContractError> {
    if bps.is_none() || total_pot <= 0 {
        return Ok((total_pot, 0));
    }
    let bps_value = bps.unwrap();

    let taxable_base = match model {
        FeeModel::FeeOnPot => total_pot,
        FeeModel::FeeOnWinnings => {
            let profit = total_pot
                .checked_sub(winner_stakes)
                .ok_or(ContractError::Overflow)?;
            if profit <= 0 {
                return Ok((total_pot, 0));
            }
            profit
        }
    };

    let fee_amount = taxable_base
        .checked_mul(bps_value as i128)
        .ok_or(ContractError::Overflow)?
        / BPS_DENOMINATOR;
    if fee_amount == 0 {
        return Ok((total_pot, 0));
    }
    let distributable = total_pot
        .checked_sub(fee_amount)
        .ok_or(ContractError::Overflow)?;
    Ok((distributable, fee_amount))
}

pub fn _apply_protocol_fee_precision(
    env: &Env,
    round_id: u64,
    total_pot: i128,
    winner_stakes: i128,
) -> Result<(i128, i128), ContractError> {
    let bps = _read_protocol_fee_bps(env);
    let model = _read_fee_model(env);
    let (distributable, fee_amount) =
        calculate_protocol_fee_precision(bps, model, total_pot, winner_stakes)?;
    if fee_amount > 0 {
        _collect_protocol_fee(env, round_id, fee_amount, bps, model)?;
    }
    Ok((distributable, fee_amount))
}

// ─── Internal helpers ───────────────────────────────────────────────────────

pub fn _current_config_payload(env: &Env, kind: &ConfigChangeKind) -> ConfigChangePayload {
    match kind {
        ConfigChangeKind::Windows => {
            let bet: u32 = env
                .storage()
                .persistent()
                .get(&DataKeyCore::BetWindowLedgers)
                .unwrap_or(DEFAULT_BET_WINDOW_LEDGERS);
            let run: u32 = env
                .storage()
                .persistent()
                .get(&DataKeyCore::RunWindowLedgers)
                .unwrap_or(DEFAULT_RUN_WINDOW_LEDGERS);
            ConfigChangePayload::Windows(bet, run)
        }
        ConfigChangeKind::MaxStake => {
            ConfigChangePayload::MaxStake(env.storage().persistent().get(&DataKeyCore::MaxStake))
        }
        ConfigChangeKind::MaxUserRoundExposure => ConfigChangePayload::MaxUserRoundExposure(
            env.storage()
                .persistent()
                .get(&DataKeyCore::MaxUserRoundExposure),
        ),
        ConfigChangeKind::MaxPendingWinnings => ConfigChangePayload::MaxPendingWinnings(
            env.storage()
                .persistent()
                .get(&DataKeyCore::MaxPendingWinnings),
        ),
        ConfigChangeKind::OracleStaleThreshold => ConfigChangePayload::OracleStaleThreshold(
            env.storage()
                .persistent()
                .get(&DataKeyCore::OracleStaleThreshold)
                .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD),
        ),
        ConfigChangeKind::OracleMaxDeviationBps => ConfigChangePayload::OracleMaxDeviationBps(
            env.storage()
                .persistent()
                .get(&DataKeyCore::OracleMaxDeviationBps),
        ),
        ConfigChangeKind::ProtocolFeeBps => ConfigChangePayload::ProtocolFeeBps(
            env.storage().persistent().get(&DataKeyCore::ProtocolFeeBps),
        ),
        ConfigChangeKind::MinParticipants => ConfigChangePayload::MinParticipants(
            env.storage()
                .persistent()
                .get(&DataKeyCore::MinParticipants),
        ),
        ConfigChangeKind::MaxPrecisionParticipants => {
            ConfigChangePayload::MaxPrecisionParticipants(
                env.storage()
                    .persistent()
                    .get(&DataKeyCore::MaxPrecisionParticipants)
                    .unwrap_or(DEFAULT_MAX_PRECISION_PARTICIPANTS),
            )
        }
        ConfigChangeKind::MintLimit => ConfigChangePayload::MintLimit(
            env.storage()
                .instance()
                .get(&DataKeyCore::MintLimitConfig)
                .unwrap_or(0),
        ),
        ConfigChangeKind::ArchiveRetention => ConfigChangePayload::ArchiveRetention(
            env.storage()
                .persistent()
                .get(&DataKeyCore::ArchiveRetention)
                .unwrap_or(DEFAULT_ARCHIVE_RETENTION),
        ),
        ConfigChangeKind::CloseBufferLedgers => ConfigChangePayload::CloseBufferLedgers(
            env.storage()
                .persistent()
                .get(&DataKeyCore::CloseBufferLedgers)
                .unwrap_or(DEFAULT_CLOSE_BUFFER_LEDGERS),
        ),
        ConfigChangeKind::OracleTimestampSkew => ConfigChangePayload::OracleTimestampSkew(
            env.storage()
                .instance()
                .get(&symbol_short!("otskew"))
                .unwrap_or(DEFAULT_ORACLE_TIMESTAMP_SKEW),
        ),
        ConfigChangeKind::PendingWinningsExpiry => ConfigChangePayload::PendingWinningsExpiry(
            env.storage()
                .persistent()
                .get(&PENDING_WINNINGS_EXPIRY_KEY)
                .unwrap_or(DEFAULT_PENDING_WINNINGS_EXPIRY),
        ),
        ConfigChangeKind::MinBet => {
            ConfigChangePayload::MinBet(env.storage().persistent().get(&DataKeyCore::MinBet))
        }
        ConfigChangeKind::EpochMintBudget => ConfigChangePayload::EpochMintBudget(
            env.storage()
                .instance()
                .get(&EPOCH_MINT_BUDGET_KEY)
                .unwrap_or(0),
        ),
        ConfigChangeKind::PrecisionPayoutPolicy => ConfigChangePayload::PrecisionPayoutPolicy(
            env.storage()
                .persistent()
                .get(&DataKeyCore::PrecisionPayoutPolicy)
                .unwrap_or(0),
        ),
        ConfigChangeKind::DisputeLedgers => ConfigChangePayload::DisputeLedgers(
            env.storage()
                .persistent()
                .get(&_dispute_ledgers_key(env))
                .unwrap_or(DEFAULT_DISPUTE_LEDGERS),
        ),
        ConfigChangeKind::FeeModel => ConfigChangePayload::FeeModel(_read_fee_model(env)),
        ConfigChangeKind::EarlyCashoutBps => ConfigChangePayload::EarlyCashoutBps(
            env.storage()
                .persistent()
                .get(&DataKeyCore::EarlyCashoutBps),
        ),
    }
}

pub fn _schedule_config_change(
    env: &Env,
    kind: ConfigChangeKind,
    payload: ConfigChangePayload,
) -> Result<(), ContractError> {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(env).inspect_err(|&e| {
        _emit_action_rejected(env, &admin, symbol_short!("sched"), e);
    })?;

    let key = DataKeyScoped::PendingConfigChange(kind.clone());
    if env.storage().persistent().has(&key) {
        _emit_action_rejected(
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
    _extend_persistent_ttl(env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("config"), symbol_short!("sched")),
        (kind, activation_ledger),
    );

    Ok(())
}

pub fn _apply_config_payload(
    env: &Env,
    kind: &ConfigChangeKind,
    payload: &ConfigChangePayload,
) -> Result<(), ContractError> {
    let old_value = _current_config_payload(env, kind);
    match (kind, payload) {
        (ConfigChangeKind::Windows, ConfigChangePayload::Windows(bet, run)) => {
            _validate_windows(*bet, *run)?;
            env.storage()
                .persistent()
                .set(&DataKeyCore::BetWindowLedgers, bet);
            _extend_persistent_ttl(env, &DataKeyCore::BetWindowLedgers);
            env.storage()
                .persistent()
                .set(&DataKeyCore::RunWindowLedgers, run);
            _extend_persistent_ttl(env, &DataKeyCore::RunWindowLedgers);
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("windows"), symbol_short!("updated")),
                (*bet, *run),
            );
        }
        (ConfigChangeKind::MaxStake, ConfigChangePayload::MaxStake(max)) => {
            _validate_max_stake(*max)?;
            let key = DataKeyCore::MaxStake;
            if let Some(v) = max {
                env.storage().persistent().set(&key, v);
                _extend_persistent_ttl(env, &key);
            } else {
                env.storage().persistent().remove(&key);
            }
        }
        (ConfigChangeKind::CloseBufferLedgers, ConfigChangePayload::CloseBufferLedgers(buffer)) => {
            _validate_close_buffer_ledgers(*buffer)?;
            let key = DataKeyCore::CloseBufferLedgers;
            env.storage().persistent().set(&key, buffer);
            _extend_persistent_ttl(env, &key);
        }
        (
            ConfigChangeKind::MaxUserRoundExposure,
            ConfigChangePayload::MaxUserRoundExposure(max),
        ) => {
            _validate_max_stake(*max)?;
            let key = DataKeyCore::MaxUserRoundExposure;
            if let Some(v) = max {
                env.storage().persistent().set(&key, v);
                _extend_persistent_ttl(env, &key);
            } else {
                env.storage().persistent().remove(&key);
            }
        }
        (ConfigChangeKind::MaxPendingWinnings, ConfigChangePayload::MaxPendingWinnings(max)) => {
            _validate_max_stake(*max)?;
            let key = DataKeyCore::MaxPendingWinnings;
            if let Some(v) = max {
                env.storage().persistent().set(&key, v);
                _extend_persistent_ttl(env, &key);
            } else {
                env.storage().persistent().remove(&key);
            }
        }
        (
            ConfigChangeKind::OracleStaleThreshold,
            ConfigChangePayload::OracleStaleThreshold(seconds),
        ) => {
            _validate_oracle_stale_threshold(*seconds)?;
            let key = DataKeyCore::OracleStaleThreshold;
            env.storage().persistent().set(&key, seconds);
            _extend_persistent_ttl(env, &key);
        }
        (
            ConfigChangeKind::OracleMaxDeviationBps,
            ConfigChangePayload::OracleMaxDeviationBps(bps),
        ) => {
            _validate_oracle_max_deviation_bps(*bps)?;
            let key = DataKeyCore::OracleMaxDeviationBps;
            if let Some(v) = bps {
                env.storage().persistent().set(&key, v);
                _extend_persistent_ttl(env, &key);
            } else {
                env.storage().persistent().remove(&key);
            }
        }
        (
            ConfigChangeKind::OracleTimestampSkew,
            ConfigChangePayload::OracleTimestampSkew(seconds),
        ) => {
            _validate_oracle_timestamp_skew(*seconds)?;
            env.storage()
                .instance()
                .set(&symbol_short!("otskew"), seconds);
        }
        (
            ConfigChangeKind::PendingWinningsExpiry,
            ConfigChangePayload::PendingWinningsExpiry(ledgers),
        ) => {
            _validate_pending_winnings_expiry(*ledgers)?;
            env.storage()
                .persistent()
                .set(&PENDING_WINNINGS_EXPIRY_KEY, ledgers);
            _extend_persistent_ttl(env, &PENDING_WINNINGS_EXPIRY_KEY);
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("pending"), symbol_short!("expiry")),
                (*ledgers,),
            );
        }
        (ConfigChangeKind::MinBet, ConfigChangePayload::MinBet(min)) => {
            _validate_min_bet(*min)?;
            let key = DataKeyCore::MinBet;
            if let Some(v) = min {
                env.storage().persistent().set(&key, v);
                _extend_persistent_ttl(env, &key);
            } else {
                env.storage().persistent().remove(&key);
            }
        }
        (ConfigChangeKind::ProtocolFeeBps, ConfigChangePayload::ProtocolFeeBps(bps)) => {
            _validate_protocol_fee_bps(*bps)?;
            let key = DataKeyCore::ProtocolFeeBps;
            if let Some(v) = bps {
                env.storage().persistent().set(&key, v);
                _extend_persistent_ttl(env, &key);
            } else {
                env.storage().persistent().remove(&key);
            }
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("protocol"), symbol_short!("fee_bps")),
                (*bps,),
            );
        }
        (
            ConfigChangeKind::PrecisionPayoutPolicy,
            ConfigChangePayload::PrecisionPayoutPolicy(policy),
        ) => {
            if *policy > 1 {
                return Err(ContractError::InvalidMode);
            }
            let key = DataKeyCore::PrecisionPayoutPolicy;
            env.storage().persistent().set(&key, policy);
            _extend_persistent_ttl(env, &key);
        }
        (ConfigChangeKind::DisputeLedgers, ConfigChangePayload::DisputeLedgers(ledgers)) => {
            if *ledgers > MAX_DISPUTE_LEDGERS {
                return Err(ContractError::WindowOutOfRange);
            }
            let key = _dispute_ledgers_key(env);
            env.storage().persistent().set(&key, ledgers);
            _extend_ttl_symbol(env, &key);
        }
        (ConfigChangeKind::FeeModel, ConfigChangePayload::FeeModel(model)) => {
            let key = DataKeyCore::FeeModel;
            env.storage().persistent().set(&key, model);
            _extend_persistent_ttl(env, &key);
        }
        (ConfigChangeKind::EpochMintBudget, ConfigChangePayload::EpochMintBudget(budget)) => {
            if *budget < 0 {
                return Err(ContractError::InvalidBetAmount);
            }
            env.storage().instance().set(&EPOCH_MINT_BUDGET_KEY, budget);
        }
        (ConfigChangeKind::MintLimit, ConfigChangePayload::MintLimit(limit)) => {
            env.storage()
                .instance()
                .set(&DataKeyCore::MintLimitConfig, limit);
        }
        (ConfigChangeKind::ArchiveRetention, ConfigChangePayload::ArchiveRetention(limit)) => {
            if !(MIN_ARCHIVE_RETENTION..=MAX_ARCHIVE_RETENTION).contains(limit) {
                return Err(ContractError::WindowOutOfRange);
            }
            let key = DataKeyCore::ArchiveRetention;
            env.storage().persistent().set(&key, limit);
            _extend_persistent_ttl(env, &key);
        }
        (ConfigChangeKind::MinParticipants, ConfigChangePayload::MinParticipants(min)) => {
            if let Some(v) = min {
                if *v == 0 || *v > MAX_MIN_PARTICIPANTS {
                    return Err(ContractError::InvalidMinParticipants);
                }
            }
            let key = DataKeyCore::MinParticipants;
            if let Some(v) = min {
                env.storage().persistent().set(&key, v);
                _extend_persistent_ttl(env, &key);
            } else {
                env.storage().persistent().remove(&key);
            }
        }
        (
            ConfigChangeKind::MaxPrecisionParticipants,
            ConfigChangePayload::MaxPrecisionParticipants(max),
        ) => {
            if *max == 0 || *max > MAX_PRECISION_PARTICIPANTS_LIMIT {
                return Err(ContractError::InvalidPrecisionCap);
            }
            let key = DataKeyCore::MaxPrecisionParticipants;
            env.storage().persistent().set(&key, max);
            _extend_persistent_ttl(env, &key);
        }
        (ConfigChangeKind::EarlyCashoutBps, ConfigChangePayload::EarlyCashoutBps(bps)) => {
            if let Some(v) = bps {
                if *v == 0 || *v > MAX_PROTOCOL_FEE_BPS {
                    return Err(ContractError::InvalidProtocolFeeBps);
                }
            }
            let key = DataKeyCore::EarlyCashoutBps;
            if let Some(v) = bps {
                env.storage().persistent().set(&key, v);
                _extend_persistent_ttl(env, &key);
            } else {
                env.storage().persistent().remove(&key);
            }
        }
        _ => return Err(ContractError::InvalidMode),
    }
    _emit_config_updated(env, kind.clone(), old_value, payload.clone());
    Ok(())
}
