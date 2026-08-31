// SPDX-License-Identifier: MIT
//! Oracle griefing — heartbeat manipulation, nonce replay, cross-round replay.

use super::super::config_helpers::apply_oracle_stale_threshold;
use super::{emit_result, oracle_payload, setup_contract};
use crate::errors::ContractError;
use crate::types::BetSide;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

/// Attacker (or compromised oracle service) marks heartbeat offline to block settlement.
/// Defense: `OracleNotLive` — admin may arm override as recovery path.
#[test]
fn test_oracle_heartbeat_griefing_blocks_settlement() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    client.update_oracle_heartbeat(&2u32);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 200;
    });

    let result = client.try_resolve_round(&oracle_payload(&env, &contract_id, 1_5000000, 0, 1));
    assert_eq!(result, Err(Ok(ContractError::OracleNotLive)));
    assert!(client.get_active_round().is_some());

    emit_result(
        "oracle_heartbeat_griefing",
        "pass",
        "OracleNotLive",
        "admin heartbeat override available",
        "high",
        false,
    );
}

/// Attacker replays a previously consumed oracle nonce.
#[test]
fn test_oracle_nonce_replay_blocked() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 100;
    });

    let payload = oracle_payload(&env, &contract_id, 1_5000000, 0, 42);
    client.resolve_round(&payload);

    client.create_round(&1_5000000, &None);
    client.place_bet(&user, &50_0000000, &BetSide::Down);

    env.ledger().with_mut(|li| {
        li.sequence_number = 24;
        li.timestamp = 200;
    });

    let replay = client.try_resolve_round(&payload);
    assert_eq!(replay, Err(Ok(ContractError::OracleNonceReused)));

    emit_result(
        "oracle_nonce_replay",
        "pass",
        "OracleNonceReused",
        "none",
        "high",
        false,
    );
}

/// Attacker submits a valid payload from a prior round against the active round.
#[test]
fn test_cross_round_payload_replay_blocked() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 100;
    });

    let stale_payload = oracle_payload(&env, &contract_id, 1_5000000, 0, 1);
    client.resolve_round(&stale_payload);

    client.create_round(&1_5000000, &None);
    client.place_bet(&user, &50_0000000, &BetSide::Down);

    env.ledger().with_mut(|li| {
        li.sequence_number = 24;
        li.timestamp = 200;
    });

    let replay = client.try_resolve_round(&stale_payload);
    assert_eq!(replay, Err(Ok(ContractError::InvalidOracleRound)));

    emit_result(
        "cross_round_payload_replay",
        "pass",
        "InvalidOracleRound",
        "none",
        "high",
        false,
    );
}

/// Attacker submits stale oracle timestamps to force premature or delayed settlement.
#[test]
fn test_stale_oracle_timestamp_griefing_blocked() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    apply_oracle_stale_threshold(&env, &client, 300);

    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1000;
    });

    let mut payload = oracle_payload(&env, &contract_id, 1_5000000, 0, 1);
    payload.timestamp = 600;

    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::StaleOracleData)));
    assert!(client.get_active_round().is_some());

    emit_result(
        "stale_oracle_timestamp_griefing",
        "pass",
        "StaleOracleData",
        "none",
        "medium",
        false,
    );
}

/// Same-ledger round recreation is refused, closing the cross-round replay path.
///
/// `OraclePayload.round_id` binds to `Round.start_ledger`, which is
/// `env.ledger().sequence()` at creation time and is therefore not unique on its
/// own. A round can be cancelled and a replacement created within the same
/// ledger, giving both rounds an identical `start_ledger` but distinct monotonic
/// `Round.round_id` values — so a payload signed for the first round also
/// satisfies the binding check for the second.
///
/// The nonce guard does not close this gap: consumed nonces are keyed by
/// `ConsumedOracleNonce(round.round_id, nonce)` — the monotonic id — so a nonce
/// burned in the first round is unconsumed in the second.
///
/// A replacement round sharing a `start_ledger` could not be settled
/// unambiguously at all, so `create_round` refuses it up front with
/// `RoundStartLedgerReused`.
#[test]
fn test_same_ledger_round_recreation_rejected() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    let user = Address::generate(&env);
    client.mint_initial(&user);

    env.ledger().with_mut(|li| {
        li.sequence_number = 5;
        li.timestamp = 100;
    });
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    let first_round = client.get_active_round().unwrap();
    assert_eq!(first_round.start_ledger, 5);

    // Cancelling frees the active-round slot, but the ledger sequence stays claimed.
    client.cancel_round(&0u32);
    assert!(client.get_active_round().is_none());

    let recreated = client.try_create_round(&1_0000000, &None);
    assert_eq!(
        recreated,
        Err(Ok(ContractError::RoundStartLedgerReused)),
        "a ledger sequence must back at most one round"
    );

    emit_result(
        "same_ledger_round_recreation",
        "pass",
        "RoundStartLedgerReused",
        "none",
        "high",
        true,
    );
}

/// Recovery path: once the ledger advances, a replacement round is created
/// normally and settles with its own payload.
///
/// The binding defense must not strand an operator who cancelled a bad round —
/// it only forces the replacement onto a distinct `start_ledger`.
#[test]
fn test_round_recreation_allowed_after_ledger_advances() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let user = Address::generate(&env);
    client.mint_initial(&user);

    env.ledger().with_mut(|li| {
        li.sequence_number = 5;
        li.timestamp = 100;
    });
    client.create_round(&1_0000000, &None);
    let first_round = client.get_active_round().unwrap();
    client.cancel_round(&0u32);

    env.ledger().with_mut(|li| {
        li.sequence_number = 6;
        li.timestamp = 120;
    });
    client.create_round(&1_2000000, &None);

    let second_round = client.get_active_round().unwrap();
    assert_eq!(second_round.price_start, 1_2000000);
    assert_eq!(second_round.start_ledger, 6);
    assert_ne!(second_round.round_id, first_round.round_id);

    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 20;
        li.timestamp = 200;
    });

    let fresh = oracle_payload(&env, &contract_id, 1_5000000, second_round.start_ledger, 7);
    client.resolve_round(&fresh);
    assert!(client.get_active_round().is_none());
}

/// A payload bound to a settled round must not settle the round that follows it.
///
/// End-to-end consequence of the `start_ledger` uniqueness invariant: the
/// replacement round necessarily has a different `start_ledger`, so the stale
/// payload fails the binding check.
#[test]
fn test_payload_from_previous_round_rejected_after_recreation() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let user = Address::generate(&env);
    client.mint_initial(&user);

    env.ledger().with_mut(|li| {
        li.sequence_number = 5;
        li.timestamp = 100;
    });
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    let first_start_ledger = client.get_active_round().unwrap().start_ledger;

    // The operator signs a payload bound to the first round.
    let stale_payload = oracle_payload(&env, &contract_id, 1_5000000, first_start_ledger, 1);

    client.cancel_round(&0u32);

    env.ledger().with_mut(|li| {
        li.sequence_number = 6;
        li.timestamp = 120;
    });
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &50_0000000, &BetSide::Down);

    let second_round = client.get_active_round().unwrap();
    assert_ne!(second_round.start_ledger, first_start_ledger);

    env.ledger().with_mut(|li| {
        li.sequence_number = 20;
        li.timestamp = 200;
    });

    let replay = client.try_resolve_round(&stale_payload);
    assert_eq!(replay, Err(Ok(ContractError::InvalidOracleRound)));
    assert!(client.get_active_round().is_some());

    emit_result(
        "previous_round_payload_replay",
        "pass",
        "InvalidOracleRound",
        "none",
        "high",
        true,
    );
}
