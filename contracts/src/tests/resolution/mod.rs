// SPDX-License-Identifier: MIT
//! Tests for round resolution and winnings distribution.
//!
//! Split into focused scenario packs for reviewability (Issue #416):
//! - `updown` – basic up/down round resolution
//! - `precision` – precision mode resolution, determinism & conservation
//! - `min_participants` – minimum participants threshold guard
//! - `fees` – protocol fee collection, conservation & withdrawal
//! - `archive` – archived rounds & user participation history
//! - `events` – payout/outcome event emission & loss event semantics
//! - `golden` – pure settlement_math golden-vector tests
//! - `policy` – precision payout policy (equal vs. stake-weighted)

#![allow(clippy::inconsistent_digit_grouping)]

mod archive;
mod events;
mod fees;
mod golden;
mod min_participants;
mod policy;
mod precision;
mod updown;

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::settlement_math::{
    classify_price_direction, compute_deviation_bps, compute_precision_fee,
    compute_precision_payouts, compute_updown_fee, compute_updown_payouts,
    find_precision_winners, is_one_sided_pool, split_pot_among_winners,
    total_pot_updown, PrecisionEntry, PrecisionPayoutEntry, PriceDirection, UpDownPosition,
    UpDownPayoutEntry,
};
use crate::types::{
    BetSide, DataKeyCore, DataKeyScoped, OraclePayload, PrecisionPrediction, Round, RoundArchiveStatus, RoundMode,
    UserOutcomeType, UserPosition,
};
use soroban_sdk::BytesN;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, Map, TryIntoVal, Vec,
};
use std::string::{String, ToString};

/// Salt satisfying on-chain minimum entropy (non-zero, non-constant).
pub(super) fn test_salt(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        bytes[i] = seed.wrapping_add(i as u8).wrapping_mul(17).wrapping_add(3);
        i += 1;
    }
    bytes[0] = seed | 0x80;
    bytes[31] = seed ^ 0x5A;
    BytesN::from_array(env, &bytes)
}

pub(super) fn payout_outcome_events(env: &Env) -> std::vec::Vec<(u64, u32, Address, i128, u32)> {
    env.events()
        .all()
        .iter()
        .filter_map(|event| {
            let (_contract, topics, data) = event;
            if topics.len() == 2
                && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("payout"))
                && topics.get(1).unwrap().try_into_val(env) == Ok(symbol_short!("outcome"))
            {
                Some(data.clone().try_into_val(env).unwrap())
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn resolve_active_round(
    client: &VirtualTokenContractClient,
    env: &Env,
    final_price: u128,
    nonce: u64,
) -> u64 {
    let round = client.get_active_round().unwrap();
    let round_id = round.round_id;
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });
    client.resolve_round(&OraclePayload {
        price: final_price,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,    });
    round_id
}

/// Helper: counts `("outcome", "loss")` events currently emitted on the env.
pub(super) fn count_outcome_loss_events(env: &Env) -> u32 {
    env.events()
        .all()
        .iter()
        .filter(|e| {
            let (_contract, topics, _data) = e;
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("outcome"))
                && topics.get(1).unwrap().try_into_val(env) == Ok(symbol_short!("loss"))
        })
        .count() as u32
}

/// Helper: collects every decoded loss event payload for assertions.
pub(super) fn collect_outcome_loss_events(
    env: &Env,
) -> std::vec::Vec<(soroban_sdk::Address, u64, u32, i128, u32, u128)> {
    env.events()
        .all()
        .iter()
        .filter_map(|e| {
            let (_contract, topics, data) = e;
            if topics.len() != 2
                || topics.get(0).unwrap().try_into_val(env) != Ok(symbol_short!("outcome"))
                || topics.get(1).unwrap().try_into_val(env) != Ok(symbol_short!("loss"))
            {
                return None;
            }
            let res: Result<(soroban_sdk::Address, u64, u32, i128, u32, u128), _> =
                data.try_into_val(env);
            res.ok()
        })
        .collect()
}

pub(super) fn collect_protocol_fee_events(env: &Env) -> std::vec::Vec<(u64, i128, i128, u32)> {
    env.events()
        .all()
        .iter()
        .filter_map(|e| {
            let (_contract, topics, data) = e;
            if topics.len() != 2
                || topics.get(0).unwrap().try_into_val(env) != Ok(symbol_short!("protocol"))
                || topics.get(1).unwrap().try_into_val(env) != Ok(symbol_short!("fee_coll"))
            {
                return None;
            }
            let res: Result<(u64, i128, i128, u32), _> = data.try_into_val(env);
            res.ok()
        })
        .collect()
}

pub(super) fn count_protocol_fee_events(env: &Env) -> u32 {
    env.events()
        .all()
        .iter()
        .filter(|e| {
            let (_contract, topics, _data) = e;
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("protocol"))
                && topics.get(1).unwrap().try_into_val(env) == Ok(symbol_short!("fee_coll"))
        })
        .count() as u32
}

pub(super) fn sum_pending_payouts(
    env: &Env,
    contract: &soroban_sdk::Address,
    users: &[soroban_sdk::Address],
) -> i128 {
    let mut total: i128 = 0;
    env.as_contract(contract, || {
        for u in users {
            let key = crate::types::DataKeyScoped::PendingWinnings(u.clone());
            let v: Option<i128> = env.storage().persistent().get(&key);
            total = total
                .checked_add(v.unwrap_or(0))
                .expect("overflow summing pending payouts");
        }
    });
    total
}
