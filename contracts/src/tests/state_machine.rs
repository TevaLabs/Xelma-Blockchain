// SPDX-License-Identifier: MIT
//! State-machine tests for explicit round phase transitions with illegal-transition guards.
//!
//! Issue #258 — Every action has a required phase. Operations attempted in the
//! wrong phase must fail with `ContractError::IllegalPhaseTransition`.
//!
//! ## Allowed Transitions
//!
//! | Phase        | place_bet | place_precision_prediction | commit_prediction | reveal_prediction | resolve_round | cancel_round |
//! |--------------|-----------|---------------------------|-------------------|-------------------|---------------|--------------|
//! | Betting      | ✓         | ✓                         | ✓                 | ✗                 | ✗             | ✓            |
//! | Running      | ✗         | ✗                         | ✗                 | ✓                 | ✗             | ✓            |
//! | Resolvable   | ✗         | ✗                         | ✗                 | ✗                 | ✓             | ✓            |

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env,
};

/// Helper: create a round, mint user, place an Up bet, advance ledger.
#[allow(dead_code)]
fn setup_betting_round(env: &Env, client: &VirtualTokenContractClient) -> (Address, Address, Address) {
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    let user = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user);
    client.create_round(&10_0000u128, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    (admin, oracle, user)
}

// ─── Betting phase: allowed actions ─────────────────────────────────────────

/// place_bet succeeds in Betting phase.
#[test]
fn test_place_bet_succeeds_in_betting_phase() {
    let env = Env::default();
    let (client, _admin, _oracle) = {
        let a = Address::generate(&env);
        let o = Address::generate(&env);
        env.mock_all_auths();
        let contract_id = env.register(VirtualTokenContract, ());
        let c = VirtualTokenContractClient::new(&env, &contract_id);
        c.initialize(&a, &o);
        (c, a, o)
    };
    let user = Address::generate(&env);
    client.mint_initial(&user);

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &None);

    // Should succeed — ledger 100 is within Betting window (bet_end_ledger = 106)
    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert!(result.is_ok(), "place_bet should succeed in Betting phase");
}

/// place_precision_prediction succeeds in Betting phase.
#[test]
fn test_place_precision_prediction_succeeds_in_betting_phase() {
    let env = Env::default();
    let (client, _admin, _oracle) = {
        let a = Address::generate(&env);
        let o = Address::generate(&env);
        env.mock_all_auths();
        let contract_id = env.register(VirtualTokenContract, ());
        let c = VirtualTokenContractClient::new(&env, &contract_id);
        c.initialize(&a, &o);
        (c, a, o)
    };
    let user = Address::generate(&env);
    client.mint_initial(&user);

    env.ledger().with_mut(|li| li.sequence_number = 100);
    // Precision mode (mode=1)
    client.create_round(&10_0000u128, &Some(1));

    let result = client.try_place_precision_prediction(&user, &100_0000000, &12_5000);
    assert!(
        result.is_ok(),
        "place_precision_prediction should succeed in Betting phase"
    );
}

/// commit_prediction succeeds in Betting phase.
#[test]
fn test_commit_prediction_succeeds_in_betting_phase() {
    let env = Env::default();
    let (client, _admin, _oracle) = {
        let a = Address::generate(&env);
        let o = Address::generate(&env);
        env.mock_all_auths();
        let contract_id = env.register(VirtualTokenContract, ());
        let c = VirtualTokenContractClient::new(&env, &contract_id);
        c.initialize(&a, &o);
        (c, a, o)
    };
    let user = Address::generate(&env);
    client.mint_initial(&user);

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &Some(1));

    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let result = client.try_commit_prediction(&user, &hash, &100_0000000);
    assert!(
        result.is_ok(),
        "commit_prediction should succeed in Betting phase"
    );
}

// ─── Betting phase: illegal transitions ────────────────────────────────────

/// reveal_prediction fails in Betting phase.
///
/// Uses a valid salt (two distinct non-zero bytes) so the entropy gate passes
/// and the test reaches the actual phase guard.
#[test]
fn test_reveal_prediction_fails_in_betting_phase() {
    use soroban_sdk::xdr::ToXdr;

    let env = Env::default();
    let (client, _admin, _oracle) = {
        let a = Address::generate(&env);
        let o = Address::generate(&env);
        env.mock_all_auths();
        let contract_id = env.register(VirtualTokenContract, ());
        let c = VirtualTokenContractClient::new(&env, &contract_id);
        c.initialize(&a, &o);
        (c, a, o)
    };
    let user = Address::generate(&env);
    client.mint_initial(&user);

    let predicted_price = 12_5000u128;
    // Valid salt: first byte differs from the rest so entropy check passes
    let salt = BytesN::from_array(&env, &[1u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8, 2u8]);

    // Compute the commitment hash
    let mut preimage = soroban_sdk::Bytes::new(&env);
    preimage.append(&predicted_price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash: BytesN<32> = env.crypto().sha256(&preimage).into();

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &Some(1));

    // Commit in Betting phase with the computed hash
    client.commit_prediction(&user, &hash, &100_0000000);

    // Try to reveal while still in Betting phase (ledger 100 < bet_end_ledger = 106)
    let result = client.try_reveal_prediction(&user, &predicted_price, &salt);
    assert_eq!(
        result,
        Err(Ok(ContractError::IllegalPhaseTransition)),
        "reveal_prediction in Betting phase must return IllegalPhaseTransition"
    );
}

/// resolve_round fails in Betting phase.
#[test]
fn test_resolve_round_fails_in_betting_phase() {
    let env = Env::default();
    let (client, _admin, _oracle) = {
        let a = Address::generate(&env);
        let o = Address::generate(&env);
        env.mock_all_auths();
        let contract_id = env.register(VirtualTokenContract, ());
        let c = VirtualTokenContractClient::new(&env, &contract_id);
        c.initialize(&a, &o);
        (c, a, o)
    };

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &None);

    let payload = OraclePayload {
        price: 11_0000,
        timestamp: env.ledger().timestamp(),
        round_id: 100,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    };

    let result = client.try_resolve_round(&payload);
    assert_eq!(
        result,
        Err(Ok(ContractError::IllegalPhaseTransition)),
        "resolve_round in Betting phase must return IllegalPhaseTransition"
    );
}

// ─── Running phase: allowed actions ────────────────────────────────────────

/// reveal_prediction succeeds in Running phase.
///
/// Proper commit-reveal flow: commit a SHA-256 hash of (predicted_price || salt)
/// in Betting phase, then reveal it in Running phase with the preimage.
/// Uses a valid salt (two distinct non-zero bytes) so the entropy gate passes.
#[test]
fn test_reveal_prediction_succeeds_in_running_phase() {
    use soroban_sdk::xdr::ToXdr;

    let env = Env::default();
    let (client, _admin, _oracle) = {
        let a = Address::generate(&env);
        let o = Address::generate(&env);
        let c = {
            env.mock_all_auths();
            let contract_id = env.register(VirtualTokenContract, ());
            VirtualTokenContractClient::new(&env, &contract_id)
        };
        c.initialize(&a, &o);
        (c, a, o)
    };
    let user = Address::generate(&env);
    client.mint_initial(&user);

    let predicted_price = 12_5000u128;
    // Valid salt: first byte differs from the rest so salt_has_minimum_entropy passes
    let salt = BytesN::from_array(&env, &[0x42u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8]);

    // Compute the commitment hash: sha256(predicted_price.to_xdr() || salt.to_xdr())
    let mut preimage = soroban_sdk::Bytes::new(&env);
    preimage.append(&predicted_price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash: BytesN<32> = env.crypto().sha256(&preimage).into();

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &Some(1));

    // Commit in Betting phase
    client.commit_prediction(&user, &hash, &100_0000000);

    // Advance to Running phase (bet_end_ledger = 106, end_ledger = 112)
    env.ledger().with_mut(|li| li.sequence_number = 110);

    // Reveal with correct preimage — should succeed
    let result = client.try_reveal_prediction(&user, &predicted_price, &salt);
    assert!(
        result.is_ok(),
        "reveal_prediction should succeed in Running phase with correct preimage"
    );
}

// ─── Running phase: illegal transitions ────────────────────────────────────

/// place_bet fails in Running phase.
#[test]
fn test_place_bet_fails_in_running_phase() {
    let env = Env::default();
    let (client, _admin, _oracle) = {
        // One user bets in Betting, another tries in Running
        let contract_id = env.register(VirtualTokenContract, ());
        let c = VirtualTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        c.initialize(&admin, &oracle);
        (c, admin, oracle)
    };
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    client.mint_initial(&user1);
    client.mint_initial(&user2);

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &None);
    client.place_bet(&user1, &100_0000000, &BetSide::Up);

    // Advance to Running phase (ledger 106 = bet_end_ledger)
    env.ledger().with_mut(|li| li.sequence_number = 108);

    let result = client.try_place_bet(&user2, &100_0000000, &BetSide::Down);
    assert_eq!(
        result,
        Err(Ok(ContractError::IllegalPhaseTransition)),
        "place_bet in Running phase must return IllegalPhaseTransition"
    );
}

/// place_precision_prediction fails in Running phase.
#[test]
fn test_place_precision_prediction_fails_in_running_phase() {
    let env = Env::default();
    let (client, _admin, _oracle) = {
        let contract_id = env.register(VirtualTokenContract, ());
        let c = VirtualTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        c.initialize(&admin, &oracle);
        (c, admin, oracle)
    };
    let user = Address::generate(&env);
    client.mint_initial(&user);

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &Some(1));

    // Advance to Running phase
    env.ledger().with_mut(|li| li.sequence_number = 108);

    let result = client.try_place_precision_prediction(&user, &100_0000000, &12_5000);
    assert_eq!(
        result,
        Err(Ok(ContractError::IllegalPhaseTransition)),
        "place_precision_prediction in Running phase must return IllegalPhaseTransition"
    );
}

/// commit_prediction fails in Running phase.
#[test]
fn test_commit_prediction_fails_in_running_phase() {
    let env = Env::default();
    let (client, _admin, _oracle) = {
        let contract_id = env.register(VirtualTokenContract, ());
        let c = VirtualTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        c.initialize(&admin, &oracle);
        (c, admin, oracle)
    };
    let user = Address::generate(&env);
    client.mint_initial(&user);

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &Some(1));

    // Advance to Running phase
    env.ledger().with_mut(|li| li.sequence_number = 108);

    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let result = client.try_commit_prediction(&user, &hash, &100_0000000);
    assert_eq!(
        result,
        Err(Ok(ContractError::IllegalPhaseTransition)),
        "commit_prediction in Running phase must return IllegalPhaseTransition"
    );
}

/// resolve_round fails in Running phase.
#[test]
fn test_resolve_round_fails_in_running_phase() {
    let env = Env::default();
    let (client, _admin, _oracle) = {
        let contract_id = env.register(VirtualTokenContract, ());
        let c = VirtualTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let oracle_addr = Address::generate(&env);
        env.mock_all_auths();
        c.initialize(&admin, &oracle_addr);
        (c, admin, oracle_addr)
    };

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &None);

    // Advance to Running phase (bet_end_ledger = 106, end_ledger = 112)
    env.ledger().with_mut(|li| li.sequence_number = 108);

    let payload = OraclePayload {
        price: 11_0000,
        timestamp: env.ledger().timestamp(),
        round_id: 100,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    };

    let result = client.try_resolve_round(&payload);
    assert_eq!(
        result,
        Err(Ok(ContractError::IllegalPhaseTransition)),
        "resolve_round in Running phase must return IllegalPhaseTransition"
    );
}

// ─── Resolvable phase: allowed actions ─────────────────────────────────────

/// resolve_round succeeds in Resolvable phase.
#[test]
fn test_resolve_round_succeeds_in_resolvable_phase() {
    let env = Env::default();
    let (client, _admin, _oracle) = {
        let contract_id = env.register(VirtualTokenContract, ());
        let c = VirtualTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let oracle_addr = Address::generate(&env);
        env.mock_all_auths();
        c.initialize(&admin, &oracle_addr);
        (c, admin, oracle_addr)
    };
    let user = Address::generate(&env);
    client.mint_initial(&user);

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // Advance to Resolvable phase (end_ledger = 112)
    env.ledger().with_mut(|li| {
        li.sequence_number = 120;
        li.timestamp = 1_000_000;
    });

    let payload = OraclePayload {
        price: 11_0000,
        timestamp: env.ledger().timestamp(),
        round_id: 100,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    };

    let result = client.try_resolve_round(&payload);
    assert!(
        result.is_ok(),
        "resolve_round should succeed in Resolvable phase"
    );
}

// ─── Resolvable phase: illegal transitions ─────────────────────────────────

/// place_bet fails in Resolvable phase.
#[test]
fn test_place_bet_fails_in_resolvable_phase() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    let user = Address::generate(&env);
    client.mint_initial(&user);

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &None);

    // Advance to Resolvable phase (end_ledger = start=100 + run=12 = 112)
    env.ledger().with_mut(|li| li.sequence_number = 120);

    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(
        result,
        Err(Ok(ContractError::IllegalPhaseTransition)),
        "place_bet in Resolvable phase must return IllegalPhaseTransition"
    );
}

/// cancel_round always succeeds regardless of phase.
#[test]
fn test_cancel_round_succeeds_in_any_phase() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Cancel in Betting
    client.create_round(&10_0000u128, &None);
    let result = client.try_cancel_round(&0);
    assert!(result.is_ok(), "cancel_round should succeed in Betting phase");

    // Create another and cancel in Running
    env.ledger().with_mut(|li| li.sequence_number = 200);
    client.create_round(&10_0000u128, &None);
    env.ledger().with_mut(|li| li.sequence_number = 208);
    let result = client.try_cancel_round(&0);
    assert!(result.is_ok(), "cancel_round should succeed in Running phase");

    // Create another and cancel in Resolvable
    env.ledger().with_mut(|li| li.sequence_number = 300);
    client.create_round(&10_0000u128, &None);
    env.ledger().with_mut(|li| li.sequence_number = 320);
    let result = client.try_cancel_round(&0);
    assert!(
        result.is_ok(),
        "cancel_round should succeed in Resolvable phase"
    );
}

/// reveal_prediction fails in Resolvable phase.
///
/// Uses a valid salt (two distinct non-zero bytes) so the entropy gate passes
/// and the test reaches the actual phase guard.
#[test]
fn test_reveal_prediction_fails_in_resolvable_phase() {
    use soroban_sdk::xdr::ToXdr;

    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    let user = Address::generate(&env);
    client.mint_initial(&user);

    let predicted_price = 12_5000u128;
    let salt = BytesN::from_array(&env, &[0x42u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8, 0x43u8]);

    // Compute and commit the proper hash
    let mut preimage = soroban_sdk::Bytes::new(&env);
    preimage.append(&predicted_price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash: BytesN<32> = env.crypto().sha256(&preimage).into();

    env.ledger().with_mut(|li| li.sequence_number = 100);
    client.create_round(&10_0000u128, &Some(1));
    client.commit_prediction(&user, &hash, &100_0000000);

    // Advance to Resolvable phase (end_ledger = 112)
    env.ledger().with_mut(|li| li.sequence_number = 120);

    let result = client.try_reveal_prediction(&user, &predicted_price, &salt);
    assert_eq!(
        result,
        Err(Ok(ContractError::IllegalPhaseTransition)),
        "reveal_prediction in Resolvable phase must return IllegalPhaseTransition"
    );
}
