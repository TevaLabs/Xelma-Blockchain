// SPDX-License-Identifier: MIT
//! Tests for the Leaderboard Read APIs with cursor-based pagination (Issue #296).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{LeaderboardEntry, UserStats};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

#[test]
fn test_leaderboard_ordered_by_wins() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let user_c = Address::generate(&env);

    // Give Alice 3 wins
    env.as_contract(&contract_id, || {
        for _ in 0..3 {
            VirtualTokenContract::_update_stats_win(&env, user_a.clone()).unwrap();
        }
    });
    // Give Bob 5 wins
    env.as_contract(&contract_id, || {
        for _ in 0..5 {
            VirtualTokenContract::_update_stats_win(&env, user_b.clone()).unwrap();
        }
    });
    // Give Carol 2 wins
    env.as_contract(&contract_id, || {
        for _ in 0..2 {
            VirtualTokenContract::_update_stats_win(&env, user_c.clone()).unwrap();
        }
    });

    // Must create a round so the leaderboard collector can find active participants.
    client.create_round(&1_0000000u128, &None);

    // Query wins leaderboard with cursor = None (first page)
    let page = client.get_leaderboard_by_wins(&None, &10).unwrap();
    assert_eq!(page.0.len(), 3);

    let entries = page.0;
    // Expected order: Bob (5), Alice (3), Carol (2)
    assert_eq!(entries.get(0).unwrap().user, user_b);
    assert_eq!(entries.get(0).unwrap().stats.total_wins, 5);

    assert_eq!(entries.get(1).unwrap().user, user_a);
    assert_eq!(entries.get(1).unwrap().stats.total_wins, 3);

    assert_eq!(entries.get(2).unwrap().user, user_c);
    assert_eq!(entries.get(2).unwrap().stats.total_wins, 2);

    // next_cursor should be Some (the last entry's address)
    assert!(page.1.is_some());
}

#[test]
fn test_leaderboard_ordered_by_streak() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let user_c = Address::generate(&env);

    // Alice wins 3 times (best streak 3)
    env.as_contract(&contract_id, || {
        for _ in 0..3 {
            VirtualTokenContract::_update_stats_win(&env, user_a.clone()).unwrap();
        }
    });

    // Bob wins 2 times, loses, wins 1 time (best streak 2)
    env.as_contract(&contract_id, || {
        for _ in 0..2 {
            VirtualTokenContract::_update_stats_win(&env, user_b.clone()).unwrap();
        }
        VirtualTokenContract::_update_stats_loss(&env, user_b.clone()).unwrap();
        VirtualTokenContract::_update_stats_win(&env, user_b.clone()).unwrap();
    });

    // Carol wins 4 times (best streak 4)
    env.as_contract(&contract_id, || {
        for _ in 0..4 {
            VirtualTokenContract::_update_stats_win(&env, user_c.clone()).unwrap();
        }
    });

    // Must create a round so the leaderboard collector can find active participants.
    client.create_round(&1_0000000u128, &None);

    // Query streak leaderboard with cursor = None
    let page = client.get_leaderboard_by_streak(&None, &10).unwrap();
    assert_eq!(page.0.len(), 3);

    let entries = page.0;
    // Expected order: Carol (4), Alice (3), Bob (2)
    assert_eq!(entries.get(0).unwrap().user, user_c);
    assert_eq!(entries.get(0).unwrap().stats.best_streak, 4);

    assert_eq!(entries.get(1).unwrap().user, user_a);
    assert_eq!(entries.get(1).unwrap().stats.best_streak, 3);

    assert_eq!(entries.get(2).unwrap().user, user_b);
    assert_eq!(entries.get(2).unwrap().stats.best_streak, 2);
}

#[test]
fn test_leaderboard_cursor_pagination() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let user_c = Address::generate(&env);

    // Give Alice 3 wins, Bob 5 wins, Carol 2 wins
    env.as_contract(&contract_id, || {
        for _ in 0..3 {
            VirtualTokenContract::_update_stats_win(&env, user_a.clone()).unwrap();
        }
        for _ in 0..5 {
            VirtualTokenContract::_update_stats_win(&env, user_b.clone()).unwrap();
        }
        for _ in 0..2 {
            VirtualTokenContract::_update_stats_win(&env, user_c.clone()).unwrap();
        }
    });

    client.create_round(&1_0000000u128, &None);

    // First page: cursor = None, limit = 1 -> should return Bob (5 wins)
    let page0 = client.get_leaderboard_by_wins(&None, &1).unwrap();
    assert_eq!(page0.0.len(), 1);
    assert_eq!(page0.0.get(0).unwrap().user, user_b);
    assert!(page0.1.is_some());

    // Second page: cursor from page0 -> should return Alice (3 wins)
    let page1 = client.get_leaderboard_by_wins(&page0.1, &1).unwrap();
    assert_eq!(page1.0.len(), 1);
    assert_eq!(page1.0.get(0).unwrap().user, user_a);
    assert!(page1.1.is_some());

    // Third page: cursor from page1 -> should return Carol (2 wins)
    let page2 = client.get_leaderboard_by_wins(&page1.1, &1).unwrap();
    assert_eq!(page2.0.len(), 1);
    assert_eq!(page2.0.get(0).unwrap().user, user_c);
    // Last page: next_cursor should be None (exhausted)
    assert!(page2.1.is_none());

    // Fourth page: using last cursor -> empty
    let page3 = client.get_leaderboard_by_wins(&page2.1, &1).unwrap();
    assert_eq!(page3.0.len(), 0);
    assert!(page3.1.is_none());
}

#[test]
fn test_leaderboard_deterministic_tie_breaking() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    // Both users get 3 wins
    env.as_contract(&contract_id, || {
        for _ in 0..3 {
            VirtualTokenContract::_update_stats_win(&env, user_a.clone()).unwrap();
            VirtualTokenContract::_update_stats_win(&env, user_b.clone()).unwrap();
        }
    });

    client.create_round(&1_0000000u128, &None);

    // Query wins leaderboard with cursor = None
    let page = client.get_leaderboard_by_wins(&None, &10).unwrap();
    assert_eq!(page.0.len(), 2);

    // Expected order: sorted by Address ascending
    let first = page.0.get(0).unwrap().user;
    let second = page.0.get(1).unwrap().user;
    assert!(first < second);
}

#[test]
fn test_leaderboard_limit_capped_at_max_page_size() {
    let env = Env::default();
    env.cost_estimate().budget().reset_unlimited();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Create a round so we can place bets (participants appear on the leaderboard).
    client.create_round(&1_0000000u128, &None);

    // Create many users with varying wins, all placing bets in the active round.
    // All will be on the participant list and thus visible to the leaderboard.
    env.as_contract(&contract_id, || {
        for _ in 0..50 {
            let u = Address::generate(&env);
            VirtualTokenContract::_update_stats_win(&env, u.clone()).unwrap();
            VirtualTokenContract::_update_stats_win(&env, u.clone()).unwrap();
        }
    });

    // Request 150 entries — limit is capped at MAX_PAGE_SIZE (100).
    let page = client.get_leaderboard_by_wins(&None, &150);
    // With 50 participants, we should get at most 50 results, all ≤ 100.
    assert!(page.0.len() <= 100, "result count should be capped at 100");
    // Request with valid limit = 100 (MAX_PAGE_SIZE) should succeed.
    let page = client.get_leaderboard_by_wins(&None, &100).unwrap();
    // With 50 participants, we should get 50 results.
    assert_eq!(page.0.len(), 50);

    // Request 150 entries (exceeds MAX_PAGE_SIZE) — should be rejected with PageSizeExceeded error.
    let result = client.get_leaderboard_by_wins(&None, &150);
    assert!(result.is_err());
}

#[test]
fn test_leaderboard_empty_when_no_users() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // No round created, no users → empty leaderboard
    let page = client.get_leaderboard_by_wins(&None, &10).unwrap();
    assert_eq!(page.0.len(), 0);
    assert!(page.1.is_none());

    let page2 = client.get_leaderboard_by_streak(&None, &10).unwrap();
    assert_eq!(page2.0.len(), 0);
    assert!(page2.1.is_none());
}

#[test]
fn test_leaderboard_zero_limit_is_empty() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    client.create_round(&1_0000000u128, &None);

    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.place_bet(&user, &10_0000000i128, &crate::types::BetSide::Up);

    env.as_contract(&contract_id, || {
        VirtualTokenContract::_update_stats_win(&env, user.clone()).unwrap();
    });

    // limit = 0 → should be rejected with PageSizeExceeded error
    let result = client.get_leaderboard_by_wins(&None, &0);
    assert!(result.is_err());
}

#[test]
fn test_leaderboard_rejects_over_limit_adversarial() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    client.create_round(&1_0000000u128, &None);

    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.place_bet(&user, &10_0000000i128, &crate::types::BetSide::Up);

    env.as_contract(&contract_id, || {
        VirtualTokenContract::_update_stats_win(&env, user.clone()).unwrap();
    });

    // Adversarial: request with limit = MAX_PAGE_SIZE + 1 should be rejected
    let result_wins = client.get_leaderboard_by_wins(&None, &101);
    assert!(result_wins.is_err(), "Should reject limit > MAX_PAGE_SIZE (100)");

    let result_streak = client.get_leaderboard_by_streak(&None, &1000);
    assert!(result_streak.is_err(), "Should reject limit > MAX_PAGE_SIZE (100)");

    // Valid request with exactly MAX_PAGE_SIZE should succeed
    let valid_result = client.get_leaderboard_by_wins(&None, &100);
    assert!(valid_result.is_ok(), "Should accept limit == MAX_PAGE_SIZE (100)");
}
