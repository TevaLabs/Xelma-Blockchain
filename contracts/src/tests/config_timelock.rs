//! Tests for timelocked critical config changes (governance safety).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{ConfigChangeKind, ConfigChangePayload};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, TryIntoVal,
};

/// Must match `CONFIG_TIMELOCK_LEDGERS` in contract.rs.
const CONFIG_TIMELOCK_LEDGERS: u32 = 1440;

fn setup() -> (Env, Address, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, admin, client)
}

fn advance_to_activation(env: &Env, activation_ledger: u32) {
    env.ledger().with_mut(|li| {
        li.sequence_number = activation_ledger;
    });
}

#[test]
fn test_schedule_windows_does_not_apply_immediately() {
    let (env, _, client) = setup();

    client.schedule_windows(&10, &20);

    let pending = client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .expect("pending windows change should exist");
    assert_eq!(pending.payload, ConfigChangePayload::Windows(10, 20));
    assert_eq!(
        pending.activation_ledger,
        env.ledger()
            .sequence()
            .saturating_add(CONFIG_TIMELOCK_LEDGERS)
    );

    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().expect("round should exist");
    // Defaults (6, 12) still active — scheduled change not applied yet.
    assert_eq!(round.bet_end_ledger, 106);
    assert_eq!(round.end_ledger, 112);
}

#[test]
fn test_apply_scheduled_windows_happy_path() {
    let (env, _, client) = setup();

    client.schedule_windows(&10, &20);
    let pending = client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .unwrap();

    let early = client.try_apply_scheduled_changes(&ConfigChangeKind::Windows);
    assert_eq!(early, Err(Ok(ContractError::RoundNotEnded)));

    advance_to_activation(&env, pending.activation_ledger);
    client.apply_scheduled_changes(&ConfigChangeKind::Windows);

    assert!(client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .is_none());

    env.ledger().with_mut(|li| {
        li.sequence_number = 200;
    });
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();
    assert_eq!(round.bet_end_ledger, 210);
    assert_eq!(round.end_ledger, 220);
}

#[test]
fn test_cancel_scheduled_change_removes_pending() {
    let (_, _, client) = setup();

    client.schedule_windows(&8, &16);
    assert!(client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .is_some());

    client.cancel_config_change(&ConfigChangeKind::Windows);

    assert!(client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .is_none());

    let apply_after_cancel = client.try_apply_scheduled_changes(&ConfigChangeKind::Windows);
    assert_eq!(
        apply_after_cancel,
        Err(Ok(ContractError::CommitmentNotFound))
    );
}

#[test]
fn test_cancel_config_change_emits_event() {
    let (env, _, client) = setup();

    client.schedule_windows(&8, &16);
    client.cancel_config_change(&ConfigChangeKind::Windows);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("config"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("cancelled"))
    );
    assert_eq!(
        data.try_into_val(&env),
        Ok((ConfigChangeKind::Windows, env.ledger().sequence()))
    );
}

#[test]
fn test_schedule_windows_emits_scheduled_event() {
    let (env, _, client) = setup();

    client.schedule_windows(&10, &20);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("config"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("scheduled"))
    );
    let pending = client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .unwrap();
    assert_eq!(
        data.try_into_val(&env),
        Ok((ConfigChangeKind::Windows, pending.activation_ledger))
    );
}

#[test]
fn test_apply_scheduled_windows_emits_applied_event() {
    let (env, _, client) = setup();

    client.schedule_windows(&10, &20);
    let pending = client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .unwrap();
    advance_to_activation(&env, pending.activation_ledger);

    client.apply_scheduled_changes(&ConfigChangeKind::Windows);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("config"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("applied"))
    );
    assert_eq!(
        data.try_into_val(&env),
        Ok((ConfigChangeKind::Windows, pending.activation_ledger))
    );
}

#[test]
fn test_schedule_windows_rejects_duplicate_pending() {
    let (_, _, client) = setup();

    client.schedule_windows(&10, &20);
    let result = client.try_schedule_windows(&12, &24);
    assert_eq!(result, Err(Ok(ContractError::RoundAlreadyActive)));
}

#[test]
fn test_schedule_windows_validates_before_storing() {
    let (_, _, client) = setup();

    let result = client.try_schedule_windows(&0, &12);
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));
    assert!(client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .is_none());
}

#[test]
fn test_apply_scheduled_max_stake_happy_path() {
    let (env, _, client) = setup();

    client.schedule_max_stake(&Some(500_0000000));
    let pending = client
        .get_pending_config_change(&ConfigChangeKind::MaxStake)
        .unwrap();
    assert!(client.get_max_stake().is_none());

    advance_to_activation(&env, pending.activation_ledger);
    client.apply_scheduled_changes(&ConfigChangeKind::MaxStake);

    assert_eq!(client.get_max_stake(), Some(500_0000000));
}

#[test]
fn test_apply_scheduled_oracle_stale_threshold_happy_path() {
    let (env, _, client) = setup();

    client.schedule_oracle_stale_threshold(&7200);
    let pending = client
        .get_pending_config_change(&ConfigChangeKind::OracleStaleThreshold)
        .unwrap();
    assert_eq!(client.get_oracle_stale_threshold(), 3600);

    advance_to_activation(&env, pending.activation_ledger);
    client.apply_scheduled_changes(&ConfigChangeKind::OracleStaleThreshold);

    assert_eq!(client.get_oracle_stale_threshold(), 7200);
}

#[test]
fn test_apply_scheduled_oracle_max_deviation_bps_happy_path() {
    let (env, _, client) = setup();

    client.schedule_oracle_deviation_bps(&Some(500));
    let pending = client
        .get_pending_config_change(&ConfigChangeKind::OracleMaxDeviationBps)
        .unwrap();
    assert!(client.get_oracle_max_deviation_bps().is_none());

    advance_to_activation(&env, pending.activation_ledger);
    client.apply_scheduled_changes(&ConfigChangeKind::OracleMaxDeviationBps);

    assert_eq!(client.get_oracle_max_deviation_bps(), Some(500));
}

#[test]
fn test_apply_scheduled_max_user_exposure_early_apply_fails() {
    let (_, _, client) = setup();

    client.schedule_max_user_exposure(&Some(100_0000000));
    let result = client.try_apply_scheduled_changes(&ConfigChangeKind::MaxUserRoundExposure);
    assert_eq!(result, Err(Ok(ContractError::RoundNotEnded)));
    assert!(client.get_max_user_exposure().is_none());
}

#[test]
fn test_cancel_after_activation_fails() {
    let (env, _, client) = setup();

    client.schedule_windows(&8, &16);
    let pending = client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .unwrap();
    advance_to_activation(&env, pending.activation_ledger);

    let result = client.try_cancel_config_change(&ConfigChangeKind::Windows);
    assert_eq!(result, Err(Ok(ContractError::RoundNotCancellable)));
    assert!(client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .is_some());
}

#[test]
fn test_set_windows_schedules_without_immediate_apply() {
    let (env, _, client) = setup();

    client.set_windows(&10, &20);
    assert!(client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .is_some());

    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();
    assert_eq!(round.bet_end_ledger, 106);
    assert_eq!(round.end_ledger, 112);
}

// ============================================================================
// PROTOCOL FEE TIMELOCK TESTS (Issue #162)
// ============================================================================
//
// The protocol fee is a critical config setting (impacts payout fairness for
// every competitive settlement), so it goes through the same timelock pattern
// as `OracleMaxDeviationBps` etc.:
//   schedule -> activation_ledger = now + CONFIG_TIMELOCK_LEDGERS ->
//   apply_scheduled_changes (any caller) -> storage flipped -> event emitted.


#[test]
fn test_protocol_fee_timelock_full_cycle() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Initially unset.
    assert_eq!(client.get_protocol_fee_bps(), None);

    // Schedule 500 bps.
    client.schedule_protocol_fee_bps(&Some(500u32));

    // Before activation: still unset; reads should NOT expose the pending value.
    assert_eq!(client.get_protocol_fee_bps(), None);
    let pending = client
        .get_pending_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps)
        .unwrap();
    assert_eq!(pending.activation_ledger, pending.scheduled_at_ledger + 1440);

    // Advancing the ledger by an insufficient amount must NOT activate.
    env.ledger().with_mut(|li| li.sequence_number += 1439);
    let err = client.try_apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );
    assert!(err.is_err(), "apply before activation_ledger must fail");

    // Exactly at activation_ledger must succeed.
    env.ledger().with_mut(|li| li.sequence_number += 1);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);
    assert_eq!(client.get_protocol_fee_bps(), Some(500u32));

    // Pending entry must be cleared post-apply.
    assert!(
        client
            .get_pending_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps)
            .is_none()
    );

    // fee_bps_set event must be present.
    let ev_count = env
        .events()
        .all()
        .iter()
        .filter(|e| {
            let (_contract, topics, _data) = e;
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("protocol"))
                && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("fee_bps_set"))
        })
        .count();
    assert!(ev_count >= 1, "fee_bps_set event must be emitted on apply");
}

#[test]
fn test_protocol_fee_timelock_admin_can_cancel_before_activation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    client.schedule_protocol_fee_bps(&Some(100u32));
    // cancel before activation must be admin-only and clear the pending entry.
    client.cancel_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps);
    assert!(
        client
            .get_pending_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps)
            .is_none()
    );
    assert_eq!(client.get_protocol_fee_bps(), None);
}

#[test]
fn test_protocol_fee_timelock_disable_via_none() {
    // Setting fee back to None must remove the storage key (FormatOption::None on
    // the storage side) so the _read_protocol_fee_bps helper returns None and
    // the contract resumes byte-for-byte pre-#162 behaviour.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    client.schedule_protocol_fee_bps(&Some(500u32));
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );
    assert_eq!(client.get_protocol_fee_bps(), Some(500u32));

    client.schedule_protocol_fee_bps(&None);
    env.ledger().with_mut(|li| li.sequence_number = 10_000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );
    assert_eq!(client.get_protocol_fee_bps(), None,
        "re-issuing with None must remove the storage key entirely");
}
