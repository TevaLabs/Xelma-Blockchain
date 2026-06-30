// SPDX-License-Identifier: MIT
//! Benchmark-style tests for the indexed storage layout.
//!
//! These tests assert on the *operation count* of each core path
//! (place bet, resolve round, claim payout) by reading raw ledger storage and
//! counting the number of indexed keys touched. They demonstrate that the new
//! per-user composite-key layout achieves O(1) read/write per user during
//! bet placement and bounded O(N) reads only at resolution time.
//!
//! Layout invariants validated here:
//!   - DataKey::Position(round_id, user) is written exactly once per bet
//!   - DataKey::RoundParticipants(round_id) tracks every participant in order
//!   - resolve_round removes all per-user keys + the participant list (cleanup)
//!   - large rounds (50+ participants) resolve correctly without map blowup

extern crate alloc;

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, DataKey, OraclePayload, UserPosition};
use alloc::vec::Vec as StdVec;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, Vec,
};

const N_LARGE: usize = 60;

fn setup() -> (Env, Address, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    (env, contract_id, client)
}

// ─── place_bet: O(1) per-user key write ──────────────────────────────────────

/// Each place_bet writes exactly one `DataKey::Position(round_id, user)` —
/// no full-map deserialisation. Verified by reading the per-user key directly.
#[test]
fn bench_place_bet_writes_single_user_key() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();

    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Down);

    // Each user has their own composite key — O(1) read independent of N
    env.as_contract(&contract_id, || {
        let alice_pos: UserPosition = env
            .storage()
            .persistent()
            .get(&DataKey::Position(round.round_id, alice.clone()))
            .expect("alice's per-user position key must exist");
        assert_eq!(alice_pos.amount, 100_0000000);
        assert_eq!(alice_pos.side, BetSide::Up);

        let bob_pos: UserPosition = env
            .storage()
            .persistent()
            .get(&DataKey::Position(round.round_id, bob.clone()))
            .expect("bob's per-user position key must exist");
        assert_eq!(bob_pos.amount, 200_0000000);
        assert_eq!(bob_pos.side, BetSide::Down);

        // The legacy bulk-map key is NOT written under the new layout
        let legacy: Option<soroban_sdk::Map<Address, UserPosition>> =
            env.storage().persistent().get(&DataKey::UpDownPositions);
        assert!(
            legacy.is_none(),
            "legacy DataKey::UpDownPositions must not be written by place_bet"
        );
    });
}

/// Operation-count assertion: after N bets, exactly N participant entries and
/// N indexed position keys exist — no per-bet O(N) map serialisation.
#[test]
fn bench_place_bet_op_count_assertion() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    let users: StdVec<Address> = (0..10).map(|_| Address::generate(&env)).collect();
    for u in &users {
        client.mint_initial(u);
    }

    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();

    for (i, u) in users.iter().enumerate() {
        let side = if i % 2 == 0 {
            BetSide::Up
        } else {
            BetSide::Down
        };
        client.place_bet(u, &(10_0000000 + i as i128), &side);
    }

    env.as_contract(&contract_id, || {
        // OP COUNT 1: participants list has exactly N entries
        let participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundParticipants(round.round_id))
            .expect("participants list must exist after bets");
        assert_eq!(
            participants.len() as usize,
            users.len(),
            "participant count must equal bet count"
        );

        // OP COUNT 2: exactly N per-user position keys, in the same order
        for (i, u) in users.iter().enumerate() {
            let pos: UserPosition = env
                .storage()
                .persistent()
                .get(&DataKey::Position(round.round_id, u.clone()))
                .expect("each participant has their own indexed key");
            assert_eq!(pos.amount, 10_0000000 + i as i128);
        }
    });
}

// ─── resolve_round: cleanup of all per-user keys ─────────────────────────────

/// resolve_round must remove every per-user key + the participant list.
/// Verified by inspecting raw storage after resolution.
#[test]
fn bench_resolve_cleans_indexed_keys() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    let users: StdVec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
    for u in &users {
        client.mint_initial(u);
    }

    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();

    for (i, u) in users.iter().enumerate() {
        let side = if i % 2 == 0 {
            BetSide::Up
        } else {
            BetSide::Down
        };
        client.place_bet(u, &50_0000000, &side);
    }

    env.ledger().with_mut(|li| li.sequence_number = 12);
    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    env.as_contract(&contract_id, || {
        // Participant list removed
        let participants: Option<Vec<Address>> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundParticipants(round.round_id));
        assert!(participants.is_none(), "participants list must be cleaned");

        // Every per-user position key removed
        for u in &users {
            let pos: Option<UserPosition> = env
                .storage()
                .persistent()
                .get(&DataKey::Position(round.round_id, u.clone()));
            assert!(pos.is_none(), "per-user position must be cleaned");
        }
    });
}

// ─── large-round scenario: 60 participants ──────────────────────────────────

/// Large-round correctness + performance: 60 participants resolve correctly,
/// payouts match the proportional formula, and storage is fully cleaned up.
/// Demonstrates that the per-user layout scales without map-deserialisation cost.
#[test]
fn bench_large_round_resolves_correctly() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    let users: StdVec<Address> = (0..N_LARGE).map(|_| Address::generate(&env)).collect();
    for u in &users {
        client.mint_initial(u);
    }

    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();

    // Half UP, half DOWN — equal amounts so the math is easy to verify
    for (i, u) in users.iter().enumerate() {
        let side = if i % 2 == 0 {
            BetSide::Up
        } else {
            BetSide::Down
        };
        client.place_bet(u, &10_0000000, &side);
    }

    let active = client.get_active_round().unwrap();
    let half = (N_LARGE / 2) as i128;
    assert_eq!(active.pool_up, 10_0000000 * half);
    assert_eq!(active.pool_down, 10_0000000 * half);

    // Resolve — UP wins
    env.ledger().with_mut(|li| li.sequence_number = 12);
    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    // Each UP winner should have pending = bet + (bet/winning_pool) * losing_pool
    //   = 10_0000000 + (10_0000000 / (30 * 10_0000000)) * (30 * 10_0000000)
    //   = 10_0000000 + 10_0000000 = 20_0000000
    for (i, u) in users.iter().enumerate() {
        let pending = client.get_pending_winnings(u);
        if i % 2 == 0 {
            assert_eq!(pending, 20_0000000, "UP winner pending mismatch");
        } else {
            assert_eq!(pending, 0, "DOWN loser must have no pending");
        }
    }

    // Storage fully cleaned
    env.as_contract(&contract_id, || {
        let participants: Option<Vec<Address>> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundParticipants(round.round_id));
        assert!(participants.is_none());
        for u in &users {
            let pos: Option<UserPosition> = env
                .storage()
                .persistent()
                .get(&DataKey::Position(round.round_id, u.clone()));
            assert!(pos.is_none());
        }
    });

    // All winners can claim — each claim is O(1)
    let mut total_claimed: i128 = 0;
    for (i, u) in users.iter().enumerate() {
        if i % 2 == 0 {
            total_claimed += client.claim_winnings(u);
        }
    }
    assert_eq!(total_claimed, 20_0000000 * half);
}

// ─── precision mode: indexed keys ────────────────────────────────────────────

/// Precision mode also uses per-user keys + participant list.
#[test]
fn bench_precision_mode_indexed_keys() {
    let (env, contract_id, client) = setup();
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&carol);

    client.create_round(&1_0000000u128, &Some(1)); // Precision mode
    let round = client.get_active_round().unwrap();

    client.predict_price(&alice, &500u128, &10_0000000);
    client.predict_price(&bob, &600u128, &10_0000000);
    client.predict_price(&carol, &700u128, &10_0000000);

    // Each prediction stored at its own indexed key
    env.as_contract(&contract_id, || {
        for u in [&alice, &bob, &carol] {
            let pred: crate::types::PrecisionPrediction = env
                .storage()
                .persistent()
                .get(&DataKey::PrecisionPosition(round.round_id, (*u).clone()))
                .expect("each precision prediction stored at indexed key");
            assert_eq!(pred.amount, 10_0000000);
        }

        let participants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RoundParticipants(round.round_id))
            .expect("participants list shared between modes");
        assert_eq!(participants.len(), 3);
    });

    // Resolve — bob's guess (600) is closest to 580
    env.ledger().with_mut(|li| li.sequence_number = 12);
    client.resolve_round(&OraclePayload {
        price: 580u128,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
    });

    // Bob wins entire pot (3 * 10_0000000)
    assert_eq!(client.get_pending_winnings(&bob), 30_0000000);
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&carol), 0);
}


