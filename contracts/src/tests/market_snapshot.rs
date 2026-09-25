// SPDX-License-Identifier: MIT
//! Tests for the single-read `MarketSnapshot` query API (Issue #280).

use super::config_helpers::{apply_protocol_fee_bps, apply_windows};
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, FeeModel};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup() -> (Env, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, client)
}

#[test]
fn test_market_snapshot_empty_round_semantics() {
    let (_env, client) = setup();

    let snapshot = client.get_market_snapshot();

    // No active round: phase and pool_stats are both empty.
    assert!(snapshot.phase.is_empty());
    assert!(snapshot.pool_stats.is_empty());

    // Contract-wide config fields are always populated, matching the
    // default values reported by their individual getters.
    assert_eq!(snapshot.bet_window_ledgers, client.get_bet_window_ledgers());
    assert_eq!(snapshot.run_window_ledgers, client.get_run_window_ledgers());
    assert_eq!(
        snapshot.close_buffer_ledgers,
        client.get_close_buffer_ledgers()
    );
    assert_eq!(snapshot.protocol_fee_bps, client.get_protocol_fee_bps());
    assert_eq!(snapshot.protocol_fee_bps, None); // fees disabled by default
    assert_eq!(snapshot.fee_model, client.get_fee_model());
    assert_eq!(snapshot.fee_model, FeeModel::FeeOnPot); // default
}

#[test]
fn test_market_snapshot_active_round_matches_individual_getters() {
    let (env, client) = setup();

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_0000000, &Some(0)); // UpDown mode
    client.place_bet(&alice, &500, &BetSide::Up);
    client.place_bet(&bob, &300, &BetSide::Down);

    let snapshot = client.get_market_snapshot();

    assert_eq!(snapshot.phase.get(0), Some(client.get_round_phase()));
    assert_eq!(snapshot.pool_stats.get(0), client.get_round_pool_stats());

    let pool_stats = snapshot
        .pool_stats
        .get(0)
        .expect("active round should have pool stats");
    assert_eq!(pool_stats.total_up_stake, 500);
    assert_eq!(pool_stats.total_down_stake, 300);
    assert_eq!(pool_stats.up_participant_count, 1);
    assert_eq!(pool_stats.down_participant_count, 1);
}

#[test]
fn test_market_snapshot_reflects_configured_windows() {
    let (env, client) = setup();
    apply_windows(&env, &client, 20, 40);

    let snapshot = client.get_market_snapshot();

    assert_eq!(snapshot.bet_window_ledgers, 20);
    assert_eq!(snapshot.run_window_ledgers, 40);
    assert_eq!(snapshot.bet_window_ledgers, client.get_bet_window_ledgers());
    assert_eq!(snapshot.run_window_ledgers, client.get_run_window_ledgers());
}

#[test]
fn test_market_snapshot_reflects_configured_fee() {
    let (env, client) = setup();
    apply_protocol_fee_bps(&env, &client, Some(250));

    let snapshot = client.get_market_snapshot();

    assert_eq!(snapshot.protocol_fee_bps, Some(250));
    assert_eq!(snapshot.protocol_fee_bps, client.get_protocol_fee_bps());
}
