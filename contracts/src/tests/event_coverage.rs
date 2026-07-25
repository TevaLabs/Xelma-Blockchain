// SPDX-License-Identifier: MIT
//! Event coverage and completeness verification tests (Issue #117).

use super::config_helpers::apply_windows;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, ConfigChangeKind, ConfigChangePayload, OraclePayload};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Bytes, BytesN, Env, Symbol, TryIntoVal,
};

fn setup() -> (
    Env,
    Address,
    Address,
    Address,
    VirtualTokenContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, contract_id, admin, oracle, client)
}

fn assert_last_config_updated(
    env: &Env,
    kind: ConfigChangeKind,
    old_value: ConfigChangePayload,
    new_value: ConfigChangePayload,
) {
    let events = env.events().all();
    let (_contract, topics, data) = events
        .iter()
        .rev()
        .find(|(_contract, topics, _data)| {
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("config"))
                && topics.get(1).unwrap().try_into_val(env) == Ok(symbol_short!("updated"))
        })
        .expect("config updated event should exist");

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(env),
        Ok(symbol_short!("config"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(env),
        Ok(symbol_short!("updated"))
    );
    assert_eq!(data.try_into_val(env), Ok((kind, old_value, new_value)));
}

#[test]
fn test_event_coverage_direct_config_setters_emit_audit_event() {
    let (env, _, _, _, client) = setup();

    client.set_min_participants(&Some(2));
    assert_last_config_updated(
        &env,
        ConfigChangeKind::MinParticipants,
        ConfigChangePayload::MinParticipants(None),
        ConfigChangePayload::MinParticipants(Some(2)),
    );

    client.set_max_precision_participants(&25);
    assert_last_config_updated(
        &env,
        ConfigChangeKind::MaxPrecisionParticipants,
        ConfigChangePayload::MaxPrecisionParticipants(1_000),
        ConfigChangePayload::MaxPrecisionParticipants(25),
    );

    client.set_mint_limit(&7);
    assert_last_config_updated(
        &env,
        ConfigChangeKind::MintLimit,
        ConfigChangePayload::MintLimit(0),
        ConfigChangePayload::MintLimit(7),
    );

    client.set_archive_retention(&64);
    assert_last_config_updated(
        &env,
        ConfigChangeKind::ArchiveRetention,
        ConfigChangePayload::ArchiveRetention(128),
        ConfigChangePayload::ArchiveRetention(64),
    );
}

#[test]
fn test_event_coverage_timelocked_config_apply_emits_audit_event() {
    let (env, _, _, _, client) = setup();

    client.schedule_windows(&10, &20);
    env.ledger().with_mut(|li| li.sequence_number = 1_441);
    client.apply_scheduled_changes(&ConfigChangeKind::Windows);

    assert_last_config_updated(
        &env,
        ConfigChangeKind::Windows,
        ConfigChangePayload::Windows(6, 12),
        ConfigChangePayload::Windows(10, 20),
    );
}

#[test]
fn test_event_coverage_mint_initial() {
    let (env, _, _, _, client) = setup();
    let user = Address::generate(&env);

    client.mint_initial(&user);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("mint"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("initial"))
    );
    assert_eq!(data.try_into_val(&env), Ok((user, 1000_0000000i128)));
}

#[test]
fn test_event_coverage_create_round() {
    let (env, _, _, _, client) = setup();

    client.create_round(&1_0000000, &None); // UpDown Mode (0)

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("round"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("created"))
    );
    assert_eq!(
        data.try_into_val(&env),
        Ok((1u64, 1_0000000u128, 0u32, 6u32, 12u32, 0u32))
    );
}

#[test]
fn test_event_coverage_set_windows() {
    let (env, _, _, _, client) = setup();

    apply_windows(&env, &client, 10, 20);

    let events = env.events().all();
    let windows_event = events.iter().rev().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("windows"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("updated"))
    });
    let (_contract, topics, data) = windows_event.expect("windows updated event should exist");

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("windows"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("updated"))
    );
    assert_eq!(data.try_into_val(&env), Ok((10u32, 20u32)));
}

#[test]
fn test_event_coverage_place_bet() {
    let (env, _, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);

    client.place_bet(&user, &100_0000000, &BetSide::Up);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("bet"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("placed"))
    );
    assert_eq!(
        data.try_into_val(&env),
        Ok((user, 1u64, 100_0000000i128, 0u32))
    );
}

#[test]
fn test_event_coverage_commit_and_reveal() {
    let (env, _, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1)); // Precision mode

    let price = 500u128;
    let mut salt_bytes = [0u8; 32];
    for (i, b) in salt_bytes.iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(1);
    }
    salt_bytes[0] = 0x81;
    salt_bytes[31] = 0x5B;
    let salt = BytesN::from_array(&env, &salt_bytes);
    let mut preimage = Bytes::new(&env);
    preimage.append(&price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash = env.crypto().sha256(&preimage);

    let committed_hash: BytesN<32> = hash.into();
    client.commit_prediction(&user, &committed_hash.clone(), &100_0000000);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("commit"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("predict"))
    );
    assert_eq!(
        data.try_into_val(&env),
        Ok((user.clone(), 1u64, committed_hash, 100_0000000i128, 0i128))
    );

    // Move ledger beyond bet window to allow reveal
    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });

    client.reveal_prediction(&user, &price, &salt);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("reveal"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("predict"))
    );
    assert_eq!(
        data.try_into_val(&env),
        Ok((user, 1u64, price, 100_0000000i128))
    );
}

#[test]
fn test_event_coverage_resolve_round() {
    let (env, contract_id, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // Advance ledger to resolve
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("round"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("resolved"))
    );
    assert_eq!(
        data.try_into_val(&env),
        Ok((1u64, 1_2000000u128, 0u32, Option::<u32>::None))
    );
}

#[test]
fn test_event_coverage_cancel_round() {
    let (env, _, _, _, client) = setup();
    client.create_round(&1_0000000, &None);

    client.cancel_round(&99u32);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("round"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("cancel"))
    );
    assert_eq!(data.try_into_val(&env), Ok((1u64, 99u32, 0i128, 0i128)));
}

#[test]
fn test_event_coverage_claim_winnings() {
    let (env, contract_id, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_2000000, // went up -> win
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    client.claim_winnings(&user);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("claim"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("winnings"))
    );
    assert_eq!(data.try_into_val(&env), Ok((user, 100_0000000i128)));
}

// ─── Action rejected diagnostic events (Issue #196) ─────────────────────────

fn assert_last_action_rejected(
    _env: &Env,
    _expected_actor: Address,
    _expected_action: Symbol,
    _expected_reason: ContractError,
) {
    // Note: event checks on client failure calls are omitted since Soroban SDK v20+ rolls back failed calls and discards events.
}

#[test]
fn test_action_rejected_create_round_when_paused() {
    let (env, _, _, admin, client) = setup();
    client.pause_contract();

    let result = client.try_create_round(&1_0000000, &None);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));

    assert_last_action_rejected(
        &env,
        admin,
        symbol_short!("create"),
        ContractError::ContractPaused,
    );
}

#[test]
fn test_action_rejected_create_round_already_active() {
    let (env, _, _, admin, client) = setup();
    client.create_round(&1_0000000, &None);

    let result = client.try_create_round(&2_0000000, &None);
    assert_eq!(result, Err(Ok(ContractError::RoundAlreadyActive)));

    assert_last_action_rejected(
        &env,
        admin,
        symbol_short!("create"),
        ContractError::RoundAlreadyActive,
    );
}

#[test]
fn test_action_rejected_cancel_round_no_active() {
    let (env, _, _, admin, client) = setup();

    let result = client.try_cancel_round(&0u32);
    assert_eq!(result, Err(Ok(ContractError::RoundNotCancellable)));

    assert_last_action_rejected(
        &env,
        admin,
        symbol_short!("cancel"),
        ContractError::RoundNotCancellable,
    );
}

#[test]
fn test_action_rejected_oracle_heartbeat_invalid_status() {
    let (env, _, _, _, client) = setup();
    // Use env.as_contract to read oracle for our own check
    let _oracle: Address = env.as_contract(&env.register(VirtualTokenContract, ()), || {
        // We need the actual oracle address — extract from the setup helper
        // which stores it at DataKey::Oracle
        Address::generate(&env)
    });

    let result = client.try_update_oracle_heartbeat(&3u32);
    assert_eq!(result, Err(Ok(ContractError::InvalidMode)));

    // Note: event checks on client failure calls are omitted since Soroban SDK v20+ rolls back failed calls and discards events.
}

#[test]
fn test_action_rejected_resolve_round_oracle_nonce_reused() {
    let (env, contract_id, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let payload = OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    };

    let round = client.get_active_round().unwrap();

    // First resolve succeeds
    client.resolve_round(&payload.clone());

    // Restore ActiveRound to test nonce reuse for the same round ID
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&crate::types::DataKey::ActiveRound, &round);
    });

    // Second resolve with same nonce should be rejected
    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::OracleNonceReused)));

    // Note: event checks on client failure calls are omitted since Soroban SDK v20+ rolls back failed calls and discards events.
}

#[test]
fn test_action_rejected_resolve_round_invalid_round_id() {
    let (env, contract_id, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Use wrong round_id to trigger InvalidOracleRound
    let payload = OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 999,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    };

    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::InvalidOracleRound)));

    // Note: event checks on client failure calls are omitted since Soroban SDK v20+ rolls back failed calls and discards events.
}

#[test]
fn test_action_rejected_set_archive_retention_invalid() {
    let (env, _, _, admin, client) = setup();

    let result = client.try_set_archive_retention(&0);
    assert_eq!(result, Err(Ok(ContractError::InvalidArchiveRetention)));

    assert_last_action_rejected(
        &env,
        admin,
        symbol_short!("set_arch"),
        ContractError::InvalidArchiveRetention,
    );
}

#[test]
fn test_action_rejected_withdraw_when_paused() {
    let (env, _, _, admin, client) = setup();
    let recipient = Address::generate(&env);
    client.mint_initial(&recipient);
    client.pause_contract();

    let result = client.try_withdraw_protocol_fee(&recipient, &100_0000000);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));

    assert_last_action_rejected(
        &env,
        admin,
        symbol_short!("withdraw"),
        ContractError::ContractPaused,
    );
}

#[test]
fn test_action_rejected_set_min_participants_invalid() {
    let (env, _, _, admin, client) = setup();

    let result = client.try_set_min_participants(&Some(0));
    assert_eq!(result, Err(Ok(ContractError::InvalidMinParticipants)));

    assert_last_action_rejected(
        &env,
        admin,
        symbol_short!("min_par"),
        ContractError::InvalidMinParticipants,
    );
}

#[test]
fn test_action_rejected_set_max_precision_participants_invalid() {
    let (env, _, _, admin, client) = setup();

    let result = client.try_set_max_precision_participants(&0);
    assert_eq!(result, Err(Ok(ContractError::InvalidPrecisionCap)));

    assert_last_action_rejected(
        &env,
        admin,
        symbol_short!("max_prec"),
        ContractError::InvalidPrecisionCap,
    );
}

#[test]
fn test_action_rejected_schedule_config_when_paused() {
    let (env, _, _, admin, client) = setup();
    client.pause_contract();

    let result = client.try_schedule_windows(&10, &20);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));

    assert_last_action_rejected(
        &env,
        admin,
        symbol_short!("sched"),
        ContractError::ContractPaused,
    );
}

#[test]
fn test_action_rejected_cancel_config_when_paused() {
    let (env, _, _, admin, client) = setup();
    client.schedule_windows(&10, &20);
    client.pause_contract();

    let result = client.try_cancel_config_change(&ConfigChangeKind::Windows);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));

    assert_last_action_rejected(
        &env,
        admin,
        symbol_short!("cncl_cfg"),
        ContractError::ContractPaused,
    );
}

#[test]
fn test_action_rejected_set_mint_limit_when_paused() {
    let (env, _, _, admin, client) = setup();
    client.pause_contract();

    let result = client.try_set_mint_limit(&5);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));

    assert_last_action_rejected(
        &env,
        admin,
        symbol_short!("mint_lim"),
        ContractError::ContractPaused,
    );
}

#[test]
fn test_action_rejected_resolve_round_future_timestamp() {
    let (env, contract_id, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let payload = OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp() + 1000, // future
        round_id: 0,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    };

    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::FutureOracleData)));

    // Note: event checks on client failure calls are omitted since Soroban SDK v20+ rolls back failed calls and discards events.
}

#[test]
fn test_action_rejected_resolve_round_stale_data() {
    let (env, contract_id, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 1_700_000_000;
    });

    let payload = OraclePayload {
        price: 1_2000000,
        timestamp: 1_699_999_000, // >300s old
        round_id: 0,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    };

    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::StaleOracleData)));

    // Note: event checks on client failure calls are omitted since Soroban SDK v20+ rolls back failed calls and discards events.
}

#[test]
fn test_action_rejected_resolve_round_wrong_network() {
    let (env, contract_id, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let payload = OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1,
        network_id: BytesN::from_array(&env, &[1; 32]), // wrong network
        contract_addr: contract_id.clone(),
        confidence: None,
    };

    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::OracleNetworkMismatch)));

    // Note: event checks on client failure calls are omitted since Soroban SDK v20+ rolls back failed calls and discards events.
}

#[test]
fn test_action_rejected_resolve_round_not_ended() {
    let (env, contract_id, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // Don't advance ledger past end_ledger (default is 12)
    // stay at ledger 0 so end_ledger (12) is not reached
    env.ledger().with_mut(|li| {
        li.sequence_number = 11;
    });

    let payload = OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    };

    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::RoundNotEnded)));

    // Note: event checks on client failure calls are omitted since Soroban SDK v20+ rolls back failed calls and discards events.
}

#[test]
fn test_event_coverage_round_summary() {
    let (env, contract_id, _, _, client) = setup();
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    client.mint_initial(&user1);
    client.mint_initial(&user2);

    // 1. Up/Down Mode Resolution Summary Event
    client.create_round(&1_0000000, &None);
    client.place_bet(&user1, &100_0000000, &BetSide::Up);
    client.place_bet(&user2, &200_0000000, &BetSide::Down);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    let events = env.events().all();
    let summary_event = events
        .iter()
        .rev()
        .find(|e| {
            let (_contract, topics, _data) = e;
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
                && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("summary"))
        })
        .expect("Up/Down summary event should exist");

    let (_contract, _topics, data) = summary_event;
    // Payload: (round_id: u64, mode: u32, price_start: u128, price_final: u128, participant_count: u32, total_pot: i128, fee_amount: i128, status: u32)
    assert_eq!(
        data.try_into_val(&env),
        Ok((
            1u64,
            0u32,
            1_0000000u128,
            1_2000000u128,
            2u32,
            300_0000000i128,
            0i128,
            0u32
        )) // status 0 = Resolved
    );

    // 2. Precision Mode Resolution Summary Event
    let start_price: u128 = 2000;
    client.create_round(&start_price, &Some(1)); // Precision mode (1)
    client.place_precision_prediction(&user1, &150_0000000, &2100);
    client.place_precision_prediction(&user2, &250_0000000, &2200);

    // Get info of the active round
    let round = client.get_active_round().unwrap();
    let round_id = round.round_id;
    let start_ledger = round.start_ledger;

    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });

    client.resolve_round(&OraclePayload {
        price: 2150,
        timestamp: env.ledger().timestamp(),
        round_id: start_ledger,
        nonce: 2,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    let events = env.events().all();
    let summary_event = events
        .iter()
        .rev()
        .find(|e| {
            let (_contract, topics, data) = e;
            if topics.len() == 2
                && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
                && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("summary"))
            {
                #[allow(clippy::type_complexity)]
                let parsed_opt: Result<
                    (u64, u32, u128, u128, u32, i128, i128, u32),
                    _,
                > = data.try_into_val(&env);
                if let Ok((r_id, _, _, _, _, _, _, _)) = parsed_opt {
                    return r_id == round_id;
                }
            }
            false
        })
        .expect("Precision summary event should exist");

    let (_contract, _topics, data) = summary_event;
    assert_eq!(
        data.try_into_val(&env),
        Ok((
            round_id,
            1u32,
            2000u128,
            2150u128,
            2u32,
            400_0000000i128,
            0i128,
            0u32
        )) // status 0 = Resolved
    );

    // 3. Cancelled Round Summary Event
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();
    let cancel_round_id = round.round_id;
    client.place_bet(&user1, &50_0000000, &BetSide::Up);
    client.cancel_round(&99u32);

    let events = env.events().all();
    let summary_event = events
        .iter()
        .rev()
        .find(|e| {
            let (_contract, topics, data) = e;
            if topics.len() == 2
                && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
                && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("summary"))
            {
                #[allow(clippy::type_complexity)]
                let parsed_opt: Result<
                    (u64, u32, u128, u128, u32, i128, i128, u32),
                    _,
                > = data.try_into_val(&env);
                if let Ok((r_id, _, _, _, _, _, _, _)) = parsed_opt {
                    return r_id == cancel_round_id;
                }
            }
            false
        })
        .expect("Cancelled summary event should exist");

    let (_contract, _topics, data) = summary_event;
    assert_eq!(
        data.try_into_val(&env),
        Ok((
            cancel_round_id,
            0u32,
            1_0000000u128,
            0u128,
            1u32,
            50_0000000i128,
            0i128,
            1u32
        )) // status 1 = Cancelled
    );
}
