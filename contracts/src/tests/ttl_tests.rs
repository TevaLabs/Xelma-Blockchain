// SPDX-License-Identifier: MIT
//! Tests for storage TTL rent policy enforcement (Issue #142).

use super::config_helpers::apply_max_stake;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::DataKey;
use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_schema_version_and_admin_ttl_extended_on_interaction() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();

    // Initialize the contract
    client.initialize(&admin, &oracle);

    // SchemaVersion and Admin are long-lived keys.
    // Verify they are extended to BUMP_AMOUNT (518_400 ledgers)
    let schema_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::SchemaVersion)
    });
    assert!(schema_ttl >= 518_400);

    let admin_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Admin)
    });
    assert!(admin_ttl >= 518_400);
}

#[test]
fn test_balance_ttl_extended_on_mint_and_query() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    env.mock_all_auths();

    // Call mint_initial, which writes to user's Balance
    client.mint_initial(&user);

    // Verify balance key is extended to BUMP_AMOUNT
    let balance_ttl = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::Balance(user.clone()))
    });
    assert!(balance_ttl >= 518_400);

    // Query balance, which also reads and extends TTL
    client.balance(&user);
    let balance_ttl_after = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::Balance(user.clone()))
    });
    assert!(balance_ttl_after >= 518_400);
}

#[test]
fn test_paused_and_configs_ttl_extended_on_calls() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Set and get max stake to trigger TTL extensions
    apply_max_stake(&env, &client, Some(100_000_000_000));
    client.get_max_stake();

    let max_stake_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::MaxStake)
    });
    assert!(max_stake_ttl >= 518_400);

    // Paused config key checked in ensure_not_paused
    let paused_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Paused)
    });
    assert!(paused_ttl >= 518_400);
}

#[test]
fn test_oracle_and_heartbeat_ttl_extended() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Trigger heartbeat which updates OracleHeartbeat
    client.update_oracle_heartbeat(&0u32); // status 0 (online)

    let heartbeat_ttl = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::OracleHeartbeat)
    });
    assert!(heartbeat_ttl >= 518_400);

    let oracle_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Oracle)
    });
    assert!(oracle_ttl >= 518_400);
}


