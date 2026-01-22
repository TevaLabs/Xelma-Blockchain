//! Tests for contract initialization and token minting.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_mint_initial() {
    // Create a test environment
    let env = Env::default();
    
    // Register our contract in the test environment
    // This deploys the contract to the test blockchain and returns its unique ID
    // Think of it as: installing your app on a test phone before you can use it
    // The () means we're not passing any initialization arguments
    let contract_id = env.register(VirtualTokenContract, ());
    
    // Create a client to interact with the contract
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    // Generate a random test user address
    let user = Address::generate(&env);
    
    // Mock the authorization (in tests, we need to simulate user approval)
    env.mock_all_auths();
    
    // Call mint_initial for the user
    let balance = client.mint_initial(&user);
    
    // Verify the user received 1000 vXLM
    assert_eq!(balance, 1000_0000000);
    
    // Verify we can query the balance
    let queried_balance = client.balance(&user);
    assert_eq!(queried_balance, 1000_0000000);
}

#[test]
fn test_mint_initial_only_once() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    
    env.mock_all_auths();
    
    // First mint
    let first_mint = client.mint_initial(&user);
    assert_eq!(first_mint, 1000_0000000);
    
    // Try to mint again - should return existing balance, not mint more
    let second_mint = client.mint_initial(&user);
    assert_eq!(second_mint, 1000_0000000);
    
    // Balance should still be 1000, not 2000
    let balance = client.balance(&user);
    assert_eq!(balance, 1000_0000000);
}

#[test]
fn test_balance_for_new_user() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    
    // Query balance for a user who never minted
    let balance = client.balance(&user);
    
    // Should return 0
    assert_eq!(balance, 0);
}

#[test]
fn test_initialize() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    // Generate an admin and oracle address
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    env.mock_all_auths();
    
    // Initialize the contract
    client.initialize(&admin, &oracle);
    
    // Verify admin and oracle are set
    let stored_admin = client.get_admin();
    let stored_oracle = client.get_oracle();
    assert_eq!(stored_admin, Some(admin));
    assert_eq!(stored_oracle, Some(oracle));
}

#[test]
fn test_initialize_twice_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    env.mock_all_auths();
    
    // Initialize once
    client.initialize(&admin, &oracle);
    
    // Try to initialize again - should return error
    let result = client.try_initialize(&admin, &oracle);
    assert_eq!(result, Err(Ok(ContractError::AlreadyInitialized)));
}

