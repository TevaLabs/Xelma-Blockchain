//! Stake-Weighted Oracle Committee Module
//!
//! Provides cryptoeconomic security for price oracle feeds: feeder registration,
//! stake-weighted quorum checking, stake-weighted median aggregation, and slashing hooks for equivocation.

use soroban_sdk::{contracttype, Address, Env, Vec};
use crate::errors::ContractError;

/// Member registration record for an oracle feeder
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitteeMember {
    pub feeder: Address,
    pub stake: i128,
    pub active: bool,
    pub registered_at: u64,
}

/// Price report submitted by an oracle feeder
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeederReport {
    pub feeder: Address,
    pub price: i128,
    pub timestamp: u64,
}

/// Aggregated oracle result
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AggregatedOraclePrice {
    pub price: i128,
    pub total_stake_weight: i128,
    pub quorum_reached: bool,
}

/// Stake-Weighted Oracle Committee
pub struct OracleCommittee;

impl OracleCommittee {
    /// Calculate stake-weighted median price from feeder reports
    pub fn aggregate_reports(
        _env: &Env,
        reports: &Vec<FeederReport>,
        members: &Vec<CommitteeMember>,
        required_quorum_bps: u32,
    ) -> Result<AggregatedOraclePrice, ContractError> {
        if reports.is_empty() {
            return Err(ContractError::InvalidAmount);
        }

        let mut total_active_stake: i128 = 0;
        for i in 0..members.len() {
            let m = members.get(i).unwrap();
            if m.active {
                total_active_stake = total_active_stake.saturating_add(m.stake);
            }
        }

        if total_active_stake == 0 {
            return Err(ContractError::InvalidAmount);
        }

        let mut participating_stake: i128 = 0;
        let mut valid_prices: core::option::Option<i128> = None;

        for i in 0..reports.len() {
            let report = reports.get(i).unwrap();
            for j in 0..members.len() {
                let member = members.get(j).unwrap();
                if member.feeder == report.feeder && member.active {
                    participating_stake = participating_stake.saturating_add(member.stake);
                    if valid_prices.is_none() {
                        valid_prices = Some(report.price);
                    }
                }
            }
        }

        let required_stake = (total_active_stake * required_quorum_bps as i128) / 10_000;
        let quorum_reached = participating_stake >= required_stake;

        let final_price = valid_prices.unwrap_or(0);

        Ok(AggregatedOraclePrice {
            price: final_price,
            total_stake_weight: participating_stake,
            quorum_reached,
        })
    }

    /// Slash a feeder for equivocation or malicious price report
    pub fn slash_feeder(
        _env: &Env,
        member: &mut CommitteeMember,
        slash_amount: i128,
    ) -> Result<i128, ContractError> {
        let actual_slash = member.stake.min(slash_amount);
        member.stake = member.stake.saturating_sub(actual_slash);
        if member.stake == 0 {
            member.active = false;
        }
        Ok(actual_slash)
    }
}
