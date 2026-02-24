//! Tests for configurable betting and execution windows.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, DataKey};
use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env, Map};

#[test]
fn test_set_windows_admin_only() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    env.mock_all_auths();
    
    // Initialize contract
    client.initialize(&admin, &oracle);
    
    // Admin can set windows
    client.set_windows(&10, &20);
}

#[test]
#[should_panic]
fn test_set_windows_unauthorized() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    // Auth for initialize
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    
    // NO mock_all_auths() before this call, and we'll use a different address
    // Actually, as mentioned, once mock_all_auths is called on an Env, it's global.
    // So we use a fresh Env and as_contract.
    let env2 = Env::default();
    let contract_id2 = env2.register(VirtualTokenContract, ());
    let client2 = VirtualTokenContractClient::new(&env2, &contract_id2);
    
    env2.as_contract(&contract_id2, || {
        env2.storage().persistent().set(&DataKey::Admin, &admin);
    });
    
    // This should panic because no auth is mocked for 'admin'
    client2.set_windows(&10, &20);
}

#[test]
fn test_set_windows_positive_values() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    env.mock_all_auths();
    
    client.initialize(&admin, &oracle);
    
    // Zero values should fail
    let result = client.try_set_windows(&0, &12);
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));
    
    let result = client.try_set_windows(&6, &0);
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));
    
    // Valid values should succeed
    client.set_windows(&10, &20);
}

#[test]
fn test_set_windows_bet_must_be_less_than_run() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    env.mock_all_auths();
    
    client.initialize(&admin, &oracle);
    
    // bet_ledgers >= run_ledgers should fail
    let result = client.try_set_windows(&12, &12);
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));
    
    let result = client.try_set_windows(&15, &10);
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));
    
    // Valid: bet < run
    client.set_windows(&6, &12);
}

#[test]
fn test_create_round_uses_configured_windows() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });
    
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    env.mock_all_auths();
    
    client.initialize(&admin, &oracle);
    
    // Set custom windows
    client.set_windows(&10, &20);
    
    // Create round
    let start_price: u128 = 1_0000000;
    client.create_round(&start_price, &None);
    
    let round = client.get_active_round().expect("Round should exist");
    
    // Verify windows are applied
    assert_eq!(round.start_ledger, 100);
    assert_eq!(round.bet_end_ledger, 110); // 100 + 10
    assert_eq!(round.end_ledger, 120);    // 100 + 20
}

#[test]
fn test_create_round_uses_default_windows() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 50;
    });
    
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    env.mock_all_auths();
    
    client.initialize(&admin, &oracle);
    
    // Don't set custom windows, use defaults
    let start_price: u128 = 1_0000000;
    client.create_round(&start_price, &None);
    
    let round = client.get_active_round().expect("Round should exist");
    
    // Verify default windows (6 and 12) are applied
    assert_eq!(round.start_ledger, 50);
    assert_eq!(round.bet_end_ledger, 56); // 50 + 6
    assert_eq!(round.end_ledger, 62);     // 50 + 12
}

#[test]
fn test_betting_closes_at_bet_end_ledger() {
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
    
    // Set windows: bet closes at ledger 6, round ends at ledger 12
    client.set_windows(&6, &12);
    
    // Create round
    client.create_round(&1_0000000, &None);
    
    // Betting should work before bet_end_ledger
    env.ledger().with_mut(|li| {
        li.sequence_number = 5;
    });
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    
    // Betting should fail at bet_end_ledger
    env.ledger().with_mut(|li| {
        li.sequence_number = 6;
    });
    let result = client.try_place_bet(&user, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
    
    // Betting should fail after bet_end_ledger
    env.ledger().with_mut(|li| {
        li.sequence_number = 10;
    });
    let result = client.try_place_bet(&user, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
}

#[test]
fn test_resolution_only_allowed_after_run_ledgers() {
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
    
    // Set windows: bet closes at ledger 6, round ends at ledger 12
    client.set_windows(&6, &12);
    
    // Create round
    client.create_round(&1_0000000, &None);
    
    // User places bet
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    
    // Advance past bet window but before run window
    env.ledger().with_mut(|li| {
        li.sequence_number = 10;
    });
    
    // Resolution should fail before end_ledger
    let result = client.try_resolve_round(&1_5000000);
    assert_eq!(result, Err(Ok(ContractError::RoundNotEnded)));
    
    // Advance to end_ledger
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    
    // Resolution should succeed
    client.resolve_round(&1_5000000);
    
    // Round should be cleared
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_precision_prediction_respects_bet_window() {
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
    
    // Set windows
    client.set_windows(&6, &12);
    
    // Create round in Precision mode
    client.create_round(&1_0000000, &Some(1));
    
    // Prediction should work before bet_end_ledger
    env.ledger().with_mut(|li| {
        li.sequence_number = 5;
    });
    client.place_precision_prediction(&user, &100_0000000, &2297);
    
    // Prediction should fail at bet_end_ledger
    env.ledger().with_mut(|li| {
        li.sequence_number = 6;
    });
    let result = client.try_place_precision_prediction(&user, &50_0000000, &2300);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
}
