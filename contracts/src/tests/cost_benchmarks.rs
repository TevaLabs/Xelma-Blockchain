//! Gas/cost benchmark baselines with regression guardrails (Issue #121).
//!
//! These benchmarks measure the host CPU-instruction and memory cost of each
//! critical contract path and assert it stays within a documented ceiling.
//! They give maintainers an early-warning gate against performance drift as
//! features evolve.
//!
//! Paths covered: `create_round`, `place_bet`, `place_precision_prediction`
//! (precision submit), `resolve_round`, and `claim_winnings`.
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
    std::println!("[bench] {label:<24} cpu={cpu:>12} mem={mem:>12}");
}

fn setup() -> (Env, Address, Address, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, admin, oracle, client)
}

#[test]
fn bench_cost_create_round() {
    let (env, _admin, _oracle, client) = setup();
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
    let (env, _admin, _oracle, client) = setup();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&1_0000000u128, &None);

    let (cpu, mem, _) = measure(&env, || client.place_bet(&alice, &100_0000000, &BetSide::Up));
    report("place_bet", cpu, mem);
    assert!(cpu <= PLACE_BET_CPU_MAX, "place_bet CPU regression: {cpu}");
    assert!(mem <= PLACE_BET_MEM_MAX, "place_bet MEM regression: {mem}");
}

#[test]
fn bench_cost_precision_submit() {
    let (env, _admin, _oracle, client) = setup();
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
    let (env, _admin, _oracle, client) = setup();
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
    };
    let (cpu, mem, _) = measure(&env, || client.resolve_round(&payload));
    report("resolve_round", cpu, mem);
    assert!(cpu <= RESOLVE_CPU_MAX, "resolve_round CPU regression: {cpu}");
    assert!(mem <= RESOLVE_MEM_MAX, "resolve_round MEM regression: {mem}");
}

#[test]
fn bench_cost_claim_winnings() {
    let (env, _admin, _oracle, client) = setup();
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
    });

    let (cpu, mem, claimed) = measure(&env, || client.claim_winnings(&alice));
    report("claim_winnings", cpu, mem);
    assert!(claimed > 0, "alice should have winnings to claim");
    assert!(cpu <= CLAIM_CPU_MAX, "claim_winnings CPU regression: {cpu}");
    assert!(mem <= CLAIM_MEM_MAX, "claim_winnings MEM regression: {mem}");
}
