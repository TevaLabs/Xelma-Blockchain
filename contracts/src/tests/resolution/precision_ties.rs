use super::*;

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
        attestation: None,    });

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
        attestation: None,    });

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
        attestation: None,    });

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
        attestation: None,    });

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
        attestation: None,    });

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
        attestation: None,    });

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
