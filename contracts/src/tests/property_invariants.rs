// SPDX-License-Identifier: MIT
//! Property-based tests for payout invariants.
//!
//! These tests exercise randomized scenarios to ensure core invariants such as:
//! - Conservation of value (no payouts exceed the total pot)
//! - Non-negative pending winnings and balances
//! - Monotonic user statistics (wins, losses, and best streak never decrease)

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{
    BetSide, DataKey, OraclePayload, PrecisionPrediction, Round, UserPosition, UserStats,
};
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, Map,
};

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

            env.storage().persistent().set(&DataKey::UpDownPositions, &positions);

            let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
            round.pool_up = total_up;
            round.pool_down = total_down;
            env.storage().persistent().set(&DataKey::ActiveRound, &round);
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
        });

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
                .set(&DataKey::PrecisionPositions, &predictions);
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
        });

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


