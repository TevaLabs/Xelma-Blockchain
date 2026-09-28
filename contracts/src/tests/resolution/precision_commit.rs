use super::*;

#[test]
fn test_precision_commit_reveal_resolution_payout_with_unrevealed_participants() {
    use soroban_sdk::xdr::ToXdr;
    use soroban_sdk::{Bytes, BytesN};

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

    client.create_round(&1_0000000, &Some(1));

    // Alice commits and reveals guess of 2000
    let price_alice = 2000u128;
    let salt_alice = test_salt(&env, 1);
    let mut preimage_alice = Bytes::new(&env);
    preimage_alice.append(&price_alice.to_xdr(&env));
    preimage_alice.append(&salt_alice.clone().to_xdr(&env));
    let hash_alice = env.crypto().sha256(&preimage_alice);
    let committed_hash_alice: BytesN<32> = hash_alice.into();
    client.commit_prediction(&alice, &committed_hash_alice, &100_0000000);

    // Bob commits but does NOT reveal
    let price_bob = 2200u128;
    let salt_bob = test_salt(&env, 2);
    let mut preimage_bob = Bytes::new(&env);
    preimage_bob.append(&price_bob.to_xdr(&env));
    preimage_bob.append(&salt_bob.clone().to_xdr(&env));
    let hash_bob = env.crypto().sha256(&preimage_bob);
    let committed_hash_bob: BytesN<32> = hash_bob.into();
    client.commit_prediction(&bob, &committed_hash_bob, &150_0000000);

    // Move to reveal window
    env.ledger().with_mut(|li| {
        li.sequence_number = 7;
    });

    // Only Alice reveals
    client.reveal_prediction(&alice, &price_alice, &salt_alice);

    // Move past end of round to allow resolution
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    // Resolve round with actual price 2050
    client.resolve_round(&OraclePayload {
        price: 2050,
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

    // Total pot is 250 (Alice 100 + Bob 150)
    // Alice is the only revealed participant, so she wins the entire pot
    assert_eq!(client.get_pending_winnings(&alice), 250_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 0);
}

