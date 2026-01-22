//! Tests for boundary conditions and unusual scenarios.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, DataKey, Round, UserPosition};
use soroban_sdk::{testutils::Address as _, Address, Env, Map};

#[test]
fn test_round_with_no_participants() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    env.mock_all_auths();
    
    client.initialize(&admin, &oracle);
    
    // Create round with no bets
    client.create_round(&1_0000000, &100);
    
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 0);
    
    // Resolve with no participants
    client.resolve_round(&1_5000000);
    
    // Should clear round without errors
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_round_with_only_one_side() {
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
    
    // Create round and only bet on UP
    client.create_round(&1_0000000, &100);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &150_0000000, &BetSide::Up);
    
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 250_0000000);
    assert_eq!(round.pool_down, 0);
    
    // Resolve - UP wins but no losers to take from
    client.resolve_round(&1_5000000);
    
    // Winners should only get their bets back (no losing pool to split)
    assert_eq!(client.get_pending_winnings(&alice), 100_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 150_0000000);
}

#[test]
fn test_accumulate_pending_winnings() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    
    env.mock_all_auths();
    
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    
    // Round 1: Alice bets UP and wins
    client.create_round(&1_0000000, &100);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(alice.clone(), UserPosition {
            amount: 100_0000000,
            side: BetSide::Up,
        });
        env.storage().persistent().set(&DataKey::Positions, &positions);
        
        let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
        round.pool_up = 100_0000000;
        round.pool_down = 50_0000000;
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });
    
    client.resolve_round(&1_5000000); // UP wins
    
    let first_pending = client.get_pending_winnings(&alice);
    assert!(first_pending > 0);
    
    // Round 2: Alice bets and gets refund
    client.create_round(&2_0000000, &100);
    client.place_bet(&alice, &50_0000000, &BetSide::Down);
    
    client.resolve_round(&2_0000000); // Price unchanged - refund
    
    // Should have accumulated pending from both rounds
    let total_pending = client.get_pending_winnings(&alice);
    assert_eq!(total_pending, first_pending + 50_0000000);
    
    // Claim all at once
    let claimed = client.claim_winnings(&alice);
    assert_eq!(claimed, total_pending);
    assert_eq!(client.get_pending_winnings(&alice), 0);
}

