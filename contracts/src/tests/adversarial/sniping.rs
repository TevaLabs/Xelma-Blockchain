// SPDX-License-Identifier: MIT
//! Last-ledger sniping — attacker tries to bet after seeing late price info.

use super::super::config_helpers::apply_windows;
use super::{emit_result, setup_contract};
use crate::errors::ContractError;
use crate::types::BetSide;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

/// Attacker snipes at the close-buffer edge in UpDown mode.
/// Defense: close buffer rejects bets before `bet_end_ledger`; balance unchanged.
#[test]
fn test_critical_last_ledger_sniping_updown_blocked() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.sequence_number = 0);

    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    let sniper = Address::generate(&env);
    let early = Address::generate(&env);
    client.mint_initial(&sniper);
    client.mint_initial(&early);

    apply_windows(&env, &client, 6, 12);
    client.set_close_buffer_ledgers(&3);
    client.create_round(&1_0000000, &None);

    // Legitimate bet before close buffer
    env.ledger().with_mut(|li| li.sequence_number = 2);
    client.place_bet(&early, &50_0000000, &BetSide::Up);

    // Sniper tries at close-buffer edge (ledger 3) — blocked
    env.ledger().with_mut(|li| li.sequence_number = 3);
    let balance_before = client.balance(&sniper);
    let result = client.try_place_bet(&sniper, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
    assert_eq!(client.balance(&sniper), balance_before);

    emit_result(
        "last_ledger_sniping_updown",
        "pass",
        "RoundEnded (close buffer)",
        "none when close_buffer configured",
        "medium",
        true,
    );
}

/// Attacker snipes with a precision prediction in the close-buffer window.
#[test]
fn test_last_ledger_sniping_precision_blocked() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.sequence_number = 0);

    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    let sniper = Address::generate(&env);
    client.mint_initial(&sniper);

    apply_windows(&env, &client, 6, 12);
    client.set_close_buffer_ledgers(&2);
    client.create_round(&1_0000000, &Some(1));

    env.ledger().with_mut(|li| li.sequence_number = 4);
    let balance_before = client.balance(&sniper);
    let result = client.try_place_precision_prediction(&sniper, &50_0000000, &2297);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
    assert_eq!(client.balance(&sniper), balance_before);

    emit_result(
        "last_ledger_sniping_precision",
        "pass",
        "RoundEnded (close buffer)",
        "none when close_buffer configured",
        "medium",
        false,
    );
}
