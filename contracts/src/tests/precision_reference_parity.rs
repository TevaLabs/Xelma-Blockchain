// SPDX-License-Identifier: MIT
//! Differential parity harness: Precision closest-guess settlement vs the
//! reference model (Issue #803).
//!
//! For each of `N` deterministic seeds the harness builds a randomized
//! sequence of Precision rounds, drives them through the real contract and
//! through [`ref_precision_resolve`] / [`ReferenceModel`], and asserts that
//! both agree **exactly** on:
//!
//! - the winner set (closest revealed guess, exact ties share),
//! - every participant's recorded `UserRoundOutcome` (outcome type, stake,
//!   payout, recorded predicted price),
//! - every participant's cumulative pending winnings (across rounds),
//! - the protocol-fee treasury delta,
//! - the conservation identity `Σ payouts + fee == total pot`.
//!
//! The generator is biased toward the hard cases for closest-guess
//! selection: predictions are clustered within a few stroops of the final
//! price (so symmetric `±d` ties and multi-way ties are frequent), some
//! participants commit and never reveal (forfeit-to-pot), some rounds have
//! nobody revealing (full refund), and fee bps / fee model / payout policy
//! are re-drawn per round.
//!
//! # Knobs
//!
//! - `PRECISION_PARITY_SEEDS` — number of seeds (default 64).
//! - `PRECISION_PARITY_SEED`  — run exactly one seed (for reproducing a
//!   failure; the failing seed is printed in the panic message).

extern crate std;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, Bytes, BytesN, Env};
use std::format;
use std::string::String;
use std::vec::Vec;

use super::reference_model::{
    ref_precision_resolve, RefPrecisionEntry, RefPrecisionOutcome, ReferenceModel,
    FEE_MODEL_ON_POT, FEE_MODEL_ON_WINNINGS, PRECISION_POLICY_EQUAL,
    PRECISION_POLICY_STAKE_WEIGHTED,
};
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{
    DataKeyCore, DataKeyScoped, FeeModel, OraclePayload, UserOutcomeType, UserRoundOutcome,
};

const DEFAULT_SEEDS: u64 = 64;
const USERS: usize = 8;
const MAX_ROUNDS_PER_SEED: usize = 3;
/// Ledger offset from round start at which the reveal window is open
/// (bet window is 6 ledgers, run window 12 by default).
const REVEAL_OFFSET: u32 = 7;
const RESOLVE_OFFSET: u32 = 12;

#[derive(Clone, Debug)]
enum Kind {
    /// `place_precision_prediction` during the bet window.
    Direct(u128),
    /// `commit_prediction` then `reveal_prediction` in the reveal window.
    CommitReveal(u128),
    /// `commit_prediction` and never reveal.
    CommitOnly(u128),
}

#[derive(Clone, Debug)]
struct Participant {
    user_idx: usize,
    amount: i128,
    kind: Kind,
}

#[derive(Clone, Debug)]
struct RoundPlan {
    final_price: u128,
    fee_bps: Option<u32>,
    fee_model: u32,
    payout_policy: u32,
    participants: Vec<Participant>,
}

fn make_commitment(env: &Env, price: u128, salt: &BytesN<32>) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&price.to_xdr(env));
    preimage.append(&salt.clone().to_xdr(env));
    env.crypto().sha256(&preimage).into()
}

/// Salt that satisfies the on-chain minimum-entropy check.
fn make_salt(env: &Env, seed: u64, round: usize, user: usize) -> BytesN<32> {
    let mut rng = StdRng::seed_from_u64(seed ^ ((round as u64) << 32) ^ (user as u64 + 1));
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes);
    bytes[0] |= 0x80;
    bytes[31] = bytes[0] ^ 0x5A;
    BytesN::from_array(env, &bytes)
}

fn gen_round(rng: &mut StdRng) -> RoundPlan {
    let final_price: u128 = rng.gen_range(1_000u128..=50_000_000);
    let fee_bps = match rng.gen_range(0..7) {
        0 => None,
        1 => Some(0),
        2 => Some(1),
        3 => Some(37),
        4 => Some(250),
        5 => Some(999),
        _ => Some(1_000),
    };
    let fee_model = if rng.gen_bool(0.5) {
        FEE_MODEL_ON_POT
    } else {
        FEE_MODEL_ON_WINNINGS
    };
    let payout_policy = if rng.gen_bool(0.5) {
        PRECISION_POLICY_EQUAL
    } else {
        PRECISION_POLICY_STAKE_WEIGHTED
    };

    // Distinct users per round (one position per user per round).
    let n = rng.gen_range(1..=USERS);
    let mut idxs: Vec<usize> = (0..USERS).collect();
    for i in (1..idxs.len()).rev() {
        let j = rng.gen_range(0..=i);
        idxs.swap(i, j);
    }
    idxs.truncate(n);

    // Occasionally force an all-unrevealed round.
    let all_unrevealed = rng.gen_ratio(1, 10);
    // Spread controls how often ties occur: tiny spread ⇒ many ties.
    let spread: u128 = match rng.gen_range(0..3) {
        0 => 2,
        1 => 10,
        _ => 1_000,
    };

    let participants = idxs
        .into_iter()
        .map(|user_idx| {
            let offset = rng.gen_range(0..=spread);
            let price = if rng.gen_bool(0.5) {
                final_price + offset
            } else {
                final_price.saturating_sub(offset)
            };
            // Mix of tiny stakes (stress rounding) and large ones.
            let amount: i128 = if rng.gen_bool(0.3) {
                rng.gen_range(1..=17)
            } else {
                rng.gen_range(1..=50_0000000)
            };
            let kind = if all_unrevealed {
                Kind::CommitOnly(price)
            } else {
                match rng.gen_range(0..10) {
                    0..=4 => Kind::Direct(price),
                    5..=7 => Kind::CommitReveal(price),
                    _ => Kind::CommitOnly(price),
                }
            };
            Participant {
                user_idx,
                amount,
                kind,
            }
        })
        .collect();

    RoundPlan {
        final_price,
        fee_bps,
        fee_model,
        payout_policy,
        participants,
    }
}

fn outcome_to_ref(o: &UserOutcomeType) -> Option<RefPrecisionOutcome> {
    match o {
        UserOutcomeType::Win => Some(RefPrecisionOutcome::Win),
        UserOutcomeType::Loss => Some(RefPrecisionOutcome::Loss),
        UserOutcomeType::Refund => Some(RefPrecisionOutcome::Refund),
        _ => None,
    }
}

/// Scenario coverage accumulated across seeds, so the harness can prove it
/// actually exercised the hard closest-guess branches rather than passing
/// vacuously.
#[derive(Default, Debug)]
struct Coverage {
    rounds: u32,
    multi_winner_ties: u32,
    forfeits: u32,
    all_unrevealed_refunds: u32,
    fee_charged: u32,
    fee_on_winnings_zero_profit: u32,
    stake_weighted_rounds: u32,
    nonzero_remainder: u32,
}

/// Runs one seed end-to-end. Returns `Err(diagnostic)` on the first divergence.
fn run_seed(seed: u64, cov: &mut Coverage) -> Result<(), String> {
    let mut rng = StdRng::seed_from_u64(seed);

    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    let users: Vec<Address> = (0..USERS).map(|_| Address::generate(&env)).collect();
    let mut model = ReferenceModel::new();
    for u in &users {
        client.mint_initial(u);
        model.deposit(u, 1000_0000000);
    }

    let rounds = rng.gen_range(1..=MAX_ROUNDS_PER_SEED);
    let mut ledger: u32 = env.ledger().sequence();

    for r in 0..rounds {
        let plan = gen_round(&mut rng);
        let ctx =
            |msg: String| -> String { format!("seed={seed} round={r} plan={plan:#?}\n{msg}") };

        // Configuration for this round.
        env.as_contract(&contract_id, || match plan.fee_bps {
            Some(bps) => env
                .storage()
                .persistent()
                .set(&DataKeyCore::ProtocolFeeBps, &bps),
            None => env
                .storage()
                .persistent()
                .remove(&DataKeyCore::ProtocolFeeBps),
        });
        client.set_fee_model(&if plan.fee_model == FEE_MODEL_ON_WINNINGS {
            FeeModel::FeeOnWinnings
        } else {
            FeeModel::FeeOnPot
        });
        client.set_precision_payout_policy(&plan.payout_policy);
        model.set_fee_bps(plan.fee_bps);

        // Bet window.
        let start = ledger;
        client.create_round(&plan.final_price, &Some(1));
        let round_id = client.get_active_round().unwrap().round_id;

        let mut entries: Vec<RefPrecisionEntry> = Vec::new();
        for p in &plan.participants {
            let user = &users[p.user_idx];
            match p.kind {
                Kind::Direct(price) => {
                    client.place_precision_prediction(user, &p.amount, &price);
                }
                Kind::CommitReveal(price) | Kind::CommitOnly(price) => {
                    let salt = make_salt(&env, seed, r, p.user_idx);
                    client.commit_prediction(user, &make_commitment(&env, price, &salt), &p.amount);
                }
            }
            model.withdraw(user, p.amount);
            let revealed = match p.kind {
                Kind::Direct(price) | Kind::CommitReveal(price) => Some(price),
                Kind::CommitOnly(_) => None,
            };
            entries.push(RefPrecisionEntry {
                user: user.clone(),
                amount: p.amount,
                predicted_price: revealed,
            });
        }

        // Reveal window.
        ledger = start + REVEAL_OFFSET;
        env.ledger().with_mut(|li| li.sequence_number = ledger);
        for p in &plan.participants {
            if let Kind::CommitReveal(price) = p.kind {
                let salt = make_salt(&env, seed, r, p.user_idx);
                client.reveal_prediction(&users[p.user_idx], &price, &salt);
            }
        }

        // Resolve.
        ledger = start + RESOLVE_OFFSET;
        env.ledger().with_mut(|li| li.sequence_number = ledger);
        let treasury_before = client.get_protocol_fee_treasury();
        let round = client.get_active_round().unwrap();
        client.resolve_round(&OraclePayload {
            price: plan.final_price,
            timestamp: env.ledger().timestamp(),
            round_id: round.start_ledger,
            nonce: 1u64,
            network_id: env.ledger().network_id(),
            contract_addr: contract_id.clone(),
            confidence: None,
            attestation: None,
        });
        let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;

        let expected = ref_precision_resolve(
            &entries,
            plan.final_price,
            plan.fee_bps,
            plan.fee_model,
            plan.payout_policy,
        );
        model.apply_precision_settlement(&expected);

        cov.rounds += 1;
        if expected.winners.len() > 1 {
            cov.multi_winner_ties += 1;
        }
        if !expected.winners.is_empty() && entries.iter().any(|e| e.predicted_price.is_none()) {
            cov.forfeits += 1;
        }
        if expected.winners.is_empty() {
            cov.all_unrevealed_refunds += 1;
        }
        if expected.fee > 0 {
            cov.fee_charged += 1;
        }
        if plan.fee_model == FEE_MODEL_ON_WINNINGS
            && matches!(plan.fee_bps, Some(b) if b > 0)
            && !expected.winners.is_empty()
            && expected.fee == 0
        {
            cov.fee_on_winnings_zero_profit += 1;
        }
        if plan.payout_policy == PRECISION_POLICY_STAKE_WEIGHTED {
            cov.stake_weighted_rounds += 1;
        }
        if expected.winners.len() > 1 {
            let first = expected.winners[0].clone();
            let first_pay = expected
                .results
                .iter()
                .find(|x| x.user == first)
                .map(|x| x.payout)
                .unwrap_or(0);
            let others_max = expected
                .results
                .iter()
                .filter(|x| x.outcome == RefPrecisionOutcome::Win && x.user != first)
                .map(|x| x.payout)
                .max()
                .unwrap_or(0);
            if plan.payout_policy == PRECISION_POLICY_EQUAL && first_pay != others_max {
                cov.nonzero_remainder += 1;
            }
        }

        // Conservation (sanity on the reference itself).
        if expected.sum_payouts() + expected.fee != expected.total_pot {
            return Err(ctx(format!(
                "reference conservation broken: payouts={} fee={} pot={}",
                expected.sum_payouts(),
                expected.fee,
                expected.total_pot
            )));
        }

        // Fee parity.
        if treasury_delta != expected.fee {
            return Err(ctx(format!(
                "fee mismatch: contract treasury_delta={treasury_delta} reference fee={}",
                expected.fee
            )));
        }

        // Per-user outcome parity (winner set, payouts, recorded fields).
        let mut contract_winners: Vec<Address> = Vec::new();
        let mut contract_sum = 0i128;
        for exp in &expected.results {
            let rec: Option<UserRoundOutcome> = env.as_contract(&contract_id, || {
                env.storage()
                    .persistent()
                    .get(&DataKeyScoped::UserRoundOutcome(round_id, exp.user.clone()))
            });
            let rec = rec.ok_or_else(|| ctx(format!("missing outcome for {:?}", exp.user)))?;
            let kind = outcome_to_ref(&rec.outcome);
            if kind == Some(RefPrecisionOutcome::Win) {
                contract_winners.push(exp.user.clone());
            }
            contract_sum += rec.payout;
            if kind != Some(exp.outcome)
                || rec.payout != exp.payout
                || rec.stake != exp.stake
                || rec.predicted_price != exp.predicted_price
            {
                return Err(ctx(format!(
                    "outcome mismatch for {:?}:\n  contract  = {:?}\n  reference = {:?}",
                    exp.user, rec, exp
                )));
            }
        }
        if contract_winners != expected.winners {
            return Err(ctx(format!(
                "winner set mismatch: contract={contract_winners:?} reference={:?} (min_diff={:?})",
                expected.winners, expected.min_diff
            )));
        }
        if contract_sum + treasury_delta != expected.total_pot {
            return Err(ctx(format!(
                "contract conservation broken: payouts={contract_sum} fee={treasury_delta} pot={}",
                expected.total_pot
            )));
        }

        // Cumulative pending-winnings parity across rounds.
        for u in &users {
            let got = client.get_pending_winnings(u);
            let want = model.pending_winnings.get(u).copied().unwrap_or(0);
            if got != want {
                return Err(ctx(format!(
                    "pending winnings mismatch for {u:?}: contract={got} reference={want}"
                )));
            }
        }
        if !model.check_invariants().is_empty() {
            return Err(ctx(format!(
                "reference invariants violated: {:?}",
                model.check_invariants()
            )));
        }

        // Next round starts strictly after this one.
        ledger += 1;
        env.ledger().with_mut(|li| li.sequence_number = ledger);
    }

    Ok(())
}

/// Main N-seed differential harness (Issue #803).
#[test]
fn precision_closest_winner_reference_parity_n_seeds() {
    let seeds: Vec<u64> = match std::env::var("PRECISION_PARITY_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        Some(one) => std::vec![one],
        None => {
            let n = std::env::var("PRECISION_PARITY_SEEDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_SEEDS);
            (0..n).map(|i| 0x803_0000 + i).collect()
        }
    };

    let full_run = seeds.len() >= 32;
    let mut cov = Coverage::default();
    for seed in seeds {
        if let Err(diag) = run_seed(seed, &mut cov) {
            panic!(
                "\n========== PRECISION REFERENCE PARITY MISMATCH ==========\n\
                 Reproduce with: PRECISION_PARITY_SEED={seed} cargo test \
                 precision_closest_winner_reference_parity\n{diag}\n\
                 ========================================================="
            );
        }
    }

    // Guard against a vacuous pass: a full run must hit every hard branch.
    std::println!("precision reference parity coverage: {cov:?}");
    if full_run {
        assert!(
            cov.multi_winner_ties > 0,
            "no multi-winner ties exercised: {cov:?}"
        );
        assert!(
            cov.forfeits > 0,
            "no forfeit-to-pot rounds exercised: {cov:?}"
        );
        assert!(
            cov.all_unrevealed_refunds > 0,
            "no all-unrevealed refunds: {cov:?}"
        );
        assert!(cov.fee_charged > 0, "no fee-charging rounds: {cov:?}");
        assert!(
            cov.stake_weighted_rounds > 0,
            "no stake-weighted rounds: {cov:?}"
        );
        assert!(
            cov.nonzero_remainder > 0,
            "no Equal-split remainder exercised: {cov:?}"
        );
    }
}

/// Deterministic regression vector: symmetric ±d tie plus an exact hit by
/// an unrevealed commitment. The unrevealed exact hit must *not* win; the
/// two revealed ±d guesses share the pot with the remainder going to the
/// lower address.
#[test]
fn precision_parity_unrevealed_exact_hit_does_not_beat_revealed_tie() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    client.initialize(&Address::generate(&env), &Address::generate(&env));
    client.update_oracle_heartbeat(&0u32);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    for u in [&a, &b, &c] {
        client.mint_initial(u);
    }
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::ProtocolFeeBps, &1_000u32)
    });

    client.create_round(&1_000u128, &Some(1));
    client.place_precision_prediction(&a, &33, &1_003u128);
    client.place_precision_prediction(&b, &33, &997u128);
    let salt = make_salt(&env, 803, 0, 2);
    client.commit_prediction(&c, &make_commitment(&env, 1_000u128, &salt), &35);

    env.ledger()
        .with_mut(|li| li.sequence_number = RESOLVE_OFFSET);
    let round = client.get_active_round().unwrap();
    client.resolve_round(&OraclePayload {
        price: 1_000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    let entries = [
        RefPrecisionEntry {
            user: a.clone(),
            amount: 33,
            predicted_price: Some(1_003),
        },
        RefPrecisionEntry {
            user: b.clone(),
            amount: 33,
            predicted_price: Some(997),
        },
        RefPrecisionEntry {
            user: c.clone(),
            amount: 35,
            predicted_price: None,
        },
    ];
    let expected = ref_precision_resolve(&entries, 1_000, Some(1_000), FEE_MODEL_ON_POT, 0);
    assert_eq!(expected.min_diff, Some(3));
    assert_eq!(expected.fee, 10);
    assert_eq!(client.get_protocol_fee_treasury(), expected.fee);
    for r in &expected.results {
        assert_eq!(
            client.get_pending_winnings(&r.user),
            r.payout,
            "user {:?}",
            r.user
        );
    }
    assert_eq!(client.get_pending_winnings(&c), 0);
    assert_eq!(
        client.get_pending_winnings(&a) + client.get_pending_winnings(&b),
        91
    );
}
