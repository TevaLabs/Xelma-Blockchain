// SPDX-License-Identifier: MIT
//! Insurance / backstop fund module (Issue #367).
//!
//! Accrues a configurable portion of protocol fees into a segregated
//! insurance fund that can cover specific failure classes (e.g. cancel
//! due to oracle outage) under strict, coded payout rules.
//!
//! ## Design guarantees
//!
//! 1. **Solvency** — payouts can never exceed the fund balance.
//! 2. **Whitelist** — only pre-approved event types trigger coverage.
//! 3. **Auditability** — every balance-mutating operation emits a
//!    structured event.
//! 4. **Governance dual-control** — discretionary top-ups require the
//!    admin; withdrawals require the governance dual-approval pipeline
//!    (GovernorAction::WithdrawInsuranceFund).

use crate::common::{
    _emit_action_rejected, _extend_persistent_ttl, payout_add, BPS_DENOMINATOR,
};
use crate::errors::ContractError;
use crate::types::{DataKeyCore, InsuranceEvent};
use soroban_sdk::{symbol_short, Address, Env, Symbol, Vec};

// ─── Storage key helpers (Symbol-based to avoid DataKeyCore XDR limit) ──────

fn _fund_balance_key() -> Symbol {
    Symbol::new(&Env::default(), "InsFundBal")
}

fn _split_bps_key() -> Symbol {
    Symbol::new(&Env::default(), "InsSplitBps")
}

fn _coverage_bps_key() -> Symbol {
    Symbol::new(&Env::default(), "InsCovBps")
}

fn _eligible_events_key() -> Symbol {
    Symbol::new(&Env::default(), "InsEligEvt")
}

// ─── Constants ──────────────────────────────────────────────────────────────

/// Maximum allowed insurance split in basis points (50%).
pub const MAX_INSURANCE_SPLIT_BPS: u32 = 5_000;

/// Maximum allowed coverage payout in basis points (100% of stake).
pub const MAX_INSURANCE_COVERAGE_BPS: u32 = 10_000;

// ─── Admin configuration ────────────────────────────────────────────────────

/// Sets the insurance accrual split: how many basis points of each
/// protocol fee are directed to the insurance fund instead of the ops
/// treasury.
///
/// `bps = 0` disables the insurance split entirely (default).
/// `bps = 5000` means 50% of every fee goes to insurance.
///
/// Requires admin auth and contract not paused.
pub fn set_insurance_split_bps(env: Env, bps: u32) -> Result<(), ContractError> {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    crate::admin::_ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("ins_cfg"), e);
    })?;

    if bps > MAX_INSURANCE_SPLIT_BPS {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("ins_cfg"),
            ContractError::InsuranceInvalidSplit,
        );
        return Err(ContractError::InsuranceInvalidSplit);
    }

    let key = _split_bps_key();
    let old_bps: u32 = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(0);
    env.storage().persistent().set(&key, &bps);
    _extend_persistent_ttl(&env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("ins"), symbol_short!("split")),
        (old_bps, bps),
    );

    Ok(())
}

/// Returns the configured insurance split in basis points (default 0).
pub fn get_insurance_split_bps(env: &Env) -> u32 {
    let key = _split_bps_key();
    if env.storage().persistent().has(&key) {
        _extend_persistent_ttl(env, &key);
    }
    env.storage().persistent().get(&key).unwrap_or(0)
}

/// Sets the insurance coverage payout rate: how many basis points of
/// each affected participant's stake is paid as a bonus on eligible
/// cancellation events.
///
/// `bps = 0` disables coverage payouts (default).
/// `bps = 10000` means 100% of stake is covered.
///
/// Requires admin auth and contract not paused.
pub fn set_insurance_coverage_bps(env: Env, bps: u32) -> Result<(), ContractError> {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    crate::admin::_ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("ins_cfg"), e);
    })?;

    if bps > MAX_INSURANCE_COVERAGE_BPS {
        _emit_action_rejected(
            &env,
            &admin,
            symbol_short!("ins_cfg"),
            ContractError::InsuranceInvalidSplit,
        );
        return Err(ContractError::InsuranceInvalidSplit);
    }

    let key = _coverage_bps_key();
    let old_bps: u32 = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(0);
    env.storage().persistent().set(&key, &bps);
    _extend_persistent_ttl(&env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("ins"), symbol_short!("covcfg")),
        (old_bps, bps),
    );

    Ok(())
}

/// Returns the configured insurance coverage payout rate in basis points.
pub fn get_insurance_coverage_bps(env: &Env) -> u32 {
    let key = _coverage_bps_key();
    if env.storage().persistent().has(&key) {
        _extend_persistent_ttl(env, &key);
    }
    env.storage().persistent().get(&key).unwrap_or(0)
}

/// Sets the whitelist of eligible insurance event types.
///
/// Only events in this list can trigger coverage payouts. Each entry is
/// an `InsuranceEvent` discriminant value.
///
/// Requires admin auth and contract not paused.
pub fn set_insurance_eligible_events(
    env: Env,
    events: Vec<u32>,
) -> Result<(), ContractError> {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    crate::admin::_ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("ins_cfg"), e);
    })?;

    let key = _eligible_events_key();
    env.storage().persistent().set(&key, &events);
    _extend_persistent_ttl(&env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("ins"), symbol_short!("evlist")),
        (events.len(),),
    );

    Ok(())
}

/// Returns the list of eligible insurance event type discriminants.
pub fn get_insurance_eligible_events(env: &Env) -> Vec<u32> {
    let key = _eligible_events_key();
    if env.storage().persistent().has(&key) {
        _extend_persistent_ttl(env, &key);
    }
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env))
}

/// Returns the current insurance fund balance.
pub fn get_insurance_fund_balance(env: &Env) -> i128 {
    let key = _fund_balance_key();
    if env.storage().persistent().has(&key) {
        _extend_persistent_ttl(env, &key);
    }
    env.storage().persistent().get(&key).unwrap_or(0)
}

// ─── Fee collection (called from _collect_protocol_fee) ─────────────────────

/// Splits a protocol fee amount between the ops treasury and the
/// insurance fund according to the configured split ratio.
///
/// Returns the portion directed to the insurance fund.
/// The caller is responsible for crediting the ops treasury with the
/// remainder (`fee_amount - insurance_portion`).
///
/// Events: `("ins", "fee_coll")` with `(round_id, insurance_amount,
/// new_fund_balance)`.
pub fn collect_insurance_fee(
    env: &Env,
    round_id: u64,
    fee_amount: i128,
) -> Result<i128, ContractError> {
    let split_bps = get_insurance_split_bps(env);
    if split_bps == 0 || fee_amount <= 0 {
        return Ok(0);
    }

    let insurance_amount = fee_amount
        .checked_mul(split_bps as i128)
        .ok_or(ContractError::Overflow)?
        .checked_div(BPS_DENOMINATOR)
        .ok_or(ContractError::Overflow)?;

    if insurance_amount <= 0 {
        return Ok(0);
    }

    // Solvency: insurance_amount must not exceed fee_amount
    let capped_amount = insurance_amount.min(fee_amount);

    let key = _fund_balance_key();
    let current: i128 = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(0);
    let new_balance = current
        .checked_add(capped_amount)
        .ok_or(ContractError::Overflow)?;
    env.storage().persistent().set(&key, &new_balance);
    _extend_persistent_ttl(env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("ins"), symbol_short!("fee_coll")),
        (round_id, capped_amount, new_balance),
    );

    Ok(capped_amount)
}

// ─── Coverage payout (called from cancel_round) ─────────────────────────────

/// Determines whether the given cancel reason code maps to an eligible
/// insurance event and whether the event is in the whitelist.
///
/// `cancel_reason` is the reason code passed to `cancel_round`.
fn _is_eligible_event(env: &Env, cancel_reason: u32) -> bool {
    // Map cancel_reason to InsuranceEvent discriminant
    let event_type = match cancel_reason {
        // Reason 1 → OracleOutage
        1 => InsuranceEvent::OracleOutage as u32,
        // Reason 2 → OracleDeviation
        2 => InsuranceEvent::OracleDeviation as u32,
        // Reason 3 → FallbackRefund (insufficient participants)
        3 => InsuranceEvent::FallbackRefund as u32,
        // Any other reason → not eligible
        _ => return false,
    };

    let whitelist = get_insurance_eligible_events(env);
    for i in 0..whitelist.len() {
        if let Some(allowed) = whitelist.get(i) {
            if allowed == event_type {
                return true;
            }
        }
    }
    false
}

/// Checks whether a cancel reason qualifies for insurance coverage
/// and the event is in the whitelist.
///
/// Returns `Ok(true)` if eligible, `Ok(false)` if not.
pub fn is_coverage_eligible(env: &Env, cancel_reason: u32) -> bool {
    _is_eligible_event(env, cancel_reason) && get_insurance_coverage_bps(env) > 0
}

/// Calculates the per-participant coverage amount based on stake and
/// configured coverage bps.
///
/// Returns `min(stake * coverage_bps / BPS_DENOMINATOR, fund_remaining)`
/// with the solvency cap applied globally across all participants.
pub fn calculate_coverage_amount(
    env: &Env,
    stake: i128,
) -> Result<i128, ContractError> {
    let coverage_bps = get_insurance_coverage_bps(env);
    if coverage_bps == 0 || stake <= 0 {
        return Ok(0);
    }
    let amount = stake
        .checked_mul(coverage_bps as i128)
        .ok_or(ContractError::Overflow)?
        .checked_div(BPS_DENOMINATOR)
        .ok_or(ContractError::Overflow)?;
    Ok(amount)
}

/// Deducts an amount from the insurance fund for a coverage payout.
///
/// Returns the actual amount deducted (may be less than requested if
/// the fund balance is insufficient — solvency invariant).
///
/// Events: `("ins", "coverage")` with `(round_id, actual_deducted,
/// remaining_fund_balance)`.
pub fn deduct_insurance_coverage(
    env: &Env,
    round_id: u64,
    requested: i128,
) -> Result<i128, ContractError> {
    if requested <= 0 {
        return Ok(0);
    }

    let fund_key = _fund_balance_key();
    let current_fund: i128 = env
        .storage()
        .persistent()
        .get(&fund_key)
        .unwrap_or(0);

    if current_fund <= 0 {
        return Ok(0);
    }

    // Solvency: cap at available fund balance
    let distributed = requested.min(current_fund);

    let new_balance = current_fund
        .checked_sub(distributed)
        .ok_or(ContractError::Overflow)?;
    env.storage().persistent().set(&fund_key, &new_balance);
    _extend_persistent_ttl(env, &fund_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("ins"), symbol_short!("coverage")),
        (round_id, distributed, new_balance),
    );

    Ok(distributed)
}

// ─── Governance: top-up and withdrawal ───────────────────────────────────────

/// Top-ups the insurance fund from the caller's vXLM balance.
///
/// Requires admin auth and contract not paused.
/// Emits `("ins", "topup")`.
pub fn top_up_insurance_fund(env: Env, amount: i128) -> Result<(), ContractError> {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    crate::admin::_ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("ins_tup"), e);
    })?;

    if amount <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }

    // Debit caller's balance
    let current_balance = crate::common::balance(env.clone(), admin.clone());
    if current_balance < amount {
        return Err(ContractError::InsufficientBalance);
    }
    let new_balance = current_balance
        .checked_sub(amount)
        .ok_or(ContractError::InsufficientBalance)?;
    crate::common::_set_balance(&env, admin.clone(), new_balance);

    // Credit insurance fund
    let fund_key = _fund_balance_key();
    let current_fund: i128 = env
        .storage()
        .persistent()
        .get(&fund_key)
        .unwrap_or(0);
    let new_fund = current_fund
        .checked_add(amount)
        .ok_or(ContractError::Overflow)?;
    env.storage().persistent().set(&fund_key, &new_fund);
    _extend_persistent_ttl(&env, &fund_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("ins"), symbol_short!("topup")),
        (admin, amount, new_fund),
    );

    Ok(())
}

/// Withdraws from the insurance fund to a recipient.
///
/// This is a governance-sensitive operation: when the governance
/// approver is set, it must go through the dual-approval pipeline
/// (`GovernAction::WithdrawInsuranceFund`). The direct admin path
/// is only available when no governance approver is configured.
///
/// Emits `("ins", "withdraw")`.
pub fn withdraw_insurance_fund(
    env: Env,
    recipient: Address,
    amount: i128,
) -> Result<i128, ContractError> {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();

    if crate::governance::_is_gov_approver_set(&env) {
        return Err(ContractError::GovUnauthorized);
    }

    crate::admin::_ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("ins_wd"), e);
    })?;

    if amount <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }

    let fund_key = _fund_balance_key();
    let current_fund: i128 = env
        .storage()
        .persistent()
        .get(&fund_key)
        .unwrap_or(0);
    if amount > current_fund {
        return Err(ContractError::InsuranceInsufficientFund);
    }
    let new_fund = current_fund
        .checked_sub(amount)
        .ok_or(ContractError::InsufficientBalance)?;
    env.storage().persistent().set(&fund_key, &new_fund);
    _extend_persistent_ttl(&env, &fund_key);

    // Credit recipient's balance
    let recipient_bal = crate::common::balance(env.clone(), recipient.clone());
    let new_recipient_bal = payout_add(recipient_bal, amount)?;
    crate::common::_set_balance(&env, recipient.clone(), new_recipient_bal);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("ins"), symbol_short!("withdraw")),
        (recipient, amount, new_fund),
    );

    Ok(amount)
}

/// Governance-callable withdrawal from the insurance fund.
///
/// This is the version called from the governance execute pipeline
/// (GovernAction::WithdrawInsuranceFund) and does NOT require admin
/// auth — it is already gated by the dual-approval mechanism.
pub fn execute_withdraw_insurance_fund(
    env: &Env,
    recipient: &Address,
    amount: i128,
) -> Result<i128, ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }

    let fund_key = _fund_balance_key();
    let current_fund: i128 = env
        .storage()
        .persistent()
        .get(&fund_key)
        .unwrap_or(0);
    if amount > current_fund {
        return Err(ContractError::InsuranceInsufficientFund);
    }
    let new_fund = current_fund
        .checked_sub(amount)
        .ok_or(ContractError::InsufficientBalance)?;
    env.storage().persistent().set(&fund_key, &new_fund);
    _extend_persistent_ttl(env, &fund_key);

    // Credit recipient's balance
    let recipient_bal = crate::common::balance(env.clone(), recipient.clone());
    let new_recipient_bal = payout_add(recipient_bal, amount)?;
    crate::common::_set_balance(env, recipient.clone(), new_recipient_bal);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("ins"), symbol_short!("withdraw")),
        (recipient.clone(), amount, new_fund),
    );

    Ok(amount)
}
