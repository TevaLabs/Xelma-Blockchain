// SPDX-License-Identifier: MIT
//! Per-round pending outcome key and dispute-window TTL (Issue #685).

use crate::common::TTL_BUMP_AMOUNT;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::settlement::{_pending_disputes_key, PendingDisputeSettlement};
use crate::storage::{dispute_ttl_target, pending_outcome_key};
use crate::types::{BetSide, DataKeyScoped, OraclePayload};
use soroban_sdk::{
    testutils::{storage::Persistent as _, Address as _, Ledger as _},
    Address, Env, Map,
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

fn exists(env: &Env, contract_id: &Address) -> bool {
    env.as_contract(contract_id, || {
        env.storage().persistent().has(&pending_outcome_key(1))
    })
}

#[test]
fn staged_outcome_uses_per_round_key_and_no_shared_map() {
    let env = Env::default();
    let (_client, contract_id, _a, _b, round_id) = setup(&env);
    assert_eq!(round_id, 1);
    assert!(exists(&env, &contract_id));
    env.as_contract(&contract_id, || {
        assert!(!env.storage().persistent().has(&_pending_disputes_key(&env)));
        let staged: PendingDisputeSettlement = env
            .storage()
            .persistent()
            .get(&pending_outcome_key(round_id))
            .unwrap();
        assert_eq!(staged.round.round_id, round_id);
        assert_eq!(staged.final_price, 2_000);
    });
}

#[test]
fn staged_entries_outlive_dispute_window() {
    let env = Env::default();
    let (_client, contract_id, alice, _b, round_id) = setup(&env);
    let target = dispute_ttl_target(5);
    assert!(target >= 5 + TTL_BUMP_AMOUNT);
    env.as_contract(&contract_id, || {
        let p = env.storage().persistent();
        assert!(p.get_ttl(&pending_outcome_key(round_id)) >= target);
        assert!(p.get_ttl(&DataKeyScoped::RoundParticipants(round_id)) >= target);
        assert!(p.get_ttl(&DataKeyScoped::Position(round_id, alice.clone())) >= target);
    });
}

#[test]
fn finalize_removes_per_round_key() {
    let env = Env::default();
    let (client, contract_id, _a, _b, round_id) = setup(&env);
    let deadline = env.ledger().sequence() + 5;
    env.ledger().with_mut(|l| l.sequence_number = deadline);
    client.finalize_round(&round_id);
    assert!(!exists(&env, &contract_id));
}

#[test]
fn void_removes_per_round_key() {
    let env = Env::default();
    let (client, contract_id, _a, _b, round_id) = setup(&env);
    client.void_round(&round_id);
    assert!(!exists(&env, &contract_id));
}

#[test]
fn legacy_shared_map_entry_is_still_finalizable_and_cleaned() {
    let env = Env::default();
    let (client, contract_id, _a, _b, round_id) = setup(&env);
    // Simulate a round staged before #685: move the entry into the old map.
    env.as_contract(&contract_id, || {
        let p = env.storage().persistent();
        let staged: PendingDisputeSettlement = p.get(&pending_outcome_key(round_id)).unwrap();
        p.remove(&pending_outcome_key(round_id));
        let mut m: Map<u64, PendingDisputeSettlement> = Map::new(&env);
        m.set(round_id, staged);
        p.set(&_pending_disputes_key(&env), &m);
    });
    let deadline = env.ledger().sequence() + 5;
    env.ledger().with_mut(|l| l.sequence_number = deadline);
    client.finalize_round(&round_id);
    env.as_contract(&contract_id, || {
        assert!(!env.storage().persistent().has(&_pending_disputes_key(&env)));
    });
}
