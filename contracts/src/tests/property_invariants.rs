// SPDX-License-Identifier: MIT
//! Property-based tests for payout invariants.
//!
//! These tests exercise randomized scenarios to ensure core invariants such as:
//! - Conservation of value (no payouts exceed the total pot)
//! - Non-negative pending winnings and balances
//! - Monotonic user statistics (wins, losses, and best streak never decrease)
//! - **Fee conservation**: `user_payouts + treasury_delta == pot` for every
//!   settlement path including fee=0 and fee>0 cases.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{
    BetSide, DataKeyCore, DataKeyScoped, OraclePayload, PrecisionPrediction, Round, UserPosition, UserStats,
};
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, Map,
};

// ─── Existing tests (unchanged) ───────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Up/Down mode: payouts should never exceed the total pot and losers should
    /// never receive positive pending winnings.
    #[test]
    fn updown_payout_conserves_pot_and_is_non_negative(
        a_up in 0i128..1_000_000_000i128,
        b_up in 0i128..1_000_000_000i128,
        c_down in 0i128..1_000_000_000i128,
    ) {
        let total_up = a_up.saturating_add(b_up);
        let total_down = c_down;
        let total_pot = total_up.saturating_add(total_down);

        // Require at least one winner and one loser with a non-zero pot.
        prop_assume!(a_up > 0 || b_up > 0);
        prop_assume!(c_down > 0);
        prop_assume!(total_pot > 0);

        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

        // Create a simple Up/Down round
        let start_price: u128 = 1_0000000;
        client.create_round(&start_price, &None);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        // Install synthetic positions and pools directly in storage
        env.as_contract(&contract_id, || {
            let mut positions = Map::<Address, UserPosition>::new(&env);

            if a_up > 0 {
                positions.set(alice.clone(), UserPosition {
                    amount: a_up,
                    side: BetSide::Up,
                });
            }

            if b_up > 0 {
                positions.set(bob.clone(), UserPosition {
                    amount: b_up,
                    side: BetSide::Up,
                });
            }

            if c_down > 0 {
                positions.set(charlie.clone(), UserPosition {
                    amount: c_down,
                    side: BetSide::Down,
                });
            }

            env.storage().persistent().set(&DataKeyCore::UpDownPositions, &positions);

            let mut round: Round = env.storage().persistent().get(&DataKeyCore::ActiveRound).unwrap();
            round.pool_up = total_up;
            round.pool_down = total_down;
            env.storage().persistent().set(&DataKeyCore::ActiveRound, &round);
        });

        // Advance ledger to allow resolution
        env.ledger().with_mut(|li| {
            li.sequence_number = 12;
        });

        // Force "price went up" scenario
        client.resolve_round(&OraclePayload {
            price: 2_0000000,
            timestamp: env.ledger().timestamp(),
            round_id: 0,
            nonce: 1u64,
            network_id: env.ledger().network_id(),
            contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,        });

        let alice_pending = client.get_pending_winnings(&alice);
        let bob_pending = client.get_pending_winnings(&bob);
        let charlie_pending = client.get_pending_winnings(&charlie);

        let winners_total = alice_pending.saturating_add(bob_pending);

        // No negative pending winnings for any participant
        prop_assert!(alice_pending >= 0);
        prop_assert!(bob_pending >= 0);
        prop_assert!(charlie_pending >= 0);

        // Loser (Down side) should not receive positive winnings
        prop_assert_eq!(charlie_pending, 0);

        // Total payouts to winners should never exceed the total pot
        prop_assert!(winners_total <= total_pot);

        // Winners should at least receive back the amount they staked
        prop_assert!(winners_total >= total_up);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Precision mode: payouts to winners should never exceed the total pot,
    /// and all pending winnings must remain non-negative.
    #[test]
    fn precision_payout_respects_pot_and_non_negative(
        amount_a in 0i128..1_000_000_000i128,
        amount_b in 0i128..1_000_000_000i128,
        amount_c in 0i128..1_000_000_000i128,
        price_a in 0u128..=99_999_999u128,
        price_b in 0u128..=99_999_999u128,
        price_c in 0u128..=99_999_999u128,
        final_price in 0u128..=99_999_999u128,
    ) {
        let total_pot = amount_a.saturating_add(amount_b).saturating_add(amount_c);

        // Require at least one non-zero prediction so there is something to resolve.
        prop_assume!(total_pot > 0);

        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

        // Create a Precision round
        let start_price: u128 = 1_0000000;
        client.create_round(&start_price, &Some(1));

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);

            if amount_a > 0 {
                predictions.set(
                    alice.clone(),
                    PrecisionPrediction {
                        user: alice.clone(),
                        predicted_price: price_a,
                        amount: amount_a,
                    },
                );
            }

            if amount_b > 0 {
                predictions.set(
                    bob.clone(),
                    PrecisionPrediction {
                        user: bob.clone(),
                        predicted_price: price_b,
                        amount: amount_b,
                    },
                );
            }

            if amount_c > 0 {
                predictions.set(
                    charlie.clone(),
                    PrecisionPrediction {
                        user: charlie.clone(),
                        predicted_price: price_c,
                        amount: amount_c,
                    },
                );
            }

            env.storage()
                .persistent()
                .set(&DataKeyCore::PrecisionPositions, &predictions);
        });

        // Advance ledger to allow resolution
        env.ledger().with_mut(|li| {
            li.sequence_number = 12;
        });

        client.resolve_round(&OraclePayload {
            price: final_price,
            timestamp: env.ledger().timestamp(),
            round_id: 0,
            nonce: 1u64,
            network_id: env.ledger().network_id(),
            contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,        });

        let alice_pending = client.get_pending_winnings(&alice);
        let bob_pending = client.get_pending_winnings(&bob);
        let charlie_pending = client.get_pending_winnings(&charlie);

        let total_pending = alice_pending
            .saturating_add(bob_pending)
            .saturating_add(charlie_pending);

        // Pending winnings are never negative
        prop_assert!(alice_pending >= 0);
        prop_assert!(bob_pending >= 0);
        prop_assert!(charlie_pending >= 0);

        // Total payouts should never exceed the total pot
        prop_assert!(total_pending <= total_pot);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// User statistics should be monotonic:
    /// - total_wins and total_losses never decrease
    /// - best_streak never decreases
    /// - current_streak is reset on loss and increases on consecutive wins
    #[test]
    fn user_stats_are_monotonic(outcomes in proptest::collection::vec(any::<bool>(), 1..32)) {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let user = Address::generate(&env);

        env.as_contract(&contract_id, || {
            for outcome in outcomes {
                let before: UserStats = VirtualTokenContract::get_user_stats(env.clone(), user.clone());

                if outcome {
                    VirtualTokenContract::_update_stats_win(&env, user.clone()).unwrap();
                } else {
                    VirtualTokenContract::_update_stats_loss(&env, user.clone()).unwrap();
                }

                let after: UserStats = VirtualTokenContract::get_user_stats(env.clone(), user.clone());

                // wins and losses are monotonic
                assert!(after.total_wins >= before.total_wins);
                assert!(after.total_losses >= before.total_losses);

                // best_streak is monotonic
                assert!(after.best_streak >= before.best_streak);

                // current_streak is never negative (u32) and resets on loss
                if outcome {
                    assert!(after.current_streak >= before.current_streak);
                } else {
                    assert_eq!(after.current_streak, 0);
                }
            }
        });
    }
}

// ─── Fee-conservation property tests ─────────────────────────────────────────
//
// These four suites assert: user_payouts + treasury_delta == pot
// for every settlement path, for both fee=0 and fee>0.
//
// On any failure proptest prints its minimal repro seed automatically.

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Up/Down mode — fee conservation for both fee=0 (disabled) and fee>0.
    ///
    /// Invariant: `sum_winner_payouts + treasury_delta` is within one stroop
    /// per winner of `total_pot`.  Losers never receive positive winnings.
    /// Treasury must not move when fee is disabled.
    #[test]
    fn fee_conservation_updown(
        a_up   in 1i128..500_000_000i128,
        b_up   in 1i128..500_000_000i128,
        c_down in 1i128..500_000_000i128,
        // fee_bps=0 means disabled; 1..=1000 means enabled (max 10%)
        fee_bps_raw in 0u32..=1_000u32,
    ) {
        let total_up   = a_up.saturating_add(b_up);
        let total_down = c_down;
        let total_pot  = total_up.saturating_add(total_down);

        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);

        let admin  = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
        client.create_round(&1_0000000u128, &None);

        let alice   = Address::generate(&env);
        let bob     = Address::generate(&env);
        let charlie = Address::generate(&env);

        // Inject positions and optional fee bps directly into storage.
        env.as_contract(&contract_id, || {
            let mut positions = Map::<Address, UserPosition>::new(&env);
            positions.set(alice.clone(),   UserPosition { amount: a_up,   side: BetSide::Up });
            positions.set(bob.clone(),     UserPosition { amount: b_up,   side: BetSide::Up });
            positions.set(charlie.clone(), UserPosition { amount: c_down, side: BetSide::Down });
            env.storage().persistent().set(&DataKeyCore::UpDownPositions, &positions);

            let mut round: Round = env.storage().persistent().get(&DataKeyCore::ActiveRound).unwrap();
            round.pool_up   = total_up;
            round.pool_down = total_down;
            env.storage().persistent().set(&DataKeyCore::ActiveRound, &round);

            // fee_bps_raw == 0  →  fee disabled (no key written)
            if fee_bps_raw > 0 {
                env.storage().persistent().set(&DataKeyCore::ProtocolFeeBps, &fee_bps_raw);
            }
        });

        // Snapshot treasury before resolution.
        let treasury_before = client.get_protocol_fee_treasury();

        env.ledger().with_mut(|li| { li.sequence_number = 12; });

        // Price went up → Up-side wins.
        client.resolve_round(&OraclePayload {
            price: 2_0000000,
            timestamp: env.ledger().timestamp(),
            round_id: 0,
            nonce: 1u64,
            network_id: env.ledger().network_id(),
            contract_addr: contract_id.clone(),
            confidence: None,
            attestation: None,        });

        let alice_pending   = client.get_pending_winnings(&alice);
        let bob_pending     = client.get_pending_winnings(&bob);
        let charlie_pending = client.get_pending_winnings(&charlie);
        let treasury_after  = client.get_protocol_fee_treasury();

        let sum_winner_payouts = alice_pending + bob_pending;
        let treasury_delta     = treasury_after - treasury_before;
        let winner_count: i128 = 2; // alice and bob both placed Up bets

        // Loser must receive nothing.
        prop_assert_eq!(charlie_pending, 0,
            "Loser received positive winnings: charlie_pending={}", charlie_pending);

        // When fee is disabled treasury must not move.
        if fee_bps_raw == 0 {
            prop_assert_eq!(treasury_delta, 0,
                "Treasury moved despite fee being disabled: delta={}", treasury_delta);
        }

        // Conservation upper bound: sum_payouts + treasury_delta <= pot
        prop_assert!(sum_winner_payouts + treasury_delta <= total_pot,
            "Conservation upper bound violated: payouts={} treasury_delta={} pot={}",
            sum_winner_payouts, treasury_delta, total_pot);

        // Conservation lower bound: sum_payouts + treasury_delta >= pot - (winners-1)
        // (slack accounts for per-winner integer truncation)
        prop_assert!(
            sum_winner_payouts + treasury_delta >= total_pot - (winner_count - 1),
            "Conservation lower bound violated: payouts={} treasury_delta={} pot={} winner_count={}",
            sum_winner_payouts, treasury_delta, total_pot, winner_count
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Precision mode — fee conservation for both fee=0 (disabled) and fee>0.
    ///
    /// Invariant: `sum_winner_payouts + treasury_delta == total_pot` **exactly**
    /// (no per-winner truncation slack because the contract assigns the remainder
    /// to the first winner).
    #[test]
    fn fee_conservation_precision(
        amount_a    in 1i128..300_000_000i128,
        amount_b    in 1i128..300_000_000i128,
        amount_c    in 1i128..300_000_000i128,
        price_a     in 0u128..99_999_999u128,
        price_b     in 1u128..99_999_999u128,
        price_c     in 2u128..99_999_999u128,
        final_price in 0u128..99_999_999u128,
        fee_bps_raw in 0u32..=1_000u32,
    ) {
        // Ensure distinct prices so winner determination is unambiguous in many cases.
        prop_assume!(price_a != price_b && price_b != price_c && price_a != price_c);

        let total_pot = amount_a + amount_b + amount_c;
        prop_assume!(total_pot > 0);

        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);

        let admin  = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
        client.create_round(&1_0000000u128, &Some(1));

        let alice   = Address::generate(&env);
        let bob     = Address::generate(&env);
        let charlie = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);
            predictions.set(alice.clone(),
                PrecisionPrediction { user: alice.clone(),   predicted_price: price_a, amount: amount_a });
            predictions.set(bob.clone(),
                PrecisionPrediction { user: bob.clone(),     predicted_price: price_b, amount: amount_b });
            predictions.set(charlie.clone(),
                PrecisionPrediction { user: charlie.clone(), predicted_price: price_c, amount: amount_c });
            env.storage().persistent().set(&DataKeyCore::PrecisionPositions, &predictions);

            if fee_bps_raw > 0 {
                env.storage().persistent().set(&DataKeyCore::ProtocolFeeBps, &fee_bps_raw);
            }
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
            attestation: None,        });

        let alice_pending   = client.get_pending_winnings(&alice);
        let bob_pending     = client.get_pending_winnings(&bob);
        let charlie_pending = client.get_pending_winnings(&charlie);
        let treasury_after  = client.get_protocol_fee_treasury();

        let sum_payouts    = alice_pending + bob_pending + charlie_pending;
        let treasury_delta = treasury_after - treasury_before;

        // All pending winnings must be non-negative.
        prop_assert!(alice_pending   >= 0, "alice_pending < 0: {}", alice_pending);
        prop_assert!(bob_pending     >= 0, "bob_pending < 0: {}", bob_pending);
        prop_assert!(charlie_pending >= 0, "charlie_pending < 0: {}", charlie_pending);

        // When fee is disabled treasury must not move.
        if fee_bps_raw == 0 {
            prop_assert_eq!(treasury_delta, 0,
                "Treasury moved despite fee being disabled: delta={}", treasury_delta);
        }

        // Precision conservation is exact (remainder goes to first winner).
        prop_assert_eq!(
            sum_payouts + treasury_delta,
            total_pot,
            "Precision fee conservation violated: payouts={} treasury_delta={} pot={} fee_bps={}",
            sum_payouts, treasury_delta, total_pot, fee_bps_raw
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    /// Up/Down mode — tie (price unchanged): full refund, treasury must not move.
    ///
    /// Invariant: `sum_refunds == pot` and `treasury_delta == 0`.
    #[test]
    fn fee_conservation_updown_tie_refund(
        a_up   in 1i128..300_000_000i128,
        b_down in 1i128..300_000_000i128,
        fee_bps_raw in 0u32..=1_000u32,
    ) {
        let total_pot = a_up + b_down;

        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);

        let admin  = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
        client.create_round(&1_0000000u128, &None);

        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let mut positions = Map::<Address, UserPosition>::new(&env);
            positions.set(alice.clone(), UserPosition { amount: a_up,   side: BetSide::Up   });
            positions.set(bob.clone(),   UserPosition { amount: b_down, side: BetSide::Down });
            env.storage().persistent().set(&DataKeyCore::UpDownPositions, &positions);

            let mut round: Round = env.storage().persistent().get(&DataKeyCore::ActiveRound).unwrap();
            round.pool_up   = a_up;
            round.pool_down = b_down;
            env.storage().persistent().set(&DataKeyCore::ActiveRound, &round);

            // Even with a fee configured, it must NOT be charged on a tie/refund.
            if fee_bps_raw > 0 {
                env.storage().persistent().set(&DataKeyCore::ProtocolFeeBps, &fee_bps_raw);
            }
        });

        let treasury_before = client.get_protocol_fee_treasury();

        env.ledger().with_mut(|li| { li.sequence_number = 12; });

        // Resolve with the same price — triggers full refund path.
        client.resolve_round(&OraclePayload {
            price: 1_0000000, // == start_price → tie
            timestamp: env.ledger().timestamp(),
            round_id: 0,
            nonce: 1u64,
            network_id: env.ledger().network_id(),
            contract_addr: contract_id.clone(),
            confidence: None,
            attestation: None,        });

        let alice_refund   = client.get_pending_winnings(&alice);
        let bob_refund     = client.get_pending_winnings(&bob);
        let treasury_after = client.get_protocol_fee_treasury();

        let sum_refunds    = alice_refund + bob_refund;
        let treasury_delta = treasury_after - treasury_before;

        // Treasury must never move on a refund path regardless of fee config.
        prop_assert_eq!(treasury_delta, 0,
            "Fee was charged on tie/refund: treasury_delta={} fee_bps={}",
            treasury_delta, fee_bps_raw);

        // All stake must be returned exactly.
        prop_assert!(alice_refund >= 0);
        prop_assert!(bob_refund   >= 0);
        prop_assert_eq!(sum_refunds, total_pot,
            "Tie refund conservation violated: refunds={} pot={}",
            sum_refunds, total_pot);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    /// Cancel path — fee must not be charged; every participant must receive
    /// back their exact stake.
    ///
    /// Invariant: `treasury_delta == 0` and `sum_refunds == pot`.
    #[test]
    fn fee_conservation_cancel_refund(
        a_up   in 1i128..300_000_000i128,
        b_down in 1i128..300_000_000i128,
        fee_bps_raw in 0u32..=1_000u32,
    ) {
        let total_pot = a_up + b_down;

        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);

        let admin  = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
        client.create_round(&1_0000000u128, &None);

        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);

        // Set up positions and optional fee bps.
        env.as_contract(&contract_id, || {
            let mut positions = Map::<Address, UserPosition>::new(&env);
            positions.set(alice.clone(), UserPosition { amount: a_up,   side: BetSide::Up   });
            positions.set(bob.clone(),   UserPosition { amount: b_down, side: BetSide::Down });
            env.storage().persistent().set(&DataKeyCore::UpDownPositions, &positions);

            let mut round: Round = env.storage().persistent().get(&DataKeyCore::ActiveRound).unwrap();
            round.pool_up   = a_up;
            round.pool_down = b_down;
            env.storage().persistent().set(&DataKeyCore::ActiveRound, &round);

            if fee_bps_raw > 0 {
                env.storage().persistent().set(&DataKeyCore::ProtocolFeeBps, &fee_bps_raw);
            }
        });

        // Register both users as participants so the cancel path can find them.
        env.as_contract(&contract_id, || {
            let round: Round = env.storage().persistent().get(&DataKeyCore::ActiveRound).unwrap();
            let round_id = round.round_id;
            let mut parts = soroban_sdk::Vec::<Address>::new(&env);
            parts.push_back(alice.clone());
            parts.push_back(bob.clone());
            env.storage().persistent().set(
                &DataKeyScoped::RoundParticipants(round_id),
                &parts,
            );
            // Also store individual position keys so cancel can read them.
            env.storage().persistent().set(
                &DataKeyScoped::Position(round_id, alice.clone()),
                &UserPosition { amount: a_up,   side: BetSide::Up   },
            );
            env.storage().persistent().set(
                &DataKeyScoped::Position(round_id, bob.clone()),
                &UserPosition { amount: b_down, side: BetSide::Down },
            );
        });

        let treasury_before = client.get_protocol_fee_treasury();

        // Cancel the round — admin auth is mocked.
        client.cancel_round(&0u32);

        let alice_refund   = client.get_pending_winnings(&alice);
        let bob_refund     = client.get_pending_winnings(&bob);
        let treasury_after = client.get_protocol_fee_treasury();

        let sum_refunds    = alice_refund + bob_refund;
        let treasury_delta = treasury_after - treasury_before;

        // Fee must never be charged on cancel.
        prop_assert_eq!(treasury_delta, 0,
            "Fee was charged on cancel: treasury_delta={} fee_bps={}",
            treasury_delta, fee_bps_raw);

        // Every stroop staked must be returned.
        prop_assert!(alice_refund >= 0);
        prop_assert!(bob_refund   >= 0);
        prop_assert_eq!(sum_refunds, total_pot,
            "Cancel refund conservation violated: refunds={} pot={}",
            sum_refunds, total_pot);
    }
}

// ─── Cross-product: FeeModel × One-Sided × Precision-Tie ─────────────────
//
// These property suites exercise every combination in the matrix:
//   FeeModel{FeeOnPot, FeeOnWinnings} × OneSided{Up, Down} × PrecisionTie{none, 2-way, 3-way}
//
// Deterministic seeds: proptest prints the minimal-failing seed on any
// failure, so every regression is instantly reproducible.
//
// Readable diagnostics: every prop_assert! includes a human-readable
// message tagging the failed combination's parameters.

use crate::types::FeeModel;

fn set_fee_model_now(env: &Env, contract_id: &Address, model: FeeModel) {
    env.as_contract(contract_id, || {
        env.storage().persistent().set(&DataKeyCore::FeeModel, &model);
    });
}

fn set_fee_bps_now(env: &Env, contract_id: &Address, bps: u32) {
    env.as_contract(contract_id, || {
        env.storage().persistent().set(&DataKeyCore::ProtocolFeeBps, &bps);
    });
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Cross-product: FeeModel × one-sided UpDown pool × fee bps.
    ///
    /// Vary FeeModel (FeeOnPot / FeeOnWinnings), fee bps (disabled / enabled),
    /// one-sided direction (Up-only / Down-only), and stake amounts.
    ///
    /// Invariant: one-sided pools always refund; fee must never apply and
    /// total accounted value must equal the pot exactly.
    #[test]
    fn cross_product_fee_model_one_sided(
        a_stake in 1i128..200_000_000i128,
        b_stake in 1i128..200_000_000i128,
        fee_bps_raw in 0u32..=1_000u32,
        side in 0u8..=1u8, // 0 = Up-only, 1 = Down-only
        model in prop_oneof![Just(FeeModel::FeeOnPot), Just(FeeModel::FeeOnWinnings)],
    ) {
        let total_pot = a_stake.saturating_add(b_stake);
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
        client.create_round(&1_0000000u128, &None);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        let bet_side = if side == 0 { BetSide::Up } else { BetSide::Down };

        env.as_contract(&contract_id, || {
            let mut positions = Map::<Address, UserPosition>::new(&env);
            positions.set(alice.clone(), UserPosition { amount: a_stake, side: bet_side.clone() });
            positions.set(bob.clone(), UserPosition { amount: b_stake, side: bet_side.clone() });
            env.storage().persistent().set(&DataKeyCore::UpDownPositions, &positions);

            let mut round: Round = env.storage().persistent().get(&DataKeyCore::ActiveRound).unwrap();
            if side == 0 {
                round.pool_up = total_pot;
                round.pool_down = 0;
            } else {
                round.pool_up = 0;
                round.pool_down = total_pot;
            }
            env.storage().persistent().set(&DataKeyCore::ActiveRound, &round);

            if fee_bps_raw > 0 {
                env.storage().persistent().set(&DataKeyCore::ProtocolFeeBps, &fee_bps_raw);
            }
            env.storage().persistent().set(&DataKeyCore::FeeModel, &model);
        });

        let treasury_before = client.get_protocol_fee_treasury();

        // Resolve in the opposite direction to trigger refund path
        let final_price = if side == 0 { 900u128 } else { 1_100u128 };
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

        let alice_pay = client.get_pending_winnings(&alice);
        let bob_pay = client.get_pending_winnings(&bob);
        let treasury_delta = client.get_protocol_fee_treasury() - treasury_before;
        let accounted = alice_pay + bob_pay + treasury_delta;

        prop_assert!(alice_pay >= 0,
            "FAIL [one-sided] negative payout: alice_pay={} side={} fee_bps={} fee_model={:?}",
            alice_pay, side, fee_bps_raw, model);
        prop_assert!(bob_pay >= 0,
            "FAIL [one-sided] negative payout: bob_pay={} side={} fee_bps={} fee_model={:?}",
            bob_pay, side, fee_bps_raw, model);
        prop_assert_eq!(treasury_delta, 0,
            "FAIL [one-sided] fee charged on refund: treasury_delta={} side={} fee_bps={} fee_model={:?}",
            treasury_delta, side, fee_bps_raw, model);
        prop_assert_eq!(accounted, total_pot,
            "FAIL [one-sided] conservation violated: accounted={} pot={} side={} fee_bps={} fee_model={:?}",
            accounted, total_pot, side, fee_bps_raw, model);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Cross-product: FeeModel × precision tie (2-way) × fee bps.
    ///
    /// Two users predict at equal distance from the final price (tie),
    /// the third loses. Vary FeeModel, fee bps, and stake amounts.
    ///
    /// Invariant: precision conservation is exact (payouts + treasury == pot)
    /// regardless of which fee model is active.
    #[test]
    fn cross_product_fee_model_precision_tie(
        stake_a in 1i128..200_000_000i128,
        stake_b in 1i128..200_000_000i128,
        stake_c in 1i128..200_000_000i128,
        tie_distance in 1u128..100_000u128,
        fee_bps_raw in 0u32..=1_000u32,
        model in prop_oneof![Just(FeeModel::FeeOnPot), Just(FeeModel::FeeOnWinnings)],
    ) {
        let total_pot = stake_a.saturating_add(stake_b).saturating_add(stake_c);
        let final_price = 1_000_000u128;

        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
        client.create_round(&1_0000000u128, &Some(1));

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        // Alice and Bob tied at `tie_distance` from final_price; charlie loses
        let price_a = final_price.saturating_add(tie_distance);
        let price_b = final_price.saturating_sub(tie_distance);
        let price_c = final_price.saturating_add(tie_distance.saturating_mul(10));

        prop_assume!(price_b > 0 && price_c > 0);

        env.as_contract(&contract_id, || {
            let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);
            predictions.set(alice.clone(), PrecisionPrediction {
                user: alice.clone(), predicted_price: price_a, amount: stake_a });
            predictions.set(bob.clone(), PrecisionPrediction {
                user: bob.clone(), predicted_price: price_b, amount: stake_b });
            predictions.set(charlie.clone(), PrecisionPrediction {
                user: charlie.clone(), predicted_price: price_c, amount: stake_c });
            env.storage().persistent().set(&DataKeyCore::PrecisionPositions, &predictions);

            if fee_bps_raw > 0 {
                env.storage().persistent().set(&DataKeyCore::ProtocolFeeBps, &fee_bps_raw);
            }
            env.storage().persistent().set(&DataKeyCore::FeeModel, &model);
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
        let treasury_after = client.get_protocol_fee_treasury();
        let treasury_delta = treasury_after - treasury_before;
        let sum_payouts = alice_pending + bob_pending + charlie_pending;
        let accounted = sum_payouts + treasury_delta;

        prop_assert!(alice_pending >= 0,
            "FAIL [precision-tie] negative payout: alice={} fee_bps={} fee_model={:?} tie_dist={}",
            alice_pending, fee_bps_raw, model, tie_distance);
        prop_assert!(bob_pending >= 0,
            "FAIL [precision-tie] negative payout: bob={} fee_bps={} fee_model={:?} tie_dist={}",
            bob_pending, fee_bps_raw, model, tie_distance);
        prop_assert!(charlie_pending >= 0,
            "FAIL [precision-tie] negative payout: charlie={} fee_bps={} fee_model={:?} tie_dist={}",
            charlie_pending, fee_bps_raw, model, tie_distance);
        prop_assert!(treasury_delta >= 0,
            "FAIL [precision-tie] negative treasury: delta={} fee_bps={} fee_model={:?}",
            treasury_delta, fee_bps_raw, model);

        // Precision mode conserves exactly: no truncation slack.
        prop_assert_eq!(accounted, total_pot,
            "FAIL [precision-tie] conservation violated: accounted={} pot={} fee_bps={} fee_model={:?} tie_dist={} stakes=({},{},{})",
            accounted, total_pot, fee_bps_raw, model, tie_distance,
            stake_a, stake_b, stake_c);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Cross-product: FeeModel × precision three-way tie × fee bps.
    ///
    /// All three users predicted at equal distance from the final price.
    /// Vary FeeModel, fee bps, and stake amounts.
    ///
    /// Invariant: precision conservation is exact.
    #[test]
    fn cross_product_fee_model_precision_three_way_tie(
        stake_a in 1i128..200_000_000i128,
        stake_b in 1i128..200_000_000i128,
        stake_c in 1i128..200_000_000i128,
        tie_distance in 1u128..100_000u128,
        fee_bps_raw in 0u32..=1_000u32,
        model in prop_oneof![Just(FeeModel::FeeOnPot), Just(FeeModel::FeeOnWinnings)],
    ) {
        let total_pot = stake_a.saturating_add(stake_b).saturating_add(stake_c);
        let final_price = 1_000_000u128;

        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
        client.create_round(&1_0000000u128, &Some(1));

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        // All three tied at tie_distance from final_price
        let price_a = final_price.saturating_add(tie_distance);
        let price_b = final_price.saturating_sub(tie_distance);
        // Third price must also be at the same distance; choose an alternative
        // path that equals the same distance but from a different direction:
        // price_c = final_price + tie_distance would duplicate price_a,
        // so instead we verify the tie_distance constraint is satisfied.
        prop_assume!(price_b > 0);

        env.as_contract(&contract_id, || {
            let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);
            // Alice and Bob tied at tie_distance; charlie is third-way tie
            // (exact same distance from final_price on the same side as Alice)
            // but different price to avoid duplicate address
            predictions.set(alice.clone(), PrecisionPrediction {
                user: alice.clone(), predicted_price: price_a, amount: stake_a });
            predictions.set(bob.clone(), PrecisionPrediction {
                user: bob.clone(), predicted_price: price_b, amount: stake_b });
            predictions.set(charlie.clone(), PrecisionPrediction {
                user: charlie.clone(), predicted_price: price_a, amount: stake_c });
            env.storage().persistent().set(&DataKeyCore::PrecisionPositions, &predictions);

            if fee_bps_raw > 0 {
                env.storage().persistent().set(&DataKeyCore::ProtocolFeeBps, &fee_bps_raw);
            }
            env.storage().persistent().set(&DataKeyCore::FeeModel, &model);
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
        let accounted = alice_pending + bob_pending + charlie_pending + treasury_delta;

        prop_assert!(alice_pending >= 0,
            "FAIL [3-way-tie] negative payout: alice={} fee_bps={} fee_model={:?}",
            alice_pending, fee_bps_raw, model);
        prop_assert!(bob_pending >= 0,
            "FAIL [3-way-tie] negative payout: bob={} fee_bps={} fee_model={:?}",
            bob_pending, fee_bps_raw, model);
        prop_assert!(charlie_pending >= 0,
            "FAIL [3-way-tie] negative payout: charlie={} fee_bps={} fee_model={:?}",
            charlie_pending, fee_bps_raw, model);
        prop_assert!(treasury_delta >= 0,
            "FAIL [3-way-tie] negative treasury: delta={} fee_bps={} fee_model={:?}",
            treasury_delta, fee_bps_raw, model);

        prop_assert_eq!(accounted, total_pot,
            "FAIL [3-way-tie] conservation violated: accounted={} pot={} fee_bps={} fee_model={:?} tie_dist={} stakes=({},{},{})",
            accounted, total_pot, fee_bps_raw, model, tie_distance,
            stake_a, stake_b, stake_c);
    }
}
