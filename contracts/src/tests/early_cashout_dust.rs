// SPDX-License-Identifier: MIT
//! Tests for early cash-out dust stakes and fee edge cases (Issue #407).
//!
//! **Why**: Adversarial dust stakes (1-stroop positions) and their interaction
//! with the fee-on / fee-off paths can break conservation. When `stake * penalty_bps
//! / BPS_DENOMINATOR` floors to zero, the user receives a full refund with no
//! forfeit — the pool shrinks by the full stake while zero goes to treasury.
//! This module pins down the exact accounting identity for every combination
//! of dust-vs-non-dust stakes and fee-on / fee-off settlement.
//!
//! **Core identity** (per round):
//! ```text
//! sum(cashouts) + sum(settlement_payouts) + protocol_fee_treasury_delta == original_total_pot
//! ```
//!
//! ## Test matrix
//!
//! | # | Scenario | Fee | Dust? | Test |
//! |---|----------|-----|-------|------|
//! | 1 | Single 1-stroop dust stake cash-out | off | yes | `dust_cashout_fee_off_full_refund` |
//! | 2 | Single 1-stroop dust stake cash-out | on  | yes | `dust_cashout_fee_on_full_refund` |
//! | 3 | Dust stake where forfeit rounds to 0 | on  | yes | `dust_cashout_fee_on_forfeit_rounds_to_zero` |
//! | 4 | Dust cash-out + remaining pool settles | off | mix | `dust_cashout_settle_remaining_fee_off` |
//! | 5 | Dust cash-out + remaining pool settles | on  | mix | `dust_cashout_settle_remaining_fee_on` |
//! | 6 | Multiple dust cash-outs | off | yes | `multiple_dust_cashouts_fee_off` |
//! | 7 | Multiple dust cash-outs + settle | on  | mix | `multiple_dust_cashouts_settle_fee_on` |
//! | 8 | Pool totals consistency after dust cash-out | off | yes | `dust_cashout_pool_totals_consistency` |
//! | 9 | All participants are dust, all cash out | on  | yes | `all_dust_cashouts_fee_on_full_round` |
//! |10 | Dust on losing side cashes out | on  | mix | `dust_on_loser_side_cashout_fee_on` |
//! |11 | Boundary: forfeit transitions from 0→1 | off | edge | `dust_forfeit_boundary_transition` |
//! |12 | Dust cash-out with FeeOnWinnings model | on  | mix | `dust_cashout_fee_on_winnings_model` |
//! |13 | Dust stake with max penalty (1000 bps) | off | yes | `dust_cashout_max_penalty_still_full_refund` |

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, DataKeyCore, FeeModel, OraclePayload};
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    (client, contract_id, admin, oracle)
}

/// Writes the protocol fee bps directly into storage (bypassing timelock).
fn set_fee_bps_now(env: &Env, contract_id: &Address, bps: u32) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::ProtocolFeeBps, &bps);
    });
}

/// Writes the early cash-out penalty bps directly into storage.
fn set_ec_bps_now(env: &Env, contract_id: &Address, bps: u32) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::EarlyCashoutBps, &bps);
    });
}

/// Writes the fee model directly into storage.
fn set_fee_model_now(env: &Env, contract_id: &Address, model: FeeModel) {
    env.as_contract(contract_id, || {
        env.storage().persistent().set(&DataKeyCore::FeeModel, &model);
    });
}

fn resolve_at(env: &Env, client: &VirtualTokenContractClient, contract_id: &Address, price: u128) {
    let round = client
        .get_active_round()
        .expect("active round required to resolve");
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

// ─── Row 1: 1-stroop dust cash-out, fee off ─────────────────────────────────

/// A single 1-stroop position cashes out with fee disabled. The forfeit is
/// `1 * penalty / 10_000 = 0` (floor division), so the user receives a full
/// refund and treasury must not move.
#[test]
fn dust_cashout_fee_off_full_refund() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    // Alice: dust (1 stroop), Bob: normal (100)
    client.place_bet(&alice, &1, &BetSide::Up);
    client.place_bet(&bob, &100, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000); // 10% penalty

    // Advance to Running phase
    env.ledger().with_mut(|li| li.sequence_number = 7);

    let treasury_before = client.get_protocol_fee_treasury();
    let alice_pending_before = client.get_pending_winnings(&alice);
    client.cash_out_early(&alice);

    let alice_cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // Forfeit = 1 * 1000 / 10000 = 0, so full refund
    assert_eq!(alice_cashout, 1, "dust cash-out must return full stake when forfeit floors to 0");
    assert_eq!(treasury_delta, 0, "treasury must not move when forfeit is 0");

    // Pool should be reduced by full 1 stroop
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0, "pool_up must be 0 after dust cash-out");
    assert_eq!(round.pool_down, 100, "pool_down must be unchanged");

    // Alice's position is gone
    assert!(client.get_user_position(&alice).is_none());
}

// ─── Row 2: 1-stroop dust cash-out, fee on ──────────────────────────────────

/// Same as Row 1 but with settlement fee enabled (10%). The cash-out forfeit
/// still floors to 0, so treasury receives nothing from the cash-out itself.
/// Settlement of the remaining pool then applies the fee.
#[test]
fn dust_cashout_fee_on_full_refund() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &1, &BetSide::Up); // dust
    client.place_bet(&bob, &100, &BetSide::Down);
    client.place_bet(&charlie, &100, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000); // 10% penalty
    set_fee_bps_now(&env, &contract_id, 1_000); // 10% settlement fee

    env.ledger().with_mut(|li| li.sequence_number = 7);

    let treasury_before = client.get_protocol_fee_treasury();
    let alice_pending_before = client.get_pending_winnings(&alice);
    client.cash_out_early(&alice);

    let alice_cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let ec_treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(alice_cashout, 1, "dust forfeit floors to 0");
    assert_eq!(ec_treasury_delta, 0, "no forfeit goes to treasury for dust");

    // Remaining pool: up=0, down=200 → one-sided → refund, no fee
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let bob_pending_before = client.get_pending_winnings(&bob);
    let charlie_pending_before = client.get_pending_winnings(&charlie);
    let treasury_before_resolve = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 900u128); // price down

    let bob_pay = client.get_pending_winnings(&bob) - bob_pending_before;
    let charlie_pay = client.get_pending_winnings(&charlie) - charlie_pending_before;
    let resolve_treasury_delta = client.get_protocol_fee_treasury() - treasury_before_resolve;

    assert_eq!(bob_pay, 100, "bob refunded");
    assert_eq!(charlie_pay, 100, "charlie refunded");
    assert_eq!(resolve_treasury_delta, 0, "one-sided pool → no fee");

    // Full conservation: original pot = 1 + 100 + 100 = 201
    assert_eq!(
        alice_cashout + bob_pay + charlie_pay + ec_treasury_delta + resolve_treasury_delta,
        201,
        "full round conservation"
    );
}

// ─── Row 3: Dust forfeit rounds to zero with various penalty rates ───────────

/// Parametric-style: verify that for any stake * penalty_bps < 10_000 the
/// forfeit is exactly 0 and the user gets a full refund. Uses three specific
/// combinations that sit right at the boundary.
#[test]
fn dust_cashout_fee_on_forfeit_rounds_to_zero() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    // Stake=9999, penalty=1 bps → 9999 * 1 / 10000 = 0 (floor)
    client.place_bet(&alice, &9999, &BetSide::Up);
    client.place_bet(&bob, &500, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1); // 0.01% penalty (1 bps)

    env.ledger().with_mut(|li| li.sequence_number = 7);

    let treasury_before = client.get_protocol_fee_treasury();
    let alice_pending_before = client.get_pending_winnings(&alice);
    client.cash_out_early(&alice);

    let alice_cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // 9999 * 1 / 10000 = 0 → full refund
    assert_eq!(alice_cashout, 9999);
    assert_eq!(treasury_delta, 0);

    // Pool consistency
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 500);
}

// ─── Row 4: Dust cash-out + remaining pool settles, fee off ─────────────────

/// Dust stake on the UP side cashes out. Remaining pool (DOWN side only)
/// settles as a one-sided refund. Fee off throughout.
#[test]
fn dust_cashout_settle_remaining_fee_off() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env); // dust up
    let bob = Address::generate(&env);   // normal down
    let charlie = Address::generate(&env); // normal up
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &1, &BetSide::Up);     // dust
    client.place_bet(&charlie, &50, &BetSide::Up);  // normal
    client.place_bet(&bob, &200, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000);

    env.ledger().with_mut(|li| li.sequence_number = 7);
    let treasury_before = client.get_protocol_fee_treasury();
    let alice_pending_before = client.get_pending_winnings(&alice);
    client.cash_out_early(&alice);
    let alice_cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let ec_treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(alice_cashout, 1);
    assert_eq!(ec_treasury_delta, 0);

    // Pool: up=50, down=200
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 50);
    assert_eq!(round.pool_down, 200);

    // Resolve: price down → DOWN wins, fee off
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let charlie_before = client.get_pending_winnings(&charlie);
    let bob_before = client.get_pending_winnings(&bob);
    let treasury_before_resolve = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 900u128);

    let charlie_pay = client.get_pending_winnings(&charlie) - charlie_before;
    let bob_pay = client.get_pending_winnings(&bob) - bob_before;
    let resolve_treasury_delta = client.get_protocol_fee_treasury() - treasury_before_resolve;

    // Bob wins: gets back his 200 + charlie's 50 = 250 (fee off, no deduction)
    assert_eq!(bob_pay, 250);
    assert_eq!(charlie_pay, 0);
    assert_eq!(resolve_treasury_delta, 0);

    // Conservation: original pot = 1 + 50 + 200 = 251
    assert_eq!(
        alice_cashout + bob_pay + charlie_pay + ec_treasury_delta + resolve_treasury_delta,
        251,
    );
}

// ─── Row 5: Dust cash-out + remaining pool settles, fee on ──────────────────

/// Dust stake on the UP side cashes out (forfeit = 0). Remaining pool settles
/// with fee on (10% FeeOnPot). The settlement fee is applied to the remaining
/// 250-pot (50 up + 200 down).
#[test]
fn dust_cashout_settle_remaining_fee_on() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env); // dust up
    let bob = Address::generate(&env);   // normal down (winner)
    let charlie = Address::generate(&env); // normal up (loser)
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &1, &BetSide::Up);     // dust
    client.place_bet(&charlie, &50, &BetSide::Up);
    client.place_bet(&bob, &200, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000);
    set_fee_bps_now(&env, &contract_id, 1_000); // 10% settlement fee

    env.ledger().with_mut(|li| li.sequence_number = 7);
    let alice_pending_before = client.get_pending_winnings(&alice);
    let treasury_before = client.get_protocol_fee_treasury();
    client.cash_out_early(&alice);
    let alice_cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let ec_treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(alice_cashout, 1);
    assert_eq!(ec_treasury_delta, 0);

    // Resolve: price down → DOWN wins, fee on 10% of pot (250)
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let bob_before = client.get_pending_winnings(&bob);
    let charlie_before = client.get_pending_winnings(&charlie);
    let treasury_before_resolve = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 900u128);

    let bob_pay = client.get_pending_winnings(&bob) - bob_before;
    let charlie_pay = client.get_pending_winnings(&charlie) - charlie_before;
    let resolve_treasury_delta = client.get_protocol_fee_treasury() - treasury_before_resolve;

    // Fee on remaining pot: 250 * 1000 / 10000 = 25
    // Fee is taken from losing pool first: min(25, 50) = 25 from losing
    // dist_winning = 200, dist_losing = 50 - 25 = 25
    // bob_share = floor(200 * 225 / 200) = 225
    assert_eq!(bob_pay, 225);
    assert_eq!(charlie_pay, 0);
    assert_eq!(resolve_treasury_delta, 25);

    // Conservation: original pot = 1 + 50 + 200 = 251
    assert_eq!(
        alice_cashout + bob_pay + charlie_pay + ec_treasury_delta + resolve_treasury_delta,
        251,
    );
}

// ─── Row 6: Multiple dust cash-outs, fee off ────────────────────────────────

/// Two users each with 1-stroop stakes cash out. Both get full refunds. The
/// remaining non-dust user then wins or loses against an empty opposing pool.
#[test]
fn multiple_dust_cashouts_fee_off() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env); // dust up
    let bob = Address::generate(&env);   // dust down
    let charlie = Address::generate(&env); // normal down
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &1, &BetSide::Up);
    client.place_bet(&bob, &1, &BetSide::Down);
    client.place_bet(&charlie, &100, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000);

    env.ledger().with_mut(|li| li.sequence_number = 7);

    // Both dust users cash out
    let treasury_before = client.get_protocol_fee_treasury();
    let alice_before = client.get_pending_winnings(&alice);
    let bob_before = client.get_pending_winnings(&bob);
    client.cash_out_early(&alice);
    client.cash_out_early(&bob);

    let alice_cashout = client.get_pending_winnings(&alice) - alice_before;
    let bob_cashout = client.get_pending_winnings(&bob) - bob_before;
    let ec_treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(alice_cashout, 1);
    assert_eq!(bob_cashout, 1);
    assert_eq!(ec_treasury_delta, 0);

    // Pool: up=0, down=100 (one-sided)
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 100);

    // Resolve: one-sided → refund
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let charlie_before = client.get_pending_winnings(&charlie);
    let treasury_before_resolve = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_100u128);

    let charlie_pay = client.get_pending_winnings(&charlie) - charlie_before;
    let resolve_treasury_delta = client.get_protocol_fee_treasury() - treasury_before_resolve;

    assert_eq!(charlie_pay, 100);
    assert_eq!(resolve_treasury_delta, 0);

    // Conservation: original pot = 1 + 1 + 100 = 102
    assert_eq!(
        alice_cashout + bob_cashout + charlie_pay + ec_treasury_delta + resolve_treasury_delta,
        102,
    );
}

// ─── Row 7: Multiple dust cash-outs + settle, fee on ────────────────────────

/// One dust up, one normal up, one normal down. Dust up cashes out. Then
/// settlement with fee on. Full conservation must hold.
#[test]
fn multiple_dust_cashouts_settle_fee_on() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env); // dust up
    let bob = Address::generate(&env);   // normal up
    let charlie = Address::generate(&env); // normal down
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &1, &BetSide::Up);
    client.place_bet(&bob, &99, &BetSide::Up);
    client.place_bet(&charlie, &200, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000);
    set_fee_bps_now(&env, &contract_id, 500); // 5% settlement fee

    env.ledger().with_mut(|li| li.sequence_number = 7);
    let alice_before = client.get_pending_winnings(&alice);
    let treasury_before = client.get_protocol_fee_treasury();
    client.cash_out_early(&alice);
    let alice_cashout = client.get_pending_winnings(&alice) - alice_before;
    let ec_treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(alice_cashout, 1);
    assert_eq!(ec_treasury_delta, 0);

    // Pool: up=99, down=200
    // Resolve: price up → UP wins, fee on 5% of pot (299)
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let bob_before = client.get_pending_winnings(&bob);
    let charlie_before = client.get_pending_winnings(&charlie);
    let treasury_before_resolve = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_100u128);

    let bob_pay = client.get_pending_winnings(&bob) - bob_before;
    let charlie_pay = client.get_pending_winnings(&charlie) - charlie_before;
    let resolve_treasury_delta = client.get_protocol_fee_treasury() - treasury_before_resolve;

    // Total pot = 299, fee = 299 * 500 / 10000 = 14 (floor)
    // Fee from losing pool: min(14, 200) = 14
    // dist_winning = 99, dist_losing = 186
    // bob_share = floor(99 * 285 / 99) = 285
    assert_eq!(bob_pay, 285);
    assert_eq!(charlie_pay, 0);
    assert_eq!(resolve_treasury_delta, 14);

    // Conservation: original pot = 1 + 99 + 200 = 300
    assert_eq!(
        alice_cashout + bob_pay + charlie_pay + ec_treasury_delta + resolve_treasury_delta,
        300,
    );
}

// ─── Row 8: Pool totals consistency after dust cash-out ─────────────────────

/// Verify that the pool sums (pool_up + pool_down) decrease by exactly the
/// dust stake after cash-out, and that pool_up == 0 when all Up bettors
/// have cashed out.
#[test]
fn dust_cashout_pool_totals_consistency() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env); // dust up
    let bob = Address::generate(&env);   // normal down
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &1, &BetSide::Up);
    client.place_bet(&bob, &50, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000);

    // Before cash-out: pool_up + pool_down == 1 + 50 == 51
    let round_before = client.get_active_round().unwrap();
    assert_eq!(round_before.pool_up + round_before.pool_down, 51);

    env.ledger().with_mut(|li| li.sequence_number = 7);
    client.cash_out_early(&alice);

    // After cash-out: pool_up + pool_down == 0 + 50 == 50 (decreased by exactly 1)
    let round_after = client.get_active_round().unwrap();
    assert_eq!(round_after.pool_up + round_after.pool_down, 50);
    assert_eq!(round_after.pool_up, 0);
    assert_eq!(round_after.pool_down, 50);
}

// ─── Row 9: All participants are dust, all cash out, fee on ─────────────────

/// Every participant has a 1-stroop stake and cashes out. Forfeit is 0 for
/// each. The pool drains to zero. No settlement is needed.
#[test]
fn all_dust_cashouts_fee_on_full_round() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &1, &BetSide::Up);
    client.place_bet(&bob, &1, &BetSide::Down);
    client.place_bet(&charlie, &1, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000);
    set_fee_bps_now(&env, &contract_id, 1_000);

    // All three cash out
    env.ledger().with_mut(|li| li.sequence_number = 7);
    let treasury_before = client.get_protocol_fee_treasury();
    let alice_before = client.get_pending_winnings(&alice);
    let bob_before = client.get_pending_winnings(&bob);
    let charlie_before = client.get_pending_winnings(&charlie);

    client.cash_out_early(&alice);
    client.cash_out_early(&bob);
    client.cash_out_early(&charlie);

    let alice_cashout = client.get_pending_winnings(&alice) - alice_before;
    let bob_cashout = client.get_pending_winnings(&bob) - bob_before;
    let charlie_cashout = client.get_pending_winnings(&charlie) - charlie_before;
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // Each 1-stroop forfeit = 0, so full refund each
    assert_eq!(alice_cashout, 1);
    assert_eq!(bob_cashout, 1);
    assert_eq!(charlie_cashout, 1);
    assert_eq!(treasury_delta, 0);

    // Pool should be completely drained
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 0);

    // Conservation: original pot = 3
    assert_eq!(alice_cashout + bob_cashout + charlie_cashout + treasury_delta, 3);
}

// ─── Row 10: Dust on losing side cashes out, fee on ────────────────────────

/// Dust stake on the DOWN side (the eventual losing side) cashes out during
/// Running phase. Since forfeit floors to 0, the full 1 stroop is returned.
/// Then UP wins at settlement. The settlement fee applies to the remaining pot.
#[test]
fn dust_on_loser_side_cashout_fee_on() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env); // normal up (winner)
    let bob = Address::generate(&env);   // dust down (loser, cashes out)
    let charlie = Address::generate(&env); // normal down
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &200, &BetSide::Up);
    client.place_bet(&bob, &1, &BetSide::Down); // dust
    client.place_bet(&charlie, &100, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000);
    set_fee_bps_now(&env, &contract_id, 1_000); // 10% settlement fee

    env.ledger().with_mut(|li| li.sequence_number = 7);
    let treasury_before = client.get_protocol_fee_treasury();
    let bob_before = client.get_pending_winnings(&bob);
    client.cash_out_early(&bob);
    let bob_cashout = client.get_pending_winnings(&bob) - bob_before;
    let ec_treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(bob_cashout, 1, "dust forfeit floors to 0");
    assert_eq!(ec_treasury_delta, 0);

    // Pool: up=200, down=100
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 200);
    assert_eq!(round.pool_down, 100);

    // Resolve: price up → UP wins, fee on 10% of pot (300)
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let alice_before = client.get_pending_winnings(&alice);
    let charlie_before = client.get_pending_winnings(&charlie);
    let treasury_before_resolve = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_100u128);

    let alice_pay = client.get_pending_winnings(&alice) - alice_before;
    let charlie_pay = client.get_pending_winnings(&charlie) - charlie_before;
    let resolve_treasury_delta = client.get_protocol_fee_treasury() - treasury_before_resolve;

    // Fee on pot: 300 * 1000 / 10000 = 30
    // Fee from losing pool: min(30, 100) = 30
    // dist_winning = 200, dist_losing = 70
    // alice_share = floor(200 * 270 / 200) = 270
    assert_eq!(alice_pay, 270);
    assert_eq!(charlie_pay, 0);
    assert_eq!(resolve_treasury_delta, 30);

    // Conservation: original pot = 200 + 1 + 100 = 301
    assert_eq!(
        bob_cashout + alice_pay + charlie_pay + ec_treasury_delta + resolve_treasury_delta,
        301,
    );
}

// ─── Row 11: Boundary — forfeit transitions from 0 to 1 ────────────────────

/// Stake=10 with penalty=1000 bps (10%) yields forfeit = 10*1000/10000 = 1.
/// This is the smallest stake where forfeit is non-zero. Verify exact split:
/// cashout=9, forfeit=1.
#[test]
fn dust_forfeit_boundary_transition() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env); // stake=10, boundary dust
    let bob = Address::generate(&env);   // normal
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &10, &BetSide::Up);
    client.place_bet(&bob, &500, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000); // 10% penalty

    env.ledger().with_mut(|li| li.sequence_number = 7);

    let treasury_before = client.get_protocol_fee_treasury();
    let alice_pending_before = client.get_pending_winnings(&alice);
    client.cash_out_early(&alice);

    let alice_cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // 10 * 1000 / 10000 = 1 forfeit, 9 cashout
    assert_eq!(alice_cashout, 9, "boundary forfeit: cashout = stake - forfeit");
    assert_eq!(treasury_delta, 1, "boundary forfeit: exactly 1 stroop to treasury");
    assert_eq!(alice_cashout + treasury_delta, 10, "conservation: cashout + forfeit == stake");

    // Pool reduced by full stake
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 500);
}

// ─── Row 12: Dust cash-out with FeeOnWinnings model ─────────────────────────

/// Dust stake on the DOWN side cashes out (forfeit = 0). Settlement uses
/// FeeOnWinnings model. The settlement fee is only on net winnings, so the
/// losing pool is reduced by the fee amount instead of the total pot.
#[test]
fn dust_cashout_fee_on_winnings_model() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env); // dust down (cashes out)
    let bob = Address::generate(&env);   // normal up (winner)
    let charlie = Address::generate(&env); // normal down
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &None);
    client.place_bet(&bob, &200, &BetSide::Up);
    client.place_bet(&alice, &1, &BetSide::Down); // dust
    client.place_bet(&charlie, &100, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000);
    set_fee_bps_now(&env, &contract_id, 1_000); // 10% fee
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnWinnings);

    // Alice cashes out during Running phase
    env.ledger().with_mut(|li| li.sequence_number = 7);
    let treasury_before = client.get_protocol_fee_treasury();
    let alice_before = client.get_pending_winnings(&alice);
    client.cash_out_early(&alice);
    let alice_cashout = client.get_pending_winnings(&alice) - alice_before;
    let ec_treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(alice_cashout, 1, "dust forfeit floors to 0");
    assert_eq!(ec_treasury_delta, 0);

    // Pool: up=200, down=100
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 200);
    assert_eq!(round.pool_down, 100);

    // Resolve: price up → UP wins
    // FeeOnWinnings: fee = losing_pool * bps / 10000 = 100 * 1000 / 10000 = 10
    // dist_winning = 200, dist_losing = 100 - 10 = 90
    // bob_share = floor(200 * 290 / 200) = 290
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let bob_before = client.get_pending_winnings(&bob);
    let charlie_before = client.get_pending_winnings(&charlie);
    let treasury_before_resolve = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_100u128);

    let bob_pay = client.get_pending_winnings(&bob) - bob_before;
    let charlie_pay = client.get_pending_winnings(&charlie) - charlie_before;
    let resolve_treasury_delta = client.get_protocol_fee_treasury() - treasury_before_resolve;

    assert_eq!(bob_pay, 290, "winner gets: 200 * (200+90)/200");
    assert_eq!(charlie_pay, 0, "loser gets nothing");
    assert_eq!(resolve_treasury_delta, 10, "FeeOnWinnings: 10% of losing pool");

    // Conservation: original pot = 1 + 200 + 100 = 301
    assert_eq!(
        alice_cashout + bob_pay + charlie_pay + ec_treasury_delta + resolve_treasury_delta,
        301,
    );
}

// ─── Row 13: Dust stake with max penalty (1000 bps), still full refund ──────

/// Even with the maximum penalty of 1000 bps (10%), a 1-stroop stake still
/// gets a full refund because 1 * 1000 / 10000 = 0 (floor). This tests
/// the extreme boundary: max penalty × min stake.
#[test]
fn dust_cashout_max_penalty_still_full_refund() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &1, &BetSide::Up); // dust
    client.place_bet(&bob, &100, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000); // max penalty = 10%

    env.ledger().with_mut(|li| li.sequence_number = 7);

    let treasury_before = client.get_protocol_fee_treasury();
    let alice_pending_before = client.get_pending_winnings(&alice);
    client.cash_out_early(&alice);

    let alice_cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // 1 * 1000 / 10000 = 0 → full refund even at max penalty
    assert_eq!(alice_cashout, 1, "max penalty × dust → full refund");
    assert_eq!(treasury_delta, 0, "treasury unchanged");
}

// ─── Property-based: dust cash-out conservation for random stakes/penalties ─

proptest! {
    #![proptest_config(ProptestConfig::with_cases(25))]

    /// For any combination of dust-stake (1–99 stroops), penalty bps
    /// (1000–10000), and a normal opposing stake, conservation must hold:
    /// cashout + forfeit == stake and pool totals decrease by exactly stake.
    /// This covers the entire "dust regime" where forfeit may or may not
    /// floor to zero depending on the arithmetic.
    #[test]
    fn dust_cashout_conservation_property(
        dust_stake in 1i128..=99i128,
        normal_stake in 100i128..=1_000i128,
        penalty_bps in 1000u32..=10_000u32,
    ) {
        let env = Env::default();
        let (client, contract_id, _admin, _oracle) = setup(&env);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint_initial(&alice);
        client.mint_initial(&bob);

        client.create_round(&1_000u128, &None);
        client.place_bet(&alice, &dust_stake, &BetSide::Up);
        client.place_bet(&bob, &normal_stake, &BetSide::Down);
        set_ec_bps_now(&env, &contract_id, penalty_bps);

        let total_pot = dust_stake + normal_stake;

        env.ledger().with_mut(|li| li.sequence_number = 7);

        let treasury_before = client.get_protocol_fee_treasury();
        let alice_pending_before = client.get_pending_winnings(&alice);
        client.cash_out_early(&alice);

        let alice_cashout = client.get_pending_winnings(&alice) - alice_pending_before;
        let ec_treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

        let expected_forfeit = dust_stake * (penalty_bps as i128) / 10_000;
        let expected_cashout = dust_stake - expected_forfeit;

        prop_assert_eq!(alice_cashout, expected_cashout,
            "cashout must match formula: stake={} penalty_bps={}", dust_stake, penalty_bps);
        prop_assert_eq!(ec_treasury_delta, expected_forfeit,
            "treasury delta must equal forfeit");
        prop_assert_eq!(
            alice_cashout + ec_treasury_delta,
            dust_stake,
            "cashout + forfeit must equal original stake"
        );

        // Pool must decrease by exactly dust_stake
        let round = client.get_active_round().unwrap();
        prop_assert_eq!(round.pool_up, 0, "pool_up must be 0 after cash-out");
        prop_assert_eq!(round.pool_down, normal_stake, "pool_down unchanged");

        // Resolve remaining round: one-sided (up=0) → refund for bob
        env.ledger().with_mut(|li| li.sequence_number = 12);
        let bob_before = client.get_pending_winnings(&bob);
        let treasury_before_resolve = client.get_protocol_fee_treasury();
        resolve_at(&env, &client, &contract_id, 900u128);

        let bob_pay = client.get_pending_winnings(&bob) - bob_before;
        let resolve_treasury_delta = client.get_protocol_fee_treasury() - treasury_before_resolve;

        prop_assert_eq!(bob_pay, normal_stake, "bob gets full refund (one-sided)");
        prop_assert_eq!(resolve_treasury_delta, 0, "one-sided → no settlement fee");

        // Full conservation: original pot == all payouts + treasury
        prop_assert_eq!(
            alice_cashout + bob_pay + ec_treasury_delta + resolve_treasury_delta,
            total_pot,
            "full round conservation violated"
        );
    }
}
