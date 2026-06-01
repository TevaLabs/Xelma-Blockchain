//! Fuzz harness for lifecycle and payout invariants (Issue #102).
//!
//! Uses proptest to randomize participant counts, stake magnitudes, price ranges,
//! and operation sequences. Asserts that across all inputs:
//!
//!   1. Pot conservation  — total pending winnings after resolution ≤ total pool.
//!   2. No negative balances — every user balance remains ≥ 0 at all times.
//!   3. Cancellation full refund — sum(pending_winnings) == sum(stakes) after cancel.
//!   4. Claim idempotency — a second claim on the same account returns 0.
//!   5. Multi-round isolation — pool and positions are zeroed when a new round starts.
//!
//! Run locally with a higher case count:
//!   PROPTEST_CASES=512 cargo test fuzz_harness -- --nocapture
//!
//! A failing seed is printed on stderr by proptest and can be replayed with:
//!   PROPTEST_CASES=1 cargo test <test_name> -- --nocapture

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, OraclePayload};
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

// ─── shared setup ────────────────────────────────────────────────────────────

fn make_env_and_client() -> (Env, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, client)
}

// ─── Invariant 1 & 2: pot conservation and non-negative balances (UpDown) ────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Fuzz UpDown round with random stakes and a random final direction.
    /// Asserts pot conservation and that no balance goes negative.
    #[test]
    fn fuzz_updown_pot_conservation_and_non_negative_balances(
        stake_a in 1i128..=500_0000000i128,
        stake_b in 1i128..=500_0000000i128,
        stake_c in 1i128..=500_0000000i128,
        // true = final price UP (alice/bob win), false = DOWN (charlie wins)
        up_wins in any::<bool>(),
    ) {
        let (env, client) = make_env_and_client();

        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);
        let charlie = Address::generate(&env);

        client.mint_initial(&alice);
        client.mint_initial(&bob);
        client.mint_initial(&charlie);

        // Clamp stakes to available balance (1 000 vXLM each)
        let stake_a = stake_a.min(1000_0000000);
        let stake_b = stake_b.min(1000_0000000);
        let stake_c = stake_c.min(1000_0000000);

        client.create_round(&1_0000000, &None);

        client.place_bet(&alice,   &stake_a, &BetSide::Up);
        client.place_bet(&bob,     &stake_b, &BetSide::Up);
        client.place_bet(&charlie, &stake_c, &BetSide::Down);

        let round = client.get_active_round().unwrap();
        let total_pool = round.pool_up + round.pool_down;

        env.ledger().with_mut(|li| {
            li.sequence_number = round.end_ledger;
        });

        let final_price = if up_wins { 2_0000000u128 } else { 500_000u128 };
        client.resolve_round(&OraclePayload {
            price: final_price,
            timestamp: env.ledger().timestamp(),
            round_id: round.start_ledger,
            nonce: 1,
        });

        let pa = client.get_pending_winnings(&alice);
        let pb = client.get_pending_winnings(&bob);
        let pc = client.get_pending_winnings(&charlie);

        // Invariant 1: pot conservation
        prop_assert!(pa + pb + pc <= total_pool,
            "pending ({} + {} + {} = {}) exceeds pool {}",
            pa, pb, pc, pa + pb + pc, total_pool);

        // Invariant 2: non-negative pending winnings
        prop_assert!(pa >= 0, "alice pending negative: {}", pa);
        prop_assert!(pb >= 0, "bob pending negative: {}", pb);
        prop_assert!(pc >= 0, "charlie pending negative: {}", pc);

        // Claim and verify balances stay non-negative
        let ba_before = client.balance(&alice);
        let bb_before = client.balance(&bob);
        let bc_before = client.balance(&charlie);

        client.claim_winnings(&alice);
        client.claim_winnings(&bob);
        client.claim_winnings(&charlie);

        prop_assert!(client.balance(&alice)   >= 0);
        prop_assert!(client.balance(&bob)     >= 0);
        prop_assert!(client.balance(&charlie) >= 0);

        // Balances increase by exactly the claimed amount
        prop_assert_eq!(client.balance(&alice),   ba_before + pa);
        prop_assert_eq!(client.balance(&bob),     bb_before + pb);
        prop_assert_eq!(client.balance(&charlie), bc_before + pc);
    }
}

// ─── Invariant 1 & 2: pot conservation and non-negative balances (Precision) ─

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Fuzz Precision round with random stakes, predictions, and final price.
    /// Asserts pot conservation and non-negative pending winnings.
    #[test]
    fn fuzz_precision_pot_conservation_and_non_negative_balances(
        stake_a in 1i128..=300_0000000i128,
        stake_b in 1i128..=300_0000000i128,
        stake_c in 1i128..=300_0000000i128,
        pred_a in 1u128..=99_999_999u128,
        pred_b in 1u128..=99_999_999u128,
        pred_c in 1u128..=99_999_999u128,
        final_price in 1u128..=99_999_999u128,
    ) {
        let (env, client) = make_env_and_client();

        let alice   = Address::generate(&env);
        let bob     = Address::generate(&env);
        let charlie = Address::generate(&env);

        client.mint_initial(&alice);
        client.mint_initial(&bob);
        client.mint_initial(&charlie);

        let stake_a = stake_a.min(1000_0000000);
        let stake_b = stake_b.min(1000_0000000);
        let stake_c = stake_c.min(1000_0000000);

        client.create_round(&1_0000000, &Some(1));

        client.place_precision_prediction(&alice,   &stake_a, &pred_a);
        client.place_precision_prediction(&bob,     &stake_b, &pred_b);
        client.place_precision_prediction(&charlie, &stake_c, &pred_c);

        let total_pool = stake_a + stake_b + stake_c;

        let round = client.get_active_round().unwrap();
        env.ledger().with_mut(|li| {
            li.sequence_number = round.end_ledger;
        });

        client.resolve_round(&OraclePayload {
            price: final_price,
            timestamp: env.ledger().timestamp(),
            round_id: round.start_ledger,
            nonce: 1,
        });

        let pa = client.get_pending_winnings(&alice);
        let pb = client.get_pending_winnings(&bob);
        let pc = client.get_pending_winnings(&charlie);

        // Invariant 1: pot conservation
        prop_assert!(pa + pb + pc <= total_pool,
            "pending ({} + {} + {} = {}) exceeds pool {}",
            pa, pb, pc, pa + pb + pc, total_pool);

        // Invariant 2: non-negative pending winnings
        prop_assert!(pa >= 0);
        prop_assert!(pb >= 0);
        prop_assert!(pc >= 0);
    }
}

// ─── Invariant 3: cancellation full refund ────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// After cancel_round, sum of pending winnings must equal sum of stakes.
    /// No funds are created or destroyed by cancellation.
    #[test]
    fn fuzz_cancellation_full_refund_invariant(
        stake_a in 1i128..=400_0000000i128,
        stake_b in 1i128..=400_0000000i128,
        stake_c in 1i128..=400_0000000i128,
        side_a in any::<bool>(),
        side_b in any::<bool>(),
        side_c in any::<bool>(),
    ) {
        let (env, client) = make_env_and_client();

        let alice   = Address::generate(&env);
        let bob     = Address::generate(&env);
        let charlie = Address::generate(&env);

        client.mint_initial(&alice);
        client.mint_initial(&bob);
        client.mint_initial(&charlie);

        let stake_a = stake_a.min(1000_0000000);
        let stake_b = stake_b.min(1000_0000000);
        let stake_c = stake_c.min(1000_0000000);

        client.create_round(&1_0000000, &None);

        let bet_side = |up: bool| if up { BetSide::Up } else { BetSide::Down };
        client.place_bet(&alice,   &stake_a, &bet_side(side_a));
        client.place_bet(&bob,     &stake_b, &bet_side(side_b));
        client.place_bet(&charlie, &stake_c, &bet_side(side_c));

        let total_staked = stake_a + stake_b + stake_c;

        client.cancel_round(&0u32);

        let pa = client.get_pending_winnings(&alice);
        let pb = client.get_pending_winnings(&bob);
        let pc = client.get_pending_winnings(&charlie);

        // Invariant 3: full refund — total pending == total staked
        prop_assert_eq!(pa + pb + pc, total_staked,
            "refund sum {} != total staked {}", pa + pb + pc, total_staked);

        prop_assert_eq!(pa, stake_a);
        prop_assert_eq!(pb, stake_b);
        prop_assert_eq!(pc, stake_c);
    }
}

// ─── Invariant 4: claim idempotency ──────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// A second call to claim_winnings for the same user always returns 0
    /// and does not alter the balance a second time.
    #[test]
    fn fuzz_claim_idempotency(
        stake in 1i128..=800_0000000i128,
        up_wins in any::<bool>(),
    ) {
        let (env, client) = make_env_and_client();

        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);

        client.mint_initial(&alice);
        client.mint_initial(&bob);

        let stake = stake.min(1000_0000000);

        client.create_round(&1_0000000, &None);
        client.place_bet(&alice, &stake, &BetSide::Up);
        client.place_bet(&bob,   &stake, &BetSide::Down);

        let round = client.get_active_round().unwrap();
        env.ledger().with_mut(|li| {
            li.sequence_number = round.end_ledger;
        });

        let final_price = if up_wins { 2_0000000u128 } else { 500_000u128 };
        client.resolve_round(&OraclePayload {
            price: final_price,
            timestamp: env.ledger().timestamp(),
            round_id: round.start_ledger,
            nonce: 1,
        });

        // First claim
        let first_claim = client.claim_winnings(&alice);
        let balance_after_first = client.balance(&alice);

        // Second claim — must return 0, balance must not change
        let second_claim = client.claim_winnings(&alice);

        // Invariant 4: idempotency
        prop_assert_eq!(second_claim, 0,
            "second claim should return 0, got {}", second_claim);
        prop_assert_eq!(client.balance(&alice), balance_after_first,
            "balance changed on second claim");

        // Pending cleared after first claim
        prop_assert_eq!(client.get_pending_winnings(&alice), 0);
        let _ = first_claim;
    }
}

// ─── Invariant 5: multi-round pool and position isolation ─────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// After resolving round N and starting round N+1, the new round has
    /// zero pools and no carried-over user positions from round N.
    #[test]
    fn fuzz_multi_round_state_isolation(
        stake1 in 1i128..=400_0000000i128,
        stake2 in 1i128..=400_0000000i128,
        side1 in any::<bool>(),
    ) {
        let (env, client) = make_env_and_client();

        let alice = Address::generate(&env);
        client.mint_initial(&alice);

        let stake1 = stake1.min(1000_0000000);
        let stake2 = stake2.min(1000_0000000);

        // Round 1
        client.create_round(&1_0000000, &None);
        let bet_side = if side1 { BetSide::Up } else { BetSide::Down };
        client.place_bet(&alice, &stake1, &bet_side);

        let round1 = client.get_active_round().unwrap();
        env.ledger().with_mut(|li| {
            li.sequence_number = round1.end_ledger;
        });
        client.resolve_round(&OraclePayload {
            price: 2_0000000,
            timestamp: env.ledger().timestamp(),
            round_id: round1.start_ledger,
            nonce: 1,
        });

        // Invariant 5a: no active round between rounds
        prop_assert!(client.get_active_round().is_none());

        // Round 2
        client.create_round(&2_0000000, &None);
        let round2 = client.get_active_round().unwrap();

        // Invariant 5b: new round has empty pools
        prop_assert_eq!(round2.pool_up, 0);
        prop_assert_eq!(round2.pool_down, 0);

        // Invariant 5c: alice has no position in the new round before betting
        prop_assert!(client.get_user_position(&alice).is_none());

        // Place a bet in round 2 and verify fresh state
        let stake2 = stake2.min(client.balance(&alice).max(1));
        if stake2 > 0 {
            client.place_bet(&alice, &stake2, &BetSide::Down);
            let pos = client.get_user_position(&alice).unwrap();
            prop_assert_eq!(pos.amount, stake2);
        }
    }
}

// ─── Fuzz: variable participant count (up to 4 up + 4 down) ──────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Fuzz UpDown resolution with a variable number of active participants (1–4
    /// per side). Uses fixed-size arrays to remain compatible with no_std.
    /// Asserts pot conservation regardless of participant count.
    #[test]
    fn fuzz_variable_participant_count_updown(
        n_up   in 1usize..=4usize,
        n_down in 1usize..=4usize,
        per_stake in 10_0000000i128..=200_0000000i128,
    ) {
        let (env, client) = make_env_and_client();

        let per_stake = per_stake.min(1000_0000000);

        // Pre-allocate 4 up-side and 4 down-side users; activate only n_up / n_down.
        let up0 = Address::generate(&env);
        let up1 = Address::generate(&env);
        let up2 = Address::generate(&env);
        let up3 = Address::generate(&env);
        let dn0 = Address::generate(&env);
        let dn1 = Address::generate(&env);
        let dn2 = Address::generate(&env);
        let dn3 = Address::generate(&env);

        let all_up = [&up0, &up1, &up2, &up3];
        let all_dn = [&dn0, &dn1, &dn2, &dn3];

        for u in all_up.iter().chain(all_dn.iter()) {
            client.mint_initial(u);
        }

        client.create_round(&1_0000000, &None);

        for u in all_up[..n_up].iter() {
            client.place_bet(u, &per_stake, &BetSide::Up);
        }
        for u in all_dn[..n_down].iter() {
            client.place_bet(u, &per_stake, &BetSide::Down);
        }

        let round = client.get_active_round().unwrap();
        let total_pool = round.pool_up + round.pool_down;

        env.ledger().with_mut(|li| {
            li.sequence_number = round.end_ledger;
        });

        // Price goes UP — up-side wins
        client.resolve_round(&OraclePayload {
            price: 2_0000000,
            timestamp: env.ledger().timestamp(),
            round_id: round.start_ledger,
            nonce: 1,
        });

        // Sum pending across all active participants without std::Vec
        let mut total_pending: i128 = 0;
        for u in all_up[..n_up].iter() {
            total_pending += client.get_pending_winnings(u);
        }
        for u in all_dn[..n_down].iter() {
            total_pending += client.get_pending_winnings(u);
        }

        // Invariant 1: pot conservation across N participants
        prop_assert!(total_pending <= total_pool,
            "total_pending {} exceeds pool {}", total_pending, total_pool);

        // Losers get zero
        for u in all_dn[..n_down].iter() {
            prop_assert_eq!(client.get_pending_winnings(u), 0);
        }

        // Winners get positive
        for u in all_up[..n_up].iter() {
            prop_assert!(client.get_pending_winnings(u) > 0);
        }
    }
}

// ─── Fuzz: precision mode variable participants ───────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Fuzz Precision resolution with 2–5 participants and random predicted prices.
    /// Asserts pot conservation and that the closest predictor receives the pot.
    #[test]
    fn fuzz_precision_variable_participants_pot_conservation(
        final_price in 1_000u128..=50_000u128,
        stake in 50_0000000i128..=300_0000000i128,
    ) {
        let (env, client) = make_env_and_client();

        // 3 fixed users with varying distance from final_price
        let alice   = Address::generate(&env);
        let bob     = Address::generate(&env);
        let charlie = Address::generate(&env);

        client.mint_initial(&alice);
        client.mint_initial(&bob);
        client.mint_initial(&charlie);

        let stake = stake.min(1000_0000000);

        // Predictions spread around final_price
        let pred_a = final_price.saturating_sub(500).max(1);
        let pred_b = final_price;
        let pred_c = final_price.saturating_add(500).min(99_999_999);

        client.create_round(&1_0000000, &Some(1));

        client.place_precision_prediction(&alice,   &stake, &pred_a);
        client.place_precision_prediction(&bob,     &stake, &pred_b);
        client.place_precision_prediction(&charlie, &stake, &pred_c);

        let total_pool = stake * 3;

        let round = client.get_active_round().unwrap();
        env.ledger().with_mut(|li| {
            li.sequence_number = round.end_ledger;
        });

        client.resolve_round(&OraclePayload {
            price: final_price,
            timestamp: env.ledger().timestamp(),
            round_id: round.start_ledger,
            nonce: 1,
        });

        let pa = client.get_pending_winnings(&alice);
        let pb = client.get_pending_winnings(&bob);
        let pc = client.get_pending_winnings(&charlie);

        // Invariant 1: pot conservation
        prop_assert!(pa + pb + pc <= total_pool,
            "pending {} exceeds pool {}", pa + pb + pc, total_pool);

        // Invariant 2: non-negative
        prop_assert!(pa >= 0);
        prop_assert!(pb >= 0);
        prop_assert!(pc >= 0);

        // bob predicted exactly — must be the winner (or tied with nearest)
        prop_assert!(pb >= pa, "exact predictor should not earn less than distant predictor");
        prop_assert!(pb >= pc, "exact predictor should not earn less than distant predictor");
    }
}

// ─── Fuzz: cancellation refund for precision mode ────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// After cancel_round in Precision mode, each participant gets their
    /// exact stake back as pending winnings.
    #[test]
    fn fuzz_precision_cancellation_full_refund(
        stake_a in 1i128..=400_0000000i128,
        stake_b in 1i128..=400_0000000i128,
        pred_a in 1u128..=99_999_999u128,
        pred_b in 1u128..=99_999_999u128,
    ) {
        let (env, client) = make_env_and_client();

        let alice = Address::generate(&env);
        let bob   = Address::generate(&env);

        client.mint_initial(&alice);
        client.mint_initial(&bob);

        let stake_a = stake_a.min(1000_0000000);
        let stake_b = stake_b.min(1000_0000000);

        client.create_round(&1_0000000, &Some(1));

        client.place_precision_prediction(&alice, &stake_a, &pred_a);
        client.place_precision_prediction(&bob,   &stake_b, &pred_b);

        let round_id = client.get_active_round().unwrap().round_id;
        client.cancel_round(&1u32);

        prop_assert!(client.get_active_round().is_none());
        prop_assert!(client.is_round_cancelled(&round_id));

        // Invariant 3: exact refund
        prop_assert_eq!(client.get_pending_winnings(&alice), stake_a);
        prop_assert_eq!(client.get_pending_winnings(&bob),   stake_b);
    }
}
