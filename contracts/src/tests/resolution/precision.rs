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
        attestation: None,    });

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
        attestation: None,    });

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
        attestation: None,    });

    // Round should be cleared
    assert_eq!(client.get_active_round(), None);
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
        attestation: None,    });

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
        attestation: None,    });

    assert_eq!(client.get_pending_winnings(&alice), 200_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 0);
}

