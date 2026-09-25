// SPDX-License-Identifier: MIT
//! Regression tests for the sealed batch auction Up/Down commit-reveal flow.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::BetSide;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    xdr::ToXdr,
    Address, Bytes, BytesN, Env,
};

fn sealed_order_hash(
    env: &Env,
    side: &BetSide,
    amount: i128,
    price_guess: u128,
    salt: &BytesN<32>,
) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&side.to_xdr(env));
    preimage.append(&amount.to_xdr(env));
    preimage.append(&price_guess.to_xdr(env));
    preimage.append(&salt.to_xdr(env));
    env.crypto().sha256(&preimage).into()
}

#[test]
fn test_sealed_batch_auction_legacy_mode_remains_unchanged() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&10_0000000, &None);

    assert!(!client.get_sealed_batch_auction());
    let result = client.try_place_bet(&user, &100, &BetSide::Up);
    assert_eq!(result, Ok(Ok(())));
    assert!(client.get_user_position(&user).is_some());

    client.set_sealed_batch_auction(&true);
    assert!(client.get_sealed_batch_auction());

    let result = client.try_place_bet(&user, &75, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::InvalidMode)));
}

#[test]
fn test_sealed_batch_commit_reveal_finalize_accepts_only_revealed_orders() {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 0;
    });

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.create_round(&12_0000000, &None);
    client.set_sealed_batch_auction(&true);

    let alice_salt = BytesN::from_array(
        &env,
        &[
            1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 32,
        ],
    );
    let bob_salt = BytesN::from_array(
        &env,
        &[
            33u8, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53,
            54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64,
        ],
    );
    let alice_hash = sealed_order_hash(&env, &BetSide::Up, 150, 11_5000000u128, &alice_salt);
    let bob_hash = sealed_order_hash(&env, &BetSide::Down, 250, 12_7000000u128, &bob_salt);

    client.commit_order(&alice, &150, &BetSide::Up, &11_5000000u128, &alice_hash);
    client.commit_order(&bob, &250, &BetSide::Down, &12_7000000u128, &bob_hash);

    env.ledger().with_mut(|li| {
        li.sequence_number = 6;
    });

    client.reveal_order(&alice, &150, &BetSide::Up, &11_5000000u128, &alice_salt);
    client.finalize_sealed_batch();

    let round = client.get_active_round().expect("round should still exist");
    assert_eq!(round.pool_up, 150);
    assert_eq!(round.pool_down, 0);

    let alice_pos = client.get_user_position(&alice).expect("alice accepted");
    assert_eq!(alice_pos.amount, 150);
    assert_eq!(alice_pos.side, BetSide::Up);
    assert!(client.get_user_position(&bob).is_none());
}
