// SPDX-License-Identifier: MIT
//! Tests for schema versioning and migration guards.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::DataKeyCore;
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
            .set(&DataKeyCore::SchemaVersion, &999u32);
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
        env.storage()
            .persistent()
            .remove(&DataKeyCore::SchemaVersion);
    });

    assert_eq!(client.get_schema_version(), 1u32);
    client.migrate_schema_v1_to_v2(&false);
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
        env.storage()
            .persistent()
            .remove(&DataKeyCore::SchemaVersion);
    });

    // Create an active round so migration is blocked.
    client.create_round(&1_0000000u128, &None);
    let res = client.try_migrate_schema_v1_to_v2(&false);
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
            .set(&DataKeyCore::SchemaVersion, &2u32);
    });

    assert_eq!(client.get_schema_version(), 2u32);
    client.migrate_schema_v2_to_v3(&false);
    assert_eq!(client.get_schema_version(), 3u32);

    let migrated = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKeyCore::MigratedToV3)
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
            .set(&DataKeyCore::SchemaVersion, &2u32);
    });

    client.create_round(&1_0000000u128, &None);
    let res = client.try_migrate_schema_v2_to_v3(&false);
    assert_eq!(res, Err(Ok(ContractError::MigrationActiveRound)));
}

#[test]
fn test_dry_run_v1_to_v2_passes_validation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Simulate legacy schema v1.
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .remove(&DataKeyCore::SchemaVersion);
    });

    assert_eq!(client.get_schema_version(), 1u32);

    // Dry-run should succeed.
    let res = client.try_migrate_schema_v1_to_v2(&true);
    assert_eq!(res, Ok(Ok(())));

    // Schema version must NOT have changed.
    assert_eq!(client.get_schema_version(), 1u32);
}

#[test]
fn test_dry_run_v2_to_v3_passes_validation() {
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
            .set(&DataKeyCore::SchemaVersion, &2u32);
    });

    assert_eq!(client.get_schema_version(), 2u32);

    // Dry-run should succeed.
    let res = client.try_migrate_schema_v2_to_v3(&true);
    assert_eq!(res, Ok(Ok(())));

    // Schema version must NOT have changed.
    assert_eq!(client.get_schema_version(), 2u32);

    // MigratedToV3 must NOT have been written.
    let migrated = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKeyCore::MigratedToV3)
    });
    assert_eq!(migrated, None);
}

#[test]
fn test_dry_run_still_validates_active_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Dry-run with an active round should still be rejected.
    client.create_round(&1_0000000u128, &None);
    let res = client.try_migrate_schema_v1_to_v2(&true);
    assert_eq!(res, Err(Ok(ContractError::MigrationActiveRound)));
}

#[test]
fn test_dry_run_still_validates_unsupported_version() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Set schema to v2, then try a dry-run of v1→v2 (wrong source).
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::SchemaVersion, &2u32);
    });

    let res = client.try_migrate_schema_v1_to_v2(&true);
    assert_eq!(res, Err(Ok(ContractError::UnsupportedSchemaVersion)));
}

#[test]
fn test_announce_next_schema() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // No next schema initially.
    assert_eq!(client.get_next_schema(), None);

    // Announce next schema v4.
    client.announce_next_schema(&4u32);
    assert_eq!(client.get_next_schema(), Some(4u32));

    // Overwrite with a different version.
    client.announce_next_schema(&5u32);
    assert_eq!(client.get_next_schema(), Some(5u32));
}

#[test]
fn test_announce_next_schema_rejects_invalid() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Must be > CURRENT_SCHEMA_VERSION (3).
    let res = client.try_announce_next_schema(&0u32);
    assert_eq!(res, Err(Ok(ContractError::UnsupportedSchemaVersion)));

    let res = client.try_announce_next_schema(&3u32);
    assert_eq!(res, Err(Ok(ContractError::UnsupportedSchemaVersion)));

    let res = client.try_announce_next_schema(&2u32);
    assert_eq!(res, Err(Ok(ContractError::UnsupportedSchemaVersion)));

    let res = client.try_announce_next_schema(&1u32);
    assert_eq!(res, Err(Ok(ContractError::UnsupportedSchemaVersion)));
}

#[test]
fn test_clear_next_schema() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    client.announce_next_schema(&4u32);
    assert_eq!(client.get_next_schema(), Some(4u32));

    client.clear_next_schema();
    assert_eq!(client.get_next_schema(), None);
}

#[test]
fn test_clear_next_schema_fails_when_not_set() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    let res = client.try_clear_next_schema();
    assert_eq!(res, Err(Ok(ContractError::UnsupportedSchemaVersion)));
}
