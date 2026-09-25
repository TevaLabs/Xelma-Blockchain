// SPDX-License-Identifier: MIT
//! Lifecycle attacks — double-claim, mode confusion.

use super::{emit_result, oracle_payload, setup_contract};
use crate::errors::ContractError;
use crate::types::BetSide;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

/// Attacker calls `claim_winnings` twice to double-spend pending payouts.
/// Defense: second call is idempotent and returns 0.
#[test]
fn test_double_claim_attack_idempotent() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let winner = Address::generate(&env);
    let loser = Address::generate(&env);
    client.mint_initial(&winner);
    client.mint_initial(&loser);

    client.create_round(&1_0000000, &None);
    client.place_bet(&winner, &100_0000000, &BetSide::Up);
    client.place_bet(&loser, &100_0000000, &BetSide::Down);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
        li.timestamp = 100;
    });

    client.resolve_round(&oracle_payload(&env, &contract_id, 1_5000000, 0, 1));

    let pending = client.get_pending_winnings(&winner);
    assert!(pending > 0);

    let balance_before = client.balance(&winner);
    let first_claim = client.claim_winnings(&winner);
    assert_eq!(first_claim, pending);
    assert_eq!(client.get_pending_winnings(&winner), 0);

    let second_claim = client.claim_winnings(&winner);
    assert_eq!(second_claim, 0);
    assert_eq!(client.balance(&winner), balance_before + first_claim);

    emit_result(
        "double_claim_attack",
        "pass",
        "idempotent zero payout",
        "none",
        "high",
        false,
    );
}

/// Attacker uses UpDown bet entrypoint in a Precision round.
#[test]
fn test_mode_confusion_updown_in_precision_blocked() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    let attacker = Address::generate(&env);
    client.mint_initial(&attacker);
    client.create_round(&1_0000000, &Some(1));

    let balance_before = client.balance(&attacker);
    let result = client.try_place_bet(&attacker, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::WrongModeForPrediction)));
    assert_eq!(client.balance(&attacker), balance_before);

    emit_result(
        "mode_confusion_updown_in_precision",
        "pass",
        "WrongModeForPrediction",
        "none",
        "medium",
        false,
    );
}
