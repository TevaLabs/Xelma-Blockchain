// SPDX-License-Identifier: MIT
//! Regression tests for Issue #405: precision / indexed payout arithmetic
//! overflow must return `PayoutOverflow` (25), never a generic `Overflow`
//! (11), so clients can unambiguously detect a payout failure.
//!
//! Two layers cover the fix:
//!   1. Pure unit tests on the `settlement_math` precision helpers
//!      (`compute_precision_fee`, `split_pot_stake_weighted`).
//!   2. A contract-level integration test that drives the actual *indexed*
//!      Precision settlement path (`_resolve_precision_mode`) into a pot
//!      overflow via crafted near-maximum participant stakes without ever
//!      emitting a partial payout.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::settlement_math::{compute_precision_fee, split_pot_stake_weighted};
use crate::types::{DataKeyCore, DataKeyScoped, OraclePayload};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

// ─── Pure settlement_math helper units ───────────────────────────────────────

/// `total_pot * fee_bps` overflows i128 → must be `PayoutOverflow`.
#[test]
fn test_compute_precision_fee_mul_overflow_returns_payout_overflow() {
    // i128::MAX * 65_535 overflows i128.
    assert_eq!(
        compute_precision_fee(i128::MAX, Some(u32::MAX)),
        Err(ContractError::PayoutOverflow)
    );
}

/// Summing winner stakes overflows i128 → must be `PayoutOverflow`.
#[test]
fn test_split_pot_stake_weighted_stake_sum_overflow_returns_payout_overflow() {
    let stakes = [i128::MAX, i128::MAX];
    assert_eq!(
        split_pot_stake_weighted(1, &stakes),
        Err(ContractError::PayoutOverflow)
    );
}

/// Intermediate `stake * distributable` overflows i128 even though the final
/// quotient would fit → must be `PayoutOverflow`.
#[test]
fn test_split_pot_stake_weighted_payout_mul_overflow_returns_payout_overflow() {
    let stakes = [i128::MAX - 1];
    assert_eq!(
        split_pot_stake_weighted(i128::MAX, &stakes),
        Err(ContractError::PayoutOverflow)
    );
}

// ─── Indexed Precision settlement path (contract-level) ─────────────────────

fn setup() -> (Env, Address, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    (env, contract_id, client)
}

/// Two participants stake just under `i128::MAX` each. During indexed
/// Precision settlement `_resolve_precision_mode` accumulates
/// `total_pot = stake_a + stake_b`, which overflows i128. This must surface
/// as `PayoutOverflow` — not a generic `Overflow`, and never a panic — and
/// settle atomically (no partial pending payouts, active round preserved).
#[test]
fn test_resolve_precision_indexed_total_pot_overflow_returns_payout_overflow() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    // Grant both users enormous balances so they can stake near `i128::MAX`.
    env.as_contract(&contract_id, || {
        let bal_a = DataKeyScoped::Balance(alice.clone());
        env.storage().persistent().set(&bal_a, &i128::MAX);
        let bal_b = DataKeyScoped::Balance(bob.clone());
        env.storage().persistent().set(&bal_b, &i128::MAX);
    });

    // Indexed Precision round.
    client.create_round(&2000u128, &Some(1));

    // Both stake `i128::MAX - 2`; the summed indexed pot overflows i128.
    client.place_precision_prediction(&alice, &(i128::MAX - 2), &2297);
    client.place_precision_prediction(&bob, &(i128::MAX - 2), &2297);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let round = client.get_active_round().unwrap();
    let result = client.try_resolve_round(&OraclePayload {
        price: 2298,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert_eq!(result, Err(Ok(ContractError::PayoutOverflow)));

    // All-or-nothing settlement: no partial payouts were recorded, and the
    // active round is untouched because settlement aborted before any writes.
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);
    assert!(client.get_active_round().is_some());
}

/// Regression for Issue #556: the *indexed* Precision fee calculation
/// (`config::calculate_protocol_fee_precision`, reached through
/// `_resolve_precision_mode` → `_apply_protocol_fee_precision`) previously
/// returned the generic `Overflow` (11) when `taxable_base * fee_bps` overflowed
/// i128, instead of the payout-specific `PayoutOverflow` (25). A wallet that
/// keys off the error code to decide whether to retry could not distinguish a
/// settlement-payout failure from unrelated arithmetic overflow.
///
/// This drives the real indexed settlement path with a pot large enough that
/// `total_pot * fee_bps` overflows i128, and asserts the specific
/// `PayoutOverflow` code — not `Overflow` — plus the all-or-nothing guarantee.
#[test]
fn test_resolve_precision_indexed_fee_mul_overflow_returns_payout_overflow() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    // Enable a 10% protocol fee so `calculate_protocol_fee_precision` actually
    // multiplies the pot by `fee_bps` (with no fee, the mul is skipped and the
    // overflow site is never reached).
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::ProtocolFeeBps, &1000u32);
    });

    // Grant both users balances large enough to stake near i128::MAX.
    env.as_contract(&contract_id, || {
        let bal_a = DataKeyScoped::Balance(alice.clone());
        env.storage().persistent().set(&bal_a, &i128::MAX);
        let bal_b = DataKeyScoped::Balance(bob.clone());
        env.storage().persistent().set(&bal_b, &i128::MAX);
    });

    // Indexed Precision round. Both stake `i128::MAX / 100` and tie on price so
    // both are winners. The summed pot (`i128::MAX / 50`) fits in i128, so the
    // payout *accumulation* does NOT overflow (that path already returns
    // PayoutOverflow and is covered by the test above). But
    // `total_pot * fee_bps` (bps capped at MAX_PROTOCOL_FEE_BPS = 1000) does
    // overflow i128 — exactly the site fixed in Issue #556.
    client.create_round(&2000u128, &Some(1));
    client.place_precision_prediction(&alice, &(i128::MAX / 100), &2297);
    client.place_precision_prediction(&bob, &(i128::MAX / 100), &2297);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let round = client.get_active_round().unwrap();
    let result = client.try_resolve_round(&OraclePayload {
        price: 2298,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert_eq!(result, Err(Ok(ContractError::PayoutOverflow)));

    // All-or-nothing settlement: no partial payouts were recorded, and the
    // active round is untouched because settlement aborted before any writes.
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);
    assert!(client.get_active_round().is_some());
}
