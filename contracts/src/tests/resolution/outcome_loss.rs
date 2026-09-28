use super::*;

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
    client.update_oracle_heartbeat(&0u32);
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
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    // Two losers => exactly two loss events.
    assert_eq!(
        count_outcome_loss_events(&env),
        2,
        "one loss event must be emitted per UpDown loser",
    );

    let losses = collect_outcome_loss_events(&env);
    assert_eq!(losses.len(), 2);
    for (_user, round_id, mode, _amount, _side, predicted_price) in losses.clone() {
        assert_eq!(mode, 0u32, "UpDown loss events must carry mode=0");
        assert_eq!(round_id, 1u64);
        assert_eq!(
            predicted_price, 0u128,
            "`predicted_price` is unused in UpDown mode"
        );
    }

    // Verify both losers are represented, each with their losing side.
    let mut by_addr: std::collections::HashMap<String, (i128, u32)> =
        std::collections::HashMap::new();
    for (user, _round_id, _mode, amount, side, _price) in losses.clone() {
        by_addr.insert(user.to_string().to_string(), (amount, side));
    }
    assert_eq!(
        by_addr[&charlie.to_string().to_string()],
        (150_0000000i128, 1u32)
    );
    assert_eq!(
        by_addr[&diana.to_string().to_string()],
        (50_0000000i128, 1u32)
    );
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
    client.update_oracle_heartbeat(&0u32);
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
            .set(&DataKeyCore::UpDownPositions, &positions);

        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKeyCore::ActiveRound)
            .unwrap();
        round.pool_up = 100_0000000;
        round.pool_down = 50_0000000;
        env.storage()
            .persistent()
            .set(&DataKeyCore::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_5000000, // price went UP -> alice wins, bob loses
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

    // One loser (bob) => exactly one loss event.
    assert_eq!(count_outcome_loss_events(&env), 1);

    let losses = collect_outcome_loss_events(&env);
    assert_eq!(losses.len(), 1);
    let (user, round_id, mode, amount, side, predicted_price) = losses[0].clone();
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
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&2000, &Some(1)); // Precision mode

    // Alice and Bobby place direct predictions; Charlie commits and will NOT reveal.
    client.place_precision_prediction(&alice, &100_0000000, &2297u128);
    client.place_precision_prediction(&bob, &150_0000000, &2500u128);

    // Build Charlie's commitment hash locally and submit it via the contract API.
    let price_c = 2200u128;
    let salt_c = test_salt(&env, 3);
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
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    // Two losers => two loss events (includes the unrevealed-commitment loser).
    assert_eq!(
        count_outcome_loss_events(&env),
        2,
        "one loss event per Precision loser (including unrevealed-commitment losers)",
    );

    let losses = collect_outcome_loss_events(&env);
    assert_eq!(losses.len(), 2);
    for (_user, round_id, mode, _amount, side, _predicted_price) in losses.clone() {
        assert_eq!(round_id, 1u64);
        assert_eq!(mode, 1u32, "Precision loss events must carry mode=1");
        assert_eq!(side, 0u32, "`side` is unused in Precision mode");
    }

    let mut by_addr: std::collections::HashMap<String, (i128, u128)> =
        std::collections::HashMap::new();
    for (user, _, _, amount, _, price) in losses.clone() {
        by_addr.insert(user.to_string().to_string(), (amount, price));
    }
    // Bob revealed 2500.
    assert_eq!(
        by_addr[&bob.to_string().to_string()],
        (150_0000000i128, 2500u128)
    );
    // Charlie never revealed → predicted_price = 0 (unknown on-chain).
    assert_eq!(
        by_addr[&charlie.to_string().to_string()],
        (80_0000000i128, 0u128)
    );
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
    client.update_oracle_heartbeat(&0u32);
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
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 2298, // Alice (diff 1) wins; bob (diff 202) and charlie (diff 2702) lose
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

    // 2 losers => 2 loss events.
    assert_eq!(count_outcome_loss_events(&env), 2);
    let losses: std::collections::HashMap<String, (i128, u128)> = collect_outcome_loss_events(&env)
        .iter()
        .map(|(u, _, _, amount, _, price)| (u.to_string().to_string(), (*amount, *price)))
        .collect();
    assert_eq!(
        losses.len(),
        2,
        "exactly two loss events (bob, charlie) must be emitted",
    );
    // Per-user explicit assertions make regress failures far more diagnostic
    // than a generic loop+panic.
    assert_eq!(
        losses[&bob.to_string().to_string()],
        (150_0000000i128, 2500u128),
        "bob loss event must carry his revealed guess",
    );
    assert_eq!(
        losses[&charlie.to_string().to_string()],
        (50_0000000i128, 5000u128),
        "charlie loss event must carry his revealed guess",
    );
    // Winner (alice, predicted_price=2297) MUST NOT appear in any loss event.
    assert!(
        !losses.contains_key(&alice.to_string().to_string()),
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
    client.update_oracle_heartbeat(&0u32);
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
            .set(&DataKeyCore::UpDownPositions, &positions);
        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKeyCore::ActiveRound)
            .unwrap();
        round.pool_up = 100_0000000;
        round.pool_down = 50_0000000;
        env.storage()
            .persistent()
            .set(&DataKeyCore::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });
    client.resolve_round(&OraclePayload {
        price: start_price, // unchanged
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
    client.update_oracle_heartbeat(&0u32);
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
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

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
    client.update_oracle_heartbeat(&0u32);
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
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&u_a);
    client.mint_initial(&u_b);
    client.mint_initial(&u_c);
    client.mint_initial(&u_d);

    // --- UpDown round: 4 participants, 2 winners, 2 losers ---
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
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    let updown_count = count_outcome_loss_events(&env);
    assert_eq!(
        updown_count, 2,
        "UpDown round must emit exactly 2 loss events"
    );

    // --- Precision round: 4 participants, 1 winner, 3 losers ---
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
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 2u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    let total_after_precision = count_outcome_loss_events(&env);
    assert_eq!(
        total_after_precision, 3,
        "Precision round must emit exactly 3 new loss events for the 3 losers",
    );

    // Sanity: winner (u_a, who won the Precision round) never gets a loss event.
    let losses = collect_outcome_loss_events(&env);
    for (user, _, _, _, _, _) in losses.clone() {
        assert_ne!(user, u_a, "winners must never emit loss events");
    }

    // Ordering invariant: in this fixture the UpDown round was resolved
    // first (at ledger 12), then the Precision round (at ledger 24). All
    // UpDown loss events therefore arrive before any Precision loss event.
    // This guards against accidental batched-replay re-orderings pooling
    // loss events across rounds.
    let mut first_precision_idx = None::<u32>;
    for (idx, (_user, _round_id, mode, _, _, _)) in losses.clone().into_iter().enumerate() {
        if mode == 1u32 && first_precision_idx.is_none() {
            first_precision_idx = Some(idx as u32);
        }
    }
    if let Some(idx) = first_precision_idx {
        // UpDown losses (mode=0) must all be ordered before the first Precision (mode=1) loss event.
        for (other_idx, (_, _, mode, _, _, _)) in losses.clone().into_iter().enumerate() {
            if (other_idx as u32) < idx {
                assert_eq!(
                    mode, 0u32,
                    "UpDown loss event must appear before any Precision loss event",
                );
            }
        }
    }
}
