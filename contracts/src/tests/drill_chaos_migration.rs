// SPDX-License-Identifier: MIT
//! Automated chaos drill: active round × pause × migration dry-run ×
//! claims-only × resume/cancel (Issue #565).
//!
//! Every scenario starts from a contract that still has a migration pending
//! (schema v2), opens a round with real stakes, then walks one of several
//! emergency sequences before leaving through one of several exits. After
//! **every** step the drill checks the value-conservation invariant:
//!
//! ```text
//! Σ(balance + pending) over every address that ever held value
//!   + active round pot + protocol fee treasury + insurance fund
//!   == total ever minted
//! ```
//!
//! and after the exit it proves that nothing is stuck: all pending winnings
//! can be claimed, the deferred migration dry-run passes without mutating
//! state, the real migration completes, and a fresh round can be traded.
//!
//! The module name starts with `drill` so the CI step
//! `cargo test --package xelma-contract --lib tests::drill` runs it too.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, DataKeyCore, OraclePayload, ProtocolStatus, RoundStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    vec, Address, Env,
};
use std::vec::Vec as StdVec;

// ─── Drill vocabulary ─────────────────────────────────────────────────────────

/// One emergency action applied while the round is live.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// `pause_contract()` → `RuntimeMode::FullyPaused`.
    Pause,
    /// `set_runtime_mode(1)` → `RuntimeMode::ClaimsOnly`.
    ClaimsOnly,
    /// `migrate_schema_v2_to_v3(dry_run = true)`; must be refused atomically
    /// while a round is active (or the contract is paused).
    MigrationDryRun,
}

/// How the operator leaves the incident.
#[derive(Clone, Copy, Debug)]
enum Exit {
    /// Stay in ClaimsOnly and let the oracle settle the round.
    ResolveInClaimsOnly,
    /// Return to Normal, then settle the round.
    ResumeThenResolve,
    /// Cancel from ClaimsOnly with a generic reason (no insurance).
    CancelInClaimsOnly,
    /// Cancel from ClaimsOnly for an oracle outage (insurance-eligible
    /// reason 1), exercising the coverage payout path.
    CancelOracleOutageWithInsurance,
    /// Return to Normal, then cancel.
    ResumeThenCancel,
}

const STAKE_ALICE: i128 = 100_0000000;
const STAKE_BOB: i128 = 250_0000000;
const INSURANCE_TOP_UP: i128 = 50_0000000;

struct Drill<'a> {
    env: Env,
    client: VirtualTokenContractClient<'a>,
    contract_id: Address,
    admin: Address,
    alice: Address,
    bob: Address,
    carol: Address,
    total_minted: i128,
    nonce: u64,
}

impl<'a> Drill<'a> {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|li| {
            li.sequence_number = 100;
            li.timestamp = 1_000;
        });
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        client.initialize(&admin, &oracle);
        client.update_oracle_heartbeat(&0u32);

        // A migration is pending: roll storage back to schema v2.
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&DataKeyCore::SchemaVersion, &2u32);
        });
        assert_eq!(client.get_schema_version(), 2);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let carol = Address::generate(&env);
        let mut total_minted = 0i128;
        for who in [&admin, &alice, &bob, &carol] {
            client.mint_initial(who);
            total_minted += client.balance(who);
        }

        Drill {
            env,
            client,
            contract_id,
            admin,
            alice,
            bob,
            carol,
            total_minted,
            nonce: 1,
        }
    }

    fn holders(&self) -> [&Address; 4] {
        [&self.admin, &self.alice, &self.bob, &self.carol]
    }

    fn active_pot(&self) -> i128 {
        self.client
            .get_active_round()
            .map(|r| r.pool_up + r.pool_down)
            .unwrap_or(0)
    }

    /// Value-conservation invariant; `ctx` names the step for diagnostics.
    fn assert_conserved(&self, ctx: &str) {
        let held: i128 = self
            .holders()
            .iter()
            .map(|a| self.client.balance(a) + self.client.get_pending_winnings(a))
            .sum();
        let total = held
            + self.active_pot()
            + self.client.get_protocol_fee_treasury()
            + self.client.get_insurance_fund_balance();
        assert_eq!(
            total, self.total_minted,
            "value leaked or created at: {}",
            ctx
        );
    }

    /// Snapshot of every user-visible amount, used to prove dry-runs are atomic.
    fn snapshot(&self) -> (StdVec<(i128, i128)>, i128, i128, u32, u32) {
        (
            self.holders()
                .iter()
                .map(|a| (self.client.balance(a), self.client.get_pending_winnings(a)))
                .collect(),
            self.active_pot(),
            self.client.get_protocol_fee_treasury(),
            self.client.get_schema_version(),
            self.client.get_runtime_mode(),
        )
    }

    fn open_round_with_stakes(&self) {
        self.client.create_round(&1_0000000, &None);
        self.client
            .place_bet(&self.alice, &STAKE_ALICE, &BetSide::Up);
        self.client.place_bet(&self.bob, &STAKE_BOB, &BetSide::Down);
        assert_eq!(self.active_pot(), STAKE_ALICE + STAKE_BOB);
        self.assert_conserved("round opened");
    }

    fn apply(&self, step: Step) {
        let before = self.snapshot();
        match step {
            Step::Pause => {
                self.client.pause_contract();
                assert_eq!(self.client.get_protocol_status(), ProtocolStatus::Paused);
                assert_eq!(
                    self.client.try_claim_winnings(&self.alice),
                    Err(Ok(ContractError::ContractPaused))
                );
            }
            Step::ClaimsOnly => {
                self.client.set_runtime_mode(&1u32);
                assert_eq!(self.client.get_runtime_mode(), 1);
                assert!(!self.client.is_paused());
            }
            Step::MigrationDryRun => {
                let expected = if self.client.is_paused() {
                    ContractError::ContractPaused
                } else {
                    ContractError::MigrationActiveRound
                };
                assert_eq!(
                    self.client.try_migrate_schema_v2_to_v3(&true),
                    Err(Ok(expected))
                );
                // Refused dry-run leaves every amount and the schema intact.
                assert_eq!(self.snapshot(), before, "dry-run mutated state");
            }
        }

        // Once the incident is declared (any non-Normal mode) no new stakes
        // may enter the round.
        if self.client.get_runtime_mode() != 0 {
            assert_eq!(
                self.client
                    .try_place_bet(&self.carol, &10_0000000, &BetSide::Up),
                Err(Ok(ContractError::ContractPaused)),
                "bet accepted after {:?}",
                step
            );
        }
        // The live round survives every emergency step untouched.
        assert!(self.client.get_active_round().is_some());
        assert_eq!(self.active_pot(), STAKE_ALICE + STAKE_BOB);
        self.assert_conserved(&std::format!("after {:?}", step));
    }

    fn advance_past_round_end(&self) {
        let end = self.client.get_active_round().unwrap().end_ledger;
        self.env.ledger().with_mut(|li| {
            li.sequence_number = end + 1;
            li.timestamp += 60;
        });
        self.client.update_oracle_heartbeat(&0u32);
    }

    fn resolve(&mut self, price: u128) {
        self.advance_past_round_end();
        let round = self.client.get_active_round().unwrap();
        self.nonce += 1;
        self.client.resolve_round(&OraclePayload {
            price,
            timestamp: self.env.ledger().timestamp(),
            round_id: round.start_ledger,
            nonce: self.nonce,
            network_id: self.env.ledger().network_id(),
            contract_addr: self.contract_id.clone(),
            confidence: None,
            attestation: None,
        });
    }

    fn leave(&mut self, exit: Exit) {
        // Whatever the entry sequence, operators de-escalate to ClaimsOnly
        // before choosing the exit (FullyPaused blocks settlement).
        self.client.set_runtime_mode(&1u32);
        let round_id = self.client.get_active_round().unwrap().round_id;

        match exit {
            Exit::ResolveInClaimsOnly => {
                self.resolve(2_0000000);
                assert_eq!(
                    self.client.get_round_status(&round_id),
                    RoundStatus::Resolved
                );
            }
            Exit::ResumeThenResolve => {
                self.client.set_runtime_mode(&0u32);
                assert_eq!(self.client.get_protocol_status(), ProtocolStatus::Active);
                self.resolve(5000000);
                assert_eq!(
                    self.client.get_round_status(&round_id),
                    RoundStatus::Resolved
                );
            }
            Exit::CancelInClaimsOnly => {
                self.client.cancel_round(&0u32);
                self.assert_full_refund(0);
                assert_eq!(
                    self.client.get_round_status(&round_id),
                    RoundStatus::Cancelled
                );
            }
            Exit::CancelOracleOutageWithInsurance => {
                // Insurance was configured and funded before the incident
                // (see `run`). Reason 1 = oracle outage.
                let fund_before = self.client.get_insurance_fund_balance();
                self.client.cancel_round(&1u32);
                let coverage = fund_before - self.client.get_insurance_fund_balance();
                assert!(
                    coverage > 0,
                    "oracle-outage cancel should pay insurance coverage"
                );
                self.assert_full_refund(coverage);
                assert_eq!(
                    self.client.get_round_status(&round_id),
                    RoundStatus::Cancelled
                );
            }
            Exit::ResumeThenCancel => {
                self.client.set_runtime_mode(&0u32);
                self.client.cancel_round(&0u32);
                self.assert_full_refund(0);
                assert_eq!(
                    self.client.get_round_status(&round_id),
                    RoundStatus::Cancelled
                );
            }
        }

        assert!(self.client.get_active_round().is_none());
        self.assert_conserved(&std::format!("after exit {:?}", exit));
    }

    /// Cancellation refunds every stake in full, plus any insurance coverage.
    fn assert_full_refund(&self, coverage: i128) {
        let refunded = self.client.get_pending_winnings(&self.alice)
            + self.client.get_pending_winnings(&self.bob);
        assert_eq!(refunded, STAKE_ALICE + STAKE_BOB + coverage);
        assert!(self.client.get_pending_winnings(&self.alice) >= STAKE_ALICE);
        assert!(self.client.get_pending_winnings(&self.bob) >= STAKE_BOB);
    }

    /// Proves no funds are stuck and the deferred migration can complete.
    fn recover(&mut self) {
        // 1. Everyone can claim; nothing is left pending.
        for who in self.holders() {
            self.client.claim_winnings(who);
            assert_eq!(self.client.get_pending_winnings(who), 0);
        }
        self.assert_conserved("after claims");
        let circulating: i128 = self.holders().iter().map(|a| self.client.balance(a)).sum();
        assert_eq!(
            circulating
                + self.client.get_protocol_fee_treasury()
                + self.client.get_insurance_fund_balance(),
            self.total_minted,
            "funds stuck outside balances/treasury/insurance"
        );

        // 2. With the round gone the deferred dry-run passes and is atomic.
        let before = self.snapshot();
        self.client.migrate_schema_v2_to_v3(&true);
        assert_eq!(self.snapshot(), before, "successful dry-run mutated state");
        assert_eq!(self.client.get_schema_version(), 2);

        // 3. Real migration, then resume normal trading.
        self.client.migrate_schema_v2_to_v3(&false);
        assert_eq!(self.client.get_schema_version(), 3);
        self.client.set_runtime_mode(&0u32);
        assert_eq!(
            self.client.get_protocol_status(),
            ProtocolStatus::ClaimsOnly
        );
        self.assert_conserved("after migration");

        self.env.ledger().with_mut(|li| li.sequence_number += 1);
        self.client.create_round(&1_0000000, &None);
        self.client
            .place_bet(&self.carol, &10_0000000, &BetSide::Up);
        assert_eq!(self.client.get_protocol_status(), ProtocolStatus::Active);
        self.assert_conserved("fresh round after recovery");
    }

    fn run(entry: &[Step], exit: Exit) {
        let mut d = Drill::new();
        if let Exit::CancelOracleOutageWithInsurance = exit {
            // Oracle outage is insurance event 0; cover 10% of each stake.
            d.client.set_insurance_eligible_events(&vec![&d.env, 0u32]);
            d.client.set_insurance_coverage_bps(&1_000u32);
            d.client.top_up_insurance_fund(&INSURANCE_TOP_UP);
            d.assert_conserved("insurance funded");
        }
        d.open_round_with_stakes();
        for step in entry {
            d.apply(*step);
        }
        d.leave(exit);
        d.recover();
    }
}

// ─── Scenarios ────────────────────────────────────────────────────────────────

/// Entry sequences: the canonical order from the issue plus chaotic variants
/// (repeated pauses, escalation and de-escalation, dry-runs in every mode).
const ENTRIES: &[&[Step]] = &[
    // Issue #565 canonical: pause → dry-run → claims-only.
    &[Step::Pause, Step::MigrationDryRun, Step::ClaimsOnly],
    // Dry-run attempted before anyone noticed the active round.
    &[
        Step::MigrationDryRun,
        Step::Pause,
        Step::MigrationDryRun,
        Step::ClaimsOnly,
    ],
    // Contain first, then escalate to a full pause, then de-escalate.
    &[
        Step::ClaimsOnly,
        Step::MigrationDryRun,
        Step::Pause,
        Step::MigrationDryRun,
        Step::ClaimsOnly,
    ],
    // Operator double-pauses and flips modes repeatedly.
    &[
        Step::Pause,
        Step::Pause,
        Step::ClaimsOnly,
        Step::Pause,
        Step::MigrationDryRun,
    ],
];

const EXITS: &[Exit] = &[
    Exit::ResolveInClaimsOnly,
    Exit::ResumeThenResolve,
    Exit::CancelInClaimsOnly,
    Exit::CancelOracleOutageWithInsurance,
    Exit::ResumeThenCancel,
];

/// Full matrix: every entry sequence × every exit (20 drills).
#[test]
fn drill_chaos_migration_full_matrix() {
    for entry in ENTRIES {
        for exit in EXITS {
            Drill::run(entry, *exit);
        }
    }
}

/// Canonical path from the issue, resume branch, as a standalone test so a
/// failure is easy to spot in CI output.
#[test]
fn drill_chaos_migration_canonical_resume() {
    Drill::run(ENTRIES[0], Exit::ResumeThenResolve);
}

/// Canonical path from the issue, cancel branch.
#[test]
fn drill_chaos_migration_canonical_cancel() {
    Drill::run(ENTRIES[0], Exit::CancelInClaimsOnly);
}

/// Regression: `cancel_round` with an insurance-eligible reason trapped with
/// "mis-tagged object reference" because the insurance storage keys were
/// built in a foreign `Env`. That left the round impossible to cancel, i.e.
/// the stakes were stuck.
#[test]
fn drill_cancel_with_insurance_eligible_reason_does_not_trap() {
    for reason in 1u32..=3 {
        let d = Drill::new();
        d.open_round_with_stakes();
        d.client.cancel_round(&reason);
        d.assert_full_refund(0);
        d.assert_conserved("cancel with eligible reason");
    }
}
