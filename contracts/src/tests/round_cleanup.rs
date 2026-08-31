// SPDX-License-Identifier: MIT
//! Coverage for the canonical round-cleanup API in `storage.rs`
//! (`clear_round_storage` / `clear_round_storage_keep_active`).
//!
//! Every terminal path (cancel, immediate resolve, dispute-window void,
//! dispute-window finalize) must route through one of these two functions
//! so no `Position` / `PrecisionPosition` / `PrecisionCommitment` /
//! `RoundParticipants` key is ever left behind. The dispute-window paths
//! must additionally leave a *newer* round's `ActiveRound` marker intact,
//! since a fresh round is allowed to start while an older result is still
//! inside its dispute window.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, DataKeyCore, DataKeyScoped};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    let alice = Address::generate(env);
    let bob = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    (client, contract_id, admin, alice, bob)
}

fn assert_round_keys_absent(
    env: &Env,
    contract_id: &Address,
    round_id: u64,
    alice: &Address,
    bob: &Address,
) {
    env.as_contract(contract_id, || {
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::Position(round_id, alice.clone())));
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::Position(round_id, bob.clone())));
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::PrecisionPosition(round_id, alice.clone())));
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::PrecisionPosition(round_id, bob.clone())));
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::PrecisionCommitment(round_id, alice.clone())));
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::PrecisionCommitment(round_id, bob.clone())));
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKeyScoped::RoundParticipants(round_id)));
    });
}

/// `cancel_round` targets the current `ActiveRound`, so the canonical
/// cleanup must remove every participant/position key *and* `ActiveRound`.
#[test]
fn cancel_round_leaves_no_participant_or_active_round_keys() {
    let env = Env::default();
    let (client, contract_id, _admin, alice, bob) = setup(&env);

    client.create_round(&1_000u128, &None);
    let round_id = client.get_active_round().unwrap().round_id;
    client.place_bet(&alice, &60i128, &BetSide::Up);
    client.place_bet(&bob, &40i128, &BetSide::Down);

    client.cancel_round(&0u32);

    assert_round_keys_absent(&env, &contract_id, round_id, &alice, &bob);
    env.as_contract(&contract_id, || {
        assert!(!env.storage().persistent().has(&DataKeyCore::ActiveRound));
    });
}

/// An immediate resolve (no dispute window configured) settles the round
/// while it is still `ActiveRound`, so cleanup must remove `ActiveRound`
/// along with every position/participant key.
#[test]
fn immediate_resolve_leaves_no_participant_or_active_round_keys() {
    let env = Env::default();
    let (client, contract_id, _admin, alice, bob) = setup(&env);

    client.create_round(&1_000u128, &None);
    let round = client.get_active_round().unwrap();
    let round_id = round.round_id;
    client.place_bet(&alice, &60i128, &BetSide::Up);
    client.place_bet(&bob, &40i128, &BetSide::Down);

    env.ledger()
        .with_mut(|ledger| ledger.sequence_number = round.end_ledger);
    client.resolve_round(&crate::types::OraclePayload {
        price: 2_000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    assert_round_keys_absent(&env, &contract_id, round_id, &alice, &bob);
    env.as_contract(&contract_id, || {
        assert!(!env.storage().persistent().has(&DataKeyCore::ActiveRound));
    });
}

/// While round A sits inside its dispute window, round B is allowed to
/// start and become the new `ActiveRound`. Voiding A must remove A's
/// position/participant keys without disturbing B's `ActiveRound` marker —
/// this is exactly the scenario `clear_round_storage_keep_active` exists for.
#[test]
fn void_during_dispute_window_keeps_newer_active_round_intact() {
    let env = Env::default();
    let (client, contract_id, _admin, alice, bob) = setup(&env);
    client.set_dispute_ledgers(&5u32);

    client.create_round(&1_000u128, &None);
    let round_a = client.get_active_round().unwrap();
    let round_a_id = round_a.round_id;
    client.place_bet(&alice, &60i128, &BetSide::Up);
    client.place_bet(&bob, &40i128, &BetSide::Down);

    env.ledger()
        .with_mut(|ledger| ledger.sequence_number = round_a.end_ledger);
    client.resolve_round(&crate::types::OraclePayload {
        price: 2_000,
        timestamp: env.ledger().timestamp(),
        round_id: round_a.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    // Resolving under a dispute window releases `ActiveRound`, so a new
    // round can start while A awaits finalization/void.
    client.create_round(&1_500u128, &None);
    let round_b_id = client.get_active_round().unwrap().round_id;
    assert_ne!(round_a_id, round_b_id);

    client.void_round(&round_a_id);

    assert_round_keys_absent(&env, &contract_id, round_a_id, &alice, &bob);
    assert_eq!(client.get_active_round().unwrap().round_id, round_b_id);
}

/// Same scenario as above, but finalizing (not voiding) A after the dispute
/// window elapses must equally leave B's `ActiveRound` marker untouched.
#[test]
fn finalize_after_dispute_window_keeps_newer_active_round_intact() {
    let env = Env::default();
    let (client, contract_id, _admin, alice, bob) = setup(&env);
    client.set_dispute_ledgers(&5u32);

    client.create_round(&1_000u128, &None);
    let round_a = client.get_active_round().unwrap();
    let round_a_id = round_a.round_id;
    client.place_bet(&alice, &60i128, &BetSide::Up);
    client.place_bet(&bob, &40i128, &BetSide::Down);

    env.ledger()
        .with_mut(|ledger| ledger.sequence_number = round_a.end_ledger);
    client.resolve_round(&crate::types::OraclePayload {
        price: 2_000,
        timestamp: env.ledger().timestamp(),
        round_id: round_a.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    client.create_round(&1_500u128, &None);
    let round_b_id = client.get_active_round().unwrap().round_id;

    env.ledger()
        .with_mut(|ledger| ledger.sequence_number += 5);
    client.finalize_round(&round_a_id);

    assert_round_keys_absent(&env, &contract_id, round_a_id, &alice, &bob);
    assert_eq!(client.get_active_round().unwrap().round_id, round_b_id);
}
