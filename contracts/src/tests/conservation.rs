// SPDX-License-Identifier: MIT
//! Stroop-level conservation invariant matrix for settlement.
//!
//! **Why**: value leaks hide in interactions between features (fee on/off,
//! remainder handling, cancellation, and the min-participants fallback —
//! collectively the closest thing this protocol has to a "dispute"
//! resolution path, since there is no separate on-chain dispute primitive).
//! A change that is safe in isolation can silently drop or fabricate a
//! stroop once combined with another code path. This module pins down the
//! exact accounting identity for every settlement path so any leak — even a
//! single stroop — fails a test immediately.
//!
//! The core identity checked throughout, per round:
//! ```text
//! sum(participant payouts/refunds) + protocol_fee_treasury_delta == total_pot
//! ```
//! For Precision mode this holds **exactly** (the contract assigns the
//! integer-division remainder to a single winner). For UpDown mode with
//! multiple winners sharing a side, per-winner floor division can drop up to
//! `winner_count - 1` stroops — that slack is bounded and asserted exactly
//! (never approximately) in every test below.
//!
//! ## Feature matrix
//!
//! | # | Mode      | Path                          | Fee | Remainder | Test |
//! |---|-----------|-------------------------------|-----|-----------|------|
//! | 1 | UpDown    | Win (2-way split)              | off | yes (1 stroop) | [`updown_win_two_way_split_fee_disabled_pins_exact_truncation`] |
//! | 2 | UpDown    | Win (2-way split, clean divide) | on  | no        | [`updown_win_two_way_split_fee_enabled_exact_conservation`] |
//! | 3 | UpDown    | Tie / refund                   | on (must not apply) | n/a | [`updown_tie_refund_ignores_configured_fee`] |
//! | 4 | UpDown    | One-sided pool / refund         | on (must not apply) | n/a | [`updown_one_sided_pool_ignores_configured_fee`] |
//! | 5 | UpDown    | Cancel                          | on (must not apply) | n/a | [`updown_cancel_ignores_configured_fee`] |
//! | 6 | UpDown    | Fallback refund (min-participants) | on (must not apply) | n/a | [`updown_fallback_refund_ignores_configured_fee`] |
//! | 7 | Precision | Win (single winner)             | off | no        | [`precision_single_winner_fee_disabled_exact_conservation`] |
//! | 8 | Precision | Win (tie, 2 winners)             | on  | yes (1 stroop, assigned) | [`precision_tie_two_winners_fee_enabled_exact_remainder`] |
//! | 9 | Precision | All-unrevealed refund           | on (must not apply) | n/a | [`precision_all_unrevealed_refunds_ignores_configured_fee`] |
//! |10 | Precision | Mixed reveal (forfeit-to-pot)    | on  | no        | [`precision_mixed_reveal_forfeit_fee_enabled_exact_conservation`] |
//! |11 | Precision | Cancel (mixed reveal/commit)     | on (must not apply) | n/a | [`precision_cancel_mixed_reveal_ignores_configured_fee`] |
//! |12 | Precision | Fallback refund (mixed reveal/commit) | on (must not apply) | n/a | [`precision_fallback_refund_mixed_reveal_ignores_configured_fee`] |
//! |13 | UpDown    | Early cash-out (10% penalty)        | n/a (penalty→treasury) | n/a | [`early_cashout_conservation_pins_exact_forfeit`] |
//! |14 | UpDown    | Early cash-out disabled              | n/a                    | n/a | [`early_cashout_disabled_rejects_call`] |
//! |15 | UpDown    | Early cash-out betting phase          | n/a                    | n/a | [`early_cashout_betting_phase_rejected`] |
//! |16 | UpDown    | Early cash-out after end              | n/a                    | n/a | [`early_cashout_after_end_rejected`] |
//! |17 | Precision | Early cash-out rejected               | n/a                    | n/a | [`early_cashout_precision_mode_rejected`] |
//! |18 | UpDown    | Early cash-out + settlement fee       | on (settlement only)   | n/a | [`early_cashout_with_settlement_fee_conservation`] |
//! |19 | UpDown    | Early cash-out zero forfeit           | n/a                    | n/a | [`early_cashout_zero_forfeit_full_refund`] |
//! |20 | UpDown    | Early cash-out no position            | n/a                    | n/a | [`early_cashout_no_position_rejected`] |
//! |21 | UpDown    | Early cash-out double call            | n/a                    | n/a | [`early_cashout_double_call_rejected`] |
//! |22 | UpDown    | Early cash-out dust stake (forfeit floors to 0) | n/a            | n/a | [`early_cashout_dust_stake_forfeits_nothing_and_refunds_in_full`] |
//! |23 | UpDown    | Early cash-out dust + insurance split | n/a                  | n/a | [`early_cashout_dust_stake_with_insurance_split_credits_nothing`] |
//! |24 | UpDown    | Early cash-out forfeit split across insurance/ops | n/a             | n/a | [`early_cashout_forfeit_split_conserves_across_insurance_and_treasury`] |
//! |25 | UpDown    | Early cash-out fee-on / fee-off        | on + off             | n/a | [`early_cashout_fee_on_and_fee_off_conserve_and_keep_pools_consistent`] |
//! |26 | UpDown    | Early cash-out, all participants exit  | n/a                    | n/a | [`early_cashout_all_participants_exit_leaves_consistent_empty_pot`] |
//! |27 | UpDown    | Dust boundary sweep (property)          | on + off             | floor | [`early_cashout_dust_boundary_conservation`] |
//!
//! Rows 22–27 (Issue #525) cover early cash-out dust and fee edges. The
//! forfeited amount is floor-divided, so a small stake forfeits nothing and is
//! refunded in full while still leaving the pot. The fee is also **fenced**:
//! `_collect_protocol_fee` routes `insurance_split_bps` of it to the
//! segregated insurance fund and only the remainder to the ops treasury, so
//! conservation must be asserted as
//! `cashout + treasury_delta + insurance_delta == stake`. Asserting only the
//! treasury leg would under-count by exactly the insurance portion.
//!
//! Rows 9–10 guard a real regression: `_resolve_precision_mode` used to drop
//! every committed-but-unrevealed stroop on the floor when **nobody**
//! revealed (no winner to forfeit the pot to, and no refund branch existed).
//! That is fixed in `settlement.rs` alongside this suite — row 9 is the
//! exact scenario that leaked before the fix.
//!
//! Two property-based blocks at the bottom add small-random coverage on top
//! of the fixed matrix above (fixed + small random, per the task spec).
//!
//! This file is registered as `mod conservation;` in `tests/mod.rs`, so it
//! runs under the existing `cargo test --workspace` CI job — no separate
//! wiring is required for CI inclusion.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, DataKeyCore, OraclePayload, RoundArchiveStatus};
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    xdr::ToXdr,
    Address, Bytes, BytesN, Env,
};

// ─── Shared test helpers ─────────────────────────────────────────────────────

fn setup_contract(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    (client, contract_id, admin, oracle)
}

/// Writes the protocol fee bps directly into storage, bypassing the
/// timelocked scheduler (`set_protocol_fee_bps` only *schedules* a pending
/// change) so tests can exercise a fee-active round immediately. Mirrors the
/// existing convention in `tests/property_invariants.rs`.
fn set_fee_bps_now(env: &Env, contract_id: &Address, bps: u32) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::ProtocolFeeBps, &bps);
    });
}

/// Resolves the currently-active round at `final_price` using a fresh nonce.
fn resolve_at(
    env: &Env,
    client: &VirtualTokenContractClient,
    contract_id: &Address,
    final_price: u128,
) {
    let round = client
        .get_active_round()
        .expect("active round required to resolve");
    client.resolve_round(&OraclePayload {
        price: final_price,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });
}

/// `sha256(price.to_xdr() || salt.to_xdr())` — matches `reveal_prediction`.
fn make_commitment(env: &Env, price: u128, salt: &BytesN<32>) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&price.to_xdr(env));
    preimage.append(&salt.clone().to_xdr(env));
    let hash = env.crypto().sha256(&preimage);
    hash.into()
}

/// Salt satisfying on-chain minimum entropy (non-zero, non-constant bytes).
fn test_salt(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        bytes[i] = seed.wrapping_add(i as u8).wrapping_mul(17).wrapping_add(3);
        i += 1;
    }
    bytes[0] = seed | 0x80;
    bytes[31] = seed ^ 0x5A;
    BytesN::from_array(env, &bytes)
}

// ─── Row 1–2: UpDown wins ────────────────────────────────────────────────────

/// Row 1 — fee disabled, two winners on the same side sharing an uneven
/// split. Floor division on each winner independently drops exactly one
/// stroop here; the assertion pins that exact number so a regression that
/// drops (or fabricates) an *additional* stroop fails immediately.
#[test]
fn updown_win_two_way_split_fee_disabled_pins_exact_truncation() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &3, &BetSide::Up);
    client.place_bet(&bob, &4, &BetSide::Up);
    client.place_bet(&charlie, &5, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 2_000u128); // price up

    let alice_pay = client.get_pending_winnings(&alice);
    let bob_pay = client.get_pending_winnings(&bob);
    let charlie_pay = client.get_pending_winnings(&charlie);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(alice_pay, 5); // floor(3*12/7)
    assert_eq!(bob_pay, 6); // floor(4*12/7)
    assert_eq!(charlie_pay, 0);
    assert_eq!(treasury_delta, 0);
    assert_eq!(
        alice_pay + bob_pay + charlie_pay + treasury_delta,
        11,
        "expected exactly 1 stroop of unavoidable floor-division slack"
    );
}

/// Row 2 — fee enabled (10%), amounts chosen so every division is exact.
/// Conservation must hold with **zero** slack: payouts + treasury == pot.
#[test]
fn updown_win_two_way_split_fee_enabled_exact_conservation() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &60, &BetSide::Up);
    client.place_bet(&bob, &40, &BetSide::Up);
    client.place_bet(&charlie, &100, &BetSide::Down);
    set_fee_bps_now(&env, &contract_id, 1_000); // 10%

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 2_000u128); // price up

    let alice_pay = client.get_pending_winnings(&alice);
    let bob_pay = client.get_pending_winnings(&bob);
    let charlie_pay = client.get_pending_winnings(&charlie);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(alice_pay, 108);
    assert_eq!(bob_pay, 72);
    assert_eq!(charlie_pay, 0);
    assert_eq!(treasury_delta, 20);
    assert_eq!(alice_pay + bob_pay + charlie_pay + treasury_delta, 200);
}

// ─── Row 3–4: UpDown refund paths ────────────────────────────────────────────

/// Row 3 — price unchanged (tie): full refund, and a configured fee must
/// never be charged on a non-competitive settlement.
#[test]
fn updown_tie_refund_ignores_configured_fee() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &7, &BetSide::Up);
    client.place_bet(&bob, &13, &BetSide::Down);
    set_fee_bps_now(&env, &contract_id, 500);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_000u128); // unchanged

    assert_eq!(client.get_pending_winnings(&alice), 7);
    assert_eq!(client.get_pending_winnings(&bob), 13);
    assert_eq!(client.get_protocol_fee_treasury() - treasury_before, 0);
}

/// Row 4 — one-sided pool (only Up bets exist): refund regardless of price
/// movement direction, and a configured fee must not apply.
#[test]
fn updown_one_sided_pool_ignores_configured_fee() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &9, &BetSide::Up);
    client.place_bet(&bob, &11, &BetSide::Up);
    set_fee_bps_now(&env, &contract_id, 700);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 2_000u128); // price up, but down pool is empty

    assert_eq!(client.get_pending_winnings(&alice), 9);
    assert_eq!(client.get_pending_winnings(&bob), 11);
    assert_eq!(client.get_protocol_fee_treasury() - treasury_before, 0);
}

// ─── Row 5–6: UpDown cancel / fallback ("dispute-adjacent") paths ───────────

/// Row 5 — admin cancellation: full refund, fee must not apply.
#[test]
fn updown_cancel_ignores_configured_fee() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &15, &BetSide::Up);
    client.place_bet(&bob, &25, &BetSide::Down);
    set_fee_bps_now(&env, &contract_id, 700);

    let treasury_before = client.get_protocol_fee_treasury();
    client.cancel_round(&1u32);

    assert_eq!(client.get_pending_winnings(&alice), 15);
    assert_eq!(client.get_pending_winnings(&bob), 25);
    assert_eq!(client.get_protocol_fee_treasury() - treasury_before, 0);
    assert_eq!(client.get_active_round(), None);
}

/// Row 6 — insufficient participants at settlement: fallback refund, fee
/// must not apply. Also pins the archived-round status code.
#[test]
fn updown_fallback_refund_ignores_configured_fee() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &17, &BetSide::Up);
    client.set_min_participants(&Some(2u32));
    set_fee_bps_now(&env, &contract_id, 300);

    let round_id = client.get_active_round().unwrap().round_id;
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_001u128);

    assert_eq!(client.get_pending_winnings(&alice), 17);
    assert_eq!(client.get_protocol_fee_treasury() - treasury_before, 0);
    assert_eq!(client.get_active_round(), None);
    assert_eq!(
        client.get_archived_round(&round_id).unwrap().status,
        RoundArchiveStatus::FallbackRefund
    );
}

// ─── Row 7–8: Precision wins ─────────────────────────────────────────────────

/// Row 7 — single winner, fee disabled: winner takes the entire pot.
#[test]
fn precision_single_winner_fee_disabled_exact_conservation() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &Some(1));
    client.place_precision_prediction(&alice, &50, &1_005u128);
    client.place_precision_prediction(&bob, &30, &1_100u128);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_006u128);

    let alice_pay = client.get_pending_winnings(&alice);
    let bob_pay = client.get_pending_winnings(&bob);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(alice_pay, 80);
    assert_eq!(bob_pay, 0);
    assert_eq!(treasury_delta, 0);
    assert_eq!(alice_pay + bob_pay + treasury_delta, 80);
}

/// Row 8 — two-way tie with fee enabled (10%): remainder handling is exact
/// (Precision mode assigns the leftover stroop to exactly one winner, so
/// conservation holds with zero slack regardless of which winner it is).
#[test]
fn precision_tie_two_winners_fee_enabled_exact_remainder() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &Some(1));
    client.place_precision_prediction(&alice, &50, &1_005u128); // diff 5
    client.place_precision_prediction(&bob, &50, &995u128); // diff 5 (tie)
    client.place_precision_prediction(&charlie, &50, &1_200u128); // loses
    set_fee_bps_now(&env, &contract_id, 1_000); // 10%

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_000u128);

    let alice_pay = client.get_pending_winnings(&alice);
    let bob_pay = client.get_pending_winnings(&bob);
    let charlie_pay = client.get_pending_winnings(&charlie);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    let (lo, hi) = if alice_pay < bob_pay {
        (alice_pay, bob_pay)
    } else {
        (bob_pay, alice_pay)
    };
    assert_eq!(lo, 67);
    assert_eq!(hi, 68);
    assert_eq!(charlie_pay, 0);
    assert_eq!(treasury_delta, 15);
    assert_eq!(
        alice_pay + bob_pay + charlie_pay + treasury_delta,
        150,
        "Precision conservation must be exact, even with an odd remainder"
    );
}

// ─── Row 9–10: Precision commit-reveal interactions (the leak scenario) ────

/// Row 9 — **regression guard**: nobody reveals. Before the fix in
/// `settlement.rs::_resolve_precision_mode`, this branch had no refund path
/// at all — every committed stroop vanished (0 payouts, 0 treasury, but a
/// non-zero pot). This test fails immediately if that leak reappears.
#[test]
fn precision_all_unrevealed_refunds_ignores_configured_fee() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &Some(1));
    let salt_a = test_salt(&env, 1);
    let salt_b = test_salt(&env, 2);
    client.commit_prediction(&alice, &make_commitment(&env, 1_005u128, &salt_a), &40);
    client.commit_prediction(&bob, &make_commitment(&env, 995u128, &salt_b), &60);
    set_fee_bps_now(&env, &contract_id, 500);

    // Neither user reveals.
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_000u128);

    let alice_pay = client.get_pending_winnings(&alice);
    let bob_pay = client.get_pending_winnings(&bob);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(
        alice_pay, 40,
        "unrevealed stake must be refunded, not burned"
    );
    assert_eq!(bob_pay, 60, "unrevealed stake must be refunded, not burned");
    assert_eq!(treasury_delta, 0, "fee must not apply on a refund path");
    assert_eq!(alice_pay + bob_pay + treasury_delta, 100);
}

/// Row 10 — mixed reveal: Alice reveals and wins the full (fee-adjusted)
/// pot; Bob never reveals and forfeits his stake to the pot rather than
/// being refunded (anti-griefing incentive to reveal). Conservation is
/// exact even though the two participants take very different paths.
#[test]
fn precision_mixed_reveal_forfeit_fee_enabled_exact_conservation() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &Some(1));
    let salt_a = test_salt(&env, 3);
    let salt_b = test_salt(&env, 4);
    client.commit_prediction(&alice, &make_commitment(&env, 1_005u128, &salt_a), &40);
    client.commit_prediction(&bob, &make_commitment(&env, 995u128, &salt_b), &60);
    set_fee_bps_now(&env, &contract_id, 500); // 5%

    env.ledger().with_mut(|li| li.sequence_number = 7); // reveal window opens at 6
    client.reveal_prediction(&alice, &1_005u128, &salt_a);
    // Bob deliberately never reveals.

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_006u128);

    let alice_pay = client.get_pending_winnings(&alice);
    let bob_pay = client.get_pending_winnings(&bob);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    assert_eq!(alice_pay, 95); // (40 + 60) * 0.95, single winner, no remainder
    assert_eq!(bob_pay, 0);
    assert_eq!(treasury_delta, 5);
    assert_eq!(alice_pay + bob_pay + treasury_delta, 100);
}

// ─── Row 11–12: Precision cancel / fallback ("dispute-adjacent") paths ─────

/// Row 11 — admin cancellation with a mix of a revealed prediction and a
/// still-committed (unrevealed) commitment: both must be refunded in full,
/// and a configured fee must not apply.
#[test]
fn precision_cancel_mixed_reveal_ignores_configured_fee() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &Some(1));
    let salt_a = test_salt(&env, 5);
    let salt_b = test_salt(&env, 6);
    client.commit_prediction(&alice, &make_commitment(&env, 1_005u128, &salt_a), &25);
    client.commit_prediction(&bob, &make_commitment(&env, 995u128, &salt_b), &35);
    set_fee_bps_now(&env, &contract_id, 600);

    env.ledger().with_mut(|li| li.sequence_number = 7);
    client.reveal_prediction(&alice, &1_005u128, &salt_a);
    // Bob stays an unrevealed commitment.

    let treasury_before = client.get_protocol_fee_treasury();
    client.cancel_round(&2u32);

    assert_eq!(client.get_pending_winnings(&alice), 25);
    assert_eq!(client.get_pending_winnings(&bob), 35);
    assert_eq!(client.get_protocol_fee_treasury() - treasury_before, 0);
    assert_eq!(client.get_active_round(), None);
}

/// Row 12 — insufficient participants at settlement with a mix of a
/// revealed prediction and an unrevealed commitment: both refunded, fee
/// must not apply, archived status pinned.
#[test]
fn precision_fallback_refund_mixed_reveal_ignores_configured_fee() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &Some(1));
    let salt_a = test_salt(&env, 7);
    let salt_b = test_salt(&env, 8);
    client.commit_prediction(&alice, &make_commitment(&env, 1_005u128, &salt_a), &10);
    client.commit_prediction(&bob, &make_commitment(&env, 995u128, &salt_b), &20);
    client.set_min_participants(&Some(3u32));
    set_fee_bps_now(&env, &contract_id, 400);

    env.ledger().with_mut(|li| li.sequence_number = 7);
    client.reveal_prediction(&alice, &1_005u128, &salt_a);
    // Bob stays an unrevealed commitment.

    let round_id = client.get_active_round().unwrap().round_id;
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_000u128);

    assert_eq!(client.get_pending_winnings(&alice), 10);
    assert_eq!(client.get_pending_winnings(&bob), 20);
    assert_eq!(client.get_protocol_fee_treasury() - treasury_before, 0);
    assert_eq!(
        client.get_archived_round(&round_id).unwrap().status,
        RoundArchiveStatus::FallbackRefund
    );
}

// ─── Early cash-out conservation ────────────────────────────────────────────

/// Helper: writes early cash-out bps directly into storage, bypassing the
/// admin setter (mirrors `set_fee_bps_now` convention).
fn set_ec_bps_now(env: &Env, contract_id: &Address, bps: u32) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::EarlyCashoutBps, &bps);
    });
}

/// Row 13 — Early cash-out with 10% penalty: user forfeits 10% to treasury,
/// remaining participants benefit from the full pool conservation.
/// Conservation identity: cashout + treasury_delta_at_cashout == stake.
#[test]
fn early_cashout_conservation_pins_exact_forfeit() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &100, &BetSide::Up);
    client.place_bet(&bob, &50, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 1_000); // 10% penalty

    // Advance to Running phase (ledger 6 >= bet_end_ledger)
    env.ledger().with_mut(|li| li.sequence_number = 7);

    let alice_pending_before = client.get_pending_winnings(&alice);
    let treasury_before = client.get_protocol_fee_treasury();
    client.cash_out_early(&alice);

    let alice_cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // 100 * 1000 / 10000 = 10 forfeited, 90 returned
    assert_eq!(alice_cashout, 90);
    assert_eq!(treasury_delta, 10);
    assert_eq!(alice_cashout + treasury_delta, 100);

    // Alice's position should be gone
    assert!(client.get_user_position(&alice).is_none());

    // Bob's position remains intact
    let bobs_pos = client.get_user_position(&bob).unwrap();
    assert_eq!(bobs_pos.amount, 50);

    // Pool should be reduced by full stake
    let round = client.get_active_round().unwrap();
    assert_eq!(
        round.pool_up, 0,
        "pool_up should be 0 after alice cashed out"
    );
    assert_eq!(round.pool_down, 50);

    // Resolve the round — Bob wins (price down). Bob's 50 in pool_down wins
    // against 0 in pool_up (since alice cashed out).
    // This is a one-sided pool now, so bob gets refunded, not winnings.
    // Conservation check: bob's payout should equal his stake.
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let bob_pending_before = client.get_pending_winnings(&bob);
    let treasury_before_resolve = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 900u128); // price down

    let bob_pay = client.get_pending_winnings(&bob) - bob_pending_before;
    let resolve_treasury_delta = client.get_protocol_fee_treasury() - treasury_before_resolve;

    // One-sided pool (up=0) → both sides refunded. Bob gets his 50 back.
    assert_eq!(bob_pay, 50);
    assert_eq!(resolve_treasury_delta, 0);
    assert_eq!(client.get_active_round(), None);
}

/// Row 14 — Early cash-out disabled: error when feature not enabled.
#[test]
fn early_cashout_disabled_rejects_call() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &100, &BetSide::Up);

    // Advance to Running phase
    env.ledger().with_mut(|li| li.sequence_number = 7);

    // Feature not enabled — should fail with EarlyCashoutDisabled
    let result = client.try_cash_out_early(&alice);
    assert_eq!(result, Err(Ok(ContractError::EarlyCashoutDisabled)));
}

/// Row 15 — Early cash-out during Betting phase: rejected.
#[test]
fn early_cashout_betting_phase_rejected() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &100, &BetSide::Up);
    set_ec_bps_now(&env, &contract_id, 500);

    // Still in Betting phase (ledger 0 < bet_end_ledger 6)
    let result = client.try_cash_out_early(&alice);
    assert_eq!(result, Err(Ok(ContractError::InvalidPhaseForCashout)));
}

/// Row 16 — Early cash-out after round ended: rejected.
#[test]
fn early_cashout_after_end_rejected() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &100, &BetSide::Up);
    set_ec_bps_now(&env, &contract_id, 500);

    // Advance past end_ledger (12)
    env.ledger().with_mut(|li| li.sequence_number = 13);

    let result = client.try_cash_out_early(&alice);
    assert_eq!(result, Err(Ok(ContractError::InvalidPhaseForCashout)));
}

/// Row 17 — Early cash-out on Precision round: rejected.
#[test]
fn early_cashout_precision_mode_rejected() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_000u128, &Some(1)); // Precision mode
    client.place_precision_prediction(&alice, &100, &1_005u128);
    set_ec_bps_now(&env, &contract_id, 500);

    // Advance to Running phase
    env.ledger().with_mut(|li| li.sequence_number = 7);

    let result = client.try_cash_out_early(&alice);
    assert_eq!(result, Err(Ok(ContractError::WrongModeForCashout)));
}

/// Row 18 — Conservation with fee enabled: early cash-out forfeit goes to
/// treasury, then normal settlement with protocol fee on the remaining pot
/// still conserves exactly.
#[test]
fn early_cashout_with_settlement_fee_conservation() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &200, &BetSide::Up);
    client.place_bet(&bob, &100, &BetSide::Down);
    client.place_bet(&charlie, &100, &BetSide::Down);

    // Enable both early cash-out (5% penalty) and settlement fee (10%)
    set_ec_bps_now(&env, &contract_id, 500);
    set_fee_bps_now(&env, &contract_id, 1_000);

    // Alice cashes out during Running phase
    env.ledger().with_mut(|li| li.sequence_number = 7);
    let alice_pending_before = client.get_pending_winnings(&alice);
    let treasury_before = client.get_protocol_fee_treasury();
    client.cash_out_early(&alice);

    let alice_cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let ec_treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // 5% penalty on 200 = 10 forfeit, 190 cashout
    assert_eq!(alice_cashout, 190);
    assert_eq!(ec_treasury_delta, 10);

    // Remaining pool: up=0, down=200
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 200);

    // Resolve — price up (one-sided: up=0, down=200). Refund for both.
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let bob_pending_before = client.get_pending_winnings(&bob);
    let charlie_pending_before = client.get_pending_winnings(&charlie);
    let treasury_before_resolve = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_100u128); // price up

    let bob_pay = client.get_pending_winnings(&bob) - bob_pending_before;
    let charlie_pay = client.get_pending_winnings(&charlie) - charlie_pending_before;
    let resolve_treasury_delta = client.get_protocol_fee_treasury() - treasury_before_resolve;

    // One-sided pool → refund, no fee applied
    assert_eq!(bob_pay, 100);
    assert_eq!(charlie_pay, 100);
    assert_eq!(resolve_treasury_delta, 0);

    // Total conservation: cashout + refunds + treasury == original stakes
    assert_eq!(
        alice_cashout + bob_pay + charlie_pay + ec_treasury_delta + resolve_treasury_delta,
        400,
        "full round conservation including early cash-out penalty"
    );
}

/// Row 19 — Full refund on zero forfeit (stake too small relative to bps).
#[test]
fn early_cashout_zero_forfeit_full_refund() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &5, &BetSide::Up);
    client.place_bet(&bob, &100, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 100); // 1% → 5 * 100 / 10000 = 0

    env.ledger().with_mut(|li| li.sequence_number = 7);
    let alice_pending_before = client.get_pending_winnings(&alice);
    let treasury_before = client.get_protocol_fee_treasury();
    client.cash_out_early(&alice);

    let alice_cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // Forfeit = 5 * 100 / 10000 = 0 (floor), full refund
    assert_eq!(alice_cashout, 5);
    assert_eq!(treasury_delta, 0);
    assert_eq!(alice_cashout + treasury_delta, 5);

    // Pool should be reduced by full 5
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 100);
}

/// Row 20 — User with no position calling cash-out: PositionNotFound error.
#[test]
fn early_cashout_no_position_rejected() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &100, &BetSide::Up);
    set_ec_bps_now(&env, &contract_id, 500);

    // Bob never placed a bet
    env.ledger().with_mut(|li| li.sequence_number = 7);
    let result = client.try_cash_out_early(&bob);
    assert_eq!(result, Err(Ok(ContractError::PositionNotFound)));
}

/// Row 21 — Double cash-out: second call fails (position already removed).
#[test]
fn early_cashout_double_call_rejected() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &100, &BetSide::Up);
    set_ec_bps_now(&env, &contract_id, 500);

    env.ledger().with_mut(|li| li.sequence_number = 7);
    client.cash_out_early(&alice);

    // Second call should fail — position was removed
    let result = client.try_cash_out_early(&alice);
    assert_eq!(result, Err(Ok(ContractError::PositionNotFound)));
}

/// Early cash-out rejected when contract is paused.
#[test]
fn early_cashout_paused_rejected() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &100, &BetSide::Up);
    set_ec_bps_now(&env, &contract_id, 500);

    env.ledger().with_mut(|li| li.sequence_number = 7);
    client.pause_contract();

    let result = client.try_cash_out_early(&alice);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
}

/// Invariant: cashout + forfeit == stake verified explicitly.
#[test]
fn test_early_cashout_conservation_invariant() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_000u128, &None);
    let stake = 100i128;
    client.place_bet(&alice, &stake, &BetSide::Up);
    let penalty_bps = 1000u32; // 10%
    set_ec_bps_now(&env, &contract_id, penalty_bps);

    env.ledger().with_mut(|li| li.sequence_number = 7);

    let expected_forfeit = stake * (penalty_bps as i128) / 10000i128;
    let expected_cashout = stake - expected_forfeit;
    assert_eq!(
        expected_cashout + expected_forfeit,
        stake,
        "cashout + forfeit == stake invariant holds"
    );

    client.cash_out_early(&alice);

    let pending = client.get_pending_winnings(&alice);
    assert_eq!(pending, expected_cashout);
}

// ─── Small-random coverage on top of the fixed matrix ───────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    /// UpDown mode across randomized pool sizes, fee bps, and settlement
    /// direction (up/down/tie). Regardless of which branch fires (win,
    /// tie-refund, or one-sided-refund), conservation must hold within the
    /// documented floor-division slack bound (at most 1 stroop here, since
    /// at most two bettors ever share the winning side).
    #[test]
    fn conservation_matrix_updown_small_random(
        a_up in 1i128..=1_000i128,
        b_up in 0i128..=1_000i128,
        c_down in 0i128..=1_000i128,
        fee_bps_raw in 0u32..=1_000u32,
        direction in 0u8..=2u8, // 0 = down, 1 = tie, 2 = up
    ) {
        let env = Env::default();
        let (client, contract_id, _admin, _oracle) = setup_contract(&env);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);
        client.mint_initial(&alice);
        client.mint_initial(&bob);
        client.mint_initial(&charlie);

        client.create_round(&1_000u128, &None);
        client.place_bet(&alice, &a_up, &BetSide::Up);
        if b_up > 0 {
            client.place_bet(&bob, &b_up, &BetSide::Up);
        }
        if c_down > 0 {
            client.place_bet(&charlie, &c_down, &BetSide::Down);
        }
        if fee_bps_raw > 0 {
            set_fee_bps_now(&env, &contract_id, fee_bps_raw);
        }

        let total_pot = a_up + b_up + c_down;
        let final_price = match direction {
            0 => 900u128,
            1 => 1_000u128,
            _ => 1_100u128,
        };

        env.ledger().with_mut(|li| li.sequence_number = 12);
        let treasury_before = client.get_protocol_fee_treasury();
        resolve_at(&env, &client, &contract_id, final_price);

        let alice_pay = client.get_pending_winnings(&alice);
        let bob_pay = client.get_pending_winnings(&bob);
        let charlie_pay = client.get_pending_winnings(&charlie);
        let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
        let accounted = alice_pay + bob_pay + charlie_pay + treasury_delta;

        prop_assert!(alice_pay >= 0);
        prop_assert!(bob_pay >= 0);
        prop_assert!(charlie_pay >= 0);
        prop_assert!(treasury_delta >= 0);
        prop_assert!(accounted <= total_pot,
            "conservation upper bound violated: accounted={} pot={}", accounted, total_pot);
        prop_assert!(accounted >= total_pot - 1,
            "leak beyond documented 1-stroop floor-division slack: accounted={} pot={}",
            accounted, total_pot);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    /// Precision mode across randomized amounts, predicted prices, final
    /// price, fee bps, and — crucially — whether each of the two
    /// participants reveals at all. This directly randomizes the exact
    /// interaction that produced the all-/mixed-unrevealed leak, so
    /// conservation is asserted **exactly** (Precision mode has no
    /// multi-winner truncation slack: the remainder always goes to a single
    /// winner, and refund paths return every stroop).
    #[test]
    fn conservation_matrix_precision_small_random(
        amount_a in 1i128..=1_000i128,
        amount_b in 1i128..=1_000i128,
        price_a in 1u128..=99_999_998u128,
        price_b in 1u128..=99_999_998u128,
        final_price in 1u128..=99_999_998u128,
        fee_bps_raw in 0u32..=1_000u32,
        reveal_a in any::<bool>(),
        reveal_b in any::<bool>(),
    ) {
        let env = Env::default();
        let (client, contract_id, _admin, _oracle) = setup_contract(&env);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint_initial(&alice);
        client.mint_initial(&bob);

        client.create_round(&1_000_000u128, &Some(1));
        let salt_a = test_salt(&env, 101);
        let salt_b = test_salt(&env, 102);
        client.commit_prediction(&alice, &make_commitment(&env, price_a, &salt_a), &amount_a);
        client.commit_prediction(&bob, &make_commitment(&env, price_b, &salt_b), &amount_b);
        if fee_bps_raw > 0 {
            set_fee_bps_now(&env, &contract_id, fee_bps_raw);
        }

        env.ledger().with_mut(|li| li.sequence_number = 7);
        if reveal_a {
            client.reveal_prediction(&alice, &price_a, &salt_a);
        }
        if reveal_b {
            client.reveal_prediction(&bob, &price_b, &salt_b);
        }

        env.ledger().with_mut(|li| li.sequence_number = 12);
        let treasury_before = client.get_protocol_fee_treasury();
        resolve_at(&env, &client, &contract_id, final_price);

        let alice_pay = client.get_pending_winnings(&alice);
        let bob_pay = client.get_pending_winnings(&bob);
        let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
        let total_pot = amount_a + amount_b;

        prop_assert!(alice_pay >= 0);
        prop_assert!(bob_pay >= 0);
        prop_assert!(treasury_delta >= 0);
        prop_assert_eq!(
            alice_pay + bob_pay + treasury_delta,
            total_pot,
            "Precision conservation must be exact: alice={} bob={} treasury_delta={} pot={} reveal_a={} reveal_b={}",
            alice_pay, bob_pay, treasury_delta, total_pot, reveal_a, reveal_b
        );

        if !reveal_a && !reveal_b {
            prop_assert_eq!(treasury_delta, 0, "no winner exists to be charged a fee against");
        }
    }
}

// ─── Early cash-out dust & fee edge cases (Issue #525) ──────────────────────
//
// The early cash-out path computes the forfeited amount with floor division:
//
// ```text
// forfeit  = stake * penalty_bps / 10_000      (floor)
// cashout  = stake - forfeit
// ```
//
// Two consequences make dust a genuine edge case rather than a rounding
// footnote:
//
// 1. **Forfeit can floor to zero.** For a small stake (or a small penalty),
//    `stake * penalty_bps < 10_000`, so `forfeit == 0` and the user receives a
//    **full refund** — while still being removed from the pool. The stake
//    leaves the round but no fee is collected, so the round's pot shrinks by
//    the full stake with nothing entering the treasury.
// 2. **The forfeit is split, not fully credited.** `_collect_protocol_fee`
//    routes `insurance_split_bps` of every fee to the segregated insurance
//    fund and only the remainder to the ops treasury. Conservation must
//    therefore be asserted against `cashout + treasury_delta +
//    insurance_delta`, not `cashout + treasury_delta` alone — asserting only
//    the treasury leg would miss value routed to insurance.
//
// The tests below pin both behaviours, plus pool-total consistency after a
// cash-out, for fee-on and fee-off configurations.

/// Row 22 — A stake small enough that the penalty floors to zero is refunded
/// in full, collects no fee, and still leaves the pool.
///
/// This is the dust case: value is returned to the user and removed from the
/// pot without ever reaching treasury or insurance. That is intentional (no
/// fee to collect on a dust amount), but it must not silently *create* value —
/// the pot must decrease by exactly the stake.
#[test]
fn early_cashout_dust_stake_forfeits_nothing_and_refunds_in_full() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    // Alice's dust stake: 5 * 1% = 5/100 of a percent → floors to 0.
    client.place_bet(&alice, &5, &BetSide::Up);
    client.place_bet(&bob, &100, &BetSide::Down);
    set_ec_bps_now(&env, &contract_id, 100); // 1%

    // Preconditions: confirm the fixture really is a dust amount.
    let stake = 5i128;
    let penalty_bps = 100i128;
    assert_eq!(
        stake * penalty_bps / 10_000,
        0,
        "fixture must be dust: forfeit must floor to 0"
    );

    env.ledger().with_mut(|li| li.sequence_number = 7);
    let alice_pending_before = client.get_pending_winnings(&alice);
    let treasury_before = client.get_protocol_fee_treasury();
    let insurance_before = client.get_insurance_fund_balance();

    client.cash_out_early(&alice);

    let cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
    let insurance_delta = client.get_insurance_fund_balance() - insurance_before;

    // Full refund, and no fee leg is credited anywhere.
    assert_eq!(cashout, stake, "dust stake must be refunded in full");
    assert_eq!(
        treasury_delta, 0,
        "dust forfeit must not reach ops treasury"
    );
    assert_eq!(
        insurance_delta, 0,
        "dust forfeit must not reach insurance fund"
    );
    assert_eq!(
        cashout + treasury_delta + insurance_delta,
        stake,
        "no value may be created or destroyed at the cash-out"
    );

    // Pool totals stay consistent: the full stake leaves the pot even though
    // the user was refunded it.
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0, "dust stake must still leave the pool");
    assert_eq!(round.pool_down, 100);
}

/// Row 23 — Dust cash-out with an active insurance split still credits nothing.
///
/// Guards the specific case where a fee model is configured but the fee
/// amount is zero: neither the insurance nor the ops leg may move.
#[test]
fn early_cashout_dust_stake_with_insurance_split_credits_nothing() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &5, &BetSide::Up);
    client.place_bet(&bob, &100, &BetSide::Down);

    set_ec_bps_now(&env, &contract_id, 100); // 1% → dust on a 5-stake
    client.set_insurance_split_bps(&5_000); // 50% of any fee → insurance

    env.ledger().with_mut(|li| li.sequence_number = 7);
    let treasury_before = client.get_protocol_fee_treasury();
    let insurance_before = client.get_insurance_fund_balance();

    client.cash_out_early(&alice);

    assert_eq!(
        client.get_protocol_fee_treasury() - treasury_before,
        0,
        "zero fee must not credit ops treasury"
    );
    assert_eq!(
        client.get_insurance_fund_balance() - insurance_before,
        0,
        "zero fee must not credit insurance fund"
    );
}

/// Row 24 — A non-dust forfeit is split across insurance and ops exactly, and
/// the two legs plus the cashout reconstruct the stake.
///
/// This is the corrected conservation identity for the cash-out path: the fee
/// is *fenced* between two destinations, so asserting only the treasury leg
/// would under-count by exactly the insurance portion.
#[test]
fn early_cashout_forfeit_split_conserves_across_insurance_and_treasury() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    let stake = 1_000i128;
    client.place_bet(&alice, &stake, &BetSide::Up);
    client.place_bet(&bob, &100, &BetSide::Down);

    set_ec_bps_now(&env, &contract_id, 1_000); // 10% → forfeit 100
    client.set_insurance_split_bps(&5_000); // 50% of the forfeit → insurance

    env.ledger().with_mut(|li| li.sequence_number = 7);
    let alice_pending_before = client.get_pending_winnings(&alice);
    let treasury_before = client.get_protocol_fee_treasury();
    let insurance_before = client.get_insurance_fund_balance();

    client.cash_out_early(&alice);

    let cashout = client.get_pending_winnings(&alice) - alice_pending_before;
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
    let insurance_delta = client.get_insurance_fund_balance() - insurance_before;

    let expected_forfeit = stake * 1_000 / 10_000; // 100
    let expected_cashout = stake - expected_forfeit; // 900
    let expected_insurance = expected_forfeit * 5_000 / 10_000; // 50
    let expected_ops = expected_forfeit - expected_insurance; // 50

    assert_eq!(cashout, expected_cashout);
    assert_eq!(insurance_delta, expected_insurance, "insurance leg");
    assert_eq!(treasury_delta, expected_ops, "ops leg");
    assert_eq!(
        cashout + treasury_delta + insurance_delta,
        stake,
        "cashout + ops fee + insurance fee must reconstruct the stake exactly"
    );
}

/// Row 25 — The full fee-on / fee-off matrix: a non-dust cash-out conserves
/// under both settings, with pool totals consistent afterwards.
///
/// Fee-off here means no *settlement* fee configured; the early cash-out
/// penalty is independent of it and must still be collected. Fee-on means a
/// settlement fee is also active, which must not double-charge the cash-out.
#[test]
fn early_cashout_fee_on_and_fee_off_conserve_and_keep_pools_consistent() {
    for settlement_fee_bps in [0u32, 1_000u32] {
        let env = Env::default();
        let (client, contract_id, _admin, _oracle) = setup_contract(&env);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint_initial(&alice);
        client.mint_initial(&bob);

        client.create_round(&1_000u128, &None);
        let stake = 1_000i128;
        client.place_bet(&alice, &stake, &BetSide::Up);
        client.place_bet(&bob, &500, &BetSide::Down);

        set_ec_bps_now(&env, &contract_id, 1_000); // 10% penalty
        if settlement_fee_bps > 0 {
            set_fee_bps_now(&env, &contract_id, settlement_fee_bps);
        }
        client.set_insurance_split_bps(&5_000);

        env.ledger().with_mut(|li| li.sequence_number = 7);
        let alice_pending_before = client.get_pending_winnings(&alice);
        let treasury_before = client.get_protocol_fee_treasury();
        let insurance_before = client.get_insurance_fund_balance();

        client.cash_out_early(&alice);

        let cashout = client.get_pending_winnings(&alice) - alice_pending_before;
        let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
        let insurance_delta = client.get_insurance_fund_balance() - insurance_before;

        // The cash-out penalty is identical regardless of the settlement fee.
        let expected_forfeit = stake * 1_000 / 10_000;
        assert_eq!(cashout, stake - expected_forfeit);
        assert_eq!(
            cashout + treasury_delta + insurance_delta,
            stake,
            "conservation must hold for settlement fee bps = {settlement_fee_bps}"
        );

        // Pool totals consistent: Alice's full stake left the UpDown side.
        let round = client.get_active_round().unwrap();
        assert_eq!(round.pool_up, 0, "fee_bps={settlement_fee_bps}");
        assert_eq!(round.pool_down, 500, "fee_bps={settlement_fee_bps}");
        assert_eq!(
            round.pool_up + round.pool_down,
            500,
            "remaining pot must equal surviving stakes only"
        );

        // Alice is out of the round entirely; Bob is untouched.
        assert!(client.get_user_position(&alice).is_none());
        assert_eq!(client.get_user_position(&bob).unwrap().amount, 500);
    }
}

/// Row 26 — Every participant cashing out leaves an empty, consistent pot
/// that settles as a full refund with no fee on either leg.
///
/// Guards the degenerate all-dust/exit case: repeated cash-outs must not
/// underflow the pool or strand value in a non-existent pot.
#[test]
fn early_cashout_all_participants_exit_leaves_consistent_empty_pot() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &300, &BetSide::Up);
    client.place_bet(&bob, &200, &BetSide::Down);

    set_ec_bps_now(&env, &contract_id, 1_000); // 10%
    client.set_insurance_split_bps(&5_000);

    env.ledger().with_mut(|li| li.sequence_number = 7);
    let alice_before = client.get_pending_winnings(&alice);
    let bob_before = client.get_pending_winnings(&bob);
    let treasury_before = client.get_protocol_fee_treasury();
    let insurance_before = client.get_insurance_fund_balance();

    client.cash_out_early(&alice);
    client.cash_out_early(&bob);

    let alice_cashout = client.get_pending_winnings(&alice) - alice_before; // 270
    let bob_cashout = client.get_pending_winnings(&bob) - bob_before; // 180
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
    let insurance_delta = client.get_insurance_fund_balance() - insurance_before;

    assert_eq!(alice_cashout, 270, "300 less 10% forfeit");
    assert_eq!(bob_cashout, 180, "200 less 10% forfeit");

    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 0, "every stake must have left the pot");

    // Total across both participants and both fee destinations.
    assert_eq!(
        alice_cashout + bob_cashout + treasury_delta + insurance_delta,
        500,
        "full exit must conserve every stroop of the original stakes"
    );
}

// ─── Property: dust boundary sweep (Issue #525) ─────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Across the whole legal penalty range and a dust-to-substantial stake
    /// range, the cash-out accounting identity must hold exactly:
    ///
    /// ```text
    /// cashout + ops_fee + insurance_fee == stake
    /// cashout == stake - floor(stake * penalty_bps / 10_000)
    /// ```
    ///
    /// The forfeit leg is floor-divided, so the exact expected value is
    /// recomputed in the test rather than approximated — any drift in the
    /// contract's division would break equality, not just a tolerance.
    #[test]
    fn early_cashout_dust_boundary_conservation(
        stake in 1i128..=50_000i128,
        penalty_bps in 1u32..=1_000u32,
        insurance_split_bps in 0u32..=5_000u32,
    ) {
        let env = Env::default();
        let (client, contract_id, _admin, _oracle) = setup_contract(&env);

        let alice = Address::generate(&env);
        client.mint_initial(&alice);
        client.create_round(&1_000u128, &None);
        client.place_bet(&alice, &stake, &BetSide::Up);

        set_ec_bps_now(&env, &contract_id, penalty_bps);
        client.set_insurance_split_bps(&insurance_split_bps);

        env.ledger().with_mut(|li| li.sequence_number = 7);
        let pending_before = client.get_pending_winnings(&alice);
        let treasury_before = client.get_protocol_fee_treasury();
        let insurance_before = client.get_insurance_fund_balance();

        client.cash_out_early(&alice);

        let cashout = client.get_pending_winnings(&alice) - pending_before;
        let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
        let insurance_delta = client.get_insurance_fund_balance() - insurance_before;

        // Recompute the contract's own floor-division semantics exactly.
        let expected_forfeit = stake * (penalty_bps as i128) / 10_000i128;
        let expected_cashout = stake - expected_forfeit;
        let expected_insurance = expected_forfeit * (insurance_split_bps as i128) / 10_000i128;
        let expected_ops = expected_forfeit - expected_insurance;

        prop_assert_eq!(cashout, expected_cashout,
            "cashout must be stake minus the floor-divided forfeit");
        prop_assert_eq!(insurance_delta, expected_insurance,
            "insurance leg must be floor(split_bps) of the forfeit");
        prop_assert_eq!(treasury_delta, expected_ops,
            "ops leg must be the remainder of the forfeit");
        prop_assert_eq!(cashout + treasury_delta + insurance_delta, stake,
            "no stroop may be created or destroyed at cash-out");

        // The pot must shed the stake regardless of forfeit size.
        let round = client.get_active_round().unwrap();
        prop_assert_eq!(round.pool_up, 0,
            "the stake must leave the pool even for a full-refund dust exit");
    }
}
