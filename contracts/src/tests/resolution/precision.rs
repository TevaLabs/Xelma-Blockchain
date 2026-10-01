use super::*;

#[test]
fn test_resolve_precision_closest_guess_wins() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    // Advance ledger to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve with actual price 2298
    client.resolve_round(&OraclePayload {
        price: 2298,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    // Advance ledger
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve with actual price 2200
    client.resolve_round(&OraclePayload {
        price: 2200,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Alice guessed exactly right
    client.resolve_round(&OraclePayload {
        price: 2250,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);

    // Create Precision mode round with no predictions
    client.create_round(&2000, &Some(1));

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve with no predictions - should succeed without errors
    client.resolve_round(&OraclePayload {
        price: 2250,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Actual price 2200 - Alice diff 10, Bob diff 10, Charlie diff 10
    client.resolve_round(&OraclePayload {
        price: 2200,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Single prediction always wins
    client.resolve_round(&OraclePayload {
        price: 2500,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Actual price is 1_0001 - Alice is closest (diff 1 vs Bob's diff 8_9998)
    client.resolve_round(&OraclePayload {
        price: 1_0001,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // All tied with perfect guess
    client.resolve_round(&OraclePayload {
        price: 2_0000,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // All tied
    client.resolve_round(&OraclePayload {
        price: 5_0000,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 3_0000,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    // Total pot: 100, Winner count: 2
    // payout_per_winner = 100 / 2 = 50
    // remainder = 100 % 2 = 0
    // Both get exactly 50
    assert_eq!(client.get_pending_winnings(&alice), 50_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 50_0000000);
}

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
        client.update_oracle_heartbeat(&0u32);
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
                .set(&DataKeyCore::PrecisionPositions, &predictions);
        });

        env.ledger().with_mut(|li| {
            li.sequence_number = 12;
        });
        client.resolve_round(&OraclePayload {
            price: final_price,
            timestamp: env.ledger().timestamp(),
            round_id: client
                .get_active_round()
                .map(|r| r.start_ledger)
                .unwrap_or(0),
            nonce: 1u64,
            network_id: env.ledger().network_id(),
            contract_addr: contract_id.clone(),
            confidence: None,
            attestation: None,
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
    client.update_oracle_heartbeat(&0u32);
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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    client.resolve_round(&OraclePayload {
        price: 3_0000,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);
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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    client.resolve_round(&OraclePayload {
        price: 7_0000,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_0000000, &Some(1));

    // Alice commits and reveals guess of 2000
    let price_alice = 2000u128;
    let salt_alice = test_salt(&env, 1);
    let mut preimage_alice = Bytes::new(&env);
    preimage_alice.append(&price_alice.to_xdr(&env));
    preimage_alice.append(&salt_alice.clone().to_xdr(&env));
    let hash_alice = env.crypto().sha256(&preimage_alice);
    let committed_hash_alice: BytesN<32> = hash_alice.into();
    client.commit_prediction(&alice, &committed_hash_alice, &100_0000000);

    // Bob commits but does NOT reveal
    let price_bob = 2200u128;
    let salt_bob = test_salt(&env, 2);
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
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
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
    client.update_oracle_heartbeat(&0u32);
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
    let salt_a = test_salt(&env, 1);
    let mut preimage_a = Bytes::new(&env);
    preimage_a.append(&price.to_xdr(&env));
    preimage_a.append(&salt_a.clone().to_xdr(&env));
    let hash_a = env.crypto().sha256(&preimage_a);
    let committed_hash_a: BytesN<32> = hash_a.into();
    client.commit_prediction(&lowest_user, &committed_hash_a, &bet_lowest);

    let salt_b = test_salt(&env, 2);
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
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    // Total pot = 200_0000001
    // split = 200_0000001 / 2 = 100_0000000
    // remainder = 1
    // The lexicographically lowest winner (lowest_user) must get: split + remainder = 100_0000001
    // The other winner (other_user) must get: 100_0000000
    assert_eq!(client.get_pending_winnings(&lowest_user), 100_0000001);
    assert_eq!(client.get_pending_winnings(&other_user), 100_0000000);
}

/// Extends `test_precision_remainder_goes_to_lexicographically_lowest_winner`
/// to a 3-way tie (Issue #404's "tie cases with 2+ winners" acceptance
/// criterion): the indivisible remainder must still land on the
/// lexicographically-lowest address among the *winners*, not the first
/// address to bet or the first address generated.
#[test]
fn test_precision_remainder_3way_tie_goes_to_lexicographically_lowest_winner() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    let mut users: alloc::vec::Vec<Address> = alloc::vec![
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];
    users.sort();
    let (lowest, mid, highest) = (users[0].clone(), users[1].clone(), users[2].clone());

    for u in &users {
        client.mint_initial(u);
    }

    client.create_round(&1_0000000, &Some(1)); // Precision mode

    // All three predict the exact same price -> guaranteed 3-way tie.
    // Total pot = 40 + 30 + 30 = 100_0000000; split 3 ways leaves a
    // 1-stroop remainder (100_0000000 % 3 == 1).
    let price = 2000u128;
    client.place_precision_prediction(&lowest, &40_0000000, &price);
    client.place_precision_prediction(&mid, &30_0000000, &price);
    client.place_precision_prediction(&highest, &30_0000000, &price);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    // per_winner = 100_0000000 / 3 = 33_3333333, remainder = 1.
    // The lowest-address winner gets the remainder regardless of stake size
    // or bet order — `lowest` staked the *most* here specifically to prove
    // the remainder follows address order, not stake size.
    assert_eq!(client.get_pending_winnings(&lowest), 33_3333334);
    assert_eq!(client.get_pending_winnings(&mid), 33_3333333);
    assert_eq!(client.get_pending_winnings(&highest), 33_3333333);

    // Conservation: the whole pot is accounted for.
    let total: i128 = client.get_pending_winnings(&lowest)
        + client.get_pending_winnings(&mid)
        + client.get_pending_winnings(&highest);
    assert_eq!(total, 100_0000000);
}
