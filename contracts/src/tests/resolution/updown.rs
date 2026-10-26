use super::*;

#[test]
fn test_resolve_round_price_unchanged() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::UpDownPositions, &positions);

        // Update round pools to match positions
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
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::UpDownPositions, &positions);

        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKeyCore::ActiveRound)
            .unwrap();
        round.pool_up = 300_0000000;
        round.pool_down = 150_0000000;
        env.storage()
            .persistent()
            .set(&DataKeyCore::ActiveRound, &round);
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
    client.update_oracle_heartbeat(&0u32);

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
            .set(&DataKeyCore::UpDownPositions, &positions);

        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKeyCore::ActiveRound)
            .unwrap();
        round.pool_up = 100_0000000;
        round.pool_down = 200_0000000;
        env.storage()
            .persistent()
            .set(&DataKeyCore::ActiveRound, &round);
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
    client.update_oracle_heartbeat(&0u32);

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
    client.update_oracle_heartbeat(&0u32);

    // Try to resolve without creating a round - should return error
    let result = client.try_resolve_round(&OraclePayload {
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
        attestation: None,
    });
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

#[test]
fn test_protocol_fee_not_collected_on_one_sided_resolution() {
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

    client.schedule_protocol_fee_bps(&Some(500u32));
    env.ledger().with_mut(|li| li.sequence_number = 2_000);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    let start_price: u128 = 1_500_0000;
    client.create_round(&start_price, &None);

    // Place bets on only one side (Down) to create a one-sided pool
    client.place_bet(&alice, &100_000_0000, &BetSide::Down);
    client.place_bet(&bob, &50_000_0000, &BetSide::Down);

    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(
            alice.clone(),
            UserPosition {
                amount: 100_000_0000,
                side: BetSide::Down,
            },
        );
        positions.set(
            bob.clone(),
            UserPosition {
                amount: 50_000_0000,
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
        round.pool_up = 0;
        round.pool_down = 150_000_0000;
        env.storage()
            .persistent()
            .set(&DataKeyCore::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
        price: start_price,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 7u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    // One-sided resolution with protocol fees enabled must NOT collect a fee
    assert_eq!(
        count_protocol_fee_events(&env),
        0,
        "one-sided resolution MUST NOT emit a fee event"
    );
    assert_eq!(
        client.get_protocol_fee_treasury(),
        0,
        "one-sided resolution MUST NOT credit the treasury"
    );
    let payouts = sum_pending_payouts(&env, &client.address, &[alice.clone(), bob.clone()]);
    assert_eq!(
        payouts, 150_000_0000i128,
        "all participants refunded their full stake on one-sided pool"
    );
}
