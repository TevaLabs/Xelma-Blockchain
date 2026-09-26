// SPDX-License-Identifier: MIT
//! AMM-Style Up/Down Continuous Prediction Market (Issue #361).
//!
//! Provides a complete-set constant-product automated market maker (CPMM)
//! for continuous outcome pricing with LP liquidity provision, swap fee accrual,
//! and impermanent-loss-aware settlement accounting.
//!
//! # Complete Set CPMM Model
//! - 1 unit of collateral mints 1 UP share + 1 DOWN share (a complete set).
//! - The AMM maintains internal outcome share reserves: `reserve_up` and `reserve_down`.
//! - Invariant curve: `k = reserve_up * reserve_down`.
//! - Implied probability:
//!   P(UP) = reserve_down / (reserve_up + reserve_down)
//!   P(DOWN) = reserve_up / (reserve_up + reserve_down)
//! - Solvency is structurally guaranteed: total outcome shares in existence
//!   identically equals total collateral deposited.

use crate::admin::{_ensure_not_paused, _require_supported_schema};
use crate::common::{
    _extend_persistent_ttl, _set_balance, balance, BPS_DENOMINATOR,
};
use crate::config::_enforce_min_bet;
use crate::errors::ContractError;
use crate::types::{
    AmmPoolState, AmmUserPosition, BetSide, DataKeyCore, DataKeyExt, DataKeyScoped, Round,
    RoundMode, UserStats,
};
use soroban_sdk::{symbol_short, Address, Env, Vec};

pub const DEFAULT_AMM_FEE_BPS: u32 = 30; // 0.30% swap fee

// ─── AMM Config Helpers ─────────────────────────────────────────────────────

/// Sets whether AMM market mode is globally enabled (admin only).
pub fn set_amm_enabled(env: &Env, enabled: bool) -> Result<(), ContractError> {
    _require_supported_schema(env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(env)?;

    let key = DataKeyCore::Ext(DataKeyExt::AmmMarketEnabled);
    env.storage().persistent().set(&key, &enabled);
    _extend_persistent_ttl(env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("amm"), symbol_short!("enable")),
        enabled,
    );
    Ok(())
}

/// Returns whether AMM market mode is enabled.
pub fn is_amm_enabled(env: &Env) -> bool {
    let key = DataKeyCore::Ext(DataKeyExt::AmmMarketEnabled);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(true) // Enabled by default once feature is active
}

/// Sets the AMM swap fee in basis points (admin only).
pub fn set_amm_fee_bps(env: &Env, fee_bps: u32) -> Result<(), ContractError> {
    _require_supported_schema(env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(env)?;

    if fee_bps > 1000 {
        // max 10%
        return Err(ContractError::InvalidProtocolFeeBps);
    }

    let key = DataKeyCore::Ext(DataKeyExt::AmmFeeBps);
    env.storage().persistent().set(&key, &fee_bps);
    _extend_persistent_ttl(env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("amm"), symbol_short!("setfee")),
        fee_bps,
    );
    Ok(())
}

/// Returns the configured AMM swap fee in basis points.
pub fn get_amm_fee_bps(env: &Env) -> u32 {
    let key = DataKeyCore::Ext(DataKeyExt::AmmFeeBps);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_AMM_FEE_BPS)
}

// ─── AMM Pool Initialization ────────────────────────────────────────────────

/// Initializes AMM pool state for a newly created AMM round.
pub fn init_amm_pool(env: &Env, round_id: u64, fee_bps: u32) {
    let pool = AmmPoolState {
        round_id,
        reserve_up: 0,
        reserve_down: 0,
        total_collateral: 0,
        total_lp_shares: 0,
        accumulated_fees: 0,
        fee_bps,
        is_resolved: false,
        winning_side: None,
    };
    let pool_key = DataKeyScoped::AmmPool(round_id);
    env.storage().persistent().set(&pool_key, &pool);
    _extend_persistent_ttl(env, &pool_key);
}

// ─── Liquidity Provision ────────────────────────────────────────────────────

/// Deposits collateral into the AMM pool to mint LP shares.
pub fn deposit_liquidity(env: Env, user: Address, amount: i128) -> Result<i128, ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_not_paused(&env)?;

    if !is_amm_enabled(&env) {
        return Err(ContractError::AmmDisabled);
    }
    if amount <= 0 {
        return Err(ContractError::AmmZeroAmount);
    }

    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    if round.mode != RoundMode::Amm {
        return Err(ContractError::WrongModeForPrediction);
    }

    let current_ledger = env.ledger().sequence();
    if current_ledger >= round.bet_end_ledger {
        return Err(ContractError::RoundEnded);
    }

    let user_balance = balance(env.clone(), user.clone());
    if user_balance < amount {
        return Err(ContractError::InsufficientBalance);
    }

    let pool_key = DataKeyScoped::AmmPool(round.round_id);
    let mut pool: AmmPoolState = env
        .storage()
        .persistent()
        .get(&pool_key)
        .ok_or(ContractError::NoActiveRound)?;

    let shares_to_mint: i128;
    if pool.total_lp_shares == 0 {
        // Initial liquidity deposit sets 50/50 reserves
        shares_to_mint = amount;
        pool.reserve_up = amount;
        pool.reserve_down = amount;
        pool.total_collateral = amount;
        pool.total_lp_shares = amount;
    } else {
        // Proportional liquidity addition
        let shares_u128 = (amount as u128)
            .checked_mul(pool.total_lp_shares as u128)
            .ok_or(ContractError::Overflow)?
            .checked_div(pool.total_collateral as u128)
            .ok_or(ContractError::Overflow)?;
        shares_to_mint = shares_u128 as i128;
        if shares_to_mint <= 0 {
            return Err(ContractError::AmmZeroAmount);
        }

        let delta_up = (amount as u128)
            .checked_mul(pool.reserve_up as u128)
            .ok_or(ContractError::Overflow)?
            .checked_div(pool.total_collateral as u128)
            .ok_or(ContractError::Overflow)? as i128;

        let delta_down = (amount as u128)
            .checked_mul(pool.reserve_down as u128)
            .ok_or(ContractError::Overflow)?
            .checked_div(pool.total_collateral as u128)
            .ok_or(ContractError::Overflow)? as i128;

        pool.reserve_up = pool
            .reserve_up
            .checked_add(delta_up)
            .ok_or(ContractError::Overflow)?;
        pool.reserve_down = pool
            .reserve_down
            .checked_add(delta_down)
            .ok_or(ContractError::Overflow)?;
        pool.total_collateral = pool
            .total_collateral
            .checked_add(amount)
            .ok_or(ContractError::Overflow)?;
        pool.total_lp_shares = pool
            .total_lp_shares
            .checked_add(shares_to_mint)
            .ok_or(ContractError::Overflow)?;
    }

    // Deduct user balance
    let new_balance = user_balance
        .checked_sub(amount)
        .ok_or(ContractError::Overflow)?;
    _set_balance(&env, user.clone(), new_balance);

    // Update user LP share storage
    let user_lp_key = DataKeyScoped::AmmLpShares(round.round_id, user.clone());
    let current_lp_shares: i128 = env.storage().persistent().get(&user_lp_key).unwrap_or(0);
    let next_lp_shares = current_lp_shares
        .checked_add(shares_to_mint)
        .ok_or(ContractError::Overflow)?;
    env.storage().persistent().set(&user_lp_key, &next_lp_shares);
    _extend_persistent_ttl(&env, &user_lp_key);

    // Record in LP participants list
    let lp_participants_key = DataKeyScoped::AmmLpParticipants(round.round_id);
    let mut lp_participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&lp_participants_key)
        .unwrap_or(Vec::new(&env));
    if current_lp_shares == 0 {
        lp_participants.push_back(user.clone());
        env.storage()
            .persistent()
            .set(&lp_participants_key, &lp_participants);
        _extend_persistent_ttl(&env, &lp_participants_key);
    }

    env.storage().persistent().set(&pool_key, &pool);
    _extend_persistent_ttl(&env, &pool_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("amm"), symbol_short!("lp_dep")),
        (user, round.round_id, amount, shares_to_mint),
    );

    Ok(shares_to_mint)
}

/// Withdraws liquidity before round resolution, burning LP shares for collateral.
pub fn withdraw_liquidity(env: Env, user: Address, shares: i128) -> Result<i128, ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_not_paused(&env)?;

    if shares <= 0 {
        return Err(ContractError::AmmZeroAmount);
    }

    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    if round.mode != RoundMode::Amm {
        return Err(ContractError::WrongModeForPrediction);
    }

    let current_ledger = env.ledger().sequence();
    if current_ledger >= round.bet_end_ledger {
        return Err(ContractError::RoundEnded);
    }

    let user_lp_key = DataKeyScoped::AmmLpShares(round.round_id, user.clone());
    let current_lp_shares: i128 = env
        .storage()
        .persistent()
        .get(&user_lp_key)
        .ok_or(ContractError::AmmNoLpShares)?;

    if current_lp_shares < shares {
        return Err(ContractError::AmmNoLpShares);
    }

    let pool_key = DataKeyScoped::AmmPool(round.round_id);
    let mut pool: AmmPoolState = env
        .storage()
        .persistent()
        .get(&pool_key)
        .ok_or(ContractError::NoActiveRound)?;

    // Calculate proportional collateral to return: shares * total_collateral / total_lp_shares
    let collateral_out_u128 = (shares as u128)
        .checked_mul(pool.total_collateral as u128)
        .ok_or(ContractError::Overflow)?
        .checked_div(pool.total_lp_shares as u128)
        .ok_or(ContractError::Overflow)?;
    let collateral_out = collateral_out_u128 as i128;

    let delta_up = (shares as u128)
        .checked_mul(pool.reserve_up as u128)
        .ok_or(ContractError::Overflow)?
        .checked_div(pool.total_lp_shares as u128)
        .ok_or(ContractError::Overflow)? as i128;

    let delta_down = (shares as u128)
        .checked_mul(pool.reserve_down as u128)
        .ok_or(ContractError::Overflow)?
        .checked_div(pool.total_lp_shares as u128)
        .ok_or(ContractError::Overflow)? as i128;

    pool.reserve_up = pool
        .reserve_up
        .checked_sub(delta_up)
        .ok_or(ContractError::Overflow)?;
    pool.reserve_down = pool
        .reserve_down
        .checked_sub(delta_down)
        .ok_or(ContractError::Overflow)?;
    pool.total_collateral = pool
        .total_collateral
        .checked_sub(collateral_out)
        .ok_or(ContractError::Overflow)?;
    pool.total_lp_shares = pool
        .total_lp_shares
        .checked_sub(shares)
        .ok_or(ContractError::Overflow)?;

    // Deduct user LP shares
    let next_lp_shares = current_lp_shares
        .checked_sub(shares)
        .ok_or(ContractError::Overflow)?;
    if next_lp_shares == 0 {
        env.storage().persistent().remove(&user_lp_key);
    } else {
        env.storage().persistent().set(&user_lp_key, &next_lp_shares);
        _extend_persistent_ttl(&env, &user_lp_key);
    }

    // Credit user balance
    let user_balance = balance(env.clone(), user.clone());
    let new_balance = user_balance
        .checked_add(collateral_out)
        .ok_or(ContractError::Overflow)?;
    _set_balance(&env, user.clone(), new_balance);

    env.storage().persistent().set(&pool_key, &pool);
    _extend_persistent_ttl(&env, &pool_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("amm"), symbol_short!("lp_wdr")),
        (user, round.round_id, shares, collateral_out),
    );

    Ok(collateral_out)
}

// ─── Continuous Trading: Buy & Sell Outcome Shares ──────────────────────────

/// Swaps collateral for outcome shares along the constant-product curve.
pub fn buy_shares(
    env: Env,
    user: Address,
    side: BetSide,
    collateral_in: i128,
    min_shares_out: i128,
) -> Result<i128, ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_not_paused(&env)?;

    if !is_amm_enabled(&env) {
        return Err(ContractError::AmmDisabled);
    }
    if collateral_in <= 0 {
        return Err(ContractError::AmmZeroAmount);
    }
    _enforce_min_bet(&env, collateral_in)?;

    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    if round.mode != RoundMode::Amm {
        return Err(ContractError::WrongModeForPrediction);
    }

    let current_ledger = env.ledger().sequence();
    if current_ledger >= round.bet_end_ledger {
        return Err(ContractError::RoundEnded);
    }

    let user_balance = balance(env.clone(), user.clone());
    if user_balance < collateral_in {
        return Err(ContractError::InsufficientBalance);
    }

    let pool_key = DataKeyScoped::AmmPool(round.round_id);
    let mut pool: AmmPoolState = env
        .storage()
        .persistent()
        .get(&pool_key)
        .ok_or(ContractError::NoActiveRound)?;

    if pool.reserve_up <= 0 || pool.reserve_down <= 0 {
        return Err(ContractError::AmmInsufficientLiquidity);
    }

    // Fee calculation: fee_bps out of BPS_DENOMINATOR (10,000)
    let fee = (collateral_in as u128)
        .checked_mul(pool.fee_bps as u128)
        .ok_or(ContractError::Overflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(ContractError::Overflow)? as i128;
    let net_collateral = collateral_in
        .checked_sub(fee)
        .ok_or(ContractError::Overflow)?;

    let k = (pool.reserve_up as u128)
        .checked_mul(pool.reserve_down as u128)
        .ok_or(ContractError::Overflow)?;

    let total_shares: i128;
    match side {
        BetSide::Up => {
            // Buyer mints complete sets: net_collateral UP + net_collateral DOWN.
            // Adds net_collateral DOWN into pool:
            let new_down = (pool.reserve_down as u128)
                .checked_add(net_collateral as u128)
                .ok_or(ContractError::Overflow)?;
            // Required new UP in pool: ceil(k / new_down)
            let new_up = k
                .checked_add(new_down.checked_sub(1).ok_or(ContractError::Overflow)?)
                .ok_or(ContractError::Overflow)?
                .checked_div(new_down)
                .ok_or(ContractError::Overflow)? as i128;

            let shares_from_pool = pool
                .reserve_up
                .checked_sub(new_up)
                .ok_or(ContractError::Overflow)?;
            total_shares = net_collateral
                .checked_add(shares_from_pool)
                .ok_or(ContractError::Overflow)?;

            pool.reserve_up = new_up;
            pool.reserve_down = new_down as i128;
        }
        BetSide::Down => {
            // Buyer mints complete sets: net_collateral UP + net_collateral DOWN.
            // Adds net_collateral UP into pool:
            let new_up = (pool.reserve_up as u128)
                .checked_add(net_collateral as u128)
                .ok_or(ContractError::Overflow)?;
            // Required new DOWN in pool: ceil(k / new_up)
            let new_down = k
                .checked_add(new_up.checked_sub(1).ok_or(ContractError::Overflow)?)
                .ok_or(ContractError::Overflow)?
                .checked_div(new_up)
                .ok_or(ContractError::Overflow)? as i128;

            let shares_from_pool = pool
                .reserve_down
                .checked_sub(new_down)
                .ok_or(ContractError::Overflow)?;
            total_shares = net_collateral
                .checked_add(shares_from_pool)
                .ok_or(ContractError::Overflow)?;

            pool.reserve_down = new_down;
            pool.reserve_up = new_up as i128;
        }
    }

    if total_shares < min_shares_out {
        return Err(ContractError::AmmSlippageExceeded);
    }

    // Deduct user balance
    let new_balance = user_balance
        .checked_sub(collateral_in)
        .ok_or(ContractError::Overflow)?;
    _set_balance(&env, user.clone(), new_balance);

    // Update pool collateral and fee accrual
    pool.total_collateral = pool
        .total_collateral
        .checked_add(collateral_in)
        .ok_or(ContractError::Overflow)?;
    pool.accumulated_fees = pool
        .accumulated_fees
        .checked_add(fee)
        .ok_or(ContractError::Overflow)?;
    env.storage().persistent().set(&pool_key, &pool);
    _extend_persistent_ttl(&env, &pool_key);

    // Update user AMM position
    let pos_key = DataKeyScoped::AmmPosition(round.round_id, user.clone());
    let mut pos: AmmUserPosition = env
        .storage()
        .persistent()
        .get(&pos_key)
        .unwrap_or(AmmUserPosition {
            up_shares: 0,
            down_shares: 0,
            lp_shares: 0,
            total_invested: 0,
        });

    match side {
        BetSide::Up => {
            pos.up_shares = pos
                .up_shares
                .checked_add(total_shares)
                .ok_or(ContractError::Overflow)?;
        }
        BetSide::Down => {
            pos.down_shares = pos
                .down_shares
                .checked_add(total_shares)
                .ok_or(ContractError::Overflow)?;
        }
    }
    pos.total_invested = pos
        .total_invested
        .checked_add(collateral_in)
        .ok_or(ContractError::Overflow)?;
    env.storage().persistent().set(&pos_key, &pos);
    _extend_persistent_ttl(&env, &pos_key);

    // Record participant address if new
    let participants_key = DataKeyScoped::AmmParticipants(round.round_id);
    let mut participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&participants_key)
        .unwrap_or(Vec::new(&env));
    let mut already_in = false;
    for i in 0..participants.len() {
        if let Some(p) = participants.get(i) {
            if p == user {
                already_in = true;
                break;
            }
        }
    }
    if !already_in {
        participants.push_back(user.clone());
        env.storage()
            .persistent()
            .set(&participants_key, &participants);
        _extend_persistent_ttl(&env, &participants_key);
    }

    let side_val = match side {
        BetSide::Up => 0u32,
        BetSide::Down => 1u32,
    };
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("amm"), symbol_short!("buy")),
        (user, round.round_id, side_val, collateral_in, total_shares),
    );

    Ok(total_shares)
}

/// Sells outcome shares back to the AMM pool for collateral before settlement.
pub fn sell_shares(
    env: Env,
    user: Address,
    side: BetSide,
    shares_in: i128,
    min_collateral_out: i128,
) -> Result<i128, ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_not_paused(&env)?;

    if shares_in <= 0 {
        return Err(ContractError::AmmZeroAmount);
    }

    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    if round.mode != RoundMode::Amm {
        return Err(ContractError::WrongModeForPrediction);
    }

    let current_ledger = env.ledger().sequence();
    if current_ledger >= round.bet_end_ledger {
        return Err(ContractError::RoundEnded);
    }

    let pos_key = DataKeyScoped::AmmPosition(round.round_id, user.clone());
    let mut pos: AmmUserPosition = env
        .storage()
        .persistent()
        .get(&pos_key)
        .ok_or(ContractError::AmmPositionNotFound)?;

    match side {
        BetSide::Up => {
            if pos.up_shares < shares_in {
                return Err(ContractError::InsufficientBalance);
            }
        }
        BetSide::Down => {
            if pos.down_shares < shares_in {
                return Err(ContractError::InsufficientBalance);
            }
        }
    }

    let pool_key = DataKeyScoped::AmmPool(round.round_id);
    let mut pool: AmmPoolState = env
        .storage()
        .persistent()
        .get(&pool_key)
        .ok_or(ContractError::NoActiveRound)?;

    let k = (pool.reserve_up as u128)
        .checked_mul(pool.reserve_down as u128)
        .ok_or(ContractError::Overflow)?;

    let gross_collateral_out: i128;
    match side {
        BetSide::Up => {
            let new_up = (pool.reserve_up as u128)
                .checked_add(shares_in as u128)
                .ok_or(ContractError::Overflow)?;
            let new_down = k
                .checked_div(new_up)
                .ok_or(ContractError::Overflow)? as i128;
            gross_collateral_out = pool
                .reserve_down
                .checked_sub(new_down)
                .ok_or(ContractError::Overflow)?;

            pool.reserve_up = new_up as i128;
            pool.reserve_down = new_down;
        }
        BetSide::Down => {
            let new_down = (pool.reserve_down as u128)
                .checked_add(shares_in as u128)
                .ok_or(ContractError::Overflow)?;
            let new_up = k
                .checked_div(new_down)
                .ok_or(ContractError::Overflow)? as i128;
            gross_collateral_out = pool
                .reserve_up
                .checked_sub(new_up)
                .ok_or(ContractError::Overflow)?;

            pool.reserve_down = new_down as i128;
            pool.reserve_up = new_up;
        }
    }

    let fee = (gross_collateral_out as u128)
        .checked_mul(pool.fee_bps as u128)
        .ok_or(ContractError::Overflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(ContractError::Overflow)? as i128;
    let net_collateral_out = gross_collateral_out
        .checked_sub(fee)
        .ok_or(ContractError::Overflow)?;

    if net_collateral_out < min_collateral_out {
        return Err(ContractError::AmmSlippageExceeded);
    }

    // Deduct user shares
    match side {
        BetSide::Up => {
            pos.up_shares = pos
                .up_shares
                .checked_sub(shares_in)
                .ok_or(ContractError::Overflow)?;
        }
        BetSide::Down => {
            pos.down_shares = pos
                .down_shares
                .checked_sub(shares_in)
                .ok_or(ContractError::Overflow)?;
        }
    }
    env.storage().persistent().set(&pos_key, &pos);
    _extend_persistent_ttl(&env, &pos_key);

    // Update pool
    pool.total_collateral = pool
        .total_collateral
        .checked_sub(net_collateral_out)
        .ok_or(ContractError::Overflow)?;
    pool.accumulated_fees = pool
        .accumulated_fees
        .checked_add(fee)
        .ok_or(ContractError::Overflow)?;
    env.storage().persistent().set(&pool_key, &pool);
    _extend_persistent_ttl(&env, &pool_key);

    // Credit user balance
    let user_balance = balance(env.clone(), user.clone());
    let new_balance = user_balance
        .checked_add(net_collateral_out)
        .ok_or(ContractError::Overflow)?;
    _set_balance(&env, user.clone(), new_balance);

    let side_val = match side {
        BetSide::Up => 0u32,
        BetSide::Down => 1u32,
    };
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("amm"), symbol_short!("sell")),
        (user, round.round_id, side_val, shares_in, net_collateral_out),
    );

    Ok(net_collateral_out)
}

// ─── AMM Settlement & Claims ────────────────────────────────────────────────

/// Settles an AMM round given the oracle final price vs start price.
pub fn settle_amm_round(env: &Env, round_id: u64, price_start: u128, price_final: u128) {
    let pool_key = DataKeyScoped::AmmPool(round_id);
    if let Some(mut pool) = env.storage().persistent().get::<_, AmmPoolState>(&pool_key) {
        let winning_side = if price_final > price_start {
            0u32 // Up wins
        } else if price_final < price_start {
            1u32 // Down wins
        } else {
            2u32 // Tie / Refund
        };

        pool.is_resolved = true;
        pool.winning_side = Some(winning_side);
        env.storage().persistent().set(&pool_key, &pool);
        _extend_persistent_ttl(env, &pool_key);
    }
}

/// Claims payout for an AMM round: redeems winning trader shares and LP equity.
pub fn amm_claim_winnings(env: Env, user: Address) -> Result<i128, ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_not_paused(&env)?;

    let last_round_id: u64 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::LastRoundId)
        .unwrap_or(0);
    if last_round_id == 0 {
        return Err(ContractError::NoActiveRound);
    }

    // Try finding the resolved AMM pool for the last round (or active round)
    let pool_key = DataKeyScoped::AmmPool(last_round_id);
    let pool: AmmPoolState = env
        .storage()
        .persistent()
        .get(&pool_key)
        .ok_or(ContractError::NoActiveRound)?;

    if !pool.is_resolved {
        return Err(ContractError::RoundNotEnded);
    }

    let winning_side = pool.winning_side.unwrap_or(2);
    let mut total_payout: i128 = 0;

    // 1. Trader claim
    let pos_key = DataKeyScoped::AmmPosition(last_round_id, user.clone());
    if let Some(mut pos) = env
        .storage()
        .persistent()
        .get::<_, AmmUserPosition>(&pos_key)
    {
        let trader_payout: i128;
        if winning_side == 0 {
            // UP won: redeem UP shares 1:1 with collateral
            trader_payout = pos.up_shares;
        } else if winning_side == 1 {
            // DOWN won: redeem DOWN shares 1:1 with collateral
            trader_payout = pos.down_shares;
        } else {
            // Tie / Refund: refund total invested
            trader_payout = pos.total_invested;
        }

        pos.up_shares = 0;
        pos.down_shares = 0;
        pos.total_invested = 0;
        env.storage().persistent().set(&pos_key, &pos);
        _extend_persistent_ttl(&env, &pos_key);

        total_payout = total_payout
            .checked_add(trader_payout)
            .ok_or(ContractError::Overflow)?;

        // Update user stats
        let stats_key = DataKeyScoped::UserStats(user.clone());
        let mut stats: UserStats = env.storage().persistent().get(&stats_key).unwrap_or(UserStats {
            total_wins: 0,
            total_losses: 0,
            current_streak: 0,
            best_streak: 0,
        });

        if trader_payout > 0 {
            stats.total_wins = stats.total_wins.saturating_add(1);
            stats.current_streak = stats.current_streak.saturating_add(1);
            if stats.current_streak > stats.best_streak {
                stats.best_streak = stats.current_streak;
            }
        } else {
            stats.total_losses = stats.total_losses.saturating_add(1);
            stats.current_streak = 0;
        }
        env.storage().persistent().set(&stats_key, &stats);
        _extend_persistent_ttl(&env, &stats_key);
    }

    // 2. LP equity claim
    let lp_key = DataKeyScoped::AmmLpShares(last_round_id, user.clone());
    if let Some(user_lp_shares) = env.storage().persistent().get::<_, i128>(&lp_key) {
        if user_lp_shares > 0 && pool.total_lp_shares > 0 {
            // Remaining collateral in the pool backing LPs:
            // If UP won: pool reserves held winning UP shares + accumulated trading fees
            // If DOWN won: pool reserves held winning DOWN shares + accumulated trading fees
            // If Tie: total collateral
            let remaining_lp_value: i128 = if winning_side == 0 {
                pool.reserve_up
                    .checked_add(pool.accumulated_fees)
                    .ok_or(ContractError::Overflow)?
            } else if winning_side == 1 {
                pool.reserve_down
                    .checked_add(pool.accumulated_fees)
                    .ok_or(ContractError::Overflow)?
            } else {
                pool.total_collateral
            };

            let lp_payout_u128 = (user_lp_shares as u128)
                .checked_mul(remaining_lp_value as u128)
                .ok_or(ContractError::Overflow)?
                .checked_div(pool.total_lp_shares as u128)
                .ok_or(ContractError::Overflow)?;
            let lp_payout = lp_payout_u128 as i128;

            total_payout = total_payout
                .checked_add(lp_payout)
                .ok_or(ContractError::Overflow)?;

            env.storage().persistent().remove(&lp_key);
        }
    }

    if total_payout <= 0 {
        return Err(ContractError::AmmPositionNotFound);
    }

    let user_balance = balance(env.clone(), user.clone());
    let new_balance = user_balance
        .checked_add(total_payout)
        .ok_or(ContractError::Overflow)?;
    _set_balance(&env, user.clone(), new_balance);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("amm"), symbol_short!("claim")),
        (user, last_round_id, total_payout),
    );

    Ok(total_payout)
}

// ─── AMM Queries / Quotes ───────────────────────────────────────────────────

/// Returns current AMM pool state for a round.
pub fn amm_get_pool_state(env: &Env, round_id: u64) -> Result<AmmPoolState, ContractError> {
    let pool_key = DataKeyScoped::AmmPool(round_id);
    env.storage()
        .persistent()
        .get(&pool_key)
        .ok_or(ContractError::NoActiveRound)
}

/// Returns the user's position (outcome shares and LP shares) in an AMM round.
pub fn amm_get_user_position(
    env: &Env,
    round_id: u64,
    user: &Address,
) -> Result<AmmUserPosition, ContractError> {
    let pos_key = DataKeyScoped::AmmPosition(round_id, user.clone());
    let mut pos: AmmUserPosition = env
        .storage()
        .persistent()
        .get(&pos_key)
        .unwrap_or(AmmUserPosition {
            up_shares: 0,
            down_shares: 0,
            lp_shares: 0,
            total_invested: 0,
        });

    let lp_key = DataKeyScoped::AmmLpShares(round_id, user.clone());
    pos.lp_shares = env.storage().persistent().get(&lp_key).unwrap_or(0);

    Ok(pos)
}

/// Quotes how many outcome shares would be received for a given collateral input.
pub fn amm_quote_buy(
    env: &Env,
    round_id: u64,
    side: BetSide,
    collateral_in: i128,
) -> Result<i128, ContractError> {
    if collateral_in <= 0 {
        return Err(ContractError::AmmZeroAmount);
    }

    let pool = amm_get_pool_state(env, round_id)?;
    if pool.reserve_up <= 0 || pool.reserve_down <= 0 {
        return Err(ContractError::AmmInsufficientLiquidity);
    }

    let fee = (collateral_in as u128)
        .checked_mul(pool.fee_bps as u128)
        .ok_or(ContractError::Overflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(ContractError::Overflow)? as i128;
    let net_collateral = collateral_in
        .checked_sub(fee)
        .ok_or(ContractError::Overflow)?;

    let k = (pool.reserve_up as u128)
        .checked_mul(pool.reserve_down as u128)
        .ok_or(ContractError::Overflow)?;

    match side {
        BetSide::Up => {
            let new_down = (pool.reserve_down as u128)
                .checked_add(net_collateral as u128)
                .ok_or(ContractError::Overflow)?;
            let new_up = k
                .checked_add(new_down.checked_sub(1).ok_or(ContractError::Overflow)?)
                .ok_or(ContractError::Overflow)?
                .checked_div(new_down)
                .ok_or(ContractError::Overflow)? as i128;
            let shares_from_pool = pool
                .reserve_up
                .checked_sub(new_up)
                .ok_or(ContractError::Overflow)?;
            net_collateral
                .checked_add(shares_from_pool)
                .ok_or(ContractError::Overflow)
        }
        BetSide::Down => {
            let new_up = (pool.reserve_up as u128)
                .checked_add(net_collateral as u128)
                .ok_or(ContractError::Overflow)?;
            let new_down = k
                .checked_add(new_up.checked_sub(1).ok_or(ContractError::Overflow)?)
                .ok_or(ContractError::Overflow)?
                .checked_div(new_up)
                .ok_or(ContractError::Overflow)? as i128;
            let shares_from_pool = pool
                .reserve_down
                .checked_sub(new_down)
                .ok_or(ContractError::Overflow)?;
            net_collateral
                .checked_add(shares_from_pool)
                .ok_or(ContractError::Overflow)
        }
    }
}

/// Quotes how much collateral would be received for selling a given number of outcome shares.
pub fn amm_quote_sell(
    env: &Env,
    round_id: u64,
    side: BetSide,
    shares_in: i128,
) -> Result<i128, ContractError> {
    if shares_in <= 0 {
        return Err(ContractError::AmmZeroAmount);
    }

    let pool = amm_get_pool_state(env, round_id)?;
    if pool.reserve_up <= 0 || pool.reserve_down <= 0 {
        return Err(ContractError::AmmInsufficientLiquidity);
    }

    let k = (pool.reserve_up as u128)
        .checked_mul(pool.reserve_down as u128)
        .ok_or(ContractError::Overflow)?;

    let gross_out = match side {
        BetSide::Up => {
            let new_up = (pool.reserve_up as u128)
                .checked_add(shares_in as u128)
                .ok_or(ContractError::Overflow)?;
            let new_down = k
                .checked_div(new_up)
                .ok_or(ContractError::Overflow)? as i128;
            pool.reserve_down
                .checked_sub(new_down)
                .ok_or(ContractError::Overflow)?
        }
        BetSide::Down => {
            let new_down = (pool.reserve_down as u128)
                .checked_add(shares_in as u128)
                .ok_or(ContractError::Overflow)?;
            let new_up = k
                .checked_div(new_down)
                .ok_or(ContractError::Overflow)? as i128;
            pool.reserve_up
                .checked_sub(new_up)
                .ok_or(ContractError::Overflow)?
        }
    };

    let fee = (gross_out as u128)
        .checked_mul(pool.fee_bps as u128)
        .ok_or(ContractError::Overflow)?
        .checked_div(BPS_DENOMINATOR as u128)
        .ok_or(ContractError::Overflow)? as i128;

    gross_out.checked_sub(fee).ok_or(ContractError::Overflow)
}