//! Tests for bet placement and validation.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::BetSide;
use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env};

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
    client.create_round(&1_0000000, &100);
    
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
    client.create_round(&1_0000000, &100);
    
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
    
    // Create round that ends at ledger 50
    client.create_round(&1_0000000, &50);
    
    // Advance ledger past end time
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });
    
    // Try to bet after round ended - should return error
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
    client.create_round(&1_0000000, &100);
    
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
    client.create_round(&1_0000000, &100);
    
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

