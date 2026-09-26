// SPDX-License-Identifier: MIT
//! Commitment-based public aggregate management for betting phase confidentiality.
//!
//! # Overview & Trust Boundary
//!
//! To prevent MEV, copy-trading, and frontrunning during active betting windows,
//! public aggregates during `RoundPhase::Betting` expose commitment-based totals
//! (`RollingCommitment` hash, total commitment count, total committed stake) while
//! blinding raw side pool totals (`total_up_stake`, `total_down_stake`, side ratios).
//!
//! Upon betting close (`RoundPhase::Running`, `Settlement`, or after close), the
//! **open path** activates via settlement hooks, revealing/unblinding raw side pool
//! composition for resolution transparency.

use crate::common::{_derive_round_phase, _extend_persistent_ttl};
use crate::errors::ContractError;
use crate::types::{DataKeyCore, DataKeyScoped, RollingCommitment, Round, RoundPhase};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, Bytes, BytesN, Env};

/// Initializes a rolling commitment record for a new round.
pub fn _init_rolling_commitment(env: &Env, round_id: u64) -> RollingCommitment {
    let key = DataKeyScoped::RollingCommitment(round_id);
    let commitment = RollingCommitment {
        round_id,
        commitment_hash: BytesN::from_array(env, &[0u8; 32]),
        total_commitments: 0,
        total_stake: 0,
        is_opened: false,
    };
    env.storage().persistent().set(&key, &commitment);
    _extend_persistent_ttl(env, &key);
    commitment
}

/// Accumulates a new bet or prediction into the round's rolling commitment.
///
/// Rolling hash: `sha256(prev_hash || user || amount || payload_hash)`
pub fn _accumulate_commitment(
    env: &Env,
    round_id: u64,
    user: &Address,
    amount: i128,
    payload_hash: &BytesN<32>,
) -> Result<RollingCommitment, ContractError> {
    let key = DataKeyScoped::RollingCommitment(round_id);
    let mut commitment: RollingCommitment = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| _init_rolling_commitment(env, round_id));

    let mut preimage = Bytes::new(env);
    preimage.append(&commitment.commitment_hash.to_xdr(env));
    preimage.append(&user.to_xdr(env));
    preimage.append(&amount.to_xdr(env));
    preimage.append(&payload_hash.to_xdr(env));

    let new_hash: BytesN<32> = env.crypto().sha256(&preimage).into();

    commitment.commitment_hash = new_hash;
    commitment.total_commitments = commitment
        .total_commitments
        .checked_add(1)
        .ok_or(ContractError::Overflow)?;
    commitment.total_stake = commitment
        .total_stake
        .checked_add(amount)
        .ok_or(ContractError::Overflow)?;

    env.storage().persistent().set(&key, &commitment);
    _extend_persistent_ttl(env, &key);

    Ok(commitment)
}

/// Settlement hook: opens the commitment aggregate after betting closes.
pub fn _open_commitment_aggregate(
    env: &Env,
    round_id: u64,
) -> Result<RollingCommitment, ContractError> {
    let key = DataKeyScoped::RollingCommitment(round_id);
    if let Some(mut commitment) = env.storage().persistent().get::<_, RollingCommitment>(&key) {
        commitment.is_opened = true;
        env.storage().persistent().set(&key, &commitment);
        _extend_persistent_ttl(env, &key);
        Ok(commitment)
    } else {
        let mut commitment = _init_rolling_commitment(env, round_id);
        commitment.is_opened = true;
        env.storage().persistent().set(&key, &commitment);
        _extend_persistent_ttl(env, &key);
        Ok(commitment)
    }
}

/// Retrieves the rolling commitment record for a round.
pub fn _get_rolling_commitment(env: &Env, round_id: u64) -> Option<RollingCommitment> {
    let key = DataKeyScoped::RollingCommitment(round_id);
    let mut commitment: RollingCommitment = env.storage().persistent().get(&key)?;
    if !commitment.is_opened {
        if let Some(round) = env
            .storage()
            .persistent()
            .get::<_, Round>(&DataKeyCore::ActiveRound)
        {
            if round.round_id == round_id
                && _derive_round_phase(env.ledger().sequence(), &round) != RoundPhase::Betting
            {
                commitment.is_opened = true;
            }
        }
    }
    Some(commitment)
}

/// Checks whether the commitment aggregate for a round has been opened.
pub fn _is_aggregate_opened(env: &Env, round_id: u64) -> bool {
    _get_rolling_commitment(env, round_id)
        .map(|c| c.is_opened)
        .unwrap_or(false)
}
