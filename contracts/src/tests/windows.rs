//! Tests for configurable betting and execution windows.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, IntoVal,
};

const MAX_BET_WINDOW_LEDGERS: u32 = 1_440;
const MAX_RUN_WINDOW_LEDGERS: u32 = 2_880;

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

    // Note: Testing non-admin access is complex in Soroban test environment
    // The require_auth() call will fail if the caller doesn't match admin
    // This is tested implicitly through the admin requirement in the function
}

#[test]
fn test_set_windows_fails_without_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    // Initialize contract with explicit auth instead of mock_all_auths
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

    // Attempting to set windows without admin auth
    let result = client.try_set_windows(&10, &20);
    assert!(result.is_err());
}

#[test]
fn test_set_windows_fails_with_wrong_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let malicious_user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // We only provide auth for malicious_user, but the contract expects admin auth
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &malicious_user,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_windows",
            args: (10u32, 20u32).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_set_windows(&10, &20);
    assert!(result.is_err());
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
fn test_set_windows_respects_max_bounds() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Inclusive max bounds are accepted.
    client.set_windows(&MAX_BET_WINDOW_LEDGERS, &MAX_RUN_WINDOW_LEDGERS);

    // Off-by-one on either field is rejected with explicit bound error.
    let result = client.try_set_windows(&(MAX_BET_WINDOW_LEDGERS + 1), &MAX_RUN_WINDOW_LEDGERS);
    assert_eq!(result, Err(Ok(ContractError::WindowOutOfRange)));

    let result = client.try_set_windows(&MAX_BET_WINDOW_LEDGERS, &(MAX_RUN_WINDOW_LEDGERS + 1));
    assert_eq!(result, Err(Ok(ContractError::WindowOutOfRange)));
}

#[test]
fn test_set_windows_does_not_mutate_state_on_validation_failure() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.set_windows(&20, &40);

    let result = client.try_set_windows(&41, &40);
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));

    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().expect("Round should exist");
    assert_eq!(round.bet_end_ledger, 20);
    assert_eq!(round.end_ledger, 40);
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
    assert_eq!(round.end_ledger, 120); // 100 + 20
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
    assert_eq!(round.end_ledger, 62); // 50 + 12
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
    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
    });
    assert_eq!(result, Err(Ok(ContractError::RoundNotEnded)));

    // Advance to end_ledger
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolution should succeed
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
    });

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

#[test]
fn test_place_precision_prediction_fails_without_user_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    // Setup with explicit auth
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
            args: (1_0000000u128, Some(1u32)).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.create_round(&1_0000000, &Some(1));

    // Attempt to place precision prediction without user auth
    let result = client.try_place_precision_prediction(&user, &100_0000000, &2297);
    assert!(result.is_err());
}

// ─── Issue #119: start-price bounds ─────────────────────────────────────────

#[test]
fn test_create_round_rejects_zero_start_price() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let result = client.try_create_round(&0u128, &None);
    assert_eq!(result, Err(Ok(ContractError::StartPriceTooLow)));
}

#[test]
fn test_create_round_rejects_price_above_max() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // MAX_START_PRICE = 1_000_000_000_000_000_000; one above must fail
    let result = client.try_create_round(&1_000_000_000_000_000_001u128, &None);
    assert_eq!(result, Err(Ok(ContractError::StartPriceTooHigh)));
}

#[test]
fn test_create_round_accepts_boundary_prices() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Minimum allowed price (1)
    client.create_round(&1u128, &None);
    assert!(client.get_active_round().is_some());

    // Cancel to allow a second round
    client.cancel_round(&0u32);

    // Maximum allowed price
    client.create_round(&1_000_000_000_000_000_000u128, &None);
    assert!(client.get_active_round().is_some());
}
