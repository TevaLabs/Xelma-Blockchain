// SPDX-License-Identifier: MIT
//! Tests for schema versioning and migration guards.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::DataKey;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

#[test]
fn test_rejects_unsupported_schema_version() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Simulate a future/unsupported schema version.
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersion, &999u32);
    });

    // Any mutating entrypoint should fail clearly.
    let res = client.try_create_round(&1_0000000u128, &None);
    assert_eq!(res, Err(Ok(ContractError::UnsupportedSchemaVersion)));
}

#[test]
fn test_migrate_v1_to_v2_happy_path() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Simulate legacy deployment missing schema version (treated as v1).
    env.as_contract(&contract_id, || {
        env.storage().persistent().remove(&DataKey::SchemaVersion);
    });

    assert_eq!(client.get_schema_version(), 1u32);
    client.migrate_schema_v1_to_v2();
    assert_eq!(client.get_schema_version(), 2u32);
}

#[test]
fn test_migration_blocked_when_round_active() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Simulate legacy schema.
    env.as_contract(&contract_id, || {
        env.storage().persistent().remove(&DataKey::SchemaVersion);
    });

    // Create an active round so migration is blocked.
    client.create_round(&1_0000000u128, &None);
    let res = client.try_migrate_schema_v1_to_v2();
    assert_eq!(res, Err(Ok(ContractError::MigrationActiveRound)));
}

#[test]
fn test_migrate_v2_to_v3_happy_path() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Simulate schema v2 (current before this migration).
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersion, &2u32);
    });

    assert_eq!(client.get_schema_version(), 2u32);
    client.migrate_schema_v2_to_v3();
    assert_eq!(client.get_schema_version(), 3u32);

    let migrated = env.as_contract(&contract_id, || {
        env.storage().persistent().get::<_, bool>(&DataKey::MigratedToV3)
    });
    assert_eq!(migrated, Some(true));
}

#[test]
fn test_migration_v2_to_v3_blocked_when_round_active() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Simulate schema v2.
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::SchemaVersion, &2u32);
    });

    client.create_round(&1_0000000u128, &None);
    let res = client.try_migrate_schema_v2_to_v3();
    assert_eq!(res, Err(Ok(ContractError::MigrationActiveRound)));
}
