// SPDX-License-Identifier: MIT
//! End-to-end coverage for the staged dispute-window settlement lifecycle.

use crate::common::MAX_DISPUTE_LEDGERS;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload, RoundArchiveStatus};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, IntoVal, TryIntoVal,
};

#[test]
fn set_dispute_ledgers_requires_admin_and_enforces_bounds() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let attacker = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_dispute_ledgers",
            args: (5u32,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.try_set_dispute_ledgers(&5u32).is_err());

    env.mock_all_auths();
    client.set_dispute_ledgers(&MAX_DISPUTE_LEDGERS);
    assert_eq!(client.get_dispute_ledgers(), MAX_DISPUTE_LEDGERS);
    assert_eq!(
        client.try_set_dispute_ledgers(&(MAX_DISPUTE_LEDGERS + 1)),
        Err(Ok(ContractError::WindowOutOfRange))
    );
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
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    let alice = Address::generate(env);
    let bob = Address::generate(env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.set_dispute_ledgers(&5u32);
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
    assert!(has_round_event(env, &contract_id, symbol_short!("pending")));

    (client, contract_id, alice, bob, round.round_id)
}

fn has_round_event(env: &Env, contract_id: &Address, action: soroban_sdk::Symbol) -> bool {
    env.events().all().iter().any(|(emitter, topics, _)| {
        emitter == *contract_id
            && topics.len() == 2
            && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(env) == Ok(action.clone())
    })
}

#[test]
fn void_during_window_refunds_exact_stakes_and_conserves_pot() {
    let env = Env::default();
    let (client, contract_id, alice, bob, round_id) = setup(&env);

    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);
    assert_eq!(
        client.try_finalize_round(&round_id),
        Err(Ok(ContractError::RoundNotEnded))
    );

    let treasury_before = client.get_protocol_fee_treasury();
    client.void_round(&round_id);
    // `events().all()` only holds the last invocation, so check right away.
    assert!(has_round_event(&env, &contract_id, symbol_short!("voided")));

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
    assert!(has_round_event(
        &env,
        &contract_id,
        symbol_short!("pending")
    ));
    assert!(has_round_event(&env, &contract_id, symbol_short!("voided")));
    // Replay protection: a second void_round call must be rejected.
    assert_eq!(
        client.try_void_round(&round_id),
        Err(Ok(ContractError::RoundNotCancellable))
    );
}

#[test]
fn void_during_window_refunds_precision_round_stakes() {
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
    client.create_round(&1_000u128, &Some(1u32));
    client.place_precision_prediction(&alice, &75i128, &1_050u128);
    client.place_precision_prediction(&bob, &25i128, &950u128);

    let round = client.get_active_round().unwrap();
    let round_id = round.round_id;
    env.ledger()
        .with_mut(|ledger| ledger.sequence_number = round.end_ledger);
    client.resolve_round(&OraclePayload {
        price: 1_050,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    let treasury_before = client.get_protocol_fee_treasury();
    client.void_round(&round_id);

    assert_eq!(client.get_pending_winnings(&alice), 75);
    assert_eq!(client.get_pending_winnings(&bob), 25);
    assert_eq!(client.get_protocol_fee_treasury() - treasury_before, 0);
    assert_eq!(
        client.get_archived_round(&round_id).unwrap().status,
        RoundArchiveStatus::Voided
    );
    assert!(has_round_event(&env, &contract_id, symbol_short!("voided")));
    assert_eq!(
        client.try_void_round(&round_id),
        Err(Ok(ContractError::RoundNotCancellable))
    );
}

#[test]
fn finalize_after_window_settles_and_late_void_is_blocked() {
    let env = Env::default();
    let (client, contract_id, alice, bob, round_id) = setup(&env);
    env.ledger().with_mut(|ledger| ledger.sequence_number += 5);

    assert_eq!(
        client.try_void_round(&round_id),
        Err(Ok(ContractError::RoundNotCancellable))
    );

    let treasury_before = client.get_protocol_fee_treasury();
    client.finalize_round(&round_id);
    assert!(has_round_event(
        &env,
        &contract_id,
        symbol_short!("finalized")
    ));

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
}

#[test]
fn finalize_does_not_disturb_newer_active_round() {
    let env = Env::default();
    let (client, _contract_id, alice, _bob, round_id) = setup(&env);

    // A newer round opens and takes a bet while round 1 sits in its window.
    client.create_round(&1_000u128, &None);
    let newer = client.get_active_round().unwrap();
    assert_ne!(newer.round_id, round_id);
    client.place_bet(&alice, &10i128, &BetSide::Up);
    let newer_pool_up = client.get_active_round().unwrap().pool_up;
    assert_eq!(newer_pool_up, 10);

    env.ledger().with_mut(|ledger| ledger.sequence_number += 5);
    client.finalize_round(&round_id);

    let active = client
        .get_active_round()
        .expect("newer round must survive finalize");
    assert_eq!(active.round_id, newer.round_id);
    assert_eq!(active.pool_up, 10);
    assert_eq!(client.get_user_position(&alice).map(|p| p.amount), Some(10));
}

#[test]
fn finalize_rejected_one_ledger_before_deadline_then_allowed_at_deadline() {
    let env = Env::default();
    let (client, _contract_id, alice, _bob, round_id) = setup(&env);

    env.ledger().with_mut(|ledger| ledger.sequence_number += 4);
    assert_eq!(
        client.try_finalize_round(&round_id),
        Err(Ok(ContractError::RoundNotEnded))
    );
    assert_eq!(client.get_pending_winnings(&alice), 0);

    env.ledger().with_mut(|ledger| ledger.sequence_number += 1);
    client.finalize_round(&round_id);
    assert_eq!(client.get_pending_winnings(&alice), 100);
}

#[test]
fn finalize_twice_or_unknown_round_fails_without_double_payout() {
    let env = Env::default();
    let (client, _contract_id, alice, _bob, round_id) = setup(&env);
    env.ledger().with_mut(|ledger| ledger.sequence_number += 5);
    client.finalize_round(&round_id);

    assert_eq!(
        client.try_finalize_round(&round_id),
        Err(Ok(ContractError::NoActiveRound))
    );
    assert_eq!(
        client.try_finalize_round(&(round_id + 99)),
        Err(Ok(ContractError::NoActiveRound))
    );
    assert_eq!(client.get_pending_winnings(&alice), 100);
}
