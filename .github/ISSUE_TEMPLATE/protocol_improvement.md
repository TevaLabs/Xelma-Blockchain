---
name: Protocol improvement
about: Propose a change to contract logic, economics, or lifecycle behavior
title: "[Protocol] "
labels: protocol, enhancement
assignees: ""
---

> **Before you start:** Read [`CONTRIBUTING.md`](../../CONTRIBUTING.md) for workflow expectations and the [Contributor Task Matrix](../../docs/CONTRIBUTOR_TASK_MATRIX.md) for protocol-specific test evidence requirements. Protocol changes must demonstrate full invariant coverage for any invariant they touch (see [invariant coverage matrix](../../PROTOCOL_SPEC.md#invariant-coverage-matrix) I1–I13).

## Summary

One-line description of the protocol change.

## Why this change matters

Explain the user, economic, or operational problem this solves.

## Where to work

List the files/modules involved (e.g. `contracts/src/contract.rs`, `contracts/src/types.rs`).

-
-

## What should be implemented

Describe the concrete behavior to add or change.

-
-

## Risk

- [ ] Funds movement / accounting
- [ ] Oracle / resolution path
- [ ] Lifecycle / round state
- [ ] Storage layout / migration
- [ ] None of the above

Explain the worst-case impact and any backward-compatibility concerns.

## Scope

- [ ] Contract behavior
- [ ] Bindings/API shape
- [ ] CI/release flow
- [ ] Documentation/governance

## Affected invariants

Check the [invariant coverage matrix](../../PROTOCOL_SPEC.md#invariant-coverage-matrix) and list which invariants (I1–I13) this change touches:

- [ ] I1 — Single active round
- [ ] I2 — Role authorization
- [ ] I3 — Pause safety
- [ ] I4 — Round timing
- [ ] I5 — One position per user per round
- [ ] I6 — Mode isolation
- [ ] I7 — Balance and pending-winnings accounting
- [ ] I8 — Settlement conservation
- [ ] I9 — Checked arithmetic
- [ ] I10 — Oracle payload binding
- [ ] I11 — Cancellation and fallback refunds
- [ ] I12 — Storage cleanup and migration compatibility
- [ ] I13 — Event semantics

## Acceptance criteria

List verifiable, testable criteria for completion.

- [ ]
- [ ]

## Test plan

Describe the tests that prove the change is correct (unit, property, chaos, benchmark). Reference the [Contributor Task Matrix](../../docs/CONTRIBUTOR_TASK_MATRIX.md) for domain-specific requirements.

- Happy path:
- Failure/revert path:
- Edge case / boundary:
- Property / invariant:
-

## Docs reference

- [ ] [`PROTOCOL_SPEC.md`](../../PROTOCOL_SPEC.md) — update invariant evidence entries
- [ ] [`docs/EVENT_SCHEMA.md`](../../docs/EVENT_SCHEMA.md) — update event schema if new events added
- [ ] [`MIGRATION.md`](../../MIGRATION.md) — document breaking ABI, storage, or event changes
- [ ] [`COMPATIBILITY_POLICY.md`](../../COMPATIBILITY_POLICY.md) — classify MAJOR/MINOR/PATCH impact
- [ ] [`SECURITY_REVIEW.md`](../../SECURITY_REVIEW.md) — note if open findings are affected

## Proof-of-work validation (required before PR)

- [ ] `cargo test --workspace` passes with new tests covering all affected invariants
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cd bindings && npm ci && npm run build && npm run test:parity` passes (if ABI changed)
- [ ] Benchmark output included if hot paths (`create_round`, `place_bet`, `resolve_round`) changed — see [`contracts/BENCHMARKS.md`](../../contracts/BENCHMARKS.md)

## Difficulty

beginner | intermediate | advanced

---

### Label guidance
- Use `protocol` + `enhancement` for all protocol improvements.
- Add `blockchain` / `contract` for contract-level changes.
- Add `Rust` for Rust-specific implementation work.
- Add `priority:` labels based on protocol impact (`priority: high`, `priority: medium`, `priority: low`).
- If the change affects security posture, also add `security`.
