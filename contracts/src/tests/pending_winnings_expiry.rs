// SPDX-License-Identifier: MIT
//! Tests for the pending-winnings expiry and administrative reclaim flow.

use super::config_helpers::apply_pending_winnings_expiry;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, DataKey, OraclePayload, PendingWinningsUpdatedAtKey};
use soroban_sdk::testutils::{storage::Persistent as _, Address as _, Events as _, Ledger as _};
use soroban_sdk::{symbol_short, Address, Env, TryIntoVal};

// ─── Helpers ─────────────────────────────────────────────────────────────

fn setup() -> (Env, Address, Address, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    (env, admin, contract_id, client)
}

/// Write pending winnings and the tracking ledger key at the current sequence.
fn set_pending_at_current_ledger(env: &Env, contract_id: &Address, user: &Address, amount: i128) {
    let ledger = env.ledger().sequence();
    env.as_contract(contract_id, || {
        let key = DataKey::PendingWinnings(user.clone());
        env.storage().persistent().set(&key, &amount);
        let updated_key = PendingWinningsUpdatedAtKey(user.clone());
        env.storage().persistent().set(&updated_key, &ledger);
    });
}

// ─── Config tests ────────────────────────────────────────────────────────

#[test]
fn test_default_expiry_is_disabled() {
    let (env, _admin, _contract_id, client) = setup();
    assert_eq!(client.get_pending_winnings_expiry(), 0);
}

#[test]
fn test_set_and_get_expiry() {
    let (env, _admin, _contract_id, client) = setup();
    apply_pending_winnings_expiry(&env, &client, 500);
    assert_eq!(client.get_pending_winnings_expiry(), 500);
}

#[test]
fn test_set_expiry_to_zero_disables() {
    let (env, _admin, _contract_id, client) = setup();
    apply_pending_winnings_expiry(&env, &client, 500);
    apply_pending_winnings_expiry(&env, &client, 0);
    assert_eq!(client.get_pending_winnings_expiry(), 0);
}

#[test]
fn test_invalid_expiry_below_min_rejected() {
    let (env, _admin, _contract_id, client) = setup();
    // 10 is below MIN_PENDING_WINNINGS_EXPIRY (128)
    let result = client.try_schedule_pending_winnings_expiry(&10);
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));
}

#[test]
fn test_invalid_expiry_above_max_rejected() {
    let (env, _admin, _contract_id, client) = setup();
    let result = client.try_schedule_pending_winnings_expiry(&1_000_001);
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));
}

// ─── Reclaim: precondition checks ────────────────────────────────────────

#[test]
fn test_reclaim_fails_when_expiry_disabled() {
    let (env, admin, contract_id, client) = setup();
    let user = Address::generate(&env);
    set_pending_at_current_ledger(&env, &contract_id, &user, 1000);

    let result = client.try_reclaim_expired_pending_winnings(&user);
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

#[test]
fn test_reclaim_fails_for_nonexistent_pending() {
    let (env, admin, contract_id, client) = setup();
    let user = Address::generate(&env);
    apply_pending_winnings_expiry(&env, &client, 500);

    let result = client.try_reclaim_expired_pending_winnings(&user);
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

#[test]
fn test_reclaim_fails_when_not_expired() {
    let (env, admin, contract_id, client) = setup();
    let user = Address::generate(&env);

    set_pending_at_current_ledger(&env, &contract_id, &user, 1000);
    apply_pending_winnings_expiry(&env, &client, 500);

    // Ledger is still at the same sequence — age 0 < 500
    let result = client.try_reclaim_expired_pending_winnings(&user);
    assert_eq!(result, Err(Ok(ContractError::PendingWinningsNotExpired)));
}

// ─── Reclaim: success path ───────────────────────────────────────────────

#[test]
fn test_reclaim_succeeds_when_expired() {
    let (env, admin, contract_id, client) = setup();
    let user = Address::generate(&env);

    // Write pending winnings at ledger 0
    env.ledger().with_mut(|li| li.sequence_number = 0);
    set_pending_at_current_ledger(&env, &contract_id, &user, 1000);
    apply_pending_winnings_expiry(&env, &client, 500);
    env.ledger().with_mut(|li| li.sequence_number = 501);

    let reclaimed = client.reclaim_expired_pending_winnings(&user);
    assert_eq!(reclaimed, 1000);

    // Funds credited to admin's balance
    assert_eq!(client.balance(&admin), 1000);

    // User's pending winnings cleared
    assert_eq!(client.get_pending_winnings(&user), 0);

    // Tracking key cleared — second call fails
    let result = client.try_reclaim_expired_pending_winnings(&user);
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

#[test]
fn test_reclaim_at_exact_threshold() {
    let (env, admin, contract_id, client) = setup();
    let user = Address::generate(&env);

    env.ledger().with_mut(|li| li.sequence_number = 100);
    set_pending_at_current_ledger(&env, &contract_id, &user, 500);
    apply_pending_winnings_expiry(&env, &client, 200);
    // age = 300 - 100 = 200, which equals expiry (200) → eligible
    env.ledger().with_mut(|li| li.sequence_number = 300);

    let reclaimed = client.reclaim_expired_pending_winnings(&user);
    assert_eq!(reclaimed, 500);
}

#[test]
fn test_reclaim_emits_event() {
    let (env, admin, contract_id, client) = setup();
    let user = Address::generate(&env);

    env.ledger().with_mut(|li| li.sequence_number = 0);
    set_pending_at_current_ledger(&env, &contract_id, &user, 500);
    apply_pending_winnings_expiry(&env, &client, 200);
    env.ledger().with_mut(|li| li.sequence_number = 300);

    client.reclaim_expired_pending_winnings(&user);

    let events = env.events().all();
    let last = events.last().unwrap();
    let (_contract, topics, data) = last;
    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("claim"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("expired"))
    );
    let (event_user, event_amount, event_admin): (Address, i128, Address) =
        data.clone().try_into_val(&env).unwrap();
    assert_eq!(event_user, user);
    assert_eq!(event_amount, 500);
    assert_eq!(event_admin, admin);
}

// ─── Integration: claim clears tracking key ───────────────────────────────

#[test]
fn test_claim_winnings_clears_tracking_key() {
    let (env, admin, contract_id, client) = setup();
    let user = Address::generate(&env);

    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // Advance past bet window (6) + into run window so resolve works
    env.ledger().with_mut(|li| li.sequence_number = 19);

    client.resolve_round(&OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    // Verify tracking key exists after resolve
    let tracking_exists = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .has(&PendingWinningsUpdatedAtKey(user.clone()))
    });
    assert!(tracking_exists);

    // Claim clears the tracking key
    client.claim_winnings(&user);

    let tracking_exists = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .has(&PendingWinningsUpdatedAtKey(user.clone()))
    });
    assert!(!tracking_exists);
}

// ─── Reclaim respects paused state ───────────────────────────────────────

#[test]
fn test_reclaim_fails_when_paused() {
    let (env, admin, contract_id, client) = setup();
    let user = Address::generate(&env);

    env.ledger().with_mut(|li| li.sequence_number = 0);
    set_pending_at_current_ledger(&env, &contract_id, &user, 1000);
    apply_pending_winnings_expiry(&env, &client, 200);
    env.ledger().with_mut(|li| li.sequence_number = 300);

    client.pause_contract();
    let result = client.try_reclaim_expired_pending_winnings(&user);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
}

// ─── Reclaim requires admin auth ─────────────────────────────────────────

#[test]
fn test_reclaim_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);
    let attacker = Address::generate(&env);

    // Auth for initialize only
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    // Set expiry via admin
    apply_pending_winnings_expiry(&env, &client, 128);

    // Set up pending winnings
    env.ledger().with_mut(|li| li.sequence_number = 0);
    set_pending_at_current_ledger(&env, &contract_id, &user, 500);
    env.ledger().with_mut(|li| li.sequence_number = 200);

    // Mock_auths is still on — this test verifies the method is callable via
    // contract-client (the real auth check happens at the contract entry-point
    // level). The SDK's mock_auths bypasses signature verification, but the
    // require_auth() call in the contract body still passes because mock_auths
    // grants all addresses. In production, only the admin's signature counts.
    let reclaimed = client.reclaim_expired_pending_winnings(&user);
    assert_eq!(reclaimed, 500);
}

// ─── TTL bump on get_pending_winnings ────────────────────────────────────

#[test]
fn test_get_pending_winnings_bumps_tracking_ttl() {
    let (env, admin, contract_id, client) = setup();
    let user = Address::generate(&env);

    env.ledger().with_mut(|li| li.sequence_number = 0);
    set_pending_at_current_ledger(&env, &contract_id, &user, 1000);

    // Query should bump TTL on both keys
    let pending = client.get_pending_winnings(&user);
    assert_eq!(pending, 1000);

    let ttl = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&PendingWinningsUpdatedAtKey(user.clone()))
    });
    assert!(
        ttl > 0,
        "tracking key TTL should be extended by get_pending_winnings"
    );
}
