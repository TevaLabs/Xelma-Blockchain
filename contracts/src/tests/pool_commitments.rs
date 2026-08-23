// SPDX-License-Identifier: MIT
//! Tests for pool-aggregate commitments (Observability / ZK-friendly design).

extern crate alloc;

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload, RoundMode};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Bytes, BytesN, Env, TryIntoVal,
};

fn setup() -> (Env, Address, VirtualTokenContractClient<'static>, Address, Address) {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    (env, contract_id, client, admin, oracle)
}

fn resolve_active(env: &Env, client: &VirtualTokenContractClient, contract_id: &Address, price: u128) {
    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| li.sequence_number = round.end_ledger);
    client.resolve_round(&OraclePayload {
        price,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });
}

/// Recomputes `sha256(round_id || seq || pool_up || pool_down ||
/// precision_total_stake || salt)` exactly like `commitments::_compute_commitment`.
fn recompute(
    env: &Env,
    round_id: u64,
    seq: u32,
    pool_up: i128,
    pool_down: i128,
    precision_total_stake: i128,
    salt: &BytesN<32>,
) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&round_id.to_xdr(env));
    preimage.append(&seq.to_xdr(env));
    preimage.append(&pool_up.to_xdr(env));
    preimage.append(&pool_down.to_xdr(env));
    preimage.append(&precision_total_stake.to_xdr(env));
    preimage.append(&salt.to_xdr(env));
    env.crypto().sha256(&preimage).into()
}

fn last_commit_event(env: &Env) -> Option<(u64, u32, BytesN<32>, u32)> {
    let events = env.events().all();
    events.iter().rev().find_map(|e| {
        let (_contract, topics, data) = e;
        if topics.len() == 2
            && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("pool"))
            && topics.get(1).unwrap().try_into_val(env) == Ok(symbol_short!("commit"))
        {
            data.try_into_val(env).ok()
        } else {
            None
        }
    })
}

// ─── Commitment advances on mutating actions ─────────────────────────────────

#[test]
fn test_commitment_advances_on_place_bet_and_hash_matches() {
    let (env, contract_id, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);

    let round = client.get_active_round().unwrap();
    assert!(client.get_pool_commitment(&round.round_id).is_none());

    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    // Event carries only the hash/seq/ledger — never raw pool values. Must be
    // read immediately: the test event log only retains the most recent
    // top-level contract invocation.
    let (event_round_id, event_seq, event_hash, event_ledger) =
        last_commit_event(&env).expect("commit event must be emitted");

    let commitment = client
        .get_pool_commitment(&round.round_id)
        .expect("commitment must exist after first bet");
    assert_eq!(commitment.seq, 1);

    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 100_0000000);

    assert_eq!(event_round_id, round.round_id);
    assert_eq!(event_seq, commitment.seq);
    assert_eq!(event_hash, commitment.commitment);
    assert_eq!(event_ledger, commitment.ledger);

    // Second bet advances the sequence again.
    let bob = Address::generate(&env);
    client.mint_initial(&bob);
    client.place_bet(&bob, &50_0000000, &BetSide::Down);
    let commitment2 = client.get_pool_commitment(&round.round_id).unwrap();
    assert_eq!(commitment2.seq, 2);
    assert_ne!(commitment2.commitment, commitment.commitment);
}

#[test]
fn test_commitment_advances_on_precision_flows() {
    let (env, contract_id, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&1_0000000, &Some(1));
    let round = client.get_active_round().unwrap();

    client.place_precision_prediction(&alice, &100_0000000, &2200u128);
    let c1 = client.get_pool_commitment(&round.round_id).unwrap();
    assert_eq!(c1.seq, 1);

    let bob = Address::generate(&env);
    client.mint_initial(&bob);

    let predicted_price = 2100u128;
    let mut salt_bytes = [0x42u8; 32];
    salt_bytes[0] = 0x91;
    salt_bytes[31] = 0x07;
    let salt = BytesN::from_array(&env, &salt_bytes);
    let mut preimage = Bytes::new(&env);
    preimage.append(&predicted_price.to_xdr(&env));
    preimage.append(&salt.clone().to_xdr(&env));
    let hash: BytesN<32> = env.crypto().sha256(&preimage).into();

    client.commit_prediction(&bob, &hash, &50_0000000);
    let c2 = client.get_pool_commitment(&round.round_id).unwrap();
    assert_eq!(c2.seq, 2);

    env.ledger().with_mut(|li| li.sequence_number = round.bet_end_ledger);
    client.reveal_prediction(&bob, &predicted_price, &salt);
    let c3 = client.get_pool_commitment(&round.round_id).unwrap();
    assert_eq!(c3.seq, 3);
}

// ─── Additive-only: legacy stats surface unchanged ───────────────────────────

#[test]
fn test_round_pool_stats_unaffected_by_commitments() {
    let (env, contract_id, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    // get_round_pool_stats keeps exposing raw values in real time — the
    // commitment channel is additive, not a replacement (Issue: additive-
    // only decision, avoids a MAJOR/breaking change per COMPATIBILITY_POLICY.md).
    let stats = client.get_round_pool_stats().unwrap();
    assert_eq!(stats.total_up_stake, 100_0000000);
    assert_eq!(stats.mode, RoundMode::UpDown);
}

// ─── Force-open timing and idempotency ───────────────────────────────────────

#[test]
fn test_open_pool_commitment_rejected_before_bet_end() {
    let (env, contract_id, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    let result = client.try_open_pool_commitment(&round.round_id);
    assert_eq!(result, Err(Ok(ContractError::InvalidRevealWindow)));
    assert!(client.get_pool_opening(&round.round_id).is_none());
}

#[test]
fn test_open_pool_commitment_succeeds_after_bet_end_and_matches_storage() {
    let (env, contract_id, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    // Captured immediately: the test event log only retains the most recent
    // top-level contract invocation, so this is what an indexer streaming
    // events live would have last seen before bet_end.
    let (.., last_seen_commit_hash, _) =
        last_commit_event(&env).expect("commit event must be emitted");

    env.ledger().with_mut(|li| li.sequence_number = round.bet_end_ledger);
    client.open_pool_commitment(&round.round_id);

    let opening = client.get_pool_opening(&round.round_id).unwrap();
    let live_round = client.get_active_round().unwrap();
    assert_eq!(opening.pool_up, live_round.pool_up);
    assert_eq!(opening.pool_down, live_round.pool_down);
    assert_eq!(opening.pool_up, 100_0000000);

    // The opening's salt + values must reproduce the last published commitment.
    let commitment = client.get_pool_commitment(&round.round_id).unwrap();
    let recomputed = recompute(
        &env,
        opening.round_id,
        opening.seq,
        opening.pool_up,
        opening.pool_down,
        opening.precision_total_stake,
        &opening.salt,
    );
    assert_eq!(recomputed, commitment.commitment);

    // And it matches the last ("pool","commit") event an indexer would have seen.
    assert_eq!(recomputed, last_seen_commit_hash);
}

#[test]
fn test_open_pool_commitment_idempotent() {
    let (env, contract_id, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    env.ledger().with_mut(|li| li.sequence_number = round.bet_end_ledger);
    client.open_pool_commitment(&round.round_id);

    let result = client.try_open_pool_commitment(&round.round_id);
    assert_eq!(result, Err(Ok(ContractError::AlreadyRevealed)));
}

// ─── Missing-opening policy: auto-open at every terminal transition ─────────

#[test]
fn test_resolve_without_manual_open_still_opens() {
    let (env, contract_id, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    // Fail-closed: nobody opened it yet, before bet_end.
    assert!(client.get_pool_opening(&round.round_id).is_none());

    resolve_active(&env, &client, &contract_id, 2_0000000);

    let opening = client
        .get_pool_opening(&round.round_id)
        .expect("resolve_round must auto-open the pool commitment");
    assert_eq!(opening.pool_up, 100_0000000);
    assert_eq!(opening.pool_down, 0);
}

#[test]
fn test_cancel_round_without_manual_open_still_opens() {
    let (env, contract_id, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    client.cancel_round(&0u32);

    let opening = client
        .get_pool_opening(&round.round_id)
        .expect("cancel_round must auto-open the pool commitment");
    assert_eq!(opening.pool_up, 100_0000000);
}

#[test]
fn test_fallback_refund_without_manual_open_still_opens() {
    let (env, contract_id, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.set_min_participants(&Some(5));
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    resolve_active(&env, &client, &contract_id, 2_0000000);

    let opening = client
        .get_pool_opening(&round.round_id)
        .expect("fallback-refund path must auto-open the pool commitment");
    assert_eq!(opening.pool_up, 100_0000000);
}

#[test]
fn test_pool_opening_none_for_unopened_round() {
    let (env, contract_id, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();
    assert_eq!(client.get_pool_opening(&round.round_id), None);
    assert_eq!(client.get_pool_opening(&999u64), None);
}

// ─── Gas-bounded: cost stays flat regardless of participant count ───────────

#[test]
fn test_commitment_cost_is_flat_across_round_size() {
    let (env, contract_id, client, _admin, _oracle) = setup();
    // Scaling-behaviour test, not a precise gas measurement (see
    // cost_benchmarks.rs for that) — reset so a large round can't trip the
    // default interpreted-host budget on its own.
    env.cost_estimate().budget().reset_unlimited();

    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    let users: alloc::vec::Vec<Address> = (0..40).map(|_| Address::generate(&env)).collect();
    for (i, u) in users.iter().enumerate() {
        client.mint_initial(u);
        let side = if i % 2 == 0 { BetSide::Up } else { BetSide::Down };
        client.place_bet(u, &10_0000000, &side);
    }

    // The commitment struct itself stays a single fixed-size record per
    // round throughout — advancing never grows with participant count.
    let commitment = client.get_pool_commitment(&round.round_id).unwrap();
    assert_eq!(commitment.seq, 40);

    env.ledger().with_mut(|li| li.sequence_number = round.bet_end_ledger);
    client.open_pool_commitment(&round.round_id);
    let opening = client.get_pool_opening(&round.round_id).unwrap();
    assert_eq!(opening.pool_up + opening.pool_down, 10_0000000 * 40);
}
