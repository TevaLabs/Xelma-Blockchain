---
name: Test task
about: Request additional test coverage (unit, property, chaos, benchmark)
title: "[Test] "
labels: testing
assignees: ""
---

> **Before you start:** Read [`CONTRIBUTING.md`](../../CONTRIBUTING.md) for workflow expectations and the [Contributor Task Matrix](../../docs/CONTRIBUTOR_TASK_MATRIX.md) for test-specific evidence requirements. The tests are the deliverable — the PR must demonstrate that the added tests cover a genuine gap.

## Summary

One-line description of the coverage gap.

## What needs coverage

Describe the behavior, path, or invariant that is currently under-tested. Reference the [invariant coverage matrix](../../PROTOCOL_SPEC.md#invariant-coverage-matrix) (I1–I13) if applicable.

## Where to work

- [ ] `contracts/src/tests/` (unit / integration)
- [ ] `contracts/src/tests/property_invariants.rs` (property)
- [ ] `contracts/src/tests/chaos_recovery.rs` (chaos / recovery)
- [ ] `contracts/src/tests/storage_benchmarks.rs` / benchmarks (performance)
- [ ] `contracts/src/tests/event_coverage.rs` (event emission)
- [ ] `contracts/src/tests/cei_ordering.rs` (CEI ordering)
- [ ] `contracts/src/tests/commit_reveal_e2e.rs` (commit-reveal)
- [ ] `bindings/tests/` (TypeScript bindings)

## Risk

What can break silently today because this is not tested?

## Scope

- [ ] Happy path
- [ ] Failure / revert paths
- [ ] Boundary values
- [ ] Multi-round / lifecycle interactions

## Acceptance criteria

- [ ] Tests fail before the fix / cover the gap
- [ ] Tests pass on `cargo test --workspace`
- [ ] Test names follow the existing `test_<behaviour>_<condition>` convention
- [ ] No new public contract methods or storage keys introduced (open a separate Protocol Improvement issue if needed)

## Test plan

List the specific cases to add. Show that no existing test would have caught this scenario.

-
-

## Docs reference

- [ ] [`PROTOCOL_SPEC.md`](../../PROTOCOL_SPEC.md) — update invariant coverage matrix if new evidence added
- [ ] [`CONTRIBUTOR_TASK_MATRIX.md`](../../docs/CONTRIBUTOR_TASK_MATRIX.md) — confirm domain-specific test requirements are met

## Proof-of-work validation (required before PR)

- [ ] `cargo test --workspace` passes with new tests
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cd bindings && npm ci && npm run build && npm run test:parity` passes (if bindings tests added)

## Difficulty

beginner | intermediate | advanced

---

### Label guidance
- Use `testing` for all test tasks.
- Add `blockchain` / `contract` for contract test coverage.
- Add `Rust` for Rust test work, `Stellar Wave` if eligible.
- Add `priority:` labels based on coverage gap severity (`priority: high` for critical untested paths, `priority: medium`, `priority: low`).
