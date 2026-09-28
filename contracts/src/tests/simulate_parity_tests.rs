// SPDX-License-Identifier: MIT
//! Parity tests ensuring `simulate_payout` matches live settlement for all fee models and policies.
#![cfg(test)]
extern crate std;

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, OraclePayload, RoundMode, UserOutcomeType};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

/// Parity test for UpDown mode with protocol fees: `simulate_payout` predictions must
/// match actual `resolve_round` payouts exactly, accounting for fee deduction from
/// both losing and winning pools depending on fee incidence model.
#[test]
fn test_simulate_payout_updown_with_fees_matches_resolve() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Set protocol fee to 1% (100 bps)
    client.set_protocol_fee_bps(&Some(100));

    client.create_round(&10000, &Some(0)); // UpDown mode

    // Alice bets 500 on Up, Bob bets 300 on Down
    client.place_bet(&alice, &500, &BetSide::Up);
    client.place_bet(&bob, &300, &BetSide::Down);

    let final_price = 10100u128; // Price goes up, Alice wins

    // Snapshot the simulation before resolving
    let sim = client.simulate_payout(&final_price);
    assert_eq!(sim.mode, RoundMode::UpDown);
    assert_eq!(sim.pool_up, 500);
    assert_eq!(sim.pool_down, 300);
    assert!(sim.fee_amount > 0, "Fee should be deducted from total pot");

    let mut sim_alice = sim.outcomes.get(0).unwrap();
    let mut sim_bob = sim.outcomes.get(1).unwrap();
    if sim_alice.user == bob {
        core::mem::swap(&mut sim_alice, &mut sim_bob);
    }

    assert_eq!(sim_alice.outcome, UserOutcomeType::Win);
    assert_eq!(sim_bob.outcome, UserOutcomeType::Loss);
    assert!(sim_alice.payout > 0, "Winner should receive a payout");
    assert_eq!(sim_bob.payout, 0, "Loser should receive nothing");

    // Now resolve and confirm exact parity
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

    // Payouts must match exactly
    assert_eq!(client.get_pending_winnings(&alice), sim_alice.payout,
        "Winner's pending winnings must match simulation");
    assert_eq!(client.get_pending_winnings(&bob), sim_bob.payout,
        "Loser's pending winnings must match simulation");

    // Conservation: payout + fee should equal total pot
    let total_payout = sim_alice.payout + sim_bob.payout;
    assert_eq!(total_payout + sim.fee_amount, 800i128,
        "Payouts and fees must conserve total pot value");
}

/// Parity test for UpDown mode with one-sided pool: `simulate_payout` must predict
/// refund of all stakes when exactly one pool is empty, matching the live settlement.
#[test]
fn test_simulate_payout_updown_one_sided_matches_resolve() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&10000, &Some(0)); // UpDown mode

    // All three bet on Up side only (one-sided pool)
    client.place_bet(&alice, &100, &BetSide::Up);
    client.place_bet(&bob, &200, &BetSide::Up);
    client.place_bet(&charlie, &150, &BetSide::Up);

    let final_price = 10500u128; // Price goes up (doesn't matter for one-sided)

    // Simulate should predict refunds for all
    let sim = client.simulate_payout(&final_price);
    assert_eq!(sim.outcomes.len(), 3);
    for i in 0..sim.outcomes.len() {
        let outcome = sim.outcomes.get(i).unwrap();
        assert_eq!(outcome.outcome, UserOutcomeType::Refund,
            "One-sided round must refund all stakes");
        assert_eq!(outcome.payout, outcome.stake,
            "Refund payout must equal original stake");
    }

    // Now resolve and verify parity
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

    // All should receive their original stakes back
    assert_eq!(client.get_pending_winnings(&alice), 100i128);
    assert_eq!(client.get_pending_winnings(&bob), 200i128);
    assert_eq!(client.get_pending_winnings(&charlie), 150i128);
}

/// Parity test for UpDown with price unchanged (tie): `simulate_payout` must predict
/// refunds for all participants, matching the live settlement.
#[test]
fn test_simulate_payout_updown_tie_matches_resolve() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    let start_price = 10000u128;
    client.create_round(&start_price, &Some(0)); // UpDown mode

    client.place_bet(&alice, &500, &BetSide::Up);
    client.place_bet(&bob, &300, &BetSide::Down);

    // Simulate at exactly the start price (tie)
    let sim = client.simulate_payout(&start_price);
    assert_eq!(sim.outcomes.len(), 2);

    let mut sim_alice = sim.outcomes.get(0).unwrap();
    let mut sim_bob = sim.outcomes.get(1).unwrap();
    if sim_alice.user == bob {
        core::mem::swap(&mut sim_alice, &mut sim_bob);
    }

    assert_eq!(sim_alice.outcome, UserOutcomeType::Refund);
    assert_eq!(sim_bob.outcome, UserOutcomeType::Refund);
    assert_eq!(sim_alice.payout, 500i128, "Alice should get her stake back");
    assert_eq!(sim_bob.payout, 300i128, "Bob should get his stake back");

    // Verify with real settlement
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    let round = client.get_active_round().unwrap();
    client.resolve_round(&OraclePayload {
        price: start_price,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert_eq!(client.get_pending_winnings(&alice), 500i128);
    assert_eq!(client.get_pending_winnings(&bob), 300i128);
}

/// Parity test for Precision mode with Equal payout policy: `simulate_payout` must
/// predict equal distribution among winners, matching live settlement exactly.
#[test]
fn test_simulate_payout_precision_equal_policy_matches_resolve() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    // 0 = Equal (default)
    client.set_precision_payout_policy(&0u32);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&10000, &Some(1)); // Precision mode

    let final_price = 10250u128;
    // Alice: stake 100, exact match (winner)
    // Bob: stake 300, exact match (winner)
    // Charlie: stake 200, way off (loser)
    client.place_precision_prediction(&alice, &100, &final_price);
    client.place_precision_prediction(&bob, &300, &final_price);
    client.place_precision_prediction(&charlie, &200, &9000);

    let sim = client.simulate_payout(&final_price);
    assert_eq!(sim.outcomes.len(), 3);
    assert_eq!(sim.precision_total_stake, 600i128);

    // With Equal policy, both winners should get equal shares
    let mut sim_alice = sim.outcomes.get(0).unwrap();
    let mut sim_bob = sim.outcomes.get(1).unwrap();
    let mut sim_charlie = sim.outcomes.get(2).unwrap();

    // Re-order to match addresses
    if sim_alice.user == bob || sim_alice.user == charlie {
        for i in 0..sim.outcomes.len() {
            let outcome = sim.outcomes.get(i).unwrap();
            if outcome.user == alice {
                sim_alice = outcome;
            } else if outcome.user == bob {
                sim_bob = outcome;
            } else if outcome.user == charlie {
                sim_charlie = outcome;
            }
        }
    }

    assert_eq!(sim_alice.outcome, UserOutcomeType::Win);
    assert_eq!(sim_bob.outcome, UserOutcomeType::Win);
    assert_eq!(sim_charlie.outcome, UserOutcomeType::Loss);
    
    // Both winners should get equal payouts (600 / 2 = 300)
    assert_eq!(sim_alice.payout, 300i128, "Alice should get equal share");
    assert_eq!(sim_bob.payout, 300i128, "Bob should get equal share");
    assert_eq!(sim_charlie.payout, 0i128, "Charlie (loser) should get nothing");

    // Verify parity with live settlement
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

    assert_eq!(client.get_pending_winnings(&alice), 300i128);
    assert_eq!(client.get_pending_winnings(&bob), 300i128);
    assert_eq!(client.get_pending_winnings(&charlie), 0i128);
}
