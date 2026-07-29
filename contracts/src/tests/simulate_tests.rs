#![cfg(test)]
extern crate std;

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, RoundMode, UserOutcomeType};
use soroban_sdk::testutils::Address as _;
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
