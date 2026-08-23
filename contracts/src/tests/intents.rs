// SPDX-License-Identifier: MIT
//! Security and happy-path unit tests for authorization-zone intents (Issue #370).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, KeeperIntentStatus, KeeperScope, OraclePayload};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

fn setup_env<'a>() -> (
    Env,
    Address,
    VirtualTokenContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);
    let keeper = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    (env, contract_id, client, admin, oracle, user, keeper)
}

#[test]
fn test_authorize_and_get_intent() {
    let (env, _cid, client, _admin, _oracle, user, keeper) = setup_env();

    let nonce = client.authorize_keeper_intent(&user, &keeper, &KeeperScope::Claim, &100);
    assert_eq!(nonce, 0);

    let intent = client
        .get_keeper_intent(&user, &KeeperScope::Claim, &nonce)
        .expect("Intent should exist");

    assert_eq!(intent.user, user);
    assert_eq!(intent.keeper, keeper);
    assert_eq!(intent.scope, KeeperScope::Claim);
    assert_eq!(intent.nonce, 0);
    assert_eq!(intent.expires_at_ledger, 100);
    assert_eq!(intent.status, KeeperIntentStatus::Active);

    // Monotonic nonces per (user, scope)
    let nonce2 = client.authorize_keeper_intent(&user, &keeper, &KeeperScope::Claim, &100);
    assert_eq!(nonce2, 1);
}

#[test]
fn test_invalid_expiry_rejected() {
    let (_env, _cid, client, _admin, _oracle, user, keeper) = setup_env();

    // Expiry too short (< 6 ledgers)
    let res = client.try_authorize_keeper_intent(&user, &keeper, &KeeperScope::Resolve, &2);
    assert_eq!(res, Err(Ok(ContractError::InvalidIntentExpiry)));

    // Expiry zero
    let res = client.try_authorize_keeper_intent(&user, &keeper, &KeeperScope::Resolve, &0);
    assert_eq!(res, Err(Ok(ContractError::InvalidIntentExpiry)));
}

#[test]
fn test_keeper_claim_happy_path_and_custody_preservation() {
    let (env, _cid, client, _admin, oracle, user, keeper) = setup_env();

    // Prepare pending winnings for user
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // End betting and resolve
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let payload = OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    };
    client.resolve_round(&payload);

    // User should have pending winnings
    let pending = client.get_pending_winnings(&user);
    assert!(pending > 0);

    // User authorizes keeper to claim
    let nonce = client.authorize_keeper_intent(&user, &keeper, &KeeperScope::Claim, &100);

    // Keeper executes claim
    client.execute_keeper_claim(&keeper, &user, &nonce);

    // Winnings claimed for user
    let pending_after = client.get_pending_winnings(&user);
    assert_eq!(pending_after, 0);

    // Check user balance got credited (custody preserved)
    let user_balance = client.balance(&user);
    assert!(user_balance > 0);

    // Intent is now consumed
    let intent_after = client
        .get_keeper_intent(&user, &KeeperScope::Claim, &nonce)
        .unwrap();
    assert_eq!(intent_after.status, KeeperIntentStatus::Consumed);
}

#[test]
fn test_replayed_intent_rejected() {
    let (env, _cid, client, _admin, oracle, user, keeper) = setup_env();

    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let payload = OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    };
    client.resolve_round(&payload);

    let nonce = client.authorize_keeper_intent(&user, &keeper, &KeeperScope::Claim, &100);
    client.execute_keeper_claim(&keeper, &user, &nonce);

    // Attempting to execute the same intent again fails with IntentAlreadyConsumed
    let res = client.try_execute_keeper_claim(&keeper, &user, &nonce);
    assert_eq!(res, Err(Ok(ContractError::IntentAlreadyConsumed)));
}

#[test]
fn test_expired_intent_rejected() {
    let (env, _cid, client, _admin, _oracle, user, keeper) = setup_env();

    let nonce = client.authorize_keeper_intent(&user, &keeper, &KeeperScope::Claim, &20);

    // Advance ledger sequence past expiry (current 0 + 20 = 20 max allowed ledger)
    env.ledger().with_mut(|li| {
        li.sequence_number = 25;
    });

    let res = client.try_execute_keeper_claim(&keeper, &user, &nonce);
    assert_eq!(res, Err(Ok(ContractError::IntentExpired)));
}

#[test]
fn test_scope_cannot_escalate() {
    let (_env, _cid, client, _admin, _oracle, user, keeper) = setup_env();

    // User grants Claim scope
    let nonce = client.authorize_keeper_intent(&user, &keeper, &KeeperScope::Claim, &100);

    // Keeper tries to use this nonce for execute_keeper_create_next (privilege escalation attempt)
    let res = client.try_execute_keeper_create_next(&keeper, &user, &nonce);
    assert_eq!(res, Err(Ok(ContractError::IntentNotFound)));
}

#[test]
fn test_keeper_mismatch_rejected() {
    let (_env, _cid, client, _admin, _oracle, user, keeper) = setup_env();

    let attacker_keeper = Address::generate(&client.env);

    let nonce = client.authorize_keeper_intent(&user, &keeper, &KeeperScope::Claim, &100);

    // Other keeper tries to execute the intent
    let res = client.try_execute_keeper_claim(&attacker_keeper, &user, &nonce);
    assert_eq!(res, Err(Ok(ContractError::IntentKeeperMismatch)));
}

#[test]
fn test_revoked_intent_rejected() {
    let (_env, _cid, client, _admin, _oracle, user, keeper) = setup_env();

    let nonce = client.authorize_keeper_intent(&user, &keeper, &KeeperScope::Claim, &100);

    // User revokes intent
    client.revoke_keeper_intent(&user, &KeeperScope::Claim, &nonce);

    let intent = client
        .get_keeper_intent(&user, &KeeperScope::Claim, &nonce)
        .unwrap();
    assert_eq!(intent.status, KeeperIntentStatus::Revoked);

    // Keeper attempts to execute revoked intent
    let res = client.try_execute_keeper_claim(&keeper, &user, &nonce);
    assert_eq!(res, Err(Ok(ContractError::IntentRevoked)));
}

#[test]
fn test_keeper_registration_requirement() {
    let (_env, _cid, client, admin, _oracle, user, keeper) = setup_env();

    // Enable keeper registration requirement
    client.set_keeper_registration_required(&true);
    assert!(client.is_keeper_registration_required());

    // Unregistered keeper intent authorization fails
    let res = client.try_authorize_keeper_intent(&user, &keeper, &KeeperScope::Claim, &100);
    assert_eq!(res, Err(Ok(ContractError::KeeperNotRegistered)));

    // Admin registers keeper
    client.register_keeper(&keeper);
    assert!(client.is_keeper_registered(&keeper));

    // Now authorization succeeds
    let nonce = client.authorize_keeper_intent(&user, &keeper, &KeeperScope::Claim, &100);
    assert_eq!(nonce, 0);

    // Deregister keeper
    client.deregister_keeper(&keeper);
    assert!(!client.is_keeper_registered(&keeper));

    // Execution by deregistered keeper fails
    let res = client.try_execute_keeper_claim(&keeper, &user, &nonce);
    assert_eq!(res, Err(Ok(ContractError::KeeperNotRegistered)));
}

#[test]
fn test_keeper_create_next_happy_path() {
    let (_env, _cid, client, _admin, _oracle, user, keeper) = setup_env();

    // Set round template
    client.set_round_template(&1_5000000, &None);

    let nonce = client.authorize_keeper_intent(&user, &keeper, &KeeperScope::CreateNext, &100);

    // Keeper executes create_next
    client.execute_keeper_create_next(&keeper, &user, &nonce);

    // Active round created
    let round = client.get_active_round().expect("Round should exist");
    assert_eq!(round.price_start, 1_5000000);
}
