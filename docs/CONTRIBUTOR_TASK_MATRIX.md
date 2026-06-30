# Contributor Task Matrix

This document maps each issue type and protocol domain to the specific tests,
evidence, and completion artifacts required before a PR is considered complete.
Use it alongside [`PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md),
which lists the base validation commands every PR must run.

---

## Base Gates (all PRs)

Every PR must clear these before domain-specific requirements apply.

| Gate | Command |
|---|---|
| All tests pass | `cargo test --workspace` |
| Lints clean | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Format correct | `cargo fmt --all -- --check` |
| Bindings build and parity | `cd bindings && npm ci && npm run build && npm run test:parity` |

> **Note on parity coverage:** `parity.js` checks public method parity only —
> it does not check error enum parity. If your PR adds a new `ContractError`
> variant, you must also update the error map in `bindings/src/index.ts`
> manually. The parity script will not catch the drift (open finding
> SR-2026-04-001).

---

## Part 1 — Requirements by Issue Type

### Bug Report

A bug fix PR must prove the defect is reproducible and that the patch closes it.

| Domain | Required test module(s) | Test categories | Evidence to include in PR |
|---|---|---|---|
| Architecture / Lifecycle | `lifecycle.rs`, `guard_tests.rs`, `chaos_recovery.rs` | Unit, regression | `cargo test <module>` output before patch (showing failure) and after (showing pass) |
| Oracle | `security.rs` | Unit, negative | Test output showing the bug's error code before and the correct behaviour after |
| Economics | `resolution.rs`, `overflow_tests.rs` | Unit, regression | State snapshot before and after — confirm no partial writes |
| Observability | `event_coverage.rs` | Unit | Before/after test output; if bindings were wrong, include updated `index.ts` diff |

**Definition of done:**
- A new regression test named to match the bug exists in the relevant module.
- The test fails on `main` and passes on the patch branch.
- `cargo test --workspace` is clean.

---

### Feature Request

A feature PR must cover the happy path, at least one failure/revert path, and
at least one edge-case boundary.

| Domain | Required test module(s) | Test categories | Evidence to include in PR |
|---|---|---|---|
| Architecture / Lifecycle | `lifecycle.rs`, `guard_tests.rs` | Unit, boundary | `cargo test <module> -- --nocapture` showing new event or storage key written and cleaned up |
| Oracle | `security.rs`, `mode_tests.rs` | Unit, negative | Negative test output showing correct error code on bad input |
| Economics | `resolution.rs`, `property_invariants.rs` | Unit, property | Payout conservation check; `cargo test property_invariants -- --nocapture` |
| Observability | `event_coverage.rs` | Unit | Test output showing new event topic pair emitted with correct payload field order |

**Definition of done:**
- Happy-path test, one revert-path test, and one boundary test exist.
- If a new public entrypoint is added: bindings are updated and `npm run test:parity` passes.
- If new events are added: `docs/EVENT_SCHEMA.md` is updated before the PR is opened.

---

### Protocol Improvement

Protocol changes must demonstrate full invariant coverage for any invariant they
touch. See the [invariant coverage matrix in PROTOCOL_SPEC.md](../PROTOCOL_SPEC.md)
for the authoritative list (I1–I13) and which test files cover each.

| Domain | Required test module(s) | Test categories | Evidence to include in PR |
|---|---|---|---|
| Architecture / Lifecycle | `lifecycle.rs`, `guard_tests.rs`, `chaos_recovery.rs`, `migration_versioning.rs` | Unit, chaos | Updated invariant row in `PROTOCOL_SPEC.md`; `cargo test lifecycle -- --nocapture` output |
| Oracle | `security.rs`, `mode_tests.rs` | Unit, negative, property | Test output for stale, future-dated, deviation, and replay cases |
| Economics | `resolution.rs`, `overflow_tests.rs`, `property_invariants.rs`, `cost_benchmarks.rs` | Unit, property, benchmark | `cargo test cost_benchmarks -- --nocapture` showing CPU/mem within budget; conservation invariant output |
| Observability | `event_coverage.rs` | Unit | Updated `docs/EVENT_SCHEMA.md` diff in PR; `npm run test:parity` output |

**Definition of done:**
- All affected invariants (I1–I13) in `PROTOCOL_SPEC.md` have updated evidence entries.
- If accounting or payout logic changed: `property_invariants.rs` payout conservation test passes.
- If any hot path changed (`create_round`, `place_bet`, `resolve_round`): benchmark output
  included in PR description showing CPU and memory remain within the Soroban budget
  (`100_000_000` CPU instructions, `100 MiB` memory). See [`contracts/BENCHMARKS.md`](../contracts/BENCHMARKS.md).
- If storage keys or struct fields changed: see the Architecture domain-specific
  requirements in Part 2.

---

### Security Hardening Task

Security PRs must supply negative tests proving the mitigation holds. Positive
happy-path tests are secondary; what matters is that the attack surface is
demonstrably closed.

| Domain | Required test module(s) | Test categories | Evidence to include in PR |
|---|---|---|---|
| Architecture / Lifecycle | `guard_tests.rs`, `pause.rs`, `chaos_recovery.rs` | Negative, chaos | Test output showing the correct `ContractError` variant and numeric code returned; security clippy output |
| Oracle | `security.rs` | Negative, replay | Negative test output for replay, stale, future-dated, and zero-price cases; confirm nonce consumed under `DataKey::ConsumedOracleNonce` |
| Economics | `overflow_tests.rs`, `resolution.rs` | Negative, overflow | Test output showing `PayoutOverflow (25)` — not generic `Overflow (11)` — on payout arithmetic failure |
| Observability | `security.rs`, `event_coverage.rs` | Negative | Confirm no sensitive data leaks into event payloads; `cargo audit --deny warnings` output |

**Security clippy — run before every security PR:**

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

**Definition of done:**
- Negative test exists for the specific attack vector, asserting the correct error
  code by numeric value (not just by name, since bindings consumers see the number).
- Security clippy output included in PR body showing no new `-D` violations.
- `cargo audit --deny warnings` is clean.
- PR description states the threat being addressed and the worst-case impact if left unpatched.

---

### Test Task

The tests are the deliverable. The PR must demonstrate that the added tests cover
a genuine gap, not a path that was already implicitly covered.

| Domain | Required test module(s) | Test categories | Evidence to include in PR |
|---|---|---|---|
| Architecture / Lifecycle | `lifecycle.rs`, `guard_tests.rs`, `chaos_recovery.rs` | Unit, chaos | Show that no existing test would have caught the scenario; `cargo test <module> -- --nocapture` output |
| Oracle | `security.rs` | Negative | Describe what oracle behaviour was previously untested |
| Economics | `property_invariants.rs`, `overflow_tests.rs` | Property, overflow | `cargo test property_invariants -- --nocapture` showing the new invariant assertion |
| Observability | `event_coverage.rs` | Unit | Show that the event field or ordering was previously unasserted |

**Definition of done:**
- Tests fail (or the gap is explained) before the addition, and pass after.
- Test names follow the existing `test_<behaviour>_<condition>` convention used
  in the codebase (e.g. `test_place_bet_twice_same_round`).
- No new public contract methods or storage keys are introduced — if the gap
  requires a contract change, open a separate Protocol Improvement issue.

---

## Part 2 — Domain-Specific Additions

These requirements stack on top of the per-issue-type requirements above.
Apply whichever domain(s) your change touches.

### Architecture / Lifecycle

Covers: `create_round`, `cancel_round`, `resolve_round`, the single-active-round
invariant, storage key additions and removals, schema migration, TTL management,
and the participant list.

| Change | Additional requirement |
|---|---|
| Any create/cancel/resolve change | Add or update a test in `chaos_recovery.rs` covering pause-then-resume or cancel-then-create across the change |
| New `DataKey` variant (persistent, long-lived) | Add a TTL extension test in `ttl_tests.rs`; call `_extend_persistent_ttl` at every read/write site |
| New `DataKey` variant (short-lived, round-scoped) | Verify the key is deleted in `resolve_round` and `cancel_round`; add an assertion to `storage_benchmarks.rs` |
| `#[contracttype]` struct field added or removed | MAJOR version bump (XDR encoding changes); add a migration path in `MIGRATION.md`; bump `CURRENT_SCHEMA_VERSION`; add a test in `migration_versioning.rs` |
| `DataKey` variant renamed or removed | MAJOR version bump; document cutover in `MIGRATION.md`; verify legacy fallback reads still work during migration window |
| Schema migration entrypoint | Must require no active round (`MigrationActiveRound` guard); test both the guard and the successful migration in `migration_versioning.rs` |

---

### Oracle

Covers: `resolve_round`, `update_oracle_heartbeat`, `OraclePayload` validation,
deviation guardrails, stale detection, and nonce deduplication.

| Change | Additional requirement |
|---|---|
| Any `resolve_round` change | Test all four payload rejection paths: zero price, future timestamp, stale timestamp, wrong `round_id` |
| Oracle payload field added or reordered | MAJOR version bump; document new semantics in `MIGRATION.md` and integration docs |
| Deviation guardrail logic | Test both below-threshold (passes) and above-threshold (fails with `OracleDeviationExceeded (41)`) |
| Override arm/clear logic | Test that the one-shot flag is consumed and cleared after a single use |
| Heartbeat / liveness change | Test all three status codes (0, 1, 2) and the invalid-status rejection (`InvalidOracleStatus (36)`) |

> **Field naming caveat:** `OraclePayload.round_id: u32` maps to
> `Round.start_ledger`, **not** the monotonic `Round.round_id: u64`. This is an
> accepted design ambiguity (SR-2026-04-003). PR descriptions touching oracle
> payload handling must state which field the payload is validated against, to
> avoid oracle operator confusion.

---

### Economics

Covers: balance accounting, payout formulas (UpDown proportional, Precision
closest/tie), risk control caps, pending winnings accumulation, and overflow
handling.

| Change | Additional requirement |
|---|---|
| Payout formula change | Update `property_invariants.rs` conservation test; run `cost_benchmarks.rs` with `--nocapture` and include output |
| New risk control cap | Test the cap boundary (amount exactly at limit passes, amount one stroop over fails with the correct error code) |
| Balance or pending-winnings path | Use `payout_add`/`payout_mul` — not raw `checked_add` — so overflow surfaces as `PayoutOverflow (25)`, not `Overflow (11)` |
| Precision tie-breaking change | Test a 3-way tie with a non-divisible pot; verify the remainder goes to the first winner and total distributed equals the full pot |
| Unchanged-price refund path | Test that every participant receives their exact stake back and the pool fields are zero after resolution |
| Any partial-write risk | Add a test that asserts storage state is unchanged after a failure return — no intermediate write should persist |

---

### Observability

Covers: on-chain events, `docs/EVENT_SCHEMA.md`, TypeScript bindings ABI, and
the `bindings/src/parity.js` error-enum gap.

| Change | Additional requirement |
|---|---|
| New event | Add a test in `event_coverage.rs` asserting the topic pair and each payload field by position; update `docs/EVENT_SCHEMA.md` before opening the PR |
| Changed event payload field | MAJOR version bump; update `docs/EVENT_SCHEMA.md`; notify indexer/frontend consumers in PR description |
| Changed event topic string | MAJOR version bump |
| New public entrypoint | Regenerate or update `bindings/src/index.ts`; confirm `npm run test:parity` passes; include `bindings/dist/index.d.ts` diff in PR |
| New `ContractError` variant | Manually add the variant to the error map in `bindings/src/index.ts` (the parity script does not check enum parity); include the updated map in the PR diff |
| Removed or renumbered `ContractError` variant | MAJOR version bump — clients that match the numeric code will break |

---

## Completion Artifact Examples

The following examples show what to paste in the PR description as evidence.

### Example A — Bug fix (regression test)

```
$ cargo test --package xelma-contract resolution::test_resolution_unchanged_price_refunds_all -- --nocapture

running 1 test
test tests::resolution::test_resolution_unchanged_price_refunds_all ... ok

test result: ok. 1 passed; 0 failed
```

Paste the output once for the patch branch. Explain in one sentence what the
pre-patch behaviour was (e.g. "Before this fix, an unchanged price incorrectly
paid out winners instead of refunding all participants").

---

### Example B — Protocol improvement (benchmark + invariant update)

```
$ cargo test --package xelma-contract cost_benchmarks -- --nocapture

[bench] create_round      cpu=    2_847_123  mem=    1_048_576
[bench] place_bet         cpu=    3_102_489  mem=    1_048_576
[bench] resolve_round     cpu=   18_334_201  mem=    4_194_304
[bench] claim_winnings    cpu=    1_983_204  mem=      524_288
```

Include the PROTOCOL_SPEC.md diff showing the updated invariant row:

```diff
-| I8 | Settlement conservation | payout/refund helpers | `resolution.rs`, `property_invariants.rs` | Covered |
+| I8 | Settlement conservation | payout/refund helpers | `resolution.rs`, `property_invariants.rs`, `overflow_tests.rs` | Covered |
```

---

### Example C — Security hardening (negative test + clippy)

```
$ cargo test --package xelma-contract security::test_oracle_nonce_reuse_rejected -- --nocapture

running 1 test
test tests::security::test_oracle_nonce_reuse_rejected ... ok

test result: ok. 1 passed; 0 failed
```

```
$ cargo clippy --workspace --all-targets --locked -- \
    -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic \
    -D clippy::integer_arithmetic

Checking xelma-contract v0.1.0
    Finished in 3.4s — 0 warnings, 0 errors
```

State the error code asserted: "Test asserts `OracleNonceReused (33)` is
returned on the second submission of the same nonce for a round, confirming
the `ConsumedOracleNonce(round_id, nonce)` key is written and checked."

---

## Cross-References

| Document | When to consult |
|---|---|
| [`PROTOCOL_SPEC.md`](../PROTOCOL_SPEC.md) | Invariant coverage matrix (I1–I13), trust model, threat model |
| [`PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md) | Base PR checklist and governance items |
| [`COMPATIBILITY_POLICY.md`](../COMPATIBILITY_POLICY.md) | MAJOR / MINOR / PATCH classification rules |
| [`SECURITY_REVIEW.md`](../SECURITY_REVIEW.md) | Open findings (SR-2026-04-001, SR-2026-04-002) and accepted risks |
| [`contracts/BENCHMARKS.md`](../contracts/BENCHMARKS.md) | How to record benchmark baselines and tighten guardrails |
| [`docs/EVENT_SCHEMA.md`](EVENT_SCHEMA.md) | Canonical event topic pairs, payload field order, and units |
| [`docs/storage_lifecycle.md`](storage_lifecycle.md) | Long-lived vs short-lived key classification and TTL policy |
| [`MIGRATION.md`](../MIGRATION.md) | Schema version migration history and consumer steps |
