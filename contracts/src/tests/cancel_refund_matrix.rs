// SPDX-License-Identifier: MIT
//! Full-stake refund matrix for admin `cancel_round` (Issue #408).
//!
//! `cancel_round` must, for every round mode and every participation shape:
//! - refund each participant's *entire* stake (no fee is ever taken on a
//!   cancellation — the protocol fee treasury balance must be unchanged);
//! - refund unrevealed Precision commitments exactly like revealed
//!   predictions (a participant who never got to reveal doesn't forfeit
//!   their stake just because the round was cancelled out from under them —
//!   contrast with a *resolved* round, where an unrevealed commit forfeits
//!   its stake to the winners, see `settlement_math_vectors.rs`);
//! - archive the round with `RoundArchiveStatus::Cancelled`.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, RoundArchiveStatus, RoundMode};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Bytes, BytesN, Env,
};

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    // A non-zero fee is configured deliberately: if cancel_round ever
    // accidentally routed through fee-charging logic, this would surface it
    // as a treasury-balance or under-refund mismatch below.
    client.set_protocol_fee_bps(&Some(500u32)); // 5%
    (client, admin)
}

/// UpDown mode: every participant (both sides) gets their full stake back,
/// the treasury is untouched, and the round archives as `Cancelled`.
#[test]
fn test_cancel_updown_refunds_full_stake_both_sides_zero_fee() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    let alice_bal_before = client.balance(&alice);
    let bob_bal_before = client.balance(&bob);
    let treasury_before = client.get_protocol_fee_treasury();

    client.create_round(&1_0000000, &None); // UpDown
    let round_id = client.get_active_round().unwrap().round_id;
    client.place_bet(&alice, &300_0000000, &BetSide::Up);
    client.place_bet(&bob, &150_0000000, &BetSide::Down);

    client.cancel_round(&0u32);

    assert_eq!(client.get_pending_winnings(&alice), 300_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 150_0000000);
    assert_eq!(
        client.get_protocol_fee_treasury(),
        treasury_before,
        "cancel must not take a fee"
    );

    // Claiming restores the caller's exact pre-bet balance — nothing lost,
    // nothing gained.
    client.claim_winnings(&alice);
    client.claim_winnings(&bob);
    assert_eq!(client.balance(&alice), alice_bal_before);
    assert_eq!(client.balance(&bob), bob_bal_before);

    let archived = client
        .get_archived_round(&round_id)
        .expect("cancelled round must be archived");
    assert_eq!(archived.status, RoundArchiveStatus::Cancelled);
    assert_eq!(archived.mode, RoundMode::UpDown);
    assert!(client.is_round_cancelled(&round_id));
}

/// Precision mode, all revealed: every participant gets their full stake
/// back regardless of how close their prediction was.
#[test]
fn test_cancel_precision_revealed_refunds_full_stake_zero_fee() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    let treasury_before = client.get_protocol_fee_treasury();

    client.create_round(&2000, &Some(1)); // Precision
    let round_id = client.get_active_round().unwrap().round_id;
    client.place_precision_prediction(&alice, &200_0000000, &2001); // near
    client.place_precision_prediction(&bob, &300_0000000, &9999); // far — would lose if resolved

    client.cancel_round(&0u32);

    assert_eq!(client.get_pending_winnings(&alice), 200_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 300_0000000);
    assert_eq!(
        client.get_protocol_fee_treasury(),
        treasury_before,
        "cancel must not take a fee"
    );

    let archived = client
        .get_archived_round(&round_id)
        .expect("cancelled round must be archived");
    assert_eq!(archived.status, RoundArchiveStatus::Cancelled);
    assert_eq!(archived.mode, RoundMode::Precision);
}

/// Precision mode, all unrevealed commitments: an unrevealed commit still
/// gets its *full* stake back on cancellation. This is the sharpest
/// contrast with resolution, where an unrevealed commit's stake is
/// forfeited to the winners instead of refunded.
#[test]
fn test_cancel_precision_unrevealed_commitments_refunded_per_policy() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    let treasury_before = client.get_protocol_fee_treasury();

    client.create_round(&2000, &Some(1)); // Precision
    let round_id = client.get_active_round().unwrap().round_id;

    let hash_alice = BytesN::from_array(&env, &[1u8; 32]);
    let hash_bob = BytesN::from_array(&env, &[2u8; 32]);
    client.commit_prediction(&alice, &hash_alice, &120_0000000);
    client.commit_prediction(&bob, &hash_bob, &80_0000000);
    // Neither reveals before the round is cancelled.

    client.cancel_round(&0u32);

    assert_eq!(client.get_pending_winnings(&alice), 120_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 80_0000000);
    assert_eq!(
        client.get_protocol_fee_treasury(),
        treasury_before,
        "cancel must not take a fee"
    );

    let archived = client
        .get_archived_round(&round_id)
        .expect("cancelled round must be archived");
    assert_eq!(archived.status, RoundArchiveStatus::Cancelled);
}

/// Precision mode, mixed revealed + unrevealed: both refund paths inside
/// the same `cancel_round` call must independently return full stakes.
#[test]
fn test_cancel_precision_mixed_revealed_and_unrevealed_refunds_full_stake() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let alice = Address::generate(&env); // will reveal
    let bob = Address::generate(&env); // will not reveal
    let carol = Address::generate(&env); // will not even commit-reveal, uses direct path
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&carol);
    let treasury_before = client.get_protocol_fee_treasury();

    client.create_round(&2000, &Some(1)); // Precision
    let round_id = client.get_active_round().unwrap().round_id;

    let alice_price = 2000u128;
    let mut alice_salt_bytes = [0x11u8; 32];
    alice_salt_bytes[0] = 0x99; // satisfy salt_has_minimum_entropy (non-zero, non-constant)
    let alice_salt = BytesN::from_array(&env, &alice_salt_bytes);
    let mut alice_preimage = Bytes::new(&env);
    alice_preimage.append(&alice_price.to_xdr(&env));
    alice_preimage.append(&alice_salt.clone().to_xdr(&env));
    let hash_alice: BytesN<32> = env.crypto().sha256(&alice_preimage).into();
    client.commit_prediction(&alice, &hash_alice, &50_0000000);

    let hash_bob = BytesN::from_array(&env, &[4u8; 32]);
    client.commit_prediction(&bob, &hash_bob, &60_0000000);
    client.place_precision_prediction(&carol, &70_0000000, &2005);

    // Move to the reveal window and reveal only Alice.
    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });
    client.reveal_prediction(&alice, &alice_price, &alice_salt);

    client.cancel_round(&0u32);

    assert_eq!(
        client.get_pending_winnings(&alice),
        50_0000000,
        "revealed prediction refunded in full"
    );
    assert_eq!(
        client.get_pending_winnings(&bob),
        60_0000000,
        "unrevealed commitment refunded in full"
    );
    assert_eq!(
        client.get_pending_winnings(&carol),
        70_0000000,
        "direct prediction refunded in full"
    );
    assert_eq!(
        client.get_protocol_fee_treasury(),
        treasury_before,
        "cancel must not take a fee"
    );

    let total_refunded = client.get_pending_winnings(&alice)
        + client.get_pending_winnings(&bob)
        + client.get_pending_winnings(&carol);
    assert_eq!(
        total_refunded,
        50_0000000 + 60_0000000 + 70_0000000,
        "conservation: nothing lost or gained"
    );

    let archived = client
        .get_archived_round(&round_id)
        .expect("cancelled round must be archived");
    assert_eq!(archived.status, RoundArchiveStatus::Cancelled);
}
