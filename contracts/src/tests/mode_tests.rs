//! Tests for round mode flag and separate prediction storage.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload, RoundMode};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, TryIntoVal,
};

#[test]
fn test_create_round_default_mode() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Create round without specifying mode (should default to UpDown)
    client.create_round(&1_0000000, &None);

    let round = client.get_active_round().unwrap();
    assert_eq!(round.mode, RoundMode::UpDown);
}

#[test]
fn test_create_round_updown_mode_explicit() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Create round with explicit Up/Down mode (0)
    client.create_round(&1_0000000, &Some(0));

    let round = client.get_active_round().unwrap();
    assert_eq!(round.mode, RoundMode::UpDown);
}

#[test]
fn test_create_round_precision_mode() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Create round with Precision mode (1)
    client.create_round(&1_0000000, &Some(1));

    let round = client.get_active_round().unwrap();
    assert_eq!(round.mode, RoundMode::Precision);
}

#[test]
fn test_create_round_invalid_mode() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Try to create round with invalid mode (2)
    let result = client.try_create_round(&1_0000000, &Some(2));
    assert_eq!(result, Err(Ok(ContractError::InvalidMode)));
}

#[test]
fn test_place_bet_on_updown_mode() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create Up/Down round
    client.create_round(&1_0000000, &Some(0));

    // Place bet should work
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    let position = client.get_user_position(&user).unwrap();
    assert_eq!(position.amount, 100_0000000);
    assert_eq!(position.side, BetSide::Up);
}

#[test]
fn test_place_bet_on_precision_mode_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create Precision round
    client.create_round(&1_0000000, &Some(1));

    // place_bet should fail on Precision mode
    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::WrongModeForPrediction)));
}

#[test]
fn test_place_precision_prediction_on_precision_mode() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create Precision round
    client.create_round(&1_0000000, &Some(1));

    // Place precision prediction (predicted price: 0.2297 scaled to 4 decimals = 2297)
    client.place_precision_prediction(&user, &100_0000000, &2297);

    // Verify the prediction was stored
    let prediction = client.get_user_precision_prediction(&user).unwrap();
    assert_eq!(prediction.amount, 100_0000000);
    assert_eq!(prediction.predicted_price, 2297);

    // Verify balance was deducted
    assert_eq!(client.balance(&user), 900_0000000);
}

#[test]
fn test_place_precision_prediction_on_updown_mode_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create Up/Down round
    client.create_round(&1_0000000, &Some(0));

    // place_precision_prediction should fail on Up/Down mode
    let result = client.try_place_precision_prediction(&user, &100_0000000, &2297);
    assert_eq!(result, Err(Ok(ContractError::WrongModeForPrediction)));
}

#[test]
fn test_precision_prediction_already_bet() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create Precision round
    client.create_round(&1_0000000, &Some(1));

    // First prediction succeeds
    client.place_precision_prediction(&user, &100_0000000, &2297);

    // Second prediction should fail
    let result = client.try_place_precision_prediction(&user, &50_0000000, &2500);
    assert_eq!(result, Err(Ok(ContractError::AlreadyBet)));
}

#[test]
fn test_get_precision_predictions() {
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

    // Create Precision round
    client.create_round(&1_0000000, &Some(1));

    // Multiple users place predictions
    client.place_precision_prediction(&alice, &100_0000000, &2297);
    client.place_precision_prediction(&bob, &150_0000000, &2500);

    // Get all predictions
    let predictions = client.get_precision_predictions();
    assert_eq!(predictions.len(), 2);

    // Verify both predictions exist regardless of order
    let mut found_alice = false;
    let mut found_bob = false;

    for pred in predictions.iter() {
        if pred.user == alice {
            assert_eq!(pred.amount, 100_0000000);
            assert_eq!(pred.predicted_price, 2297);
            found_alice = true;
        } else if pred.user == bob {
            assert_eq!(pred.amount, 150_0000000);
            assert_eq!(pred.predicted_price, 2500);
            found_bob = true;
        }
    }

    assert!(found_alice, "Alice's prediction not found");
    assert!(found_bob, "Bob's prediction not found");
}

#[test]
fn test_get_updown_positions() {
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

    // Create Up/Down round
    client.create_round(&1_0000000, &Some(0));

    // Multiple users place bets
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &150_0000000, &BetSide::Down);

    // Get all positions
    let positions = client.get_updown_positions();
    assert_eq!(positions.len(), 2);

    // Verify alice's position
    let alice_pos = positions.get(alice.clone()).unwrap();
    assert_eq!(alice_pos.amount, 100_0000000);
    assert_eq!(alice_pos.side, BetSide::Up);

    // Verify bob's position
    let bob_pos = positions.get(bob.clone()).unwrap();
    assert_eq!(bob_pos.amount, 150_0000000);
    assert_eq!(bob_pos.side, BetSide::Down);
}

#[test]
fn test_precision_insufficient_balance() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user); // Has 1000 vXLM

    // Create Precision round
    client.create_round(&1_0000000, &Some(1));

    // Try to bet more than balance
    let result = client.try_place_precision_prediction(&user, &2000_0000000, &2297);
    assert_eq!(result, Err(Ok(ContractError::InsufficientBalance)));
}

#[test]
fn test_precision_round_ended() {
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

    // Create Precision round (default bet window is 6 ledgers)
    client.create_round(&1_0000000, &Some(1));

    // Advance ledger past bet window (bet closes at ledger 6)
    env.ledger().with_mut(|li| {
        li.sequence_number = 6;
    });

    // Try to place prediction after bet window closed
    let result = client.try_place_precision_prediction(&user, &100_0000000, &2297);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
}

#[test]
fn test_precision_invalid_amount() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create Precision round
    client.create_round(&1_0000000, &Some(1));

    // Try to bet 0 amount
    let result = client.try_place_precision_prediction(&user, &0, &2297);
    assert_eq!(result, Err(Ok(ContractError::InvalidBetAmount)));

    // Try to bet negative amount
    let result = client.try_place_precision_prediction(&user, &-100, &2297);
    assert_eq!(result, Err(Ok(ContractError::InvalidBetAmount)));
}

#[test]
fn test_predict_price_alias() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create Precision round
    client.create_round(&1_0000000, &Some(1));

    // Use predict_price function (alias with different parameter order)
    client.predict_price(&user, &2297, &100_0000000);

    // Verify the prediction was stored
    let prediction = client.get_user_precision_prediction(&user).unwrap();
    assert_eq!(prediction.amount, 100_0000000);
    assert_eq!(prediction.predicted_price, 2297);

    // Verify balance was deducted
    assert_eq!(client.balance(&user), 900_0000000);
}

#[test]
fn test_predict_price_valid_scales() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Test various valid price scales (4 decimal places)
    let test_cases = [
        1u128,        // 0.0001 XLM
        2297u128,     // 0.2297 XLM
        10000u128,    // 1.0000 XLM
        50000u128,    // 5.0000 XLM
        99999999u128, // 9999.9999 XLM (max valid)
    ];

    for price in test_cases.iter() {
        let user = Address::generate(&env);
        client.mint_initial(&user);

        // If a previous round is still active, resolve it before creating a new one
        if let Some(round) = client.get_active_round() {
            env.ledger().with_mut(|li| {
                li.sequence_number = round.end_ledger;
            });
            client.resolve_round(&OraclePayload {
                price: round.price_start,
                timestamp: env.ledger().timestamp(),
                round_id: round.start_ledger,
                nonce: 1u64,
            });
        }

        // Create new Precision round for each test case

        client.create_round(&1_0000000, &Some(1));

        // Should succeed with valid price scale
        client.predict_price(&user, price, &100_0000000);

        let prediction = client.get_user_precision_prediction(&user).unwrap();
        assert_eq!(prediction.predicted_price, *price);

        // Clean up for next iteration
        env.ledger().with_mut(|li| {
            li.sequence_number += 20;
        });
    }
}

#[test]
fn test_predict_price_invalid_scale() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create Precision round
    client.create_round(&1_0000000, &Some(1));

    // Try to predict with price exceeding max scale (> 9999.9999)
    let result = client.try_predict_price(&user, &100_000_000, &100_0000000);
    assert_eq!(result, Err(Ok(ContractError::InvalidPriceScale)));

    // Try with extremely large value
    let result = client.try_predict_price(&user, &999_999_999_999, &100_0000000);
    assert_eq!(result, Err(Ok(ContractError::InvalidPriceScale)));
}

#[test]
fn test_predict_price_event_emission() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create Precision round at ledger 0
    client.create_round(&1_0000000, &Some(1));
    let _round = client.get_active_round().unwrap();

    // Place prediction
    client.predict_price(&user, &2297, &100_0000000);

    // Verify event was emitted
    let events = env.events().all();
    assert!(!events.is_empty());

    // Find the prediction event
    // Event format: (contract_address, topics_vec, data_val)
    // Topics should contain ("predict", "price")
    let prediction_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("predict"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("price"))
    });

    assert!(prediction_event.is_some(), "Prediction event not found");
}

#[test]
fn test_all_events_for_updown_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    env.mock_all_auths();

    // 1. Initialize (no event expected)
    client.initialize(&admin, &oracle);

    // 2. Mint initial tokens - should emit mint event
    client.mint_initial(&user1);
    let events = env.events().all();
    let mint_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("mint"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("initial"))
    });
    assert!(mint_event.is_some(), "First mint should emit event");

    client.mint_initial(&user2);
    let events = env.events().all();
    let mint_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("mint"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("initial"))
    });
    assert!(mint_event.is_some(), "Second mint should emit event");

    // 3. Create round - should emit round created event
    client.create_round(&1_0000000, &Some(0));

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

    // 4. Place bets - should emit bet placed events
    client.place_bet(&user1, &100_0000000, &BetSide::Up);
    let events = env.events().all();
    let bet_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("bet"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("placed"))
    });
    assert!(bet_event.is_some(), "First bet should emit event");

    client.place_bet(&user2, &150_0000000, &BetSide::Down);
    let events = env.events().all();
    let bet_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("bet"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("placed"))
    });
    assert!(bet_event.is_some(), "Second bet should emit event");

    // 5. Resolve round - should emit round resolved event
    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });

    client.resolve_round(&OraclePayload {
        price: 1_5000000, // Price went up
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
    });

    let events = env.events().all();
    let resolved_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("resolved"))
    });
    assert!(
        resolved_event.is_some(),
        "Round resolved event should be emitted"
    );

    // 6. Claim winnings - should emit claim event
    client.claim_winnings(&user1);

    let events = env.events().all();
    let claim_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("claim"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("winnings"))
    });
    assert!(
        claim_event.is_some(),
        "Claim winnings event should be emitted"
    );
}

#[test]
fn test_all_events_for_precision_round() {
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
    env.events().all();
    client.mint_initial(&user2);
    env.events().all();
    client.mint_initial(&user3);
    env.events().all();

    // Create Precision mode round
    client.create_round(&2_0000000, &Some(1));

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

    // Place predictions - should emit prediction events
    client.predict_price(&user1, &2_2000000, &100_0000000);
    let events = env.events().all();
    let prediction_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("predict"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("price"))
    });
    assert!(
        prediction_event.is_some(),
        "First prediction should emit event"
    );

    client.predict_price(&user2, &2_3000000, &150_0000000);
    let events = env.events().all();
    let prediction_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("predict"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("price"))
    });
    assert!(
        prediction_event.is_some(),
        "Second prediction should emit event"
    );

    client.predict_price(&user3, &2_4000000, &200_0000000);
    let events = env.events().all();
    let prediction_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("predict"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("price"))
    });
    assert!(
        prediction_event.is_some(),
        "Third prediction should emit event"
    );

    // Resolve round
    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });

    client.resolve_round(&OraclePayload {
        price: 2_2500000, // Closest to user2's prediction
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
    });

    let events = env.events().all();
    let resolved_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("resolved"))
    });
    assert!(
        resolved_event.is_some(),
        "Round resolved event should be emitted"
    );

    // Winner claims
    client.claim_winnings(&user2);

    let events = env.events().all();
    let claim_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("claim"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("winnings"))
    });
    assert!(
        claim_event.is_some(),
        "Claim winnings event should be emitted"
    );
}

#[test]
fn test_windows_update_event() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Update windows - should emit windows updated event
    client.set_windows(&10, &30);

    let windows_events = env
        .events()
        .all()
        .iter()
        .filter(|e| {
            let (_contract, topics, _data) = e;
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("windows"))
                && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("updated"))
        })
        .count();
    assert_eq!(windows_events, 1, "Should have 1 windows updated event");
}

// ─── Economic controls for precision mode (Issue #113) ────────────────────────

#[test]
fn test_precision_prediction_exceeds_max_stake_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.set_max_stake(&Some(50_0000000i128));
    client.create_round(&1_0000000, &Some(1));

    let result = client.try_place_precision_prediction(&user, &100_0000000, &2297u128);
    assert_eq!(result, Err(Ok(ContractError::StakeExceedsMax)));
}

#[test]
fn test_precision_prediction_at_max_stake_boundary_succeeds() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.set_max_stake(&Some(100_0000000i128));
    client.create_round(&1_0000000, &Some(1));

    // Exactly at cap — must succeed
    client.place_precision_prediction(&user, &100_0000000, &2297u128);
    assert_eq!(client.balance(&user), 900_0000000);
}

#[test]
fn test_precision_prediction_exposure_cap_exceeded_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.set_max_user_exposure(&Some(75_0000000i128));
    client.create_round(&1_0000000, &Some(1));

    let result = client.try_place_precision_prediction(&user, &80_0000000, &2297u128);
    assert_eq!(result, Err(Ok(ContractError::ExposureCapExceeded)));
}

#[test]
fn test_caps_disabled_precision_prediction_succeeds() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    // No caps configured — large bet allowed
    client.create_round(&1_0000000, &Some(1));
    client.place_precision_prediction(&user, &500_0000000, &2297u128);
    assert_eq!(client.balance(&user), 500_0000000);
}

#[test]
fn test_precision_commit_reveal_happy_path() {
    use soroban_sdk::xdr::ToXdr;
    use soroban_sdk::{Bytes, BytesN};

    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1)); // Precision mode

    let price = 2297u128;
    let salt = BytesN::from_array(&env, &[9; 32]);
    let mut preimage = Bytes::new(&env);
    preimage.append(&price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash = env.crypto().sha256(&preimage);

    // Commit
    let committed_hash: BytesN<32> = hash.into();
    client.commit_prediction(&user, &committed_hash, &100_0000000);
    assert_eq!(client.balance(&user), 900_0000000);

    // Move to reveal window (ledger closes betting at sequence >= 6)
    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });

    // Reveal
    client.reveal_prediction(&user, &price, &salt);

    // Verify prediction is stored
    let prediction = client.get_user_precision_prediction(&user).unwrap();
    assert_eq!(prediction.amount, 100_0000000);
    assert_eq!(prediction.predicted_price, price);
}

#[test]
fn test_precision_commit_reveal_already_revealed() {
    use soroban_sdk::xdr::ToXdr;
    use soroban_sdk::{Bytes, BytesN};

    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1));

    let price = 2297u128;
    let salt = BytesN::from_array(&env, &[9; 32]);
    let mut preimage = Bytes::new(&env);
    preimage.append(&price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash = env.crypto().sha256(&preimage);

    let committed_hash: BytesN<32> = hash.into();
    client.commit_prediction(&user, &committed_hash, &100_0000000);

    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });

    client.reveal_prediction(&user, &price, &salt.clone());

    // Second reveal should fail
    let result = client.try_reveal_prediction(&user, &price, &salt);
    assert_eq!(result, Err(Ok(ContractError::AlreadyRevealed)));
}

#[test]
fn test_precision_commit_reveal_hash_mismatch() {
    use soroban_sdk::xdr::ToXdr;
    use soroban_sdk::{Bytes, BytesN};

    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1));

    let price = 2297u128;
    let salt = BytesN::from_array(&env, &[9; 32]);
    let mut preimage = Bytes::new(&env);
    preimage.append(&price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash = env.crypto().sha256(&preimage);

    let committed_hash: BytesN<32> = hash.into();
    client.commit_prediction(&user, &committed_hash, &100_0000000);

    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });

    // Wrong price
    let result = client.try_reveal_prediction(&user, &2500, &salt.clone());
    assert_eq!(result, Err(Ok(ContractError::HashMismatch)));

    // Wrong salt
    let wrong_salt = BytesN::from_array(&env, &[8; 32]);
    let result = client.try_reveal_prediction(&user, &price, &wrong_salt);
    assert_eq!(result, Err(Ok(ContractError::HashMismatch)));
}

#[test]
fn test_precision_commit_reveal_invalid_window_early() {
    use soroban_sdk::xdr::ToXdr;
    use soroban_sdk::{Bytes, BytesN};

    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1));

    let price = 2297u128;
    let salt = BytesN::from_array(&env, &[9; 32]);
    let mut preimage = Bytes::new(&env);
    preimage.append(&price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash = env.crypto().sha256(&preimage);

    let committed_hash: BytesN<32> = hash.into();
    client.commit_prediction(&user, &committed_hash, &100_0000000);

    // Keep sequence number at 0 (betting window is open)
    let result = client.try_reveal_prediction(&user, &price, &salt);
    assert_eq!(result, Err(Ok(ContractError::InvalidRevealWindow)));
}

#[test]
fn test_precision_commit_reveal_invalid_window_late() {
    use soroban_sdk::xdr::ToXdr;
    use soroban_sdk::{Bytes, BytesN};

    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1));

    let price = 2297u128;
    let salt = BytesN::from_array(&env, &[9; 32]);
    let mut preimage = Bytes::new(&env);
    preimage.append(&price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash = env.crypto().sha256(&preimage);

    let committed_hash: BytesN<32> = hash.into();
    client.commit_prediction(&user, &committed_hash, &100_0000000);

    // Move past end of round (sequence >= 12)
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let result = client.try_reveal_prediction(&user, &price, &salt);
    assert_eq!(result, Err(Ok(ContractError::InvalidRevealWindow)));
}

#[test]
fn test_precision_commit_reveal_commitment_not_found() {
    use soroban_sdk::BytesN;

    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1));

    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });

    // Reveal without commit
    let salt = BytesN::from_array(&env, &[9; 32]);
    let result = client.try_reveal_prediction(&user, &2297, &salt);
    assert_eq!(result, Err(Ok(ContractError::CommitmentNotFound)));
}

#[test]
fn test_precision_commit_reveal_double_bet_fails() {
    use soroban_sdk::xdr::ToXdr;
    use soroban_sdk::{Bytes, BytesN};

    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1));

    let price = 2297u128;
    let salt = BytesN::from_array(&env, &[9; 32]);
    let mut preimage = Bytes::new(&env);
    preimage.append(&price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash = env.crypto().sha256(&preimage);

    let committed_hash: BytesN<32> = hash.into();
    client.commit_prediction(&user, &committed_hash, &100_0000000);

    // Trying to place a direct prediction now should fail with AlreadyBet
    let result = client.try_place_precision_prediction(&user, &50_0000000, &2297);
    assert_eq!(result, Err(Ok(ContractError::AlreadyBet)));
}
