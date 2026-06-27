# Security Review - XLM Prediction Market Contract

**Review date:** 2026-04-29  
**Contributor / owner:** TBD (assign security review owner)  
**Reviewer context:** Focused maintainer security refresh of the current Soroban contract, Rust tests, and generated TypeScript bindings. This is not an external audit.  
**Contract:** Soroban XLM prediction market, dual-mode Up/Down and Precision  
**Current confidence:** Medium-high for testnet / integration use; external audit recommended before mainnet.  
**Overall status:** Actionable with 1 open medium item, 2 accepted design risks, and no open high/critical findings found in this pass.

## Scope

Reviewed files:

| Area | Files | Coverage |
| --- | --- | --- |
| Contract core | `contracts/src/contract.rs`, `contracts/src/errors.rs`, `contracts/src/types.rs` | Initialization, roles, pause, round lifecycle, betting, precision predictions, oracle resolution, payouts, storage cleanup |
| Contract tests | `contracts/src/tests/*.rs` | Unit/security/property/storage tests included by `contracts/src/tests/mod.rs` |
| Bindings | `bindings/src/index.ts`, `bindings/src/parity.js`, `bindings/package.json` | Public method surface and client-facing error map |
| Supporting docs | `README.md`, `STORAGE_DESIGN.md`, `ROUND_LIFECYCLE.md`, `MIGRATION.md` | Architecture and operational context |
| **CEI audit** | `contracts/src/contract.rs` (all 32 mutating entrypoints) | Full Checks-Effects-Interactions ordering review; see [`docs/CEI_AUDIT.md`](./docs/CEI_AUDIT.md) |

Out of scope:

- On-chain deployment configuration and admin key custody.
- Live oracle infrastructure, signer operations, and price source quality.
- Formal verification, fuzzing beyond the current property tests, and third-party audit.

## Methodology

1. Re-read the current contract, error types, storage model, tests, and bindings.
2. Mapped high-risk flows to code locations: auth, storage layout, arithmetic, oracle payload handling, lifecycle transitions, and bindings drift.
3. Ran the current verification suite:
   - `cargo test` -> 118 passed, 0 failed.
   - `npm --prefix bindings run test:parity` -> passed public method parity.
4. Compared the current implementation against the prior review and recent repository history:
   - `ab8a8f4` merge for precision remainder / bindings CI.
   - `9855391` merge for storage optimization.
   - `7fb45b2` merge for single-active-round guard.
   - `45dd076` merge for checked claim arithmetic.

## Quantitative Metrics

| Metric | Current value | Notes |
| --- | ---: | --- |
| Contract implementation size | 1,304 LOC | `contracts/src/contract.rs` |
| Error enum size | 25 variants | `contracts/src/errors.rs` |
| Type definitions size | 105 LOC | `contracts/src/types.rs` |
| TypeScript bindings size | 765 LOC | `bindings/src/index.ts` |
| Test modules | 13 | All modules included by `contracts/src/tests/mod.rs` |
| Contract tests | 118 | Counted by `#[test]`; confirmed by `cargo test` |
| Public contract methods | 19 | Covered by generated client surface and parity script |
| Binding parity | Passing | Method-level parity only |

Test distribution:

| Module | Tests | Main coverage |
| --- | ---: | --- |
| `betting.rs` | 9 | Bet validation, duplicates, events, position queries |
| `edge_cases.rs` | 5 | Empty rounds, one-sided rounds, pending/stat overflow boundaries |
| `guard_tests.rs` | 4 | Single-active-round invariant and non-mutation on rejection |
| `initialization.rs` | 8 | Init, auth, mint, duplicate init, admin/oracle separation |
| `lifecycle.rs` | 14 | Round creation, auth, full lifecycle, events |
| `mode_tests.rs` | 21 | Up/Down vs Precision isolation, precision scales, events |
| `overflow_tests.rs` | 6 | Payout overflow and all-or-nothing claim behavior |
| `pause.rs` | 4 | Admin pause/unpause and paused mutation guards |
| `property_invariants.rs` | 3 | Payout conservation and stats monotonicity |
| `resolution.rs` | 22 | Up/Down and Precision payout behavior, ties, remainders |
| `security.rs` | 4 | Oracle freshness, future timestamp, round-id replay, valid payload |
| `storage_benchmarks.rs` | 5 | Indexed key writes and resolution cleanup |
| `windows.rs` | 13 | Window bounds, timing, auth, precision window enforcement |

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
- **Oracle inputs:** `OraclePayload` enforces non-zero price, timestamp not in the future, 300-second freshness, and round binding against `Round.start_ledger`.
- **Resource limits:** resolution is O(n) over participants. Precision rounds include an admin-configurable participant cap; operators still need benchmark evidence before raising it near upper bounds.

## Findings

| ID | Severity | Status | Finding | Evidence / code locations | Impact | Mitigation plan / owner |
| --- | --- | --- | --- | --- | --- | --- |
| SR-2026-04-001 | Medium | Open | TypeScript `ContractError` map is stale for Rust variants 24 and 25. Method parity passes, but client-side decoding lacks `FutureOracleData` and `PayoutOverflow`. | Rust variants: `contracts/src/errors.rs:56-59`. Binding map stops at 23: `bindings/src/index.ts:39-132`. `npm --prefix bindings run test:parity` only checks methods in `bindings/src/parity.js`. | Frontends, bots, and monitoring may display unknown errors or mis-handle future oracle timestamps and payout overflow failures. | Owner: Bindings maintainer (TBD). Regenerate/update bindings, add enum/error parity to `bindings/src/parity.js`, then run `npm --prefix bindings run test:parity` and `npm --prefix bindings run lint`. |
| SR-2026-04-002 | Low | Open | Indexed Precision payout path uses generic `Overflow` for total pot, diff, remainder, and pending accumulation, while legacy Precision and Up/Down payout helpers use `PayoutOverflow`. | Indexed path: `contracts/src/contract.rs:913-973`. Helper policy: `contracts/src/contract.rs:1284-1302`. Tests assert `PayoutOverflow` for claim/refund/updown paths in `contracts/src/tests/overflow_tests.rs`. | Inconsistent error semantics can make payout incident triage harder, although arithmetic is still checked and no unchecked overflow was found. | Owner: Contract maintainer (TBD). Route indexed Precision payout arithmetic through `payout_add` where applicable and add a regression test for precision payout overflow. |
| SR-2026-04-003 | Low | Accepted risk | Oracle payload round binding uses `round.start_ledger` as the payload `round_id`, not the monotonic `Round.round_id`. | `OraclePayload.round_id: u32` documented as `Round.start_ledger` in `contracts/src/types.rs`; check at `contracts/src/contract.rs:648-650`; stable `Round.round_id` exists at `contracts/src/contract.rs:130-152`. | This blocks cross-round replay while only one active round exists, but the field name is ambiguous and external oracle operators may supply `Round.round_id` by mistake. | Owner: Oracle integration owner (TBD). Keep accepted for current ABI; document oracle payload semantics in integration docs. For the next breaking ABI, rename field or switch to `u64 round_id` matching `Round.round_id`. |
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

## Severity Summary

| Status | Critical | High | Medium | Low | Total |
| --- | ---: | ---: | ---: | ---: | ---: |
| Open | 0 | 0 | 1 | 1 | 2 |
| Accepted risk | 0 | 0 | 1 | 2 | 3 |
| Mitigated | 0 | 3 | 4 | 2 | 9 |
| **Total** | **0** | **3** | **6** | **5** | **14** |

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

- Binding error map drift is the only open medium item found in this pass.
- Single-oracle trust remains the largest accepted protocol-level risk.
- Large participant rounds may hit Soroban resource ceilings during resolution if operational caps are tuned too high.
- Current review is maintainer-focused and should not replace an external audit for mainnet.

## Verification Evidence

Commands run during this review:

```text
cargo test
running 118 tests
test result: ok. 118 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

npm --prefix bindings run test:parity
ABI parity check passed: All contract methods are synced with TS bindings.
```

Important caveat: `bindings/src/parity.js` validates public method parity, not error enum parity. That is why SR-2026-04-001 remains open even though the parity script passes.

## Follow-Up Backlog

| Priority | Item | Owner | Target evidence |
| --- | --- | --- | --- |
| P1 | Fix binding error map for `FutureOracleData` and `PayoutOverflow`; add error enum parity check. | Bindings maintainer (TBD) | Updated `bindings/src/index.ts`, enhanced `bindings/src/parity.js`, passing `npm --prefix bindings run test:parity` |
| P2 | Normalize indexed Precision payout overflow errors to `PayoutOverflow`. | Contract maintainer (TBD) | Updated arithmetic path and new precision overflow regression test |
| P2 | Document oracle payload `round_id` semantics for integrators. | Oracle integration owner (TBD) | README / integration docs showing `payload.round_id = activeRound.start_ledger` |
| P2 | Define oracle operations and incident response before mainnet. | Protocol/security owner (TBD) | Oracle runbook, monitoring checks, pause criteria |
| P3 | Maintain operational monitoring for large participant rounds and benchmark cap increases. | Contract/product owner (TBD) | Participant-count policy, resource benchmark, and cap-change runbook |
| P3 | Schedule external audit before mainnet deployment. | Maintainers (TBD) | Audit report or issue tracking external findings |
| **Closed** | CEI ordering fixes for `claim_winnings` and `cancel_config_change` (SR-2026-06-001, SR-2026-06-002). | #195 | Merged with regression tests in `contracts/src/tests/cei_ordering.rs`; full audit in [`docs/CEI_AUDIT.md`](./docs/CEI_AUDIT.md). |

## Deployment Recommendation

Proceed with testnet/integration usage after assigning owners for the open items. Do not treat the current state as mainnet-ready until:

1. SR-2026-04-001 is closed.
2. Oracle operations and pause response are documented.
3. Maintainers decide whether to fix or explicitly accept SR-2026-04-002.
4. An external audit is completed for any production-value deployment.
