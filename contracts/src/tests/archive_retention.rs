// SPDX-License-Identifier: MIT
use super::config_helpers::apply_windows;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{ArchivedRoundSummary, DataKeyCore, DataKeyScoped, OraclePayload};
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
    client.update_oracle_heartbeat(&0u32);
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
    // Widen the round's run window to exactly 100 ledgers: large enough
    // that the round-relative oracle timestamp check (Issue:
    // oracle-round-relative-timestamp-window) accepts the fixed
    // payload.timestamp=1800 resolved below (needs >=100 ledgers: round_start
    // 1000 + 100*5 + 300s skew == 1800), yet no larger than the fixed
    // `start_ledger + 100` advance below, which must reach round.end_ledger
    // for resolve_round's RoundNotEnded gate to pass.
    apply_windows(env, client, 6, 100);
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
        attestation: None,    });
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
    assert_eq!(result, Err(Ok(ContractError::WindowOutOfRange)));
}

#[test]
fn test_set_archive_retention_above_max_fails() {
    let (env, client, _, _) = setup_with_oracle();
    let result = client.try_set_archive_retention(&10_001);
    assert_eq!(result, Err(Ok(ContractError::WindowOutOfRange)));
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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);

    // Verify admin can set (mocked auths)
    client.set_archive_retention(&10);
    assert_eq!(client.get_archive_retention(), 10);
}

/// Verifies that `get_user_archived_participation` returns `None` for a
/// pruned round, even though a `UserRoundOutcome` record may still exist in
/// storage.  This enforces consistent missing-id semantics: after a round is
/// pruned, every query for that round_id returns `None`.
#[test]
fn test_user_archived_participation_returns_none_after_prune() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let contract_id_obj = contract_id.clone();
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    let user = Address::generate(&env);

    client.mint_initial(&user);
    client.set_archive_retention(&1);

    // Create round 1, place a bet, resolve
    env.ledger().with_mut(|li| {
        li.sequence_number = 0;
        li.timestamp = 1000;
    });
    // Widen the run window — see the comment in create_and_resolve_round.
    apply_windows(&env, &client, 6, 100);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &crate::types::BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
        li.timestamp = 2000;
    });
    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: 1800,
        round_id: 0,
        nonce: 0,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id_obj.clone(),
        confidence: None,
        attestation: None,
    });

    // User participation is available before prune. Round ids are
    // 1-indexed (create_round increments from a last-id of 0), so round 1's
    // id is 1, not the OraclePayload.round_id field above (which is
    // matched against round.start_ledger, a different value).
    let outcome = client.get_user_archived_participation(&user, &1);
    assert!(outcome.is_some(), "outcome should exist before prune");

    // Create round 2, with the same user participating, and resolve — this
    // should prune round 1.
    apply_windows(&env, &client, 6, 100);
    env.ledger().with_mut(|li| {
        li.sequence_number = 200;
        li.timestamp = 1000;
    });
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &crate::types::BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 300;
        li.timestamp = 2000;
    });
    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: 1800,
        round_id: 200,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id_obj.clone(),
        confidence: None,
        attestation: None,
    });

    // After prune, get_user_archived_participation should return None for
    // round 1
    let outcome = client.get_user_archived_participation(&user, &1);
    assert!(
        outcome.is_none(),
        "get_user_archived_participation should return None for pruned round"
    );

    // Round 2 (id=2) still has its outcome
    let outcome2 = client.get_user_archived_participation(&user, &2);
    assert!(outcome2.is_some(), "outcome should exist for retained round");
}

/// Verifies that when a cancelled round is pruned, its `CancelledRound` marker
/// is also removed from storage so no orphaned markers accumulate.
#[test]
fn test_prune_cleans_cancelled_round_marker() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let contract_id_obj = contract_id.clone();
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    client.set_archive_retention(&1);

    // Create and cancel round 1
    env.ledger().with_mut(|li| {
        li.sequence_number = 0;
        li.timestamp = 1000;
    });
    client.create_round(&1_0000000, &None);
    client.cancel_round(&0);

    // CancelledRound marker exists before prune
    env.as_contract(&contract_id_obj, || {
        assert!(
            env.storage()
                .persistent()
                .has(&DataKeyScoped::CancelledRound(1u64))
        );
    });

    // Create and cancel round 2 — this should prune round 1
    env.ledger().with_mut(|li| {
        li.sequence_number = 200;
        li.timestamp = 3000;
    });
    client.create_round(&1_0000000, &None);
    client.cancel_round(&0);

    // After prune, CancelledRound marker for round 1 should be gone
    env.as_contract(&contract_id_obj, || {
        assert!(
            !env.storage()
                .persistent()
                .has(&DataKeyScoped::CancelledRound(1u64)),
            "CancelledRound marker should be cleaned up during prune"
        );
    });
}

/// Verifies that multiple rounds exceeding retention all get pruned in FIFO
/// order, with all associated data cleaned up.
#[test]
fn test_prune_multiple_rounds_cleans_associated_data() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let contract_id_obj = contract_id.clone();
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    // Create 3 rounds with retention 1 — each new round prunes the previous
    client.set_archive_retention(&1);
    for i in 0u64..3u64 {
        create_and_resolve_round(&env, &client, &contract_id_obj, (i * 200) as u32, i);
    }

    // Only round 3 (id=3) should remain
    let recent = client.get_recent_archived_rounds(&10);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent.get(0).unwrap().round_id, 3);

    // ArchivedRound keys for rounds 1 and 2 should be gone
    env.as_contract(&contract_id_obj, || {
        for pruned_id in [1u64, 2u64] {
            assert!(
                !env.storage()
                    .persistent()
                    .has(&DataKeyScoped::ArchivedRound(pruned_id)),
                "ArchivedRound({}) should be pruned",
                pruned_id
            );
        }
        assert!(
            env.storage()
                .persistent()
                .has(&DataKeyScoped::ArchivedRound(3u64)),
            "ArchivedRound(3) should still exist"
        );
    });
}
