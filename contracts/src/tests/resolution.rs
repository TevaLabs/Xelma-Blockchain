//! Tests for round resolution and winnings distribution.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, DataKey, Round, UserPosition};
use soroban_sdk::{testutils::Address as _, Address, Env, Map};

#[test]
fn test_resolve_round_price_unchanged() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin, &oracle);
    
    // Create a round with start price 1.5 XLM
    let start_price: u128 = 1_5000000;
    client.create_round(&start_price, &60);
    
    // Manually set up some test positions using env.as_contract
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    
    // Give users initial balances
    client.mint_initial(&user1);
    client.mint_initial(&user2);
    
    // Manually create positions for testing using as_contract
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(user1.clone(), UserPosition {
            amount: 100_0000000,
            side: BetSide::Up,
        });
        positions.set(user2.clone(), UserPosition {
            amount: 50_0000000,
            side: BetSide::Down,
        });
        
        // Store positions
        env.storage().persistent().set(&DataKey::Positions, &positions);
        
        // Update round pools to match positions
        let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
        round.pool_up = 100_0000000;
        round.pool_down = 50_0000000;
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });
    
    // Get balances before resolution
    let user1_balance_before = client.balance(&user1);
    let user2_balance_before = client.balance(&user2);
    
    // Resolve with SAME price (unchanged)
    client.resolve_round(&start_price);
    
    // Check pending winnings (not claimed yet)
    assert_eq!(client.get_pending_winnings(&user1), 100_0000000);
    assert_eq!(client.get_pending_winnings(&user2), 50_0000000);
    
    // Claim winnings
    let claimed1 = client.claim_winnings(&user1);
    let claimed2 = client.claim_winnings(&user2);
    
    assert_eq!(claimed1, 100_0000000);
    assert_eq!(claimed2, 50_0000000);
    
    // Both users should get their bets back
    assert_eq!(client.balance(&user1), user1_balance_before + 100_0000000);
    assert_eq!(client.balance(&user2), user2_balance_before + 50_0000000);
    
    // Round should be cleared
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_resolve_round_price_went_up() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin, &oracle);
    
    // Create a round with start price 1.0 XLM
    let start_price: u128 = 1_0000000;
    client.create_round(&start_price, &60);
    
    // Set up test users
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    
    // Give users initial balances
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);
    
    // Create positions using as_contract
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(alice.clone(), UserPosition {
            amount: 100_0000000,
            side: BetSide::Up,
        });
        positions.set(bob.clone(), UserPosition {
            amount: 200_0000000,
            side: BetSide::Up,
        });
        positions.set(charlie.clone(), UserPosition {
            amount: 150_0000000,
            side: BetSide::Down,
        });
        
        env.storage().persistent().set(&DataKey::Positions, &positions);
        
        let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
        round.pool_up = 300_0000000;
        round.pool_down = 150_0000000;
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });
    
    let alice_before = client.balance(&alice);
    let bob_before = client.balance(&bob);
    let charlie_before = client.balance(&charlie);
    
    // Resolve with HIGHER price (1.5 XLM - price went UP)
    client.resolve_round(&1_5000000);
    
    // Check pending winnings
    assert_eq!(client.get_pending_winnings(&alice), 150_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 300_0000000);
    assert_eq!(client.get_pending_winnings(&charlie), 0); // Lost
    
    // Check stats: Alice and Bob won, Charlie lost
    let alice_stats = client.get_user_stats(&alice);
    assert_eq!(alice_stats.total_wins, 1);
    assert_eq!(alice_stats.current_streak, 1);
    
    let charlie_stats = client.get_user_stats(&charlie);
    assert_eq!(charlie_stats.total_losses, 1);
    assert_eq!(charlie_stats.current_streak, 0);
    
    // Claim winnings
    client.claim_winnings(&alice);
    client.claim_winnings(&bob);
    
    assert_eq!(client.balance(&alice), alice_before + 150_0000000);
    assert_eq!(client.balance(&bob), bob_before + 300_0000000);
    assert_eq!(client.balance(&charlie), charlie_before); // No change (lost)
}

#[test]
fn test_resolve_round_price_went_down() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin, &oracle);
    
    // Create a round with start price 2.0 XLM
    let start_price: u128 = 2_0000000;
    client.create_round(&start_price, &60);
    
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    
    // Create positions using as_contract
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(alice.clone(), UserPosition {
            amount: 200_0000000,
            side: BetSide::Down,
        });
        positions.set(bob.clone(), UserPosition {
            amount: 100_0000000,
            side: BetSide::Up,
        });
        
        env.storage().persistent().set(&DataKey::Positions, &positions);
        
        let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
        round.pool_up = 100_0000000;
        round.pool_down = 200_0000000;
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });
    
    let alice_before = client.balance(&alice);
    let bob_before = client.balance(&bob);
    
    // Resolve with LOWER price (1.0 XLM - price went DOWN)
    client.resolve_round(&1_0000000);
    
    // Check pending winnings
    assert_eq!(client.get_pending_winnings(&alice), 300_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 0);
    
    // Alice wins: 200 + (200/200) * 100 = 200 + 100 = 300
    client.claim_winnings(&alice);
    
    assert_eq!(client.balance(&alice), alice_before + 300_0000000);
    assert_eq!(client.balance(&bob), bob_before); // No change (lost)
}

#[test]
fn test_claim_winnings_when_none() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let user = Address::generate(&env);
    env.mock_all_auths();
    
    // Try to claim with no pending winnings
    let claimed = client.claim_winnings(&user);
    assert_eq!(claimed, 0);
}

#[test]
fn test_user_stats_tracking() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    
    // Initial stats should be all zeros
    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 0);
    assert_eq!(stats.total_losses, 0);
    assert_eq!(stats.current_streak, 0);
    assert_eq!(stats.best_streak, 0);
    
    // Simulate a win
    env.as_contract(&contract_id, || {
        VirtualTokenContract::_update_stats_win(&env, alice.clone());
    });
    
    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 1);
    assert_eq!(stats.current_streak, 1);
    assert_eq!(stats.best_streak, 1);
    
    // Another win - streak increases
    env.as_contract(&contract_id, || {
        VirtualTokenContract::_update_stats_win(&env, alice.clone());
    });
    
    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 2);
    assert_eq!(stats.current_streak, 2);
    assert_eq!(stats.best_streak, 2);
    
    // A loss - streak resets
    env.as_contract(&contract_id, || {
        VirtualTokenContract::_update_stats_loss(&env, alice.clone());
    });
    
    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 2);
    assert_eq!(stats.total_losses, 1);
    assert_eq!(stats.current_streak, 0); // Reset
    assert_eq!(stats.best_streak, 2); // Best remains
}

#[test]
fn test_resolve_round_without_active_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    
    client.initialize(&admin, &oracle);
    
    // Try to resolve without creating a round - should return error
    let result = client.try_resolve_round(&1_0000000);
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

