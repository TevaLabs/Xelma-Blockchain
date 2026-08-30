// SPDX-License-Identifier: MIT
use super::*;

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
    client.update_oracle_heartbeat(&0u32);
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
    client.update_oracle_heartbeat(&0u32);
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
    client.update_oracle_heartbeat(&0u32);
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
    client.update_oracle_heartbeat(&0u32);
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

    let events = env.events().all();
    let fallback_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("summary"))
    });
    assert!(
        fallback_event.is_some(),
        "Fallback summary event must be emitted when min-participants threshold is not met"
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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);
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

    // Price went up but winning_pool (Up) = 100, losing_pool (Down) = 0 → payout = 100 + 0 = 100
    assert_eq!(client.get_pending_winnings(&user1), 100_0000000);
    assert_eq!(client.get_active_round(), None);
}
