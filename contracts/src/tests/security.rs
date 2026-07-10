// SPDX-License-Identifier: MIT
//! Security tests for Oracle data freshness and round validation.

use super::config_helpers::{apply_oracle_max_deviation_bps, apply_oracle_stale_threshold};
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{DataKey, OraclePayload};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, BytesN, Env, IntoVal, TryIntoVal,
};

#[test]
fn test_resolve_round_stale_timestamp() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);

    // Advance ledger time to 1000
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
        li.sequence_number = 12; // Allow resolution
    });

    // Submit payload with timestamp 600 (400s old, > 300s limit)
    let payload = OraclePayload {
        price: 1_5000000,
        timestamp: 600,
        round_id: 0, // Starts at ledger 0
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    };

    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::StaleOracleData)));
}

#[test]
fn test_resolve_round_invalid_round_id() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Submit payload with wrong round_id (e.g., 999 instead of 0)
    let payload = OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 999,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    };

    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::InvalidOracleRound)));
}

#[test]
fn test_resolve_round_valid_payload() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    // Valid payload: within 300s and correct round_id
    let payload = OraclePayload {
        price: 1_5000000,
        timestamp: 900, // 100s old, OK
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    };

    client.resolve_round(&payload);
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_resolve_round_future_timestamp() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);

    // Current ledger time is 1000
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
        li.sequence_number = 12;
    });

    // Submit payload with timestamp 1001 (future)
    let payload = OraclePayload {
        price: 1_5000000,
        timestamp: 1001,
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    };

    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::FutureOracleData)));
}

// ─── Cancel-round security tests (Issue #111) ────────────────────────────────

#[test]
fn test_cancelled_round_cannot_be_resolved() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);

    client.cancel_round(&0u32);

    // After cancellation there is no active round, so resolve_round returns NoActiveRound
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

#[test]
fn test_cancel_round_without_admin_auth_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    // Initialize with only admin auth
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &oracle).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &oracle);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_round",
            args: (1_0000000u128, Option::<u32>::None).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.create_round(&1_0000000, &None);

    // No auth for cancel_round
    let result = client.try_cancel_round(&0u32);
    assert!(result.is_err());
}

// ─── Oracle nonce replay protection (Issue #118) ─────────────────────────────

/// A nonce already consumed for a round must be rejected on re-submission.
/// We seed the consumed-nonce marker to simulate a prior submission, then
/// assert the resolver rejects a payload reusing that nonce for the same round.
#[test]
fn test_resolve_round_duplicate_nonce_rejected() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    // Simulate a prior submission having consumed nonce 42 for this round.
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ConsumedOracleNonce(round.round_id, 42u64), &true);
    });

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: 42u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::OracleNonceReused)));
}

/// A fresh, unique nonce resolves normally and records the consumed marker.
#[test]
fn test_resolve_round_unique_nonce_resolves() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: 7u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    // Round resolved and the nonce is recorded as consumed for that round.
    assert_eq!(client.get_active_round(), None);
    env.as_contract(&contract_id, || {
        let consumed: bool = env
            .storage()
            .persistent()
            .get(&DataKey::ConsumedOracleNonce(round.round_id, 7u64))
            .unwrap_or(false);
        assert!(consumed, "resolved nonce must be marked consumed");
    });
}

// ─── Oracle heartbeat and liveness tests ─────────────────────────────────────

#[test]
fn test_oracle_heartbeat_requires_oracle_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &oracle).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &oracle);

    // No oracle auth set up — must fail
    let result = client.try_update_oracle_heartbeat(&0u32);
    assert!(result.is_err());
}

#[test]
fn test_oracle_heartbeat_updates_timestamp_and_status() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    env.ledger().with_mut(|li| {
        li.timestamp = 500;
    });

    client.update_oracle_heartbeat(&0u32);

    let record = client.get_oracle_heartbeat().expect("heartbeat must exist");
    assert_eq!(record.timestamp, 500);
    assert_eq!(record.status, 0);

    // Update to degraded status
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });
    client.update_oracle_heartbeat(&1u32);

    let record = client.get_oracle_heartbeat().unwrap();
    assert_eq!(record.timestamp, 1000);
    assert_eq!(record.status, 1);
}

#[test]
fn test_oracle_heartbeat_invalid_status_rejected() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let result = client.try_update_oracle_heartbeat(&3u32);
    assert_eq!(result, Err(Ok(ContractError::InvalidMode)));
}

#[test]
fn test_oracle_liveness_within_threshold() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Heartbeat at t=0
    env.ledger().with_mut(|li| {
        li.timestamp = 0;
    });
    client.update_oracle_heartbeat(&0u32);

    // Check liveness 100 s later (well within 3600 s default)
    env.ledger().with_mut(|li| {
        li.timestamp = 100;
    });
    assert!(client.is_oracle_live());
}

#[test]
fn test_oracle_liveness_stale_after_threshold() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Heartbeat at t=0
    env.ledger().with_mut(|li| {
        li.timestamp = 0;
    });
    client.update_oracle_heartbeat(&0u32);

    // Check 4000 s later — beyond 3600 s default threshold
    env.ledger().with_mut(|li| {
        li.timestamp = 4000;
    });
    assert!(!client.is_oracle_live());
}

#[test]
fn test_oracle_liveness_offline_status_not_live() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    env.ledger().with_mut(|li| {
        li.timestamp = 0;
    });
    // Record offline status
    client.update_oracle_heartbeat(&2u32);

    // Even within threshold, offline means not live
    env.ledger().with_mut(|li| {
        li.timestamp = 10;
    });
    assert!(!client.is_oracle_live());
}

#[test]
fn test_oracle_liveness_no_heartbeat_returns_false() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // No heartbeat recorded — must return false
    assert!(!client.is_oracle_live());
}

#[test]
fn test_oracle_heartbeat_event_emitted() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    client.update_oracle_heartbeat(&0u32);

    let events = env.events().all();
    let hb_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("oracle"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("hbeat"))
    });
    assert!(
        hb_event.is_some(),
        "Heartbeat event must be emitted on update_oracle_heartbeat"
    );
}

#[test]
fn test_set_oracle_stale_threshold_admin_only() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &oracle).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &oracle);

    // No admin auth for set_oracle_stale_threshold
    let result = client.try_set_oracle_stale_threshold(&1800u64);
    assert!(result.is_err());
}

#[test]
fn test_set_oracle_stale_threshold_validation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Below minimum (< 60)
    let result = client.try_set_oracle_stale_threshold(&59u64);
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));

    // Above maximum (> 86400)
    let result = client.try_set_oracle_stale_threshold(&86_401u64);
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));

    // Valid value
    apply_oracle_stale_threshold(&env, &client, 1800u64);
    assert_eq!(client.get_oracle_stale_threshold(), 1800u64);
}

// ─── Oracle deviation guardrails tests ───────────────────────────────────────

#[test]
fn test_oracle_deviation_rejected_when_over_threshold() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();

    // Set max deviation to 5% (500 bp)
    apply_oracle_max_deviation_bps(&env, &client, Some(500u32));

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    // 50% jump: diff_bps = 5000 > 500
    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000u128,
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::OracleDeviationExceeded)));
}

#[test]
fn test_oracle_deviation_allows_at_exact_threshold() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();

    // 5% (500 bp)
    apply_oracle_max_deviation_bps(&env, &client, Some(500u32));

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    // Exactly 5%: 1.00 -> 1.05 => diff_bps = 500
    client.resolve_round(&OraclePayload {
        price: 1_0500000u128,
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_oracle_deviation_rounding_floor_is_deterministic() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    // Start price 3, final 4 => diff_bps = floor(1*10000/3)=3333
    client.create_round(&3u128, &None);
    let round = client.get_active_round().unwrap();

    apply_oracle_max_deviation_bps(&env, &client, Some(3333u32));

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    // At threshold should pass
    client.resolve_round(&OraclePayload {
        price: 4u128,
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_oracle_deviation_override_allows_over_threshold_and_emits_event() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();

    apply_oracle_max_deviation_bps(&env, &client, Some(500u32)); // 5%
    client.arm_oracle_deviation_override();

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    client.resolve_round(&OraclePayload {
        price: 2_0000000u128, // 100% jump
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    // Verify override event emitted (check before env.as_contract which resets event scope)
    let events = env.events().all();
    let override_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("oracle"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("override"))
    });
    assert!(override_event.is_some(), "override event must be emitted");

    // Override is one-shot and must be cleared
    env.as_contract(&contract_id, || {
        let armed: bool = env
            .storage()
            .persistent()
            .get(&DataKey::OracleDeviationOverrideArmed)
            .unwrap_or(false);
        assert!(!armed, "override must be cleared after use");
    });
}

#[test]
fn test_oracle_liveness_custom_threshold() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Set a short 120 s threshold
    apply_oracle_stale_threshold(&env, &client, 120u64);

    env.ledger().with_mut(|li| {
        li.timestamp = 0;
    });
    client.update_oracle_heartbeat(&0u32);

    // 100 s later — within custom 120 s threshold
    env.ledger().with_mut(|li| {
        li.timestamp = 100;
    });
    assert!(client.is_oracle_live());

    // 130 s later — beyond 120 s threshold
    env.ledger().with_mut(|li| {
        li.timestamp = 130;
    });
    assert!(!client.is_oracle_live());
}

/// Boundary nonces (0 and u64::MAX) are rejected on reuse for the same round.
#[test]
fn test_resolve_round_nonce_boundary_values() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    // Pre-seed both boundary nonces as consumed for this round.
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ConsumedOracleNonce(round.round_id, 0u64), &true);
        env.storage().persistent().set(
            &DataKey::ConsumedOracleNonce(round.round_id, u64::MAX),
            &true,
        );
    });

    let zero = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: 0u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(zero, Err(Ok(ContractError::OracleNonceReused)));

    let max = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: u64::MAX,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(max, Err(Ok(ContractError::OracleNonceReused)));
}

// ─── Oracle domain-context validation tests (Issue #143) ────────────────────

#[test]
fn test_resolve_round_wrong_network_id_rejected() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    let wrong_network = BytesN::from_array(&env, &[0xFFu8; 32]);

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: wrong_network,
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::OracleNetworkMismatch)));
}

#[test]
fn test_resolve_round_wrong_contract_addr_rejected() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    let wrong_contract = Address::generate(&env);

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: wrong_contract,
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::OracleNetworkMismatch)));
}

#[test]
fn test_resolve_round_valid_domain_context_resolves() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    // Correct network + correct contract => resolves normally
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_resolve_round_both_network_and_contract_wrong() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    let wrong_network = BytesN::from_array(&env, &[0xFFu8; 32]);
    let wrong_contract = Address::generate(&env);

    // Network is checked first, so we get OracleNetworkMismatch
    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: 900,
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: wrong_network,
        contract_addr: wrong_contract,
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::OracleNetworkMismatch)));
}

// ─── Protocol health endpoint tests ──────────────────────────────────────────

#[test]
fn test_protocol_health_no_heartbeat_unknown_oracle() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // No heartbeat recorded → oracle_status=3, oracle_live=false
    env.ledger().with_mut(|li| {
        li.sequence_number = 1;
    });
    let health = client.get_protocol_health();
    assert_eq!(health.oracle_status, 3); // unknown
    assert!(!health.oracle_live);
    assert!(!health.has_active_round);
    assert_eq!(health.status_code, 2); // ORACLE_STALE
    assert!(health.ledger_sequence > 0);
}

#[test]
fn test_protocol_health_oracle_offline() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Record offline heartbeat
    env.ledger().with_mut(|li| {
        li.timestamp = 500;
    });
    client.update_oracle_heartbeat(&2u32); // offline

    let health = client.get_protocol_health();
    assert!(!health.oracle_live);
    assert_eq!(health.oracle_status, 2); // offline
    assert_eq!(health.status_code, 2); // ORACLE_STALE
}

#[test]
fn test_protocol_health_round_resolvable_stale() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);

    // Advance past end_ledger so round is resolvable
    env.ledger().with_mut(|li| {
        li.sequence_number = 30;
        li.timestamp = 100;
    });

    // Keep oracle alive
    client.update_oracle_heartbeat(&0u32);

    let health = client.get_protocol_health();
    assert!(health.has_active_round);
    assert_eq!(health.active_round_phase, 3); // resolvable
    assert_eq!(health.status_code, 3); // ROUND_STALE
}

#[test]
fn test_protocol_health_multiple_issues() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);

    // No heartbeat (oracle unknown) + round past end_ledger → multiple issues
    env.ledger().with_mut(|li| {
        li.sequence_number = 30;
        li.timestamp = 100;
    });

    let health = client.get_protocol_health();
    assert!(!health.oracle_live);
    assert!(health.has_active_round);
    assert_eq!(health.active_round_phase, 3); // resolvable
    assert_eq!(health.status_code, 5); // MULTIPLE_ISSUES
}

#[test]
fn test_protocol_health_schema_version_present() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let health = client.get_protocol_health();
    assert_eq!(health.schema_version, 3);
}

#[test]
fn test_protocol_health_round_betting_phase() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Oracle heartbeat active
    env.ledger().with_mut(|li| {
        li.timestamp = 100;
    });
    client.update_oracle_heartbeat(&0u32);

    // Create round at ledger 6
    env.ledger().with_mut(|li| {
        li.sequence_number = 6;
    });
    client.create_round(&1_0000000, &None);

    // Still in betting window (ledger 6, bet_end = 6+6=12)
    let health = client.get_protocol_health();
    assert!(health.has_active_round);
    assert_eq!(health.active_round_phase, 1); // betting
    assert_eq!(health.status_code, 0); // HEALTHY
}

#[test]
fn test_protocol_health_round_running_phase() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Oracle heartbeat active
    env.ledger().with_mut(|li| {
        li.timestamp = 100;
    });
    client.update_oracle_heartbeat(&0u32);

    // Create round at ledger 0
    client.create_round(&1_0000000, &None);
    // Default windows: bet=6, run=12
    // bet_end = 6, end = 12

    // Advance into running phase (past bet_end, before end)
    env.ledger().with_mut(|li| {
        li.sequence_number = 8;
    });

    let health = client.get_protocol_health();
    assert!(health.has_active_round);
    assert_eq!(health.active_round_phase, 2); // running
    assert_eq!(health.status_code, 0); // HEALTHY
}
// ── Oracle confidence score tests ────────────────────────────────────────────

#[test]
fn test_confidence_below_threshold_rejected() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.set_oracle_min_confidence_bps(&Some(8000u32));
    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 100;
    });

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: Some(5000u32),
    });
    assert_eq!(result, Err(Ok(ContractError::InvalidPrice)));
}

#[test]
fn test_confidence_above_threshold_accepted() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.set_oracle_min_confidence_bps(&Some(8000u32));
    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 100;
    });

    client.resolve_round(&OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: Some(9000u32),
    });
}

#[test]
fn test_missing_confidence_accepted_when_not_strict() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.set_oracle_min_confidence_bps(&Some(8000u32));
    // strict mode NOT enabled
    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 100;
    });

    // Legacy payload without confidence accepted when strict mode is off
    client.resolve_round(&OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
}

#[test]
fn test_missing_confidence_rejected_in_strict_mode() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.set_oracle_min_confidence_bps(&Some(8000u32));
    client.set_oracle_strict_mode(&true);
    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 100;
    });

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::InvalidPrice)));
}

#[test]
fn test_no_confidence_check_when_threshold_unset() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    // No min confidence configured
    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 100;
    });

    // Even zero confidence accepted when threshold unset
    client.resolve_round(&OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: Some(0u32),
    });
}
