// SPDX-License-Identifier: MIT
//! Tests for configurable betting and execution windows.

use super::config_helpers::apply_windows;
use super::ledger_snapshot::assert_ledger_unchanged;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload, RoundPhase, RoundStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env, IntoVal,
};

const MAX_BET_WINDOW_LEDGERS: u32 = 1_440;
const MAX_RUN_WINDOW_LEDGERS: u32 = 2_880;

/// `sha256(price.to_xdr() || salt.to_xdr())` — the digest the production
/// `reveal_prediction` path recomputes and compares against the commitment
/// stored at commit time (mirrors `commit_reveal_e2e::make_commitment`).
fn make_commitment(env: &Env, price: u128, salt: &BytesN<32>) -> BytesN<32> {
    use soroban_sdk::xdr::ToXdr;
    let mut preimage = soroban_sdk::Bytes::new(env);
    preimage.append(&price.to_xdr(env));
    preimage.append(&salt.clone().to_xdr(env));
    env.crypto().sha256(&preimage).into()
}

/// Non-zero, non-constant salt that satisfies the contract's minimum
/// entropy check (`salt_has_minimum_entropy`).
fn boundary_test_salt(env: &Env) -> BytesN<32> {
    let mut bytes = [0x5Au8; 32];
    bytes[0] = 0xA5;
    BytesN::from_array(env, &bytes)
}

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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);
    apply_windows(&env, &client, 20, 40);

    let result = client.try_set_windows(&41, &40);
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));

    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().expect("Round should exist");
    assert_eq!(round.bet_end_ledger, 20);
    assert_eq!(round.end_ledger, 40);
}

#[test]
fn test_set_close_buffer_ledgers_rejects_out_of_range() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    let result = client.try_set_close_buffer_ledgers(&(MAX_BET_WINDOW_LEDGERS + 1));
    assert_eq!(result, Err(Ok(ContractError::WindowOutOfRange)));
}

#[test]
fn test_close_buffer_rejects_bets_in_final_window() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 0;
    });

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user_a);
    client.mint_initial(&user_b);

    apply_windows(&env, &client, 6, 12);
    client.set_close_buffer_ledgers(&2);
    client.create_round(&1_0000000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 3;
    });
    client.place_bet(&user_a, &50_0000000, &BetSide::Up);

    // Close buffer freezes at ledger 4 (bet_end=6, close_buffer=2)
    env.ledger().with_mut(|li| {
        li.sequence_number = 4;
    });
    let result = client.try_place_bet(&user_b, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::BettingClosed)));

    // At bet_end_ledger, the error should be RoundEnded
    env.ledger().with_mut(|li| {
        li.sequence_number = 6;
    });
    let result = client.try_place_bet(&user_b, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
}

#[test]
fn test_close_buffer_distinct_error_boundary() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 0;
    });

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let user_c = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user_a);
    client.mint_initial(&user_b);
    client.mint_initial(&user_c);

    // bet_end=6, close_buffer=3, close_ledger=3
    apply_windows(&env, &client, 6, 12);
    client.set_close_buffer_ledgers(&3);
    client.create_round(&1_0000000, &None);

    // Ledger 2: before close buffer — bets accepted
    env.ledger().with_mut(|li| li.sequence_number = 2);
    client.place_bet(&user_a, &50_0000000, &BetSide::Up);

    // Ledger 3: at close buffer edge — BettingClosed
    env.ledger().with_mut(|li| li.sequence_number = 3);
    let result = client.try_place_bet(&user_b, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::BettingClosed)));

    // Ledger 5: still in buffer zone — BettingClosed
    env.ledger().with_mut(|li| li.sequence_number = 5);
    let result = client.try_place_bet(&user_b, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::BettingClosed)));

    // Ledger 6: at bet_end_ledger — RoundEnded
    env.ledger().with_mut(|li| li.sequence_number = 6);
    let result = client.try_place_bet(&user_c, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
}

#[test]
fn test_close_buffer_precision_prediction_distinct_error() {
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

    apply_windows(&env, &client, 6, 12);
    client.set_close_buffer_ledgers(&2);
    client.create_round(&1_0000000, &Some(1));

    // Ledger 4: close buffer freezes — BettingClosed for precision
    env.ledger().with_mut(|li| li.sequence_number = 4);
    let result = client.try_place_precision_prediction(&user, &50_0000000, &2297);
    assert_eq!(result, Err(Ok(ContractError::BettingClosed)));

    // Ledger 6: bet window ended — RoundEnded
    env.ledger().with_mut(|li| li.sequence_number = 6);
    let result = client.try_place_precision_prediction(&user, &50_0000000, &2297);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
}

#[test]
fn test_close_buffer_commit_prediction_distinct_error() {
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

    apply_windows(&env, &client, 6, 12);
    client.set_close_buffer_ledgers(&2);
    client.create_round(&1_0000000, &Some(1));

    let hash = BytesN::from_array(&env, &[7; 32]);

    // Ledger 4: close buffer freezes — BettingClosed for commit
    env.ledger().with_mut(|li| li.sequence_number = 4);
    let result = client.try_commit_prediction(&user, &hash, &50_0000000);
    assert_eq!(result, Err(Ok(ContractError::BettingClosed)));

    // Ledger 6: bet window ended — RoundEnded
    env.ledger().with_mut(|li| li.sequence_number = 6);
    let result = client.try_commit_prediction(&user, &hash, &50_0000000);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
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
    client.update_oracle_heartbeat(&0u32);

    // Set custom windows
    apply_windows(&env, &client, 10, 20);

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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user);

    // Set windows: bet closes at ledger 6, round ends at ledger 12
    apply_windows(&env, &client, 6, 12);

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
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user);

    // Set windows: bet closes at ledger 6, round ends at ledger 12
    apply_windows(&env, &client, 6, 12);

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
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user);

    // Set windows
    apply_windows(&env, &client, 6, 12);

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
    client.update_oracle_heartbeat(&0u32);

    let result = client.try_create_round(&0u128, &None);
    assert_eq!(result, Err(Ok(ContractError::InvalidStartPrice)));
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
    client.update_oracle_heartbeat(&0u32);

    // MAX_START_PRICE = 1_000_000_000_000_000_000; one above must fail
    let result = client.try_create_round(&1_000_000_000_000_000_001u128, &None);
    assert_eq!(result, Err(Ok(ContractError::InvalidStartPrice)));
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
    client.update_oracle_heartbeat(&0u32);

    // Minimum allowed price (1)
    client.create_round(&1u128, &None);
    assert!(client.get_active_round().is_some());

    // Cancel to allow a second round
    client.cancel_round(&0u32);

    // A ledger sequence backs at most one round (oracle payloads bind to
    // `Round.start_ledger`), so advance before creating the replacement.
    env.ledger().with_mut(|li| {
        li.sequence_number += 1;
    });

    // Maximum allowed price
    client.create_round(&1_000_000_000_000_000_000u128, &None);
    assert!(client.get_active_round().is_some());
}

#[test]
fn test_get_round_phase_no_active_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    let result = client.try_get_round_phase();
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

#[test]
fn test_get_round_phase_boundary_ledgers() {
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
    client.update_oracle_heartbeat(&0u32);

    apply_windows(&env, &client, 10, 20);
    client.create_round(&1_0000000u128, &None);

    // start=100, bet_end=110, end=120
    env.ledger().with_mut(|li| li.sequence_number = 109);
    assert_eq!(client.get_round_phase(), RoundPhase::Betting);

    env.ledger().with_mut(|li| li.sequence_number = 110);
    assert_eq!(client.get_round_phase(), RoundPhase::Running);

    env.ledger().with_mut(|li| li.sequence_number = 119);
    assert_eq!(client.get_round_phase(), RoundPhase::Running);

    env.ledger().with_mut(|li| li.sequence_number = 120);
    assert_eq!(client.get_round_phase(), RoundPhase::Resolvable);
}

#[test]
fn test_get_round_phase_default_windows() {
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
    client.update_oracle_heartbeat(&0u32);
    client.create_round(&1_0000000u128, &None);

    // default: bet_end=56, end=62
    env.ledger().with_mut(|li| li.sequence_number = 55);
    assert_eq!(client.get_round_phase(), RoundPhase::Betting);

    env.ledger().with_mut(|li| li.sequence_number = 56);
    assert_eq!(client.get_round_phase(), RoundPhase::Running);

    env.ledger().with_mut(|li| li.sequence_number = 62);
    assert_eq!(client.get_round_phase(), RoundPhase::Resolvable);
}

// ─── Issue #668: bet-window boundary ledgers (off-by-one) ───────────────────

/// `commit_prediction` must be accepted at `bet_end_ledger - 1` (the last
/// betting ledger) and rejected with `RoundEnded` exactly at
/// `bet_end_ledger`, without mutating the rejected caller's balance.
#[test]
fn test_commit_prediction_boundary_at_bet_end_minus_one_and_bet_end() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 0;
    });

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user_a);
    client.mint_initial(&user_b);

    // start_ledger = 0 ⇒ bet_end_ledger = 6, end_ledger = 12
    apply_windows(&env, &client, 6, 12);
    client.create_round(&1_0000000, &Some(1));

    let hash_a = BytesN::from_array(&env, &[7u8; 32]);
    let hash_b = BytesN::from_array(&env, &[8u8; 32]);

    // bet_end_ledger - 1 = 5: last ledger of the bet window — accepted.
    env.ledger().with_mut(|li| li.sequence_number = 5);
    client.commit_prediction(&user_a, &hash_a, &50_0000000);
    assert_eq!(client.balance(&user_a), 1000_0000000 - 50_0000000);

    // bet_end_ledger = 6: the reveal window opens and the commit window
    // closes on the same ledger — off-by-one must reject here.
    env.ledger().with_mut(|li| li.sequence_number = 6);
    let result = client.try_commit_prediction(&user_b, &hash_b, &50_0000000);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
    assert_eq!(
        client.balance(&user_b),
        1000_0000000,
        "rejected commit must not move balance"
    );
}

/// The reveal window opens exactly at `bet_end_ledger` (Issue #668): a
/// reveal at `bet_end_ledger - 1` fails with `InvalidRevealWindow` and
/// writes no prediction; the identical reveal succeeds at
/// `bet_end_ledger` itself.
#[test]
fn test_reveal_window_opens_exactly_at_bet_end_ledger() {
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
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user);

    // start_ledger = 0 ⇒ bet_end_ledger = 6, end_ledger = 12
    apply_windows(&env, &client, 6, 12);
    client.create_round(&1_0000000, &Some(1));

    let price = 2297u128;
    let salt = boundary_test_salt(&env);
    let hash = make_commitment(&env, price, &salt);
    client.commit_prediction(&user, &hash, &50_0000000);
    assert_eq!(client.balance(&user), 1000_0000000 - 50_0000000);

    // bet_end_ledger - 1 = 5: still the betting window — reveal rejected,
    // no prediction written, commitment left intact.
    env.ledger().with_mut(|li| li.sequence_number = 5);
    let result = client.try_reveal_prediction(&user, &price, &salt);
    assert_eq!(result, Err(Ok(ContractError::InvalidRevealWindow)));
    assert!(client.get_user_precision_prediction(&user).is_none());

    // bet_end_ledger = 6: reveal window opens — the same reveal succeeds.
    env.ledger().with_mut(|li| li.sequence_number = 6);
    client.reveal_prediction(&user, &price, &salt);
    let prediction = client.get_user_precision_prediction(&user).unwrap();
    assert_eq!(prediction.predicted_price, price);
    assert_eq!(prediction.amount, 50_0000000);
}

// ─── Issue #804: host bet-window boundary ledgers (bet_end-1 vs bet_end) ────

/// Host ledger simulation across `bet_end_ledger - 1` and `bet_end_ledger` for
/// Up/Down `place_bet` and phase/status queries on a non-zero `start_ledger`.
///
/// Uses a distinct fresh bettor at `bet_end_ledger` so `AlreadyBet` cannot mask
/// `RoundEnded`, and proves the rejected call leaves the entire ledger
/// snapshot unchanged.
#[test]
fn test_host_bet_window_boundary_updown_place_bet_and_status() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user_a);
    client.mint_initial(&user_b);

    // start_ledger = 100 ⇒ bet_end_ledger = 110, end_ledger = 120
    apply_windows(&env, &client, 10, 20);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().expect("Round should exist");
    assert_eq!(round.bet_end_ledger, 110);

    // bet_end_ledger - 1 = 109: last betting ledger — queries report Betting
    // and place_bet succeeds.
    env.ledger().with_mut(|li| li.sequence_number = 109);
    assert_eq!(client.get_round_phase(), RoundPhase::Betting);
    assert_eq!(
        client.get_round_status(&round.round_id),
        RoundStatus::Betting
    );
    let snap_before = client.get_market_snapshot();
    assert_eq!(snap_before.phase.get(0), Some(RoundPhase::Betting));

    client.place_bet(&user_a, &50_0000000, &BetSide::Up);
    assert_eq!(client.balance(&user_a), 1000_0000000 - 50_0000000);
    assert!(client.get_user_position(&user_a).is_some());

    // bet_end_ledger = 110: betting window closed, phase flips to Running,
    // and a fresh bettor is rejected with RoundEnded without mutating storage.
    env.ledger().with_mut(|li| li.sequence_number = 110);
    assert_eq!(client.get_round_phase(), RoundPhase::Running);
    assert_eq!(
        client.get_round_status(&round.round_id),
        RoundStatus::Running
    );
    let snap_at_end = client.get_market_snapshot();
    assert_eq!(snap_at_end.phase.get(0), Some(RoundPhase::Running));

    let res = assert_ledger_unchanged(&env, "place_bet at bet_end_ledger", || {
        client.try_place_bet(&user_b, &50_0000000, &BetSide::Down)
    });
    assert_eq!(res, Err(Ok(ContractError::RoundEnded)));
    assert_eq!(client.balance(&user_b), 1000_0000000);
    assert!(client.get_user_position(&user_b).is_none());

    let updated = client.get_active_round().expect("Round should exist");
    assert_eq!(updated.pool_up, 50_0000000);
    assert_eq!(updated.pool_down, 0);
}

/// Host ledger simulation at `bet_end_ledger - 1` and `bet_end_ledger` for
/// Precision-mode `place_precision_prediction` and its `predict_price` alias.
#[test]
fn test_host_bet_window_boundary_precision_predictions_and_alias() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 250;
    });

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let user_c = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user_a);
    client.mint_initial(&user_b);
    client.mint_initial(&user_c);

    // start_ledger = 250 ⇒ bet_end_ledger = 258, end_ledger = 266
    apply_windows(&env, &client, 8, 16);
    client.create_round(&1_0000000, &Some(1));

    // bet_end_ledger - 1 = 257: both direct prediction entrypoints succeed.
    env.ledger().with_mut(|li| li.sequence_number = 257);
    client.place_precision_prediction(&user_a, &50_0000000i128, &1_0500000u128);
    client.predict_price(&user_b, &1_0600000u128, &25_0000000i128);
    assert_eq!(client.balance(&user_a), 1000_0000000 - 50_0000000);
    assert_eq!(client.balance(&user_b), 1000_0000000 - 25_0000000);

    // bet_end_ledger = 258: both entrypoints reject with RoundEnded and leave
    // the full ledger untouched.
    env.ledger().with_mut(|li| li.sequence_number = 258);
    let res_direct =
        assert_ledger_unchanged(&env, "place_precision_prediction at bet_end_ledger", || {
            client.try_place_precision_prediction(&user_c, &50_0000000i128, &1_0700000u128)
        });
    assert_eq!(res_direct, Err(Ok(ContractError::RoundEnded)));

    let res_alias = assert_ledger_unchanged(&env, "predict_price at bet_end_ledger", || {
        client.try_predict_price(&user_c, &1_0700000u128, &50_0000000i128)
    });
    assert_eq!(res_alias, Err(Ok(ContractError::RoundEnded)));
    assert_eq!(client.balance(&user_c), 1000_0000000);
    assert!(client.get_user_precision_prediction(&user_c).is_none());
}

/// Early cash-out opens on the exact ledger betting closes (`bet_end_ledger`):
/// rejected with `InvalidPhaseForCashout` (and zero storage mutation) at
/// `bet_end_ledger - 1`, and accepted at `bet_end_ledger`.
#[test]
fn test_host_bet_window_boundary_early_cashout() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 500;
    });

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.set_early_cashout_bps(&Some(1_000)); // 10% penalty
    client.mint_initial(&user_a);
    client.mint_initial(&user_b);

    // start_ledger = 500 ⇒ bet_end_ledger = 506, end_ledger = 512
    apply_windows(&env, &client, 6, 12);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user_a, &100_0000000, &BetSide::Up);
    client.place_bet(&user_b, &100_0000000, &BetSide::Down);

    // bet_end_ledger - 1 = 505: still Betting phase — cash_out_early rejected.
    env.ledger().with_mut(|li| li.sequence_number = 505);
    let res = assert_ledger_unchanged(&env, "cash_out_early at bet_end_ledger - 1", || {
        client.try_cash_out_early(&user_a)
    });
    assert_eq!(res, Err(Ok(ContractError::InvalidPhaseForCashout)));
    assert!(client.get_user_position(&user_a).is_some());
    assert_eq!(client.get_pending_winnings(&user_a), 0);

    // bet_end_ledger = 506: Running phase starts — cash_out_early succeeds.
    env.ledger().with_mut(|li| li.sequence_number = 506);
    client.cash_out_early(&user_a);
    assert!(client.get_user_position(&user_a).is_none());
    assert_eq!(client.get_pending_winnings(&user_a), 90_0000000);

    let round = client.get_active_round().expect("Round should exist");
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 100_0000000);
}

/// Minimal 1-ledger bet window (`bet_ledgers = 1, run_ledgers = 2`):
/// `start_ledger == bet_end_ledger - 1` accepts bets, and the very next ledger
/// (`bet_end_ledger`) rejects with `RoundEnded` and no storage mutation.
#[test]
fn test_host_bet_window_boundary_minimal_one_ledger_window() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 42;
    });

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user_a);
    client.mint_initial(&user_b);

    // start_ledger = 42 ⇒ bet_end_ledger = 43, end_ledger = 44
    apply_windows(&env, &client, 1, 2);
    client.create_round(&1_0000000, &None);

    // bet_end_ledger - 1 = 42: single open betting ledger.
    assert_eq!(client.get_round_phase(), RoundPhase::Betting);
    client.place_bet(&user_a, &50_0000000, &BetSide::Up);

    // bet_end_ledger = 43: immediately closed.
    env.ledger().with_mut(|li| li.sequence_number = 43);
    assert_eq!(client.get_round_phase(), RoundPhase::Running);
    let res = assert_ledger_unchanged(&env, "place_bet at bet_end (1-ledger window)", || {
        client.try_place_bet(&user_b, &50_0000000, &BetSide::Down)
    });
    assert_eq!(res, Err(Ok(ContractError::RoundEnded)));
}

/// Host ledger simulation across the close-buffer and `bet_end_ledger`
/// boundary ladder (`close_ledger - 1`, `close_ledger`, `bet_end_ledger - 1`,
/// `bet_end_ledger`) proving exact error codes and zero storage mutation on
/// every rejected boundary ledger.
#[test]
fn test_host_bet_window_boundary_with_close_buffer_no_mutation() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user_a);
    client.mint_initial(&user_b);

    // start_ledger = 100, bet_end_ledger = 110, close_buffer = 2 ⇒ close_ledger = 108
    apply_windows(&env, &client, 10, 20);
    client.set_close_buffer_ledgers(&2);
    client.create_round(&1_0000000, &None);

    // close_ledger - 1 = 107: last accepted ledger before soft close.
    env.ledger().with_mut(|li| li.sequence_number = 107);
    client.place_bet(&user_a, &50_0000000, &BetSide::Up);

    // close_ledger = 108: soft close starts — BettingClosed, no mutation.
    env.ledger().with_mut(|li| li.sequence_number = 108);
    let res_close = assert_ledger_unchanged(&env, "place_bet at close_ledger", || {
        client.try_place_bet(&user_b, &50_0000000, &BetSide::Down)
    });
    assert_eq!(res_close, Err(Ok(ContractError::BettingClosed)));

    // bet_end_ledger - 1 = 109: final ledger inside close buffer — BettingClosed, no mutation.
    env.ledger().with_mut(|li| li.sequence_number = 109);
    let res_before_end =
        assert_ledger_unchanged(&env, "place_bet at bet_end - 1 (buffered)", || {
            client.try_place_bet(&user_b, &50_0000000, &BetSide::Down)
        });
    assert_eq!(res_before_end, Err(Ok(ContractError::BettingClosed)));

    // bet_end_ledger = 110: hard close takes precedence — RoundEnded, no mutation.
    env.ledger().with_mut(|li| li.sequence_number = 110);
    let res_end = assert_ledger_unchanged(&env, "place_bet at bet_end (buffered)", || {
        client.try_place_bet(&user_b, &50_0000000, &BetSide::Down)
    });
    assert_eq!(res_end, Err(Ok(ContractError::RoundEnded)));
}
