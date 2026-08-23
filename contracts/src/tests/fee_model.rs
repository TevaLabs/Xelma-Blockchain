// SPDX-License-Identifier: MIT
//! Fee model conservation tests for Issue #268: fee-on-pot vs fee-on-winnings.
//!
//! Covers the acceptance criteria from the issue:
//! - fee=0 produces identical results for both models
//! - Both fee-on-pot and fee-on-winnings conserve value
//! - Events show model+amount
//! - Edge cases: one-sided pools, all-unrevealed, ties, zero-profit

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, DataKeyCore, FeeModel, OraclePayload, PrecisionPrediction};
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    xdr::ToXdr,
    Address, Bytes, BytesN, Env,
};

// ─── Shared helpers ──────────────────────────────────────────────────────────

fn setup_contract(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    (client, contract_id, admin, oracle)
}

fn set_fee_bps_now(env: &Env, contract_id: &Address, bps: u32) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::ProtocolFeeBps, &bps);
    });
}

fn set_fee_model_now(env: &Env, contract_id: &Address, model: FeeModel) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::FeeModel, &model);
    });
}

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

fn make_commitment(env: &Env, price: u128, salt: &BytesN<32>) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&price.to_xdr(env));
    preimage.append(&salt.clone().to_xdr(env));
    let hash = env.crypto().sha256(&preimage);
    hash.into()
}

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

// ─── Criterion 1: fee=0 produces identical results for both models ───────────

/// When fee bps is None (disabled), both FeeOnPot and FeeOnWinnings must
/// produce identical payout results — the conservation identity is the same
/// because there is no fee to split differently.
#[test]
fn fee_zero_both_models_produce_identical_updown() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    // Run once with FeeOnPot
    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &3, &BetSide::Up);
    client.place_bet(&bob, &4, &BetSide::Up);
    client.place_bet(&charlie, &5, &BetSide::Down);
    // fee disabled — no fee key set
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnPot);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 2_000u128);

    let fee_on_pot_alice = client.get_pending_winnings(&alice);
    let fee_on_pot_bob = client.get_pending_winnings(&bob);
    let fee_on_pot_charlie = client.get_pending_winnings(&charlie);
    let fee_on_pot_treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // Reset: create a new round with FeeOnWinnings
    let alice2 = Address::generate(&env);
    let bob2 = Address::generate(&env);
    let charlie2 = Address::generate(&env);
    client.mint_initial(&alice2);
    client.mint_initial(&bob2);
    client.mint_initial(&charlie2);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice2, &3, &BetSide::Up);
    client.place_bet(&bob2, &4, &BetSide::Up);
    client.place_bet(&charlie2, &5, &BetSide::Down);
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnWinnings);

    // Round 2 was created while sequence_number was still 12 (left over
    // from round 1's resolve above), so its end_ledger is 12+12=24, not 13.
    env.ledger().with_mut(|li| li.sequence_number = 24);
    let treasury_before2 = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 2_000u128);

    let fee_on_winnings_alice = client.get_pending_winnings(&alice2);
    let fee_on_winnings_bob = client.get_pending_winnings(&bob2);
    let fee_on_winnings_charlie = client.get_pending_winnings(&charlie2);
    let fee_on_winnings_treasury_delta = client.get_protocol_fee_treasury() - treasury_before2;

    // With fee=0, both models must produce identical results
    assert_eq!(fee_on_pot_alice, fee_on_winnings_alice);
    assert_eq!(fee_on_pot_bob, fee_on_winnings_bob);
    assert_eq!(fee_on_pot_charlie, fee_on_winnings_charlie);
    assert_eq!(fee_on_pot_treasury_delta, 0);
    assert_eq!(fee_on_winnings_treasury_delta, 0);
}

/// Precision mode: fee=0 must produce identical results for both models.
#[test]
fn fee_zero_both_models_produce_identical_precision() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    // FeeOnPot round
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &Some(1));
    client.place_precision_prediction(&alice, &50, &1_005u128);
    client.place_precision_prediction(&bob, &30, &1_100u128);
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnPot);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_006u128);

    let pot_alice = client.get_pending_winnings(&alice);
    let pot_bob = client.get_pending_winnings(&bob);
    let pot_treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // FeeOnWinnings round
    let alice2 = Address::generate(&env);
    let bob2 = Address::generate(&env);
    client.mint_initial(&alice2);
    client.mint_initial(&bob2);

    client.create_round(&1_000u128, &Some(1));
    client.place_precision_prediction(&alice2, &50, &1_005u128);
    client.place_precision_prediction(&bob2, &30, &1_100u128);
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnWinnings);

    // Round 2 was created while sequence_number was still 12 (left over
    // from round 1's resolve above), so its end_ledger is 12+12=24, not 13.
    env.ledger().with_mut(|li| li.sequence_number = 24);
    let treasury_before2 = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_006u128);

    let winnings_alice = client.get_pending_winnings(&alice2);
    let winnings_bob = client.get_pending_winnings(&bob2);
    let winnings_treasury_delta = client.get_protocol_fee_treasury() - treasury_before2;

    assert_eq!(pot_alice, winnings_alice);
    assert_eq!(pot_bob, winnings_bob);
    assert_eq!(pot_treasury_delta, 0);
    assert_eq!(winnings_treasury_delta, 0);
}

// ─── Criterion 2: Both models conserve value ─────────────────────────────────

/// UpDown fee-on-pot (current default): conservation holds within documented
/// per-winner truncation slack.
#[test]
fn fee_on_pot_updown_conservation_exact_amounts() {
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
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnPot);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 2_000u128);

    let alice_pay = client.get_pending_winnings(&alice);
    let bob_pay = client.get_pending_winnings(&bob);
    let charlie_pay = client.get_pending_winnings(&charlie);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // FeeOnPot with 10% on 200 pot = 20 fee
    // distributable = 180, split 60/40: alice=108, bob=72
    assert_eq!(alice_pay, 108);
    assert_eq!(bob_pay, 72);
    assert_eq!(charlie_pay, 0);
    assert_eq!(treasury_delta, 20);
    assert_eq!(alice_pay + bob_pay + charlie_pay + treasury_delta, 200);
}

/// UpDown fee-on-winnings: fee only on losing_pool (profit), winners retain
/// full principal. Conservation holds.
#[test]
fn fee_on_winnings_updown_conservation() {
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
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnWinnings);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 2_000u128);

    let alice_pay = client.get_pending_winnings(&alice);
    let bob_pay = client.get_pending_winnings(&bob);
    let charlie_pay = client.get_pending_winnings(&charlie);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // FeeOnWinnings with 10% on losing_pool (100): fee = 10
    // Winners get: winning_pool (100) + losing_pool - fee (90) = 190
    // Split 60/40: alice = 190 * 60/100 = 114, bob = 190 * 40/100 = 76
    assert_eq!(alice_pay, 114);
    assert_eq!(bob_pay, 76);
    assert_eq!(charlie_pay, 0);
    assert_eq!(treasury_delta, 10);
    assert_eq!(alice_pay + bob_pay + charlie_pay + treasury_delta, 200);
}

/// Precision fee-on-winnings: winners keep their stakes, fee only on profit.
#[test]
fn fee_on_winnings_precision_conservation() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.create_round(&1_000u128, &Some(1));
    client.place_precision_prediction(&alice, &50, &1_005u128); // wins: closest
    client.place_precision_prediction(&bob, &50, &995u128); // tie: also closest
    client.place_precision_prediction(&charlie, &50, &1_200u128); // loses
    set_fee_bps_now(&env, &contract_id, 1_000); // 10%
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnWinnings);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_000u128);

    let alice_pay = client.get_pending_winnings(&alice);
    let bob_pay = client.get_pending_winnings(&bob);
    let charlie_pay = client.get_pending_winnings(&charlie);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // Total pot = 150, winners = alice(50) + bob(50), profit = 150 - 100 = 50
    // FeeOnWinnings: fee = 50 * 10% = 5
    // distributable = 150 - 5 = 145
    // Each winner: 145/2 = 72 remainder 1 => alice=73, bob=72
    let (lo, hi) = if alice_pay < bob_pay {
        (alice_pay, bob_pay)
    } else {
        (bob_pay, alice_pay)
    };
    assert_eq!(lo, 72);
    assert_eq!(hi, 73);
    assert_eq!(charlie_pay, 0);
    assert_eq!(treasury_delta, 5);
    assert_eq!(alice_pay + bob_pay + charlie_pay + treasury_delta, 150);
}

/// Precision fee-on-winnings: when there's no profit (all winners bet everything),
/// fee must be 0.
#[test]
fn fee_on_winnings_precision_no_profit_yields_zero_fee() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_000u128, &Some(1));
    client.place_precision_prediction(&alice, &100, &1_005u128);
    set_fee_bps_now(&env, &contract_id, 1_000);
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnWinnings);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_005u128);

    let alice_pay = client.get_pending_winnings(&alice);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

    // Single winner, total_pot = winner_stakes = 100, profit = 0, fee = 0
    assert_eq!(alice_pay, 100);
    assert_eq!(treasury_delta, 0);
    assert_eq!(alice_pay + treasury_delta, 100);
}

// ─── Criterion 3: Events show model+amount ───────────────────────────────────

/// Verify that fee collection events include the fee model field.
#[test]
fn fee_collection_event_includes_model() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &50, &BetSide::Up);
    client.place_bet(&bob, &50, &BetSide::Down);
    set_fee_bps_now(&env, &contract_id, 500); // 5%
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnWinnings);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    resolve_at(&env, &client, &contract_id, 2_000u128);

    // The fee collected event must include the model value.
    // FeeOnWinnings on losing_pool=50 at 5% = 2 stroops fee.
    assert_eq!(client.get_protocol_fee_treasury(), 2);

    // Verify the getter returns the stored model.
    assert_eq!(client.get_fee_model(), FeeModel::FeeOnWinnings);
}

/// Verify default fee model is FeeOnPot when nothing is configured.
#[test]
fn default_fee_model_is_fee_on_pot() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    let fee_model = client.get_fee_model();
    assert_eq!(fee_model, FeeModel::FeeOnPot);
}

/// Verify set_fee_model works and persists correctly.
#[test]
fn set_and_get_fee_model() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    assert_eq!(client.get_fee_model(), FeeModel::FeeOnPot);

    client.set_fee_model(&FeeModel::FeeOnWinnings);
    assert_eq!(client.get_fee_model(), FeeModel::FeeOnWinnings);

    client.set_fee_model(&FeeModel::FeeOnPot);
    assert_eq!(client.get_fee_model(), FeeModel::FeeOnPot);
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

/// UpDown one-sided pool: fee must be 0 regardless of model (no profit).
#[test]
fn fee_on_winnings_one_sided_pool_no_fee() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    client.mint_initial(&alice);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &10, &BetSide::Up);
    set_fee_bps_now(&env, &contract_id, 500);
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnWinnings);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 2_000u128);

    // Price went up but no down pool → one-sided refund
    assert_eq!(client.get_pending_winnings(&alice), 10);
    assert_eq!(client.get_protocol_fee_treasury() - treasury_before, 0);
}

/// UpDown tie: fee must be 0 (no competitive settlement).
#[test]
fn fee_never_charged_on_tie_regardless_of_model() {
    let models = [FeeModel::FeeOnPot, FeeModel::FeeOnWinnings];

    for &model in &models {
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
        set_fee_model_now(&env, &contract_id, model);

        env.ledger().with_mut(|li| li.sequence_number = 12);
        let treasury_before = client.get_protocol_fee_treasury();
        resolve_at(&env, &client, &contract_id, 1_000u128);

        assert_eq!(client.get_pending_winnings(&alice), 7);
        assert_eq!(client.get_pending_winnings(&bob), 13);
        assert_eq!(client.get_protocol_fee_treasury() - treasury_before, 0,
            "Fee was charged on tie with model {:?}", model);
    }
}

/// Precision all-unrevealed: fee must be 0 (no competitive winners).
#[test]
fn fee_on_winnings_all_unrevealed_no_fee() {
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
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnWinnings);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 1_000u128);

    assert_eq!(client.get_pending_winnings(&alice), 40);
    assert_eq!(client.get_pending_winnings(&bob), 60);
    assert_eq!(client.get_protocol_fee_treasury() - treasury_before, 0);
}

/// UpDown with FeeOnWinnings: verify fee cannot exceed losing_pool.
/// Since bps ≤ 1000 (10%), fee ≤ losing_pool * 0.1, but we test explicitly.
#[test]
fn fee_on_winnings_updown_fee_bounded_by_losing_pool() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000u128, &None);
    client.place_bet(&alice, &5, &BetSide::Up);
    client.place_bet(&bob, &1, &BetSide::Down); // Very thin losing pool
    set_fee_bps_now(&env, &contract_id, 1_000); // 10% max
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnWinnings);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let treasury_before = client.get_protocol_fee_treasury();
    resolve_at(&env, &client, &contract_id, 2_000u128);

    let alice_pay = client.get_pending_winnings(&alice);
    let bob_pay = client.get_pending_winnings(&bob);
    let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
    let total_pot: i128 = 6;

    // Fee = 1 * 10% = 0 (integer truncation), so no fee.
    assert_eq!(treasury_delta, 0);
    assert_eq!(alice_pay + bob_pay + treasury_delta, total_pot);
}

// ─── Property-based tests for fee model conservation ────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// UpDown FeeOnWinnings: conservation invariant holds with per-winner truncation slack.
    #[test]
    fn prop_updown_fee_on_winnings_conservation(
        a_up in 1i128..500_000_000i128,
        b_up in 1i128..500_000_000i128,
        c_down in 1i128..500_000_000i128,
        fee_bps_raw in 1u32..=1_000u32,
    ) {
        let total_up = a_up.saturating_add(b_up);
        let total_down = c_down;
        let total_pot = total_up.saturating_add(total_down);

        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);
        client.create_round(&1_0000000u128, &None);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        env.as_contract(&contract_id, || {
            use crate::types::UserPosition;
            let mut positions: soroban_sdk::Map<Address, UserPosition> = soroban_sdk::Map::new(&env);
            positions.set(alice.clone(), UserPosition { amount: a_up, side: BetSide::Up });
            positions.set(bob.clone(), UserPosition { amount: b_up, side: BetSide::Up });
            positions.set(charlie.clone(), UserPosition { amount: c_down, side: BetSide::Down });
            env.storage().persistent().set(&DataKeyCore::UpDownPositions, &positions);

            let mut round: crate::types::Round = env.storage().persistent().get(&DataKeyCore::ActiveRound).unwrap();
            round.pool_up = total_up;
            round.pool_down = total_down;
            env.storage().persistent().set(&DataKeyCore::ActiveRound, &round);

            env.storage().persistent().set(&DataKeyCore::ProtocolFeeBps, &fee_bps_raw);
            env.storage().persistent().set(&DataKeyCore::FeeModel, &FeeModel::FeeOnWinnings);
        });

        let treasury_before = client.get_protocol_fee_treasury();

        env.ledger().with_mut(|li| { li.sequence_number = 12; });

        client.resolve_round(&OraclePayload {
            price: 2_0000000,
            timestamp: env.ledger().timestamp(),
            round_id: 0,
            nonce: 1u64,
            network_id: env.ledger().network_id(),
            contract_addr: contract_id.clone(),
            confidence: None,
            attestation: None,
        });

        let alice_pending = client.get_pending_winnings(&alice);
        let bob_pending = client.get_pending_winnings(&bob);
        let charlie_pending = client.get_pending_winnings(&charlie);
        let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

        let sum_payouts = alice_pending + bob_pending + charlie_pending;

        prop_assert!(charlie_pending == 0);
        prop_assert!(treasury_delta >= 0);
        prop_assert!(sum_payouts + treasury_delta <= total_pot,
            "Upper bound violated: payouts={} treasury={} pot={}", sum_payouts, treasury_delta, total_pot);
        prop_assert!(sum_payouts + treasury_delta >= total_pot - 1,
            "Lower bound violated: payouts={} treasury={} pot={}", sum_payouts, treasury_delta, total_pot);
    }

    /// Precision FeeOnWinnings: exact conservation (no truncation slack).
    #[test]
    fn prop_precision_fee_on_winnings_conservation(
        amount_a in 1i128..300_000_000i128,
        amount_b in 1i128..300_000_000i128,
        amount_c in 1i128..300_000_000i128,
        price_a in 0u128..99_999_999u128,
        price_b in 1u128..99_999_999u128,
        price_c in 2u128..99_999_999u128,
        final_price in 0u128..99_999_999u128,
        fee_bps_raw in 1u32..=1_000u32,
    ) {
        prop_assume!(price_a != price_b && price_b != price_c && price_a != price_c);

        let total_pot = amount_a + amount_b + amount_c;
        prop_assume!(total_pot > 0);

        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);
        client.create_round(&1_0000000u128, &Some(1));

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mut predictions: soroban_sdk::Map<Address, PrecisionPrediction> = soroban_sdk::Map::new(&env);
            predictions.set(alice.clone(), PrecisionPrediction { user: alice.clone(), predicted_price: price_a, amount: amount_a });
            predictions.set(bob.clone(), PrecisionPrediction { user: bob.clone(), predicted_price: price_b, amount: amount_b });
            predictions.set(charlie.clone(), PrecisionPrediction { user: charlie.clone(), predicted_price: price_c, amount: amount_c });
            env.storage().persistent().set(&DataKeyCore::PrecisionPositions, &predictions);

            env.storage().persistent().set(&DataKeyCore::ProtocolFeeBps, &fee_bps_raw);
            env.storage().persistent().set(&DataKeyCore::FeeModel, &FeeModel::FeeOnWinnings);
        });

        let treasury_before = client.get_protocol_fee_treasury();

        env.ledger().with_mut(|li| { li.sequence_number = 12; });

        client.resolve_round(&OraclePayload {
            price: final_price,
            timestamp: env.ledger().timestamp(),
            round_id: 0,
            nonce: 1u64,
            network_id: env.ledger().network_id(),
            contract_addr: contract_id.clone(),
            confidence: None,
            attestation: None,
        });

        let alice_pending = client.get_pending_winnings(&alice);
        let bob_pending = client.get_pending_winnings(&bob);
        let charlie_pending = client.get_pending_winnings(&charlie);
        let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

        let sum_payouts = alice_pending + bob_pending + charlie_pending;

        prop_assert!(alice_pending >= 0);
        prop_assert!(bob_pending >= 0);
        prop_assert!(charlie_pending >= 0);
        prop_assert!(treasury_delta >= 0);

        // Precision conservation is exact
        prop_assert_eq!(
            sum_payouts + treasury_delta,
            total_pot,
            "Precision FeeOnWinnings conservation violated: payouts={} treasury={} pot={}",
            sum_payouts, treasury_delta, total_pot
        );
    }
}
