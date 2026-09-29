// SPDX-License-Identifier: MIT
//! #534 — optional Precision sealed-bid commit fee.
//!
//! A Precision commitment is cheap to place and its stake is forfeit to the
//! pot if it is never revealed, so commit-only spam is nearly free for an
//! attacker. This fee makes bulk commitment spam economically irrational.
//!
//! Design constraints this suite pins down:
//!
//! * **Backward compatible default.** With no configuration the fee is `0`
//!   and `commit_prediction` follows the pre-#534 code path exactly. Every
//!   existing deployment already has the absent key, so the fee is opt-in.
//! * **The fee is never refunded.** Not on the all-unrevealed refund path,
//!   not on round cancellation. That is what makes it a deterrent rather
//!   than a rounding error.
//! * **The fee is not part of the pot.** It is pure protocol revenue, so
//!   settlement maths and pot conservation are untouched by it.
//! * **Conserve value.** A user's balance drop is always exactly
//!   `stake + fee`, and the fee ledger (treasury + insurance) gains exactly
//!   `fee`. No stroop appears or vanishes.
//! * **A rejected commit changes nothing** — no debit, no commitment, no fee.

#![cfg(test)]
extern crate std;

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{
    BetSide, DataKeyScoped, OraclePayload, PrecisionCommitment, CANCEL_REASON_GENERIC,
};
use alloc::vec::Vec;
use soroban_sdk::symbol_short;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, Bytes, BytesN, Env, IntoVal, TryIntoVal, Val, Vec as SorobanVec};

const BPS: i128 = 10_000;
const START_PRICE: u128 = 10_000;

// The setter reuses `InvalidProtocolFeeBps` (code 51) so the commit-fee
// surface shares one error vocabulary with the other bps knobs. Asserted by
// value below via the getter rather than by code, so the tests stay valid if
// the enum is ever renumbered.

// ─── Harness ─────────────────────────────────────────────────────────────────

struct Harness<'a> {
    env: Env,
    client: VirtualTokenContractClient<'a>,
    contract_id: Address,
}

impl<'a> Harness<'a> {
    fn balance(&self, user: &Address) -> i128 {
        self.client.balance(user)
    }

    /// Total fee ledger: ops treasury + segregated insurance fund.
    fn fee_ledger(&self) -> i128 {
        self.client.get_protocol_fee_treasury() + self.client.get_insurance_fund_balance()
    }

    /// The active round's end ledger — the first point at which it resolves.
    fn end_ledger(&self) -> u32 {
        self.client
            .get_active_round()
            .expect("active round")
            .end_ledger
    }

    /// Moves to the reveal window (`bet_end_ledger .. end_ledger`), which is
    /// where a sealed bid must be opened.
    fn enter_reveal_window(&self) {
        let round = self.client.get_active_round().expect("active round");
        let target = round.bet_end_ledger;
        assert!(
            target < round.end_ledger,
            "the reveal window must be non-empty for this test"
        );
        self.env.ledger().with_mut(|li| li.sequence_number = target);
    }

    /// Reveals a sealed bid. Must be called inside the reveal window.
    fn reveal(&self, user: &Address, price: u128, seed: u8) {
        self.client
            .reveal_prediction(user, &price, &commit_salt(&self.env, seed));
    }

    /// Advances to the end ledger and resolves at `final_price`.
    fn resolve(&self, final_price: u128) {
        let end = self.end_ledger();
        self.env.ledger().with_mut(|li| li.sequence_number = end);
        let round = self.client.get_active_round().expect("active round");
        self.client.resolve_round(&OraclePayload {
            price: final_price,
            timestamp: self.env.ledger().timestamp(),
            round_id: round.start_ledger,
            nonce: 1u64,
            network_id: self.env.ledger().network_id(),
            contract_addr: self.contract_id.clone(),
            confidence: None,
            attestation: None,
        });
    }
}

fn harness(env: &Env) -> Harness<'_> {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    // Issue #264: settlement is blocked without a fresh, healthy heartbeat.
    client.update_oracle_heartbeat(&0u32);
    Harness {
        env: env.clone(),
        client,
        contract_id,
    }
}

/// Directly seeds a balance. `mint_initial` issues a fixed small grant, which
/// is too small for the round sizes the fee is meant to gate.
fn set_balance(h: &Harness<'_>, user: &Address, amount: i128) {
    h.env.as_contract(&h.contract_id, || {
        h.env
            .storage()
            .persistent()
            .set(&DataKeyScoped::Balance(user.clone()), &amount);
    });
}

fn commit_hash(env: &Env, price: u128, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = seed.wrapping_add(i as u8).wrapping_mul(31).wrapping_add(7);
    }
    bytes[0] = seed | 0x80;
    bytes[31] = seed ^ 0x5F;
    let salt = BytesN::from_array(env, &bytes);
    let mut preimage = Bytes::new(env);
    preimage.append(&price.to_xdr(env));
    preimage.append(&salt.to_xdr(env));
    env.crypto().sha256(&preimage).into()
}

fn open_precision_round(h: &Harness<'_>) {
    h.client.create_round(&START_PRICE, &Some(1));
}

fn commitment_for(h: &Harness<'_>, round_id: u64, user: &Address) -> Option<PrecisionCommitment> {
    h.env.as_contract(&h.contract_id, || {
        h.env.storage().persistent().get::<_, PrecisionCommitment>(
            &DataKeyScoped::PrecisionCommitment(round_id, user.clone()),
        )
    })
}

/// Decodes the most recent `(topic0, topic1)` event into its raw fields.
///
/// Field-wise decoding: `try_into_val` on a mismatched tuple shape panics
/// inside the SDK rather than returning `Err`, so the payload is read as a
/// `soroban_sdk::Vec<Val>` and each field asserted individually.
fn last_event_fields(
    h: &Harness<'_>,
    topic0: soroban_sdk::Symbol,
    topic1: soroban_sdk::Symbol,
) -> SorobanVec<Val> {
    let env = &h.env;
    let events = env.events().all();
    let (_c, topics, data) = events
        .iter()
        .rev()
        .find(|(_c, topics, _d)| {
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(env) == Ok(topic0.clone())
                && topics.get(1).unwrap().try_into_val(env) == Ok(topic1.clone())
        })
        .unwrap_or_else(|| panic!("expected a ({topic0:?}, {topic1:?}) event"));
    data.into_val(env)
}

fn has_event(h: &Harness<'_>, topic0: soroban_sdk::Symbol, topic1: soroban_sdk::Symbol) -> bool {
    let env = &h.env;
    env.events().all().iter().any(|(_c, topics, _d)| {
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(env) == Ok(topic0.clone())
            && topics.get(1).unwrap().try_into_val(env) == Ok(topic1.clone())
    })
}

fn f_i128(env: &Env, f: &SorobanVec<Val>, i: u32) -> i128 {
    f.get(i).unwrap().try_into_val(env).unwrap()
}

fn f_u128(env: &Env, f: &SorobanVec<Val>, i: u32) -> u128 {
    f.get(i).unwrap().try_into_val(env).unwrap()
}

fn f_u32(env: &Env, f: &SorobanVec<Val>, i: u32) -> u32 {
    f.get(i).unwrap().try_into_val(env).unwrap()
}

fn f_u64(env: &Env, f: &SorobanVec<Val>, i: u32) -> u64 {
    f.get(i).unwrap().try_into_val(env).unwrap()
}

fn f_addr(env: &Env, f: &SorobanVec<Val>, i: u32) -> Address {
    f.get(i).unwrap().try_into_val(env).unwrap()
}

// ─── Backward compatible default ─────────────────────────────────────────────

/// With no configuration the fee is absent and commitments are free, so the
/// default is indistinguishable from pre-#534 behaviour.
#[test]
fn default_commit_fee_is_disabled_and_commits_are_free() {
    let env = Env::default();
    let h = harness(&env);

    assert_eq!(
        h.client.get_precision_commit_fee_bps(),
        None,
        "the commit fee must be off by default"
    );

    let alice = Address::generate(&env);
    set_balance(&h, &alice, 10_000);
    open_precision_round(&h);

    let amount = 4_000i128;
    let balance_before = h.balance(&alice);
    let ledger_before = h.fee_ledger();

    h.client
        .commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &amount);

    assert_eq!(
        h.balance(&alice),
        balance_before - amount,
        "the default must debit only the stake"
    );
    assert_eq!(h.fee_ledger(), ledger_before, "default moves no fee");

    // The stored commitment shape is unchanged: no `fee_paid` field was
    // introduced, so pre-#534 commitments stay readable.
    let round_id = h.client.get_active_round().unwrap().round_id;
    let c = commitment_for(&h, round_id, &alice).expect("commitment stored");
    assert_eq!(c.amount, amount);
    assert!(!c.revealed);
}

/// A 1 bp fee on a 3-stroop commitment floors to zero. The fee is a
/// deterrent, not a revenue line, so rounding down to zero is safe — and no
/// fee event is emitted when there is nothing to charge.
#[test]
fn sub_stroop_commit_fee_rounds_to_zero_without_events() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(1));

    let alice = Address::generate(&env);
    set_balance(&h, &alice, 1_000);
    open_precision_round(&h);

    let balance_before = h.balance(&alice);
    h.client
        .commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &3i128);

    assert_eq!(h.balance(&alice), balance_before - 3);
    assert_eq!(h.client.get_protocol_fee_treasury(), 0, "no fee collected");
    assert!(
        !has_event(&h, symbol_short!("commit"), symbol_short!("fee_chrg")),
        "no commit-fee event when the fee rounds to zero"
    );
}

// ─── Configurable fee ────────────────────────────────────────────────────────

/// The fee is admin-configurable, applies immediately (no timelock — an
/// operator must be able to raise it mid-attack), and is charged per commit
/// at the exact configured rate.
#[test]
fn commit_fee_is_configurable_and_applies_immediately() {
    let env = Env::default();
    let h = harness(&env);

    for bps in [1u32, 25, 100, 500, 1_000] {
        h.client.set_precision_commit_fee_bps(&Some(bps));
        assert_eq!(
            h.client.get_precision_commit_fee_bps(),
            Some(bps),
            "{bps} bps must read back immediately, with no timelock"
        );
    }

    for bps in [1u32, 25, 100, 500, 1_000] {
        h.client.set_precision_commit_fee_bps(&Some(bps));
        assert_eq!(
            h.client.get_precision_commit_fee_bps(),
            Some(bps),
            "{bps} bps must read back immediately, with no timelock"
        );
    }

    // A fresh Env per rate: cancelling a round and re-creating one at the same
    // ledger sequence reuses `start_ledger` (#93), so the rounds cannot share
    // a single environment.
    for bps in [1u32, 25, 100, 500, 1_000] {
        let env = Env::default();
        let h = harness(&env);
        h.client.set_precision_commit_fee_bps(&Some(bps));

        let alice = Address::generate(&env);
        set_balance(&h, &alice, 100_000);
        open_precision_round(&h);

        let amount = 10_000i128;
        let expected_fee = amount * bps as i128 / BPS;
        let balance_before = h.balance(&alice);
        let ledger_before = h.fee_ledger();

        h.client
            .commit_prediction(&alice, &commit_hash(&env, 10_050, bps as u8), &amount);

        assert_eq!(
            h.balance(&alice),
            balance_before - amount - expected_fee,
            "{bps} bps: balance drops by stake + fee"
        );
        // Conservation: the fee moves out of the user balance into the fee
        // ledger. It is never minted or burned.
        let out = balance_before - h.balance(&alice) - amount;
        assert_eq!(
            out, expected_fee,
            "{bps} bps: the fee is exactly as configured"
        );
        assert_eq!(
            h.fee_ledger() - ledger_before,
            out,
            "{bps} bps: the fee must be fully accounted for, not created or destroyed"
        );
    }
}

/// Disabling the fee restores free commitments.
#[test]
fn commit_fee_can_be_disabled() {
    let env = Env::default();
    let h = harness(&env);

    h.client.set_precision_commit_fee_bps(&Some(500));
    assert_eq!(h.client.get_precision_commit_fee_bps(), Some(500));

    h.client.set_precision_commit_fee_bps(&None);
    assert_eq!(h.client.get_precision_commit_fee_bps(), None);

    let alice = Address::generate(&env);
    set_balance(&h, &alice, 10_000);
    open_precision_round(&h);
    let balance_before = h.balance(&alice);
    h.client
        .commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &2_000);

    assert_eq!(h.balance(&alice), balance_before - 2_000);
    assert_eq!(h.fee_ledger(), 0);
}

/// `Some(0)` is rejected so "configured but zero" is never reachable:
/// disabling always means `None`. `MAX_COMMIT_FEE_BPS` is the upper bound.
#[test]
fn commit_fee_rejects_out_of_range_rates() {
    let env = Env::default();
    let h = harness(&env);

    for bad in [0u32, 1_001, u32::MAX] {
        assert!(
            h.client
                .try_set_precision_commit_fee_bps(&Some(bad))
                .is_err(),
            "{bad} bps must be rejected (InvalidProtocolFeeBps)"
        );
    }
    // A rejected setter must not partially apply.
    assert_eq!(h.client.get_precision_commit_fee_bps(), None);
    // The cap itself is accepted.
    assert!(h
        .client
        .try_set_precision_commit_fee_bps(&Some(1_000))
        .is_ok());
    assert_eq!(h.client.get_precision_commit_fee_bps(), Some(1_000));
}

/// Only the admin can change the fee. With no authorised invocation
/// supplied, `admin.require_auth()` cannot be satisfied and the call fails,
/// leaving the fee disabled.
#[test]
fn commit_fee_can_only_be_set_by_admin() {
    let env = Env::default();
    let h = harness(&env);

    env.set_auths(&[]);
    assert!(
        h.client
            .try_set_precision_commit_fee_bps(&Some(100))
            .is_err(),
        "a non-admin caller must not be able to set the commit fee"
    );
    assert_eq!(
        h.client.get_precision_commit_fee_bps(),
        None,
        "a rejected setter must not change the fee"
    );
}

// ─── Treasury + events ───────────────────────────────────────────────────────

/// Both the round-level `protocol::fee_coll` and the per-user
/// `commit::fee_charged` events are emitted with correct payloads.
#[test]
fn commit_fee_emits_treasury_and_per_user_events() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(100)); // 1%

    let alice = Address::generate(&env);
    set_balance(&h, &alice, 100_000);
    open_precision_round(&h);

    let amount = 5_000i128;
    let expected_fee = 50i128;
    // Read the round id first: `env.events().all()` is scoped to the most
    // recent top-level invocation, so any later client call would clear the
    // commit's events before they are asserted.
    let round_id = h.client.get_active_round().unwrap().round_id;
    h.client
        .commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &amount);

    // Per-user attribution: (round_id, user, stake, fee)
    let f = last_event_fields(&h, symbol_short!("commit"), symbol_short!("fee_chrg"));
    assert_eq!(f_u64(&env, &f, 0), round_id, "round id");
    assert_eq!(f_addr(&env, &f, 1), alice, "committing user");
    assert_eq!(f_i128(&env, &f, 2), amount, "stake");
    assert_eq!(f_i128(&env, &f, 3), expected_fee, "fee charged");

    // Round-level accounting: (round_id, fee, treasury_after, bps, fee_model)
    let f = last_event_fields(&h, symbol_short!("protocol"), symbol_short!("fee_coll"));
    assert_eq!(f_u64(&env, &f, 0), round_id, "round id");
    assert_eq!(f_i128(&env, &f, 1), expected_fee, "fee collected");
    assert_eq!(f_i128(&env, &f, 2), expected_fee, "treasury after");
    assert_eq!(f_u32(&env, &f, 3), 100, "bps recorded");
    assert_eq!(f_u32(&env, &f, 4), 0, "default FeeOnPot recorded");
}

/// Setting the fee emits a config event so operators can audit the change.
#[test]
fn setting_commit_fee_emits_config_event() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(250));
    let f = last_event_fields(&h, symbol_short!("config"), symbol_short!("pc_fee"));
    assert_eq!(f_u32(&env, &f, 0), 250, "configured bps");
}

/// The fee routes through the standard fee accounting, so the insurance split
/// applies exactly as it does for settlement fees.
#[test]
fn commit_fee_respects_the_insurance_split() {
    let env = Env::default();
    let h = harness(&env);

    // A non-default split so the assertion actually bites.
    h.client.set_insurance_split_bps(&500); // 5% to insurance
    h.client.set_precision_commit_fee_bps(&Some(1_000)); // 10% of stake

    let alice = Address::generate(&env);
    set_balance(&h, &alice, 100_000);
    open_precision_round(&h);

    let fee = 1_000i128; // 10% of a 10,000 stake
    h.client
        .commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &10_000);

    let insurance = h.client.get_insurance_fund_balance();
    let treasury = h.client.get_protocol_fee_treasury();
    assert_eq!(insurance, 50, "5% of the commit fee goes to insurance");
    assert_eq!(treasury, 950, "the remainder goes to ops");
    assert_eq!(insurance + treasury, fee, "the split must conserve the fee");
}

// ─── Conservation ────────────────────────────────────────────────────────────

/// The fee is not added to the round pot, so pot conservation at settlement
/// is exactly the pre-#534 identity.
///
/// Alice commits *and* reveals, so she wins the pot. Bob commits and never
/// reveals, which is exactly the spam pattern the fee is meant to deter: his
/// stake is forfeited, but he has already paid the fee, and that fee is not
/// part of what Alice wins.
#[test]
fn commit_fee_is_not_part_of_the_pot() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(1_000)); // 10%

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    set_balance(&h, &alice, 100_000);
    set_balance(&h, &bob, 100_000);
    open_precision_round(&h);

    let a_stake = 1_000i128;
    let b_stake = 500i128;
    let a_fee = a_stake * 1_000 / BPS;
    let b_fee = b_stake * 1_000 / BPS;

    let alice_balance_before = h.balance(&alice);
    let bob_balance_before = h.balance(&bob);
    let ledger_before = h.fee_ledger();

    h.client
        .commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &a_stake);
    h.client
        .commit_prediction(&bob, &commit_hash(&env, 11_000, 2), &b_stake);
    // Alice reveals inside the reveal window; Bob never does.
    h.enter_reveal_window();
    h.reveal(&alice, 10_050, 1);

    h.resolve(10_050);

    // The winner takes the whole pot; neither commit fee entered it.
    let alice_payout = h.client.get_pending_winnings(&alice);
    assert_eq!(
        alice_payout,
        a_stake + b_stake,
        "winner takes the pot, fee excluded"
    );
    assert_eq!(
        h.client.get_pending_winnings(&bob),
        0,
        "Bob forfeits, having not revealed"
    );

    // Both fees are protocol revenue, collected at commit time and untouched
    // by settlement.
    assert_eq!(
        h.fee_ledger() - ledger_before,
        a_fee + b_fee,
        "both commit fees are protocol revenue"
    );

    // Conservation, per participant. Payouts are credited to *pending
    // winnings* and claimed separately, so the liquid balance drop is exactly
    // stake + fee; the payout is a credit, not a rebate of that debit.
    assert_eq!(
        alice_balance_before - h.balance(&alice),
        a_stake + a_fee,
        "Alice's liquid balance drops by stake + fee exactly"
    );
    assert_eq!(
        bob_balance_before - h.balance(&bob),
        b_stake + b_fee,
        "Bob's liquid balance drops by stake + fee exactly"
    );
    // And the claimable amounts are what the settlement maths predicted.
    assert_eq!(h.client.get_pending_winnings(&alice), a_stake + b_stake);
    assert_eq!(h.client.get_pending_winnings(&bob), 0);
}

/// A direct (non-commit) precision prediction is never charged the commit fee:
/// the fee targets sealed-bid commitment spam specifically.
#[test]
fn direct_precision_predictions_are_not_charged() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(1_000));

    let alice = Address::generate(&env);
    set_balance(&h, &alice, 100_000);
    open_precision_round(&h);

    let before = h.balance(&alice);
    h.client.place_precision_prediction(&alice, &500, &10_050);
    assert_eq!(h.balance(&alice), before - 500);
    assert_eq!(h.fee_ledger(), 0, "no commit, no fee");
}

/// The fee is **not** refunded on the all-unrevealed path. That is the whole
/// deterrent: an attacker who commits and never reveals still pays.
#[test]
fn commit_fee_is_not_refunded_on_all_unrevealed() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(500)); // 5%

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    set_balance(&h, &alice, 100_000);
    set_balance(&h, &bob, 100_000);
    open_precision_round(&h);

    let a_stake = 1_000i128;
    let b_stake = 800i128;
    let a_fee = a_stake * 500 / BPS;
    let b_fee = b_stake * 500 / BPS;

    let alice_balance_before = h.balance(&alice);
    let ledger_before = h.fee_ledger();
    h.client
        .commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &a_stake);
    h.client
        .commit_prediction(&bob, &commit_hash(&env, 10_050, 2), &b_stake);
    let ledger_after_commits = h.fee_ledger();
    assert_eq!(
        ledger_after_commits - ledger_before,
        a_fee + b_fee,
        "both fees were collected at commit time"
    );

    h.resolve(10_050); // nobody reveals

    // The stake is refunded in full...
    assert_eq!(
        h.client.get_pending_winnings(&alice),
        a_stake,
        "the stake is refunded even though the fee is not"
    );
    assert_eq!(
        h.client.get_pending_winnings(&bob),
        b_stake,
        "Bob's stake is refunded too"
    );
    // ...but the fee is not clawed back.
    assert_eq!(
        h.fee_ledger(),
        ledger_after_commits,
        "the commit fees survive the all-unrevealed refund"
    );
    assert_eq!(
        alice_balance_before - h.balance(&alice),
        a_stake + a_fee,
        "Alice paid stake + fee in total"
    );
}

/// The fee is **not** refunded on round cancellation either.
#[test]
fn commit_fee_is_not_refunded_on_cancellation() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(500));

    let alice = Address::generate(&env);
    set_balance(&h, &alice, 100_000);
    open_precision_round(&h);

    let stake = 1_000i128;
    let fee = stake * 500 / BPS;
    let balance_before = h.balance(&alice);
    let ledger_before = h.fee_ledger();
    h.client
        .commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &stake);
    let ledger_after_commits = h.fee_ledger();
    assert_eq!(
        ledger_after_commits - ledger_before,
        fee,
        "the fee was collected at commit time"
    );

    h.client.cancel_round(&CANCEL_REASON_GENERIC);

    assert_eq!(
        h.client.get_pending_winnings(&alice),
        stake,
        "cancellation refunds the stake"
    );
    assert_eq!(
        h.fee_ledger(),
        ledger_after_commits,
        "cancellation does not refund the commit fee"
    );
    assert_eq!(
        balance_before - h.balance(&alice),
        stake + fee,
        "Alice paid stake + fee in total"
    );
}

// ─── Insufficient balance ────────────────────────────────────────────────────

/// A user who can afford the stake but not the stake + fee is rejected, and
/// **no** state changes: no debit, no commitment, no fee. This is the check
/// that stops the fee from silently confiscating a stake.
#[test]
fn insufficient_balance_for_stake_plus_fee_changes_nothing() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(1_000)); // 10%

    let alice = Address::generate(&env);
    set_balance(&h, &alice, 1_000); // exactly the stake, no room for the fee
    open_precision_round(&h);

    let round_id = h.client.get_active_round().unwrap().round_id;
    let balance_before = h.balance(&alice);
    let ledger_before = h.fee_ledger();

    // 1,000 stake needs 1,000 + 100.
    assert!(h
        .client
        .try_commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &1_000i128)
        .is_err());

    assert_eq!(h.balance(&alice), balance_before, "no debit on rejection");
    assert_eq!(h.fee_ledger(), ledger_before, "no fee on rejection");
    assert!(
        commitment_for(&h, round_id, &alice).is_none(),
        "a rejected commit must not create a commitment"
    );
    assert_eq!(
        h.client.get_user_stats(&alice).total_wins,
        0,
        "a rejected commit must not touch stats"
    );
}

/// The balance boundary is exact: one stroop short fails, one stroop more
/// succeeds with the identical commit.
#[test]
fn balance_boundary_is_exact() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(1_000)); // 10%

    let alice = Address::generate(&env);
    let stake = 1_000i128;
    let fee = 100i128;

    // Exactly one stroop short of stake + fee.
    set_balance(&h, &alice, stake + fee - 1);
    open_precision_round(&h);
    assert!(
        h.client
            .try_commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &stake)
            .is_err(),
        "must reject when the fee cannot be covered"
    );

    // Exactly stake + fee.
    set_balance(&h, &alice, stake + fee);
    h.client
        .commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &stake);

    assert_eq!(h.balance(&alice), 0, "the balance is exactly consumed");
    assert_eq!(h.client.get_protocol_fee_treasury(), fee);
}

// ─── Round integrity ─────────────────────────────────────────────────────────

/// The fee must not weaken the existing commit guards: zero hashes, non-positive
/// amounts, duplicate commits and wrong-mode commits are all still rejected —
/// and a rejected commit never charges a second fee.
#[test]
fn commit_guards_still_apply_with_fee_enabled() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(500));

    let alice = Address::generate(&env);
    set_balance(&h, &alice, 100_000);
    open_precision_round(&h);

    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    assert!(h
        .client
        .try_commit_prediction(&alice, &zero_hash, &1_000i128)
        .is_err());
    assert!(h
        .client
        .try_commit_prediction(&alice, &commit_hash(&env, 1, 1), &0i128)
        .is_err());
    assert!(h
        .client
        .try_commit_prediction(&alice, &commit_hash(&env, 1, 1), &-5i128)
        .is_err());

    h.client
        .commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &1_000);
    let ledger_before = h.fee_ledger();
    assert!(h
        .client
        .try_commit_prediction(&alice, &commit_hash(&env, 10_050, 2), &1_000)
        .is_err());
    assert_eq!(
        h.fee_ledger(),
        ledger_before,
        "a rejected commit charges nothing"
    );
}

/// Up/Down bets are entirely unaffected by the commit fee.
#[test]
fn updown_round_is_unaffected_by_the_commit_fee() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(1_000));

    let alice = Address::generate(&env);
    set_balance(&h, &alice, 100_000);
    h.client.create_round(&START_PRICE, &Some(0));

    let before = h.balance(&alice);
    h.client.place_bet(&alice, &1_000i128, &BetSide::Up);
    assert_eq!(h.balance(&alice), before - 1_000);
    assert_eq!(
        h.fee_ledger(),
        0,
        "UpDown bets are never charged the commit fee"
    );

    // A sealed-bid commitment in an UpDown round is still rejected, and the
    // rejection does not charge a fee.
    let bob = Address::generate(&env);
    set_balance(&h, &bob, 100_000);
    assert!(
        h.client
            .try_commit_prediction(&bob, &commit_hash(&env, 10_050, 3), &1_000i128)
            .is_err(),
        "a commitment must be rejected in an UpDown round"
    );
    assert_eq!(h.fee_ledger(), 0, "a rejected commit charges nothing");
}

/// A revealed commitment settles exactly as before: the reveal refund and the
/// fee are independent, so enabling the fee never changes the payout.
#[test]
fn revealing_still_settles_normally_with_fee_enabled() {
    let env = Env::default();
    let h = harness(&env);
    h.client.set_precision_commit_fee_bps(&Some(1_000));

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    set_balance(&h, &alice, 100_000);
    set_balance(&h, &bob, 100_000);
    open_precision_round(&h);

    let a_stake = 1_000i128;
    let a_fee = a_stake * 1_000 / BPS;
    h.client
        .commit_prediction(&alice, &commit_hash(&env, 10_050, 1), &a_stake);
    h.client.place_precision_prediction(&bob, &500, &40_000);

    let ledger_before = h.fee_ledger();
    h.enter_reveal_window();
    h.reveal(&alice, 10_050, 1);
    h.resolve(10_050);

    // Alice wins the 1,500 pot; the 100 fee never entered it.
    assert_eq!(h.client.get_pending_winnings(&alice), 1_500);
    assert_eq!(h.client.get_pending_winnings(&bob), 0);
    assert_eq!(
        h.fee_ledger() - ledger_before,
        0,
        "no extra fee at settlement"
    );
    assert_eq!(
        h.client.get_protocol_fee_treasury(),
        a_fee,
        "only the commit fee"
    );
}

fn commit_salt(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = seed.wrapping_add(i as u8).wrapping_mul(31).wrapping_add(7);
    }
    bytes[0] = seed | 0x80;
    bytes[31] = seed ^ 0x5F;
    BytesN::from_array(env, &bytes)
}
