---
name: Security hardening task
about: Track a non-sensitive security hardening or defense-in-depth task
title: "[Security] "
labels: security
assignees: ""
---

> ⚠️ Do **not** disclose exploitable vulnerabilities here. For undisclosed
> vulnerabilities, follow the private disclosure path in [`SUPPORT.md`](../../SUPPORT.md).
> Use this template only for hardening work that is safe to discuss publicly.
>
> **Before you start:** Read [`CONTRIBUTING.md`](../../CONTRIBUTING.md) for workflow expectations and the [Contributor Task Matrix](../../docs/CONTRIBUTOR_TASK_MATRIX.md) for security-specific test evidence requirements. Security PRs must supply negative tests proving the mitigation holds.

## Summary

One-line description of the hardening task.

## Threat / weakness

Describe the weakness or attack surface being addressed. Reference any related findings from [`SECURITY_REVIEW.md`](../../SECURITY_REVIEW.md) if applicable.

## Where to work

-
-

## Risk

- [ ] Critical (funds / authorization)
- [ ] High (resolution / accounting correctness)
- [ ] Medium (DoS / griefing / state confusion)
- [ ] Low (defense-in-depth / logging)

Explain impact if left unaddressed. State the worst-case scenario.

## Scope

- [ ] Contract behavior
- [ ] Bindings/API shape
- [ ] CI/release flow
- [ ] Documentation/governance

## Acceptance criteria

- [ ]
- [ ]

## Test plan

Security-relevant tests proving the mitigation (negative tests, replay/abuse cases). See the [Contributor Task Matrix](../../docs/CONTRIBUTOR_TASK_MATRIX.md) 
for the security-specific requirements.

- Negative test for the attack vector (assert correct error code by numeric value):
- Replay / abuse case:
-

## Docs reference

- [ ] [`SECURITY_REVIEW.md`](../../SECURITY_REVIEW.md) — note if this addresses or relates to an open finding
- [ ] [`PROTOCOL_SPEC.md`](../../PROTOCOL_SPEC.md) — update threat model or invariants if affected
- [ ] [`MIGRATION.md`](../../MIGRATION.md) — if error codes or authorization checks change

## Proof-of-work validation (required before PR)

- [ ] `cargo test --workspace` passes with negative tests proving the mitigation
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] Security clippy passes:
  ```
  cargo clippy --workspace --all-targets --locked -- \
    -D clippy::unwrap_used \
    -D clippy::expect_used \
    -D clippy::panic \
    -D clippy::integer_arithmetic \
    -W clippy::arithmetic_side_effects \
    -W clippy::cast_possible_truncation \
    -W clippy::cast_sign_loss
  ```
- [ ] `cargo audit --deny warnings` passes
- [ ] `cargo fmt --all -- --check` passes

## Difficulty

beginner | intermediate | advanced

---

### Label guidance
- Use `security` for all security hardening tasks.
- Add `blockchain` / `contract` for contract-level hardening.
- Add `Rust` for Rust-specific security work.
- Add `priority:` labels based on risk severity (`priority: high` for critical/high risk, `priority: medium`, `priority: low`).
