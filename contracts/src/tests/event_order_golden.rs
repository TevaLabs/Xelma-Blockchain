// SPDX-License-Identifier: MIT
//! Golden field-order tests for governance, TTL and season events (Issue #568).
//!
//! Indexers decode these events positionally, so the topic symbols, the
//! number of payload fields and their order are part of the public API
//! (see `docs/EVENT_SCHEMA.md`). Each test pins, for a single invocation:
//!
//! 1. the **exact ordered list** of topic pairs the call emits — so a new
//!    event slipped in before/after, or a reordering, is caught; and
//! 2. every payload decoded into its **exact tuple type** — so adding,
//!    removing, retyping or swapping a field fails the test.
//!
//! If one of these tests fails, the change is a breaking event-schema change:
//! update `docs/EVENT_SCHEMA.md` and `CHANGELOG.md` together with the golden.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, DataKeyCore, GovAction, OraclePayload};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    vec, Address, Env, Symbol, TryFromVal, Val,
};
use std::vec::Vec as StdVec;

// ─── Harness ─────────────────────────────────────────────────────────────────

struct Ctx {
    env: Env,
    client: VirtualTokenContractClient<'static>,
    contract_id: Address,
    admin: Address,
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 1_000;
        li.timestamp = 10_000;
    });
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    Ctx {
        env,
        client,
        contract_id,
        admin,
    }
}

/// Events published by *this contract* during the most recent invocation,
/// as `(topic0, topic1, data)`.
fn last_call_events(ctx: &Ctx) -> StdVec<(Symbol, Symbol, Val)> {
    let mut out = StdVec::new();
    for (contract, topics, data) in ctx.env.events().all().iter() {
        if contract != ctx.contract_id {
            continue;
        }
        assert_eq!(
            topics.len(),
            2,
            "every protocol event has exactly two topics"
        );
        let t0 = Symbol::try_from_val(&ctx.env, &topics.get(0).unwrap()).unwrap();
        let t1 = Symbol::try_from_val(&ctx.env, &topics.get(1).unwrap()).unwrap();
        out.push((t0, t1, data));
    }
    out
}

/// Asserts the ordered topic list of the last invocation.
fn assert_topic_sequence(ctx: &Ctx, expected: &[(Symbol, Symbol)]) -> StdVec<Val> {
    let events = last_call_events(ctx);
    let got: StdVec<(Symbol, Symbol)> = events
        .iter()
        .map(|(a, b, _)| (a.clone(), b.clone()))
        .collect();
    assert_eq!(got, expected.to_vec(), "event topic order changed");
    events.into_iter().map(|(_, _, d)| d).collect()
}

/// Decodes `data` as exactly `T` (tuple arity and field types must match).
fn decode<T: TryFromVal<Env, Val>>(env: &Env, data: &Val) -> T {
    T::try_from_val(env, data).unwrap_or_else(|_| panic!("event payload shape changed"))
}

fn sym(env: &Env, s: &str) -> Symbol {
    Symbol::new(env, s)
}

// ─── Governance: propose / approve / execute / cancel ────────────────────────

#[test]
fn golden_gov_approver_set() {
    let ctx = setup();
    let approver = Address::generate(&ctx.env);
    ctx.client.set_gov_approver(&approver);

    let data = assert_topic_sequence(&ctx, &[(symbol_short!("gov"), symbol_short!("appr_set"))]);
    // (admin, approver)
    let (admin, appr): (Address, Address) = decode(&ctx.env, &data[0]);
    assert_eq!(admin, ctx.admin);
    assert_eq!(appr, approver);
}

#[test]
fn golden_gov_propose_default_and_custom_ttl() {
    let ctx = setup();
    let approver = Address::generate(&ctx.env);
    ctx.client.set_gov_approver(&approver);
    let now = ctx.env.ledger().sequence();

    // Default TTL comes from `get_gov_proposal_ttl`.
    let default_ttl = ctx.client.get_gov_proposal_ttl();
    let id =
        ctx.client
            .propose_gov_action(&ctx.admin, &GovAction::SetProtocolFeeBps(Some(100)), &None);
    let data = assert_topic_sequence(&ctx, &[(symbol_short!("gov"), symbol_short!("proposed"))]);
    // (proposal_id, proposer, action_code, expires_at_ledger)
    let (pid, proposer, action_code, expires): (u64, Address, u32, u32) =
        decode(&ctx.env, &data[0]);
    assert_eq!(pid, id);
    assert_eq!(proposer, ctx.admin);
    assert_eq!(action_code, 2); // SetProtocolFeeBps
    assert_eq!(expires, now + default_ttl);

    // Custom TTL overrides the default and is reflected in the same slot.
    ctx.client.set_gov_proposal_ttl(&500u32);
    let id2 = ctx
        .client
        .propose_gov_action(&approver, &GovAction::PauseProtocol, &Some(42u32));
    let data = assert_topic_sequence(&ctx, &[(symbol_short!("gov"), symbol_short!("proposed"))]);
    let (pid, proposer, action_code, expires): (u64, Address, u32, u32) =
        decode(&ctx.env, &data[0]);
    assert_eq!(
        (pid, proposer, action_code, expires),
        (id2, approver, 0u32, now + 42)
    );
}

#[test]
fn golden_gov_approve() {
    let ctx = setup();
    let approver = Address::generate(&ctx.env);
    ctx.client.set_gov_approver(&approver);
    let id = ctx
        .client
        .propose_gov_action(&ctx.admin, &GovAction::PauseProtocol, &None);

    ctx.client.approve_gov_proposal(&approver, &id);
    let data = assert_topic_sequence(&ctx, &[(symbol_short!("gov"), symbol_short!("approved"))]);
    // (proposal_id, approver)
    let (pid, who): (u64, Address) = decode(&ctx.env, &data[0]);
    assert_eq!((pid, who), (id, approver));
}

/// Executing `PauseProtocol` emits the mode transition *before* the
/// `executed` receipt; indexers rely on that ordering.
#[test]
fn golden_gov_execute_pause_then_unpause() {
    let ctx = setup();
    let approver = Address::generate(&ctx.env);
    ctx.client.set_gov_approver(&approver);

    let id = ctx
        .client
        .propose_gov_action(&ctx.admin, &GovAction::PauseProtocol, &None);
    ctx.client.approve_gov_proposal(&approver, &id);
    ctx.client.execute_gov_proposal(&ctx.admin, &id);

    let data = assert_topic_sequence(
        &ctx,
        &[
            (symbol_short!("mode"), sym(&ctx.env, "transition")),
            (symbol_short!("gov"), symbol_short!("executed")),
        ],
    );
    // mode transition: (old_mode, new_mode)
    let (old_mode, new_mode): (u32, u32) = decode(&ctx.env, &data[0]);
    assert_eq!((old_mode, new_mode), (0, 2));
    // executed: (proposal_id, executor, action_code)
    let (pid, executor, action_code): (u64, Address, u32) = decode(&ctx.env, &data[1]);
    assert_eq!((pid, executor, action_code), (id, ctx.admin.clone(), 0u32));

    let id2 = ctx
        .client
        .propose_gov_action(&approver, &GovAction::UnpauseProtocol, &None);
    ctx.client.approve_gov_proposal(&ctx.admin, &id2);
    ctx.client.execute_gov_proposal(&approver, &id2);
    let data = assert_topic_sequence(
        &ctx,
        &[
            (symbol_short!("mode"), sym(&ctx.env, "transition")),
            (symbol_short!("gov"), symbol_short!("executed")),
        ],
    );
    let (old_mode, new_mode): (u32, u32) = decode(&ctx.env, &data[0]);
    assert_eq!((old_mode, new_mode), (2, 0));
    let (pid, executor, action_code): (u64, Address, u32) = decode(&ctx.env, &data[1]);
    assert_eq!((pid, executor, action_code), (id2, approver, 1u32));
}

/// Actions without a side-effect event emit only the `executed` receipt.
#[test]
fn golden_gov_execute_set_oracle_single_event() {
    let ctx = setup();
    let approver = Address::generate(&ctx.env);
    ctx.client.set_gov_approver(&approver);
    let new_oracle = Address::generate(&ctx.env);

    let id =
        ctx.client
            .propose_gov_action(&ctx.admin, &GovAction::SetOracle(new_oracle.clone()), &None);
    ctx.client.approve_gov_proposal(&approver, &id);
    ctx.client.execute_gov_proposal(&approver, &id);

    let data = assert_topic_sequence(&ctx, &[(symbol_short!("gov"), symbol_short!("executed"))]);
    let (pid, executor, action_code): (u64, Address, u32) = decode(&ctx.env, &data[0]);
    assert_eq!((pid, executor, action_code), (id, approver, 6u32));
    assert_eq!(ctx.client.get_oracle(), Some(new_oracle));
}

#[test]
fn golden_gov_cancel_pending_and_approved() {
    let ctx = setup();
    let approver = Address::generate(&ctx.env);
    ctx.client.set_gov_approver(&approver);

    // Cancel while Pending.
    let id = ctx
        .client
        .propose_gov_action(&ctx.admin, &GovAction::PauseProtocol, &None);
    ctx.client.cancel_gov_proposal(&approver, &id);
    let data = assert_topic_sequence(&ctx, &[(symbol_short!("gov"), symbol_short!("cancel"))]);
    // (proposal_id, canceller)
    let (pid, who): (u64, Address) = decode(&ctx.env, &data[0]);
    assert_eq!((pid, who), (id, approver.clone()));

    // Cancel while Approved: same shape.
    let id2 = ctx
        .client
        .propose_gov_action(&ctx.admin, &GovAction::PauseProtocol, &None);
    ctx.client.approve_gov_proposal(&approver, &id2);
    ctx.client.cancel_gov_proposal(&ctx.admin, &id2);
    let data = assert_topic_sequence(&ctx, &[(symbol_short!("gov"), symbol_short!("cancel"))]);
    let (pid, who): (u64, Address) = decode(&ctx.env, &data[0]);
    assert_eq!((pid, who), (id2, ctx.admin.clone()));
}

/// Full lifecycle in one place: the ids and action codes threaded through
/// propose → approve → execute must line up field-for-field.
#[test]
fn golden_gov_full_lifecycle_ids_line_up() {
    let ctx = setup();
    let approver = Address::generate(&ctx.env);
    ctx.client.set_gov_approver(&approver);

    let id =
        ctx.client
            .propose_gov_action(&ctx.admin, &GovAction::SetProtocolFeeBps(Some(250)), &None);
    let proposed =
        assert_topic_sequence(&ctx, &[(symbol_short!("gov"), symbol_short!("proposed"))]);
    let (p_id, _, p_code, _): (u64, Address, u32, u32) = decode(&ctx.env, &proposed[0]);

    ctx.client.approve_gov_proposal(&approver, &id);
    let approved =
        assert_topic_sequence(&ctx, &[(symbol_short!("gov"), symbol_short!("approved"))]);
    let (a_id, _): (u64, Address) = decode(&ctx.env, &approved[0]);

    ctx.client.execute_gov_proposal(&ctx.admin, &id);
    let events = last_call_events(&ctx);
    let (_, _, executed) = events
        .iter()
        .rev()
        .find(|(a, b, _)| *a == symbol_short!("gov") && *b == symbol_short!("executed"))
        .expect("executed receipt must be emitted")
        .clone();
    // The receipt is always the final event of an execute call.
    assert_eq!(events.last().unwrap().1, symbol_short!("executed"));
    let (e_id, _, e_code): (u64, Address, u32) = decode(&ctx.env, &executed);

    assert_eq!(p_id, id);
    assert_eq!(a_id, id);
    assert_eq!(e_id, id);
    assert_eq!(p_code, 2); // SetProtocolFeeBps
    assert_eq!(e_code, p_code);
}

// ─── TTL ─────────────────────────────────────────────────────────────────────

#[test]
fn golden_storage_touch_counts() {
    let ctx = setup();
    // Admin/Oracle/SchemaVersion exist after init; MaxStake is allowlisted
    // but unset, so it is skipped.
    let keys = vec![
        &ctx.env,
        DataKeyCore::Admin,
        DataKeyCore::Oracle,
        DataKeyCore::SchemaVersion,
        DataKeyCore::MaxStake,
    ];
    let touched = ctx.client.batch_touch_ttl(&keys);
    assert_eq!(touched, 3);

    let data = assert_topic_sequence(&ctx, &[(symbol_short!("storage"), symbol_short!("touch"))]);
    // (touched, skipped)
    let (t, s): (u32, u32) = decode(&ctx.env, &data[0]);
    assert_eq!((t, s), (3, 1));
}

#[test]
fn golden_storage_touch_empty_batch() {
    let ctx = setup();
    let keys: soroban_sdk::Vec<DataKeyCore> = vec![&ctx.env];
    assert_eq!(ctx.client.batch_touch_ttl(&keys), 0);
    let data = assert_topic_sequence(&ctx, &[(symbol_short!("storage"), symbol_short!("touch"))]);
    let (t, s): (u32, u32) = decode(&ctx.env, &data[0]);
    assert_eq!((t, s), (0, 0));
}

// ─── Season ──────────────────────────────────────────────────────────────────

fn play_one_resolved_round(ctx: &Ctx, winner: &Address, loser: &Address) {
    ctx.client.mint_initial(winner);
    ctx.client.mint_initial(loser);
    ctx.client.create_round(&1_0000000, &None);
    ctx.client.place_bet(winner, &10_0000000, &BetSide::Up);
    ctx.client.place_bet(loser, &10_0000000, &BetSide::Down);
    let round = ctx.client.get_active_round().unwrap();
    ctx.env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger + 1;
        li.timestamp = round.start_timestamp + 60;
    });
    ctx.client.update_oracle_heartbeat(&0u32);
    ctx.client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: ctx.env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 7,
        network_id: ctx.env.ledger().network_id(),
        contract_addr: ctx.contract_id.clone(),
        confidence: None,
        attestation: None,
    });
}

#[test]
fn golden_season_reset_empty_season() {
    let ctx = setup();
    let season = ctx.client.get_current_season_id();
    let new_season = ctx.client.reset_leaderboard_season();

    let data = assert_topic_sequence(&ctx, &[(symbol_short!("season"), symbol_short!("reset"))]);
    // (season_id, new_season_id, ended_at_ledger, participant_count)
    let (sid, nsid, ended, count): (u32, u32, u32, u32) = decode(&ctx.env, &data[0]);
    assert_eq!(
        (sid, nsid, ended, count),
        (season, new_season, ctx.env.ledger().sequence(), 0u32)
    );
    assert_eq!(new_season, season + 1);
}

#[test]
fn golden_season_reset_with_participants_matches_archive() {
    let ctx = setup();
    let alice = Address::generate(&ctx.env);
    let bob = Address::generate(&ctx.env);
    play_one_resolved_round(&ctx, &alice, &bob);

    let season = ctx.client.get_current_season_id();
    let new_season = ctx.client.reset_leaderboard_season();
    let data = assert_topic_sequence(&ctx, &[(symbol_short!("season"), symbol_short!("reset"))]);
    let (sid, nsid, ended, count): (u32, u32, u32, u32) = decode(&ctx.env, &data[0]);

    let archive = ctx
        .client
        .get_season_archive(&season)
        .expect("reset must archive the ended season");
    assert_eq!(sid, season);
    assert_eq!(nsid, new_season);
    assert_eq!(ended, archive.ended_at_ledger);
    assert_eq!(count, archive.participant_count);
    assert!(
        count >= 1,
        "the winner must be counted as a season participant"
    );

    // Consecutive resets keep the same shape and advance monotonically.
    ctx.env.ledger().with_mut(|li| li.sequence_number += 10);
    let third = ctx.client.reset_leaderboard_season();
    let data = assert_topic_sequence(&ctx, &[(symbol_short!("season"), symbol_short!("reset"))]);
    let (sid, nsid, ended, count): (u32, u32, u32, u32) = decode(&ctx.env, &data[0]);
    assert_eq!(
        (sid, nsid, ended, count),
        (new_season, third, ctx.env.ledger().sequence(), 0u32)
    );
}
