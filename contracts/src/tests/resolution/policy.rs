// SPDX-License-Identifier: MIT
use super::*;

#[test]
fn test_precision_payout_policy_config() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    assert_eq!(client.get_precision_payout_policy(), 0);

    client.set_precision_payout_policy(&1);
    assert_eq!(client.get_precision_payout_policy(), 1);

    let res = client.try_set_precision_payout_policy(&2);
    assert_eq!(res, Err(Ok(ContractError::InvalidMode)));
    assert_eq!(client.get_precision_payout_policy(), 1);

    client.set_precision_payout_policy(&0);
    assert_eq!(client.get_precision_payout_policy(), 0);
}

#[test]
fn test_resolve_precision_stake_weighted_policy() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let user_c = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    client.set_precision_payout_policy(&1);

    client.mint_initial(&user_a);
    client.mint_initial(&user_b);
    client.mint_initial(&user_c);

    let mut sorted_users = std::vec![user_a.clone(), user_b.clone(), user_c.clone()];
    sorted_users.sort();
    let lowest_user = sorted_users[0].clone();
    let middle_user = sorted_users[1].clone();
    let highest_user = sorted_users[2].clone();

    client.create_round(&1_0000000, &Some(1));

    client.place_precision_prediction(&lowest_user, &100_0000000i128, &1500u128);
    client.place_precision_prediction(&middle_user, &200_0000000i128, &1500u128);
    client.place_precision_prediction(&highest_user, &300_0000000i128, &2000u128);

    env.ledger().with_mut(|li| {
        li.sequence_number = 20;
    });

    client.resolve_round(&OraclePayload {
        price: 1500u128,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert_eq!(
        client.balance(&lowest_user),
        1000_0000000 - 100_0000000 + 200_0000000
    );
    assert_eq!(
        client.balance(&middle_user),
        1000_0000000 - 200_0000000 + 400_0000000
    );
    assert_eq!(client.balance(&highest_user), 1000_0000000 - 300_0000000);

    let events = env.events().all();
    let resolved_event = events
        .iter()
        .find(|e| {
            let (_contract, topics, _data) = e;
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("round"))
                && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("resolved"))
        })
        .unwrap();

    let resolved_data: (u64, u128, u32, Option<u32>, u32) =
        resolved_event.2.clone().try_into_val(&env).unwrap();
    assert_eq!(resolved_data.4, 1);
}

#[test]
fn test_precision_stake_weighted_conservation_remainder() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    client.set_precision_payout_policy(&1);

    client.mint_initial(&user_a);
    client.mint_initial(&user_b);

    let mut sorted_users = std::vec![user_a.clone(), user_b.clone()];
    sorted_users.sort();
    let lowest_user = sorted_users[0].clone();
    let other_user = sorted_users[1].clone();

    client.create_round(&1_0000000, &Some(1));

    client.place_precision_prediction(&lowest_user, &100i128, &1500u128);
    client.place_precision_prediction(&other_user, &200i128, &1500u128);

    env.ledger().with_mut(|li| {
        li.sequence_number = 20;
    });

    client.resolve_round(&OraclePayload {
        price: 1500u128,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert_eq!(
        client.balance(&lowest_user),
        1000_0000000 - 100i128 + 34i128
    );
    assert_eq!(client.balance(&other_user), 1000_0000000 - 200i128 + 66i128);
}
