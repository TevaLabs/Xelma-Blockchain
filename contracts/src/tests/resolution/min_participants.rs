use super::*;
use soroban_sdk::Bytes;
use soroban_sdk::xdr::ToXdr;

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

#[test]
fn test_precision_under_threshold_single_revealed_prediction() {
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
    client.create_round(&2000, &Some(1));

    // user1 places a revealed prediction
    client.place_precision_prediction(&user1, &100_0000000, &2100u128);
    
    // user2 commits but doesn't reveal (unrevealed commitment)
    let salt = BytesN::from_array(&env, &[1u8; 32]);
    let predicted_price = 2050u128;
    let mut message = Bytes::new(&env);
    message.append(&predicted_price.to_xdr(&env));
    message.append(&salt.to_xdr(&env));
    let commitment = env.crypto().sha256(&message);
    client.commit_prediction(&user2, &commitment, &100_0000000);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let balance_before_user1 = client.balance(&user1);
    let balance_before_user2 = client.balance(&user2);

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

    // Both participants should be refunded (only 1 revealed = under threshold)
    assert_eq!(client.get_pending_winnings(&user1), 100_0000000);
    assert_eq!(client.get_pending_winnings(&user2), 100_0000000);
    assert_eq!(client.get_active_round(), None);
    
    // No fee should be charged on under-threshold refund
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    
    // Stats should not be mutated
    let user1_stats = client.get_user_stats(&user1);
    assert_eq!(user1_stats.total_wins, 0);
    assert_eq!(user1_stats.total_losses, 0);
    
    let user2_stats = client.get_user_stats(&user2);
    assert_eq!(user2_stats.total_wins, 0);
    assert_eq!(user2_stats.total_losses, 0);
}

#[test]
fn test_precision_under_threshold_no_revealed_predictions() {
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
    client.create_round(&2000, &Some(1));

    // Both users commit but neither reveals
    let salt1 = BytesN::from_array(&env, &[1u8; 32]);
    let predicted_price1 = 2100u128;
    let mut message1 = Bytes::new(&env);
    message1.append(&predicted_price1.to_xdr(&env));
    message1.append(&salt1.to_xdr(&env));
    let commitment1 = env.crypto().sha256(&message1);
    client.commit_prediction(&user1, &commitment1, &100_0000000);

    let salt2 = BytesN::from_array(&env, &[2u8; 32]);
    let predicted_price2 = 2050u128;
    let mut message2 = Bytes::new(&env);
    message2.append(&predicted_price2.to_xdr(&env));
    message2.append(&salt2.to_xdr(&env));
    let commitment2 = env.crypto().sha256(&message2);
    client.commit_prediction(&user2, &commitment2, &150_0000000);

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

    // Both participants should be refunded (0 revealed = under threshold)
    assert_eq!(client.get_pending_winnings(&user1), 100_0000000);
    assert_eq!(client.get_pending_winnings(&user2), 150_0000000);
    assert_eq!(client.get_active_round(), None);
    assert_eq!(client.get_protocol_fee_treasury(), 0);
}

#[test]
fn test_precision_above_threshold_two_revealed_predictions() {
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
    client.create_round(&2000, &Some(1));

    // Both users place revealed predictions
    client.place_precision_prediction(&user1, &100_0000000, &2100u128);
    client.place_precision_prediction(&user2, &100_0000000, &2050u128);

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

    // With 2 revealed predictions, normal settlement occurs
    // user1 predicted 2100, user2 predicted 2050, actual was 2200
    // user1 is closer (diff=100 vs diff=150), so user1 wins
    assert!(client.get_pending_winnings(&user1) > 100_0000000); // Winner gets pot
    assert_eq!(client.get_pending_winnings(&user2), 0); // Loser gets nothing
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_precision_under_threshold_single_participant() {
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

    // Single user places a revealed prediction
    client.place_precision_prediction(&user1, &100_0000000, &2100u128);

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

    // Single participant should be refunded (only 1 revealed = under threshold)
    assert_eq!(client.get_pending_winnings(&user1), 100_0000000);
    assert_eq!(client.get_active_round(), None);
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    
    // Stats should not be mutated
    let user1_stats = client.get_user_stats(&user1);
    assert_eq!(user1_stats.total_wins, 0);
    assert_eq!(user1_stats.total_losses, 0);
}

#[test]
fn test_precision_under_threshold_conservation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user1);
    client.mint_initial(&user2);
    client.mint_initial(&user3);
    client.create_round(&2000, &Some(1));

    // One revealed, two unrevealed
    client.place_precision_prediction(&user1, &100_0000000, &2100u128);
    
    let salt2 = BytesN::from_array(&env, &[2u8; 32]);
    let mut message2 = Bytes::new(&env);
    message2.append(&2050u128.to_xdr(&env));
    message2.append(&salt2.to_xdr(&env));
    let commitment2 = env.crypto().sha256(&message2);
    client.commit_prediction(&user2, &commitment2, &200_0000000);
    
    let salt3 = BytesN::from_array(&env, &[3u8; 32]);
    let mut message3 = Bytes::new(&env);
    message3.append(&2150u128.to_xdr(&env));
    message3.append(&salt3.to_xdr(&env));
    let commitment3 = env.crypto().sha256(&message3);
    client.commit_prediction(&user3, &commitment3, &300_0000000);

    let total_pot = 100_0000000 + 200_0000000 + 300_0000000;

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

    // All participants refunded
    let refund1 = client.get_pending_winnings(&user1);
    let refund2 = client.get_pending_winnings(&user2);
    let refund3 = client.get_pending_winnings(&user3);
    
    assert_eq!(refund1, 100_0000000);
    assert_eq!(refund2, 200_0000000);
    assert_eq!(refund3, 300_0000000);
    
    // Conservation: sum of refunds == total pot
    assert_eq!(refund1 + refund2 + refund3, total_pot);
    
    // No fee charged
    assert_eq!(client.get_protocol_fee_treasury(), 0);
}
