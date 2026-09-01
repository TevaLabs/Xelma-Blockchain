// SPDX-License-Identifier: MIT
//! Tests for domain-separated signed oracle attestations (Issue #263).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::settlement::_build_attestation_message;
use crate::types::OraclePayload;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Bytes, BytesN, Env, IntoVal,
};

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    (client, contract_id, admin, oracle)
}

/// Generates a fresh ed25519 keypair and returns the Soroban-encoded public
/// key alongside the raw dalek signing key used to sign test payloads.
fn generate_keypair(env: &Env) -> (BytesN<32>, SigningKey) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let pubkey_bytes = signing_key.verifying_key().to_bytes();
    (BytesN::from_array(env, &pubkey_bytes), signing_key)
}

fn sign_payload(env: &Env, signing_key: &SigningKey, payload: &OraclePayload) -> BytesN<64> {
    let message: Bytes = _build_attestation_message(env, payload);
    let message_bytes: std::vec::Vec<u8> = message.iter().collect();
    let signature = signing_key.sign(&message_bytes);
    BytesN::from_array(env, &signature.to_bytes())
}

fn base_payload(
    env: &Env,
    contract_id: &Address,
    round_id: u32,
    nonce: u64,
    price: u128,
) -> OraclePayload {
    OraclePayload {
        price,
        timestamp: env.ledger().timestamp(),
        round_id,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    }
}

#[test]
fn test_attestation_disabled_by_default_no_signature_required() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);
    client.create_round(&10_000_000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    assert_eq!(client.get_attestation_key(), None);
    // No attestation configured — resolves with account auth only, exactly
    // as before Issue #263 existed.
    client.resolve_round(&base_payload(&env, &contract_id, 0, 1, 10_000_000));
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_attestation_required_rejects_missing_signature() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);
    let (pubkey, _signing_key) = generate_keypair(&env);

    client.set_attestation_key(&Some(pubkey));
    client.create_round(&10_000_000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let result = client.try_resolve_round(&base_payload(&env, &contract_id, 0, 1, 10_000_000));
    assert_eq!(result, Err(Ok(ContractError::WindowOutOfRange)));
}

#[test]
fn test_attestation_valid_signature_resolves_successfully() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);
    let (pubkey, signing_key) = generate_keypair(&env);

    client.set_attestation_key(&Some(pubkey));
    client.create_round(&10_000_000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let mut payload = base_payload(&env, &contract_id, 0, 1, 10_000_000);
    let signature = sign_payload(&env, &signing_key, &payload);
    payload.attestation = Some(signature);

    client.resolve_round(&payload);
    assert_eq!(client.get_active_round(), None);
}

#[test]
#[should_panic]
fn test_attestation_wrong_key_signature_rejected() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);
    let (pubkey, _correct_key) = generate_keypair(&env);
    let (_other_pubkey, wrong_key) = generate_keypair(&env);

    client.set_attestation_key(&Some(pubkey));
    client.create_round(&10_000_000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let mut payload = base_payload(&env, &contract_id, 0, 1, 10_000_000);
    // Signed with a different key than the one configured on-chain —
    // `ed25519_verify` traps the host, matching the "fail closed" design:
    // an invalid signature must never let settlement continue.
    let signature = sign_payload(&env, &wrong_key, &payload);
    payload.attestation = Some(signature);

    client.resolve_round(&payload);
}

#[test]
#[should_panic]
fn test_attestation_tampered_price_after_signing_rejected() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);
    let (pubkey, signing_key) = generate_keypair(&env);

    client.set_attestation_key(&Some(pubkey));
    client.create_round(&10_000_000, &None);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let mut payload = base_payload(&env, &contract_id, 0, 1, 10_000_000);
    let signature = sign_payload(&env, &signing_key, &payload);
    // Tamper with the price after signing — the signature no longer covers
    // this message, so verification must fail even though the signature
    // bytes themselves are well-formed.
    payload.price = 20_000_000;
    payload.attestation = Some(signature);

    client.resolve_round(&payload);
}

#[test]
fn test_attestation_key_disabled_after_clearing() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);
    let (pubkey, signing_key) = generate_keypair(&env);

    client.set_attestation_key(&Some(pubkey));
    client.create_round(&10_000_000, &None);
    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let mut payload = base_payload(&env, &contract_id, 0, 1, 10_000_000);
    let signature = sign_payload(&env, &signing_key, &payload);
    payload.attestation = Some(signature);
    client.resolve_round(&payload);

    // Clear the key and settle the next round with no signature at all.
    client.set_attestation_key(&None);
    assert_eq!(client.get_attestation_key(), None);

    client.create_round(&10_000_000, &None);
    env.ledger().with_mut(|li| {
        li.sequence_number = 24;
    });
    client.resolve_round(&base_payload(&env, &contract_id, 12, 2, 10_000_000));
    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_set_attestation_key_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (pubkey, _signing_key) = generate_keypair(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_attestation_key",
            args: (Some(pubkey.clone()),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_set_attestation_key(&Some(pubkey));
    assert!(result.is_err());
}
