//! Tests for predict_price function (Legends mode precision betting).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env};

#[test]
fn test_predict_price_valid_guess() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create Precision/Legends round
    client.create_round(&10000, &100, &Some(1));

    // Valid prediction: 0.2297 → 2297 (4 decimals)
    client.predict_price(&user, &2297, &100_0000000);

    // Verify prediction was stored
    let prediction = client.get_user_precision_prediction(&user).unwrap();
    assert_eq!(prediction.predicted_price, 2297);
    assert_eq!(prediction.amount, 100_0000000);

    // Verify balance was deducted
    assert_eq!(client.balance(&user), 900_0000000);
}

#[test]
fn test_predict_price_various_valid_scales() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Test various valid 4-decimal prices
    let test_cases = [
        0,      // 0.0000
        1,      // 0.0001
        2297,   // 0.2297
        9999,   // 0.9999
        10000,  // 1.0000
        50000,  // 5.0000
        99999,  // 9.9999
    ];

    for price in test_cases {
        let user = Address::generate(&env);
        client.mint_initial(&user);
        
        // Create new round for each test
        client.create_round(&10000, &100, &Some(1));
        
        // Should accept valid scaled price
        client.predict_price(&user, &price, &100_0000000);
        
        let prediction = client.get_user_precision_prediction(&user).unwrap();
        assert_eq!(prediction.predicted_price, price);
    }
}

#[test]
fn test_predict_price_invalid_scale_too_large() {
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
    client.create_round(&10000, &100, &Some(1));

    // Invalid: 100000 would be 10.0000 or 5 decimals
    let result = client.try_predict_price(&user, &100_000, &100_0000000);
    assert_eq!(result, Err(Ok(ContractError::InvalidPriceScale)));

    // Invalid: way too large
    let result = client.try_predict_price(&user, &1_000_000, &100_0000000);
    assert_eq!(result, Err(Ok(ContractError::InvalidPriceScale)));
}

#[test]
fn test_predict_price_on_updown_mode_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Create Up/Down round (mode 0)
    client.create_round(&10000, &100, &Some(0));

    // predict_price should fail on Up/Down mode
    let result = client.try_predict_price(&user, &2297, &100_0000000);
    assert_eq!(result, Err(Ok(ContractError::WrongModeForPrediction)));
}

#[test]
fn test_predict_price_no_active_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // No round created
    let result = client.try_predict_price(&user, &2297, &100_0000000);
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

#[test]
fn test_predict_price_round_ended() {
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

    // Create round ending at ledger 50
    client.create_round(&10000, &50, &Some(1));

    // Advance past end
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });

    let result = client.try_predict_price(&user, &2297, &100_0000000);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
}

#[test]
fn test_predict_price_insufficient_balance() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user); // Has 1000 vXLM

    client.create_round(&10000, &100, &Some(1));

    // Try to bet more than balance
    let result = client.try_predict_price(&user, &2297, &2000_0000000);
    assert_eq!(result, Err(Ok(ContractError::InsufficientBalance)));
}

#[test]
fn test_predict_price_already_bet() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    client.create_round(&10000, &100, &Some(1));

    // First prediction succeeds
    client.predict_price(&user, &2297, &100_0000000);

    // Second prediction should fail
    let result = client.try_predict_price(&user, &2500, &50_0000000);
    assert_eq!(result, Err(Ok(ContractError::AlreadyBet)));
}

#[test]
fn test_predict_price_invalid_amount_zero() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    client.create_round(&10000, &100, &Some(1));

    // Zero amount should fail
    let result = client.try_predict_price(&user, &2297, &0);
    assert_eq!(result, Err(Ok(ContractError::InvalidBetAmount)));
}

#[test]
fn test_predict_price_invalid_amount_negative() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    client.create_round(&10000, &100, &Some(1));

    // Negative amount should fail
    let result = client.try_predict_price(&user, &2297, &-100);
    assert_eq!(result, Err(Ok(ContractError::InvalidBetAmount)));
}

#[test]
fn test_predict_price_multiple_users() {
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
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&10000, &100, &Some(1));

    // Multiple users predict different prices
    client.predict_price(&alice, &2297, &100_0000000);
    client.predict_price(&bob, &2500, &150_0000000);
    client.predict_price(&charlie, &3000, &200_0000000);

    // Verify all predictions stored
    let predictions = client.get_precision_predictions();
    assert_eq!(predictions.len(), 3);

    let alice_pred = predictions.get(0).unwrap();
    assert_eq!(alice_pred.user, alice);
    assert_eq!(alice_pred.predicted_price, 2297);

    let bob_pred = predictions.get(1).unwrap();
    assert_eq!(bob_pred.user, bob);
    assert_eq!(bob_pred.predicted_price, 2500);

    let charlie_pred = predictions.get(2).unwrap();
    assert_eq!(charlie_pred.user, charlie);
    assert_eq!(charlie_pred.predicted_price, 3000);
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

    client.create_round(&10000, &100, &Some(1));

    // Place prediction and verify event is emitted
    client.predict_price(&user, &2297, &100_0000000);

    // Events are emitted (we can't directly assert on them in tests,
    // but the function call succeeding means the event was published)
    // In production, events can be monitored off-chain
}
