//! Tests for round creation and full round lifecycle scenarios.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, DataKey, Round, UserPosition};
use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env, Map};

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
    client.resolve_round(&final_price);
    
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
    assert_eq!(client.balance(&bob), 1100_0000000);   // 800 + 300
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
    client.mint_initial(&alice);
    
    // ROUND 1: Alice bets UP and wins
    client.create_round(&1_0000000, &None);
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
    
    // Advance ledger to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    client.resolve_round(&1_5000000); // UP wins
    client.claim_winnings(&alice);
    
    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 1);
    assert_eq!(stats.current_streak, 1);
    
    // ROUND 2: Alice bets DOWN and wins again
    client.create_round(&2_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Down);
    
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(alice.clone(), UserPosition {
            amount: 100_0000000,
            side: BetSide::Down,
        });
        env.storage().persistent().set(&DataKey::Positions, &positions);
        
        let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
        round.pool_up = 80_0000000;
        round.pool_down = 100_0000000;
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });
    
    // Advance ledger to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 24; // 12 + 12 for second round
    });
    client.resolve_round(&1_5000000); // DOWN wins
    
    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 2);
    assert_eq!(stats.current_streak, 2);
    assert_eq!(stats.best_streak, 2);
}


#[test]
#[should_panic]
fn test_create_round_unauthorized() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::Admin, &admin);
    });
    
    // Fails because 'admin' didn't authorize
    client.create_round(&1_0000000, &None);
}

#[test]
#[should_panic]
fn test_resolve_round_unauthorized() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let oracle = Address::generate(&env);
    
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::Oracle, &oracle);
        // Also need an active round and to be at the right ledger
        let round = Round {
            price_start: 100,
            start_ledger: 0,
            bet_end_ledger: 6,
            end_ledger: 12,
            pool_up: 0,
            pool_down: 0,
            mode: crate::types::RoundMode::UpDown,
        };
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });
    
    env.ledger().with_mut(|li| li.sequence_number = 12);
    
    // Fails because 'oracle' didn't authorize
    client.resolve_round(&150);
}

#[test]
#[should_panic]
fn test_place_bet_unauthorized() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let user = Address::generate(&env);
    
    env.as_contract(&contract_id, || {
        let round = Round {
            price_start: 100,
            start_ledger: 0,
            bet_end_ledger: 6,
            end_ledger: 12,
            pool_up: 0,
            pool_down: 0,
            mode: crate::types::RoundMode::UpDown,
        };
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
        // Give user some balance
        env.storage().persistent().set(&DataKey::Balance(user.clone()), &1000i128);
    });
    
    // Fails because 'user' didn't authorize
    client.place_bet(&user, &100, &BetSide::Up);
}

#[test]
#[should_panic]
fn test_claim_winnings_unauthorized() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let user = Address::generate(&env);
    
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::PendingWinnings(user.clone()), &100i128);
    });
    
    // Fails because 'user' didn't authorize
    client.claim_winnings(&user);
}

#[test]
#[should_panic]
fn test_place_precision_prediction_unauthorized() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let user = Address::generate(&env);
    
    env.as_contract(&contract_id, || {
        let round = Round {
            price_start: 100,
            start_ledger: 0,
            bet_end_ledger: 6,
            end_ledger: 12,
            pool_up: 0,
            pool_down: 0,
            mode: crate::types::RoundMode::Precision,
        };
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
        // Give user some balance
        env.storage().persistent().set(&DataKey::Balance(user.clone()), &1000i128);
    });
    
    // Fails because 'user' didn't authorize
    client.place_precision_prediction(&user, &100, &2300);
}
