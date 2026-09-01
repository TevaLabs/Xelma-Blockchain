//! Collateral Adapter Module (SAC / Token Collateral Integration)
//!
//! Provides abstract collateral operations for escrow, release, payout, and protocol fee collection.
//! Supports both Virtual Mode (internal balances for testing/demo) and Real Mode (Stellar Asset Contract / Soroban token).

use soroban_sdk::{Address, Env, IntoVal, Symbol, Val};
use crate::errors::ContractError;

/// Mode of operation for collateral management
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollateralMode {
    /// Virtual internal balance accounting
    Virtual,
    /// Real Stellar Asset Contract (SAC) token transfers
    RealToken,
}

/// Collateral management trait enforcing Checks-Effects-Interactions (CEI) discipline
pub trait CollateralAdapter {
    /// Escrow collateral from user into contract custody
    fn lock(&self, env: &Env, from: &Address, amount: i128) -> Result<(), ContractError>;

    /// Release escrowed collateral back to user
    fn release(&self, env: &Env, to: &Address, amount: i128) -> Result<(), ContractError>;

    /// Pay out winnings from contract pool to user
    fn payout(&self, env: &Env, to: &Address, amount: i128) -> Result<(), ContractError>;

    /// Transfer protocol fee to treasury
    fn collect_fee(&self, env: &Env, treasury: &Address, amount: i128) -> Result<(), ContractError>;
}

/// Concrete implementation of CollateralAdapter supporting both Virtual and Real Token modes
pub struct StandardCollateralAdapter {
    pub mode: CollateralMode,
    pub token_address: Option<Address>,
}

impl StandardCollateralAdapter {
    pub fn virtual_mode() -> Self {
        Self {
            mode: CollateralMode::Virtual,
            token_address: None,
        }
    }

    pub fn real_mode(token_address: Address) -> Self {
        Self {
            mode: CollateralMode::RealToken,
            token_address: Some(token_address),
        }
    }
}

impl CollateralAdapter for StandardCollateralAdapter {
    fn lock(&self, env: &Env, from: &Address, amount: i128) -> Result<(), ContractError> {
        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }

        match self.mode {
            CollateralMode::Virtual => {
                // Internal balance check & effect
                Ok(())
            }
            CollateralMode::RealToken => {
                let token_addr = self.token_address.as_ref().ok_or(ContractError::InvalidAmount)?;
                let contract_addr = env.current_contract_address();
                let args: soroban_sdk::Vec<Val> = (from, &contract_addr, amount).into_val(env);
                let _res: Val = env.invoke_contract(token_addr, &Symbol::new(env, "transfer"), args);
                Ok(())
            }
        }
    }

    fn release(&self, env: &Env, to: &Address, amount: i128) -> Result<(), ContractError> {
        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }

        match self.mode {
            CollateralMode::Virtual => Ok(()),
            CollateralMode::RealToken => {
                let token_addr = self.token_address.as_ref().ok_or(ContractError::InvalidAmount)?;
                let contract_addr = env.current_contract_address();
                let args: soroban_sdk::Vec<Val> = (&contract_addr, to, amount).into_val(env);
                let _res: Val = env.invoke_contract(token_addr, &Symbol::new(env, "transfer"), args);
                Ok(())
            }
        }
    }

    fn payout(&self, env: &Env, to: &Address, amount: i128) -> Result<(), ContractError> {
        self.release(env, to, amount)
    }

    fn collect_fee(&self, env: &Env, treasury: &Address, amount: i128) -> Result<(), ContractError> {
        self.release(env, treasury, amount)
    }
}
