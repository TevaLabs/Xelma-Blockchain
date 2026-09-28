// SPDX-License-Identifier: MIT
//! Adversarial boundary tests for bounded cursor queries (Issue #574).

use crate::common::MAX_PAGE_SIZE;
use crate::errors::ContractError;
use crate::queries::{
    get_leaderboard_by_streak, get_leaderboard_by_wins, get_precision_predictions_cursor,
    get_updown_positions_cursor,
};
use soroban_sdk::Env;

#[test]
fn cursor_queries_reject_zero_and_over_limit_before_scanning_storage() {
    let env = Env::default();
    let invalid_limits = [0, MAX_PAGE_SIZE + 1, u32::MAX];

    for limit in invalid_limits {
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
}

#[test]
fn cursor_queries_accept_exactly_max_page_size() {
    let env = Env::default();

    assert!(get_precision_predictions_cursor(env.clone(), None, MAX_PAGE_SIZE).is_ok());
    assert!(get_updown_positions_cursor(env.clone(), None, MAX_PAGE_SIZE).is_ok());
    assert!(get_leaderboard_by_wins(env.clone(), None, MAX_PAGE_SIZE).is_ok());
    assert!(get_leaderboard_by_streak(env, None, MAX_PAGE_SIZE).is_ok());
}
