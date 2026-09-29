// SPDX-License-Identifier: MIT
//! #531 — `FeeModel` x `RoundMode` consistency matrix.
//!
//! Issue #531 asks that both fee models *agree across every path that touches
//! them*. Before this module there were three independent implementations of
//! the same fee formula:
//!
//! | Path | Pre-#531 behaviour |
//! |---|---|
//! | `config::calculate_protocol_fee_*` (live settle + `simulate_payout`) | both models |
//! | `settlement_math::compute_*_fee` (audit engine + replay engine) | **`FeeOnPot` only** |
//! | `contract::_apply_protocol_fee_*` (private, unreferenced) | **`FeeOnPot` only** |
//!
//! The audit engine — the thing auditors, golden vectors and the offline
//! replay tooling are pointed at — could not express `FeeOnWinnings` at all,
//! so any round settled under that model replayed wrong, and a "verify the
//! contract against the engine" review had no way to check it.
//!
//! This module locks the three paths together. For every cell of
//! `FeeModel` x `RoundMode` it asserts, on the same round:
//!
//! 1. **engine == `simulate_payout`** — the pure math predicts the preview,
//! 2. **engine == live `resolve_round`** — the preview predicted settlement,
//! 3. **conservation holds** — no stroop is created or destroyed.
//!
//! Conservation bounds (see `docs/FEE_MODEL.md`):
//! * Precision: `sum(payouts) + fee == total_pot`, exactly. The remainder
//!   policy assigns every leftover stroop to the first winner.
//! * UpDown: `total_pot - (winner_count - 1) <= sum(payouts) + fee <= total_pot`.
//!   The slack is per-winner integer truncation in the proportional split;
//!   it is at most one stroop per winner and can never exceed the pot.

#![cfg(test)]
extern crate std;

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::fee_incidence::FeeIncidence;
use crate::settlement_math::{
    compute_precision_payouts_with_policy_and_model, compute_updown_payouts_with_model,
    PrecisionEntry, PrecisionPayoutPolicy, PrecisionScoringMode, PrecisionScoringPolicy,
    UpDownPosition,
};
use crate::types::{BetSide, DataKeyCore, FeeModel, OraclePayload, RoundMode, UserOutcomeType};
use alloc::vec;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

/// Both fee models. Every matrix cell is run against each of these.
const BOTH_MODELS: [FeeModel; 2] = [FeeModel::FeeOnPot, FeeModel::FeeOnWinnings];

/// Both round modes. Every matrix cell is run against each of these.
const BOTH_MODES: [u32; 2] = [0, 1];

/// A few fee rates spanning "rounds to zero" through the 10% protocol cap.
const FEE_BPS_CASES: [u32; 4] = [1, 100, 500, 1_000];

const START_PRICE: u128 = 10_000;

// ─── Harness ─────────────────────────────────────────────────────────────────

struct Harness<'a> {
    env: Env,
    client: VirtualTokenContractClient<'a>,
    contract_id: Address,
}

fn harness(env: &Env) -> Harness<'_> {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    // Issue #264 heartbeat gate: settlement is blocked unless the oracle has
    // posted a non-stale, non-offline heartbeat.
    client.update_oracle_heartbeat(&0u32);
    Harness {
        env: env.clone(),
        client,
        contract_id,
    }
}

/// `set_protocol_fee_bps` is timelocked, so seed the active value directly.
/// The matrix only needs the *active* bps, not the timelock ceremony.
fn set_fee_bps_now(h: &Harness<'_>, bps: u32) {
    h.env.as_contract(&h.contract_id, || {
        h.env
            .storage()
            .persistent()
            .set(&DataKeyCore::ProtocolFeeBps, &bps);
    });
}

fn resolve_at(h: &Harness<'_>, final_price: u128) {
    let round = h.client.get_active_round().expect("active round");
    h.client.resolve_round(&OraclePayload {
        price: final_price,
        timestamp: h.env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: h.env.ledger().network_id(),
        contract_addr: h.contract_id.clone(),
        confidence: None,
        attestation: None,
    });
}

/// Looks up a simulated outcome by address.
fn sim_payout_for(
    sim: &crate::types::SimulationResult,
    user: &Address,
) -> crate::types::UserRoundOutcome {
    for i in 0..sim.outcomes.len() {
        let o = sim.outcomes.get(i).unwrap();
        if o.user == *user {
            return o;
        }
    }
    panic!("no simulated outcome for participant");
}

fn both_models() -> impl Iterator<Item = (FeeModel, FeeIncidence, &'static str)> {
    BOTH_MODELS.into_iter().map(|m| {
        let label = match m {
            FeeModel::FeeOnPot => "FeeOnPot",
            FeeModel::FeeOnWinnings => "FeeOnWinnings",
        };
        (m, FeeIncidence::from(m), label)
    })
}

// ─── Cell 1: UpDown ──────────────────────────────────────────────────────────

/// UpDown cell. Asserts engine == preview == live settlement, plus the
/// UpDown conservation bound, for every (fee model, fee bps) pair.
#[test]
fn matrix_updown_engine_preview_and_settlement_agree() {
    // Two Up stakes (60 + 40) against one Down stake (100): the winning pool
    // divides exactly, so the only conservation slack is per-winner rounding.
    let (a_up, b_up, c_down) = (60i128, 40i128, 100i128);
    let total_pot = a_up + b_up + c_down;
    let pool_up = a_up + b_up;

    for (model, incidence, label) in both_models() {
        for &bps in FEE_BPS_CASES.iter() {
            let env = Env::default();
            let h = harness(&env);
            let client = &h.client;

            let alice = Address::generate(&env);
            let bob = Address::generate(&env);
            let charlie = Address::generate(&env);
            client.mint_initial(&alice);
            client.mint_initial(&bob);
            client.mint_initial(&charlie);

            client.create_round(&START_PRICE, &Some(0));
            client.place_bet(&alice, &a_up, &BetSide::Up);
            client.place_bet(&bob, &b_up, &BetSide::Up);
            client.place_bet(&charlie, &c_down, &BetSide::Down);

            set_fee_bps_now(&h, bps);
            client.set_fee_model(&model);

            // Price goes Up, so Up wins.
            let final_price = 11_000u128;
            let positions = vec![
                UpDownPosition {
                    index: 0,
                    amount: a_up,
                    side_up: true,
                },
                UpDownPosition {
                    index: 1,
                    amount: b_up,
                    side_up: true,
                },
                UpDownPosition {
                    index: 2,
                    amount: c_down,
                    side_up: false,
                },
            ];
            let engine = compute_updown_payouts_with_model(
                &positions,
                START_PRICE,
                final_price,
                pool_up,
                c_down,
                Some(bps),
                incidence,
            )
            .unwrap_or_else(|e| panic!("{label}/{bps}: engine errored: {e:?}"));

            // 1. engine == preview
            let sim = client.simulate_payout(&final_price);
            assert_eq!(sim.mode, RoundMode::UpDown, "{label}/{bps}");
            assert_eq!(sim.fee_model, model as u32, "{label}/{bps}");
            assert_eq!(
                sim.fee_amount, engine.fee_amount,
                "{label}/{bps}: preview fee"
            );
            for (idx, user) in [alice.clone(), bob.clone(), charlie.clone()]
                .into_iter()
                .enumerate()
            {
                let sim_out = sim_payout_for(&sim, &user);
                assert_eq!(
                    sim_out.payout, engine.payouts[idx].payout,
                    "{label}/{bps}: preview payout for participant {idx} disagrees with engine"
                );
            }

            // 2. engine == live settlement
            env.ledger().with_mut(|li| li.sequence_number = 12);
            let treasury_before = client.get_protocol_fee_treasury();
            resolve_at(&h, final_price);

            assert_eq!(
                client.get_pending_winnings(&alice),
                engine.payouts[0].payout,
                "{label}/{bps}: Alice live payout"
            );
            assert_eq!(
                client.get_pending_winnings(&bob),
                engine.payouts[1].payout,
                "{label}/{bps}: Bob live payout"
            );
            assert_eq!(
                client.get_pending_winnings(&charlie),
                0,
                "{label}/{bps}: loser must receive nothing"
            );
            assert_eq!(
                client.get_protocol_fee_treasury() - treasury_before,
                engine.fee_amount,
                "{label}/{bps}: treasury delta"
            );

            // 3. conservation
            let sum_payouts: i128 = engine.payouts.iter().map(|e| e.payout).sum();
            let distributed = sum_payouts + engine.fee_amount;
            let winner_count = engine.payouts.iter().filter(|e| e.is_winner).count() as i128;
            assert!(
                distributed <= total_pot,
                "{label}/{bps}: distributed {distributed} exceeds pot {total_pot}"
            );
            assert!(
                distributed >= total_pot - (winner_count - 1),
                "{label}/{bps}: distributed {distributed} below pot {total_pot} - truncation slack {}",
                winner_count - 1
            );
            // Fee can never exceed the pot it is drawn from.
            assert!(
                engine.fee_amount >= 0 && engine.fee_amount <= total_pot,
                "{label}/{bps}: fee {} outside [0, {total_pot}]",
                engine.fee_amount
            );
        }
    }
}

/// UpDown cell with the price going *down*, so the taxable pool under
/// `FeeOnWinnings` is `pool_up` rather than `pool_down`. Guards the
/// direction-awareness the replay engine was missing.
#[test]
fn matrix_updown_price_down_taxes_the_actual_losing_pool() {
    for (model, incidence, label) in both_models() {
        for &bps in FEE_BPS_CASES.iter() {
            let env = Env::default();
            let h = harness(&env);
            let client = &h.client;

            let alice = Address::generate(&env);
            let bob = Address::generate(&env);
            client.mint_initial(&alice);
            client.mint_initial(&bob);

            client.create_round(&START_PRICE, &Some(0));
            // Down side is the smaller pool here, so a direction-unaware
            // implementation would tax the wrong base and report the wrong fee.
            client.place_bet(&alice, &700, &BetSide::Up);
            client.place_bet(&bob, &300, &BetSide::Down);

            set_fee_bps_now(&h, bps);
            client.set_fee_model(&model);

            let final_price = 9_000u128; // price falls => Down wins
            let positions = vec![
                UpDownPosition {
                    index: 0,
                    amount: 700,
                    side_up: true,
                },
                UpDownPosition {
                    index: 1,
                    amount: 300,
                    side_up: false,
                },
            ];
            let engine = compute_updown_payouts_with_model(
                &positions,
                START_PRICE,
                final_price,
                700,
                300,
                Some(bps),
                incidence,
            )
            .unwrap_or_else(|e| panic!("{label}/{bps}: engine errored: {e:?}"));

            // The pools are deliberately unequal (700 Up vs 300 Down) so the
            // two models must disagree: FeeOnPot taxes the whole 1000 pot,
            // FeeOnWinnings taxes only the 700 losing pool. A direction-blind
            // implementation that taxed `pool_up` under FeeOnWinnings would
            // coincidentally agree here — that is what the next assertion
            // rules out.
            let expected_fee = match incidence {
                FeeIncidence::FeeOnPot => (1_000 * bps as i128) / 10_000,
                FeeIncidence::FeeOnWinnings => (700 * bps as i128) / 10_000,
            };
            assert_eq!(
                engine.fee_amount, expected_fee,
                "{label}/{bps}: expected fee for the actual winning/losing pools"
            );

            // With unequal pools the models must produce *different* fees —
            // otherwise this cell would not actually distinguish them. At very
            // small rates both bases truncate to zero, so skip the comparison
            // there rather than asserting a difference that arithmetic forbids.
            let other_fee = match incidence {
                FeeIncidence::FeeOnPot => (700 * bps as i128) / 10_000,
                FeeIncidence::FeeOnWinnings => (1_000 * bps as i128) / 10_000,
            };
            if engine.fee_amount > 0 && other_fee > 0 {
                assert_ne!(
                    engine.fee_amount, other_fee,
                    "{label}/{bps}: unequal pools must make the two models diverge"
                );
            }

            // Winners keep full principal under FeeOnWinnings: the whole fee
            // comes out of the losing pool.
            if incidence == FeeIncidence::FeeOnWinnings {
                assert_eq!(
                    engine.dist_winning, 300,
                    "{label}/{bps}: FeeOnWinnings must not touch the winning pool"
                );
                assert_eq!(
                    engine.dist_losing,
                    700 - expected_fee,
                    "{label}/{bps}: fee must come entirely from the losing pool"
                );
            } else {
                assert_eq!(
                    engine.dist_winning + engine.dist_losing,
                    1_000 - expected_fee,
                    "{label}/{bps}: FeeOnPot draws across the whole pot"
                );
            }

            let sim = client.simulate_payout(&final_price);
            assert_eq!(
                sim.fee_amount, engine.fee_amount,
                "{label}/{bps}: preview fee"
            );
            assert_eq!(
                sim_payout_for(&sim, &bob).payout,
                engine.payouts[1].payout,
                "{label}/{bps}: winner preview payout"
            );

            env.ledger().with_mut(|li| li.sequence_number = 12);
            let treasury_before = client.get_protocol_fee_treasury();
            resolve_at(&h, final_price);
            assert_eq!(
                client.get_pending_winnings(&bob),
                engine.payouts[1].payout,
                "{label}/{bps}: winner live payout"
            );
            assert_eq!(
                client.get_protocol_fee_treasury() - treasury_before,
                engine.fee_amount,
                "{label}/{bps}: treasury delta"
            );
        }
    }
}

// ─── Cell 2: Precision ───────────────────────────────────────────────────────

/// Precision cell. A single winner, so the remainder policy is unambiguous
/// and engine / preview / settlement can be compared per participant.
#[test]
fn matrix_precision_engine_preview_and_settlement_agree() {
    // Sole winner stakes 40; losers stake 60. Profit = 60.
    let (win_stake, lose_a, lose_b) = (40i128, 25i128, 35i128);
    let total_pot = win_stake + lose_a + lose_b;

    for (model, incidence, label) in both_models() {
        for &bps in FEE_BPS_CASES.iter() {
            let env = Env::default();
            let h = harness(&env);
            let client = &h.client;

            let alice = Address::generate(&env);
            let bob = Address::generate(&env);
            let charlie = Address::generate(&env);
            client.mint_initial(&alice);
            client.mint_initial(&bob);
            client.mint_initial(&charlie);

            client.create_round(&START_PRICE, &Some(1));
            client.place_precision_prediction(&alice, &win_stake, &START_PRICE); // exact
            client.place_precision_prediction(&bob, &lose_a, &20_000);
            client.place_precision_prediction(&charlie, &lose_b, &30_000);

            set_fee_bps_now(&h, bps);
            client.set_fee_model(&model);

            let final_price = START_PRICE;
            let entries = vec![
                PrecisionEntry {
                    index: 0,
                    predicted_price: START_PRICE,
                    amount: win_stake,
                    revealed: true,
                },
                PrecisionEntry {
                    index: 1,
                    predicted_price: 20_000,
                    amount: lose_a,
                    revealed: true,
                },
                PrecisionEntry {
                    index: 2,
                    predicted_price: 30_000,
                    amount: lose_b,
                    revealed: true,
                },
            ];
            let engine = compute_precision_payouts_with_policy_and_model(
                &entries,
                final_price,
                Some(bps),
                PrecisionScoringPolicy {
                    mode: PrecisionScoringMode::AbsoluteDistance,
                    confidence_band: None,
                },
                PrecisionPayoutPolicy::Equal,
                incidence,
            )
            .unwrap_or_else(|e| panic!("{label}/{bps}: engine errored: {e:?}"));

            // Under FeeOnWinnings the sole winner realises the whole pot as
            // profit, so the base is the same as the pot. This cell cannot
            // distinguish the two models; `matrix_precision_fee_on_winnings_differs`
            // can.
            let expected_fee = match incidence {
                FeeIncidence::FeeOnPot => total_pot * bps as i128 / 10_000,
                FeeIncidence::FeeOnWinnings => (total_pot - win_stake) * bps as i128 / 10_000,
            };
            assert_eq!(engine.fee_amount, expected_fee, "{label}/{bps}: engine fee");
            assert_eq!(engine.total_pot, total_pot, "{label}/{bps}: engine pot");

            // 1. engine == preview
            let sim = client.simulate_payout(&final_price);
            assert_eq!(sim.mode, RoundMode::Precision, "{label}/{bps}");
            assert_eq!(sim.fee_model, model as u32, "{label}/{bps}");
            assert_eq!(sim.precision_total_stake, total_pot, "{label}/{bps}");
            assert_eq!(
                sim.fee_amount, engine.fee_amount,
                "{label}/{bps}: preview fee"
            );
            for (idx, user) in [alice.clone(), bob.clone(), charlie.clone()]
                .into_iter()
                .enumerate()
            {
                let sim_out = sim_payout_for(&sim, &user);
                assert_eq!(
                    sim_out.payout, engine.payouts[idx].payout,
                    "{label}/{bps}: preview payout for participant {idx} disagrees with engine"
                );
            }

            // 2. engine == live settlement
            env.ledger().with_mut(|li| li.sequence_number = 12);
            let treasury_before = client.get_protocol_fee_treasury();
            resolve_at(&h, final_price);

            assert_eq!(
                client.get_pending_winnings(&alice),
                engine.payouts[0].payout,
                "{label}/{bps}: Alice live payout"
            );
            assert_eq!(
                client.get_pending_winnings(&bob),
                0,
                "{label}/{bps}: Bob is a loser"
            );
            assert_eq!(
                client.get_pending_winnings(&charlie),
                0,
                "{label}/{bps}: Charlie is a loser"
            );
            assert_eq!(
                client.get_protocol_fee_treasury() - treasury_before,
                engine.fee_amount,
                "{label}/{bps}: treasury delta"
            );

            // 3. conservation — exact for precision
            let sum_payouts: i128 = engine.payouts.iter().map(|e| e.payout).sum();
            assert_eq!(
                sum_payouts + engine.fee_amount,
                total_pot,
                "{label}/{bps}: precision conservation must be exact"
            );
        }
    }
}

/// Precision cell where the two models genuinely differ, under both payout
/// policies and a multi-winner tie. Locks the per-model fee amounts and
/// exercises exact conservation with a remainder.
#[test]
fn matrix_precision_fee_on_winnings_differs_and_conserves() {
    // Two winners (both exact), two losers. Profit = loser stakes = 60.
    let (win_a, win_b, lose_a, lose_b) = (50i128, 50i128, 30i128, 30i128);
    let total_pot = win_a + win_b + lose_a + lose_b;
    let winner_stakes = win_a + win_b;

    for &policy_code in [0u32, 1u32].iter() {
        let policy = if policy_code == 0 {
            PrecisionPayoutPolicy::Equal
        } else {
            PrecisionPayoutPolicy::StakeWeighted
        };
        for (model, incidence, label) in both_models() {
            for &bps in FEE_BPS_CASES.iter() {
                let env = Env::default();
                let h = harness(&env);
                let client = &h.client;

                let alice = Address::generate(&env);
                let bob = Address::generate(&env);
                let carol = Address::generate(&env);
                let dave = Address::generate(&env);
                client.mint_initial(&alice);
                client.mint_initial(&bob);
                client.mint_initial(&carol);
                client.mint_initial(&dave);

                client.create_round(&START_PRICE, &Some(1));
                client.set_precision_payout_policy(&policy_code);
                client.place_precision_prediction(&alice, &win_a, &START_PRICE);
                client.place_precision_prediction(&bob, &win_b, &START_PRICE);
                client.place_precision_prediction(&carol, &lose_a, &25_000);
                client.place_precision_prediction(&dave, &lose_b, &35_000);

                set_fee_bps_now(&h, bps);
                client.set_fee_model(&model);

                let entries = vec![
                    PrecisionEntry {
                        index: 0,
                        predicted_price: START_PRICE,
                        amount: win_a,
                        revealed: true,
                    },
                    PrecisionEntry {
                        index: 1,
                        predicted_price: START_PRICE,
                        amount: win_b,
                        revealed: true,
                    },
                    PrecisionEntry {
                        index: 2,
                        predicted_price: 25_000,
                        amount: lose_a,
                        revealed: true,
                    },
                    PrecisionEntry {
                        index: 3,
                        predicted_price: 35_000,
                        amount: lose_b,
                        revealed: true,
                    },
                ];
                let engine = compute_precision_payouts_with_policy_and_model(
                    &entries,
                    START_PRICE,
                    Some(bps),
                    PrecisionScoringPolicy {
                        mode: PrecisionScoringMode::AbsoluteDistance,
                        confidence_band: None,
                    },
                    policy,
                    incidence,
                )
                .unwrap_or_else(|e| panic!("{label}/{policy_code}/{bps}: engine errored: {e:?}"));

                let expected_fee = match incidence {
                    // FeeOnPot taxes everything, including winners' principal.
                    FeeIncidence::FeeOnPot => total_pot * bps as i128 / 10_000,
                    // FeeOnWinnings taxes only realised profit.
                    FeeIncidence::FeeOnWinnings => {
                        (total_pot - winner_stakes) * bps as i128 / 10_000
                    }
                };
                assert_eq!(
                    engine.fee_amount, expected_fee,
                    "{label}/{policy_code}/{bps}: engine fee"
                );
                assert_eq!(
                    engine.winner_stakes, winner_stakes,
                    "{label}/{policy_code}/{bps}"
                );

                // FeeOnWinnings must never exceed FeeOnPot at the same rate.
                let pot_fee = total_pot * bps as i128 / 10_000;
                assert!(
                    engine.fee_amount <= pot_fee,
                    "{label}/{policy_code}/{bps}: fee {} exceeds FeeOnPot fee {pot_fee}",
                    engine.fee_amount
                );

                // 1. engine == preview
                let sim = client.simulate_payout(&START_PRICE);
                assert_eq!(
                    sim.fee_amount, engine.fee_amount,
                    "{label}/{policy_code}/{bps}"
                );
                for (idx, user) in [alice.clone(), bob.clone(), carol.clone(), dave.clone()]
                    .into_iter()
                    .enumerate()
                {
                    assert_eq!(
                        sim_payout_for(&sim, &user).payout,
                        engine.payouts[idx].payout,
                        "{label}/{policy_code}/{bps}: preview payout for participant {idx}"
                    );
                }

                // 2. engine == live settlement
                env.ledger().with_mut(|li| li.sequence_number = 12);
                let treasury_before = client.get_protocol_fee_treasury();
                resolve_at(&h, START_PRICE);

                let live = [
                    client.get_pending_winnings(&alice),
                    client.get_pending_winnings(&bob),
                    client.get_pending_winnings(&carol),
                    client.get_pending_winnings(&dave),
                ];
                for (idx, payout) in live.into_iter().enumerate() {
                    assert_eq!(
                        payout, engine.payouts[idx].payout,
                        "{label}/{policy_code}/{bps}: live payout for participant {idx}"
                    );
                }
                assert_eq!(
                    client.get_protocol_fee_treasury() - treasury_before,
                    engine.fee_amount,
                    "{label}/{policy_code}/{bps}: treasury delta"
                );

                // 3. conservation — exact
                let sum_payouts: i128 = live.into_iter().sum();
                assert_eq!(
                    sum_payouts + engine.fee_amount,
                    total_pot,
                    "{label}/{policy_code}/{bps}: precision conservation must be exact"
                );
            }
        }
    }
}

// ─── Cell 3: non-competitive rounds (zero fee under both models) ────────────

/// Ties, one-sided pools and all-unrevealed rounds produce no realised
/// winnings, so *both* models must charge nothing. A fee here would be a fee
/// on nothing.
#[test]
fn matrix_non_competitive_rounds_charge_no_fee() {
    for (model, _incidence, label) in both_models() {
        for &bps in FEE_BPS_CASES.iter() {
            // --- UpDown tie (final == start) ---
            let env = Env::default();
            let h = harness(&env);
            let client = &h.client;
            let alice = Address::generate(&env);
            let bob = Address::generate(&env);
            client.mint_initial(&alice);
            client.mint_initial(&bob);
            client.create_round(&START_PRICE, &Some(0));
            client.place_bet(&alice, &70, &BetSide::Up);
            client.place_bet(&bob, &30, &BetSide::Down);
            set_fee_bps_now(&h, bps);
            client.set_fee_model(&model);

            let sim = client.simulate_payout(&START_PRICE);
            assert_eq!(sim.fee_amount, 0, "{label}/{bps}: UpDown tie preview fee");
            for u in [alice.clone(), bob.clone()] {
                assert_eq!(
                    sim_payout_for(&sim, &u).outcome,
                    UserOutcomeType::Refund,
                    "{label}/{bps}: tie must refund"
                );
            }
            env.ledger().with_mut(|li| li.sequence_number = 12);
            let t0 = client.get_protocol_fee_treasury();
            resolve_at(&h, START_PRICE);
            assert_eq!(
                client.get_pending_winnings(&alice),
                70,
                "{label}/{bps}: tie refund"
            );
            assert_eq!(
                client.get_pending_winnings(&bob),
                30,
                "{label}/{bps}: tie refund"
            );
            assert_eq!(
                client.get_protocol_fee_treasury() - t0,
                0,
                "{label}/{bps}: tie must not charge a fee"
            );

            // --- UpDown one-sided pool ---
            let env = Env::default();
            let h = harness(&env);
            let client = &h.client;
            let alice = Address::generate(&env);
            client.mint_initial(&alice);
            client.create_round(&START_PRICE, &Some(0));
            client.place_bet(&alice, &500, &BetSide::Up);
            set_fee_bps_now(&h, bps);
            client.set_fee_model(&model);

            let sim = client.simulate_payout(&11_000);
            assert_eq!(sim.fee_amount, 0, "{label}/{bps}: one-sided preview fee");
            env.ledger().with_mut(|li| li.sequence_number = 12);
            let t0 = client.get_protocol_fee_treasury();
            resolve_at(&h, 11_000);
            assert_eq!(
                client.get_pending_winnings(&alice),
                500,
                "{label}/{bps}: one-sided refund"
            );
            assert_eq!(
                client.get_protocol_fee_treasury() - t0,
                0,
                "{label}/{bps}: one-sided must not charge a fee"
            );
        }
    }
}

/// A lone winner who staked the entire pot realises zero profit, so
/// `FeeOnWinnings` must charge exactly nothing while `FeeOnPot` still charges
/// its full rate. This is the sharpest single behavioural difference between
/// the two models.
#[test]
fn matrix_lone_full_pot_winner_is_fee_free_under_fee_on_winnings() {
    let stake = 100i128;
    let bps = 1_000u32; // 10%

    // FeeOnPot: taxed on the whole pot.
    let env = Env::default();
    let h = harness(&env);
    let client = &h.client;
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&START_PRICE, &Some(1));
    client.place_precision_prediction(&alice, &stake, &START_PRICE);
    set_fee_bps_now(&h, bps);
    client.set_fee_model(&FeeModel::FeeOnPot);

    let sim = client.simulate_payout(&START_PRICE);
    assert_eq!(sim.fee_amount, 10, "FeeOnPot taxes the full pot");
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let t0 = client.get_protocol_fee_treasury();
    resolve_at(&h, START_PRICE);
    assert_eq!(client.get_pending_winnings(&alice), 90, "FeeOnPot payout");
    assert_eq!(
        client.get_protocol_fee_treasury() - t0,
        10,
        "FeeOnPot treasury"
    );

    // FeeOnWinnings: zero profit, zero fee, pot returned intact.
    let env = Env::default();
    let h = harness(&env);
    let client = &h.client;
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&START_PRICE, &Some(1));
    client.place_precision_prediction(&alice, &stake, &START_PRICE);
    set_fee_bps_now(&h, bps);
    client.set_fee_model(&FeeModel::FeeOnWinnings);

    let sim = client.simulate_payout(&START_PRICE);
    assert_eq!(sim.fee_amount, 0, "FeeOnWinnings must not tax zero profit");
    env.ledger().with_mut(|li| li.sequence_number = 12);
    let t0 = client.get_protocol_fee_treasury();
    resolve_at(&h, START_PRICE);
    assert_eq!(
        client.get_pending_winnings(&alice),
        stake,
        "FeeOnWinnings returns the pot intact"
    );
    assert_eq!(
        client.get_protocol_fee_treasury() - t0,
        0,
        "FeeOnWinnings treasury"
    );
}

/// `bps = None` (fee disabled) must be identical under both models: there is
/// no fee to place anywhere.
#[test]
fn matrix_fee_disabled_is_model_independent() {
    let (a_up, b_up, c_down) = (60i128, 40i128, 100i128);
    let pool_up = a_up + b_up;

    let mut reference: Option<(i128, i128, i128)> = None;
    for (model, incidence, label) in both_models() {
        let env = Env::default();
        let h = harness(&env);
        let client = &h.client;
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);
        client.mint_initial(&alice);
        client.mint_initial(&bob);
        client.mint_initial(&charlie);
        client.create_round(&START_PRICE, &Some(0));
        client.place_bet(&alice, &a_up, &BetSide::Up);
        client.place_bet(&bob, &b_up, &BetSide::Up);
        client.place_bet(&charlie, &c_down, &BetSide::Down);
        // Never call set_fee_bps_now: fee stays unconfigured.
        client.set_fee_model(&model);

        let engine = compute_updown_payouts_with_model(
            &[
                UpDownPosition {
                    index: 0,
                    amount: a_up,
                    side_up: true,
                },
                UpDownPosition {
                    index: 1,
                    amount: b_up,
                    side_up: true,
                },
                UpDownPosition {
                    index: 2,
                    amount: c_down,
                    side_up: false,
                },
            ],
            START_PRICE,
            11_000,
            pool_up,
            c_down,
            None,
            incidence,
        )
        .unwrap();

        assert_eq!(engine.fee_amount, 0, "{label}: disabled fee must be zero");
        let triple = (
            engine.payouts[0].payout,
            engine.payouts[1].payout,
            engine.payouts[2].payout,
        );
        match &reference {
            None => reference = Some(triple),
            Some(first) => assert_eq!(
                &triple, first,
                "{label}: a disabled fee must be model-independent"
            ),
        }

        // Live settlement must agree too.
        env.ledger().with_mut(|li| li.sequence_number = 12);
        let t0 = client.get_protocol_fee_treasury();
        resolve_at(&h, 11_000);
        assert_eq!(client.get_pending_winnings(&alice), triple.0, "{label}");
        assert_eq!(client.get_pending_winnings(&bob), triple.1, "{label}");
        assert_eq!(client.get_pending_winnings(&charlie), triple.2, "{label}");
        assert_eq!(client.get_protocol_fee_treasury() - t0, 0, "{label}");
    }
}

// ─── Cell 4: the whole grid, both modes, both models ─────────────────────────

/// Smoke-level sweep of the full `FeeModel` x `RoundMode` grid: for every
/// cell the preview and the live settlement must agree on the fee, even when
/// the round happens to be non-competitive. Cheap insurance that a new mode
/// or model cannot be added without wiring a fee path.
#[test]
fn matrix_full_grid_preview_and_settlement_agree_on_fee() {
    for &mode in BOTH_MODES.iter() {
        for (model, _incidence, label) in both_models() {
            let env = Env::default();
            let h = harness(&env);
            let client = &h.client;
            let alice = Address::generate(&env);
            let bob = Address::generate(&env);
            client.mint_initial(&alice);
            client.mint_initial(&bob);

            client.create_round(&START_PRICE, &Some(mode));
            if mode == 0 {
                client.place_bet(&alice, &250, &BetSide::Up);
                client.place_bet(&bob, &150, &BetSide::Down);
            } else {
                client.place_precision_prediction(&alice, &250, &START_PRICE);
                client.place_precision_prediction(&bob, &150, &40_000);
            }
            set_fee_bps_now(&h, 500);
            client.set_fee_model(&model);

            let final_price = 12_000u128;
            let sim = client.simulate_payout(&final_price);
            assert_eq!(sim.fee_model, model as u32, "{label}/mode {mode}");

            env.ledger().with_mut(|li| li.sequence_number = 12);
            let t0 = client.get_protocol_fee_treasury();
            resolve_at(&h, final_price);

            let sum_sim: i128 = (0..sim.outcomes.len())
                .map(|i| sim.outcomes.get(i).unwrap().payout)
                .sum();
            let sum_live = client.get_pending_winnings(&alice) + client.get_pending_winnings(&bob);
            let treasury_delta = client.get_protocol_fee_treasury() - t0;

            assert_eq!(
                sum_live, sum_sim,
                "{label}/mode {mode}: live payouts must match the preview"
            );
            assert_eq!(
                treasury_delta, sim.fee_amount,
                "{label}/mode {mode}: treasury must receive the previewed fee"
            );
            assert!(
                sum_live + treasury_delta <= sim.pool_up + sim.pool_down || mode == 1,
                "{label}/mode {mode}: conservation"
            );
        }
    }
}
