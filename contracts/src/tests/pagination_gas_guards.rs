// SPDX-License-Identifier: MIT
//! Adversarial boundary tests for bounded cursor queries (Issue #574).

use crate::common::MAX_PAGE_SIZE;
use crate::errors::ContractError;
use crate::queries::{
    get_leaderboard_by_streak, get_leaderboard_by_wins, get_precision_predictions_cursor,
    get_updown_positions_cursor,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

/// Registers a contract so the internal query helpers can touch storage.
fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::contract::VirtualTokenContract, ());
    let client = crate::contract::VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, contract_id)
}

#[test]
fn cursor_queries_reject_zero_and_over_limit_before_scanning_storage() {
    let (env, contract_id) = setup();
    let invalid_limits = [0, MAX_PAGE_SIZE + 1, u32::MAX];

    for limit in invalid_limits {
        // Limit validation must happen before any storage access, so it is
        // observable from outside the contract frame.
        assert!(matches!(
            get_precision_predictions_cursor(env.clone(), None, limit),
            Err(ContractError::PageSizeExceeded)
        ));
        assert!(matches!(
            get_updown_positions_cursor(env.clone(), None, limit),
            Err(ContractError::PageSizeExceeded)
        ));
        assert!(matches!(
            get_leaderboard_by_wins(env.clone(), None, limit),
            Err(ContractError::PageSizeExceeded)
        ));
        assert!(matches!(
            get_leaderboard_by_streak(env.clone(), None, limit),
            Err(ContractError::PageSizeExceeded)
        ));
    }
    let _ = contract_id;
}

#[test]
fn cursor_queries_accept_exactly_max_page_size() {
    let (env, contract_id) = setup();

    // At the limit the queries do scan storage, so they must run inside the
    // contract frame.
    env.as_contract(&contract_id, || {
        assert!(get_precision_predictions_cursor(env.clone(), None, MAX_PAGE_SIZE).is_ok());
        assert!(get_updown_positions_cursor(env.clone(), None, MAX_PAGE_SIZE).is_ok());
        assert!(get_leaderboard_by_wins(env.clone(), None, MAX_PAGE_SIZE).is_ok());
        assert!(get_leaderboard_by_streak(env.clone(), None, MAX_PAGE_SIZE).is_ok());
    });
}
