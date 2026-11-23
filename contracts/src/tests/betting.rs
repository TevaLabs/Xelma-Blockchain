// SPDX-License-Identifier: MIT
//! Tests for bet placement and validation.

use super::config_helpers::{apply_max_stake, apply_max_user_exposure};
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::BetSide;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, TryIntoVal,
};

#[test]
fn test_place_bet_zero_amount() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);

    // Try to bet 0 amount - should return error
    let result = client.try_place_bet(&user, &0, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::InvalidBetAmount)));
}

#[test]
fn test_place_bet_negative_amount() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);

    // Try to bet negative amount - should return error
    let result = client.try_place_bet(&user, &-100, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::InvalidBetAmount)));
}

#[test]
fn test_place_bet_no_active_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Try to bet without active round - should return error
    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

#[test]
fn test_place_bet_after_round_ended() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 0;
    });

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create round (default bet window is 6 ledgers)
    client.create_round(&1_0000000, &None);

    // Advance ledger past bet window (bet closes at ledger 6)
    env.ledger().with_mut(|li| {
        li.sequence_number = 6;
    });

    // Try to bet after bet window closed - should return error
    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
}

#[test]
fn test_place_bet_insufficient_balance() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user); // Has 1000 vXLM
    client.create_round(&1_0000000, &None);

    // Try to bet more than balance - should return error
    let result = client.try_place_bet(&user, &2000_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::InsufficientBalance)));
}

#[test]
fn test_place_bet_twice_same_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);

    // First bet succeeds
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // Second bet should fail with error
    let result = client.try_place_bet(&user, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::AlreadyBet)));
}

#[test]
fn test_get_user_position_no_bet() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    // No position should return None
    let position = client.get_user_position(&user);
    assert_eq!(position, None);
}

#[test]
fn test_bet_placed_event_payload() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);

    // Place bet
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // Verify bet placed event was emitted
    let events = env.events().all();
    let bet_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("bet"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("placed"))
    });

    assert!(bet_event.is_some(), "Bet placed event should be emitted");
}

#[test]
fn test_multiple_bets_emit_separate_events() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user1);
    client.mint_initial(&user2);
    client.mint_initial(&user3);
    client.create_round(&1_0000000, &None);

    // Place multiple bets
    client.place_bet(&user1, &100_0000000, &BetSide::Up);
    let events = env.events().all();
    let bet_event = events.iter().any(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("bet"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("placed"))
    });
    assert!(bet_event, "First bet should emit event");

    client.place_bet(&user2, &150_0000000, &BetSide::Down);
    let events = env.events().all();
    let bet_event = events.iter().any(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("bet"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("placed"))
    });
    assert!(bet_event, "Second bet should emit event");

    client.place_bet(&user3, &200_0000000, &BetSide::Up);
    let events = env.events().all();
    let bet_event = events.iter().any(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("bet"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("placed"))
    });
    assert!(bet_event, "Third bet should emit event");
}

// ─── Economic controls tests (Issue #113) ─────────────────────────────────────

#[test]
fn test_bet_exceeds_max_stake_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Set max stake to 50
    apply_max_stake(&env, &client, Some(50_0000000i128));
    client.create_round(&1_0000000, &None);

    // Exactly at cap — should succeed
    client.place_bet(&user, &50_0000000, &BetSide::Up);

    // Over cap — should fail
    let user2 = Address::generate(&env);
    client.mint_initial(&user2);
    let result = client.try_place_bet(&user2, &51_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::StakeExceedsMax)));
}

#[test]
fn test_bet_at_max_stake_boundary_succeeds() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    apply_max_stake(&env, &client, Some(100_0000000i128));
    client.create_round(&1_0000000, &None);

    // Exactly at cap — must succeed
    client.place_bet(&user, &100_0000000, &BetSide::Down);
    assert_eq!(client.balance(&user), 900_0000000);
}

#[test]
fn test_bet_no_max_stake_cap_disabled() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Set cap then disable it
    apply_max_stake(&env, &client, Some(50_0000000i128));
    apply_max_stake(&env, &client, None);

    client.create_round(&1_0000000, &None);

    // Should succeed — cap is disabled
    client.place_bet(&user, &500_0000000, &BetSide::Up);
    assert_eq!(client.balance(&user), 500_0000000);
}

#[test]
fn test_exposure_cap_exceeded_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    apply_max_user_exposure(&env, &client, Some(80_0000000i128));
    client.create_round(&1_0000000, &None);

    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::ExposureCapExceeded)));
}

#[test]
fn test_exposure_cap_at_boundary_succeeds() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    apply_max_user_exposure(&env, &client, Some(100_0000000i128));
    client.create_round(&1_0000000, &None);

    // Exactly at cap — must succeed
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(client.balance(&user), 900_0000000);
}

#[test]
fn test_get_max_stake_returns_configured_value() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    assert_eq!(client.get_max_stake(), None);
    apply_max_stake(&env, &client, Some(200_0000000i128));
    assert_eq!(client.get_max_stake(), Some(200_0000000i128));
    apply_max_stake(&env, &client, None);
    assert_eq!(client.get_max_stake(), None);
}

#[test]
fn test_get_round_pool_stats_no_active_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    assert_eq!(client.get_round_pool_stats(), None);
}

#[test]
fn test_get_round_pool_stats_empty_updown_pool() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000000, &None);

    let stats = client.get_round_pool_stats().expect("active round stats");
    assert_eq!(stats.total_up_stake, 0);
    assert_eq!(stats.total_down_stake, 0);
    assert_eq!(stats.up_participant_count, 0);
    assert_eq!(stats.down_participant_count, 0);
    assert_eq!(stats.up_stake_ratio_bps, 0);
    assert_eq!(stats.down_stake_ratio_bps, 0);
    assert_eq!(stats.precision_total_stake, 0);
    assert_eq!(stats.precision_participant_count, 0);
}

#[test]
fn test_get_round_pool_stats_partial_updown_pool() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Up);

    let stats = client.get_round_pool_stats().expect("active round stats");
    assert_eq!(stats.total_up_stake, 300_0000000);
    assert_eq!(stats.total_down_stake, 0);
    assert_eq!(stats.up_participant_count, 2);
    assert_eq!(stats.down_participant_count, 0);
    assert_eq!(stats.up_stake_ratio_bps, 10_000);
    assert_eq!(stats.down_stake_ratio_bps, 0);
}

#[test]
fn test_get_round_pool_stats_full_updown_pool() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&carol);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Down);
    client.place_bet(&carol, &300_0000000, &BetSide::Down);

    let stats = client.get_round_pool_stats().expect("active round stats");
    let round = client.get_active_round().expect("active round");
    assert_eq!(stats.total_up_stake, round.pool_up);
    assert_eq!(stats.total_down_stake, round.pool_down);
    assert_eq!(stats.up_participant_count, 1);
    assert_eq!(stats.down_participant_count, 2);
    assert_eq!(stats.up_stake_ratio_bps, 1_666);
    assert_eq!(stats.down_stake_ratio_bps, 8_333);
}

#[test]
fn test_get_round_pool_stats_precision_pool() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.create_round(&1_0000000, &Some(1));
    client.place_precision_prediction(&alice, &125_0000000, &1_1000000);
    client.commit_prediction(
        &bob,
        &soroban_sdk::BytesN::from_array(&env, &[7; 32]),
        &75_0000000,
    );

    let stats = client.get_round_pool_stats().expect("active round stats");
    assert_eq!(stats.total_up_stake, 0);
    assert_eq!(stats.total_down_stake, 0);
    assert_eq!(stats.up_participant_count, 0);
    assert_eq!(stats.down_participant_count, 0);
    assert_eq!(stats.up_stake_ratio_bps, 0);
    assert_eq!(stats.down_stake_ratio_bps, 0);
    assert_eq!(stats.precision_total_stake, 200_0000000);
    assert_eq!(stats.precision_participant_count, 2);
    assert_eq!(stats.precision_prediction_count, 1);
    assert_eq!(stats.precision_commitment_count, 1);
    assert_eq!(stats.precision_revealed_count, 0);
}

// ─── Issue #797: Precision predictions in the reveal-only window ─────────────
//
// A Precision round has three phases by ledger:
//
//   [start, bet_end - close_buffer)   bet window — predictions accepted
//   [bet_end - close_buffer, bet_end) soft close — `BettingClosed`
//                                     (only when a close buffer is set)
//   [bet_end, end)                    reveal-only window — `RoundEnded`
//
// Inside the reveal-only window `reveal_prediction` is the only submission
// accepted. Every new-stake route (`place_precision_prediction`, its
// `predict_price` alias and `commit_prediction`) must be rejected and must
// leave storage exactly as it was.

/// Nonzero commitment hash: passes the zero-hash guard, so a rejection can
/// only come from the window check.
fn nonzero_commitment(env: &Env) -> soroban_sdk::BytesN<32> {
    soroban_sdk::BytesN::from_array(env, &[7u8; 32])
}

/// Every ledger of the reveal-only window rejects every new-stake route with
/// `RoundEnded`, for both a newcomer and a user who already predicted, and
/// no rejected attempt writes to storage.
#[test]
fn test_precision_prediction_rejected_across_reveal_only_window_without_mutation() {
    use super::ledger_snapshot::assert_ledger_unchanged;

    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let early = Address::generate(&env);
    let late = Address::generate(&env);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&early);
    client.mint_initial(&late);
    client.create_round(&1_0000000, &Some(1));

    let round = client.get_active_round().expect("round must be active");
    assert!(
        round.end_ledger > round.bet_end_ledger + 1,
        "test needs a reveal window of at least two ledgers"
    );

    // Last bet-window ledger: a prediction is still accepted.
    env.ledger()
        .with_mut(|li| li.sequence_number = round.bet_end_ledger - 1);
    client.place_precision_prediction(&early, &100_0000000, &1_2000000);

    let late_balance = client.balance(&late);
    let early_balance = client.balance(&early);
    let stats_before = client.get_round_pool_stats().expect("active round stats");

    for ledger in round.bet_end_ledger..round.end_ledger {
        env.ledger().with_mut(|li| li.sequence_number = ledger);

        for user in [&late, &early] {
            let r = assert_ledger_unchanged(&env, "place_precision_prediction", || {
                client.try_place_precision_prediction(user, &50_0000000, &1_1000000)
            });
            assert_eq!(r, Err(Ok(ContractError::RoundEnded)), "ledger {ledger}");

            let r = assert_ledger_unchanged(&env, "predict_price", || {
                client.try_predict_price(user, &1_1000000, &50_0000000)
            });
            assert_eq!(r, Err(Ok(ContractError::RoundEnded)), "ledger {ledger}");

            let r = assert_ledger_unchanged(&env, "commit_prediction", || {
                client.try_commit_prediction(user, &nonzero_commitment(&env), &50_0000000)
            });
            assert_eq!(r, Err(Ok(ContractError::RoundEnded)), "ledger {ledger}");
        }
    }

    // Observable state matches: no stake moved, no position or commitment
    // was written, and the round is untouched.
    assert_eq!(client.balance(&late), late_balance);
    assert_eq!(client.balance(&early), early_balance);
    assert!(client.get_user_precision_prediction(&late).is_none());
    let early_prediction = client
        .get_user_precision_prediction(&early)
        .expect("in-window prediction must survive");
    assert_eq!(early_prediction.predicted_price, 1_2000000);
    assert_eq!(early_prediction.amount, 100_0000000);
    assert_eq!(client.get_round_pool_stats(), Some(stats_before));
    assert_eq!(client.get_active_round(), Some(round));
}

/// With a close buffer configured, the soft close (`BettingClosed`) only
/// covers the buffer ledgers before `bet_end_ledger`. From `bet_end_ledger`
/// on, the hard reveal-only rejection (`RoundEnded`) takes precedence.
#[test]
fn test_precision_soft_close_then_reveal_only_window_error_precedence() {
    use super::ledger_snapshot::assert_ledger_unchanged;

    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user);
    client.set_close_buffer_ledgers(&2u32);
    client.create_round(&1_0000000, &Some(1));

    let round = client.get_active_round().expect("round must be active");
    let soft_close = round.bet_end_ledger - 2;

    let cases = [
        (soft_close, ContractError::BettingClosed),
        (round.bet_end_ledger - 1, ContractError::BettingClosed),
        (round.bet_end_ledger, ContractError::RoundEnded),
        (round.end_ledger - 1, ContractError::RoundEnded),
    ];
    for (ledger, expected) in cases {
        env.ledger().with_mut(|li| li.sequence_number = ledger);

        let r = assert_ledger_unchanged(&env, "place_precision_prediction", || {
            client.try_place_precision_prediction(&user, &50_0000000, &1_1000000)
        });
        assert_eq!(r, Err(Ok(expected)), "place at ledger {ledger}");

        let r = assert_ledger_unchanged(&env, "commit_prediction", || {
            client.try_commit_prediction(&user, &nonzero_commitment(&env), &50_0000000)
        });
        assert_eq!(r, Err(Ok(expected)), "commit at ledger {ledger}");
    }

    assert!(client.get_user_precision_prediction(&user).is_none());
    assert_eq!(client.get_active_round(), Some(round));
}
