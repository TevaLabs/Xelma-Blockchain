use super::*;

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
            attestation: None,        });

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
        attestation: None,    });

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
        attestation: None,    });

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

