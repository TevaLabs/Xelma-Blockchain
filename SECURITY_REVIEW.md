# Security Review - XLM Prediction Market Contract

**Review date:** 2026-09-25
**Contributor / owner:** Xelma maintainers
**Reviewer context:** Focused maintainer security refresh of the current Soroban contract, Rust tests, and generated TypeScript bindings. This is not an external audit.  
**Contract:** Soroban XLM prediction market, dual-mode Up/Down and Precision  
**Current confidence:** High for testnet / integration use; external audit recommended before mainnet.  
**Overall status:** One open client-surface finding and 3 accepted design risks remain; an external audit is required before mainnet.

## Scope

Reviewed files:

| Area | Files | Coverage |
| --- | --- | --- |
| Contract modules | `contracts/src/{access_control,admin,betting,collateral,common,config,contract,errors,governance,insurance,leaderboard,math_common,oracle_committee,queries,settlement,settlement_math,storage,types}.rs` | Access control, lifecycle, governance, insurance, queries, settlement, storage, and math |
| Contract tests | `contracts/src/tests/*.rs` | 51 registered top-level modules covering unit, security, property, storage, adversarial, and cost tests |
| Replay engine | `replay-engine/src/*.rs`, `replay-engine/tests/*.rs` | Deterministic off-chain settlement replay and parity fixtures |
| Bindings | `bindings/src/index.ts`, `bindings/src/parity.js`, `bindings/package.json`, `docs/CONTRACT_ERRORS.md` | Public method surface and Rust/TypeScript/docs error parity |
| Supporting docs | `README.md`, `STORAGE_DESIGN.md`, `ROUND_LIFECYCLE.md`, `MIGRATION.md`, `docs/CONTRIBUTOR_MAP.md` | Architecture and operational context |
| **CEI audit** | `contracts/src/contract.rs` | Full Checks-Effects-Interactions ordering review; see [`docs/CEI_AUDIT.md`](./docs/CEI_AUDIT.md) |

Out of scope:

- On-chain deployment configuration and admin key custody.
- Live oracle infrastructure, signer operations, and price source quality.
- Formal verification and third-party external audit.

## Methodology

1. Re-read the current contract, error types, storage model, tests, and bindings.
2. Mapped high-risk flows to code locations: auth, storage layout, arithmetic, oracle payload handling, lifecycle transitions, and bindings drift.
3. Verified constants single-sourcing and property-based lifecycle fuzzing extension.

## Quantitative Metrics

| Metric | Current value | Notes |
| --- | ---: | --- |
| Contract implementation size | 1,716 LOC | `contracts/src/contract.rs` |
| Contract source modules | 18 feature modules + `lib.rs` | Current module map listed in Scope above |
| Common utilities & constants size | 235 LOC | `contracts/src/common.rs` |
| Error enum size | 80 variants | `contracts/src/errors.rs` |
| Type definitions size | 1,079 LOC | `contracts/src/types.rs` |
| TypeScript bindings size | 1,480 LOC | `bindings/src/index.ts` |
| Registered contract test modules | 51 | Direct `mod` entries in `contracts/src/tests/mod.rs` |
| Declared contract tests | 767 | `#[test]` declarations under `contracts/src/tests/` after the pagination guard additions |
| Declared replay-engine tests | 8 | `#[test]` declarations under `replay-engine/` |
| Public contract methods | 185 | Public functions in the contract implementation; checked against bindings |
| Error parity | Rust ↔ TypeScript ↔ docs | CI parses all three error registries and fails on drift |
| Method parity | Failing on upstream `main` | 71 public contract methods are absent from the bindings `fromJSON` map |

Test distribution:

| Module | Tests | Main coverage |
| --- | ---: | --- |
| `betting.rs` | 20 | Bet validation, duplicates, events, position queries |
| `edge_cases.rs` | 8 | Empty rounds, one-sided rounds, pending/stat overflow boundaries |
| `guard_tests.rs` | 4 | Single-active-round invariant and non-mutation on rejection |
| `initialization.rs` | 12 | Init, auth, mint, duplicate init, admin/oracle separation |
| `lifecycle.rs` | 28 | Round creation, auth, full lifecycle, events |
| `mode_tests.rs` | 54 | Up/Down vs Precision isolation, precision scales, events |
| `overflow_tests.rs` + `precision_payout_overflow.rs` | 19 | Payout overflow and all-or-nothing behavior |
| `pause.rs` | 7 | Admin pause/unpause and paused mutation guards |
| `property_invariants.rs` | 7 | Payout conservation and stats monotonicity |
| `resolution/` | 115 | Up/Down, Precision, fees, policies, events, archives, and golden cases |
| `security.rs` | 74 | Oracle, heartbeat, replay, access, and payload defenses |
| `storage_benchmarks.rs` | 5 | Indexed key writes and resolution cleanup |
| `windows.rs` | 24 | Window bounds, timing, auth, precision window enforcement |
| `adversarial.rs` + `adversarial/` | 21 | Sybil, sniping, oracle, lifecycle, precision, and economic attacks |
| `cost_benchmarks.rs` + `pagination_gas_guards.rs` | 17 | CPU/memory ceilings and adversarial query limits |

Counts are source declarations produced with
`rg '^\s*#\[test\]' contracts/src/tests --glob '*.rs'`; the 767 total includes
all remaining specialized modules not expanded in this summary table.

## Adversarial Simulation Suite (Issue #372)

A first-class red-team suite lives at `contracts/src/tests/adversarial/` and scripts
economic attacks with deterministic seed `3722026`:

| Scenario | Defense | Residual risk |
| --- | --- | --- |
| Sybil faucet (mint limit) | `MintLimitExceeded` | none when limit configured |
| Sybil faucet (epoch budget) | `EpochBudgetExceeded` | none when budget configured |
| Last-ledger sniping (UpDown) | `RoundEnded` (close buffer) | none when close_buffer configured |
| Last-ledger sniping (Precision) | `RoundEnded` (close buffer) | none when close_buffer configured |
| Precision spam commits | `PrecisionCapExceeded` | none when cap configured |
| Oracle heartbeat griefing | `OracleNotLive` | admin override available |
| Oracle nonce replay | `OracleNonceReused` | none |
| Cross-round payload replay | `InvalidOracleRound` | none |
| Stale oracle timestamp griefing | `StaleOracleData` | none |
| Fee gaming (mid-round schedule) | Timelock (pending config only) | none |
| Exposure cap boundary | `ExposureCapExceeded` | sybil bypass (accepted) |
| Double-claim attack | idempotent zero payout | none |
| Mode confusion | `WrongModeForPrediction` | none |

Run locally:

```text
./scripts/run_adversarial_suite.sh
```

Reports are written to `adversarial-reports/adversarial-report.{md,json}`.
Three CI-critical scenarios run in the default `ci.yml` rust-test job; the full
suite (13 scenarios) runs nightly via `.github/workflows/nightly-adversarial.yml`.

## Threat Model

For the formal protocol invariant list, role assumptions, upgrade guarantees,
and coverage traceability, see [`PROTOCOL_SPEC.md`](./PROTOCOL_SPEC.md).

Primary assets:

- User vXLM balances, pending winnings, and round funds represented in contract storage.
- Round integrity: one active round at a time, stable round IDs, correct pool accounting.
- Oracle resolution integrity: price, timestamp, and round binding.
- Client correctness: bindings must expose the same method/error surface as the contract.

Trust boundaries:

- Admin is trusted to initialize the contract, configure windows, create rounds, and pause/unpause.
- Oracle signer is trusted to submit accurate prices for the intended round.
- Users are untrusted and may try invalid auth, duplicate bets, timing abuse, overflow inputs, or storage growth attacks.
- TypeScript clients depend on generated bindings and error maps for operational safety.

Soroban-specific risk considerations:

- **Auth:** state-changing role/user operations call `require_auth()` on the expected `Address`; pause/unpause and windows are admin-gated, resolution is oracle-gated, user actions are user-gated.
- **Storage:** persistent storage uses indexed per-user keys plus `RoundParticipants(round_id)` to avoid rewriting full maps on each bet, with legacy map fallbacks for migration.
- **Arithmetic:** critical arithmetic uses checked operations. Payout paths increasingly route through `payout_add` / `payout_mul`, but one Precision indexed path still returns generic `Overflow` rather than `PayoutOverflow`.
- **Oracle inputs:** `OraclePayload` enforces non-zero price, timestamp not in the future, 300-second freshness, and round binding against `Round.start_ledger`. `create_round` guarantees `start_ledger` is unique per round, so the binding identifies exactly one round.
- **Resource limits:** resolution is O(n) over participants. Precision rounds include an admin-configurable participant cap; operators still need benchmark evidence before raising it near upper bounds.

## Findings

| ID | Severity | Status | Finding | Evidence / code locations | Impact | Mitigation plan / owner |
| --- | --- | --- | --- | --- | --- | --- |
| SR-2026-04-001 | Medium | Mitigated | Client error maps could drift from the Rust enum and documentation. | `bindings/src/parity.js` now compares `contracts/src/errors.rs`, `bindings/src/index.ts`, and `docs/CONTRACT_ERRORS.md`; `.github/workflows/ci.yml` runs it in the bindings job. | Drift now hard-fails CI with file-specific repair instructions. | Keep all three registries synchronized when changing errors. |
| SR-2026-04-002 | Low | Mitigated | Indexed Precision payout arithmetic previously returned generic `Overflow` instead of `PayoutOverflow`. | Checked helpers in `contracts/src/settlement_math.rs`; regression coverage in `contracts/src/tests/precision_payout_overflow.rs`. | Client-visible payout failures now use consistent semantics and settlement remains atomic. | No further action unless payout arithmetic changes. |
| SR-2026-04-003 | Medium | Mitigated | Oracle payload round binding uses `round.start_ledger` as the payload `round_id`, not the monotonic `Round.round_id`. `start_ledger` was not unique per round, so a round created, cancelled, and replaced within one ledger shared a `start_ledger` with its predecessor — letting a payload signed for the first round settle the second. The nonce guard did not cover this, because consumed nonces are namespaced by the monotonic `Round.round_id`. | `OraclePayload.round_id: u32` in `contracts/src/types.rs`; binding checks in `resolve_round` and `resolve_round_multi` (`contracts/src/settlement.rs`); `start_ledger` assignment in `create_round` (`contracts/src/betting.rs`). | Wrong-round settlement: the replacement round settles at the previous round's price. Reproduced by `tests::adversarial::oracle::test_same_ledger_round_recreation_rejected`. | `create_round` now claims each ledger sequence under `DataKeyScoped::RoundStartLedger` and rejects reuse with `RoundStartLedgerReused` (code 93), making `start_ledger` unique per round. Binding semantics documented in `PROTOCOL_SPEC.md` I10 and `docs/ORACLE_OPERATOR_RUNBOOK.md`. Field rename to a monotonic `u64 round_id` remains deferred to the next breaking ABI. |
| SR-2026-04-004 | Medium | Accepted risk | The oracle remains a single trusted signer. Payload freshness and round checks protect replay/staleness but not bad signed prices. | Oracle auth and payload validation: `contracts/src/contract.rs:633-668`; tests in `contracts/src/tests/security.rs`. | A compromised or faulty oracle can resolve rounds with incorrect but fresh prices. | Owner: Protocol/security owner (TBD). Accepted for current architecture. Before mainnet, define oracle operations, monitoring, emergency pause playbook, and consider multi-oracle or threshold validation. |
| SR-2026-04-005 | Low | Accepted risk | Resolution loops over participant lists and remains O(n), though Precision rounds now have an explicit participant cap. | Participant append and cap checks in `contracts/src/contract.rs`; resolution cleanup in `contracts/src/contract.rs`; storage tests in `contracts/src/tests/storage_benchmarks.rs`; Precision cap tests in `contracts/src/tests/mode_tests.rs`. | Very large rounds can become expensive or fail under Soroban resource limits if caps are tuned too high. Indexed storage mitigates write amplification but does not remove O(n) resolution cost. | Owner: Contract/product owner (TBD). Maintain operational round-size monitoring and benchmark cap changes before increasing production limits. |
| SR-2026-04-006 | High | Mitigated | Multiple active rounds could corrupt lifecycle state. | Guard: `contracts/src/contract.rs:115-116` and `contracts/src/contract.rs:1276-1279`; tests in `contracts/src/tests/guard_tests.rs`; related commit `7fb45b2`. | Prevents overwriting live round state and orphaning user positions. | No further action unless lifecycle semantics change. |
| SR-2026-04-007 | High | Mitigated | Payout and claim arithmetic overflow could corrupt balances or panic. | Checked claim/refund/updown helpers: `contracts/src/contract.rs:1085-1171`, `contracts/src/contract.rs:1284-1302`; tests in `contracts/src/tests/overflow_tests.rs`; related commit `45dd076`. | Overflow paths return errors and avoid partial writes in covered payout paths. | Extend consistency to indexed Precision as tracked in SR-2026-04-002. |
| SR-2026-04-008 | High | Mitigated | Unauthorized admin/oracle/user operations. | Initialization/admin checks: `contracts/src/contract.rs:21-25`, `contracts/src/contract.rs:55-80`, `contracts/src/contract.rs:212-220`; user auth: `contracts/src/contract.rs:286`, `contracts/src/contract.rs:392`, `contracts/src/contract.rs:1087`, `contracts/src/contract.rs:1231`; oracle auth: `contracts/src/contract.rs:633-640`. Tests across `initialization.rs`, `lifecycle.rs`, `pause.rs`, and `windows.rs`. | Prevents impersonation of admin, oracle, and users. | Continue adding auth regression tests when new public methods are added. |
| SR-2026-04-009 | Medium | Mitigated | Late betting / premature resolution can bias outcomes. | Bet window checks: `contracts/src/contract.rs:305-307`, `contracts/src/contract.rs:417-419`; resolution end-ledger check: `contracts/src/contract.rs:665-668`; window validation: `contracts/src/contract.rs:222-234`; tests in `contracts/src/tests/windows.rs`. | Bets close before observation window completes, and resolution cannot happen early. | No further action unless timing model changes. |
| SR-2026-04-010 | Medium | Mitigated | Oracle replay, stale payloads, and future-dated data. | Payload checks: `contracts/src/contract.rs:628-668`; tests in `contracts/src/tests/security.rs`. | Blocks zero price, wrong-round, stale, future, and premature oracle resolution. | Pair with accepted single-oracle risk follow-ups in SR-2026-04-004. |
| SR-2026-04-011 | Medium | Mitigated | Mode confusion between Up/Down and Precision rounds. | Mode checks: `contracts/src/contract.rs:97-106`, `contracts/src/contract.rs:300-303`, `contracts/src/contract.rs:412-415`; tests in `contracts/src/tests/mode_tests.rs`. | Users cannot submit the wrong prediction type for a round. | No further action. |
| SR-2026-04-012 | Medium | Mitigated | Storage write amplification from full participant maps. | Indexed storage writes: `contracts/src/contract.rs:315-344`, `contracts/src/contract.rs:427-457`; cleanup: `contracts/src/contract.rs:684-702`; design doc `STORAGE_DESIGN.md`; related commit `9855391`. | Reduces per-bet cost and avoids repeated full-map serialization. | Monitor resource usage for high-participant rounds as tracked in SR-2026-04-005. |
| SR-2026-06-001 | Low | Mitigated | `claim_winnings`: balance was credited before `PendingWinnings` slot was removed (Interaction-before-Effect ordering). | `contracts/src/contract.rs` `claim_winnings` function; regression test `test_claim_winnings_cei_pending_cleared_after_claim` in `contracts/src/tests/cei_ordering.rs`. | Under the prior ordering, the pending winnings slot remained readable during the balance update. While Soroban's single-tenant model prevents reentrancy today, the slot removal now precedes the balance increase, eliminating any double-claim risk on future cross-contract interaction paths. | Fixed: removal of `PendingWinnings` key moved before `_set_balance` call. Full CEI audit documented in [`docs/CEI_AUDIT.md`](./docs/CEI_AUDIT.md). |
| SR-2026-06-002 | Low | Mitigated | `cancel_config_change`: event was emitted before `PendingConfigChange` key was removed from storage (Interaction before Effect). | `contracts/src/contract.rs` `cancel_config_change` function; regression test `test_cancel_config_change_cei_key_removed_before_event` in `contracts/src/tests/cei_ordering.rs`. | The emitted cancellation event did not represent a fully committed state transition — the key was still present in storage at event emission time. | Fixed: `env.storage().persistent().remove(&key)` moved before `env.events().publish(...)`. Full CEI audit documented in [`docs/CEI_AUDIT.md`](./docs/CEI_AUDIT.md). |
| SR-2026-09-001 | Medium | Open | The generated TypeScript `fromJSON` surface is missing 71 public contract methods. | `npm --prefix bindings run test:parity` reports the exact missing method list by comparing `contracts/src/contract.rs` with `bindings/src/index.ts`. | Integrators cannot construct or decode calls for newer contract methods through the published bindings. | Regenerate the bindings from the current WASM after upstream compilation is restored, then rerun method and WASM parity checks. |

## Severity Summary

| Status | Critical | High | Medium | Low | Total |
| --- | ---: | ---: | ---: | ---: | ---: |
| Open | 0 | 0 | 1 | 0 | 1 |
| Accepted risk | 0 | 0 | 1 | 2 | 3 |
| Mitigated | 0 | 3 | 5 | 3 | 11 |
| **Total** | **0** | **3** | **7** | **5** | **15** |

## Current Security Posture

Strengths:

- Role separation is explicit: admin, oracle, and user calls each authenticate the relevant address.
- One active round is enforced and tested.
- Emergency pause now exists and blocks high-risk mutations.
- Oracle resolution requires a structured payload with price, timestamp, and round binding.
- Betting and resolution windows are bounded and tested.
- Indexed participant storage reduces write amplification and preserves migration fallbacks.
- Payout conservation and core invariants are covered by tests.

Primary residual risks:

- Error-code drift is now enforced across Rust, TypeScript, and documentation in CI.
- The TypeScript method surface still trails the public Rust contract and must be regenerated after the upstream build is repaired.
- Single-oracle trust remains the largest accepted protocol-level risk.
- Large participant rounds may hit Soroban resource ceilings during resolution if operational caps are tuned too high.
- Current review is maintainer-focused and should not replace an external audit for mainnet.

## Verification Evidence

Commands run during this review:

```text
npm --prefix bindings run test:parity
Error enum parity check passed: Rust, TypeScript, and docs are synced.
```

The error-enum portion passes, while the same command currently reports the
open method drift in SR-2026-09-001 and exits non-zero. The source tree
currently declares 767 contract tests and 8 replay-engine tests.
Execution is temporarily blocked on upstream `main` by unrelated pre-existing
compile errors (including duplicate adversarial module paths and malformed
governance doc comments). Counts above describe the current source inventory,
not a claim that the broken baseline completed a test run.

## Follow-Up Backlog

| Priority | Item | Owner | Target evidence |
| --- | --- | --- | --- |
| **Closed** | Enforce Rust/TypeScript/docs error parity in CI. | Bindings maintainers | `bindings/src/parity.js`, `docs/CONTRACT_ERRORS.md`, and the bindings CI job |
| **Closed** | Normalize indexed Precision payout overflow errors to `PayoutOverflow`. | Contract maintainers | `contracts/src/tests/precision_payout_overflow.rs` |
| P1 | Regenerate the TypeScript method surface after restoring contract compilation. | Bindings maintainers | Passing method parity and WASM parity jobs |
| P2 | ~~Document oracle payload `round_id` semantics for integrators.~~ Done — `PROTOCOL_SPEC.md` I10 and `docs/ORACLE_OPERATOR_RUNBOOK.md` show `payload.round_id = activeRound.start_ledger` and explain the two-identifier split. | Oracle integration owner (TBD) | Complete |
| P2 | Define oracle operations and incident response before mainnet. | Protocol/security owner (TBD) | Oracle runbook, monitoring checks, pause criteria |
| P3 | Maintain operational monitoring for large participant rounds and benchmark cap increases. | Contract/product owner (TBD) | Participant-count policy, resource benchmark, and cap-change runbook |
| P3 | Schedule external audit before mainnet deployment. | Maintainers (TBD) | Audit report or issue tracking external findings |
| **Closed** | CEI ordering fixes for `claim_winnings` and `cancel_config_change` (SR-2026-06-001, SR-2026-06-002). | #195 | Merged with regression tests in `contracts/src/tests/cei_ordering.rs`; full audit in [`docs/CEI_AUDIT.md`](./docs/CEI_AUDIT.md). |

## Deployment Recommendation

Proceed with testnet/integration usage after restoring a green upstream build. Do not treat the state as mainnet-ready until:

1. Oracle operations and pause response are documented.
2. Accepted protocol risks are reviewed and explicitly approved for deployment.
3. An external audit is completed for any production-value deployment.

## Pre-Merge Security Review Checklist

To maintain code security and guard against regressions in payout logic, access control, and oracle validation paths, every pull request affecting the smart contracts must pass the [Pre-Merge Security Checklist](.github/PULL_REQUEST_TEMPLATE.md). 

This checklist enforces verification across five critical contract risk categories:
1. **Authentication & Access Control** (proper `require_auth()` gating)
2. **Safe Arithmetic & Overflow Protection** (use of checked math and specialized safe helpers)
3. **Lifecycle & State Transitions** (runtime mode validation, invariants)
4. **Event Emission & Observability** (forensic round summaries, canonical events)
5. **Tests & Verification** (coverage of successful and failing paths)

Additionally, any PR modifying critical paths (e.g. payout, resolution, or claim methods) requires an explicit note and description of mitigations for any potential new failure modes.
