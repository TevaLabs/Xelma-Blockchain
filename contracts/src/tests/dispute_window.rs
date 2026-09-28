// SPDX-License-Identifier: MIT
//! End-to-end coverage for the staged dispute-window settlement lifecycle.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload, RoundArchiveStatus};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, TryIntoVal,
};

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
    assert!(has_round_event(
        &env,
        &contract_id,
        symbol_short!("finalized")
    ));
}
