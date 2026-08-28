// SPDX-License-Identifier: MIT
#![cfg(test)]
extern crate std;

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, OraclePayload, RoundMode, UserOutcomeType};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

#[test]
fn test_simulate_updown() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    client.initialize(&admin, &oracle);

    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    client.mint_initial(&p1);
    client.mint_initial(&p2);

    client.create_round(&10000, &Some(0)); // UpDown Mode

    // Place bets
    client.place_bet(&p1, &500, &BetSide::Up);
    client.place_bet(&p2, &300, &BetSide::Down);

    // Simulate final price > 10000 (Up wins)
    let sim_up = client.simulate_payout(&10100);
    assert_eq!(sim_up.mode, RoundMode::UpDown);
    assert_eq!(sim_up.pool_up, 500);
    assert_eq!(sim_up.pool_down, 300);

    // There are two outcomes
    assert_eq!(sim_up.outcomes.len(), 2);

    // p1 wins, gets 500 (stake) + 300 (loser's) - fees if applicable (assuming 0% here as default)
    // wait, fee is 0 by default.
    let mut p1_outcome = sim_up.outcomes.get(0).unwrap();
    let mut p2_outcome = sim_up.outcomes.get(1).unwrap();
    if p1_outcome.user == p2 {
        core::mem::swap(&mut p1_outcome, &mut p2_outcome);
    }

    assert_eq!(p1_outcome.outcome, UserOutcomeType::Win);
    assert_eq!(p1_outcome.payout, 800);

    assert_eq!(p2_outcome.outcome, UserOutcomeType::Loss);
    assert_eq!(p2_outcome.payout, 0);

    // Simulate tie
    let sim_tie = client.simulate_payout(&10000);
    let o1_tie = sim_tie.outcomes.get(0).unwrap();
    let o2_tie = sim_tie.outcomes.get(1).unwrap();
    assert_eq!(o1_tie.outcome, UserOutcomeType::Refund);
    assert_eq!(o2_tie.outcome, UserOutcomeType::Refund);
}

/// `simulate_payout` must never mutate storage: calling it (repeatedly, with
/// different hypothetical prices) must not change the active round, pools,
/// or any user's real pending winnings.
#[test]
fn test_simulate_payout_does_not_mutate_state() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    client.mint_initial(&p1);
    client.mint_initial(&p2);

    client.create_round(&10000, &Some(0));
    client.place_bet(&p1, &500, &BetSide::Up);
    client.place_bet(&p2, &300, &BetSide::Down);

    let round_before = client.get_active_round().unwrap();

    client.simulate_payout(&10100);
    client.simulate_payout(&9900);
    client.simulate_payout(&10000);

    let round_after = client.get_active_round().unwrap();
    assert_eq!(round_before, round_after, "simulate_payout must not mutate the active round");
    assert_eq!(client.get_pending_winnings(&p1), 0);
    assert_eq!(client.get_pending_winnings(&p2), 0);
}

/// Parity test (issue #279): a Precision round configured for `StakeWeighted`
/// payouts must produce identical per-user amounts from `simulate_payout` and
/// from the real `resolve_round` settlement path. Prior to this fix,
/// `simulate_payout` hardcoded an equal split and would drift from the
/// configured policy.
#[test]
fn test_simulate_payout_precision_stake_weighted_matches_resolve() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    // 1 = StakeWeighted (see `PrecisionPayoutPolicy`).
    client.set_precision_payout_policy(&1u32);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&10000, &Some(1)); // Precision mode

    // Both predict the exact final price, so both tie for the win; their
    // stakes differ, so Equal vs StakeWeighted payouts must differ.
    let final_price: u128 = 10250;
    client.place_precision_prediction(&alice, &100, &final_price);
    client.place_precision_prediction(&bob, &300, &final_price);

    // Snapshot the preview before resolving.
    let sim = client.simulate_payout(&final_price);
    assert_eq!(sim.mode, RoundMode::Precision);
    assert_eq!(sim.outcomes.len(), 2);

    let mut sim_alice = sim.outcomes.get(0).unwrap();
    let mut sim_bob = sim.outcomes.get(1).unwrap();
    if sim_alice.user == bob {
        core::mem::swap(&mut sim_alice, &mut sim_bob);
    }
    assert_eq!(sim_alice.outcome, UserOutcomeType::Win);
    assert_eq!(sim_bob.outcome, UserOutcomeType::Win);
    // Stake-weighted: alice (100) should receive less than bob (300).
    assert!(sim_alice.payout < sim_bob.payout);
    // The full pot (minus fee) must be distributed with no leftover/dust.
    assert_eq!(
        sim_alice.payout + sim_bob.payout,
        sim.precision_total_stake - sim.fee_amount
    );

    // Now actually resolve and confirm real payouts match the preview exactly.
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    let round = client.get_active_round().unwrap();
    client.resolve_round(&OraclePayload {
        price: final_price,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert_eq!(client.get_pending_winnings(&alice), sim_alice.payout);
    assert_eq!(client.get_pending_winnings(&bob), sim_bob.payout);
}
