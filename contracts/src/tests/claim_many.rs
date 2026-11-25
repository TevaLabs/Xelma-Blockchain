// SPDX-License-Identifier: MIT
//! Tests for the bounded, all-or-nothing `claim_many` batch claim API (Issue #277).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::DataKey;
use soroban_sdk::testutils::{storage::Persistent as _, Address as _};
use soroban_sdk::{Address, Env, Vec};

/// `claim_many`'s hard cap, mirrored here so the "over cap" test stays in
/// sync with the contract without importing a private constant.
const MAX_CLAIM_BATCH_SIZE: u32 = 50;

fn setup() -> (Env, Address, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, contract_id, client)
}

/// Directly writes a user's pending winnings, bypassing the full bet/resolve
/// flow so these tests can focus purely on `claim_many`'s batch semantics.
fn set_pending(env: &Env, contract_id: &Address, user: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::PendingWinnings(user.clone()), &amount);
    });
}

fn get_pending(env: &Env, contract_id: &Address, user: &Address) -> i128 {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::PendingWinnings(user.clone()))
            .unwrap_or(0)
    })
}

#[test]
fn test_claim_many_happy_path_multiple_users() {
    let (env, contract_id, client) = setup();

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);
    set_pending(&env, &contract_id, &alice, 100);
    set_pending(&env, &contract_id, &bob, 250);
    set_pending(&env, &contract_id, &carol, 0); // no pending winnings

    let mut users: Vec<Address> = Vec::new(&env);
    users.push_back(alice.clone());
    users.push_back(bob.clone());
    users.push_back(carol.clone());

    let amounts = client.claim_many(&users);

    assert_eq!(amounts.len(), 3);
    assert_eq!(amounts.get(0).unwrap(), 100);
    assert_eq!(amounts.get(1).unwrap(), 250);
    assert_eq!(amounts.get(2).unwrap(), 0); // no-op, not an error

    assert_eq!(client.balance(&alice), 100);
    assert_eq!(client.balance(&bob), 250);
    assert_eq!(client.balance(&carol), 0);

    // Pending winnings slots are cleared for claimed users.
    assert_eq!(get_pending(&env, &contract_id, &alice), 0);
    assert_eq!(get_pending(&env, &contract_id, &bob), 0);

    // A second claim in a new batch is a safe no-op (already claimed).
    let amounts_again = client.claim_many(&users);
    assert_eq!(amounts_again.get(0).unwrap(), 0);
    assert_eq!(amounts_again.get(1).unwrap(), 0);
}

#[test]
fn test_claim_many_empty_batch_returns_empty_vec() {
    let (env, _contract_id, client) = setup();
    let users: Vec<Address> = Vec::new(&env);
    let amounts = client.claim_many(&users);
    assert_eq!(amounts.len(), 0);
}

#[test]
fn test_claim_many_rejects_batch_over_cap() {
    let (env, contract_id, client) = setup();

    let mut users: Vec<Address> = Vec::new(&env);
    for _ in 0..(MAX_CLAIM_BATCH_SIZE + 1) {
        let user = Address::generate(&env);
        set_pending(&env, &contract_id, &user, 10);
        users.push_back(user);
    }
    assert_eq!(users.len(), MAX_CLAIM_BATCH_SIZE + 1);

    let result = client.try_claim_many(&users);
    assert_eq!(result, Err(Ok(ContractError::ClaimBatchTooLarge)));

    // No user's pending winnings should have been touched.
    for i in 0..users.len() {
        let user = users.get(i).unwrap();
        assert_eq!(get_pending(&env, &contract_id, &user), 10);
    }
}

#[test]
fn test_claim_many_accepts_batch_exactly_at_cap() {
    let (env, contract_id, client) = setup();

    let mut users: Vec<Address> = Vec::new(&env);
    for _ in 0..MAX_CLAIM_BATCH_SIZE {
        let user = Address::generate(&env);
        set_pending(&env, &contract_id, &user, 1);
        users.push_back(user);
    }

    let amounts = client.claim_many(&users);
    assert_eq!(amounts.len(), MAX_CLAIM_BATCH_SIZE);
    for i in 0..amounts.len() {
        assert_eq!(amounts.get(i).unwrap(), 1);
    }
}

#[test]
fn test_claim_many_rejects_duplicate_address() {
    let (env, contract_id, client) = setup();

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    set_pending(&env, &contract_id, &alice, 100);
    set_pending(&env, &contract_id, &bob, 100);

    let mut users: Vec<Address> = Vec::new(&env);
    users.push_back(alice.clone());
    users.push_back(bob.clone());
    users.push_back(alice.clone()); // duplicate

    let result = client.try_claim_many(&users);
    assert_eq!(result, Err(Ok(ContractError::DuplicateClaimAddress)));

    // Neither user should have been paid out — the whole batch is rejected
    // before any mutation happens.
    assert_eq!(get_pending(&env, &contract_id, &alice), 100);
    assert_eq!(get_pending(&env, &contract_id, &bob), 100);
    assert_eq!(client.balance(&alice), 0);
    assert_eq!(client.balance(&bob), 0);
}

#[test]
fn test_claim_many_rejects_when_fully_paused() {
    let (env, contract_id, client) = setup();

    let alice = Address::generate(&env);
    set_pending(&env, &contract_id, &alice, 100);

    client.pause_contract();

    let mut users: Vec<Address> = Vec::new(&env);
    users.push_back(alice.clone());

    let result = client.try_claim_many(&users);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    assert_eq!(get_pending(&env, &contract_id, &alice), 100);
}
