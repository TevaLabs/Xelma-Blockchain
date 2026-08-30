// SPDX-License-Identifier: MIT
use super::*;

#[test]
fn test_archived_round_after_resolve_matches_settlement() {
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
    client.place_bet(&alice, &50_0000000, &BetSide::Up);
    client.place_bet(&bob, &50_0000000, &BetSide::Down);

    let final_price: u128 = 2_0000000;
    let round_id = resolve_active_round(&client, &env, final_price, 1);

    assert!(client.get_active_round().is_none());
    let archived = client
        .get_archived_round(&round_id)
        .expect("resolved round must be archived");
    assert_eq!(archived.round_id, round_id);
    assert_eq!(archived.price_start, start_price);
    assert_eq!(archived.price_final, final_price);
    assert_eq!(archived.mode, RoundMode::UpDown);
    assert_eq!(archived.status, RoundArchiveStatus::Resolved);
    assert_eq!(archived.pool_up, 50_0000000);
    assert_eq!(archived.pool_down, 50_0000000);
    assert_eq!(archived.participant_count, 2);
    assert_eq!(archived.settled_at_ledger, 12); // default run window end for round created at ledger 0

    assert_eq!(client.get_pending_winnings(&alice), 100_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 0);
}

#[test]
fn test_archived_round_after_cancel() {
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

    let start_price: u128 = 1_5000000;
    client.create_round(&start_price, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    let round_id = client.get_active_round().unwrap().round_id;

    client.cancel_round(&1u32);

    let archived = client
        .get_archived_round(&round_id)
        .expect("cancelled round must be archived");
    assert_eq!(archived.status, RoundArchiveStatus::Cancelled);
    assert_eq!(archived.price_final, 0);
    assert_eq!(archived.participant_count, 1);
    assert_eq!(archived.pool_up, 100_0000000);
    assert_eq!(archived.pool_down, 0);
    assert!(client.is_round_cancelled(&round_id));
}

#[test]
fn test_archived_round_after_precision_resolve() {
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

    let start_price: u128 = 2000;
    client.create_round(&start_price, &Some(1)); // Precision mode
    client.place_precision_prediction(&alice, &30_0000000, &2296);
    client.place_precision_prediction(&bob, &70_0000000, &2299);

    let final_price: u128 = 2298;
    let round_id = resolve_active_round(&client, &env, final_price, 1);

    let archived = client
        .get_archived_round(&round_id)
        .expect("precision resolved round must be archived");
    assert_eq!(archived.round_id, round_id);
    assert_eq!(archived.price_start, start_price);
    assert_eq!(archived.price_final, final_price);
    assert_eq!(archived.mode, RoundMode::Precision);
    assert_eq!(archived.status, RoundArchiveStatus::Resolved);
    assert_eq!(archived.participant_count, 2);

    // Bob is closer to final_price (10_6000000), so wins full pot.
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 100_0000000);
}

#[test]
fn test_archived_round_fallback_refund() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.set_min_participants(&Some(2u32));
    client.mint_initial(&user);

    let start_price: u128 = 1_0000000;
    client.create_round(&start_price, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    let round_id = client.get_active_round().unwrap().round_id;

    let final_price: u128 = 1_2000000;
    resolve_active_round(&client, &env, final_price, 1);

    let archived = client
        .get_archived_round(&round_id)
        .expect("fallback round must be archived");
    assert_eq!(archived.status, RoundArchiveStatus::FallbackRefund);
    assert_eq!(archived.price_final, final_price);
    assert_eq!(archived.participant_count, 1);
    assert_eq!(client.get_pending_winnings(&user), 100_0000000);
}

#[test]
fn test_get_archived_round_missing_returns_none() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    assert!(client.get_archived_round(&999).is_none());
}

#[test]
fn test_get_recent_archived_rounds_order_and_limit() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    let mut round_ids = Vec::new(&env);
    for i in 0..3 {
        client.create_round(&(1_0000000u128 + i as u128), &None);
        round_ids.push_back(resolve_active_round(
            &client,
            &env,
            1_1000000u128 + i as u128,
            i as u64 + 1,
        ));
    }

    assert!(client.get_recent_archived_rounds(&0).is_empty());

    let recent = client.get_recent_archived_rounds(&2);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent.get(0).unwrap().round_id, round_ids.get(2).unwrap());
    assert_eq!(recent.get(1).unwrap().round_id, round_ids.get(1).unwrap());

    let all = client.get_recent_archived_rounds(&10);
    assert_eq!(all.len(), 3);
    assert_eq!(all.get(0).unwrap().round_id, round_ids.get(2).unwrap());
    assert_eq!(all.get(2).unwrap().round_id, round_ids.get(0).unwrap());
}

#[test]
fn test_archive_retention_prunes_oldest() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    // MAX_ARCHIVED_ROUNDS = 128; create 129 resolved rounds to force pruning of round 1.
    let mut first_round_id = 0u64;
    for i in 0..129 {
        client.create_round(&1_0000000u128, &None);
        let round_id = resolve_active_round(&client, &env, 1_1000000u128, i as u64 + 1);
        if i == 0 {
            first_round_id = round_id;
        }
    }

    assert!(
        client.get_archived_round(&first_round_id).is_none(),
        "oldest archive must be pruned once retention limit is exceeded"
    );
    assert!(
        client.get_archived_round(&129).is_some(),
        "newest archive must remain queryable"
    );

    let recent = client.get_recent_archived_rounds(&200);
    assert_eq!(recent.len(), 128);
}

#[test]
fn test_get_user_archived_participation_updown_win() {
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
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &50_0000000, &BetSide::Down);

    let round_id = resolve_active_round(&client, &env, 1_5000000, 1);

    let alice_outcome = client
        .get_user_archived_participation(&alice, &round_id)
        .expect("alice must have an outcome record");
    assert_eq!(alice_outcome.round_mode, 0);
    assert_eq!(alice_outcome.prediction_side, 0);
    assert_eq!(alice_outcome.stake, 100_0000000);
    assert_eq!(alice_outcome.payout, 150_0000000);
    assert_eq!(alice_outcome.outcome, UserOutcomeType::Win);

    let bob_outcome = client
        .get_user_archived_participation(&bob, &round_id)
        .expect("bob must have an outcome record");
    assert_eq!(bob_outcome.round_mode, 0);
    assert_eq!(bob_outcome.prediction_side, 1);
    assert_eq!(bob_outcome.stake, 50_0000000);
    assert_eq!(bob_outcome.payout, 0);
    assert_eq!(bob_outcome.outcome, UserOutcomeType::Loss);
}

#[test]
fn test_get_user_archived_participation_updown_refund() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);

    let start_price: u128 = 1_0000000;
    client.create_round(&start_price, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    let round_id = resolve_active_round(&client, &env, start_price, 1);

    let outcome = client
        .get_user_archived_participation(&alice, &round_id)
        .expect("alice must have an outcome record");
    assert_eq!(outcome.round_mode, 0);
    assert_eq!(outcome.prediction_side, 0);
    assert_eq!(outcome.stake, 100_0000000);
    assert_eq!(outcome.payout, 100_0000000);
    assert_eq!(outcome.outcome, UserOutcomeType::Refund);
}

#[test]
fn test_get_user_archived_participation_precision_win() {
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

    client.create_round(&2000, &Some(1));
    client.place_precision_prediction(&alice, &100_0000000, &2297u128);
    client.place_precision_prediction(&bob, &150_0000000, &2500u128);

    let round_id = resolve_active_round(&client, &env, 2298, 1);

    let alice_outcome = client
        .get_user_archived_participation(&alice, &round_id)
        .expect("alice must have an outcome record");
    assert_eq!(alice_outcome.round_mode, 1);
    assert_eq!(alice_outcome.prediction_side, 2);
    assert_eq!(alice_outcome.predicted_price, 2297);
    assert_eq!(alice_outcome.stake, 100_0000000);
    assert_eq!(alice_outcome.payout, 250_0000000);
    assert_eq!(alice_outcome.outcome, UserOutcomeType::Win);

    let bob_outcome = client
        .get_user_archived_participation(&bob, &round_id)
        .expect("bob must have an outcome record");
    assert_eq!(bob_outcome.round_mode, 1);
    assert_eq!(bob_outcome.prediction_side, 2);
    assert_eq!(bob_outcome.predicted_price, 2500);
    assert_eq!(bob_outcome.stake, 150_0000000);
    assert_eq!(bob_outcome.payout, 0);
    assert_eq!(bob_outcome.outcome, UserOutcomeType::Loss);
}

#[test]
fn test_get_user_archived_participation_cancel() {
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

    let start_price: u128 = 1_0000000;
    client.create_round(&start_price, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    let round_id = client.get_active_round().unwrap().round_id;

    client.cancel_round(&1u32);

    let outcome = client
        .get_user_archived_participation(&user, &round_id)
        .expect("user must have an outcome record");
    assert_eq!(outcome.round_mode, 0);
    assert_eq!(outcome.prediction_side, 0);
    assert_eq!(outcome.stake, 100_0000000);
    assert_eq!(outcome.payout, 100_0000000);
    assert_eq!(outcome.outcome, UserOutcomeType::Void);
}

#[test]
fn test_get_user_archived_participation_missing_returns_none() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    assert!(client
        .get_user_archived_participation(&user, &999)
        .is_none());
}

#[test]
fn test_get_user_archived_participation_min_participants_refund() {
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
    client.set_min_participants(&Some(2u32));

    let start_price: u128 = 1_0000000;
    client.create_round(&start_price, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    let round_id = resolve_active_round(&client, &env, 1_5000000, 1);

    let outcome = client
        .get_user_archived_participation(&user, &round_id)
        .expect("user must have an outcome record");
    assert_eq!(outcome.outcome, UserOutcomeType::Refund);
    assert_eq!(outcome.payout, 100_0000000);
}
