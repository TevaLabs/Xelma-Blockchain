// SPDX-License-Identifier: MIT
//! Gas/cost benchmark baselines with regression guardrails (Issue #121).
//!
//! These benchmarks measure the host CPU-instruction and memory cost of each
//! critical contract path and assert it stays within a documented ceiling.
//! They give maintainers an early-warning gate against performance drift as
//! features evolve.
//!
//! Paths covered: `create_round`, `place_bet`, `place_precision_prediction`
//! (precision submit), `resolve_round`, `claim_winnings`, and bounded paginated reads.
//!
//! ## Baselines and tolerances
//!
//! Each ceiling is anchored to the standard Soroban per-transaction resource
//! budget — every critical path must fit inside a single on-chain transaction.
//! See `contracts/BENCHMARKS.md` for the recorded baselines and the procedure
//! for tightening them toward true regression detection. Run locally with:
//!
//! ```text
//! cargo test --package xelma-contract cost_benchmarks -- --nocapture
//! ```
//!
//! The `--nocapture` flag surfaces the measured numbers so CI can report drift
//! even when a run stays under the ceiling (warn-on-regression).

extern crate std;

use crate::common::LEADERBOARD_LIMIT;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, OraclePayload};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

// ─── Baseline ceilings (CPU instructions, memory bytes) ──────────────────────
// Anchored to the standard Soroban per-transaction resource budget: every
// critical path must comfortably fit inside one on-chain transaction. A path
// that breaches these ceilings is a hard regression (it would fail on-chain).
// The `--nocapture` output records the actual per-path cost so maintainers can
// tighten these toward the recorded baselines in BENCHMARKS.md over time.
const TX_CPU_BUDGET: u64 = 100_000_000; // standard CPU instruction limit
const TX_MEM_BUDGET: u64 = 104_857_600; // standard 100 MiB memory limit

const CREATE_ROUND_CPU_MAX: u64 = TX_CPU_BUDGET;
const CREATE_ROUND_MEM_MAX: u64 = TX_MEM_BUDGET;
const PLACE_BET_CPU_MAX: u64 = TX_CPU_BUDGET;
const PLACE_BET_MEM_MAX: u64 = TX_MEM_BUDGET;
const PRECISION_SUBMIT_CPU_MAX: u64 = TX_CPU_BUDGET;
const PRECISION_SUBMIT_MEM_MAX: u64 = TX_MEM_BUDGET;
const RESOLVE_CPU_MAX: u64 = TX_CPU_BUDGET;
const RESOLVE_MEM_MAX: u64 = TX_MEM_BUDGET;
const CLAIM_CPU_MAX: u64 = TX_CPU_BUDGET;
const CLAIM_MEM_MAX: u64 = TX_MEM_BUDGET;

/// Measures the host CPU-instruction and memory cost of a single closure.
///
/// The budget is reset to unlimited before the call so measurement itself
/// never trips a resource limit; we read the accumulated cost afterwards.
fn measure<T>(env: &Env, f: impl FnOnce() -> T) -> (u64, u64, T) {
    let mut budget = env.cost_estimate().budget();
    budget.reset_unlimited();
    let out = f();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();
    (cpu, mem, out)
}

fn report(label: &str, cpu: u64, mem: u64) {
    std::println!("[cost-benchmark] name={label} cpu_instructions={cpu} memory_bytes={mem}");
    std::println!("| `{label}` | `{cpu}` | `{mem}` |");
}

fn setup() -> (
    Env,
    Address,
    Address,
    Address,
    VirtualTokenContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    (env, contract_id, admin, oracle, client)
}

#[test]
fn bench_cost_create_round() {
    let (env, _cid, _admin, _oracle, client) = setup();
    let (cpu, mem, _) = measure(&env, || client.create_round(&1_0000000u128, &None));
    report("create_round", cpu, mem);
    assert!(
        cpu <= CREATE_ROUND_CPU_MAX,
        "create_round CPU regression: {cpu}"
    );
    assert!(
        mem <= CREATE_ROUND_MEM_MAX,
        "create_round MEM regression: {mem}"
    );
}

#[test]
fn bench_cost_place_bet() {
    let (env, _cid, _admin, _oracle, client) = setup();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&1_0000000u128, &None);

    let (cpu, mem, _) = measure(&env, || {
        client.place_bet(&alice, &100_0000000, &BetSide::Up)
    });
    report("place_bet", cpu, mem);
    assert!(cpu <= PLACE_BET_CPU_MAX, "place_bet CPU regression: {cpu}");
    assert!(mem <= PLACE_BET_MEM_MAX, "place_bet MEM regression: {mem}");
}

#[test]
fn bench_cost_precision_submit() {
    let (env, _cid, _admin, _oracle, client) = setup();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&1_0000000u128, &Some(1)); // Precision mode

    let (cpu, mem, _) = measure(&env, || client.predict_price(&alice, &500u128, &10_0000000));
    report("precision_submit", cpu, mem);
    assert!(
        cpu <= PRECISION_SUBMIT_CPU_MAX,
        "precision_submit CPU regression: {cpu}"
    );
    assert!(
        mem <= PRECISION_SUBMIT_MEM_MAX,
        "precision_submit MEM regression: {mem}"
    );
}

#[test]
fn bench_cost_resolve_round() {
    let (env, contract_id, _admin, _oracle, client) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();
    client.place_bet(&alice, &50_0000000, &BetSide::Up);
    client.place_bet(&bob, &50_0000000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let payload = OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    };
    let (cpu, mem, _) = measure(&env, || client.resolve_round(&payload));
    report("resolve_round", cpu, mem);
    assert!(
        cpu <= RESOLVE_CPU_MAX,
        "resolve_round CPU regression: {cpu}"
    );
    assert!(
        mem <= RESOLVE_MEM_MAX,
        "resolve_round MEM regression: {mem}"
    );
}

#[test]
fn bench_cost_claim_winnings() {
    let (env, contract_id, _admin, _oracle, client) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();
    client.place_bet(&alice, &50_0000000, &BetSide::Up);
    client.place_bet(&bob, &50_0000000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    client.resolve_round(&OraclePayload {
        price: 2_0000000, // UP wins → alice has pending winnings
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    let (cpu, mem, claimed) = measure(&env, || client.claim_winnings(&alice));
    report("claim_winnings", cpu, mem);
    assert!(claimed > 0, "alice should have winnings to claim");
    assert!(cpu <= CLAIM_CPU_MAX, "claim_winnings CPU regression: {cpu}");
    assert!(mem <= CLAIM_MEM_MAX, "claim_winnings MEM regression: {mem}");
}

#[test]
fn bench_cost_get_updown_positions_page() {
    let (env, _cid, _admin, _oracle, client) = setup();
    client.create_round(&1_0000000u128, &None);
    for i in 0..10 {
        let user = Address::generate(&env);
        client.mint_initial(&user);
        let side = if i % 2 == 0 {
            BetSide::Up
        } else {
            BetSide::Down
        };
        client.place_bet(&user, &10_0000000, &side);
    }

    let (cpu, mem, page) = measure(&env, || client.get_updown_positions_page(&0, &10));
    report("get_updown_positions_page", cpu, mem);
    assert_eq!(page.len(), 10);
    assert!(
        cpu <= TX_CPU_BUDGET,
        "get_updown_positions_page CPU regression: {cpu}"
    );
    assert!(
        mem <= TX_MEM_BUDGET,
        "get_updown_positions_page MEM regression: {mem}"
    );
}

#[test]
fn bench_cost_get_precision_predictions_page() {
    let (env, _cid, _admin, _oracle, client) = setup();
    client.create_round(&1_0000000u128, &Some(1));
    for i in 0..10u128 {
        let user = Address::generate(&env);
        client.mint_initial(&user);
        client.predict_price(&user, &(1_0000000 + i), &10_0000000);
    }

    let (cpu, mem, page) = measure(&env, || client.get_precision_predictions_page(&0, &10));
    report("get_precision_predictions_page", cpu, mem);
    assert_eq!(page.len(), 10);
    assert!(
        cpu <= TX_CPU_BUDGET,
        "get_precision_predictions_page CPU regression: {cpu}"
    );
    assert!(
        mem <= TX_MEM_BUDGET,
        "get_precision_predictions_page MEM regression: {mem}"
    );
}

#[test]
fn bench_cost_resolve_round_medium_set() {
    let (env, contract_id, _admin, _oracle, client) = setup();
    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();

    for i in 0..25 {
        let user = Address::generate(&env);
        client.mint_initial(&user);
        let side = if i % 2 == 0 {
            BetSide::Up
        } else {
            BetSide::Down
        };
        client.place_bet(&user, &10_0000000, &side);
    }

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let payload = OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    };
    let (cpu, mem, _) = measure(&env, || client.resolve_round(&payload));
    report("resolve_round_medium_n25", cpu, mem);
    assert!(cpu <= RESOLVE_CPU_MAX);
    assert!(mem <= RESOLVE_MEM_MAX);
}

#[test]
fn bench_cost_resolve_round_max_cap() {
    let (env, contract_id, _admin, _oracle, client) = setup();
    client.create_round(&1_0000000u128, &None);
    let round = client.get_active_round().unwrap();

    for i in 0..100 {
        let user = Address::generate(&env);
        client.mint_initial(&user);
        let side = if i % 2 == 0 {
            BetSide::Up
        } else {
            BetSide::Down
        };
        client.place_bet(&user, &10_0000000, &side);
    }

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let payload = OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    };
    let (cpu, mem, _) = measure(&env, || client.resolve_round(&payload));
    report("resolve_round_max_cap_n100", cpu, mem);
    assert!(cpu <= RESOLVE_CPU_MAX);
    assert!(mem <= RESOLVE_MEM_MAX);
}

#[test]
fn bench_cost_resolve_precision_round_max_cap() {
    let (env, contract_id, _admin, _oracle, client) = setup();
    client.create_round(&1_0000000u128, &Some(1));
    let round = client.get_active_round().unwrap();

    for i in 0..100u128 {
        let user = Address::generate(&env);
        client.mint_initial(&user);
        client.predict_price(&user, &(1_0000000 + i * 10_000), &10_0000000);
    }

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let payload = OraclePayload {
        price: 1_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    };
    let (cpu, mem, _) = measure(&env, || client.resolve_round(&payload));
    report("resolve_precision_max_cap_n100", cpu, mem);
    assert!(cpu <= RESOLVE_CPU_MAX);
    assert!(mem <= RESOLVE_MEM_MAX);
}

// ─── Leaderboard benchmarks (Issue #431) ───────────────────────────────────
//
// These benchmarks measure the host CPU-instruction and memory cost of
// leaderboard update, season reset, and full-page read operations when
// the bounded indexes are populated to `LEADERBOARD_LIMIT` (100 entries).
//
// Key observations from the implementation:
// - `_update_leaderboards` calls `reinsert_sorted_by_wins/streak` which
//   performs an O(n) insertion sort over at most `LEADERBOARD_LIMIT` entries.
// - `reset_leaderboard_season` iterates over wins + streak lists (each
//   capped at `LEADERBOARD_LIMIT`) and performs a bounded dedup over at
//   most 2 * `LEADERBOARD_LIMIT` entries.
// - Page reads iterate over at most `LEADERBOARD_LIMIT` entries.
//
// All operations are bounded by `LEADERBOARD_LIMIT` — there are no
// unbounded scans.

/// Populates the lifetime leaderboard to `LEADERBOARD_LIMIT` by calling
/// `_update_stats_win` for that many unique users. Returns the generated
/// addresses.
fn populate_leaderboard(
    env: &Env,
    contract_id: &Address,
) -> soroban_sdk::Vec<Address> {
    let mut addrs = soroban_sdk::Vec::new(env);
    for _ in 0..LEADERBOARD_LIMIT {
        let user = Address::generate(env);
        env.as_contract(contract_id, || {
            VirtualTokenContract::_update_stats_win(env, user.clone()).unwrap();
        });
        addrs.push_back(user);
    }
    addrs
}

#[test]
fn bench_cost_leaderboard_update_at_limit() {
    let (env, contract_id, _admin, _oracle, _client) = setup();
    populate_leaderboard(&env, &contract_id);

    // Measure worst-case: insert a new user when the leaderboard is full.
    // The existing sorted list is at LEADERBOARD_LIMIT entries; the new user
    // is removed (no-op), then re-inserted in sorted position, then the
    // list is truncated back to LEADERBOARD_LIMIT.
    let new_user = Address::generate(&env);
    let (cpu, mem, _) = measure(&env, || {
        env.as_contract(&contract_id, || {
            VirtualTokenContract::_update_stats_win(env, new_user.clone()).unwrap();
        })
    });
    report("leaderboard_update_at_limit", cpu, mem);
    assert!(
        cpu <= TX_CPU_BUDGET,
        "leaderboard_update_at_limit CPU regression: {cpu}"
    );
    assert!(
        mem <= TX_MEM_BUDGET,
        "leaderboard_update_at_limit MEM regression: {mem}"
    );
}

#[test]
fn bench_cost_season_reset_at_limit() {
    let (env, contract_id, _admin, _oracle, client) = setup();

    // Populate the active season's leaderboard to the limit. Season stats
    // are recorded alongside lifetime stats on each _update_stats_win.
    populate_leaderboard(&env, &contract_id);

    // Now measure the cost of resetting the season at capacity.
    // reset_leaderboard_season requires admin auth (mocked) and no active
    // round (none created in setup()).
    let (cpu, mem, new_season) = measure(&env, || client.reset_leaderboard_season());
    report("season_reset_at_limit", cpu, mem);
    assert_eq!(new_season, 2, "season should advance from 1 to 2");
    assert!(
        cpu <= TX_CPU_BUDGET,
        "season_reset_at_limit CPU regression: {cpu}"
    );
    assert!(
        mem <= TX_MEM_BUDGET,
        "season_reset_at_limit MEM regression: {mem}"
    );
}

#[test]
fn bench_cost_leaderboard_full_page_read_at_limit() {
    let (env, contract_id, _admin, _oracle, client) = setup();
    populate_leaderboard(&env, &contract_id);

    // Read a full page (LEADERBOARD_LIMIT entries).
    let (cpu, mem, page) = measure(&env, || {
        client.get_leaderboard_by_wins(&0, &LEADERBOARD_LIMIT)
    });
    report("leaderboard_full_page_read_at_limit", cpu, mem);
    assert_eq!(
        page.len(),
        LEADERBOARD_LIMIT,
        "should return exactly LEADERBOARD_LIMIT entries"
    );
    assert!(
        cpu <= TX_CPU_BUDGET,
        "leaderboard_full_page_read_at_limit CPU regression: {cpu}"
    );
    assert!(
        mem <= TX_MEM_BUDGET,
        "leaderboard_full_page_read_at_limit MEM regression: {mem}"
    );
}

#[test]
fn verify_leaderboard_update_cost_is_bounded() {
    // Stronger assertion: the leaderboard update at capacity must use less
    // than 50% of the per-transaction CPU budget, demonstrating the O(n)
    // bound with n = LEADERBOARD_LIMIT = 100 is well within limits.
    let (env, contract_id, _admin, _oracle, _client) = setup();
    populate_leaderboard(&env, &contract_id);

    let new_user = Address::generate(&env);
    let (cpu, _mem, _) = measure(&env, || {
        env.as_contract(&contract_id, || {
            VirtualTokenContract::_update_stats_win(env, new_user.clone()).unwrap();
        })
    });

    let ceiling = TX_CPU_BUDGET / 2;
    std::println!(
        "[cost-benchmark] leaderboard_update_at_limit cpu={cpu} ceiling={ceiling} ({:.1}% of budget)",
        (cpu as f64 / TX_CPU_BUDGET as f64) * 100.0
    );
    assert!(
        cpu <= ceiling,
        "leaderboard update at limit should use <50% of CPU budget: {cpu} > {ceiling}"
    );
}

#[test]
fn verify_season_reset_cost_is_bounded() {
    // Stronger assertion: season reset at capacity must use less than 50%
    // of the per-transaction CPU budget.
    let (env, contract_id, _admin, _oracle, client) = setup();
    populate_leaderboard(&env, &contract_id);

    let (cpu, _mem, _) = measure(&env, || client.reset_leaderboard_season());

    let ceiling = TX_CPU_BUDGET / 2;
    std::println!(
        "[cost-benchmark] season_reset_at_limit cpu={cpu} ceiling={ceiling} ({:.1}% of budget)",
        (cpu as f64 / TX_CPU_BUDGET as f64) * 100.0
    );
    assert!(
        cpu <= ceiling,
        "season reset at limit should use <50% of CPU budget: {cpu} > {ceiling}"
    );
}
