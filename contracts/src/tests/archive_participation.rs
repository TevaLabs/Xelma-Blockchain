use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, OraclePayload, RoundArchiveStatus};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, Vec};

/// Advances the ledger past round end and resolves with the given price.
fn resolve_active_round(
    client: &VirtualTokenContractClient,
    env: &Env,
    final_price: u128,
    nonce: u64,
) -> u64 {
    let round = client.get_active_round().unwrap();
    let round_id = round.round_id;
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });
    client.resolve_round(&OraclePayload {
        price: final_price,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    });
    round_id
}

// ─── Participation recorded after resolve ───────────────────────────────────

#[test]
fn test_archived_participation_after_resolve() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &50_0000000, &BetSide::Up);
    client.place_bet(&bob, &50_0000000, &BetSide::Down);

    let round_id = resolve_active_round(&client, &env, 2_0000000, 1);

    let alice_history = client.get_user_archive_history(&alice, &0, &10);
    let bob_history = client.get_user_archive_history(&bob, &0, &10);

    assert_eq!(alice_history.len(), 1);
    assert_eq!(alice_history.get(0).unwrap().round_id, round_id);
    assert_eq!(alice_history.get(0).unwrap().status, RoundArchiveStatus::Resolved);

    assert_eq!(bob_history.len(), 1);
    assert_eq!(bob_history.get(0).unwrap().round_id, round_id);
    assert_eq!(bob_history.get(0).unwrap().status, RoundArchiveStatus::Resolved);
}

#[test]
fn test_archived_participation_after_cancel() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    let round_id = client.get_active_round().unwrap().round_id;

    client.cancel_round(&1u32);

    let history = client.get_user_archive_history(&alice, &0, &10);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().round_id, round_id);
    assert_eq!(history.get(0).unwrap().status, RoundArchiveStatus::Cancelled);
}

#[test]
fn test_archived_participation_after_fallback_refund() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.set_min_participants(&Some(2u32));

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    let round_id = client.get_active_round().unwrap().round_id;

    resolve_active_round(&client, &env, 1_2000000, 1);

    let history = client.get_user_archive_history(&user, &0, &10);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().round_id, round_id);
    assert_eq!(history.get(0).unwrap().status, RoundArchiveStatus::FallbackRefund);
}

// ─── User with no participation returns empty ───────────────────────────────

#[test]
fn test_archived_participation_no_history() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let stranger = Address::generate(&env);

    let history = client.get_user_archive_history(&stranger, &0, &10);
    assert_eq!(history.len(), 0);
}

#[test]
fn test_archived_participation_non_participant_after_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &50_0000000, &BetSide::Up);
    resolve_active_round(&client, &env, 2_0000000, 1);

    let bob_history = client.get_user_archive_history(&bob, &0, &10);
    assert_eq!(bob_history.len(), 0);
}

// ─── Pagination: ordering ───────────────────────────────────────────────────

#[test]
fn test_archived_participation_newest_first() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let user = Address::generate(&env);
    client.mint_initial(&user);

    for i in 0..5 {
        client.create_round(&(1_0000000u128 + i as u128), &None);
        client.place_bet(&user, &10_0000000, &BetSide::Up);
        resolve_active_round(&client, &env, 2_0000000 + i as u128, i as u64 + 1);
    }

    let all = client.get_user_archive_history(&user, &0, &10);
    assert_eq!(all.len(), 5);

    for i in 1..all.len() {
        let prev = all.get(i - 1).unwrap().round_id;
        let curr = all.get(i).unwrap().round_id;
        assert!(
            prev > curr,
            "history must be newest-first: {} <= {}",
            prev,
            curr
        );
    }
}

// ─── Pagination: offset / limit ─────────────────────────────────────────────

#[test]
fn test_archived_participation_page_respects_offset_and_limit() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let user = Address::generate(&env);
    client.mint_initial(&user);

    let mut round_ids: Vec<u64> = Vec::new(&env);
    for i in 0..5 {
        client.create_round(&(1_0000000u128 + i as u128), &None);
        client.place_bet(&user, &10_0000000, &BetSide::Up);
        round_ids.push_back(resolve_active_round(
            &client,
            &env,
            2_0000000 + i as u128,
            i as u64 + 1,
        ));
    }

    let page0 = client.get_user_archive_history(&user, &0, &1);
    let page1 = client.get_user_archive_history(&user, &1, &1);
    let page2 = client.get_user_archive_history(&user, &2, &1);

    assert_eq!(page0.len(), 1);
    assert_eq!(page1.len(), 1);
    assert_eq!(page2.len(), 1);

    assert_eq!(page0.get(0).unwrap().round_id, round_ids.get(4).unwrap());
    assert_eq!(page1.get(0).unwrap().round_id, round_ids.get(3).unwrap());
    assert_eq!(page2.get(0).unwrap().round_id, round_ids.get(2).unwrap());
}

#[test]
fn test_archived_participation_full_page_matches_all() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let user = Address::generate(&env);
    client.mint_initial(&user);

    for i in 0..3 {
        client.create_round(&(1_0000000u128 + i as u128), &None);
        client.place_bet(&user, &10_0000000, &BetSide::Up);
        resolve_active_round(&client, &env, 2_0000000 + i as u128, i as u64 + 1);
    }

    let page = client.get_user_archive_history(&user, &0, &10);
    assert_eq!(page.len(), 3);
}

// ─── Pagination: bounds ─────────────────────────────────────────────────────

#[test]
fn test_archived_participation_offset_past_end_is_empty() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &10_0000000, &BetSide::Up);
    resolve_active_round(&client, &env, 2_0000000, 1);

    let page_at_end = client.get_user_archive_history(&user, &1, &10);
    assert_eq!(page_at_end.len(), 0);

    let page_far = client.get_user_archive_history(&user, &999, &10);
    assert_eq!(page_far.len(), 0);
}

#[test]
fn test_archived_participation_zero_limit_is_empty() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &10_0000000, &BetSide::Up);
    resolve_active_round(&client, &env, 2_0000000, 1);

    let page = client.get_user_archive_history(&user, &0, &0);
    assert_eq!(page.len(), 0);
}

#[test]
fn test_archived_participation_limit_is_capped() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let user = Address::generate(&env);
    client.mint_initial(&user);

    for i in 0..3 {
        client.create_round(&(1_0000000u128 + i as u128), &None);
        client.place_bet(&user, &10_0000000, &BetSide::Up);
        resolve_active_round(&client, &env, 2_0000000 + i as u128, i as u64 + 1);
    }

    let page = client.get_user_archive_history(&user, &0, &1_000_000);
    assert_eq!(page.len(), 3);
}

// ─── Multi-user isolation ───────────────────────────────────────────────────

#[test]
fn test_archived_participation_multi_user_isolation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &50_0000000, &BetSide::Up);
    let round1_id = resolve_active_round(&client, &env, 2_0000000, 1);

    client.create_round(&1_0000000, &None);
    client.place_bet(&bob, &30_0000000, &BetSide::Down);
    let round2_id = resolve_active_round(&client, &env, 1_5000000, 2);

    let alice_hist = client.get_user_archive_history(&alice, &0, &10);
    assert_eq!(alice_hist.len(), 1);
    assert_eq!(alice_hist.get(0).unwrap().round_id, round1_id);

    let bob_hist = client.get_user_archive_history(&bob, &0, &10);
    assert_eq!(bob_hist.len(), 1);
    assert_eq!(bob_hist.get(0).unwrap().round_id, round2_id);
}

// ─── Precision mode ─────────────────────────────────────────────────────────

#[test]
fn test_archived_participation_precision_mode() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&2000, &Some(1));
    client.place_precision_prediction(&alice, &30_0000000, &2296);
    client.place_precision_prediction(&bob, &70_0000000, &2299);

    let round_id = resolve_active_round(&client, &env, 2298, 1);

    let alice_hist = client.get_user_archive_history(&alice, &0, &10);
    let bob_hist = client.get_user_archive_history(&bob, &0, &10);

    assert_eq!(alice_hist.len(), 1);
    assert_eq!(alice_hist.get(0).unwrap().round_id, round_id);
    assert_eq!(bob_hist.len(), 1);
    assert_eq!(bob_hist.get(0).unwrap().round_id, round_id);
}
