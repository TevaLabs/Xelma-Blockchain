// SPDX-License-Identifier: MIT
//! Tests for schema versioning and migration guards.

use crate::common::_migrated_key;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, DataKeyCore, OraclePayload};
use soroban_sdk::testutils::{Address as _, Events, Ledger as _};
use soroban_sdk::{symbol_short, Address, Env, Symbol, TryIntoVal};

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
fn test_dry_run_v2_to_v3_still_validates_active_round() {
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

    // Create an active round so migration is blocked.
    client.create_round(&1_0000000u128, &None);

    // Dry-run should fail with MigrationActiveRound
    let res = client.try_migrate_schema_v2_to_v3(&true);
    assert_eq!(res, Err(Ok(ContractError::MigrationActiveRound)));

    // Schema version must NOT have changed.
    assert_eq!(client.get_schema_version(), 2u32);

    // MigratedToV3 marker must NOT be set.
    let migrated = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKeyCore::MigratedToV3)
    });
    assert_eq!(migrated, None);
}

#[test]
fn test_migration_active_round_emits_action_rejected_event() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Set schema to v1
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .remove(&DataKeyCore::SchemaVersion);
    });

    client.create_round(&1_0000000u128, &None);

    // Call admin::migrate_schema_v1_to_v2 directly in contract context to verify event emission
    env.as_contract(&contract_id, || {
        let res = crate::admin::migrate_schema_v1_to_v2(env.clone(), false);
        assert_eq!(res, Err(ContractError::MigrationActiveRound));
    });

    let events = env.events().all();
    let rejected_event = events.iter().rev().find(|(_, topics, _)| {
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("action"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("rejct"))
    });
    assert!(rejected_event.is_some(), "Rejection event must be emitted");
    let (_, topics, data) = rejected_event.unwrap();
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("action"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("rejct"))
    );
    let expected_data: (Address, Symbol, u32) = (
        admin.clone(),
        symbol_short!("migrate"),
        ContractError::MigrationActiveRound as u32,
    );
    assert_eq!(data.try_into_val(&env), Ok(expected_data.clone()));

    // Also verify dry-run for v2_to_v3 emits the rejection event
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::SchemaVersion, &2u32);
        let res = crate::admin::migrate_schema_v2_to_v3(env.clone(), true);
        assert_eq!(res, Err(ContractError::MigrationActiveRound));
    });

    let events = env.events().all();
    let (_, _, data) = events
        .iter()
        .rev()
        .find(|(_, topics, _)| {
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("action"))
                && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("rejct"))
        })
        .expect("Rejection event for dry_run v2_to_v3 must be emitted");
    assert_eq!(data.try_into_val(&env), Ok(expected_data));
}

#[test]
fn test_migration_blocked_during_active_round_succeeds_after_resolution() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user);

    // Start with schema v1
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .remove(&DataKeyCore::SchemaVersion);
    });
    assert_eq!(client.get_schema_version(), 1u32);

    // Create an active round
    client.create_round(&1_0000000u128, &None);
    let active_round = client.get_active_round().expect("active round exists");

    // Both dry-run and real migration must fail while round is active
    assert_eq!(
        client.try_migrate_schema_v1_to_v2(&true),
        Err(Ok(ContractError::MigrationActiveRound))
    );
    assert_eq!(
        client.try_migrate_schema_v1_to_v2(&false),
        Err(Ok(ContractError::MigrationActiveRound))
    );
    assert_eq!(client.get_schema_version(), 1u32);

    // Place a bet and advance ledger past end_ledger
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    env.ledger().with_mut(|li| {
        li.sequence_number = active_round.end_ledger + 1;
    });

    // Oracle resolves the round
    let round_id = active_round.start_ledger;
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    // Verify active round is resolved and cleared
    assert_eq!(client.get_active_round(), None);

    // Now dry-run passes validation
    assert_eq!(client.try_migrate_schema_v1_to_v2(&true), Ok(Ok(())));
    assert_eq!(client.get_schema_version(), 1u32);

    // Real migration succeeds and version advances to 2
    client.migrate_schema_v1_to_v2(&false);
    assert_eq!(client.get_schema_version(), 2u32);
}

#[test]
fn test_migration_blocked_during_active_round_succeeds_after_cancellation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Start with schema v2
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::SchemaVersion, &2u32);
    });
    assert_eq!(client.get_schema_version(), 2u32);

    // Create an active round
    client.create_round(&1_0000000u128, &None);
    assert!(client.get_active_round().is_some());

    // Both dry-run and real migration must fail while round is active
    assert_eq!(
        client.try_migrate_schema_v2_to_v3(&true),
        Err(Ok(ContractError::MigrationActiveRound))
    );
    assert_eq!(
        client.try_migrate_schema_v2_to_v3(&false),
        Err(Ok(ContractError::MigrationActiveRound))
    );
    assert_eq!(client.get_schema_version(), 2u32);

    // Admin cancels the active round
    client.cancel_round(&0u32);

    // Verify active round is cleared
    assert_eq!(client.get_active_round(), None);

    // Dry-run passes
    assert_eq!(client.try_migrate_schema_v2_to_v3(&true), Ok(Ok(())));
    assert_eq!(client.get_schema_version(), 2u32);

    // Real migration succeeds and version advances to 3
    client.migrate_schema_v2_to_v3(&false);
    assert_eq!(client.get_schema_version(), 3u32);

    // Marker MigratedToV3 is recorded
    let migrated = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKeyCore::MigratedToV3)
    });
    assert_eq!(migrated, Some(true));
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
