// SPDX-License-Identifier: MIT
//! Authoritative suite for oracle replay + domain-binding validation (Issue #550).
//!
//! Consolidates the nonce-reuse and network/contract domain-binding checks for
//! **both** settlement paths into one place:
//!   - `resolve_round`       — single-feed / legacy oracle path
//!   - `resolve_round_multi` — multi-feed quorum path
//!
//! Both paths share the same `ConsumedOracleNonce(round_id, nonce)` replay
//! guard and the same `network_id` / `contract_addr` domain-binding checks
//! (see `contracts/src/settlement.rs`), so each property below is asserted
//! once per path rather than being re-derived ad hoc.
//!
//! Full end-to-end cross-round replay *scenarios* (cancel/recreate a round,
//! replay a payload signed for the prior round, etc.) are exercised as
//! red-team attacker sequences in the adversarial suite
//! (`contracts/src/tests/adversarial/oracle.rs`, Issue #372) and are not
//! duplicated here; this suite instead adds the multi-feed cross-round-replay
//! case, which had no coverage anywhere before this change.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{DataKeyScoped, MultiFeedPayload, OraclePayload, OracleQuorumConfig};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env,
};

// ─────────────────────────────────────────────────────────────────────────
// Shared setup
// ─────────────────────────────────────────────────────────────────────────

/// Standard contract setup: initialize, heartbeat, and open a round.
/// Returns the client, contract id, and the created round.
fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address, crate::types::Round) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();

    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    (client, contract_id, round)
}

/// Advances the ledger sequence to (at least) the round's end ledger, so
/// `resolve_round`/`resolve_round_multi` pass their `RoundNotEnded` check.
/// The ledger timestamp is left untouched (rather than jumped forward to an
/// arbitrary value) so `round.start_timestamp` stays inside the round's
/// timestamp-window bounds — see `payload_timestamp` below.
fn advance_to_resolvable(env: &Env, round: &crate::types::Round) {
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });
}

/// A timestamp guaranteed to fall inside the round's validity window
/// (`[start_timestamp - skew, start_timestamp + duration + skew]`), whatever
/// the configured skew or round duration happen to be.
fn payload_timestamp(round: &crate::types::Round) -> u64 {
    round.start_timestamp
}

fn single_feed_payload(
    env: &Env,
    contract_id: &Address,
    round: &crate::types::Round,
    nonce: u64,
) -> OraclePayload {
    OraclePayload {
        price: 1_5000000,
        timestamp: payload_timestamp(round),
        round_id: round.start_ledger,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    }
}

/// A lenient quorum config (3 sources, all must agree exactly) so multi-feed
/// tests can focus on nonce/domain validation rather than quorum math.
fn lenient_quorum_config() -> OracleQuorumConfig {
    OracleQuorumConfig {
        min_observations: 3,
        quorum_threshold: 3,
        outlier_threshold_bps: 500,
    }
}

fn multi_feed_payload(
    env: &Env,
    contract_id: &Address,
    round: &crate::types::Round,
    nonce: u64,
) -> MultiFeedPayload {
    MultiFeedPayload {
        prices: soroban_sdk::vec![env, 1_5000000u128, 1_5000000u128, 1_5000000u128],
        sources: soroban_sdk::vec![env, 0u32, 1u32, 2u32],
        round_id: round.start_ledger,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        timestamp: payload_timestamp(round),
    }
}

fn seed_consumed_nonce(env: &Env, contract_id: &Address, round_id: u64, nonce: u64) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyScoped::ConsumedOracleNonce(round_id, nonce), &true);
    });
}

// ─────────────────────────────────────────────────────────────────────────
// Single-feed (`resolve_round`) — nonce reuse
// ─────────────────────────────────────────────────────────────────────────

/// A nonce already consumed for a round must be rejected on re-submission.
#[test]
fn test_resolve_round_duplicate_nonce_rejected() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    advance_to_resolvable(&env, &round);

    seed_consumed_nonce(&env, &contract_id, round.round_id, 42u64);

    let result = client.try_resolve_round(&single_feed_payload(&env, &contract_id, &round, 42u64));
    assert_eq!(result, Err(Ok(ContractError::OracleNonceReused)));
}

/// A fresh, unique nonce resolves normally and records the consumed marker.
#[test]
fn test_resolve_round_unique_nonce_resolves() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    advance_to_resolvable(&env, &round);

    client.resolve_round(&single_feed_payload(&env, &contract_id, &round, 7u64));

    assert_eq!(client.get_active_round(), None);
    env.as_contract(&contract_id, || {
        let consumed: bool = env
            .storage()
            .persistent()
            .get(&DataKeyScoped::ConsumedOracleNonce(round.round_id, 7u64))
            .unwrap_or(false);
        assert!(consumed, "resolved nonce must be marked consumed");
    });
}

/// Boundary nonces (0 and u64::MAX) are rejected on reuse for the same round.
#[test]
fn test_resolve_round_nonce_boundary_values() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    advance_to_resolvable(&env, &round);

    seed_consumed_nonce(&env, &contract_id, round.round_id, 0u64);
    seed_consumed_nonce(&env, &contract_id, round.round_id, u64::MAX);

    let zero = client.try_resolve_round(&single_feed_payload(&env, &contract_id, &round, 0u64));
    assert_eq!(zero, Err(Ok(ContractError::OracleNonceReused)));

    let max = client.try_resolve_round(&single_feed_payload(&env, &contract_id, &round, u64::MAX));
    assert_eq!(max, Err(Ok(ContractError::OracleNonceReused)));
}

// ─────────────────────────────────────────────────────────────────────────
// Single-feed (`resolve_round`) — domain binding (network / contract)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_resolve_round_wrong_network_id_rejected() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    advance_to_resolvable(&env, &round);

    let mut payload = single_feed_payload(&env, &contract_id, &round, 1u64);
    payload.network_id = BytesN::from_array(&env, &[0xFFu8; 32]);

    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::OracleNetworkMismatch)));
}

#[test]
fn test_resolve_round_wrong_contract_addr_rejected() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    advance_to_resolvable(&env, &round);

    let mut payload = single_feed_payload(&env, &contract_id, &round, 1u64);
    payload.contract_addr = Address::generate(&env);

    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::OracleNetworkMismatch)));
}

#[test]
fn test_resolve_round_both_network_and_contract_wrong() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    advance_to_resolvable(&env, &round);

    let mut payload = single_feed_payload(&env, &contract_id, &round, 1u64);
    payload.network_id = BytesN::from_array(&env, &[0xFFu8; 32]);
    payload.contract_addr = Address::generate(&env);

    // Network is checked first, so we get OracleNetworkMismatch.
    let result = client.try_resolve_round(&payload);
    assert_eq!(result, Err(Ok(ContractError::OracleNetworkMismatch)));
}

#[test]
fn test_resolve_round_valid_domain_context_resolves() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    advance_to_resolvable(&env, &round);

    client.resolve_round(&single_feed_payload(&env, &contract_id, &round, 1u64));
    assert_eq!(client.get_active_round(), None);
}

// ─────────────────────────────────────────────────────────────────────────
// Multi-feed (`resolve_round_multi`) — nonce reuse
//
// No dedicated test coverage existed for these properties on the multi-feed
// path before this change (`resolve_round_multi` was only exercised
// incidentally by the randomized `fuzz_lifecycle` suite, which never asserts
// on these specific rejection reasons).
// ─────────────────────────────────────────────────────────────────────────

/// A nonce already consumed for a round must be rejected on re-submission,
/// mirroring the single-feed guard — both paths share the same
/// `ConsumedOracleNonce(round_id, nonce)` key.
#[test]
fn test_resolve_round_multi_duplicate_nonce_rejected() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    client.set_oracle_quorum_config(&Some(lenient_quorum_config()));
    advance_to_resolvable(&env, &round);

    seed_consumed_nonce(&env, &contract_id, round.round_id, 42u64);

    let result =
        client.try_resolve_round_multi(&multi_feed_payload(&env, &contract_id, &round, 42u64));
    assert_eq!(result, Err(Ok(ContractError::OracleNonceReused)));
}

/// A fresh, unique nonce resolves normally and records the consumed marker.
#[test]
fn test_resolve_round_multi_unique_nonce_resolves() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    client.set_oracle_quorum_config(&Some(lenient_quorum_config()));
    advance_to_resolvable(&env, &round);

    client.resolve_round_multi(&multi_feed_payload(&env, &contract_id, &round, 7u64));

    assert_eq!(client.get_active_round(), None);
    env.as_contract(&contract_id, || {
        let consumed: bool = env
            .storage()
            .persistent()
            .get(&DataKeyScoped::ConsumedOracleNonce(round.round_id, 7u64))
            .unwrap_or(false);
        assert!(consumed, "resolved nonce must be marked consumed");
    });
}

// ─────────────────────────────────────────────────────────────────────────
// Multi-feed (`resolve_round_multi`) — domain binding (network / contract)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_resolve_round_multi_wrong_network_id_rejected() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    client.set_oracle_quorum_config(&Some(lenient_quorum_config()));
    advance_to_resolvable(&env, &round);

    let mut payload = multi_feed_payload(&env, &contract_id, &round, 1u64);
    payload.network_id = BytesN::from_array(&env, &[0xFFu8; 32]);

    let result = client.try_resolve_round_multi(&payload);
    assert_eq!(result, Err(Ok(ContractError::OracleNetworkMismatch)));
}

#[test]
fn test_resolve_round_multi_wrong_contract_addr_rejected() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    client.set_oracle_quorum_config(&Some(lenient_quorum_config()));
    advance_to_resolvable(&env, &round);

    let mut payload = multi_feed_payload(&env, &contract_id, &round, 1u64);
    payload.contract_addr = Address::generate(&env);

    let result = client.try_resolve_round_multi(&payload);
    assert_eq!(result, Err(Ok(ContractError::OracleNetworkMismatch)));
}

#[test]
fn test_resolve_round_multi_both_network_and_contract_wrong() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    client.set_oracle_quorum_config(&Some(lenient_quorum_config()));
    advance_to_resolvable(&env, &round);

    let mut payload = multi_feed_payload(&env, &contract_id, &round, 1u64);
    payload.network_id = BytesN::from_array(&env, &[0xFFu8; 32]);
    payload.contract_addr = Address::generate(&env);

    // Network is checked first, so we get OracleNetworkMismatch.
    let result = client.try_resolve_round_multi(&payload);
    assert_eq!(result, Err(Ok(ContractError::OracleNetworkMismatch)));
}

#[test]
fn test_resolve_round_multi_valid_domain_context_resolves() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    client.set_oracle_quorum_config(&Some(lenient_quorum_config()));
    advance_to_resolvable(&env, &round);

    client.resolve_round_multi(&multi_feed_payload(&env, &contract_id, &round, 1u64));
    assert_eq!(client.get_active_round(), None);
}

// ─────────────────────────────────────────────────────────────────────────
// Multi-feed (`resolve_round_multi`) — cross-round replay
//
// Genuinely new coverage: a payload signed for a settled round must not be
// replayable against the round that follows it. This is the multi-feed
// analogue of `test_cross_round_payload_replay_blocked` in the adversarial
// oracle suite, which only covers the single-feed path.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn test_resolve_round_multi_cross_round_payload_replay_blocked() {
    let env = Env::default();
    let (client, contract_id, round) = setup(&env);
    client.set_oracle_quorum_config(&Some(lenient_quorum_config()));
    advance_to_resolvable(&env, &round);

    // Resolve the first round and capture a payload bound to it.
    let stale_payload = multi_feed_payload(&env, &contract_id, &round, 1u64);
    client.resolve_round_multi(&stale_payload);
    assert_eq!(client.get_active_round(), None);

    // A new round gets a new (distinct) `start_ledger`, since ledger sequence
    // has advanced and `create_round` binds `start_ledger` to it.
    client.create_round(&1_5000000, &None);
    let second_round = client.get_active_round().unwrap();
    assert_ne!(second_round.start_ledger, round.start_ledger);

    advance_to_resolvable(&env, &second_round);

    // Replaying the payload signed for the first round must fail binding
    // validation against the now-active second round.
    let replay = client.try_resolve_round_multi(&stale_payload);
    assert_eq!(replay, Err(Ok(ContractError::InvalidOracleRound)));
    assert!(client.get_active_round().is_some());
}
