//! Tests for round resolution and winnings distribution.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, DataKey, OraclePayload, PrecisionPrediction, Round, UserPosition};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, Map, TryIntoVal,
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
    });

    // Total pot = 200_0000001
    // split = 200_0000001 / 2 = 100_0000000
    // remainder = 1
    // The lexicographically lowest winner (lowest_user) must get: split + remainder = 100_0000001
    // The other winner (other_user) must get: 100_0000000
    assert_eq!(client.get_pending_winnings(&lowest_user), 100_0000001);
    assert_eq!(client.get_pending_winnings(&other_user), 100_0000000);
}
