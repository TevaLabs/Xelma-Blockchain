use super::*;

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
    client.update_oracle_heartbeat(&0u32);
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
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    // Verify resolved event was emitted
    let events = env.events().all();
    let resolved_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("summary"))
    });

    assert!(
        resolved_event.is_some(),
        "Round summary event should be emitted"
    );
}

#[test]
fn test_updown_resolution_emits_participant_payout_outcomes() {
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
    client.create_round(&1_0000000, &None);
    let round_id = client.get_last_round_id();

    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &50_0000000, &BetSide::Down);

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

    let outcomes = payout_outcome_events(&env);

    assert_eq!(outcomes.len(), 2);
    assert!(outcomes
        .iter()
        .any(|(event_round_id, mode, user, gross_payout, outcome_type)| {
            *event_round_id == round_id
                && *mode == 0
                && user == &alice
                && *gross_payout == 150_0000000
                && *outcome_type == 0
        }));
    assert!(outcomes
        .iter()
        .any(|(event_round_id, mode, user, gross_payout, outcome_type)| {
            *event_round_id == round_id
                && *mode == 0
                && user == &bob
                && *gross_payout == 0
                && *outcome_type == 1
        }));
}

#[test]
fn test_unchanged_price_resolution_emits_refund_outcomes() {
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
    client.create_round(&1_0000000, &None);
    let round_id = client.get_last_round_id();

    client.place_bet(&alice, &75_0000000, &BetSide::Up);
    client.place_bet(&bob, &25_0000000, &BetSide::Down);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_0000000,
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

    let outcomes = payout_outcome_events(&env);

    assert_eq!(outcomes.len(), 2);
    assert!(outcomes
        .iter()
        .any(|(event_round_id, mode, user, gross_payout, outcome_type)| {
            *event_round_id == round_id
                && *mode == 0
                && user == &alice
                && *gross_payout == 75_0000000
                && *outcome_type == 2
        }));
    assert!(outcomes
        .iter()
        .any(|(event_round_id, mode, user, gross_payout, outcome_type)| {
            *event_round_id == round_id
                && *mode == 0
                && user == &bob
                && *gross_payout == 25_0000000
                && *outcome_type == 2
        }));
}

#[test]
fn test_precision_resolution_emits_participant_payout_outcomes() {
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
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);
    client.create_round(&2000, &Some(1));
    let round_id = client.get_last_round_id();

    client.place_precision_prediction(&alice, &100_0000000, &2297);
    client.place_precision_prediction(&bob, &150_0000000, &2300);
    client.place_precision_prediction(&charlie, &50_0000000, &2500);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

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

    let outcomes = payout_outcome_events(&env);

    assert_eq!(outcomes.len(), 3);
    assert!(outcomes
        .iter()
        .any(|(event_round_id, mode, user, gross_payout, outcome_type)| {
            *event_round_id == round_id
                && *mode == 1
                && user == &alice
                && *gross_payout == 300_0000000
                && *outcome_type == 0
        }));
    assert!(outcomes
        .iter()
        .any(|(event_round_id, mode, user, gross_payout, outcome_type)| {
            *event_round_id == round_id
                && *mode == 1
                && user == &bob
                && *gross_payout == 0
                && *outcome_type == 1
        }));
    assert!(outcomes
        .iter()
        .any(|(event_round_id, mode, user, gross_payout, outcome_type)| {
            *event_round_id == round_id
                && *mode == 1
                && user == &charlie
                && *gross_payout == 0
                && *outcome_type == 1
        }));
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
    client.update_oracle_heartbeat(&0u32);
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
            .set(&DataKeyCore::UpDownPositions, &positions);

        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKeyCore::ActiveRound)
            .unwrap();
        round.pool_up = 100_0000000;
        env.storage()
            .persistent()
            .set(&DataKeyCore::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve - price went up so user wins
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
    client.update_oracle_heartbeat(&0u32);
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

