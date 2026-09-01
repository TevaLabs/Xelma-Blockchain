// SPDX-License-Identifier: MIT
//! Economic attacks — fee gaming, exposure cap boundary abuse.

use super::super::config_helpers::{apply_max_stake, apply_max_user_exposure};
use super::{emit_result, oracle_payload, setup_contract};
use crate::errors::ContractError;
use crate::types::{BetSide, ConfigChangeKind};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

/// Malicious admin schedules a fee change mid-round via the public timelock API,
/// hoping to skim the active pot before settlement.
/// Defense: schedule creates pending config only — active fee remains unset until
/// timelock elapses and `apply_scheduled_changes` runs.
#[test]
fn test_fee_gaming_mid_round_schedule_does_not_affect_settlement() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &60, &BetSide::Up);
    client.place_bet(&bob, &40, &BetSide::Up);

    // Mid-round fee schedule via public API (attacker with admin key)
    client.schedule_protocol_fee_bps(&Some(1_000u32));
    assert!(client
        .get_pending_config_change(&ConfigChangeKind::ProtocolFeeBps)
        .is_some());
    assert_eq!(client.get_protocol_fee_bps(), None);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();

    client.resolve_round(&oracle_payload(&env, &contract_id, 2_000u128, 0, 1));

    let alice_pay = client.get_pending_winnings(&alice);
    let bob_pay = client.get_pending_winnings(&bob);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(alice_pay + bob_pay + treasury_delta, 100);
    assert_eq!(treasury_delta, 0);

    emit_result(
        "fee_gaming_mid_round_schedule",
        "pass",
        "timelock (pending config only)",
        "none — fee requires timelock activation",
        "low",
        false,
    );
}

/// Attacker stakes at the exposure cap boundary then tries one stroop more.
#[test]
fn test_exposure_cap_boundary_attack_blocked() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    apply_max_user_exposure(&env, &client, Some(100_0000000i128));
    apply_max_stake(&env, &client, Some(100_0000000i128));

    let attacker = Address::generate(&env);
    client.mint_initial(&attacker);
    client.create_round(&1_0000000, &None);

    client.place_bet(&attacker, &100_0000000, &BetSide::Up);

    let balance_before = client.balance(&attacker);
    let result = client.try_place_bet(&attacker, &1, &BetSide::Up);
    // Same user, same round → AlreadyBet fires before exposure check
    assert_eq!(result, Err(Ok(ContractError::AlreadyBet)));
    assert_eq!(client.balance(&attacker), balance_before);

    emit_result(
        "exposure_cap_boundary",
        "pass",
        "AlreadyBet",
        "same-user duplicate rejected before exposure check (accepted)",
        "medium",
        false,
    );
}
