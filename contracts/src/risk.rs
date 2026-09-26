// SPDX-License-Identifier: MIT
//! O(1) portfolio exposure accounting.

use crate::errors::ContractError;
use crate::types::{BetSide, DataKeyCore, DataKeyScoped};
use soroban_sdk::{contracttype, Address, Env};

/// Total outstanding exposure and correlated Up/Down buckets.
///
/// Exposure is stake locked in an active round plus pending settlement value.
/// This deliberately does not include a claimed balance, and therefore remains
/// bounded by the number of unresolved positions and pending payouts.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PortfolioExposure {
    pub total: i128,
    pub up: i128,
    pub down: i128,
}

fn key(user: &Address) -> DataKeyScoped {
    DataKeyScoped::PortfolioExposure(user.clone())
}

pub fn read(env: &Env, user: &Address) -> PortfolioExposure {
    let storage_key = key(user);
    env.storage()
        .persistent()
        .get(&storage_key)
        .unwrap_or(PortfolioExposure { total: 0, up: 0, down: 0 })
}

pub fn add_stake(
    env: &Env,
    user: Address,
    amount: i128,
    side: Option<BetSide>,
) -> Result<(), ContractError> {
    let current = read(env, &user);
    let total = current
        .total
        .checked_add(amount)
        .ok_or(ContractError::Overflow)?;
    if let Some(limit) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKeyCore::MaxUserRoundExposure)
    {
        if total > limit {
            return Err(ContractError::PortfolioExposureCapExceeded);
        }
    }
    let (up, down) = match side {
        Some(BetSide::Up) => (current.up.checked_add(amount).ok_or(ContractError::Overflow)?, current.down),
        Some(BetSide::Down) => (current.up, current.down.checked_add(amount).ok_or(ContractError::Overflow)?),
        None => (current.up, current.down),
    };
    write(env, user, PortfolioExposure { total, up, down });
    Ok(())
}

pub fn remove_stake(
    env: &Env,
    user: Address,
    amount: i128,
    side: Option<BetSide>,
) -> Result<(), ContractError> {
    let current = read(env, &user);
    // Older positions can predate the portfolio index; cleanup remains
    // compatible with those positions and simply starts tracking at zero.
    if current.total < amount {
        return Ok(());
    }
    let total = current.total.checked_sub(amount).ok_or(ContractError::Overflow)?;
    let (up, down) = match side {
        Some(BetSide::Up) => (current.up.checked_sub(amount).ok_or(ContractError::Overflow)?, current.down),
        Some(BetSide::Down) => (current.up, current.down.checked_sub(amount).ok_or(ContractError::Overflow)?),
        None => (current.up, current.down),
    };
    write(env, user, PortfolioExposure { total, up, down });
    Ok(())
}

pub fn add_pending(env: &Env, user: Address, amount: i128) -> Result<(), ContractError> {
    let current = read(env, &user);
    let total = current
        .total
        .checked_add(amount)
        .ok_or(ContractError::Overflow)?;
    write(
        env,
        user,
        PortfolioExposure {
            total,
            up: current.up,
            down: current.down,
        },
    );
    Ok(())
}

pub fn remove_pending(env: &Env, user: Address, amount: i128) -> Result<(), ContractError> {
    remove_stake(env, user, amount, None)
}

fn write(env: &Env, user: Address, exposure: PortfolioExposure) {
    let storage_key = key(&user);
    if exposure.total == 0 {
        env.storage().persistent().remove(&storage_key);
    } else {
        env.storage().persistent().set(&storage_key, &exposure);
    }
}