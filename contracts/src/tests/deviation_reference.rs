// SPDX-License-Identifier: MIT
//! Tests for TWAP / reference-price oracle deviation guardrails (Issue #266).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{ConfigChangeKind, DeviationReferenceMode, OraclePayload};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, IntoVal,
};

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    (client, contract_id, admin, oracle)
}

fn schedule_and_apply_deviation_bps(
    env: &Env,
    client: &VirtualTokenContractClient,
    bps: Option<u32>,
) {
    client.set_oracle_max_deviation_bps(&bps);
    env.ledger().with_mut(|li| {
        li.sequence_number += crate::common::CONFIG_TIMELOCK_LEDGERS + 1;
    });
    client.apply_scheduled_changes(&ConfigChangeKind::OracleMaxDeviationBps);
}

fn payload_for(
    env: &Env,
    contract_id: &Address,
    price: u128,
    round_id: u32,
    nonce: u64,
) -> OraclePayload {
    OraclePayload {
        price,
        timestamp: env.ledger().timestamp(),
        round_id,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    }
}

#[test]
fn test_deviation_reference_mode_defaults_to_start_price() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup(&env);

    assert_eq!(
        client.get_deviation_ref_mode(),
        DeviationReferenceMode::StartPrice
    );
}

#[test]
fn test_start_price_mode_unchanged_behaviour_with_deviation_bps() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);
    client.mint_initial(&user);

    schedule_and_apply_deviation_bps(&env, &client, Some(500)); // 5%
    client.create_round(&1_000_0000, &None);
    let round = client.get_active_round().unwrap();

    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });

    // 10% deviation from start price (1_000_0000) — exceeds 5% bound.
    let result = client.try_resolve_round(&payload_for(
        &env,
        &contract_id,
        1_100_0000,
        round.start_ledger,
        1,
    ));
    assert_eq!(result, Err(Ok(ContractError::OracleDeviationExceeded)));
}

#[test]
fn test_twap_mode_rejects_settlement_with_insufficient_samples() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.set_deviation_ref_mode(&DeviationReferenceMode::Twap, &3u32);
    schedule_and_apply_deviation_bps(&env, &client, Some(500));
    client.create_round(&1_000_0000, &None);
    let round = client.get_active_round().unwrap();

    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });

    // No TWAP samples recorded yet (fresh contract) — must reject, not
    // silently fall back to an unbounded/zero reference price.
    let result = client.try_resolve_round(&payload_for(
        &env,
        &contract_id,
        1_000_0000,
        round.start_ledger,
        1,
    ));
    assert_eq!(result, Err(Ok(ContractError::WindowOutOfRange)));
}

#[test]
fn test_twap_mode_settles_once_window_is_filled() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);
    client.mint_initial(&user);

    // Window of 2 — build up samples via StartPrice-mode settlements first
    // (recording happens regardless of active reference mode).
    client.create_round(&1_000_0000, &None);
    let round1 = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round1.end_ledger;
    });
    client.resolve_round(&payload_for(
        &env,
        &contract_id,
        1_000_0000,
        round1.start_ledger,
        1,
    ));

    client.create_round(&1_000_0000, &None);
    let round2 = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round2.end_ledger;
    });
    client.resolve_round(&payload_for(
        &env,
        &contract_id,
        1_010_0000,
        round2.start_ledger,
        2,
    ));

    // Now 2 samples recorded: [1_000_0000, 1_010_0000], TWAP avg ~1_005_0000.
    let samples = client.get_twap_samples();
    assert_eq!(samples.len(), 2);

    client.set_deviation_ref_mode(&DeviationReferenceMode::Twap, &2u32);
    schedule_and_apply_deviation_bps(&env, &client, Some(10_000)); // 100%, generous bound
    client.create_round(&1_000_0000, &None);
    let round3 = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round3.end_ledger;
    });
    // Settles fine against the TWAP average, not the round's own start price.
    client.resolve_round(&payload_for(
        &env,
        &contract_id,
        1_005_0000,
        round3.start_ledger,
        3,
    ));
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_set_deviation_ref_mode_validates_twap_window_bounds() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup(&env);

    let result = client.try_set_deviation_ref_mode(&DeviationReferenceMode::Twap, &0u32);
    assert_eq!(result, Err(Ok(ContractError::WindowOutOfRange)));

    let result = client.try_set_deviation_ref_mode(&DeviationReferenceMode::Twap, &1000u32);
    assert_eq!(result, Err(Ok(ContractError::WindowOutOfRange)));

    // StartPrice mode ignores window_samples entirely — any value accepted.
    client.set_deviation_ref_mode(&DeviationReferenceMode::StartPrice, &0u32);
}

#[test]
fn test_deviation_reference_mode_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let attacker = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_deviation_ref_mode",
            args: (DeviationReferenceMode::Twap, 5u32).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_set_deviation_ref_mode(&DeviationReferenceMode::Twap, &5u32);
    assert!(result.is_err());
}
