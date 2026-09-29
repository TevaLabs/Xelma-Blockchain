// SPDX-License-Identifier: MIT
//! Round-cleanup audit tests (Issue #507).
//!
//! Every terminal round path — admin cancel, oracle resolve, dispute-window
//! void, dispute-window finalize, and the min-participants fallback refund —
//! must leave **no** per-participant position keys, no shared participant
//! list, and no legacy map keys behind. All paths route through the canonical
//! [`crate::storage`] cleanup API; these tests assert key absence after each
//! terminal transition, including the case where a newer round is already
//! active while an older disputed round is terminalized: the newer round's
//! `ActiveRound` marker must survive.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, DataKeyCore, DataKeyScoped, OraclePayload};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, TryIntoVal,
};

/// Reads the total-pot field from the `("round", "summary")` event for
/// `round_id`. The canonical cleanup must run AFTER archiving: the archive
/// summary re-reads per-participant stake keys, so a cleanup-first ordering
/// silently reports a zero pot for cancelled Precision rounds (Issue #507).
fn summary_total_pot(env: &Env, contract_id: &Address, round_id: u64) -> Option<i128> {
    let events = env.events().all();
    for (_emitter, topics, data) in events.iter() {
        let is_summary = topics.len() == 2
            && topics.get(0).unwrap().try_into_val(env) == Ok(soroban_sdk::symbol_short!("round"))
            && topics.get(1).unwrap().try_into_val(env)
                == Ok(soroban_sdk::symbol_short!("summary"));
        if !is_summary {
            continue;
        }
        // Payload: (version: u32, round_id: u64, status: u32, mode: u32,
        // price_start: u128, price_final: u128, pool_up: i128,
        // pool_down: i128, participant_count: u32, total_pot: i128,
        // fee_amount: i128, settled_at_ledger: u32, confidence: Option<u32>)
        let parsed: Result<(
            u32,
            u64,
            u32,
            u32,
            u128,
            u128,
            i128,
            i128,
            u32,
            i128,
            i128,
            u32,
            Option<u32>,
        ), _> = data.try_into_val(env);
        if let Ok((
            _version,
            ev_round_id,
            _status,
            _mode,
            _price_start,
            _price_final,
            _pool_up,
            _pool_down,
            _participant_count,
            total_pot,
            _fee_amount,
            _settled_at_ledger,
            _confidence,
        )) = parsed
        {
            if ev_round_id == round_id {
                return Some(total_pot);
            }
        }
    }
    None
}

struct Ctx {
    client: VirtualTokenContractClient<'static>,
    contract_id: Address,
    admin: Address,
    oracle: Address,
}

fn setup(env: &Env) -> Ctx {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    Ctx {
        client,
        contract_id,
        admin,
        oracle,
    }
}

/// Asserts that not a single per-participant or shared round key survives a
/// terminal transition. Checks BOTH mode keys per user on purpose: the
/// canonical API removes all three keys regardless of the round's mode, and
/// a stale cross-mode key is exactly the contamination this API exists to
/// prevent.
fn assert_round_fully_clean(env: &Env, contract_id: &Address, round_id: u64, users: &[Address]) {
    env.as_contract(contract_id, || {
        for user in users {
            assert!(
                !env.storage()
                    .persistent()
                    .has(&DataKeyScoped::Position(round_id, user.clone())),
                "Position({round_id}) lingered after terminal path"
            );
            assert!(
                !env.storage()
                    .persistent()
                    .has(&DataKeyScoped::PrecisionPosition(round_id, user.clone())),
                "PrecisionPosition({round_id}) lingered after terminal path"
            );
            assert!(
                !env.storage()
                    .persistent()
                    .has(&DataKeyScoped::PrecisionCommitment(round_id, user.clone())),
                "PrecisionCommitment({round_id}) lingered after terminal path"
            );
        }
        assert!(
            !env.storage()
                .persistent()
                .has(&DataKeyScoped::RoundParticipants(round_id)),
            "RoundParticipants({round_id}) lingered after terminal path"
        );
        assert!(
            !env.storage().persistent().has(&DataKeyCore::Positions),
            "legacy Positions lingered after terminal path"
        );
        assert!(
            !env.storage().persistent().has(&DataKeyCore::UpDownPositions),
            "legacy UpDownPositions lingered after terminal path"
        );
        assert!(
            !env.storage().persistent().has(&DataKeyCore::PrecisionPositions),
            "legacy PrecisionPositions lingered after terminal path"
        );
    });
}

fn payload(env: &Env, contract_id: &Address, start_ledger: u32, nonce: u64, price: u128) -> OraclePayload {
    OraclePayload {
        price,
        timestamp: env.ledger().timestamp(),
        round_id: start_ledger,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    }
}

#[test]
fn cleanup_after_cancel_updown_leaves_no_keys() {
    let env = Env::default();
    let ctx = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    ctx.client.mint_initial(&alice);
    ctx.client.mint_initial(&bob);

    ctx.client.create_round(&1_000u128, &None);
    let round_id = ctx.client.get_last_round_id();
    ctx.client.place_bet(&alice, &60i128, &BetSide::Up);
    ctx.client.place_bet(&bob, &40i128, &BetSide::Down);

    ctx.client.cancel_round(&42u32);

    assert_round_fully_clean(&env, &ctx.contract_id, round_id, &[alice, bob]);
    assert!(ctx.client.get_active_round().is_none());
}

#[test]
fn cleanup_after_cancel_precision_leaves_no_keys() {
    let env = Env::default();
    let ctx = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    ctx.client.mint_initial(&alice);
    ctx.client.mint_initial(&bob);

    ctx.client.create_round(&2_000u128, &Some(1));
    let round_id = ctx.client.get_last_round_id();
    ctx.client.place_precision_prediction(&alice, &30_0000000i128, &2_100u128);
    ctx.client.place_precision_prediction(&bob, &50_0000000i128, &2_300u128);

    ctx.client.cancel_round(&42u32);

    // The cancelled round's summary must report the full 80 XLM pot —
    // proof that archiving ran while the stake keys were still readable.
    // NOTE: read the event before any further client/env calls, because
    // `env.events()` only exposes the most recent invocation's events.
    assert_eq!(
        summary_total_pot(&env, &ctx.contract_id, round_id),
        Some(80_0000000),
        "cancel summary pot must not be zero — archive must precede cleanup"
    );

    assert_round_fully_clean(&env, &ctx.contract_id, round_id, &[alice, bob]);
    assert!(ctx.client.get_active_round().is_none());
}

#[test]
fn cleanup_after_cancel_precision_commit_leaves_no_keys() {
    let env = Env::default();
    let ctx = setup(&env);

    let alice = Address::generate(&env);
    ctx.client.mint_initial(&alice);

    ctx.client.create_round(&2_000u128, &Some(1));
    let round_id = ctx.client.get_last_round_id();
    let hash = soroban_sdk::BytesN::from_array(&env, &[7u8; 32]);
    ctx.client.commit_prediction(&alice, &hash, &80_0000000i128);

    ctx.client.cancel_round(&42u32);

    assert_round_fully_clean(&env, &ctx.contract_id, round_id, &[alice]);
    assert!(ctx.client.get_active_round().is_none());
}

#[test]
fn cleanup_after_resolve_updown_leaves_no_keys() {
    let env = Env::default();
    let ctx = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    ctx.client.mint_initial(&alice);
    ctx.client.mint_initial(&bob);

    ctx.client.create_round(&1_000u128, &None);
    let round_id = ctx.client.get_last_round_id();
    let start_ledger = ctx.client.get_active_round().unwrap().start_ledger;
    ctx.client.place_bet(&alice, &60i128, &BetSide::Up);
    ctx.client.place_bet(&bob, &40i128, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    ctx.client
        .resolve_round(&payload(&env, &ctx.contract_id, start_ledger, 1, 2_000u128));

    assert_round_fully_clean(&env, &ctx.contract_id, round_id, &[alice, bob]);
    assert!(ctx.client.get_active_round().is_none());
}

#[test]
fn cleanup_after_resolve_precision_leaves_no_keys() {
    let env = Env::default();
    let ctx = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    ctx.client.mint_initial(&alice);
    ctx.client.mint_initial(&bob);

    ctx.client.create_round(&2_000u128, &Some(1));
    let round_id = ctx.client.get_last_round_id();
    let start_ledger = ctx.client.get_active_round().unwrap().start_ledger;
    ctx.client.place_precision_prediction(&alice, &30_0000000i128, &2_100u128);
    ctx.client.commit_prediction(
        &bob,
        &soroban_sdk::BytesN::from_array(&env, &[8u8; 32]),
        &50_0000000i128,
    );

    env.ledger().with_mut(|li| li.sequence_number += 12);
    ctx.client
        .resolve_round(&payload(&env, &ctx.contract_id, start_ledger, 1, 2_050u128));

    assert_round_fully_clean(&env, &ctx.contract_id, round_id, &[alice, bob]);
    assert!(ctx.client.get_active_round().is_none());
}

#[test]
fn cleanup_after_void_leaves_no_keys_and_keeps_newer_active_round() {
    let env = Env::default();
    let ctx = setup(&env);

    // Enable a dispute window so resolution is staged rather than immediate.
    ctx.client.set_dispute_ledgers(&5u32);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    ctx.client.mint_initial(&alice);
    ctx.client.mint_initial(&bob);

    ctx.client.create_round(&1_000u128, &None);
    let old_round_id = ctx.client.get_last_round_id();
    let old_start_ledger = ctx.client.get_active_round().unwrap().start_ledger;
    ctx.client.place_bet(&alice, &60i128, &BetSide::Up);
    ctx.client.place_bet(&bob, &40i128, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    ctx.client.resolve_round(&payload(
        &env,
        &ctx.contract_id,
        old_start_ledger,
        1,
        2_000u128,
    ));

    // While the old result is staged, open a NEW round.
    ctx.client.create_round(&1_000u128, &None);
    let new_round_id = ctx.client.get_last_round_id();

    // Void the OLD disputed round.
    ctx.client.void_round(&old_round_id);

    assert_round_fully_clean(&env, &ctx.contract_id, old_round_id, &[alice.clone(), bob.clone()]);

    // The newer round must still be the active one — voiding an older
    // disputed round must not remove its `ActiveRound` marker.
    let active = ctx
        .client
        .get_active_round()
        .expect("newer round's ActiveRound must survive void of an older disputed round");
    assert_eq!(active.round_id, new_round_id);
}

#[test]
fn cleanup_after_finalize_leaves_no_keys_and_keeps_newer_active_round() {
    let env = Env::default();
    let ctx = setup(&env);

    ctx.client.set_dispute_ledgers(&5u32);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    ctx.client.mint_initial(&alice);
    ctx.client.mint_initial(&bob);

    ctx.client.create_round(&1_000u128, &None);
    let old_round_id = ctx.client.get_last_round_id();
    let old_start_ledger = ctx.client.get_active_round().unwrap().start_ledger;
    ctx.client.place_bet(&alice, &60i128, &BetSide::Up);
    ctx.client.place_bet(&bob, &40i128, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    ctx.client.resolve_round(&payload(
        &env,
        &ctx.contract_id,
        old_start_ledger,
        1,
        2_000u128,
    ));

    // New round opens while the old result is staged; then the dispute
    // window elapses and the staged result is finalized.
    ctx.client.create_round(&1_000u128, &None);
    let new_round_id = ctx.client.get_last_round_id();
    env.ledger().with_mut(|li| li.sequence_number += 5);

    ctx.client.finalize_round(&old_round_id);

    assert_round_fully_clean(&env, &ctx.contract_id, old_round_id, &[alice.clone(), bob.clone()]);

    let active = ctx
        .client
        .get_active_round()
        .expect("newer round's ActiveRound must survive finalize of an older disputed round");
    assert_eq!(active.round_id, new_round_id);
}

#[test]
fn cleanup_after_fallback_refund_leaves_no_keys() {
    let env = Env::default();
    let mut ctx = setup(&env);

    ctx.client.set_min_participants(&Some(2u32));

    let alice = Address::generate(&env);
    ctx.client.mint_initial(&alice);

    ctx.client.create_round(&1_000u128, &None);
    let round_id = ctx.client.get_last_round_id();
    let start_ledger = ctx.client.get_active_round().unwrap().start_ledger;
    ctx.client.place_bet(&alice, &100i128, &BetSide::Up);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    ctx.client.resolve_round(&payload(
        &env,
        &ctx.contract_id,
        start_ledger,
        1,
        2_000u128,
    ));

    assert_round_fully_clean(&env, &ctx.contract_id, round_id, &[alice]);
    assert!(ctx.client.get_active_round().is_none());
}
