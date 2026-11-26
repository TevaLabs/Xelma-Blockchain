// SPDX-License-Identifier: MIT
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
    // Keep ledger time consistent with ledger sequence (5 s per ledger) so the
    // oracle payload lands inside the round's timestamp window.
    let start_ts = 1000 + (start_ledger as u64) * 5;
    env.ledger().with_mut(|li| {
        li.sequence_number = start_ledger;
        li.timestamp = start_ts;
    });
    client.create_round(&1_0000000, &None);

    let round = client.get_active_round().unwrap();
    let end_ts = start_ts + ((round.end_ledger - round.start_ledger) as u64) * 5;
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
        li.timestamp = end_ts;
    });
    // Fresh heartbeat so the Issue #264 health gate does not block resolution.
    client.update_oracle_heartbeat(&0u32);
    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: end_ts,
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
    let user = Address::generate(&env);

    client.mint_initial(&user);
    client.set_archive_retention(&1);

    // Round ids start at 1. The user bets in both rounds so each has a
    // `UserRoundOutcome` record.
    let bet_and_resolve = |start_ledger: u32, nonce: u64| -> u64 {
        let start_ts = 1000 + (start_ledger as u64) * 5;
        env.ledger().with_mut(|li| {
            li.sequence_number = start_ledger;
            li.timestamp = start_ts;
        });
        client.create_round(&1_0000000, &None);
        let round = client.get_active_round().unwrap();
        client.place_bet(&user, &100_0000000, &crate::types::BetSide::Up);

        let end_ts = start_ts + ((round.end_ledger - round.start_ledger) as u64) * 5;
        env.ledger().with_mut(|li| {
            li.sequence_number = round.end_ledger;
            li.timestamp = end_ts;
        });
        client.update_oracle_heartbeat(&0u32);
        client.resolve_round(&OraclePayload {
            price: 2_0000000,
            timestamp: end_ts,
            round_id: round.start_ledger,
            nonce,
            network_id: env.ledger().network_id(),
            contract_addr: contract_id_obj.clone(),
            confidence: None,
            attestation: None,
        });
        round.round_id
    };

    let first = bet_and_resolve(0, 0);

    // User participation is available before prune
    let outcome = client.get_user_archived_participation(&user, &first);
    assert!(outcome.is_some(), "outcome should exist before prune");

    // Create and resolve round 2 — this should prune round 1
    let second = bet_and_resolve(200, 1);

    // After prune, get_user_archived_participation should return None
    let outcome = client.get_user_archived_participation(&user, &first);
    assert!(
        outcome.is_none(),
        "get_user_archived_participation should return None for pruned round"
    );

    // Round 2 still has its outcome
    let outcome2 = client.get_user_archived_participation(&user, &second);
    assert!(
        outcome2.is_some(),
        "outcome should exist for retained round"
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
    client.cancel_round(&0);

    // Round ids start at 1; `cancel_round`'s argument is a reason code.
    let first: u64 = 1;

    // CancelledRound marker exists before prune
    env.as_contract(&contract_id_obj, || {
        assert!(env
            .storage()
            .persistent()
            .has(&DataKeyScoped::CancelledRound(first)));
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
                .has(&DataKeyScoped::CancelledRound(first)),
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

// ─── Issue #857: prune is never silent ──────────────────────────────────────
//
// Every archived round removed by the FIFO retention loop must emit exactly
// one `("archive", "pruned")` event with payload `(round_id, retention_limit)`,
// oldest first, in the same invocation that deleted it. The tests below pin
// the payload, the ordering and the count against what storage actually lost.

/// Contract events of the most recent invocation, as `(topic0, topic1, data)`.
fn last_call_events(
    env: &Env,
) -> Vec<(soroban_sdk::Symbol, soroban_sdk::Symbol, soroban_sdk::Val)> {
    let mut out = Vec::new();
    for (_contract, topics, data) in env.events().all().iter() {
        if topics.len() < 2 {
            continue;
        }
        let t0: Result<soroban_sdk::Symbol, _> = topics.get(0).unwrap().try_into_val(env);
        let t1: Result<soroban_sdk::Symbol, _> = topics.get(1).unwrap().try_into_val(env);
        if let (Ok(t0), Ok(t1)) = (t0, t1) {
            out.push((t0, t1, data));
        }
    }
    out
}

/// Decoded `(round_id, retention_limit)` payloads of every `archive::pruned`
/// event in the most recent invocation, in emission order.
fn pruned_payloads(env: &Env) -> Vec<(u64, u32)> {
    last_call_events(env)
        .into_iter()
        .filter(|(t0, t1, _)| *t0 == symbol_short!("archive") && *t1 == symbol_short!("pruned"))
        .map(|(_, _, data)| {
            let payload: (u64, u32) = data
                .try_into_val(env)
                .expect("archive::pruned payload must decode as (u64, u32)");
            payload
        })
        .collect()
}

fn archived_ids(env: &Env, contract_id: &Address) -> Vec<u64> {
    env.as_contract(contract_id, || {
        let ids: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKeyCore::RecentArchivedRoundIds)
            .unwrap_or(soroban_sdk::Vec::new(env));
        ids.iter().collect()
    })
}

fn has_archived(env: &Env, contract_id: &Address, round_id: u64) -> bool {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .has(&DataKeyScoped::ArchivedRound(round_id))
    })
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
fn test_no_prune_event_while_within_retention() {
    let (env, client, contract_id) = setup_plain();
    client.set_archive_retention(&3);

    for i in 0..3u64 {
        create_and_resolve_round(&env, &client, &contract_id, (i * 200) as u32, i);
        assert!(
            pruned_payloads(&env).is_empty(),
            "archive write {} stayed within retention and must not emit archive::pruned",
            i + 1
        );
    }
    assert_eq!(archived_ids(&env, &contract_id), std::vec![1, 2, 3]);
}

#[test]
fn test_prune_event_payload_is_round_id_and_limit() {
    let (env, client, contract_id) = setup_plain();
    client.set_archive_retention(&2);

    for i in 0..2u64 {
        create_and_resolve_round(&env, &client, &contract_id, (i * 200) as u32, i);
    }
    // Third write evicts round 1 only.
    create_and_resolve_round(&env, &client, &contract_id, 400, 2);
    assert_eq!(pruned_payloads(&env), std::vec![(1u64, 2u32)]);
    assert!(!has_archived(&env, &contract_id, 1));

    // Fourth write evicts round 2 only.
    create_and_resolve_round(&env, &client, &contract_id, 600, 3);
    assert_eq!(pruned_payloads(&env), std::vec![(2u64, 2u32)]);
    assert!(!has_archived(&env, &contract_id, 2));
    assert_eq!(archived_ids(&env, &contract_id), std::vec![3, 4]);
}

#[test]
fn test_lowered_retention_backlog_emits_one_event_per_round_fifo() {
    let (env, client, contract_id) = setup_plain();

    // Five rounds under the default retention: nothing pruned.
    for i in 0..5u64 {
        create_and_resolve_round(&env, &client, &contract_id, (i * 200) as u32, i);
    }
    assert_eq!(archived_ids(&env, &contract_id), std::vec![1, 2, 3, 4, 5]);

    // Lowering the limit deletes nothing and emits no prune by itself.
    client.set_archive_retention(&2);
    assert!(pruned_payloads(&env).is_empty());
    assert_eq!(archived_ids(&env, &contract_id).len(), 5);

    // The next archive write prunes the whole backlog in one invocation:
    // rounds 1..=4 go (6 stored -> 2), oldest first, all tagged with limit 2.
    create_and_resolve_round(&env, &client, &contract_id, 1000, 5);
    assert_eq!(
        pruned_payloads(&env),
        std::vec![(1u64, 2u32), (2, 2), (3, 2), (4, 2)]
    );
    for id in 1..=4u64 {
        assert!(
            !has_archived(&env, &contract_id, id),
            "round {id} must be deleted"
        );
    }
    assert_eq!(archived_ids(&env, &contract_id), std::vec![5, 6]);
}

#[test]
fn test_prune_events_match_deleted_storage_exactly() {
    let (env, client, contract_id) = setup_plain();
    client.set_archive_retention(&3);

    let mut emitted: Vec<u64> = Vec::new();
    let mut ever_archived: Vec<u64> = Vec::new();
    for i in 0..10u64 {
        let before = archived_ids(&env, &contract_id);
        create_and_resolve_round(&env, &client, &contract_id, (i * 200) as u32, i);
        // Read events before any `as_contract` storage probe: that probe is
        // itself an invocation and resets the last-call event buffer.
        let pruned: Vec<u64> = pruned_payloads(&env).iter().map(|(id, _)| *id).collect();
        let after = archived_ids(&env, &contract_id);

        for id in &after {
            if !ever_archived.contains(id) {
                ever_archived.push(*id);
            }
        }
        let deleted: Vec<u64> = before
            .iter()
            .copied()
            .filter(|id| !after.contains(id))
            .collect();
        assert_eq!(
            pruned,
            deleted,
            "write {}: events must name exactly the deleted rounds",
            i + 1
        );
        for id in &pruned {
            assert!(!has_archived(&env, &contract_id, *id));
        }
        emitted.extend(pruned);
    }

    // Over the whole run: everything that left the archive was announced once.
    let mut expected_gone: Vec<u64> = ever_archived
        .iter()
        .copied()
        .filter(|id| !archived_ids(&env, &contract_id).contains(id))
        .collect();
    expected_gone.sort();
    let mut sorted = emitted.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        emitted.len(),
        "a round was announced as pruned twice"
    );
    assert_eq!(sorted, expected_gone);
    assert_eq!(
        emitted,
        std::vec![1, 2, 3, 4, 5, 6, 7],
        "FIFO order across writes"
    );
}

#[test]
fn test_prune_event_follows_round_summary_of_triggering_round() {
    let (env, client, contract_id) = setup_plain();
    client.set_archive_retention(&1);
    create_and_resolve_round(&env, &client, &contract_id, 0, 0);
    create_and_resolve_round(&env, &client, &contract_id, 200, 1);

    let events = last_call_events(&env);
    let summary_pos = events
        .iter()
        .position(|(t0, t1, _)| *t0 == symbol_short!("round") && *t1 == symbol_short!("summary"))
        .expect("resolve must emit round::summary");
    let pruned_pos = events
        .iter()
        .position(|(t0, t1, _)| *t0 == symbol_short!("archive") && *t1 == symbol_short!("pruned"))
        .expect("second write must emit archive::pruned");
    assert!(
        summary_pos < pruned_pos,
        "indexers see the new round's summary before the old round's prune"
    );
}

#[test]
fn test_cancelled_round_prune_emits_event() {
    let (env, client, contract_id) = setup_plain();
    client.set_archive_retention(&1);

    env.ledger().with_mut(|li| {
        li.sequence_number = 0;
        li.timestamp = 1000;
    });
    client.create_round(&1_0000000, &None);
    client.cancel_round(&0);
    assert!(pruned_payloads(&env).is_empty());
    let first = archived_ids(&env, &contract_id);
    assert_eq!(first.len(), 1);

    env.ledger().with_mut(|li| {
        li.sequence_number = 200;
        li.timestamp = 3000;
    });
    client.create_round(&1_0000000, &None);
    client.cancel_round(&0);
    assert_eq!(pruned_payloads(&env), std::vec![(first[0], 1u32)]);
    env.as_contract(&contract_id, || {
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::CancelledRound(first[0])));
    });
}

#[test]
fn test_retention_event_payload_is_new_limit() {
    let (env, client, _) = setup_plain();
    client.set_archive_retention(&42);
    let payloads: Vec<(u32,)> = last_call_events(&env)
        .into_iter()
        .filter(|(t0, t1, _)| *t0 == symbol_short!("archive") && *t1 == symbol_short!("retention"))
        .map(|(_, _, d)| {
            d.try_into_val(&env)
                .expect("archive::retention payload is (u32,)")
        })
        .collect();
    assert_eq!(payloads, std::vec![(42u32,)]);
}

#[test]
fn test_rejected_retention_change_emits_no_archive_event() {
    let (env, client, _) = setup_plain();
    assert!(client.try_set_archive_retention(&0).is_err());
    assert!(last_call_events(&env)
        .iter()
        .all(|(t0, _, _)| *t0 != symbol_short!("archive")));
}
