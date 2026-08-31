// SPDX-License-Identifier: MIT
//! Tests for seasonal leaderboards: season-scoped stats independent of the
//! lifetime leaderboard, archive-on-reset, scoped queries, boundaries, and
//! TTL rent-safety for every season-scoped storage key.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{DataKeyCore, DataKeyExt, DataKeyScoped};
use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

fn setup_contract(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    (client, contract_id, admin, oracle)
}

fn win(env: &Env, contract_id: &Address, user: &Address) {
    env.as_contract(contract_id, || {
        VirtualTokenContract::_update_stats_win(env, user.clone()).unwrap();
    });
}

fn lose(env: &Env, contract_id: &Address, user: &Address) {
    env.as_contract(contract_id, || {
        VirtualTokenContract::_update_stats_loss(env, user.clone()).unwrap();
    });
}

// ─── Defaults ─────────────────────────────────────────────────────────────────

#[test]
fn test_season_defaults_to_one_and_starts_empty() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    assert_eq!(client.get_current_season_id(), 1);
    assert_eq!(client.get_season_leaderboard_by_wins(&1, &0, &10).len(), 0);
    assert_eq!(
        client.get_season_leaderboard_by_streak(&1, &0, &10).len(),
        0
    );

    let alice = Address::generate(&env);
    let stats = client.get_season_user_stats(&1, &alice);
    assert_eq!(stats.total_wins, 0);
    assert_eq!(stats.total_losses, 0);
    assert_eq!(stats.current_streak, 0);
    assert_eq!(stats.best_streak, 0);
}

// ─── Season-scoped stats independent of lifetime stats ────────────────────────

#[test]
fn test_season_stats_track_alongside_lifetime_stats() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);
    let alice = Address::generate(&env);

    win(&env, &contract_id, &alice);
    win(&env, &contract_id, &alice);
    lose(&env, &contract_id, &alice);

    let lifetime = client.get_user_stats(&alice);
    let season = client.get_season_user_stats(&1, &alice);

    assert_eq!(lifetime.total_wins, 2);
    assert_eq!(lifetime.total_losses, 1);
    assert_eq!(lifetime.best_streak, 2);
    assert_eq!(lifetime.current_streak, 0);

    // With only season 1 ever active, season-scoped and lifetime counters
    // agree exactly — the two ledgers are independent, not linked.
    assert_eq!(season.total_wins, lifetime.total_wins);
    assert_eq!(season.total_losses, lifetime.total_losses);
    assert_eq!(season.best_streak, lifetime.best_streak);
    assert_eq!(season.current_streak, lifetime.current_streak);
}

// ─── Ordering and pagination ──────────────────────────────────────────────────

#[test]
fn test_season_leaderboard_ordering_and_pagination() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    for _ in 0..3 {
        win(&env, &contract_id, &alice);
    }
    for _ in 0..5 {
        win(&env, &contract_id, &bob);
    }
    for _ in 0..2 {
        win(&env, &contract_id, &carol);
    }

    let page = client.get_season_leaderboard_by_wins(&1, &0, &10);
    assert_eq!(page.len(), 3);
    assert_eq!(page.get(0).unwrap().user, bob);
    assert_eq!(page.get(0).unwrap().wins, 5);
    assert_eq!(page.get(1).unwrap().user, alice);
    assert_eq!(page.get(1).unwrap().wins, 3);
    assert_eq!(page.get(2).unwrap().user, carol);
    assert_eq!(page.get(2).unwrap().wins, 2);

    // offset 1, limit 1 -> Alice only.
    let page = client.get_season_leaderboard_by_wins(&1, &1, &1);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap().user, alice);
}

// ─── Archive on reset, scoped queries, lifetime untouched ──────────────────────

/// Covers all three acceptance criteria at once: archives are preserved,
/// queries stay scoped to the season they name, and the lifetime leaderboard
/// is never wiped by a season reset.
#[test]
fn test_reset_season_archives_scopes_queries_and_preserves_lifetime_history() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    for _ in 0..3 {
        win(&env, &contract_id, &alice);
    }
    for _ in 0..5 {
        win(&env, &contract_id, &bob);
    }

    let new_season_id = client.reset_leaderboard_season();
    assert_eq!(new_season_id, 2);
    assert_eq!(client.get_current_season_id(), 2);

    // Archive preserved: season 1's frozen ranking is retrievable directly...
    let archive = client
        .get_season_archive(&1)
        .expect("season 1 must be archived");
    assert_eq!(archive.season_id, 1);
    assert_eq!(archive.participant_count, 2);
    assert_eq!(archive.wins.len(), 2);
    assert_eq!(archive.wins.get(0).unwrap().user, bob);
    assert_eq!(archive.wins.get(0).unwrap().wins, 5);
    assert_eq!(archive.wins.get(1).unwrap().user, alice);
    assert_eq!(archive.wins.get(1).unwrap().wins, 3);

    // ...and transparently through the same paginated query used for the
    // live leaderboard, now routed to the frozen snapshot.
    let season1_page = client.get_season_leaderboard_by_wins(&1, &0, &10);
    assert_eq!(season1_page.len(), 2);
    assert_eq!(season1_page.get(0).unwrap().user, bob);
    assert_eq!(season1_page.get(0).unwrap().wins, 5);

    // Queries scoped: the new active season starts completely empty.
    let season2_page = client.get_season_leaderboard_by_wins(&2, &0, &10);
    assert_eq!(season2_page.len(), 0);

    // A fresh win in season 2 only affects season 2's ranking and stats —
    // season 1's frozen numbers for the same user are untouched.
    win(&env, &contract_id, &alice);
    assert_eq!(client.get_season_user_stats(&2, &alice).total_wins, 1);
    assert_eq!(client.get_season_user_stats(&1, &alice).total_wins, 3);

    let season2_page = client.get_season_leaderboard_by_wins(&2, &0, &10);
    assert_eq!(season2_page.len(), 1);
    assert_eq!(season2_page.get(0).unwrap().user, alice);
    assert_eq!(season2_page.get(0).unwrap().wins, 1);

    // Lifetime history is never wiped by the reset: it reflects every win
    // across both seasons combined (Bob=5, Alice=3+1=4).
    let (items, _cursor) = client.get_leaderboard_by_wins(&None, &10);
    assert_eq!(items.len(), 2);
    assert_eq!(items.get(0).unwrap().user, bob);
    assert_eq!(items.get(0).unwrap().stats.total_wins, 5);
    assert_eq!(items.get(1).unwrap().user, alice);
    assert_eq!(items.get(1).unwrap().stats.total_wins, 4);
}

// ─── Boundary tests ────────────────────────────────────────────────────────────

#[test]
fn test_season_boundary_unknown_and_zero_season_ids_return_empty() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    // Season 0 never existed (seasons start at 1).
    assert_eq!(client.get_season_leaderboard_by_wins(&0, &0, &10).len(), 0);
    assert_eq!(
        client.get_season_leaderboard_by_streak(&0, &0, &10).len(),
        0
    );
    assert!(client.get_season_archive(&0).is_none());

    // A season far beyond anything ever reset.
    assert_eq!(
        client.get_season_leaderboard_by_wins(&999, &0, &10).len(),
        0
    );
    assert!(client.get_season_archive(&999).is_none());
}

#[test]
fn test_season_boundary_reset_with_zero_participants() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    // Nobody has ever won or lost — reset must still succeed and advance.
    let new_season_id = client.reset_leaderboard_season();
    assert_eq!(new_season_id, 2);

    let archive = client
        .get_season_archive(&1)
        .expect("empty season must still archive");
    assert_eq!(archive.participant_count, 0);
    assert_eq!(archive.wins.len(), 0);
    assert_eq!(archive.streak.len(), 0);
}

#[test]
fn test_season_boundary_pagination_offset_at_and_beyond_total() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    win(&env, &contract_id, &alice);

    // offset == total -> empty, not an error.
    assert_eq!(client.get_season_leaderboard_by_wins(&1, &1, &10).len(), 0);
    // offset beyond total -> empty.
    assert_eq!(client.get_season_leaderboard_by_wins(&1, &50, &10).len(), 0);
    // limit larger than remaining entries returns only what exists.
    assert_eq!(client.get_season_leaderboard_by_wins(&1, &0, &100).len(), 1);
}

#[test]
fn test_season_boundary_consecutive_resets_keep_independent_archives() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    // Season 1: nobody plays.
    assert_eq!(client.reset_leaderboard_season(), 2);

    // Season 2: Bob wins once.
    let bob = Address::generate(&env);
    win(&env, &contract_id, &bob);
    assert_eq!(client.reset_leaderboard_season(), 3);

    assert_eq!(client.get_current_season_id(), 3);

    // Both prior archives remain independently correct — the second reset
    // must not have overwritten or merged into the first.
    let archive1 = client.get_season_archive(&1).unwrap();
    assert_eq!(archive1.participant_count, 0);
    assert_eq!(archive1.wins.len(), 0);

    let archive2 = client.get_season_archive(&2).unwrap();
    assert_eq!(archive2.participant_count, 1);
    assert_eq!(archive2.wins.get(0).unwrap().user, bob);
    assert_eq!(archive2.wins.get(0).unwrap().wins, 1);

    // Season 3 (active) is empty.
    assert_eq!(client.get_season_leaderboard_by_wins(&3, &0, &10).len(), 0);
}

// ─── TTL rent-safety ────────────────────────────────────────────────────────
//
// A season is meant to be a durable demo/history surface — an archived
// season's rankings must not silently expire from persistent storage.
// Every season-scoped key extends its TTL to `TTL_BUMP_AMOUNT`
// (~30 days) on both write and read, and the admin `batch_touch_ttl`
// allowlist additionally covers the account-level (non-per-user) season
// keys for proactive rent renewal ahead of long idle periods.

const TTL_BUMP_AMOUNT: u32 = 518_400;

#[test]
fn test_season_keys_ttl_extended_on_write() {
    let env = Env::default();
    let (_client, contract_id, _admin, _oracle) = setup_contract(&env);
    let alice = Address::generate(&env);

    win(&env, &contract_id, &alice);

    // Bounded-index keys and the season-id key are extended on every write.
    env.as_contract(&contract_id, || {
        let season_id_ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKeyCore::Ext(DataKeyExt::SeasonId));
        assert!(season_id_ttl >= TTL_BUMP_AMOUNT);

        let wins_ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardWins));
        assert!(wins_ttl >= TTL_BUMP_AMOUNT);

        let streak_ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardStreak));
        assert!(streak_ttl >= TTL_BUMP_AMOUNT);

        let user_stats_ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKeyScoped::SeasonUserStats(1, alice.clone()));
        assert!(user_stats_ttl >= TTL_BUMP_AMOUNT);
    });
}

/// The frozen `SeasonArchive` written by a reset is the permanent historical
/// record for a past season — it must get the same TTL bump as any live key
/// at write time, not just when someone happens to query it later.
#[test]
fn test_season_archive_ttl_extended_on_write_and_read() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);
    let alice = Address::generate(&env);
    win(&env, &contract_id, &alice);

    client.reset_leaderboard_season();

    env.as_contract(&contract_id, || {
        let ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKeyScoped::SeasonArchive(1));
        assert!(ttl >= TTL_BUMP_AMOUNT, "archive TTL bumped at write time");
    });

    // A later read (e.g. a demo dashboard polling old seasons) must refresh
    // the TTL again rather than relying solely on the write-time bump.
    client.get_season_archive(&1);
    env.as_contract(&contract_id, || {
        let ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKeyScoped::SeasonArchive(1));
        assert!(ttl >= TTL_BUMP_AMOUNT, "archive TTL refreshed on read");
    });
}

/// The operator-facing `batch_touch_ttl` allowlist must cover the
/// account-level season keys so an operator can proactively renew them
/// ahead of a long idle period without needing a live write/read to do it.
#[test]
fn test_batch_touch_ttl_covers_season_keys() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);
    let alice = Address::generate(&env);
    win(&env, &contract_id, &alice);

    let keys: Vec<DataKeyCore> = Vec::from_array(
        &env,
        [
            DataKeyCore::Ext(DataKeyExt::SeasonId),
            DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardWins),
            DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardStreak),
        ],
    );

    let touched = client.batch_touch_ttl(&keys);
    assert_eq!(touched, 3, "all three season keys exist and are allowlisted");

    for key in keys.iter() {
        let ttl = env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&key));
        assert!(ttl >= TTL_BUMP_AMOUNT);
    }
}
