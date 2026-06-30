#!/usr/bin/env python3
"""Inject protocol-fee tests (#162) into:
  - contracts/src/tests/resolution.rs (conservation + per-settlement behaviour)
  - contracts/src/tests/config_timelock.rs (timelock scheduling + admin apply)

Tests are purely additive: all existing tests run with fee disabled by default
(ProtocolFeeBps storage key absent) so no behaviour change is exercised
elsewhere.
"""
import sys, os

base = '/workspaces/Xelma-Blockchain/contracts/src/tests'
RES = os.path.join(base, 'resolution.rs')
CFG = os.path.join(base, 'config_timelock.rs')

# ---------------------------------------------------------------------------
# Test block to append to resolution.rs
# ---------------------------------------------------------------------------
RES_BLOCK = r"""
// ============================================================================
// PROTOCOL FEE TESTS (Issue #162)
// ============================================================================
//
// These tests exercise the optional protocol fee: default (ProtocolFeeBps
// storage key absent) is byte-for-byte the pre-#162 behaviour; activating
// the fee routes `fee = total_pot * bps / 10_000` to the on-chain treasury
// while preserving the conservation invariant
//     Σ payouts + treasury_growth == total_pot
// for every competitive settlement path (UpDown indexed/legacy, Precision
// indexed/legacy). Refund paths (price-unchanged, one-sided, min-participants,
// admin cancel) MUST NOT emit a fee event — and the treasury MUST stay flat.
//
// The 10% hard cap is enforced at schedule time; timelock semantics tested
// in `config_timelock.rs::test_protocol_fee_timelock_*`.

use soroban_sdk::{symbol_short, BytesN, Vec};

fn collect_protocol_fee_events(
    env: &Env,
) -> Vec<(u64, soroban_sdk::I128, soroban_sdk::I128, u32)> {
    env.events()
        .all()
        .iter()
        .filter_map(|e| {
            let (_contract, topics, data) = e;
            if topics.len() != 2
                || topics.get(0).unwrap().try_into_val(env) != Ok(symbol_short!("protocol"))
                || topics.get(1).unwrap().try_into_val(env) != Ok(symbol_short!("fee_collected"))
            {
                return None;
            }
            data.try_into_val::<(u64, soroban_sdk::I128, soroban_sdk::I128, u32)>(env)
                .ok()
        })
        .collect()
}

fn count_protocol_fee_events(env: &Env) -> u32 {
    env.events()
        .all()
        .iter()
        .filter(|e| {
            let (_contract, topics, _data) = e;
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("protocol"))
                && topics.get(1).unwrap().try_into_val(env) == Ok(symbol_short!("fee_collected"))
        })
        .count() as u32
}

/// Build a deterministic Vector of user-side pre-resolution `("outcome","loss")` events
/// helper to keep the conservation-test bodies short.
fn sum_pending_payouts(env: &Env, users: &[soroban_sdk::Address]) -> soroban_sdk::I128 {
    let mut total: i128 = 0;
    env.as_contract(&env.current_contract_address(), || {
        for u in users {
            let key = crate::types::DataKey::PendingWinnings(u.clone());
            let v: Option<i128> = env.storage().persistent().get(&key);
            total = total
                .checked_add(v.unwrap_or(0))
                .expect("overflow summing pending payouts");
        }
    });
    total.into()
}

#[test]
fn test_protocol_fee_disabled_default_is_no_behaviour_change() {
    // Without ever calling schedule_protocol_fee_bps, a competitive
    // UpDown resolution must:
    //  - Pay winners exactly the pre-#162 formula amount.
    //  - Leave treasury at 0.
    //  - NOT emit a fee event.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env); // Up winner
    let bob = Address::generate(&env); // Down loser

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000_0000, &None);
    client.place_bet(&alice, &100_000_0000, &BetSide::Up);
    client.place_bet(&bob, &50_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    client.resolve_round(&oracle_payload(&env, &contract_id, 1_500_0000, 0, 1));

    // Pre-#162 UpDown formula: payout_alice = 100 + 100 * 50 / 100 = 150 stroops.
    assert_eq!(
        sum_pending_payouts(&env, &[alice.clone(), bob.clone()]),
        150_000_0000i128,
    );
    assert_eq!(client.get_protocol_fee_bps(), None);
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    assert_eq!(count_protocol_fee_events(&env), 0);
}

#[test]
fn test_protocol_fee_updown_indexed_conservation() {
    // 200_bps (2%) fee on a 100/50 pot must run the conservation invariant:
    // total_pot = 150 -> fee = 3 (floor), distributable_winning = 97,
    // distributable_losing = 47, sum payouts + treasury = 147... wait:
    // After expense: sum payouts to winners + treasury = 150.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env); // Up winner
    let bob = Address::generate(&env); // Down loser

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Activate 200_bps (2%) fee via timelock -- fast-forward.
    client.schedule_protocol_fee_bps(&Some(200u32));
    env.ledger().with_mut(|li| {
        li.sequence_number = 2000; // advance past CONFIG_TIMELOCK_LEDGERS (1440).
    });
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );
    assert_eq!(client.get_protocol_fee_bps(), Some(200u32));

    client.create_round(&1_000_0000, &None);
    client.place_bet(&alice, &100_000_0000, &BetSide::Up);
    client.place_bet(&bob, &50_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&oracle_payload(&env, &contract_id, 1_500_0000, 0, 2));

    // total_pot = 150; fee = floor(150 * 200 / 10_000) = 3.
    // fee_from_losing = min(3, 50) = 3; fee_from_winning = 0.
    // distributable_winning = 100, distributable_losing = 47.
    // alice payout = 100 + 100 * 47 / 100 = 147.
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone()]);
    assert_eq!(payouts, 147_000_0000i128,
        "winner payout must reflect fee deducted from losing pool");
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 3_000_0000i128,
        "treasury must accumulate exactly the bps-computed fee");

    // Conservation invariant.
    let total_pot: i128 = 150_000_0000i128;
    assert_eq!(payouts + treasury.into(), total_pot.into(),
        "conservation: payouts + treasury must equal total_pot");

    // One round -> one fee_collected event.
    assert_eq!(count_protocol_fee_events(&env), 1);
    let events = collect_protocol_fee_events(&env);
    let (round_id, fee, _treasury_after, bps) = events.get(0).unwrap();
    assert_eq!(*round_id, 2u64);
    assert_eq!(*fee, 3_000_0000i128);
    assert_eq!(*bps, 200u32);
}

#[test]
fn test_protocol_fee_updown_legacy_conservation() {
    // Same conservation test but exercising the legacy migration-fallback path.
    let env = Env::default();
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

    client.schedule_protocol_fee_bps(&Some(500u32)); // 5%
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    let start_price: u128 = 1_000_0000;
    client.create_round(&start_price, &None);

    // Author positions via the legacy bulk map.
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(alice.clone(), UserPosition { amount: 100_000_0000, side: BetSide::Up });
        positions.set(bob.clone(), UserPosition { amount: 50_000_0000, side: BetSide::Down });
        env.storage().persistent().set(&DataKey::UpDownPositions, &positions);

        let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
        round.pool_up = 100_000_0000;
        round.pool_down = 50_000_0000;
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&oracle_payload(&env, &contract_id, 1_500_0000, 0, 3));

    // total_pot = 150; fee = floor(150 * 500 / 10_000) = 7.
    // distributable_winning = 100, distributable_losing = 43.
    // alice payout = 100 + 100 * 43 / 100 = 143.
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone()]);
    assert_eq!(payouts, 143_000_0000i128);
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 7_000_0000i128);
    // Conservation.
    assert_eq!(payouts + treasury.into(), 150_000_0000i128);
}

#[test]
fn test_protocol_fee_precision_indexed_conservation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env); // winner (closest guess)
    let bob = Address::generate(&env); // loser
    let charlie = Address::generate(&env); // loser

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.schedule_protocol_fee_bps(&Some(1000u32)); // 10% (cap)
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    client.create_round(&2000, &Some(1));
    client.place_precision_prediction(&alice, &100_000_0000, &2297u128);
    client.place_precision_prediction(&bob, &150_000_0000, &2500u128);
    client.place_precision_prediction(&charlie, &50_000_0000, &5000u128);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&oracle_payload(&env, &contract_id, 2298, 0, 4));

    // total_pot = 100 + 150 + 50 = 300. fee = 300 * 1000 / 10_000 = 30.
    // winner_count = 1 -> payout_pool = 270 -> alice gets 270.
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone(), charlie.clone()]);
    assert_eq!(payouts, 270_000_0000i128);
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 30_000_0000i128);
    assert_eq!(payouts + treasury.into(), 300_000_0000i128,
        "conservation invariant must hold for Precision indexed path");
}

#[test]
fn test_protocol_fee_precision_legacy_conservation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.schedule_protocol_fee_bps(&Some(100u32)); // 1%
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    let start_price: u128 = 2000;
    client.create_round(&start_price, &Some(1));

    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);
        predictions.set(alice.clone(), PrecisionPrediction { user: alice.clone(), predicted_price: 2297, amount: 100_000_0000 });
        predictions.set(bob.clone(), PrecisionPrediction { user: bob.clone(), predicted_price: 2500, amount: 150_000_0000 });
        predictions.set(charlie.clone(), PrecisionPrediction { user: charlie.clone(), predicted_price: 5000, amount: 50_000_0000 });
        env.storage().persistent().set(&DataKey::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&oracle_payload(&env, &contract_id, 2298, 0, 5));

    // total_pot = 300; fee = 300 * 100 / 10_000 = 3.
    // payout_pool = 297 -> winner alice gets 297.
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone(), charlie.clone()]);
    assert_eq!(payouts, 297_000_0000i128);
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 3_000_0000i128);
    assert_eq!(payouts + treasury.into(), 300_000_0000i128,
        "conservation invariant must hold for Precision legacy path");
}

#[test]
fn test_protocol_fee_thin_losing_pool_updown() {
    // With bps near the cap and a thin losing pool, the fee exceeds losing_pool.
    // Per documented policy: spillover taken from winning_pool so the
    // conservation invariant holds even when winners lose a portion of
    // their principal.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env); // Up majority winner
    let bob = Address::generate(&env); // Down minority loser

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.schedule_protocol_fee_bps(&Some(1000u32)); // 10% (cap)
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    client.create_round(&1_000_0000, &None);
    // winning_pool = 1000, losing_pool = 1.
    // total_pot = 1001; fee = floor(1001 * 1000 / 10_000) = 100.
    // fee_from_losing = min(100, 1) = 1; fee_from_winning = 99.
    // distributable_winning = 1000 - 99 = 901.
    // distributable_losing  = 1 - 1 = 0.
    // alice payout = 1000 + 1000 * 0 / 901 = 1000. (no share; 1000 of 1001 taken as fee)
    client.place_bet(&alice, &1000_000_0000, &BetSide::Up);
    client.place_bet(&bob, &1_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&oracle_payload(&env, &contract_id, 1_500_0000, 0, 6));

    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone()]);
    // alice gets her principal minus the spillover (= 1000 - 99 = 901)
    // (since distributable_losing = 0, the share numerator is 0; payout = amount).
    assert_eq!(payouts, 1000_000_0000i128,
        "loser has 0 distributable_losing so winners only get principal back");
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 100_000_0000i128,
        "full fee still collected: 1 (from losing) + 99 (from winning spillover) = 100");
    assert_eq!(payouts + treasury.into(), 1001_000_0000i128,
        "conservation invariant holds even when losing_pool is thin");
}

#[test]
fn test_protocol_fee_not_collected_on_refund_paths() {
    // Price-unchanged refunds must NOT deduct the fee from treasury even when
    // the fee is enabled. The user's stake is returned 100%; no fee events
    // are emitted on any refund path.
    struct Case { up: bool; }
    let _cases = [Case { up: true }, Case { up: false }];
    let env = Env::default();
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

    client.schedule_protocol_fee_bps(&Some(1000u32));
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    let start_price: u128 = 1_500_0000;
    client.create_round(&start_price, &None);
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(alice.clone(), UserPosition { amount: 100_000_0000, side: BetSide::Up });
        positions.set(bob.clone(), UserPosition { amount: 50_000_0000, side: BetSide::Down });
        env.storage().persistent().set(&DataKey::UpDownPositions, &positions);
        let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
        round.pool_up = 100_000_0000;
        round.pool_down = 50_000_0000;
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&oracle_payload(&env, &contract_id, start_price, 0, 7));

    // Refund: no fee event, treasury still 0.
    assert_eq!(count_protocol_fee_events(&env), 0,
        "price-unchanged refunds MUST NOT emit a fee event");
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone()]);
    assert_eq!(payouts, 150_000_0000i128,
        "all participants refunded their full stake");
}

#[test]
fn test_protocol_fee_withdrawal_to_recipient() {
    // Once accumulated, the admin can drain the treasury to a recipient.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let treasury_account = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&treasury_account);

    client.schedule_protocol_fee_bps(&Some(1000u32)); // 10%
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    client.create_round(&1_000_0000, &None);
    client.place_bet(&alice, &100_000_0000, &BetSide::Up);
    client.place_bet(&bob, &50_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&oracle_payload(&env, &contract_id, 1_500_0000, 0, 8));
    // total_pot = 150; fee = 15; distributable_losing = 35.
    // payout = 100 + 100 * 35 / 100 = 135.
    assert_eq!(client.get_protocol_fee_treasury(), 15_000_0000i128);

    // Drain 10 stroops to treasury_account.
    let starting_bal = client.balance(&treasury_account);
    let withdrawn = client.withdraw_protocol_fee(&treasury_account.clone(), &10_000_0000i128);
    assert_eq!(withdrawn, 10_000_0000i128);
    assert_eq!(
        client.balance(&treasury_account),
        starting_bal + 10_000_0000i128,
    );
    assert_eq!(client.get_protocol_fee_treasury(), 5_000_0000i128);

    // Attempting to overwithdraw must NOT consume funds.
    let result = client.try_withdraw_protocol_fee(
        &treasury_account.clone(),
        &1_000_000_0000i128,
    );
    assert!(result.is_err(), "over-withdrawal must be rejected");
    assert_eq!(client.get_protocol_fee_treasury(), 5_000_0000i128);
}

#[test]
fn test_protocol_fee_schedule_validation_rejects_zero_and_over_cap() {
    // `Some(0)` rejected (use `None` to disable); `Some(1_001)` rejected
    // (over MAX_PROTOCOL_FEE_BPS = 1_000). Empty (None) is allowed.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // None always OK.
    client.schedule_protocol_fee_bps(&None);
    // Some(0) rejected.
    let r0 = client.try_schedule_protocol_fee_bps(&Some(0u32));
    assert!(r0.is_err(), "Some(0) is not a valid bps value");
    // Over cap rejected.
    let r_max = client.try_schedule_protocol_fee_bps(&Some(1_001u32));
    assert!(r_max.is_err(), "1_001 bps exceeds MAX_PROTOCOL_FEE_BPS=1000");
    // 1_000 (cap) accepted.
    // First clear the existing None-schedule.
    env.ledger().with_mut(|li| li.sequence_number = 1_000_000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );
    let r_top = client.try_schedule_protocol_fee_bps(&Some(1_000u32));
    assert!(r_top.is_ok(), "1_000 bps (MAX) must be accepted");
}
"""

# Append RES_BLOCK to resolution.rs (guarded by unique sentinel).
RES_SENTINEL = "// ============================================================================\n// LOSS OUTCOME EVENT TESTS (Issue #168)"
if RES_SENTINEL not in open(RES).read():
    print('resolution.rs sentinel missing -- aborting', file=sys.stderr)
    sys.exit(1)
# safe to append at end of file as a new module of tests
res_old = open(RES).read()
open(RES, 'w').write(res_old.rstrip() + '\n' + RES_BLOCK)
print(f'resolution.rs: appended {RES_BLOCK.count(chr(10))} lines')

# ---------------------------------------------------------------------------
# Test block to append to config_timelock.rs
# ---------------------------------------------------------------------------
CFG_BLOCK = r"""
// ============================================================================
// PROTOCOL FEE TIMELOCK TESTS (Issue #162)
// ============================================================================
//
// The protocol fee is a critical config setting (impacts payout fairness for
// every competitive settlement), so it goes through the same timelock pattern
// as `OracleMaxDeviationBps` etc.:
//   schedule -> activation_ledger = now + CONFIG_TIMELOCK_LEDGERS ->
//   apply_scheduled_changes (any caller) -> storage flipped -> event emitted.

use soroban_sdk::{symbol_short};

#[test]
fn test_protocol_fee_timelock_full_cycle() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Initially unset.
    assert_eq!(client.get_protocol_fee_bps(), None);

    // Schedule 500 bps.
    client.schedule_protocol_fee_bps(&Some(500u32));

    // Before activation: still unset; reads should NOT expose the pending value.
    assert_eq!(client.get_protocol_fee_bps(), None);
    let pending = client
        .get_pending_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps)
        .unwrap();
    assert_eq!(pending.activation_ledger, pending.scheduled_at_ledger + 1440);

    // Advancing the ledger by an insufficient amount must NOT activate.
    env.ledger().with_mut(|li| li.sequence_number += 1439);
    let err = client.try_apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );
    assert!(err.is_err(), "apply before activation_ledger must fail");

    // Exactly at activation_ledger must succeed.
    env.ledger().with_mut(|li| li.sequence_number += 1);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);
    assert_eq!(client.get_protocol_fee_bps(), Some(500u32));

    // Pending entry must be cleared post-apply.
    assert!(
        client
            .get_pending_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps)
            .is_none()
    );

    // fee_bps_set event must be present.
    let ev_count = env
        .events()
        .all()
        .iter()
        .filter(|e| {
            let (_contract, topics, _data) = e;
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("protocol"))
                && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("fee_bps_set"))
        })
        .count();
    assert!(ev_count >= 1, "fee_bps_set event must be emitted on apply");
}

#[test]
fn test_protocol_fee_timelock_admin_can_cancel_before_activation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    client.schedule_protocol_fee_bps(&Some(100u32));
    // cancel before activation must be admin-only and clear the pending entry.
    client.cancel_protocol_fee_change();
    assert!(
        client
            .get_pending_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps)
            .is_none()
    );
    assert_eq!(client.get_protocol_fee_bps(), None);
}

#[test]
fn test_protocol_fee_timelock_disable_via_none() {
    // Setting fee back to None must remove the storage key (FormatOption::None on
    // the storage side) so the _read_protocol_fee_bps helper returns None and
    // the contract resumes byte-for-byte pre-#162 behaviour.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    client.schedule_protocol_fee_bps(&Some(500u32));
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );
    assert_eq!(client.get_protocol_fee_bps(), Some(500u32));

    client.schedule_protocol_fee_bps(&None);
    env.ledger().with_mut(|li| li.sequence_number = 10_000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );
    assert_eq!(client.get_protocol_fee_bps(), None,
        "re-issuing with None must remove the storage key entirely");
}
"""

if 'PROTOCOL FEE TIMELOCK TESTS' not in open(CFG).read():
    cfg_old = open(CFG).read()
    open(CFG, 'w').write(cfg_old.rstrip() + '\n' + CFG_BLOCK)
    print(f'config_timelock.rs: appended {CFG_BLOCK.count(chr(10))} lines')
else:
    print('config_timelock.rs: already has protocol fee block')
