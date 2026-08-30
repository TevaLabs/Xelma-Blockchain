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

    let result = client.try_resolve_round(&oracle_payload(
        &env,
        &contract_id,
        1_5000000,
        0,
        1,
    ));
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
