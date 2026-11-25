// SPDX-License-Identifier: MIT
//! Tests for round creation and full round lifecycle scenarios.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{
    BetSide, DataKeyCore, DataKeyScoped, OraclePayload, Round, RoundArchiveStatus, RoundMode,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, IntoVal, TryIntoVal,
};

#[test]
fn test_create_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    // Set up admin
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    // Create a round
    let start_price: u128 = 1_5000000; // 1.5 XLM in stroops

    client.create_round(&start_price, &None);

    // Verify the round was created
    let round = client.get_active_round().expect("Round should exist");

    assert_eq!(round.price_start, start_price);
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 0);

    // Verify windows are set correctly (defaults: bet=6, run=12)
    // Note: In tests, current ledger starts at 0
    assert_eq!(round.bet_end_ledger, 6);
    assert_eq!(round.end_ledger, 12);
}

#[test]
fn test_create_round_does_not_clear_live_positions() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    let before = client.get_user_position(&user);
    assert!(before.is_some());

    let result = client.try_create_round(&1_1000000, &None);
    assert_eq!(result, Err(Ok(ContractError::RoundAlreadyActive)));

    let after = client.get_user_position(&user);
    assert_eq!(before, after);
}

#[test]
fn test_create_round_while_active_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    // Set up admin and oracle
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    // Create first round successfully
    let start_price: u128 = 1_5000000;
    client.create_round(&start_price, &None);

    // Capture current active round for later comparison
    let existing_round = client.get_active_round().expect("Round should exist");

    // Attempt to create a second round while first is still active
    let result = client.try_create_round(&2_0000000, &None);
    assert_eq!(result, Err(Ok(ContractError::RoundAlreadyActive)));

    // Ensure the original round remains unchanged
    let round_after = client.get_active_round().expect("Round should still exist");
    assert_eq!(round_after.price_start, existing_round.price_start);
    assert_eq!(round_after.start_ledger, existing_round.start_ledger);
    assert_eq!(round_after.bet_end_ledger, existing_round.bet_end_ledger);
    assert_eq!(round_after.end_ledger, existing_round.end_ledger);
}

#[test]
fn test_create_round_without_init_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    // Try to create round without initializing - should return error
    let result = client.try_create_round(&1_0000000, &None);
    assert_eq!(result, Err(Ok(ContractError::AdminNotSet)));
}

#[test]
fn test_get_active_round_when_none() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    // No round created yet
    let round = client.get_active_round();

    assert_eq!(round, None);
}

#[test]
fn test_full_round_lifecycle() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    // Setup
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    env.mock_all_auths();

    // STEP 1: Initialize contract
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    // STEP 2: Users get initial tokens
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    assert_eq!(client.balance(&alice), 1000_0000000);
    assert_eq!(client.balance(&bob), 1000_0000000);
    assert_eq!(client.balance(&charlie), 1000_0000000);

    // STEP 3: Admin creates a round
    let start_price: u128 = 1_0000000; // 1.0 XLM
    client.create_round(&start_price, &None);

    let round = client.get_active_round().unwrap();
    assert_eq!(round.price_start, start_price);
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 0);

    // STEP 4: Users place bets
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Up);
    client.place_bet(&charlie, &150_0000000, &BetSide::Down);

    // Verify balances deducted
    assert_eq!(client.balance(&alice), 900_0000000);
    assert_eq!(client.balance(&bob), 800_0000000);
    assert_eq!(client.balance(&charlie), 850_0000000);

    // Verify positions recorded
    let alice_pos = client.get_user_position(&alice).unwrap();
    assert_eq!(alice_pos.amount, 100_0000000);
    assert_eq!(alice_pos.side, BetSide::Up);

    // Verify pools updated
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 300_0000000);
    assert_eq!(round.pool_down, 150_0000000);

    // STEP 5: Oracle resolves round (price went UP)
    // Advance ledger to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 12; // Default run window is 12
    });
    let final_price: u128 = 1_5000000; // 1.5 XLM
    client.resolve_round(&OraclePayload {
        price: final_price,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    // Round should be cleared
    assert_eq!(client.get_active_round(), None);

    // STEP 6: Verify pending winnings
    // Alice: 100 + (100/300)*150 = 150
    // Bob: 200 + (200/300)*150 = 300
    // Charlie: 0 (lost)
    assert_eq!(client.get_pending_winnings(&alice), 150_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 300_0000000);
    assert_eq!(client.get_pending_winnings(&charlie), 0);

    // STEP 7: Verify stats updated
    let alice_stats = client.get_user_stats(&alice);
    assert_eq!(alice_stats.total_wins, 1);
    assert_eq!(alice_stats.current_streak, 1);

    let charlie_stats = client.get_user_stats(&charlie);
    assert_eq!(charlie_stats.total_losses, 1);
    assert_eq!(charlie_stats.current_streak, 0);

    // STEP 8: Users claim winnings
    let alice_claimed = client.claim_winnings(&alice);
    let bob_claimed = client.claim_winnings(&bob);

    assert_eq!(alice_claimed, 150_0000000);
    assert_eq!(bob_claimed, 300_0000000);

    // STEP 9: Verify final balances
    assert_eq!(client.balance(&alice), 1050_0000000); // 900 + 150
    assert_eq!(client.balance(&bob), 1100_0000000); // 800 + 300
    assert_eq!(client.balance(&charlie), 850_0000000); // Lost 150

    // STEP 10: Pending winnings cleared
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);
}

#[test]
fn test_multiple_rounds_lifecycle() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);

    // ROUND 1: Alice bets UP and wins
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    env.as_contract(&contract_id, || {
        // alice's position is already stored under DataKeyScoped::Position by place_bet;
        // we only override the round pool totals to inject a simulated losing pool.
        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKeyCore::ActiveRound)
            .unwrap();
        round.pool_up = 100_0000000;
        round.pool_down = 50_0000000;
        env.storage()
            .persistent()
            .set(&DataKeyCore::ActiveRound, &round);
    });

    // Advance ledger to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    let round1 = client.get_active_round().unwrap();
    client.resolve_round(&OraclePayload {
        price: 1_5000000, // UP wins
        timestamp: env.ledger().timestamp(),
        round_id: round1.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });
    client.claim_winnings(&alice);

    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 1);
    assert_eq!(stats.current_streak, 1);

    // ROUND 2: Alice bets DOWN and wins again
    client.create_round(&2_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Down);

    env.as_contract(&contract_id, || {
        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKeyCore::ActiveRound)
            .unwrap();
        round.pool_up = 80_0000000;
        round.pool_down = 100_0000000;
        env.storage()
            .persistent()
            .set(&DataKeyCore::ActiveRound, &round);
    });

    // Advance ledger to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 24; // 12 + 12 for second round
    });
    let round2 = client.get_active_round().unwrap();
    client.resolve_round(&OraclePayload {
        price: 1_5000000, // DOWN wins
        timestamp: env.ledger().timestamp(),
        round_id: round2.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 2);
    assert_eq!(stats.current_streak, 2);
    assert_eq!(stats.best_streak, 2);
}

#[test]
fn test_create_round_fails_without_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    // Initialize with explicit auth
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &oracle).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    // No mocking all auths, so create_round should fail
    let result = client.try_create_round(&1_0000000, &None);
    assert!(result.is_err());
}

#[test]
fn test_place_bet_fails_without_user_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    // Explicitly auth setup calls
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &oracle).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &user,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "mint_initial",
            args: (&user,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.mint_initial(&user);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_round",
            args: (1_0000000u128, Option::<u32>::None).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.create_round(&1_0000000, &None);

    // Attempt to place bet without user auth
    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert!(result.is_err());
}

#[test]
fn test_resolve_round_fails_without_oracle_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &oracle).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_round",
            args: (1_0000000u128, Option::<u32>::None).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Attempt to resolve round without oracle auth
    let result = client.try_resolve_round(&OraclePayload {
        price: 1_1000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });
    assert!(result.is_err());
}

#[test]
fn test_claim_winnings_fails_without_user_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &oracle).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &user,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "mint_initial",
            args: (&user,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.mint_initial(&user);

    // Attempt to claim winnings without user auth
    let result = client.try_claim_winnings(&user);
    assert!(result.is_err());
}

#[test]
fn test_round_created_event_includes_mode() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    // Create Up/Down mode round
    client.create_round(&1_0000000, &Some(0));

    // Verify round created event
    let events = env.events().all();
    let round_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("created"))
    });

    assert!(
        round_event.is_some(),
        "Round created event should be emitted"
    );

    // Resolve and create Precision mode round
    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });

    client.resolve_round(&OraclePayload {
        price: 1_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    client.create_round(&1_0000000, &Some(1));

    // Verify round created event for precision round
    let events = env.events().all();
    let round_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("created"))
    });

    assert!(
        round_event.is_some(),
        "Second round created event should be emitted"
    );
}

#[test]
fn test_mint_initial_event_emitted() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    env.mock_all_auths();

    // Mint initial tokens
    client.mint_initial(&user);

    // Verify mint event was emitted
    let events = env.events().all();
    let mint_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("mint"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("initial"))
    });

    assert!(mint_event.is_some(), "Mint initial event should be emitted");
}

#[test]
fn test_no_mint_event_on_second_call() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    env.mock_all_auths();

    // First mint
    client.mint_initial(&user);

    // Count mint events after first call
    let events = env.events().all();
    let mint_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("mint"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("initial"))
    });
    assert!(mint_event.is_some());

    // Second mint attempt (should return existing balance, no event)
    client.mint_initial(&user);

    // No events should be emitted
    let events = env.events().all();
    let mint_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("mint"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("initial"))
    });
    assert!(mint_event.is_none(), "Should not emit second mint event");
}

// ─── Cancel round tests (Issue #111) ─────────────────────────────────────────

#[test]
fn test_cancel_round_refunds_updown_participants() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Down);

    // Admin cancels the round
    client.cancel_round(&0u32);

    // No active round after cancellation
    assert_eq!(client.get_active_round(), None);

    // Both participants are fully refunded
    assert_eq!(client.get_pending_winnings(&alice), 100_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 200_0000000);
}

#[test]
fn test_cancel_round_refunds_precision_participants() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_0000000, &Some(1)); // Precision mode
    client.place_precision_prediction(&alice, &150_0000000, &2297u128);
    client.place_precision_prediction(&bob, &250_0000000, &2300u128);

    client.cancel_round(&1u32);

    assert_eq!(client.get_active_round(), None);
    assert_eq!(client.get_pending_winnings(&alice), 150_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 250_0000000);
}

#[test]
fn test_cancel_round_marks_round_cancelled() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.create_round(&1_0000000, &None);

    let round_id = client.get_active_round().unwrap().round_id;
    assert!(!client.is_round_cancelled(&round_id));

    client.cancel_round(&0u32);
    assert!(client.is_round_cancelled(&round_id));
}

#[test]
fn test_cancel_round_no_active_round_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    // No active round
    let result = client.try_cancel_round(&0u32);
    assert_eq!(result, Err(Ok(ContractError::RoundNotCancellable)));
}

#[test]
fn test_cancel_round_emits_event() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.create_round(&1_0000000, &None);
    client.cancel_round(&42u32);

    let events = env.events().all();
    let cancel_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("summary"))
    });
    assert!(
        cancel_event.is_some(),
        "Cancellation event should be emitted"
    );
}

#[test]
fn test_cancelled_round_allows_new_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.create_round(&1_0000000, &None);
    client.cancel_round(&0u32);

    // A ledger sequence backs at most one round (oracle payloads bind to
    // `Round.start_ledger`), so advance before creating the replacement.
    env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });

    // A new round can be started after cancellation
    client.create_round(&1_2000000, &None);
    let new_round = client.get_active_round().unwrap();
    assert_eq!(new_round.price_start, 1_2000000);
}

#[test]
fn test_cancel_round_full_refund_equals_pool() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Up);
    client.place_bet(&charlie, &300_0000000, &BetSide::Down);

    let round = client.get_active_round().unwrap();
    let total_pool = round.pool_up + round.pool_down;

    client.cancel_round(&0u32);

    let total_refunded = client.get_pending_winnings(&alice)
        + client.get_pending_winnings(&bob)
        + client.get_pending_winnings(&charlie);

    assert_eq!(
        total_refunded, total_pool,
        "Total refunds must equal total pool"
    );
}

#[test]
fn test_cross_round_mode_alternation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // ────────── ROUND 1: Up/Down mode ──────────
    client.create_round(&1_0000000, &Some(0));
    let round1 = client.get_active_round().unwrap();
    assert_eq!(round1.round_id, 1);
    assert_eq!(round1.mode, RoundMode::UpDown);

    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &50_0000000, &BetSide::Down);

    // No Precision keys should exist for this round
    env.as_contract(&contract_id, || {
        let key = DataKeyScoped::PrecisionPosition(round1.round_id, alice.clone());
        assert!(!env.storage().persistent().has(&key));
    });

    // Resolve — UP wins (price 1.5 > 1.0)
    env.ledger().with_mut(|li| {
        li.sequence_number = round1.end_ledger;
    });
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: round1.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert_eq!(client.get_active_round(), None);

    // Verify Up/Down position keys cleared after resolve
    env.as_contract(&contract_id, || {
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::Position(round1.round_id, alice.clone())));
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::Position(round1.round_id, bob.clone())));
    });

    // Verify archived summary for round 1
    let a1 = client.get_archived_round(&round1.round_id).unwrap();
    assert_eq!(a1.mode, RoundMode::UpDown);
    assert_eq!(a1.status, RoundArchiveStatus::Resolved);
    assert_eq!(a1.round_id, 1);

    // Claim winnings: Alice 100 + (100/100)*50 = 150, Bob loses 50
    assert_eq!(client.claim_winnings(&alice), 150_0000000);
    assert_eq!(client.claim_winnings(&bob), 0);

    // ────────── ROUND 2: Precision mode ──────────
    client.create_round(&2_0000000, &Some(1));
    let round2 = client.get_active_round().unwrap();
    assert_eq!(round2.round_id, 2);
    assert_eq!(round2.mode, RoundMode::Precision);

    client.place_precision_prediction(&alice, &100_0000000, &2297);
    client.place_precision_prediction(&bob, &150_0000000, &2300);

    // No Up/Down position keys should exist for round 2
    env.as_contract(&contract_id, || {
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::Position(round2.round_id, alice.clone())));
    });

    // Resolve at 2298 — Alice closest (diff 1) wins entire pot
    env.ledger().with_mut(|li| {
        li.sequence_number = round2.end_ledger;
    });
    client.resolve_round(&OraclePayload {
        price: 2298,
        timestamp: env.ledger().timestamp(),
        round_id: round2.start_ledger,
        nonce: 2u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert_eq!(client.get_active_round(), None);

    // Verify Precision position keys cleared after resolve
    env.as_contract(&contract_id, || {
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::PrecisionPosition(
                round2.round_id,
                alice.clone()
            )));
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::PrecisionPosition(
                round2.round_id,
                bob.clone()
            )));
    });

    // Verify archived summary for round 2
    let a2 = client.get_archived_round(&round2.round_id).unwrap();
    assert_eq!(a2.mode, RoundMode::Precision);
    assert_eq!(a2.round_id, 2);

    // Claim: Alice wins full pot (100 + 150 = 250)
    assert_eq!(client.claim_winnings(&alice), 250_0000000);
    assert_eq!(client.claim_winnings(&bob), 0);

    // ────────── ROUND 3: Up/Down mode again ──────────
    client.create_round(&3_0000000, &Some(0));
    let round3 = client.get_active_round().unwrap();
    assert_eq!(round3.round_id, 3);
    assert_eq!(round3.mode, RoundMode::UpDown);

    client.place_bet(&alice, &200_0000000, &BetSide::Down);
    client.place_bet(&bob, &100_0000000, &BetSide::Up);

    // No stale Precision keys from round 2 should remain
    env.as_contract(&contract_id, || {
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::PrecisionPosition(
                round2.round_id,
                bob.clone()
            )));
    });

    // Resolve — DOWN wins (price 2.5 < 3.0)
    env.ledger().with_mut(|li| {
        li.sequence_number = round3.end_ledger;
    });
    client.resolve_round(&OraclePayload {
        price: 2_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: round3.start_ledger,
        nonce: 3u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert_eq!(client.get_active_round(), None);

    // Verify Up/Down position keys cleared for round 3
    env.as_contract(&contract_id, || {
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::Position(round3.round_id, alice.clone())));
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::Position(round3.round_id, bob.clone())));
    });

    // Verify archived summary for round 3
    let a3 = client.get_archived_round(&round3.round_id).unwrap();
    assert_eq!(a3.mode, RoundMode::UpDown);
    assert_eq!(a3.status, RoundArchiveStatus::Resolved);
    assert_eq!(a3.round_id, 3);

    // Claim: Alice 200 + (200/200)*100 = 300, Bob loses 100
    assert_eq!(client.claim_winnings(&alice), 300_0000000);
    assert_eq!(client.claim_winnings(&bob), 0);

    // Final balance verification
    // Alice: 1000 - 100(R1) + 150 - 100(R2) + 250 - 200(R3) + 300 = 1300
    assert_eq!(client.balance(&alice), 1300_0000000);
    // Bob:   1000 - 50(R1) + 0 - 150(R2) + 0 - 100(R3) + 0 = 700
    assert_eq!(client.balance(&bob), 700_0000000);
}

// ─── Round templates / create-next keeper ────────────────────────────────────

/// Set → get → clear round-trip, plus the same validation `create_round`
/// itself applies (invalid start price, invalid mode).
#[test]
fn test_round_template_set_get_clear_and_validation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    assert_eq!(client.get_round_template(), None);

    // Invalid start price (0) is rejected, same as create_round.
    let result = client.try_set_round_template(&0u128, &None);
    assert_eq!(result, Err(Ok(ContractError::InvalidStartPrice)));

    // Invalid start price (over max) is rejected.
    let result = client.try_set_round_template(&u128::MAX, &None);
    assert_eq!(result, Err(Ok(ContractError::InvalidStartPrice)));

    // Invalid mode (only 0/1 allowed) is rejected.
    let result = client.try_set_round_template(&1_0000000u128, &Some(2));
    assert_eq!(result, Err(Ok(ContractError::InvalidMode)));

    // No template was persisted by any of the rejected attempts.
    assert_eq!(client.get_round_template(), None);

    // Valid template is stored and readable.
    client.set_round_template(&1_5000000u128, &Some(1));
    let template = client.get_round_template().expect("template must be set");
    assert_eq!(template.start_price, 1_5000000u128);
    assert_eq!(template.mode, Some(1));

    // A later valid call overwrites the previous template.
    client.set_round_template(&2_0000000u128, &None);
    let template = client.get_round_template().expect("template must be set");
    assert_eq!(template.start_price, 2_0000000u128);
    assert_eq!(template.mode, None);

    // Clearing removes it; clearing again with nothing configured errors.
    client.clear_round_template();
    assert_eq!(client.get_round_template(), None);
    let result = client.try_clear_round_template();
    assert_eq!(result, Err(Ok(ContractError::CommitmentNotFound)));
}

/// `create_next_from_template` requires a template to be configured first.
#[test]
fn test_create_next_from_template_requires_template() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    let result = client.try_create_next_from_template();
    assert_eq!(result, Err(Ok(ContractError::CommitmentNotFound)));
    assert_eq!(client.get_active_round(), None);
}

/// Acceptance: overlap is impossible. With a round already active,
/// `create_next_from_template` must fail exactly like `create_round` would,
/// and must not disturb the round that is already running.
#[test]
fn test_create_next_from_template_overlap_impossible() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    client.set_round_template(&3_0000000u128, &Some(0));
    client.create_round(&1_0000000u128, &None);
    let existing_round = client.get_active_round().expect("round should exist");

    let result = client.try_create_next_from_template();
    assert_eq!(result, Err(Ok(ContractError::RoundAlreadyActive)));

    // The active round is exactly the one that already existed — untouched.
    let round_after = client.get_active_round().expect("round should still exist");
    assert_eq!(round_after, existing_round);
}

/// Acceptance: settle → next. After a round resolves normally, the keeper
/// call creates the next round from the template with no manual
/// parameters, and emits both `("round", "created")` and
/// `("template", "applied")`.
#[test]
fn test_create_next_from_template_after_settle() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);

    client.set_round_template(&2_5000000u128, &Some(1));

    client.create_round(&1_0000000u128, &None);
    let round1 = client.get_active_round().unwrap();
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = round1.end_ledger;
    });
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: round1.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });
    assert_eq!(client.get_active_round(), None);

    let next_round_id = client.create_next_from_template();

    // Snapshot events immediately after the mutating call — any further
    // contract invocation (even a read-only query) clears the recorded
    // event log in this soroban-sdk testutils version, so assertions on
    // `env.events()` must happen before any subsequent client call.
    let events = env.events().all();
    let created_event = events.iter().any(|e| {
        let (_c, topics, _d) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("created"))
    });
    let applied_event = events.iter().any(|e| {
        let (_c, topics, _d) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("template"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("applied"))
    });
    assert!(
        created_event,
        "create_next_from_template must emit round/created"
    );
    assert!(
        applied_event,
        "create_next_from_template must emit template/applied"
    );

    assert_eq!(next_round_id, round1.round_id + 1);
    let round2 = client
        .get_active_round()
        .expect("template round must be active");
    assert_eq!(round2.round_id, next_round_id);
    assert_eq!(round2.price_start, 2_5000000u128);
    assert_eq!(round2.mode, RoundMode::Precision);
}

/// Acceptance: cancel → next. After an admin cancellation, the keeper call
/// creates the next round from the template.
#[test]
fn test_create_next_from_template_after_cancel() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    client.set_round_template(&4_0000000u128, &None);

    client.create_round(&1_0000000u128, &None);
    let round1 = client.get_active_round().unwrap();
    client.cancel_round(&1u32);
    assert_eq!(client.get_active_round(), None);

    // A ledger sequence backs at most one round (oracle payloads bind to
    // `Round.start_ledger`), so advance before creating the replacement.
    env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });

    let next_round_id = client.create_next_from_template();
    assert_eq!(next_round_id, round1.round_id + 1);

    let round2 = client
        .get_active_round()
        .expect("template round must be active");
    assert_eq!(round2.round_id, next_round_id);
    assert_eq!(round2.price_start, 4_0000000u128);
    assert_eq!(round2.mode, RoundMode::UpDown);
}

/// A cleared template can no longer be used to create the next round, even
/// after a settle/cancel that would otherwise permit it.
#[test]
fn test_create_next_from_template_after_clear_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    client.set_round_template(&1_0000000u128, &None);
    client.create_round(&1_0000000u128, &None);
    client.cancel_round(&1u32);
    client.clear_round_template();

    let result = client.try_create_next_from_template();
    assert_eq!(result, Err(Ok(ContractError::CommitmentNotFound)));
    assert_eq!(client.get_active_round(), None);
}
