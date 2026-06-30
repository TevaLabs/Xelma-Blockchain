// SPDX-License-Identifier: MIT
//! Tests for emergency pause and recovery controls.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal,
};

fn setup_contract(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    (client, contract_id, admin, oracle)
}

#[test]
fn test_pause_and_unpause_by_admin() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    assert!(!client.is_paused());

    client.pause_contract();
    assert!(client.is_paused());

    client.unpause_contract();
    assert!(!client.is_paused());
}

#[test]
fn test_pause_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let attacker = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause_contract",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_pause_contract();
    assert!(result.is_err());
}

#[test]
fn test_mutations_fail_while_paused() {
    let env = Env::default();
    let (client, contract_id, admin, oracle) = setup_contract(&env);
    let user = Address::generate(&env);

    client.mint_initial(&user);
    client.pause_contract();
    assert!(client.is_paused());
    assert_eq!(client.get_admin(), Some(admin));
    assert_eq!(client.get_oracle(), Some(oracle));
    assert_eq!(client.balance(&user), 1000_0000000);

    let create_round_result = client.try_create_round(&1_0000000, &None);
    assert_eq!(create_round_result, Err(Ok(ContractError::ContractPaused)));

    let bet_result = client.try_place_bet(&user, &10_0000000, &BetSide::Up);
    assert_eq!(bet_result, Err(Ok(ContractError::ContractPaused)));

    let predict_result = client.try_place_precision_prediction(&user, &10_0000000, &2297);
    assert_eq!(predict_result, Err(Ok(ContractError::ContractPaused)));

    let windows_result = client.try_set_windows(&8, &16);
    assert_eq!(windows_result, Err(Ok(ContractError::ContractPaused)));

    let claim_result = client.try_claim_winnings(&user);
    assert_eq!(claim_result, Err(Ok(ContractError::ContractPaused)));

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    let resolve_result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
    assert_eq!(resolve_result, Err(Ok(ContractError::ContractPaused)));

    client.unpause_contract();
    assert!(!client.is_paused());
}

#[test]
fn test_protocol_health_paused() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    // Not paused → status depends on other state; pause to verify status_code=1
    client.pause_contract();

    let health = client.get_protocol_health();
    assert!(health.paused);
    assert_eq!(health.status_code, 1); // PAUSED
}

#[test]
fn test_protocol_health_not_paused_healthy() {
    let env = Env::default();
    let (client, contract_id, _admin, oracle) = setup_contract(&env);
    let user = Address::generate(&env);

    // With oracle heartbeat active + no active round → NO_ACTIVE_ROUND
    env.ledger().with_mut(|li| {
        li.timestamp = 100;
    });
    client.update_oracle_heartbeat(&0u32);

    let health = client.get_protocol_health();
    assert!(!health.paused);
    assert!(health.oracle_live);
    assert!(!health.has_active_round);
    assert_eq!(health.status_code, 4); // NO_ACTIVE_ROUND

    // Create round → should become HEALTHY (betting phase)
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);

    let health = client.get_protocol_health();
    assert!(!health.paused);
    assert!(health.oracle_live);
    assert!(health.has_active_round);
    assert_eq!(health.active_round_phase, 1); // betting
    assert_eq!(health.status_code, 0); // HEALTHY
}

#[test]
fn test_protocol_health_oracle_stale() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    // Heartbeat at t=0
    env.ledger().with_mut(|li| {
        li.timestamp = 0;
    });
    client.update_oracle_heartbeat(&0u32);

    // Advance past threshold
    env.ledger().with_mut(|li| {
        li.timestamp = 4000;
    });

    let health = client.get_protocol_health();
    assert!(!health.paused);
    assert!(!health.oracle_live);
    assert_eq!(health.oracle_status, 0); // stored as active, but stale
    assert_eq!(health.status_code, 2); // ORACLE_STALE
}

#[test]
fn test_protocol_health_mint_initial_fails_while_paused() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup_contract(&env);
    let user = Address::generate(&env);

    client.pause_contract();

    let result = client.try_mint_initial(&user);
    assert!(result.is_err());
}


