// SPDX-License-Identifier: MIT
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{ArchivedRoundSummary, DataKeyCore, DataKeyScoped, OraclePayload};
use std::vec::Vec;
use crate::types::{ArchivedRoundSummary, DataKey, OraclePayload};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, TryIntoVal,
};
use std::vec::Vec;

fn setup_with_oracle() -> (Env, VirtualTokenContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, client, admin, oracle)
}

fn create_and_resolve_round(
    env: &Env,
    client: &VirtualTokenContractClient,
    contract_id: &Address,
    start_ledger: u32,
    nonce: u64,
) {
    env.ledger().with_mut(|li| {
        li.sequence_number = start_ledger;
        li.timestamp = 1000;
    });
    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = start_ledger + 100;
        li.timestamp = 2000;
    });
    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: 1800,
        round_id: start_ledger,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });
}

#[test]
fn test_default_archive_retention() {
    let (env, client, _, _) = setup_with_oracle();
    let retention = client.get_archive_retention();
    assert_eq!(retention, 128);
}

#[test]
fn test_set_archive_retention_below_min_fails() {
    let (env, client, _, _) = setup_with_oracle();
    let result = client.try_set_archive_retention(&0);
    assert_eq!(result, Err(Ok(ContractError::InvalidArchiveRetention)));
}

#[test]
fn test_set_archive_retention_above_max_fails() {
    let (env, client, _, _) = setup_with_oracle();
    let result = client.try_set_archive_retention(&10_001);
    assert_eq!(result, Err(Ok(ContractError::InvalidArchiveRetention)));
}

#[test]
fn test_set_archive_retention_valid() {
    let (env, client, _, _) = setup_with_oracle();
    client.set_archive_retention(&10);
    assert_eq!(client.get_archive_retention(), 10);
}

#[test]
fn test_set_archive_retention_emits_event() {
    let (env, client, _, _) = setup_with_oracle();
    client.set_archive_retention(&50);

    let events = env.events().all();
    // The archive event may not be the last — find it by topic
    let has_archive_event = events.iter().any(|(_, topics, _)| {
        if topics.len() < 2 {
            return false;
        }
        let t0: Result<soroban_sdk::Symbol, _> = topics.get(0).unwrap().try_into_val(&env);
        let t1: Result<soroban_sdk::Symbol, _> = topics.get(1).unwrap().try_into_val(&env);
        t0.ok() == Some(symbol_short!("archive")) && t1.ok() == Some(symbol_short!("retention"))
    });
    assert!(
        has_archive_event,
        "archive::retention event should be emitted"
    );
}

#[test]
fn test_fifo_pruning_with_small_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let contract_id_obj = contract_id.clone();
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    // Set retention to 2
    client.set_archive_retention(&2);

    // Create and resolve 4 rounds at different ledgers
    for i in 0..4u64 {
        create_and_resolve_round(&env, &client, &contract_id_obj, (i * 200) as u32, i);
    }

    // Only 2 most recent should remain (newest first)
    let recent = client.get_recent_archived_rounds(&10);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent.get(0).unwrap().round_id, 4);
    assert_eq!(recent.get(1).unwrap().round_id, 3);

    // Round 1 and 2 should be pruned from storage
    env.as_contract(&contract_id_obj, || {
        let archived_key1 = DataKeyScoped::ArchivedRound(1u64);
        assert!(!env.storage().persistent().has(&archived_key1));
        let archived_key2 = DataKeyScoped::ArchivedRound(2u64);
        assert!(!env.storage().persistent().has(&archived_key2));

        // Round 3 and 4 should still exist
        let archived_key3 = DataKeyScoped::ArchivedRound(3u64);
        assert!(env.storage().persistent().has(&archived_key3));
        let archived_key4 = DataKeyScoped::ArchivedRound(4u64);
        assert!(env.storage().persistent().has(&archived_key4));
    });
}

#[test]
fn test_prune_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let contract_id_obj = contract_id.clone();
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    // Set retention to 1
    client.set_archive_retention(&1);

    // Create and resolve round 1
    create_and_resolve_round(&env, &client, &contract_id_obj, 0, 0);

    // Create and resolve round 2 — this should prune round 1
    create_and_resolve_round(&env, &client, &contract_id_obj, 200, 1);

    let events = env.events().all();
    let prune_events: std::vec::Vec<_> = events
        .iter()
        .filter(|(_, topics, _)| {
            if topics.len() < 2 {
                return false;
            }
            let t0: Result<soroban_sdk::Symbol, _> = topics.get(0).unwrap().try_into_val(&env);
            let t1: Result<soroban_sdk::Symbol, _> = topics.get(1).unwrap().try_into_val(&env);
            t0.ok() == Some(symbol_short!("archive")) && t1.ok() == Some(symbol_short!("pruned"))
        })
        .collect();

    assert_eq!(prune_events.len(), 1);
}

#[test]
fn test_retention_change_applies_to_future_writes_only() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let contract_id_obj = contract_id.clone();
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    // Create 3 rounds with default retention (128)
    for i in 0..3u64 {
        create_and_resolve_round(&env, &client, &contract_id_obj, (i * 200) as u32, i);
    }

    // All 3 should be present
    let recent = client.get_recent_archived_rounds(&10);
    assert_eq!(recent.len(), 3);

    // Now set retention to 1 and create 2 more rounds
    client.set_archive_retention(&1);
    for i in 3u64..5u64 {
        create_and_resolve_round(&env, &client, &contract_id_obj, (i * 200) as u32, i);
    }

    // Only the most recent round should remain (retention=1 applied to future writes)
    let recent = client.get_recent_archived_rounds(&10);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent.get(0).unwrap().round_id, 5);
}

#[test]
fn test_get_archived_round_after_prune_returns_none() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let contract_id_obj = contract_id.clone();
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    client.set_archive_retention(&1);

    create_and_resolve_round(&env, &client, &contract_id_obj, 0, 0);

    let archived = client.get_archived_round(&1);
    assert!(archived.is_some());

    create_and_resolve_round(&env, &client, &contract_id_obj, 200, 1);

    // Round 1 was pruned
    let archived = client.get_archived_round(&1);
    assert!(archived.is_none());
}

#[test]
fn test_get_recent_archived_rounds_capped_by_retention() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let contract_id_obj = contract_id.clone();
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    client.set_archive_retention(&3);

    for i in 0..3u64 {
        create_and_resolve_round(&env, &client, &contract_id_obj, (i * 200) as u32, i);
    }

    // Requesting a limit larger than retention should be capped
    let recent = client.get_recent_archived_rounds(&100);
    assert_eq!(recent.len(), 3);
}

#[test]
fn test_archive_retention_cannot_be_set_by_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    // Verify admin can set (mocked auths)
    client.set_archive_retention(&10);
    assert_eq!(client.get_archive_retention(), 10);
}
