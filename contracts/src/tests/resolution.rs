//! Tests for round resolution and winnings distribution.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, DataKey, OraclePayload, PrecisionPrediction, Round, UserPosition};
use crate::types::{RoundArchiveStatus, RoundMode};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, Map, TryIntoVal, Vec,
};

#[test]
fn test_resolve_round_price_unchanged() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Create a round with start price 1.5 XLM
    let start_price: u128 = 1_5000000;
    client.create_round(&start_price, &None);

    // Manually set up some test positions using env.as_contract
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Give users initial balances
    client.mint_initial(&user1);
    client.mint_initial(&user2);

    // Manually create positions for testing using as_contract
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(
            user1.clone(),
            UserPosition {
                amount: 100_0000000,
                side: BetSide::Up,
            },
        );
        positions.set(
            user2.clone(),
            UserPosition {
                amount: 50_0000000,
                side: BetSide::Down,
            },
        );

        // Store positions in UpDownPositions (new storage location)
        env.storage()
            .persistent()
            .set(&DataKey::UpDownPositions, &positions);

        // Update round pools to match positions
        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .unwrap();
        round.pool_up = 100_0000000;
        round.pool_down = 50_0000000;
        env.storage()
            .persistent()
            .set(&DataKey::ActiveRound, &round);
    });

    // Get balances before resolution
    let user1_balance_before = client.balance(&user1);
    let user2_balance_before = client.balance(&user2);

    // Advance ledger to allow resolution (default run window is 12)
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    // Resolve with SAME price (unchanged)
    client.resolve_round(&OraclePayload {
        price: start_price,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Check pending winnings (not claimed yet)
    assert_eq!(client.get_pending_winnings(&user1), 100_0000000);
    assert_eq!(client.get_pending_winnings(&user2), 50_0000000);

    // Claim winnings
    let claimed1 = client.claim_winnings(&user1);
    let claimed2 = client.claim_winnings(&user2);

    assert_eq!(claimed1, 100_0000000);
    assert_eq!(claimed2, 50_0000000);

    // Both users should get their bets back
    assert_eq!(client.balance(&user1), user1_balance_before + 100_0000000);
    assert_eq!(client.balance(&user2), user2_balance_before + 50_0000000);

    // Round should be cleared
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_resolve_round_price_went_up() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Create a round with start price 1.0 XLM
    let start_price: u128 = 1_0000000;
    client.create_round(&start_price, &None);

    // Set up test users
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    // Give users initial balances
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    // Create positions using as_contract
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(
            alice.clone(),
            UserPosition {
                amount: 100_0000000,
                side: BetSide::Up,
            },
        );
        positions.set(
            bob.clone(),
            UserPosition {
                amount: 200_0000000,
                side: BetSide::Up,
            },
        );
        positions.set(
            charlie.clone(),
            UserPosition {
                amount: 150_0000000,
                side: BetSide::Down,
            },
        );

        env.storage()
            .persistent()
            .set(&DataKey::UpDownPositions, &positions);

        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .unwrap();
        round.pool_up = 300_0000000;
        round.pool_down = 150_0000000;
        env.storage()
            .persistent()
            .set(&DataKey::ActiveRound, &round);
    });

    let alice_before = client.balance(&alice);
    let bob_before = client.balance(&bob);
    let charlie_before = client.balance(&charlie);

    // Advance ledger to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    // Resolve with HIGHER price (1.5 XLM - price went UP)
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Check pending winnings
    assert_eq!(client.get_pending_winnings(&alice), 150_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 300_0000000);
    assert_eq!(client.get_pending_winnings(&charlie), 0); // Lost

    // Check stats: Alice and Bob won, Charlie lost
    let alice_stats = client.get_user_stats(&alice);
    assert_eq!(alice_stats.total_wins, 1);
    assert_eq!(alice_stats.current_streak, 1);

    let charlie_stats = client.get_user_stats(&charlie);
    assert_eq!(charlie_stats.total_losses, 1);
    assert_eq!(charlie_stats.current_streak, 0);

    // Claim winnings
    client.claim_winnings(&alice);
    client.claim_winnings(&bob);

    assert_eq!(client.balance(&alice), alice_before + 150_0000000);
    assert_eq!(client.balance(&bob), bob_before + 300_0000000);
    assert_eq!(client.balance(&charlie), charlie_before); // No change (lost)
}

#[test]
fn test_resolve_round_price_went_down() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Create a round with start price 2.0 XLM
    let start_price: u128 = 2_0000000;
    client.create_round(&start_price, &None);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Create positions using as_contract
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(
            alice.clone(),
            UserPosition {
                amount: 200_0000000,
                side: BetSide::Down,
            },
        );
        positions.set(
            bob.clone(),
            UserPosition {
                amount: 100_0000000,
                side: BetSide::Up,
            },
        );

        env.storage()
            .persistent()
            .set(&DataKey::UpDownPositions, &positions);

        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .unwrap();
        round.pool_up = 100_0000000;
        round.pool_down = 200_0000000;
        env.storage()
            .persistent()
            .set(&DataKey::ActiveRound, &round);
    });

    let alice_before = client.balance(&alice);
    let bob_before = client.balance(&bob);

    // Advance ledger to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    // Resolve with LOWER price (1.0 XLM - price went DOWN)
    client.resolve_round(&OraclePayload {
        price: 1_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Check pending winnings
    assert_eq!(client.get_pending_winnings(&alice), 300_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 0);

    // Alice wins: 200 + (200/200) * 100 = 200 + 100 = 300
    client.claim_winnings(&alice);

    assert_eq!(client.balance(&alice), alice_before + 300_0000000);
    assert_eq!(client.balance(&bob), bob_before); // No change (lost)
}

#[test]
fn test_claim_winnings_when_none() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    env.mock_all_auths();

    // Try to claim with no pending winnings
    let claimed = client.claim_winnings(&user);
    assert_eq!(claimed, 0);
}

#[test]
fn test_user_stats_tracking() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Initial stats should be all zeros
    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 0);
    assert_eq!(stats.total_losses, 0);
    assert_eq!(stats.current_streak, 0);
    assert_eq!(stats.best_streak, 0);

    // Simulate a win
    env.as_contract(&contract_id, || {
        VirtualTokenContract::_update_stats_win(&env, alice.clone()).unwrap();
    });

    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 1);
    assert_eq!(stats.current_streak, 1);
    assert_eq!(stats.best_streak, 1);

    // Another win - streak increases
    env.as_contract(&contract_id, || {
        VirtualTokenContract::_update_stats_win(&env, alice.clone()).unwrap();
    });

    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 2);
    assert_eq!(stats.current_streak, 2);
    assert_eq!(stats.best_streak, 2);

    // A loss - streak resets
    env.as_contract(&contract_id, || {
        VirtualTokenContract::_update_stats_loss(&env, alice.clone()).unwrap();
    });

    let stats = client.get_user_stats(&alice);
    assert_eq!(stats.total_wins, 2);
    assert_eq!(stats.total_losses, 1);
    assert_eq!(stats.current_streak, 0); // Reset
    assert_eq!(stats.best_streak, 2); // Best remains
}

#[test]
fn test_resolve_round_without_active_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Try to resolve without creating a round - should return error
    let result = client.try_resolve_round(&OraclePayload {
        price: 1_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

// ============================================================================
// PRECISION MODE RESOLUTION TESTS
// ============================================================================

#[test]
fn test_resolve_precision_closest_guess_wins() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Create Precision mode round starting at 2000
    client.create_round(&2000, &Some(1));

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    // Manually create precision predictions using as_contract
    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);

        // Alice guesses 2297 (closest to actual 2298 - diff 1)
        predictions.set(
            alice.clone(),
            PrecisionPrediction {
                user: alice.clone(),
                predicted_price: 2297,
                amount: 100_0000000,
            },
        );

        // Bob guesses 2300 (diff 2 from actual 2298)
        predictions.set(
            bob.clone(),
            PrecisionPrediction {
                user: bob.clone(),
                predicted_price: 2300,
                amount: 150_0000000,
            },
        );

        // Charlie guesses 2500 (far off - diff 202)
        predictions.set(
            charlie.clone(),
            PrecisionPrediction {
                user: charlie.clone(),
                predicted_price: 2500,
                amount: 50_0000000,
            },
        );

        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    // Advance ledger to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve with actual price 2298
    client.resolve_round(&OraclePayload {
        price: 2298,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Alice should win the entire pot (100 + 150 + 50 = 300)
    assert_eq!(client.get_pending_winnings(&alice), 300_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 0);
    assert_eq!(client.get_pending_winnings(&charlie), 0);

    // Check stats
    let alice_stats = client.get_user_stats(&alice);
    assert_eq!(alice_stats.total_wins, 1);
    assert_eq!(alice_stats.current_streak, 1);

    let bob_stats = client.get_user_stats(&bob);
    assert_eq!(bob_stats.total_losses, 1);
    assert_eq!(bob_stats.current_streak, 0);

    // Round should be cleared
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_resolve_precision_tie_splits_pot() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Create Precision mode round
    client.create_round(&2000, &Some(1));

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    // Create tied predictions
    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);

        // Alice guesses 2100 (diff 100 from actual 2200)
        predictions.set(
            alice.clone(),
            PrecisionPrediction {
                user: alice.clone(),
                predicted_price: 2100,
                amount: 100_0000000,
            },
        );

        // Bob guesses 2300 (diff 100 from actual 2200) - TIE with Alice
        predictions.set(
            bob.clone(),
            PrecisionPrediction {
                user: bob.clone(),
                predicted_price: 2300,
                amount: 150_0000000,
            },
        );

        // Charlie guesses 2500 (diff 300 from actual 2200)
        predictions.set(
            charlie.clone(),
            PrecisionPrediction {
                user: charlie.clone(),
                predicted_price: 2500,
                amount: 50_0000000,
            },
        );

        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    // Advance ledger
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve with actual price 2200
    client.resolve_round(&OraclePayload {
        price: 2200,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Total pot is 300, split evenly between Alice and Bob (150 each)
    assert_eq!(client.get_pending_winnings(&alice), 150_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 150_0000000);
    assert_eq!(client.get_pending_winnings(&charlie), 0);

    // Both Alice and Bob should have win stats
    let alice_stats = client.get_user_stats(&alice);
    assert_eq!(alice_stats.total_wins, 1);

    let bob_stats = client.get_user_stats(&bob);
    assert_eq!(bob_stats.total_wins, 1);

    let charlie_stats = client.get_user_stats(&charlie);
    assert_eq!(charlie_stats.total_losses, 1);
}

#[test]
fn test_resolve_precision_exact_match() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    client.create_round(&2000, &Some(1));

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);

        // Alice guesses exactly right (diff 0)
        predictions.set(
            alice.clone(),
            PrecisionPrediction {
                user: alice.clone(),
                predicted_price: 2250,
                amount: 100_0000000,
            },
        );

        // Bob is off by 50
        predictions.set(
            bob.clone(),
            PrecisionPrediction {
                user: bob.clone(),
                predicted_price: 2200,
                amount: 100_0000000,
            },
        );

        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Alice guessed exactly right
    client.resolve_round(&OraclePayload {
        price: 2250,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    assert_eq!(client.get_pending_winnings(&alice), 200_0000000); // Wins entire pot
    assert_eq!(client.get_pending_winnings(&bob), 0);
}

#[test]
fn test_resolve_precision_no_predictions() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    // Create Precision mode round with no predictions
    client.create_round(&2000, &Some(1));

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve with no predictions - should succeed without errors
    client.resolve_round(&OraclePayload {
        price: 2250,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Round should be cleared
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_resolve_precision_three_way_tie() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    client.create_round(&2000, &Some(1));

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);

        // All three tie with diff of 10
        predictions.set(
            alice.clone(),
            PrecisionPrediction {
                user: alice.clone(),
                predicted_price: 2190,
                amount: 100_0000000,
            },
        );

        predictions.set(
            bob.clone(),
            PrecisionPrediction {
                user: bob.clone(),
                predicted_price: 2210,
                amount: 150_0000000,
            },
        );

        predictions.set(
            charlie.clone(),
            PrecisionPrediction {
                user: charlie.clone(),
                predicted_price: 2210,
                amount: 150_0000000,
            },
        );

        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Actual price 2200 - Alice diff 10, Bob diff 10, Charlie diff 10
    client.resolve_round(&OraclePayload {
        price: 2200,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Total pot is 400, split 3 ways = 133.33... each
    // With remainder policy: Alice gets 133 + 1 (remainder), Bob and Charlie get 133
    let pot_per_winner = 400_0000000 / 3; // 133_3333333
    let remainder = 400_0000000 % 3; // 1
    assert_eq!(
        client.get_pending_winnings(&alice),
        pot_per_winner + remainder
    ); // 133_3333334
    assert_eq!(client.get_pending_winnings(&bob), pot_per_winner); // 133_3333333
    assert_eq!(client.get_pending_winnings(&charlie), pot_per_winner); // 133_3333333
}

#[test]
fn test_resolve_precision_single_prediction() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    client.create_round(&2000, &Some(1));

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);

        predictions.set(
            alice.clone(),
            PrecisionPrediction {
                user: alice.clone(),
                predicted_price: 2300,
                amount: 100_0000000,
            },
        );

        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Single prediction always wins
    client.resolve_round(&OraclePayload {
        price: 2500,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    assert_eq!(client.get_pending_winnings(&alice), 100_0000000);
}

#[test]
fn test_resolve_precision_large_differences() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    client.create_round(&100_0000, &Some(1));

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);

        // Very large price predictions
        predictions.set(
            alice.clone(),
            PrecisionPrediction {
                user: alice.clone(),
                predicted_price: 1_0000,
                amount: 100_0000000,
            },
        );

        predictions.set(
            bob.clone(),
            PrecisionPrediction {
                user: bob.clone(),
                predicted_price: 9_9999,
                amount: 100_0000000,
            },
        );

        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Actual price is 1_0001 - Alice is closest (diff 1 vs Bob's diff 8_9998)
    client.resolve_round(&OraclePayload {
        price: 1_0001,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    assert_eq!(client.get_pending_winnings(&alice), 200_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 0);
}

#[test]
fn test_precision_remainder_3way_tie_uneven_pot() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    client.create_round(&1_0000, &Some(1));

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    // Total pot: 100 vXLM, 3 winners = 33.33... each
    // Expected: Alice 34 (33 + 1 remainder), Bob 33, Charlie 33
    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);

        predictions.set(
            alice.clone(),
            PrecisionPrediction {
                user: alice.clone(),
                predicted_price: 2_0000,
                amount: 30_0000000,
            },
        );

        predictions.set(
            bob.clone(),
            PrecisionPrediction {
                user: bob.clone(),
                predicted_price: 2_0000,
                amount: 30_0000000,
            },
        );

        predictions.set(
            charlie.clone(),
            PrecisionPrediction {
                user: charlie.clone(),
                predicted_price: 2_0000,
                amount: 40_0000000,
            },
        );

        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // All tied with perfect guess
    client.resolve_round(&OraclePayload {
        price: 2_0000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Total pot: 100_0000000, Winner count: 3
    // payout_per_winner = 100_0000000 / 3 = 33_3333333
    // remainder = 100_0000000 % 3 = 1
    // Alice (first winner): 33_3333333 + 1 = 33_3333334
    // Bob: 33_3333333
    // Charlie: 33_3333333
    let pot_per_winner = 100_0000000 / 3;
    let remainder = 100_0000000 % 3;
    assert_eq!(
        client.get_pending_winnings(&alice),
        pot_per_winner + remainder
    ); // 33_3333334
    assert_eq!(client.get_pending_winnings(&bob), pot_per_winner); // 33_3333333
    assert_eq!(client.get_pending_winnings(&charlie), pot_per_winner); // 33_3333333

    // Verify full pot accounting: 33_3333334 + 33_3333333 + 33_3333333 = 100_0000000 ✓
}

#[test]
fn test_precision_remainder_5way_tie() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    client.create_round(&1_0000, &Some(1));

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);
    let user4 = Address::generate(&env);
    let user5 = Address::generate(&env);

    client.mint_initial(&user1);
    client.mint_initial(&user2);
    client.mint_initial(&user3);
    client.mint_initial(&user4);
    client.mint_initial(&user5);

    // Total pot: 103 vXLM, 5 winners = 20.6 each
    // Expected: user1 23 (20 + 3 remainder), others 20 each
    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);

        predictions.set(
            user1.clone(),
            PrecisionPrediction {
                user: user1.clone(),
                predicted_price: 5_0000,
                amount: 23_0000000,
            },
        );

        predictions.set(
            user2.clone(),
            PrecisionPrediction {
                user: user2.clone(),
                predicted_price: 5_0000,
                amount: 20_0000000,
            },
        );

        predictions.set(
            user3.clone(),
            PrecisionPrediction {
                user: user3.clone(),
                predicted_price: 5_0000,
                amount: 20_0000000,
            },
        );

        predictions.set(
            user4.clone(),
            PrecisionPrediction {
                user: user4.clone(),
                predicted_price: 5_0000,
                amount: 20_0000000,
            },
        );

        predictions.set(
            user5.clone(),
            PrecisionPrediction {
                user: user5.clone(),
                predicted_price: 5_0000,
                amount: 20_0000000,
            },
        );

        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // All tied
    client.resolve_round(&OraclePayload {
        price: 5_0000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Total pot: 103_0000000, Winner count: 5
    // payout_per_winner = 103_0000000 / 5 = 20_6000000
    // remainder = 103_0000000 % 5 = 3_0000000
    // user1 (first winner): 20_6000000 + 3_0000000 = 23_6000000
    // Others: 20_6000000 each
    let pot_per_winner = 103_0000000 / 5;
    let remainder = 103_0000000 % 5;
    assert_eq!(
        client.get_pending_winnings(&user1),
        pot_per_winner + remainder
    ); // 23_6000000
    assert_eq!(client.get_pending_winnings(&user2), pot_per_winner); // 20_6000000
    assert_eq!(client.get_pending_winnings(&user3), pot_per_winner); // 20_6000000
    assert_eq!(client.get_pending_winnings(&user4), pot_per_winner); // 20_6000000
    assert_eq!(client.get_pending_winnings(&user5), pot_per_winner); // 20_6000000

    // Verify full pot accounting: 23_6000000 + 20_6000000*4 = 103_0000000 ✓
}

#[test]
fn test_precision_no_remainder() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);

    client.create_round(&1_0000, &Some(1));

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Total pot: 100 vXLM, 2 winners = 50 each (perfect division)
    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);

        predictions.set(
            alice.clone(),
            PrecisionPrediction {
                user: alice.clone(),
                predicted_price: 3_0000,
                amount: 50_0000000,
            },
        );

        predictions.set(
            bob.clone(),
            PrecisionPrediction {
                user: bob.clone(),
                predicted_price: 3_0000,
                amount: 50_0000000,
            },
        );

        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 3_0000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Total pot: 100, Winner count: 2
    // payout_per_winner = 100 / 2 = 50
    // remainder = 100 % 2 = 0
    // Both get exactly 50
    assert_eq!(client.get_pending_winnings(&alice), 50_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 50_0000000);
}

#[test]
fn test_round_resolved_event_emitted() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);

    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // Advance ledger to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve round
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Verify resolved event was emitted
    let events = env.events().all();
    let resolved_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("resolved"))
    });

    assert!(
        resolved_event.is_some(),
        "Round resolved event should be emitted"
    );
}

#[test]
fn test_claim_winnings_event_emitted() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);

    // Manually set up position and winnings
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(
            user.clone(),
            UserPosition {
                amount: 100_0000000,
                side: BetSide::Up,
            },
        );
        env.storage()
            .persistent()
            .set(&DataKey::UpDownPositions, &positions);

        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .unwrap();
        round.pool_up = 100_0000000;
        env.storage()
            .persistent()
            .set(&DataKey::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve - price went up so user wins
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Claim winnings
    client.claim_winnings(&user);

    // Verify claim event was emitted
    let events = env.events().all();
    let claim_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("claim"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("winnings"))
    });

    assert!(
        claim_event.is_some(),
        "Claim winnings event should be emitted"
    );
}

#[test]
fn test_no_claim_event_when_no_winnings() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    // Count events before claim
    let _events_before = env.events().all().len();

    // Try to claim when no winnings available
    let claimed = client.claim_winnings(&user);
    assert_eq!(claimed, 0);

    // Count claim events after
    let events_after = env.events().all();
    let claim_events = events_after
        .iter()
        .filter(|e| {
            let (_contract, topics, _data) = e;
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("claim"))
                && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("winnings"))
        })
        .count();

    assert_eq!(
        claim_events, 0,
        "Should not emit claim event when no winnings"
    );
}

// ============================================================================
// PRECISION MODE — DETERMINISM AND CONSERVATION TESTS (Issue #71)
// ============================================================================

/// Verifies that resolving the same precision-mode state in two independent
/// environments produces byte-identical pending-winnings for every participant.
#[test]
fn test_precision_payout_deterministic_same_inputs() {
    fn run_scenario(
        pot_a: i128,
        pot_b: i128,
        pot_c: i128,
        final_price: u128,
    ) -> (i128, i128, i128) {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin, &oracle);
        client.create_round(&1_0000, &Some(1));

        env.as_contract(&contract_id, || {
            let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);
            predictions.set(
                alice.clone(),
                PrecisionPrediction {
                    user: alice.clone(),
                    predicted_price: 5_0000,
                    amount: pot_a,
                },
            );
            predictions.set(
                bob.clone(),
                PrecisionPrediction {
                    user: bob.clone(),
                    predicted_price: 5_0000,
                    amount: pot_b,
                },
            );
            predictions.set(
                charlie.clone(),
                PrecisionPrediction {
                    user: charlie.clone(),
                    predicted_price: 5_0000,
                    amount: pot_c,
                },
            );
            env.storage()
                .persistent()
                .set(&DataKey::PrecisionPositions, &predictions);
        });

        env.ledger().with_mut(|li| {
            li.sequence_number = 12;
        });
        client.resolve_round(&OraclePayload {
            price: final_price,
            timestamp: env.ledger().timestamp(),
            round_id: 0,
            nonce: 1u64,
            network_id: env.ledger().network_id(),
            contract_addr: contract_id.clone(),
        });

        (
            client.get_pending_winnings(&alice),
            client.get_pending_winnings(&bob),
            client.get_pending_winnings(&charlie),
        )
    }

    let run1 = run_scenario(30_0000000, 40_0000000, 30_0000000, 5_0000);
    let run2 = run_scenario(30_0000000, 40_0000000, 30_0000000, 5_0000);

    assert_eq!(
        run1, run2,
        "Identical inputs must produce identical payout vectors"
    );

    let total_pot: i128 = 30_0000000 + 40_0000000 + 30_0000000;
    let sum = run1.0 + run1.1 + run1.2;
    assert_eq!(
        sum, total_pot,
        "Sum of payouts must equal total pot exactly"
    );
}

/// Verifies that the sum of all pending winnings equals the total pot exactly
/// (conservation) for a two-way tie with an indivisible remainder.
#[test]
fn test_precision_payout_conservation_two_way_tie_remainder() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000, &Some(1));

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Total pot 101 — not evenly divisible by 2
    let total_pot: i128 = 101_0000001;
    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);
        predictions.set(
            alice.clone(),
            PrecisionPrediction {
                user: alice.clone(),
                predicted_price: 3_0000,
                amount: 51_0000001,
            },
        );
        predictions.set(
            bob.clone(),
            PrecisionPrediction {
                user: bob.clone(),
                predicted_price: 3_0000,
                amount: 50_0000000,
            },
        );
        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    client.resolve_round(&OraclePayload {
        price: 3_0000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    let alice_payout = client.get_pending_winnings(&alice);
    let bob_payout = client.get_pending_winnings(&bob);

    // Neither winner receives a negative amount
    assert!(alice_payout >= 0);
    assert!(bob_payout >= 0);

    // Conservation: payouts sum to exactly the total pot
    assert_eq!(alice_payout + bob_payout, total_pot);

    // Remainder (1) goes to exactly one winner
    let per_winner = total_pot / 2;
    let remainder = total_pot % 2;
    assert_eq!(alice_payout + bob_payout, per_winner * 2 + remainder);
}

// ============================================================================
// MINIMUM PARTICIPANTS THRESHOLD TESTS
// ============================================================================

#[test]
fn test_min_participants_blocks_settlement_updown() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user1 = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user1);
    client.create_round(&1_0000000, &None);

    client.place_bet(&user1, &100_0000000, &BetSide::Up);
    client.set_min_participants(&Some(2u32));

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let balance_before = client.balance(&user1);

    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Stake refunded to pending winnings, not claimed yet
    assert_eq!(client.get_pending_winnings(&user1), 100_0000000);
    assert_eq!(client.balance(&user1), balance_before);
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_min_participants_allows_settlement_at_threshold() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user1);
    client.mint_initial(&user2);
    client.create_round(&1_0000000, &None);

    client.place_bet(&user1, &100_0000000, &BetSide::Up);
    client.place_bet(&user2, &100_0000000, &BetSide::Down);
    client.set_min_participants(&Some(2u32));

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve with higher price → user1 (Up) wins the pot
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    assert_eq!(client.get_pending_winnings(&user1), 200_0000000);
    assert_eq!(client.get_pending_winnings(&user2), 0);
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_min_participants_fallback_refunds_precision_mode() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user1 = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user1);
    client.create_round(&2000, &Some(1));

    client.place_precision_prediction(&user1, &100_0000000, &2100u128);
    client.set_min_participants(&Some(2u32));

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 2200,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Precision bet refunded
    assert_eq!(client.get_pending_winnings(&user1), 100_0000000);
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_min_participants_fallback_event_emitted() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user1 = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user1);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user1, &100_0000000, &BetSide::Up);
    client.set_min_participants(&Some(3u32));

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    let events = env.events().all();
    let fallback_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("fallback"))
    });
    assert!(
        fallback_event.is_some(),
        "Fallback event must be emitted when min-participants threshold is not met"
    );
}

#[test]
fn test_set_min_participants_validation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Zero is invalid
    let result = client.try_set_min_participants(&Some(0u32));
    assert_eq!(result, Err(Ok(ContractError::InvalidMinParticipants)));

    // Exceeding max is invalid
    let result = client.try_set_min_participants(&Some(10_001u32));
    assert_eq!(result, Err(Ok(ContractError::InvalidMinParticipants)));

    // Valid value accepted
    client.set_min_participants(&Some(2u32));
    assert_eq!(client.get_min_participants(), Some(2u32));

    // None removes the threshold
    client.set_min_participants(&None);
    assert_eq!(client.get_min_participants(), None);
}

#[test]
fn test_no_min_participants_threshold_resolves_normally() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user1 = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user1);
    client.create_round(&1_0000000, &None);

    // Place a single bet with no min threshold configured
    client.place_bet(&user1, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Should resolve normally (single participant wins their own pool with no opposing side)
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Price went up but winning_pool (Up) = 100, losing_pool (Down) = 0 → payout = 100 + 0 = 100
    assert_eq!(client.get_pending_winnings(&user1), 100_0000000);
    assert_eq!(client.get_active_round(), None);
}

/// Verifies conservation and non-overflow for a large tie set (10 winners).
#[test]
fn test_precision_payout_conservation_large_tie_set() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.create_round(&1_0000, &Some(1));

    let u0 = Address::generate(&env);
    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let u3 = Address::generate(&env);
    let u4 = Address::generate(&env);
    let u5 = Address::generate(&env);
    let u6 = Address::generate(&env);
    let u7 = Address::generate(&env);
    let u8 = Address::generate(&env);
    let u9 = Address::generate(&env);

    // 7 bets of 11 + 3 bets of 10 = total pot 107 * 10_000_000
    let amounts: [i128; 10] = [11, 11, 11, 11, 11, 11, 11, 10, 10, 10];
    let total_pot: i128 = amounts.iter().sum::<i128>() * 10_000_000;

    env.as_contract(&contract_id, || {
        let users = [
            u0.clone(),
            u1.clone(),
            u2.clone(),
            u3.clone(),
            u4.clone(),
            u5.clone(),
            u6.clone(),
            u7.clone(),
            u8.clone(),
            u9.clone(),
        ];
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);
        for (user, &amount) in users.iter().zip(amounts.iter()) {
            predictions.set(
                user.clone(),
                PrecisionPrediction {
                    user: user.clone(),
                    predicted_price: 7_0000,
                    amount: amount * 10_000_000,
                },
            );
        }
        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    client.resolve_round(&OraclePayload {
        price: 7_0000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    let users = [u0, u1, u2, u3, u4, u5, u6, u7, u8, u9];
    let mut sum: i128 = 0;
    for user in &users {
        let payout = client.get_pending_winnings(user);
        assert!(payout >= 0);
        sum += payout;
    }

    assert_eq!(
        sum, total_pot,
        "Sum of all payouts must equal total pot exactly"
    );
}

#[test]
fn test_precision_commit_reveal_resolution_payout_with_unrevealed_participants() {
    use soroban_sdk::xdr::ToXdr;
    use soroban_sdk::{Bytes, BytesN};

    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_0000000, &Some(1));

    // Alice commits and reveals guess of 2000
    let price_alice = 2000u128;
    let salt_alice = BytesN::from_array(&env, &[1; 32]);
    let mut preimage_alice = Bytes::new(&env);
    preimage_alice.append(&price_alice.to_xdr(&env));
    preimage_alice.append(&salt_alice.clone().to_xdr(&env));
    let hash_alice = env.crypto().sha256(&preimage_alice);
    let committed_hash_alice: BytesN<32> = hash_alice.into();
    client.commit_prediction(&alice, &committed_hash_alice, &100_0000000);

    // Bob commits but does NOT reveal
    let price_bob = 2200u128;
    let salt_bob = BytesN::from_array(&env, &[2; 32]);
    let mut preimage_bob = Bytes::new(&env);
    preimage_bob.append(&price_bob.to_xdr(&env));
    preimage_bob.append(&salt_bob.clone().to_xdr(&env));
    let hash_bob = env.crypto().sha256(&preimage_bob);
    let committed_hash_bob: BytesN<32> = hash_bob.into();
    client.commit_prediction(&bob, &committed_hash_bob, &150_0000000);

    // Move to reveal window
    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });

    // Only Alice reveals
    client.reveal_prediction(&alice, &price_alice, &salt_alice);

    // Move past end of round to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve round with actual price 2050
    client.resolve_round(&OraclePayload {
        price: 2050,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Total pot is 250 (Alice 100 + Bob 150)
    // Alice is the only revealed participant, so she wins the entire pot
    assert_eq!(client.get_pending_winnings(&alice), 250_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 0);
}

#[test]
fn test_precision_remainder_goes_to_lexicographically_lowest_winner() {
    use soroban_sdk::xdr::ToXdr;
    use soroban_sdk::{Bytes, BytesN};

    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user_a);
    client.mint_initial(&user_b);

    client.create_round(&1_0000000, &Some(1));

    // Determine which address is lexicographically lowest
    let (lowest_user, other_user, bet_lowest, bet_other) = if user_a < user_b {
        (
            user_a.clone(),
            user_b.clone(),
            100_0000001i128,
            100_0000000i128,
        )
    } else {
        (
            user_b.clone(),
            user_a.clone(),
            100_0000001i128,
            100_0000000i128,
        )
    };

    // Both commit the same guess (2000)
    let price = 2000u128;
    let salt_a = BytesN::from_array(&env, &[1; 32]);
    let mut preimage_a = Bytes::new(&env);
    preimage_a.append(&price.to_xdr(&env));
    preimage_a.append(&salt_a.clone().to_xdr(&env));
    let hash_a = env.crypto().sha256(&preimage_a);
    let committed_hash_a: BytesN<32> = hash_a.into();
    client.commit_prediction(&lowest_user, &committed_hash_a, &bet_lowest);

    let salt_b = BytesN::from_array(&env, &[2; 32]);
    let mut preimage_b = Bytes::new(&env);
    preimage_b.append(&price.to_xdr(&env));
    preimage_b.append(&salt_b.clone().to_xdr(&env));
    let hash_b = env.crypto().sha256(&preimage_b);
    let committed_hash_b: BytesN<32> = hash_b.into();
    client.commit_prediction(&other_user, &committed_hash_b, &bet_other);

    // Move to reveal window
    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });

    client.reveal_prediction(&lowest_user, &price, &salt_a);
    client.reveal_prediction(&other_user, &price, &salt_b);

    // Move to resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve
    client.resolve_round(&OraclePayload {
        price: 2000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Total pot = 200_0000001
    // split = 200_0000001 / 2 = 100_0000000
    // remainder = 1
    // The lexicographically lowest winner (lowest_user) must get: split + remainder = 100_0000001
    // The other winner (other_user) must get: 100_0000000
    assert_eq!(client.get_pending_winnings(&lowest_user), 100_0000001);
    assert_eq!(client.get_pending_winnings(&other_user), 100_0000000);
}

fn resolve_active_round(
    client: &VirtualTokenContractClient,
    env: &Env,
    final_price: u128,
    nonce: u64,
) -> u64 {
    let round = client.get_active_round().unwrap();
    let round_id = round.round_id;
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });
    client.resolve_round(&OraclePayload {
        price: final_price,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
    });
    round_id
}

#[test]
fn test_archived_round_after_resolve_matches_settlement() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    let start_price: u128 = 1_0000000;
    client.create_round(&start_price, &None);
    client.place_bet(&alice, &50_0000000, &BetSide::Up);
    client.place_bet(&bob, &50_0000000, &BetSide::Down);

    let final_price: u128 = 2_0000000;
    let round_id = resolve_active_round(&client, &env, final_price, 1);

    assert!(client.get_active_round().is_none());
    let archived = client
        .get_archived_round(&round_id)
        .expect("resolved round must be archived");
    assert_eq!(archived.round_id, round_id);
    assert_eq!(archived.price_start, start_price);
    assert_eq!(archived.price_final, final_price);
    assert_eq!(archived.mode, RoundMode::UpDown);
    assert_eq!(archived.status, RoundArchiveStatus::Resolved);
    assert_eq!(archived.pool_up, 50_0000000);
    assert_eq!(archived.pool_down, 50_0000000);
    assert_eq!(archived.participant_count, 2);
    assert_eq!(archived.settled_at_ledger, 12); // default run window end for round created at ledger 0

    assert_eq!(client.get_pending_winnings(&alice), 100_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 0);
}

#[test]
fn test_archived_round_after_cancel() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    let start_price: u128 = 1_5000000;
    client.create_round(&start_price, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    let round_id = client.get_active_round().unwrap().round_id;

    client.cancel_round(&1u32);

    let archived = client
        .get_archived_round(&round_id)
        .expect("cancelled round must be archived");
    assert_eq!(archived.status, RoundArchiveStatus::Cancelled);
    assert_eq!(archived.price_final, 0);
    assert_eq!(archived.participant_count, 1);
    assert_eq!(archived.pool_up, 100_0000000);
    assert_eq!(archived.pool_down, 0);
    assert!(client.is_round_cancelled(&round_id));
}

#[test]
fn test_archived_round_after_precision_resolve() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    let start_price: u128 = 2000;
    client.create_round(&start_price, &Some(1)); // Precision mode
    client.place_precision_prediction(&alice, &30_0000000, &2296);
    client.place_precision_prediction(&bob, &70_0000000, &2299);

    let final_price: u128 = 2298;
    let round_id = resolve_active_round(&client, &env, final_price, 1);

    let archived = client
        .get_archived_round(&round_id)
        .expect("precision resolved round must be archived");
    assert_eq!(archived.round_id, round_id);
    assert_eq!(archived.price_start, start_price);
    assert_eq!(archived.price_final, final_price);
    assert_eq!(archived.mode, RoundMode::Precision);
    assert_eq!(archived.status, RoundArchiveStatus::Resolved);
    assert_eq!(archived.participant_count, 2);

    // Bob is closer to final_price (10_6000000), so wins full pot.
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 100_0000000);
}

#[test]
fn test_archived_round_fallback_refund() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.set_min_participants(&Some(2u32));
    client.mint_initial(&user);

    let start_price: u128 = 1_0000000;
    client.create_round(&start_price, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    let round_id = client.get_active_round().unwrap().round_id;

    let final_price: u128 = 1_2000000;
    resolve_active_round(&client, &env, final_price, 1);

    let archived = client
        .get_archived_round(&round_id)
        .expect("fallback round must be archived");
    assert_eq!(archived.status, RoundArchiveStatus::FallbackRefund);
    assert_eq!(archived.price_final, final_price);
    assert_eq!(archived.participant_count, 1);
    assert_eq!(client.get_pending_winnings(&user), 100_0000000);
}

#[test]
fn test_get_archived_round_missing_returns_none() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    assert!(client.get_archived_round(&999).is_none());
}

#[test]
fn test_get_recent_archived_rounds_order_and_limit() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let mut round_ids = Vec::new(&env);
    for i in 0..3 {
        client.create_round(&(1_0000000u128 + i as u128), &None);
        round_ids.push_back(resolve_active_round(
            &client,
            &env,
            1_1000000u128 + i as u128,
            i as u64 + 1,
        ));
    }

    assert!(client.get_recent_archived_rounds(&0).is_empty());

    let recent = client.get_recent_archived_rounds(&2);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent.get(0).unwrap().round_id, round_ids.get(2).unwrap());
    assert_eq!(recent.get(1).unwrap().round_id, round_ids.get(1).unwrap());

    let all = client.get_recent_archived_rounds(&10);
    assert_eq!(all.len(), 3);
    assert_eq!(all.get(0).unwrap().round_id, round_ids.get(2).unwrap());
    assert_eq!(all.get(2).unwrap().round_id, round_ids.get(0).unwrap());
}

// ============================================================================
// LOSS OUTCOME EVENT TESTS (Issue #168)
// ============================================================================
//
// These tests verify the additive `("outcome", "loss")` event semantics:
// - It is emitted per losing participant during competitive settlement only
//   (UpDown + Precision, both indexed and legacy per-user position layouts).
// - It is NOT emitted on refund paths (price-unchanged, one-sided pool,
//   min-participants fallback, admin cancellation).
// - For Precision losers who only committed and did not reveal, the
//   `predicted_price` field is published as 0 (the guess is unknowable
//   on-chain until reveal) — this convention is documented in
//   `docs/EVENT_SCHEMA.md` and matches the contract implementation note
//   in `_resolve_precision_mode`.

/// Helper: counts `("outcome", "loss")` events currently emitted on the env.
fn count_outcome_loss_events(env: &Env) -> u32 {
    env.events()
        .all()
        .iter()
        .filter(|e| {
            let (_contract, topics, _data) = e;
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("outcome"))
                && topics.get(1).unwrap().try_into_val(env) == Ok(symbol_short!("loss"))
        })
        .count() as u32
}

/// Helper: collects every decoded loss event payload for assertions.
fn collect_outcome_loss_events(
    env: &Env,
) -> Vec<(soroban_sdk::Address, u64, u32, soroban_sdk::I128, u32, u128)> {
    env.events()
        .all()
        .iter()
        .filter_map(|e| {
            let (_contract, topics, data) = e;
            if topics.len() != 2
                || topics.get(0).unwrap().try_into_val(env) != Ok(symbol_short!("outcome"))
                || topics.get(1).unwrap().try_into_val(env) != Ok(symbol_short!("loss"))
            {
                return None;
            }
            data.try_into_val::<(
                soroban_sdk::Address,
                u64,
                u32,
                soroban_sdk::I128,
                u32,
                u128,
            )>(env)
            .ok()
        })
        .collect()
}

#[test]
fn test_outcome_loss_event_updown_indexed_path() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env); // Up winner
    let bob = Address::generate(&env); // Up winner
    let charlie = Address::generate(&env); // Down loser
    let diana = Address::generate(&env); // Down loser

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);
    client.mint_initial(&diana);

    client.create_round(&1_0000000, &None); // UpDown
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Up);
    client.place_bet(&charlie, &150_0000000, &BetSide::Down);
    client.place_bet(&diana, &50_0000000, &BetSide::Down);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_5000000, // price went UP -> Up wins
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Two losers => exactly two loss events.
    assert_eq!(
        count_outcome_loss_events(&env),
        2,
        "one loss event must be emitted per UpDown loser",
    );

    let losses = collect_outcome_loss_events(&env);
    assert_eq!(losses.len(), 2);

    for (_user, round_id, mode, _amount, _side, predicted_price) in &losses {
        assert_eq!(*mode, 0u32, "UpDown loss events must carry mode=0");
        assert_eq!(*round_id, 1u64);
        assert_eq!(*predicted_price, 0u128, "`predicted_price` is unused in UpDown mode");
    }

    // Verify both losers are represented, each with their losing side.
    let mut by_addr: std::collections::HashMap<soroban_sdk::String, (soroban_sdk::I128, u32)> =
        std::collections::HashMap::new();
    for (user, _round_id, _mode, amount, side, _price) in &losses {
        by_addr.insert(user.to_string(), (*amount, *side));
    }
    assert_eq!(by_addr[&charlie.to_string()], (150_0000000i128, 1u32));
    assert_eq!(by_addr[&diana.to_string()], (50_0000000i128, 1u32));
}

#[test]
fn test_outcome_loss_event_updown_legacy_path() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    let start_price: u128 = 1_0000000;
    client.create_round(&start_price, &None);

    // Author positions via the legacy bulk map so resolution takes the legacy
    // winnings path (matches existing tests like `test_resolve_round_price_went_up`).
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(
            alice.clone(),
            UserPosition {
                amount: 100_0000000,
                side: BetSide::Up,
            },
        );
        positions.set(
            bob.clone(),
            UserPosition {
                amount: 50_0000000,
                side: BetSide::Down,
            },
        );
        env.storage()
            .persistent()
            .set(&DataKey::UpDownPositions, &positions);

        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .unwrap();
        round.pool_up = 100_0000000;
        round.pool_down = 50_0000000;
        env.storage()
            .persistent()
            .set(&DataKey::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_5000000, // price went UP -> alice wins, bob loses
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // One loser (bob) => exactly one loss event.
    assert_eq!(count_outcome_loss_events(&env), 1);

    let losses = collect_outcome_loss_events(&env);
    assert_eq!(losses.len(), 1);
    let (user, round_id, mode, amount, side, predicted_price) = losses.get(0).unwrap();
    assert_eq!(user, bob);
    assert_eq!(round_id, 1u64);
    assert_eq!(mode, 0u32);
    assert_eq!(amount, 50_0000000i128);
    assert_eq!(side, 1u32, "Bob bet Down → losing side is Down (1)");
    assert_eq!(predicted_price, 0u128);
}

#[test]
fn test_outcome_loss_event_precision_indexed_path() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env); // winner (closest guess)
    let bob = Address::generate(&env); // loser (revealed)
    let charlie = Address::generate(&env); // loser (unrevealed commit)

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&2000, &Some(1)); // Precision mode

    // Alice and Bobby place direct predictions; Charlie commits and will NOT reveal.
    client.place_precision_prediction(&alice, &100_0000000, &2297u128);
    client.place_precision_prediction(&bob, &150_0000000, &2500u128);

    // Build Charlie's commitment hash locally and submit it via the contract API.
    let price_c = 2200u128;
    let salt_c = soroban_sdk::BytesN::from_array(&env, &[3; 32]);
    let mut preimage_c = soroban_sdk::Bytes::new(&env);
    use soroban_sdk::xdr::ToXdr;
    preimage_c.append(&price_c.to_xdr(&env));
    preimage_c.append(&salt_c.clone().to_xdr(&env));
    let computed_c = env.crypto().sha256(&preimage_c);
    let committed_hash_c: soroban_sdk::BytesN<32> = computed_c.into();
    client.commit_prediction(&charlie, &committed_hash_c, &80_0000000);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 2298, // Alice diff=1 wins; Bob diff=202 loses; Charlie (unrevealed) loses
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Two losers => two loss events (includes the unrevealed-commitment loser).
    assert_eq!(
        count_outcome_loss_events(&env),
        2,
        "one loss event per Precision loser (including unrevealed-commitment losers)",
    );

    let losses = collect_outcome_loss_events(&env);
    assert_eq!(losses.len(), 2);

    for (_user, round_id, mode, _amount, side, _predicted_price) in &losses {
        assert_eq!(round_id, 1u64);
        assert_eq!(*mode, 1u32, "Precision loss events must carry mode=1");
        assert_eq!(*side, 0u32, "`side` is unused in Precision mode");
    }

    let mut by_addr: std::collections::HashMap<soroban_sdk::String, (soroban_sdk::I128, u128)> =
        std::collections::HashMap::new();
    for (user, _, _, amount, _, price) in &losses {
        by_addr.insert(user.to_string(), (*amount, *price));
    }
    // Bob revealed 2500.
    assert_eq!(by_addr[&bob.to_string()], (150_0000000i128, 2500u128));
    // Charlie never revealed → predicted_price = 0 (unknown on-chain).
    assert_eq!(by_addr[&charlie.to_string()], (80_0000000i128, 0u128));
}

#[test]
fn test_outcome_loss_event_precision_legacy_path() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env); // winner (closest guess)
    let bob = Address::generate(&env); // loser
    let charlie = Address::generate(&env); // loser

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    let start_price: u128 = 2000;
    client.create_round(&start_price, &Some(1)); // Precision

    // Author predictions via legacy bulk map so resolution takes the legacy
    // precision path (matches existing tests like `test_resolve_precision_*`).
    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);
        predictions.set(
            alice.clone(),
            PrecisionPrediction {
                user: alice.clone(),
                predicted_price: 2297,
                amount: 100_0000000,
            },
        );
        predictions.set(
            bob.clone(),
            PrecisionPrediction {
                user: bob.clone(),
                predicted_price: 2500,
                amount: 150_0000000,
            },
        );
        predictions.set(
            charlie.clone(),
            PrecisionPrediction {
                user: charlie.clone(),
                predicted_price: 5000,
                amount: 50_0000000,
            },
        );
        env.storage()
            .persistent()
            .set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 2298, // Alice (diff 1) wins; bob (diff 202) and charlie (diff 2702) lose
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // 2 losers => 2 loss events.
    assert_eq!(count_outcome_loss_events(&env), 2);
    let losses: std::collections::HashMap<soroban_sdk::String, (soroban_sdk::I128, u128)> =
        collect_outcome_loss_events(&env)
            .iter()
            .map(|(u, _, _, amount, _, price)| (u.to_string(), (*amount, *price)))
            .collect();
    assert_eq!(
        losses.len(),
        2,
        "exactly two loss events (bob, charlie) must be emitted",
    );
    // Per-user explicit assertions make regress failures far more diagnostic
    // than a generic loop+panic.
    assert_eq!(
        losses[&bob.to_string()],
        (150_0000000i128, 2500u128),
        "bob loss event must carry his revealed guess",
    );
    assert_eq!(
        losses[&charlie.to_string()],
        (50_0000000i128, 5000u128),
        "charlie loss event must carry his revealed guess",
    );
    // Winner (alice, predicted_price=2297) MUST NOT appear in any loss event.
    assert!(
        !losses.contains_key(&alice.to_string()),
        "winner must never emit loss events",
    );
}

#[test]
fn test_outcome_loss_event_not_emitted_on_refund() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Price-unchanged: refunds all participants; no loss events.
    let start_price: u128 = 1_5000000;
    client.create_round(&start_price, &None);
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(
            alice.clone(),
            UserPosition {
                amount: 100_0000000,
                side: BetSide::Up,
            },
        );
        positions.set(
            bob.clone(),
            UserPosition {
                amount: 50_0000000,
                side: BetSide::Down,
            },
        );
        env.storage()
            .persistent()
            .set(&DataKey::UpDownPositions, &positions);
        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .unwrap();
        round.pool_up = 100_0000000;
        round.pool_down = 50_0000000;
        env.storage()
            .persistent()
            .set(&DataKey::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    client.resolve_round(&OraclePayload {
        price: start_price, // unchanged
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    assert_eq!(
        count_outcome_loss_events(&env),
        0,
        "price-unchanged refunds must not emit loss events",
    );
}

#[test]
fn test_outcome_loss_event_not_emitted_on_min_participants_fallback() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.set_min_participants(&Some(3u32));

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    // Fallback refunds the user; no loss event should be emitted.
    assert_eq!(
        count_outcome_loss_events(&env),
        0,
        "min-participants fallback refunds must not emit loss events",
    );
}

#[test]
fn test_outcome_loss_event_not_emitted_on_cancel() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // Admin cancels; refunds the user. No loss event.
    client.cancel_round(&1u32);
    assert_eq!(
        count_outcome_loss_events(&env),
        0,
        "admin cancel refunds must not emit loss events",
    );
}

#[test]
fn test_outcome_loss_event_count_matches_outcomes_across_modes() {
    // Walks both modes in a single fixture and verifies the total emitted loss
    // events equal the number of losers (2 UpDown + 3 Precision = 5 events).
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let u_a = Address::generate(&env);
    let u_b = Address::generate(&env);
    let u_c = Address::generate(&env);
    let u_d = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&u_a);
    client.mint_initial(&u_b);
    client.mint_initial(&u_c);
    client.mint_initial(&u_d);

    // ─── UpDown round: 4 participants, 2 winners, 2 losers ───────────────────
    client.create_round(&1_0000000, &None);
    client.place_bet(&u_a, &100_0000000, &BetSide::Up);
    client.place_bet(&u_b, &200_0000000, &BetSide::Up);
    client.place_bet(&u_c, &150_0000000, &BetSide::Down); // loser
    client.place_bet(&u_d, &50_0000000, &BetSide::Down); // loser

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    client.resolve_round(&OraclePayload {
        price: 1_5000000, // price up
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    let updown_count = count_outcome_loss_events(&env);
    assert_eq!(updown_count, 2, "UpDown round must emit exactly 2 loss events");

    // ─── Precision round: 4 participants, 1 winner, 3 losers ────────────────
    client.create_round(&2000, &Some(1));
    client.place_precision_prediction(&u_a, &100_0000000, &2297u128);
    client.place_precision_prediction(&u_b, &200_0000000, &2400u128);
    client.place_precision_prediction(&u_c, &150_0000000, &3000u128);
    client.place_precision_prediction(&u_d, &150_0000000, &5000u128);

    env.ledger().with_mut(|li| {
        li.sequence_number = 24;
    });
    client.resolve_round(&OraclePayload {
        price: 2298, // u_a (diff 1) wins
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 2u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    let total_after_precision = count_outcome_loss_events(&env);
    assert_eq!(
        total_after_precision - updown_count,
        3,
        "Precision round must emit exactly 3 new loss events for the 3 losers",
    );

    // Sanity: winner (u_a, who won the Precision round) never gets a loss event.
    let losses = collect_outcome_loss_events(&env);
    for (user, _, _, _, _, _) in &losses {
        assert_ne!(user, &u_a, "winners must never emit loss events");
    }

    // Ordering invariant: in this fixture the UpDown round was resolved
    // first (at ledger 12), then the Precision round (at ledger 24). All
    // UpDown loss events therefore arrive before any Precision loss event.
    // This guards against accidental batched-replay re-orderings pooling
    // loss events across rounds.
    let mut first_precision_idx = None::<u32>;
    for (idx, (_user, _round_id, mode, _, _, _)) in losses.iter().enumerate() {
        if *mode == 1u32 && first_precision_idx.is_none() {
            first_precision_idx = Some(idx as u32);
        }
    }
    if let Some(idx) = first_precision_idx {
        // UpDown losses (mode=0) must all be ordered before the first Precision (mode=1) loss event.
        for (other_idx, (_, _, mode, _, _, _)) in losses.iter().enumerate() {
            if (other_idx as u32) < idx {
                assert_eq!(
                    *mode, 0u32,
                    "UpDown loss event must appear before any Precision loss event",
                );
            }
        }
    }
}

#[test]
fn test_archive_retention_prunes_oldest() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // MAX_ARCHIVED_ROUNDS = 128; create 129 resolved rounds to force pruning of round 1.
    let mut first_round_id = 0u64;
    for i in 0..129 {
        client.create_round(&1_0000000u128, &None);
        let round_id = resolve_active_round(&client, &env, 1_1000000u128, i as u64 + 1);
        if i == 0 {
            first_round_id = round_id;
        }
    }

    assert!(
        client.get_archived_round(&first_round_id).is_none(),
        "oldest archive must be pruned once retention limit is exceeded"
    );
    assert!(
        client.get_archived_round(&129).is_some(),
        "newest archive must remain queryable"
    );

    let recent = client.get_recent_archived_rounds(&200);
    assert_eq!(recent.len(), 128);
}

// ============================================================================
// PROTOCOL FEE TESTS (Issue #162)
// ============================================================================
//
// These tests exercise the optional protocol fee: default (ProtocolFeeBps
// storage key absent) is byte-for-byte the pre-#162 behaviour; activating
// the fee routes `fee = total_pot * bps / 10_000` to the on-chain treasury
// while preserving the conservation invariant
//     Σ payouts + treasury_growth == total_pot
// for every competitive settlement path (UpDown indexed/legacy, Precision
// indexed/legacy). Refund paths (price-unchanged, one-sided, min-participants,
// admin cancel) MUST NOT emit a fee event — and the treasury MUST stay flat.
//
// The 10% hard cap is enforced at schedule time; timelock semantics tested
// in `config_timelock.rs::test_protocol_fee_timelock_*`.


fn collect_protocol_fee_events(
    env: &Env,
) -> Vec<(u64, soroban_sdk::I128, soroban_sdk::I128, u32)> {
    env.events()
        .all()
        .iter()
        .filter_map(|e| {
            let (_contract, topics, data) = e;
            if topics.len() != 2
                || topics.get(0).unwrap().try_into_val(env) != Ok(symbol_short!("protocol"))
                || topics.get(1).unwrap().try_into_val(env) != Ok(symbol_short!("fee_collected"))
            {
                return None;
            }
            data.try_into_val::<(u64, soroban_sdk::I128, soroban_sdk::I128, u32)>(env)
                .ok()
        })
        .collect()
}

fn count_protocol_fee_events(env: &Env) -> u32 {
    env.events()
        .all()
        .iter()
        .filter(|e| {
            let (_contract, topics, _data) = e;
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("protocol"))
                && topics.get(1).unwrap().try_into_val(env) == Ok(symbol_short!("fee_collected"))
        })
        .count() as u32
}

/// Build a deterministic Vector of user-side pre-resolution `("outcome","loss")` events
/// helper to keep the conservation-test bodies short.
fn sum_pending_payouts(env: &Env, users: &[soroban_sdk::Address]) -> soroban_sdk::I128 {
    let mut total: i128 = 0;
    env.as_contract(&env.current_contract_address(), || {
        for u in users {
            let key = crate::types::DataKey::PendingWinnings(u.clone());
            let v: Option<i128> = env.storage().persistent().get(&key);
            total = total
                .checked_add(v.unwrap_or(0))
                .expect("overflow summing pending payouts");
        }
    });
    total.into()
}

#[test]
fn test_protocol_fee_disabled_default_is_no_behaviour_change() {
    // Without ever calling schedule_protocol_fee_bps, a competitive
    // UpDown resolution must:
    //  - Pay winners exactly the pre-#162 formula amount.
    //  - Leave treasury at 0.
    //  - NOT emit a fee event.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env); // Up winner
    let bob = Address::generate(&env); // Down loser

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000_0000, &None);
    client.place_bet(&alice, &100_000_0000, &BetSide::Up);
    client.place_bet(&bob, &50_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    client.resolve_round(&OraclePayload {
                    price: 1_500_0000,
                    timestamp: env.ledger().timestamp(),
                    round_id: 0u32,
                    nonce: 1u64,
                    network_id: env.ledger().network_id(),
                    contract_addr: contract_id.clone(),
                });

    // Pre-#162 UpDown formula: payout_alice = 100 + 100 * 50 / 100 = 150 stroops.
    assert_eq!(
        sum_pending_payouts(&env, &[alice.clone(), bob.clone()]),
        150_000_0000i128,
    );
    assert_eq!(client.get_protocol_fee_bps(), None);
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    assert_eq!(count_protocol_fee_events(&env), 0);
}

#[test]
fn test_protocol_fee_updown_indexed_conservation() {
    // 200_bps (2%) fee on a 100/50 pot must run the conservation invariant:
    // total_pot = 150 -> fee = 3 (floor), distributable_winning = 97,
    // distributable_losing = 47, sum payouts + treasury = 147... wait:
    // After expense: sum payouts to winners + treasury = 150.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env); // Up winner
    let bob = Address::generate(&env); // Down loser

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Activate 200_bps (2%) fee via timelock -- fast-forward.
    client.schedule_protocol_fee_bps(&Some(200u32));
    env.ledger().with_mut(|li| {
        li.sequence_number = 2000; // advance past CONFIG_TIMELOCK_LEDGERS (1440).
    });
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );
    assert_eq!(client.get_protocol_fee_bps(), Some(200u32));

    client.create_round(&1_000_0000, &None);
    client.place_bet(&alice, &100_000_0000, &BetSide::Up);
    client.place_bet(&bob, &50_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
                    price: 1_500_0000,
                    timestamp: env.ledger().timestamp(),
                    round_id: 0u32,
                    nonce: 2u64,
                    network_id: env.ledger().network_id(),
                    contract_addr: contract_id.clone(),
                });

    // total_pot = 150; fee = floor(150 * 200 / 10_000) = 3.
    // fee_from_losing = min(3, 50) = 3; fee_from_winning = 0.
    // distributable_winning = 100, distributable_losing = 47.
    // alice payout = 100 + 100 * 47 / 100 = 147.
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone()]);
    assert_eq!(payouts, 147_000_0000i128,
        "winner payout must reflect fee deducted from losing pool");
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 3_000_0000i128,
        "treasury must accumulate exactly the bps-computed fee");

    // Conservation invariant.
    let total_pot: i128 = 150_000_0000i128;
    assert_eq!(payouts + treasury.into(), total_pot.into(),
        "conservation: payouts + treasury must equal total_pot");

    // One round -> one fee_collected event.
    assert_eq!(count_protocol_fee_events(&env), 1);
    let events = collect_protocol_fee_events(&env);
    let (round_id, fee, _treasury_after, bps) = events.get(0).unwrap();
    assert_eq!(*round_id, 2u64);
    assert_eq!(*fee, 3_000_0000i128);
    assert_eq!(*bps, 200u32);
}

#[test]
fn test_protocol_fee_updown_legacy_conservation() {
    // Same conservation test but exercising the legacy migration-fallback path.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.schedule_protocol_fee_bps(&Some(500u32)); // 5%
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    let start_price: u128 = 1_000_0000;
    client.create_round(&start_price, &None);

    // Author positions via the legacy bulk map.
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(alice.clone(), UserPosition { amount: 100_000_0000, side: BetSide::Up });
        positions.set(bob.clone(), UserPosition { amount: 50_000_0000, side: BetSide::Down });
        env.storage().persistent().set(&DataKey::UpDownPositions, &positions);

        let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
        round.pool_up = 100_000_0000;
        round.pool_down = 50_000_0000;
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
                    price: 1_500_0000,
                    timestamp: env.ledger().timestamp(),
                    round_id: 0u32,
                    nonce: 3u64,
                    network_id: env.ledger().network_id(),
                    contract_addr: contract_id.clone(),
                });

    // total_pot = 150; fee = floor(150 * 500 / 10_000) = 7.
    // distributable_winning = 100, distributable_losing = 43.
    // alice payout = 100 + 100 * 43 / 100 = 143.
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone()]);
    assert_eq!(payouts, 143_000_0000i128);
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 7_000_0000i128);
    // Conservation.
    assert_eq!(payouts + treasury.into(), 150_000_0000i128);
}

#[test]
fn test_protocol_fee_precision_indexed_conservation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env); // winner (closest guess)
    let bob = Address::generate(&env); // loser
    let charlie = Address::generate(&env); // loser

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.schedule_protocol_fee_bps(&Some(1000u32)); // 10% (cap)
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    client.create_round(&2000, &Some(1));
    client.place_precision_prediction(&alice, &100_000_0000, &2297u128);
    client.place_precision_prediction(&bob, &150_000_0000, &2500u128);
    client.place_precision_prediction(&charlie, &50_000_0000, &5000u128);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
                    price: 2298,
                    timestamp: env.ledger().timestamp(),
                    round_id: 0u32,
                    nonce: 4u64,
                    network_id: env.ledger().network_id(),
                    contract_addr: contract_id.clone(),
                });

    // total_pot = 100 + 150 + 50 = 300. fee = 300 * 1000 / 10_000 = 30.
    // winner_count = 1 -> payout_pool = 270 -> alice gets 270.
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone(), charlie.clone()]);
    assert_eq!(payouts, 270_000_0000i128);
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 30_000_0000i128);
    assert_eq!(payouts + treasury.into(), 300_000_0000i128,
        "conservation invariant must hold for Precision indexed path");
}

#[test]
fn test_protocol_fee_precision_legacy_conservation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.schedule_protocol_fee_bps(&Some(100u32)); // 1%
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    let start_price: u128 = 2000;
    client.create_round(&start_price, &Some(1));

    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);
        predictions.set(alice.clone(), PrecisionPrediction { user: alice.clone(), predicted_price: 2297, amount: 100_000_0000 });
        predictions.set(bob.clone(), PrecisionPrediction { user: bob.clone(), predicted_price: 2500, amount: 150_000_0000 });
        predictions.set(charlie.clone(), PrecisionPrediction { user: charlie.clone(), predicted_price: 5000, amount: 50_000_0000 });
        env.storage().persistent().set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
                    price: 2298,
                    timestamp: env.ledger().timestamp(),
                    round_id: 0u32,
                    nonce: 5u64,
                    network_id: env.ledger().network_id(),
                    contract_addr: contract_id.clone(),
                });

    // total_pot = 300; fee = 300 * 100 / 10_000 = 3.
    // payout_pool = 297 -> winner alice gets 297.
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone(), charlie.clone()]);
    assert_eq!(payouts, 297_000_0000i128);
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 3_000_0000i128);
    assert_eq!(payouts + treasury.into(), 300_000_0000i128,
        "conservation invariant must hold for Precision legacy path");
}

#[test]
fn test_protocol_fee_thin_losing_pool_updown() {
    // With bps near the cap and a thin losing pool, the fee exceeds losing_pool.
    // Per documented policy: spillover taken from winning_pool so the
    // conservation invariant holds even when winners lose a portion of
    // their principal.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env); // Up majority winner
    let bob = Address::generate(&env); // Down minority loser

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.schedule_protocol_fee_bps(&Some(1000u32)); // 10% (cap)
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    client.create_round(&1_000_0000, &None);
    // winning_pool = 1000, losing_pool = 1.
    // total_pot = 1001; fee = floor(1001 * 1000 / 10_000) = 100.
    // fee_from_losing = min(100, 1) = 1; fee_from_winning = 99.
    // distributable_winning = 1000 - 99 = 901.
    // distributable_losing  = 1 - 1 = 0.
    // alice payout = 1000 + 1000 * 0 / 901 = 1000. (no share; 1000 of 1001 taken as fee)
    client.place_bet(&alice, &1000_000_0000, &BetSide::Up);
    client.place_bet(&bob, &1_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
                    price: 1_500_0000,
                    timestamp: env.ledger().timestamp(),
                    round_id: 0u32,
                    nonce: 6u64,
                    network_id: env.ledger().network_id(),
                    contract_addr: contract_id.clone(),
                });

    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone()]);
    // alice gets her principal minus the spillover (= 1000 - 99 = 901)
    // (since distributable_losing = 0, the share numerator is 0; payout = amount).
    assert_eq!(payouts, 1000_000_0000i128,
        "loser has 0 distributable_losing so winners only get principal back");
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 100_000_0000i128,
        "full fee still collected: 1 (from losing) + 99 (from winning spillover) = 100");
    assert_eq!(payouts + treasury.into(), 1001_000_0000i128,
        "conservation invariant holds even when losing_pool is thin");
}

#[test]
fn test_protocol_fee_not_collected_on_refund_paths() {
    // Price-unchanged refunds must NOT deduct the fee from treasury even when
    // the fee is enabled. The user's stake is returned 100%; no fee events
    // are emitted on any refund path.
    struct Case { up: bool; }
    let _cases = [Case { up: true }, Case { up: false }];
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.schedule_protocol_fee_bps(&Some(1000u32));
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    let start_price: u128 = 1_500_0000;
    client.create_round(&start_price, &None);
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(alice.clone(), UserPosition { amount: 100_000_0000, side: BetSide::Up });
        positions.set(bob.clone(), UserPosition { amount: 50_000_0000, side: BetSide::Down });
        env.storage().persistent().set(&DataKey::UpDownPositions, &positions);
        let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
        round.pool_up = 100_000_0000;
        round.pool_down = 50_000_0000;
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
            price: start_price,
            timestamp: env.ledger().timestamp(),
            round_id: 0u32,
            nonce: 7u64,
            network_id: env.ledger().network_id(),
            contract_addr: contract_id.clone(),
        });

    // Refund: no fee event, treasury still 0.
    assert_eq!(count_protocol_fee_events(&env), 0,
        "price-unchanged refunds MUST NOT emit a fee event");
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone()]);
    assert_eq!(payouts, 150_000_0000i128,
        "all participants refunded their full stake");
}

#[test]
fn test_protocol_fee_not_collected_on_one_sided_pool_refund() {
    // One-sided pool (only the losing side has bets) refunds all participants
    // without entering the winner-distribution path -- the fee MUST NOT be
    // collected even though `get_protocol_fee_bps` returns Some(active).
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.schedule_protocol_fee_bps(&Some(500u32)); // 5%
    env.ledger().with_mut(|li| li.sequence_number = 2_000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    let start_price: u128 = 1_500_0000;
    client.create_round(&start_price, &None);

    // ONLY down bets -- pool_up=0. Price goes UP -> one-sided refund of all.
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(alice.clone(), UserPosition { amount: 100_000_0000, side: BetSide::Down });
        positions.set(bob.clone(), UserPosition { amount: 50_000_0000, side: BetSide::Down });
        env.storage().persistent().set(&DataKey::UpDownPositions, &positions);
        let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
        round.pool_up = 0;
        round.pool_down = 150_000_0000;
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
        price: 1_700_0000, // up
        timestamp: env.ledger().timestamp(),
        round_id: 0u32,
        nonce: 9u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    assert_eq!(
        count_protocol_fee_events(&env),
        0,
        "one-sided refund MUST NOT emit a fee event"
    );
    assert_eq!(client.get_protocol_fee_treasury(), 0,
        "one-sided refund MUST NOT credit the treasury");
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone()]);
    assert_eq!(
        payouts, 150_000_0000i128,
        "all participants refunded their full stake on one-sided pool"
    );
}

#[test]
fn test_protocol_fee_withdrawal_to_recipient() {
    // Once accumulated, the admin can drain the treasury to a recipient.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let treasury_account = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&treasury_account);

    client.schedule_protocol_fee_bps(&Some(1000u32)); // 10%
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    client.create_round(&1_000_0000, &None);
    client.place_bet(&alice, &100_000_0000, &BetSide::Up);
    client.place_bet(&bob, &50_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
                    price: 1_500_0000,
                    timestamp: env.ledger().timestamp(),
                    round_id: 0u32,
                    nonce: 8u64,
                    network_id: env.ledger().network_id(),
                    contract_addr: contract_id.clone(),
                });
    // total_pot = 150; fee = 15; distributable_losing = 35.
    // payout = 100 + 100 * 35 / 100 = 135.
    assert_eq!(client.get_protocol_fee_treasury(), 15_000_0000i128);

    // Drain 10 stroops to treasury_account.
    let starting_bal = client.balance(&treasury_account);
    let withdrawn = client.withdraw_protocol_fee(&treasury_account.clone(), &10_000_0000i128);
    assert_eq!(withdrawn, 10_000_0000i128);
    assert_eq!(
        client.balance(&treasury_account),
        starting_bal + 10_000_0000i128,
    );
    assert_eq!(client.get_protocol_fee_treasury(), 5_000_0000i128);

    // Attempting to overwithdraw must NOT consume funds.
    let result = client.try_withdraw_protocol_fee(
        &treasury_account.clone(),
        &1_000_000_0000i128,
    );
    assert!(result.is_err(), "over-withdrawal must be rejected");
    assert_eq!(client.get_protocol_fee_treasury(), 5_000_0000i128);
}

#[test]
fn test_protocol_fee_schedule_validation_rejects_zero_and_over_cap() {
    // Each test cases schedules, fast-forwards past the timelock, and
    // applies/cancels before attempting the next one -- otherwise
    // `_schedule_config_change` would bounce subsequent calls on
    // `RoundAlreadyActive` and never reach the bps validator.
    fn run_to_activation(env: &Env) {
        env.ledger().with_mut(|li| li.sequence_number += 1_500);
    }
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // None always OK.
    client.schedule_protocol_fee_bps(&None);
    run_to_activation(&env);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);
    assert_eq!(client.get_protocol_fee_bps(), None);

    // Some(0) rejected -- explicit disable is the only legitimate way.
    let r0 = client.try_schedule_protocol_fee_bps(&Some(0u32));
    assert!(r0.is_err(), "Some(0) is not a valid bps value");
    run_to_activation(&env);
    client.cancel_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    // Over cap rejected.
    let r_max = client.try_schedule_protocol_fee_bps(&Some(1_001u32));
    assert!(r_max.is_err(), "1_001 bps exceeds MAX_PROTOCOL_FEE_BPS=1000");
    run_to_activation(&env);
    client.cancel_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    // Cap (1_000) accepted.
    let r_top = client.try_schedule_protocol_fee_bps(&Some(1_000u32));
    assert!(r_top.is_ok(), "1_000 bps (MAX) must be accepted");
    run_to_activation(&env);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);
    assert_eq!(client.get_protocol_fee_bps(), Some(1_000u32));
}
