// SPDX-License-Identifier: MIT
use crate::errors::ContractError;
use crate::types::{ConfigChangeKind, ConfigChangePayload, DataKey, Round, RoundPhase};
use soroban_sdk::{symbol_short, Address, Env, Symbol, Vec};

// â”€â”€â”€ Economic control limits â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub const MIN_CAP_VALUE: i128 = 1;
pub const MAX_MIN_PARTICIPANTS: u32 = 10_000;
pub const DEFAULT_MAX_PRECISION_PARTICIPANTS: u32 = 1_000;
pub const MAX_PRECISION_PARTICIPANTS_LIMIT: u32 = 10_000;
pub const MAX_PAGE_SIZE: u32 = 100;

// â”€â”€â”€ Oracle heartbeat limits â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub const DEFAULT_ORACLE_STALE_THRESHOLD: u64 = 3_600; // 1 hour
pub const MIN_ORACLE_STALE_THRESHOLD: u64 = 60; // 1 minute
pub const MAX_ORACLE_STALE_THRESHOLD: u64 = 86_400; // 24 hours

pub const DEFAULT_BET_WINDOW_LEDGERS: u32 = 6;
pub const DEFAULT_RUN_WINDOW_LEDGERS: u32 = 12;
pub const MAX_BET_WINDOW_LEDGERS: u32 = 1_440;
pub const MAX_RUN_WINDOW_LEDGERS: u32 = 2_880;

// â”€â”€â”€ Oracle deviation guardrails â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub const MAX_ORACLE_DEVIATION_BPS: u32 = 100_000;

// â”€â”€â”€ Protocol fee â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub const MAX_PROTOCOL_FEE_BPS: u32 = 1_000;
pub const BPS_DENOMINATOR: i128 = 10_000;

// â”€â”€â”€ Storage schema versioning â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub const CURRENT_SCHEMA_VERSION: u32 = 3;
// â”€â”€â”€ Start-price bounds â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub const MIN_START_PRICE: u128 = 1;
pub const MAX_START_PRICE: u128 = 1_000_000_000_000_000_000;
// â”€â”€â”€ Storage TTL Lifecycle Limits â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
pub const TTL_BUMP_THRESHOLD: u32 = 17_280; // ~1 day at 5-second ledgers
pub const TTL_BUMP_AMOUNT: u32 = 518_400; // ~30 days at 5-second ledgers

pub const DEFAULT_ARCHIVE_RETENTION: u32 = 128;
pub const MIN_ARCHIVE_RETENTION: u32 = 1;
pub const MAX_ARCHIVE_RETENTION: u32 = 10_000;
pub const CONFIG_TIMELOCK_LEDGERS: u32 = 1440;

/// Bumps/extends the TTL of the given persistent storage key if its remaining TTL
/// is less than the threshold. Enforces rent policy (Issue #142).
pub fn _extend_persistent_ttl(env: &Env, key: &DataKey) {
    if env.storage().persistent().has(key) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
    }
}

pub fn sort_addresses(addresses: Vec<Address>) -> Vec<Address> {
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

/// Checked addition for payout accumulation.
#[inline(always)]
pub fn payout_add(a: i128, b: i128) -> Result<i128, ContractError> {
    a.checked_add(b).ok_or(ContractError::PayoutOverflow)
}

#[inline(always)]
pub fn payout_mul(a: i128, b: i128) -> Result<i128, ContractError> {
    a.checked_mul(b).ok_or(ContractError::PayoutOverflow)
}

/// Accumulates `amount` into a user's pending winnings, enforcing the cap if set (Issue #120).
pub fn _accumulate_pending(env: &Env, user: Address, amount: i128) -> Result<(), ContractError> {
    let key = DataKey::PendingWinnings(user);
    let existing: i128 = env.storage().persistent().get(&key).unwrap_or(0);
    let new_pending = payout_add(existing, amount)?;

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
    _extend_persistent_ttl(env, &key);
    Ok(())
}

pub fn _emit_action_rejected(env: &Env, actor: &Address, action: Symbol, reason: ContractError) {
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("action"), symbol_short!("rejct")),
        (actor.clone(), action, reason as u32),
    );
}

pub fn _emit_config_updated(
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

pub fn _derive_round_phase(ledger_sequence: u32, round: &Round) -> RoundPhase {
    if ledger_sequence < round.bet_end_ledger {
        RoundPhase::Betting
    } else if ledger_sequence < round.end_ledger {
        RoundPhase::Running
    } else {
        RoundPhase::Resolvable
    }
}

pub fn assert_no_active_round(env: &Env) -> Result<(), ContractError> {
    if env.storage().persistent().has(&DataKey::ActiveRound) {
        return Err(ContractError::RoundAlreadyActive);
    }
    Ok(())
}

pub fn balance(env: Env, user: Address) -> i128 {
    let key = DataKey::Balance(user);
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key).unwrap_or(0)
}

pub fn _set_balance(env: &Env, user: Address, amount: i128) {
    let key = DataKey::Balance(user);
    env.storage().persistent().set(&key, &amount);
    _extend_persistent_ttl(env, &key);
}
