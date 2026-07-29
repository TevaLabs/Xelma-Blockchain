---
name: Bug report
about: Report a reproducible bug in contract, bindings, or CI behavior
title: "[Bug] "
labels: bug
assignees: ""
---

> **Before you start:** Read [`CONTRIBUTING.md`](../../CONTRIBUTING.md) for workflow expectations and the [Contributor Task Matrix](../../docs/CONTRIBUTOR_TASK_MATRIX.md) for test evidence requirements. Use the private disclosure path in [`SUPPORT.md`](../../SUPPORT.md) if this could be a security vulnerability.

## Summary

Describe the problem clearly.

## Area

- [ ] Contract (`contracts/`)
- [ ] Bindings (`bindings/`)
- [ ] CI/Release workflows (`.github/workflows/`)
- [ ] Documentation

## Reproduction

1.
2.
3.

## Expected behavior

## Actual behavior

## Evidence

- Logs, screenshots, or error output
- Relevant commit or PR links

## Security impact

- [ ] No security impact (publicly safe to discuss)
- [ ] Potential vulnerability — **stop here** and follow the private disclosure path in [`SUPPORT.md`](../../SUPPORT.md) before filing

## Proof-of-work validation (required before PR)

- [ ] `cargo test --workspace` passes (or a regression test proving the bug exists)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cd bindings && npm ci && npm run build && npm run test:parity` passes (if bindings affected)

## Difficulty

beginner | intermediate | advanced

---

### Label guidance
- Use `bug` for all bug reports.
- Add `security` if the bug has security implications (after private disclosure).
- Add `blockchain` / `contract` for contract bugs, `Rust` for Rust-specific issues.
- Add `priority:` labels if you can assess severity (`priority: high`, `priority: medium`, `priority: low`).
