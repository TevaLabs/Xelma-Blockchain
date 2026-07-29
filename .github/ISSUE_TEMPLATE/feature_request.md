---
name: Feature request
about: Propose an enhancement to contract logic, tooling, or docs
title: "[Feature] "
labels: enhancement
assignees: ""
---

> **Before you start:** Read [`CONTRIBUTING.md`](../../CONTRIBUTING.md) for workflow expectations and the [Contributor Task Matrix](../../docs/CONTRIBUTOR_TASK_MATRIX.md) for test evidence requirements. A feature PR must cover the happy path, at least one failure/revert path, and at least one edge-case boundary.

## Problem statement

What user/developer pain does this solve?

## Proposed change

Describe the desired behavior and constraints.

## Where to work

List the files/modules involved (e.g. `contracts/src/contract.rs`, `contracts/src/types.rs`, `bindings/src/index.ts`).

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

## Acceptance criteria

List verifiable, testable criteria for completion.

- [ ]
- [ ]

## Test plan

Describe the tests that prove the change is correct (unit, property, chaos, benchmark). Reference the [Contributor Task Matrix](../../docs/CONTRIBUTOR_TASK_MATRIX.md) for domain-specific requirements.

- Happy path:
- Failure/revert path:
- Edge case / boundary:
-

## Docs reference

- [ ] [`PROTOCOL_SPEC.md`](../../PROTOCOL_SPEC.md) — if invariants (I1–I13) are affected
- [ ] [`docs/EVENT_SCHEMA.md`](../../docs/EVENT_SCHEMA.md) — if new events are added
- [ ] [`MIGRATION.md`](../../MIGRATION.md) — if breaking ABI, storage, or event changes
- [ ] [`COMPATIBILITY_POLICY.md`](../../COMPATIBILITY_POLICY.md) — for MAJOR/MINOR/PATCH classification

## Proof-of-work validation (required before PR)

- [ ] `cargo test --workspace` passes with new tests
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cd bindings && npm ci && npm run build && npm run test:parity` passes (if ABI changed)

## Difficulty

beginner | intermediate | advanced

---

### Label guidance
- Use `enhancement` for feature requests.
- Add `protocol` if this changes contract logic or economics.
- Add `blockchain` / `contract` for contract changes, `Rust` for Rust-specific work.
- Add `priority:` labels based on urgency (`priority: high`, `priority: medium`, `priority: low`).
