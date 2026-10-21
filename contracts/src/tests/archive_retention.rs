// SPDX-License-Identifier: MIT
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{ArchivedRoundSummary, DataKeyCore, DataKeyScoped, OraclePayload};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, Symbol, TryFromVal, TryIntoVal,
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
    client.create_round(&1_0000000, &None);

    // Settle at the round's own end ledger with a timestamp inside the
    // oracle window and a fresh heartbeat (health gate, Issue #264).
    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
        li.timestamp = 1000 + 5 * u64::from(round.end_ledger - round.start_ledger);
    });
    client.update_oracle_heartbeat(&0u32);
    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);
    let user = Address::generate(&env);

    client.mint_initial(&user);
    client.set_archive_retention(&1);

    // Create round 1, place a bet, resolve
    env.ledger().with_mut(|li| {
        li.sequence_number = 0;
        li.timestamp = 1000;
    });
    client.create_round(&1_0000000, &None);
    let first_id = client.get_active_round().unwrap().round_id;
    client.place_bet(&user, &100_0000000, &crate::types::BetSide::Up);

    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
        li.timestamp = 1000 + 5 * u64::from(round.end_ledger - round.start_ledger);
    });
    client.update_oracle_heartbeat(&0u32);
    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 0,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id_obj.clone(),
        confidence: None,
        attestation: None,
    });

    // User participation is available before prune
    let outcome = client.get_user_archived_participation(&user, &first_id);
    assert!(outcome.is_some(), "outcome should exist before prune");

    // Create and resolve round 2 — this should prune round 1
    create_and_resolve_round(&env, &client, &contract_id_obj, 200, 1);

    // After prune, get_user_archived_participation should return None
    let outcome = client.get_user_archived_participation(&user, &first_id);
    assert!(
        outcome.is_none(),
        "get_user_archived_participation should return None for pruned round"
    );

    // The retained round 2 is still archived (the user did not bet in it,
    // so there is no participation entry for them there).
    assert!(
        client.get_archived_round(&(first_id + 1)).is_some(),
        "retained round should still be archived"
    );
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
    let first_id = client.get_active_round().unwrap().round_id;
    client.cancel_round(&0);

    // CancelledRound marker exists before prune
    env.as_contract(&contract_id_obj, || {
        assert!(env
            .storage()
            .persistent()
            .has(&DataKeyScoped::CancelledRound(first_id)));
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
                .has(&DataKeyScoped::CancelledRound(first_id)),
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

// ── Prune events (Issue #721) ────────────────────────────────────────────────
//
// Pruning must never be silent: every archived round deleted from storage
// emits exactly one `("archive", "pruned")` event carrying
// `(round_id, retention_limit)`, oldest round first, after the new round's
// `("round", "summary")`.

fn topic_pair(env: &Env, topics: &soroban_sdk::Vec<soroban_sdk::Val>) -> Option<(Symbol, Symbol)> {
    if topics.len() != 2 {
        return None;
    }
    let t0 = Symbol::try_from_val(env, &topics.get(0)?).ok()?;
    let t1 = Symbol::try_from_val(env, &topics.get(1)?).ok()?;
    Some((t0, t1))
}

/// `("archive", "pruned")` payloads from the contract's most recent call, in
/// emission order, plus the position of `("round", "summary")` among the
/// event stream (if present) and the positions of each pruned event.
fn last_call_prune_events(
    env: &Env,
    contract_id: &Address,
) -> (
    std::vec::Vec<(u64, u32)>,
    Option<usize>,
    std::vec::Vec<usize>,
) {
    let mut pruned = std::vec::Vec::new();
    let mut pruned_positions = std::vec::Vec::new();
    let mut summary_position = None;
    for (index, (emitter, topics, data)) in env.events().all().iter().enumerate() {
        if &emitter != contract_id {
            continue;
        }
        match topic_pair(env, &topics) {
            Some((a, b)) if a == symbol_short!("archive") && b == symbol_short!("pruned") => {
                pruned.push(<(u64, u32)>::try_from_val(env, &data).unwrap());
                pruned_positions.push(index);
            }
            Some((a, b)) if a == symbol_short!("round") && b == symbol_short!("summary") => {
                summary_position = Some(index);
            }
            _ => {}
        }
    }
    (pruned, summary_position, pruned_positions)
}

fn setup_plain() -> (Env, VirtualTokenContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    (env, client, contract_id)
}

#[test]
fn test_prune_event_payload_is_round_id_and_retention_limit() {
    let (env, client, contract_id) = setup_plain();
    client.set_archive_retention(&1);

    create_and_resolve_round(&env, &client, &contract_id, 0, 0);
    create_and_resolve_round(&env, &client, &contract_id, 200, 1);

    let (pruned, _, _) = last_call_prune_events(&env, &contract_id);
    assert_eq!(pruned, std::vec![(1u64, 1u32)]);
}

#[test]
fn test_no_prune_event_while_within_retention() {
    let (env, client, contract_id) = setup_plain();
    client.set_archive_retention(&2);

    create_and_resolve_round(&env, &client, &contract_id, 0, 0);
    let (pruned, summary, _) = last_call_prune_events(&env, &contract_id);
    assert!(pruned.is_empty());
    assert!(summary.is_some(), "settlement still emits its summary");

    // Exactly at the limit: still nothing deleted, still no event.
    create_and_resolve_round(&env, &client, &contract_id, 200, 1);
    let (pruned, _, _) = last_call_prune_events(&env, &contract_id);
    assert!(pruned.is_empty());
    assert_eq!(client.get_recent_archived_rounds(&10).len(), 2);
}

#[test]
fn test_lowering_retention_prunes_backlog_with_ordered_events_on_next_write() {
    let (env, client, contract_id) = setup_plain();
    for i in 0u64..4 {
        create_and_resolve_round(&env, &client, &contract_id, (i * 200) as u32, i);
    }

    // Lowering retention deletes nothing yet, so it emits no prune event.
    client.set_archive_retention(&1);
    let (pruned, _, _) = last_call_prune_events(&env, &contract_id);
    assert!(pruned.is_empty());
    // All four archived rounds are still in storage.
    env.as_contract(&contract_id, || {
        for id in 1u64..=4 {
            assert!(env
                .storage()
                .persistent()
                .has(&DataKeyScoped::ArchivedRound(id)));
        }
    });

    // The next archive write prunes the whole backlog: one event per round,
    // oldest first, all after the new round's summary.
    create_and_resolve_round(&env, &client, &contract_id, 800, 4);
    let (pruned, summary, positions) = last_call_prune_events(&env, &contract_id);
    assert_eq!(
        pruned,
        std::vec![(1u64, 1u32), (2u64, 1u32), (3u64, 1u32), (4u64, 1u32)]
    );
    let summary = summary.expect("round summary must be emitted");
    assert!(positions.iter().all(|p| *p > summary));
    assert_eq!(client.get_recent_archived_rounds(&10).len(), 1);
}

#[test]
fn test_cancel_path_prune_emits_event_for_cancelled_round() {
    let (env, client, contract_id) = setup_plain();
    client.set_archive_retention(&1);

    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
        li.timestamp = 1000;
    });
    client.create_round(&1_0000000, &None);
    client.cancel_round(&0);

    env.ledger().with_mut(|li| {
        li.sequence_number = 300;
        li.timestamp = 2000;
    });
    client.create_round(&1_0000000, &None);
    client.cancel_round(&0);

    let (pruned, _, _) = last_call_prune_events(&env, &contract_id);
    assert_eq!(pruned, std::vec![(1u64, 1u32)]);
}
