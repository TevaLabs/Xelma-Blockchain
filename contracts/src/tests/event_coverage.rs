//! Event coverage and completeness verification tests (Issue #117).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, OraclePayload};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Bytes, BytesN, Env, TryIntoVal,
};

fn setup() -> (Env, Address, Address, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, admin, oracle, client)
}

#[test]
fn test_event_coverage_mint_initial() {
    let (env, _, _, client) = setup();
    let user = Address::generate(&env);

    client.mint_initial(&user);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("mint"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("initial"))
    );
    assert_eq!(data.try_into_val(&env), Ok((user, 1000_0000000i128)));
}

#[test]
fn test_event_coverage_create_round() {
    let (env, _, _, client) = setup();

    client.create_round(&1_0000000, &None); // UpDown Mode (0)

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("round"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("created"))
    );
    assert_eq!(
        data.try_into_val(&env),
        Ok((1u64, 1_0000000u128, 0u32, 6u32, 12u32, 0u32))
    );
}

#[test]
fn test_event_coverage_set_windows() {
    let (env, _, _, client) = setup();

    client.set_windows(&10, &20);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("windows"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("updated"))
    );
    assert_eq!(data.try_into_val(&env), Ok((10u32, 20u32)));
}

#[test]
fn test_event_coverage_place_bet() {
    let (env, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);

    client.place_bet(&user, &100_0000000, &BetSide::Up);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("bet"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("placed"))
    );
    assert_eq!(
        data.try_into_val(&env),
        Ok((user, 1u64, 100_0000000i128, 0u32))
    );
}

#[test]
fn test_event_coverage_commit_and_reveal() {
    let (env, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1)); // Precision mode

    let price = 500u128;
    let salt = BytesN::from_array(&env, &[1; 32]);
    let mut preimage = Bytes::new(&env);
    preimage.append(&price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash = env.crypto().sha256(&preimage);

    let committed_hash: BytesN<32> = hash.into();
    client.commit_prediction(&user, &committed_hash.clone(), &100_0000000);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("commit"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("predict"))
    );
    assert_eq!(
        data.try_into_val(&env),
        Ok((user.clone(), 1u64, committed_hash, 100_0000000i128))
    );

    // Move ledger beyond bet window to allow reveal
    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });

    client.reveal_prediction(&user, &price, &salt);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("reveal"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("predict"))
    );
    assert_eq!(
        data.try_into_val(&env),
        Ok((user, 1u64, price, 100_0000000i128))
    );
}

#[test]
fn test_event_coverage_resolve_round() {
    let (env, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // Advance ledger to resolve
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1,
    });

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("round"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("resolved"))
    );
    assert_eq!(data.try_into_val(&env), Ok((1u64, 1_2000000u128, 0u32)));
}

#[test]
fn test_event_coverage_cancel_round() {
    let (env, _, _, client) = setup();
    client.create_round(&1_0000000, &None);

    client.cancel_round(&99u32);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("round"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("cancelled"))
    );
    assert_eq!(data.try_into_val(&env), Ok((1u64, 99u32, 0i128, 0i128)));
}

#[test]
fn test_event_coverage_claim_winnings() {
    let (env, _, _, client) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_2000000, // went up -> win
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1,
    });

    client.claim_winnings(&user);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_contract, topics, data) = last_event;

    assert_eq!(topics.len(), 2);
    assert_eq!(
        topics.get(0).unwrap().try_into_val(&env),
        Ok(symbol_short!("claim"))
    );
    assert_eq!(
        topics.get(1).unwrap().try_into_val(&env),
        Ok(symbol_short!("winnings"))
    );
    assert_eq!(data.try_into_val(&env), Ok((user, 100_0000000i128)));
}
