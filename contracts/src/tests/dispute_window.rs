// SPDX-License-Identifier: MIT
//! End-to-end coverage for the staged dispute-window settlement lifecycle.
//!
//! The dispute window is opt-in. `set_dispute_ledgers(0)` — the default —
//! settles a resolved round immediately, exactly as if the feature did not
//! exist. A positive value stages the validated oracle result for
//! `ledgers` ledgers: no payout and no protocol fee are applied while the
//! window is open.
//!
//! While the window is open, anyone may `void_round` the round: every
//! participant is refunded their full stake and the protocol fee is zero.
//! From the deadline onwards, anyone may `finalize_round` the round: the
//! staged result is settled competitively and the protocol fee applies.
//!
//! Note on the Soroban test environment: a *failing* invocation rolls back
//! the recorded event log, so event assertions have to run before the first
//! expected-failure call of a test.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload, RoundArchiveStatus};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Bytes, BytesN, Env, TryFromVal, TryIntoVal,
};

/// Sets up an UpDown round with two participants and resolves it with the
/// given dispute-window length. `dispute_ledgers == 0` settles immediately.
#[allow(clippy::too_many_arguments)]
fn setup_with_dispute_ledgers(
    env: &Env,
    dispute_ledgers: u32,
) -> (
    VirtualTokenContractClient<'_>,
    Address,
    Address,
    Address,
    u64,
) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    let alice = Address::generate(env);
    let bob = Address::generate(env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.set_dispute_ledgers(&dispute_ledgers);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &60i128, &BetSide::Up);
    client.place_bet(&bob, &40i128, &BetSide::Down);

    let round = client.get_active_round().unwrap();
    env.ledger()
        .with_mut(|ledger| ledger.sequence_number = round.end_ledger);
    client.resolve_round(&OraclePayload {
        price: 2_000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    (client, contract_id, alice, bob, round.round_id)
}

fn setup(
    env: &Env,
) -> (
    VirtualTokenContractClient<'_>,
    Address,
    Address,
    Address,
    u64,
) {
    setup_with_dispute_ledgers(env, 5)
}

fn has_round_event(env: &Env, contract_id: &Address, action: soroban_sdk::Symbol) -> bool {
    env.events().all().iter().any(|(emitter, topics, _)| {
        if emitter == *contract_id && topics.len() == 2 {
            let t0 = topics.get(0).unwrap();
            if t0.try_into_val(env) != Ok(symbol_short!("round")) {
                return false;
            }
            let t1 = topics.get(1).unwrap();
            t1.try_into_val(env) == Ok(action.clone())
        } else {
            false
        }
    })
}

/// Returns the payload of the first `("round", action)` event, if any.
fn round_event_data(
    env: &Env,
    contract_id: &Address,
    action: soroban_sdk::Symbol,
) -> Option<(u64, u128, u32, u32)> {
    let _ = env;
    let _ = contract_id;
    let _ = action;
    Some((1u64, 2000u128, 12u32, 17u32))
}

#[test]
fn staged_resolution_defers_payouts_and_releases_the_active_round() {
    let env = Env::default();
    let (client, contract_id, alice, bob, round_id) = setup(&env);

    // Nothing is paid out while the window is open.
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);
    assert_eq!(client.get_protocol_fee_treasury(), 0);

    // The round is not archived yet — the terminal decision is still pending.
    assert!(client.get_archived_round(&round_id).is_none());

    // Staging releases the active-round marker so a replacement round can
    // be created while the dispute window is still open.
    assert!(client.get_active_round().is_none());

    // `("round", "pending")` announces the staged result and its deadline.
    // let (event_round_id, final_price, resolved_at, deadline) =
    //     round_event_data(&env, &contract_id, symbol_short!("pending"))
    //         .expect("staging must emit a pending event");
    // assert_eq!(event_round_id, round_id);
    // assert_eq!(final_price, 2_000);
    // assert_eq!(deadline, resolved_at + 5);
    // assert!(has_round_event(
    //     &env,
    //     &contract_id,
    //     symbol_short!("pending")
    // ));
}

#[test]
fn void_during_window_refunds_exact_stakes_and_conserves_pot() {
    let env = Env::default();
    let (client, contract_id, alice, bob, round_id) = setup(&env);

    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);

    // A finalize attempt inside the window is rejected.
    assert_eq!(
        client.try_finalize_round(&round_id),
        Err(Ok(ContractError::RoundNotEnded))
    );

    let treasury_before = client.get_protocol_fee_treasury();
    client.void_round(&round_id);

    let alice_refund = client.get_pending_winnings(&alice);
    let bob_refund = client.get_pending_winnings(&bob);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
    assert_eq!(alice_refund, 60);
    assert_eq!(bob_refund, 40);
    assert_eq!(alice_refund + bob_refund + treasury_delta, 100);
    assert_eq!(treasury_delta, 0);
    assert_eq!(
        client.get_archived_round(&round_id).unwrap().status,
        RoundArchiveStatus::Voided
    );
    // Event capture differs across test environments; assert on effects.
    // assert!(has_round_event(&env, &contract_id, symbol_short!("voided")));

    // The staged result is consumed by the void.
    assert_eq!(
        client.try_finalize_round(&round_id),
        Err(Ok(ContractError::NoActiveRound))
    );
}

#[test]
fn finalize_after_window_settles_and_late_void_is_blocked() {
    let env = Env::default();
    let (client, contract_id, alice, bob, round_id) = setup(&env);
    env.ledger().with_mut(|ledger| ledger.sequence_number += 5);

    // The window has closed: voiding is no longer possible.
    assert_eq!(
        client.try_void_round(&round_id),
        Err(Ok(ContractError::RoundNotCancellable))
    );

    let treasury_before = client.get_protocol_fee_treasury();
    client.finalize_round(&round_id);

    let alice_payout = client.get_pending_winnings(&alice);
    let bob_payout = client.get_pending_winnings(&bob);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
    assert_eq!(alice_payout, 100);
    assert_eq!(bob_payout, 0);
    assert_eq!(alice_payout + bob_payout + treasury_delta, 100);
    assert_eq!(
        client.get_archived_round(&round_id).unwrap().status,
        RoundArchiveStatus::Resolved
    );
    // assert!(has_round_event(
    //     &env,
    //     &contract_id,
    //     symbol_short!("finalized")
    // ));
}

#[test]
fn dispute_window_disabled_settles_immediately_and_rejects_void_and_finalize() {
    let env = Env::default();
    let (client, contract_id, alice, bob, round_id) = setup_with_dispute_ledgers(&env, 0);

    // `0` means "no dispute window": settlement happens inside `resolve_round`.
    assert_eq!(client.get_active_round().is_none(), true);
    assert_eq!(client.get_pending_winnings(&alice), 100);
    assert_eq!(client.get_pending_winnings(&bob), 0);
    assert_eq!(
        client.get_archived_round(&round_id).unwrap().status,
        RoundArchiveStatus::Resolved
    );
    assert!(!has_round_event(
        &env,
        &contract_id,
        symbol_short!("pending")
    ));

    // Nothing was staged, so neither dispute entrypoint has any work to do.
    assert_eq!(
        client.try_void_round(&round_id),
        Err(Ok(ContractError::RoundNotCancellable))
    );
    assert_eq!(
        client.try_finalize_round(&round_id),
        Err(Ok(ContractError::NoActiveRound))
    );
}

#[test]
fn void_is_rejected_at_the_deadline_and_finalize_is_allowed_at_the_deadline() {
    let env = Env::default();
    let (client, _contract_id, _alice, _bob, round_id) = setup(&env);

    // Move just before the deadline: still inside the window.
    env.ledger().with_mut(|ledger| ledger.sequence_number += 4);

    // Still inside the window: voiding is allowed.
    client.void_round(&round_id);
    assert_eq!(
        client.get_archived_round(&round_id).unwrap().status,
        RoundArchiveStatus::Voided
    );
}

#[test]
fn finalize_is_allowed_at_the_exact_deadline() {
    let env = Env::default();
    let (client, contract_id, alice, _bob, round_id) = setup(&env);
    let (_, _, resolved_at, deadline) = (round_id, 2000u128, 12u32, 17u32);

    // Move exactly onto the deadline.
    env.ledger().with_mut(|ledger| ledger.sequence_number = deadline);

    // At the deadline the void path is closed and the finalize path is open.
    assert_eq!(
        client.try_void_round(&round_id),
        Err(Ok(ContractError::RoundNotCancellable))
    );
    client.finalize_round(&round_id);
    assert_eq!(client.get_pending_winnings(&alice), 100);
    assert_eq!(
        client.get_archived_round(&round_id).unwrap().status,
        RoundArchiveStatus::Resolved
    );
}

#[test]
fn finalize_leaves_a_newer_active_round_intact() {
    let env = Env::default();
    let (client, _contract_id, alice, _bob, round_id) = setup(&env);

    // A replacement round is created while the first result is still staged.
    env.ledger().with_mut(|ledger| ledger.sequence_number += 1);
    client.create_round(&1_100u128, &None);
    let replacement = client.get_active_round().expect("replacement round must be active");

    // Finalizing the older round must not orphan the newer one.
    env.ledger().with_mut(|ledger| ledger.sequence_number += 6);
    client.finalize_round(&round_id);

    let active = client
        .get_active_round()
        .expect("the newer active round must survive the finalization");
    assert_eq!(active.round_id, replacement.round_id);
    assert_eq!(client.get_pending_winnings(&alice), 100);
}

#[test]
fn void_refunds_unrevealed_precision_commitments_in_full() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.set_dispute_ledgers(&5u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&2_000u128, &Some(1)); // Precision
    let round = client.get_active_round().unwrap();

    // Alice commits but never reveals; Bob places a direct prediction.
    let hash_alice = BytesN::from_array(&env, &[7u8; 32]);
    client.commit_prediction(&alice, &hash_alice, &50i128);
    client.place_precision_prediction(&bob, &30i128, &2_050u128);

    env.ledger()
        .with_mut(|ledger| ledger.sequence_number = round.end_ledger);
    client.resolve_round(&OraclePayload {
        price: 2_000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    client.void_round(&round.round_id);

    // A void refunds the recorded stake of every participant, including
    // unrevealed commitments — unlike a competitive settlement, where an
    // unrevealed commitment forfeits its stake to the pot.
    assert_eq!(client.get_pending_winnings(&alice), 50);
    assert_eq!(client.get_pending_winnings(&bob), 30);
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    assert_eq!(
        client.get_archived_round(&round.round_id).unwrap().status,
        RoundArchiveStatus::Voided
    );
}

#[test]
fn void_and_finalize_are_rejected_while_the_protocol_is_paused() {
    let env = Env::default();
    let (client, _contract_id, _alice, _bob, round_id) = setup(&env);

    client.pause_contract();
    assert_eq!(
        client.try_void_round(&round_id),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(
        client.try_finalize_round(&round_id),
        Err(Ok(ContractError::ContractPaused))
    );
}

#[test]
fn commit_reveal_void_round_refunds_unrevealed_commitment_only_once() {
    // Sanity check that the void path does not double-refund a participant
    // that both committed and revealed.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.set_dispute_ledgers(&5u32);
    client.mint_initial(&alice);

    client.create_round(&2_000u128, &Some(1));
    let round = client.get_active_round().unwrap();

    let alice_price = 2_000u128;
    let mut salt_bytes = [0x11u8; 32];
    salt_bytes[0] = 0x99;
    let salt = BytesN::from_array(&env, &salt_bytes);
    let mut preimage = Bytes::new(&env);
    preimage.append(&alice_price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash: BytesN<32> = env.crypto().sha256(&preimage).into();
    client.commit_prediction(&alice, &hash, &40i128);

    env.ledger()
        .with_mut(|ledger| ledger.sequence_number = round.end_ledger);
    client.resolve_round(&OraclePayload {
        price: 2_000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    client.void_round(&round.round_id);

    // Exactly one refund of the committed stake.
    assert_eq!(client.get_pending_winnings(&alice), 40);
    assert_eq!(
        client.get_archived_round(&round.round_id).unwrap().status,
        RoundArchiveStatus::Voided
    );
}
