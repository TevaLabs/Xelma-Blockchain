// SPDX-License-Identifier: MIT
//! Tests for the Leaderboard Read APIs and bounded index synchronization.

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

    // Query wins leaderboard
    let page = client.get_leaderboard_by_wins(&0, &10);
    assert_eq!(page.len(), 3);

    // Expected order: Bob (5), Alice (3), Carol (2)
    assert_eq!(page.get(0).unwrap().user, user_b);
    assert_eq!(page.get(0).unwrap().stats.total_wins, 5);

    assert_eq!(page.get(1).unwrap().user, user_a);
    assert_eq!(page.get(1).unwrap().stats.total_wins, 3);

    assert_eq!(page.get(2).unwrap().user, user_c);
    assert_eq!(page.get(2).unwrap().stats.total_wins, 2);
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

    // Query streak leaderboard
    let page = client.get_leaderboard_by_streak(&0, &10);
    assert_eq!(page.len(), 3);

    // Expected order: Carol (4), Alice (3), Bob (2)
    assert_eq!(page.get(0).unwrap().user, user_c);
    assert_eq!(page.get(0).unwrap().stats.best_streak, 4);

    assert_eq!(page.get(1).unwrap().user, user_a);
    assert_eq!(page.get(1).unwrap().stats.best_streak, 3);

    assert_eq!(page.get(2).unwrap().user, user_b);
    assert_eq!(page.get(2).unwrap().stats.best_streak, 2);
}

#[test]
fn test_leaderboard_pagination() {
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

    // Query wins leaderboard: offset 1, limit 1 (should return Alice)
    let page = client.get_leaderboard_by_wins(&1, &1);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().user, user_a);

    // Query streak leaderboard: offset 2, limit 1 (should return Carol)
    // Streak order: Bob (5), Alice (3), Carol (2).
    let page_streak = client.get_leaderboard_by_streak(&2, &2);
    assert_eq!(page_streak.len(), 1);
    assert_eq!(page_streak.get(0).unwrap().user, user_c);
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

    // Query wins leaderboard
    let page = client.get_leaderboard_by_wins(&0, &10);
    assert_eq!(page.len(), 2);

    // Expected order: sorted by Address ascending
    let first = page.get(0).unwrap().user;
    let second = page.get(1).unwrap().user;
    assert!(first < second);
}

#[test]
fn test_leaderboard_bounded_index() {
    let env = Env::default();
    env.cost_estimate().budget().reset_unlimited();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Generate 105 users, give each some wins
    // 100 users get 2 wins.
    // 5 users get 1 win.
    // Only the top 100 should be in the leaderboard.
    let mut top_users = Vec::new(&env);
    env.as_contract(&contract_id, || {
        for _ in 0..100 {
            let u = Address::generate(&env);
            VirtualTokenContract::_update_stats_win(&env, u.clone()).unwrap();
            VirtualTokenContract::_update_stats_win(&env, u.clone()).unwrap();
            top_users.push_back(u);
        }
    });

    let mut low_users = Vec::new(&env);
    env.as_contract(&contract_id, || {
        for _ in 0..5 {
            let u = Address::generate(&env);
            VirtualTokenContract::_update_stats_win(&env, u.clone()).unwrap();
            low_users.push_back(u);
        }
    });

    let page = client.get_leaderboard_by_wins(&0, &150);
    // Bounded to 100 entries
    assert_eq!(page.len(), 100);

    // Verify none of the low_users (with 1 win) are in the leaderboard
    for entry in page.iter() {
        assert_eq!(entry.stats.total_wins, 2);
        for low_u in low_users.iter() {
            assert_ne!(entry.user, low_u);
        }
    }
}
