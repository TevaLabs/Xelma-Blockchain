// SPDX-License-Identifier: MIT
//! `get_round_pool_stats` must reflect early cash-outs immediately (Issue #695).
//!
//! `cash_out_early` removes the user's whole stake from the pool, their
//! position and their participant entry. The live pool stats and the market
//! snapshot are derived from that state, so liquidity must never read stale
//! after a cash-out.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, DataKeyCore, RoundPoolStats};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    client.initialize(&admin, &oracle);
    (client, contract_id)
}

fn enable_cashout(env: &Env, contract_id: &Address, bps: u32) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::EarlyCashoutBps, &bps);
    });
}

fn stats(client: &VirtualTokenContractClient<'_>) -> RoundPoolStats {
    client.get_round_pool_stats().expect("active round")
}

/// Two Up bettors (100 + 300) and one Down bettor (200), running phase,
/// 10% cash-out penalty.
fn running_round<'a>(env: &'a Env) -> (VirtualTokenContractClient<'a>, Address, Address, Address) {
    let (client, contract_id) = setup(env);
    let alice = Address::generate(env);
    let bob = Address::generate(env);
    let carol = Address::generate(env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&carol);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &100, &BetSide::Up);
    client.place_bet(&bob, &300, &BetSide::Up);
    client.place_bet(&carol, &200, &BetSide::Down);
    enable_cashout(env, &contract_id, 1_000);

    // Running phase: betting closed, round not ended.
    env.ledger().with_mut(|li| li.sequence_number = 7);
    (client, alice, bob, carol)
}

#[test]
fn pool_stats_drop_by_full_stake_after_up_cashout() {
    let env = Env::default();
    let (client, alice, _bob, _carol) = running_round(&env);

    let before = stats(&client);
    assert_eq!(before.total_up_stake, 400);
    assert_eq!(before.total_down_stake, 200);
    assert_eq!(before.up_participant_count, 2);
    assert_eq!(before.down_participant_count, 1);
    assert_eq!(before.up_stake_ratio_bps, 6_666);
    assert_eq!(before.down_stake_ratio_bps, 3_333);

    client.cash_out_early(&alice);

    // The full 100 stake leaves the pool (90 to the user, 10 forfeited as
    // fee); the Down side is untouched.
    let after = stats(&client);
    assert_eq!(after.round_id, before.round_id);
    assert_eq!(after.total_up_stake, 300);
    assert_eq!(after.total_down_stake, 200);
    assert_eq!(after.up_participant_count, 1);
    assert_eq!(after.down_participant_count, 1);
    assert_eq!(after.up_stake_ratio_bps, 6_000);
    assert_eq!(after.down_stake_ratio_bps, 4_000);
}

#[test]
fn pool_stats_drop_after_down_cashout() {
    let env = Env::default();
    let (client, _alice, _bob, carol) = running_round(&env);

    client.cash_out_early(&carol);

    let after = stats(&client);
    assert_eq!(after.total_up_stake, 400);
    assert_eq!(after.total_down_stake, 0);
    assert_eq!(after.up_participant_count, 2);
    assert_eq!(after.down_participant_count, 0);
    assert_eq!(after.up_stake_ratio_bps, 10_000);
    assert_eq!(after.down_stake_ratio_bps, 0);
}

#[test]
fn pool_stats_track_each_successive_cashout_and_empty_pool_reads_zero() {
    let env = Env::default();
    let (client, alice, bob, carol) = running_round(&env);

    client.cash_out_early(&alice);
    assert_eq!(stats(&client).total_up_stake, 300);

    client.cash_out_early(&bob);
    let s = stats(&client);
    assert_eq!(s.total_up_stake, 0);
    assert_eq!(s.up_participant_count, 0);
    assert_eq!(s.total_down_stake, 200);
    assert_eq!(s.down_stake_ratio_bps, 10_000);

    // Last participant out: the round is still active but the pool is empty,
    // and no ratio or count is left over.
    client.cash_out_early(&carol);
    let s = stats(&client);
    assert_eq!(s.total_up_stake, 0);
    assert_eq!(s.total_down_stake, 0);
    assert_eq!(s.up_participant_count, 0);
    assert_eq!(s.down_participant_count, 0);
    assert_eq!(s.up_stake_ratio_bps, 0);
    assert_eq!(s.down_stake_ratio_bps, 0);
}

#[test]
fn market_snapshot_pool_stats_match_after_cashout() {
    let env = Env::default();
    let (client, alice, _bob, _carol) = running_round(&env);

    client.cash_out_early(&alice);

    let snapshot = client.get_market_snapshot();
    assert_eq!(snapshot.pool_stats.len(), 1);
    let from_snapshot = snapshot.pool_stats.get(0).unwrap();
    let direct = stats(&client);
    assert_eq!(from_snapshot.total_up_stake, direct.total_up_stake);
    assert_eq!(from_snapshot.total_down_stake, direct.total_down_stake);
    assert_eq!(
        from_snapshot.up_participant_count,
        direct.up_participant_count
    );
    assert_eq!(from_snapshot.up_stake_ratio_bps, direct.up_stake_ratio_bps);
}

#[test]
fn rejected_cashout_leaves_pool_stats_unchanged() {
    let env = Env::default();
    let (client, alice, _bob, _carol) = running_round(&env);

    client.cash_out_early(&alice);
    let before = stats(&client);

    // A second cash-out by the same user has no position left and must not
    // touch the pool.
    assert!(client.try_cash_out_early(&alice).is_err());
    let after = stats(&client);
    assert_eq!(after.total_up_stake, before.total_up_stake);
    assert_eq!(after.up_participant_count, before.up_participant_count);
}
