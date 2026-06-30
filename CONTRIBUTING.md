# Contributing to Xelma-Blockchain

Thanks for improving Xelma. This document explains the expected workflow for contributors and maintainers.

## Before You Start

- Open or reference an issue describing the change.
- Keep changes focused and easy to review.
- For contract changes, include or update tests.

## Filing Issues

Blank issues are disabled. Pick the template that matches your work so every
backlog item ships with the technical detail maintainers need to triage it:

- **Bug report** — a reproducible defect in contract, bindings, or CI.
- **Feature request** — a general enhancement to logic, tooling, or docs.
- **Protocol improvement** — changes to contract logic, economics, or lifecycle.
- **Security hardening task** — public, non-sensitive defense-in-depth work.
  (Never disclose exploitable vulnerabilities publicly — use the private path
  in `SUPPORT.md`.)
- **Test task** — additional unit, property, chaos, or benchmark coverage.

The protocol, security, and test templates require **risk**, **scope**,
**acceptance criteria**, and a **test plan**. Issues missing these fields may be
sent back for detail before they are picked up.

## Development Setup

1. Fork and clone the repository.
2. Install Rust and Node.js dependencies.
3. Run local validation before opening a PR:
   - `cargo test --workspace`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo fmt --all -- --check`
   - `cd bindings && npm ci && npm run build`
   - `cd bindings && npm run test:parity` (ABI drift check; mirrors the CI `bindings-test` job)

Before opening a PR, consult [`docs/CONTRIBUTOR_TASK_MATRIX.md`](./docs/CONTRIBUTOR_TASK_MATRIX.md) for task-type-specific test and evidence requirements.

## Optional pre-commit hooks

This repository ships an optional pre-commit hook configuration to catch trivial
issues before push. Hooks are **opt-in** — CI remains the source of truth.

### Install

```bash
pip install pre-commit   # or your package manager
pre-commit install
```

After install, hooks run automatically on `git commit`. They execute:

1. `cargo fmt --check` — formatting guard
2. `cargo clippy --all-targets --all-features -- -D warnings` — targeted lint
3. `cargo test --lib` — quick test subset (unit + internal tests, no integration/runtime)

### Opt-out for a single commit

```bash
git commit --no-verify
```

### Remove entirely

```bash
pre-commit uninstall
```

### Installer script

```bash
git clone https://github.com/TevaLabs/Xelma-Blockchain
cd Xelma-Blockchain
pip install pre-commit
pre-commit install
```
## Snapshot Tests

The project uses storage-snapshot golden files (`contracts/test_snapshots/`) to detect
unintentional changes to contract state, event emissions, and error behavior.

### When snapshots should change

- You modified contract logic, storage keys, event payloads, or error variants.
- You made non-semantic refactors that still cause snapshot output to differ (rare).

### When snapshots should NOT change

- Your change is in an unrelated module, test infrastructure, or documentation.
- CI reports snapshot drift that you did not intend — investigate before regenerating.

### Updating snapshots

After an intentional behavior change, regenerate golden files from the repo root:

```bash
./scripts/update_snapshots.sh
```

Then review the diff, run the full suite, and commit the updated snapshots alongside
your logic change. See [`contracts/test_snapshots/README.md`](./contracts/test_snapshots/README.md)
for a step-by-step guide.

## Security Checks (local)

The CI `security-audit` job runs two checks that maintainers and contributors can reproduce locally.

### 1. Dependency vulnerability scan

```bash
# Install once
# Install once (pin matches CI CARGO_AUDIT_VERSION in .github/workflows/ci.yml)
cargo install cargo-audit --version 0.22.2 --locked

# Run from the repo root
cargo audit --deny warnings
```

Findings map to RustSec advisories. A non-zero exit means at least one advisory-level issue
was found. Review the output and update or replace the affected crate, or add an `audit.toml`
ignore entry with a justification comment if the advisory does not apply to this project.

### 2. Security-oriented clippy lints

```bash
cargo clippy --workspace --all-targets --locked -- \
  -D clippy::unwrap_used \
  -D clippy::expect_used \
  -D clippy::panic \
  -D clippy::integer_arithmetic \
  -W clippy::arithmetic_side_effects \
  -W clippy::cast_possible_truncation \
  -W clippy::cast_sign_loss
```

These lints catch patterns that are benign in general Rust but unsafe in smart-contract
contexts (silent panics, unchecked integer operations, sign-loss on casts).  CI treats them
as warnings that are surfaced in the audit job output; errors `-D` will fail the job.

> **Note**: These lints are stricter than the standard `cargo clippy -- -D warnings` run in
> the `rust-test` job. It is normal for code that passes standard clippy to have findings here.
> Fix or document each finding before merging contract changes.

## Code Coverage

Before opening a PR, verify that critical contract paths remain covered:

```bash
# Install cargo-llvm-cov (one-time)
cargo install cargo-llvm-cov

# Generate coverage for the workspace
cargo llvm-cov --all-features --workspace --locked
```

To view a detailed HTML report:

```bash
cargo llvm-cov --all-features --workspace --html --output-dir coverage-report --locked
# Open coverage-report/html/index.html in a browser
```

CI enforces a 90% line-coverage threshold on `contracts/src/contract.rs` and
80% overall workspace coverage.

## Canonical Contract Crate

The contract crate name is `xelma-contract`. Do not reintroduce legacy crate naming in build scripts, docs, or CI commands.

## Pull Request Expectations

- Link the issue(s) the PR closes.
- Describe behavioral impact and migration assumptions.
- Include test evidence for the changed behavior.
- Keep generated build artifacts out of commits (`target/`, `bindings/dist/`, etc.).
- For contract ABI, storage, or event changes, classify the impact (MAJOR/MINOR/PATCH) using [COMPATIBILITY_POLICY.md](./COMPATIBILITY_POLICY.md) and bump `Cargo.toml` accordingly.

## Review and Merge Policy

- At least one maintainer approval is required for non-trivial changes.
- Contract, CI, or release workflow changes require review from a listed code owner.
- Maintainers may request follow-up hardening before merge when security or correctness risk exists.

## Security Reporting

Do not open public issues for vulnerabilities. Follow the process in `SUPPORT.md` and repository security policy/disclosure instructions.
