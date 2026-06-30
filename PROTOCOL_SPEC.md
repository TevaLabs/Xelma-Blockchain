# Xelma Protocol Specification

This document defines the protocol guarantees, trust boundaries, and test
coverage map for the Xelma Soroban prediction market contract. It is intended
for maintainers, auditors, oracle operators, indexer authors, and contributors.

The implementation reference is `contracts/src/contract.rs`; the canonical data
types and errors live in `contracts/src/types.rs` and `contracts/src/errors.rs`.

## Protocol Scope

Xelma is a virtual-token prediction market for XLM price movement. Users receive
one initial vXLM allocation, stake it into one active round, and later claim
pending winnings after oracle resolution or refund paths.

The contract supports two round modes:

| Mode | Value | Settlement rule |
|---|---:|---|
| Up/Down | `0` | Correct side receives stake plus a proportional share of the losing pool. Unchanged price refunds all participants. |
| Precision | `1` | Closest price prediction wins the pot. Ties split the pot evenly; deterministic remainder goes to the first winner in resolution order. |

All token amounts are stored as `i128` stroops where `1 vXLM = 10_000_000`.
Prices are stored as `u128` values scaled to 4 decimal places.

## Roles and Trust Assumptions

| Role | Trust level | Authority | Assumptions |
|---|---|---|---|
| Admin | Trusted operator | Initialize roles, create rounds, configure windows and risk controls, pause/unpause, cancel active rounds. | Admin key custody is secure; admin does not maliciously configure unusable windows or cancel rounds unfairly. |
| Oracle | Trusted data signer | Resolve rounds, submit liveness heartbeat. | Oracle reports accurate prices for the intended round and submits fresh payloads. |
| Users | Untrusted | Mint once, bet/predict, claim winnings, read state. | Users may attempt invalid auth, duplicate bets, timing abuse, overflow inputs, and replay-like calls. |
| Indexers/frontends | Off-chain consumers | Read events and contract state. | Consumers treat `docs/EVENT_SCHEMA.md` as canonical and handle additive events safely. |

The protocol does not currently prove oracle correctness on-chain. A valid
oracle signature plus payload validation establishes authorization and
freshness, not price truth. Mainnet use requires an operational oracle runbook,
monitoring, and incident response policy.

## Core Invariants

### I1. Single Active Round

At most one round can be active at a time. `create_round` must fail without
mutating round state when `ActiveRound` already exists.

Evidence:
- Code: `assert_no_active_round`, `create_round`.
- Tests: `guard_tests.rs`, `lifecycle.rs::test_create_round_while_active_fails`.
- Docs: `ROUND_LIFECYCLE.md`.

### I2. Role Authorization

State-changing entrypoints require the relevant signer:

| Entrypoint class | Required signer |
|---|---|
| Admin operations | `Admin` |
| Oracle resolution and heartbeat | `Oracle` |
| User mint, bet, prediction, claim | `User` |

Unauthorized calls must fail before meaningful state mutation.

Evidence:
- Code: `require_auth()` calls in admin, oracle, and user entrypoints.
- Tests: `initialization.rs`, `lifecycle.rs`, `pause.rs`, `windows.rs`, `security.rs`.

### I3. Pause Safety

When paused, high-risk mutating operations are rejected. Read-only queries remain
available so operators can inspect contract state during an incident.

Evidence:
- Code: `_ensure_not_paused`, `pause_contract`, `unpause_contract`.
- Tests: `pause.rs`, `chaos_recovery.rs::test_chaos_pause_mid_round_then_unpause_resolve`.

### I4. Round Timing

Bets and predictions are only accepted before `bet_end_ledger`. Resolution is
only accepted at or after `end_ledger`. Admin-configured windows must be
positive, bounded, and must close betting before resolution.

Evidence:
- Code: `set_windows`, `place_bet`, `place_precision_prediction`, `resolve_round`.
- Tests: `windows.rs`.

### I5. One Position Per User Per Round

A user may hold at most one Up/Down position or one Precision prediction in the
active round. Duplicate submissions fail.

Evidence:
- Code: indexed keys `Position(round_id, user)` and
  `PrecisionPosition(round_id, user)`.
- Tests: `betting.rs::test_place_bet_twice_same_round`,
  `mode_tests.rs::test_precision_prediction_already_bet`.
- Docs: `STORAGE_DESIGN.md`.

### I6. Mode Isolation

Up/Down bets are only valid in Up/Down rounds. Precision predictions are only
valid in Precision rounds.

Evidence:
- Code: `RoundMode` checks in betting entrypoints.
- Tests: `mode_tests.rs`.

### I7. Balance and Pending-Winnings Accounting

User balances cannot go negative. Bets deduct the staked amount before storing a
position. Resolution credits payouts to `PendingWinnings(user)`. Claims move
pending winnings to balance atomically and clear the pending entry.

Evidence:
- Code: `balance`, `_set_balance`, `_accumulate_pending`, `claim_winnings`.
- Tests: `betting.rs`, `resolution.rs`, `overflow_tests.rs`.

### I8. Settlement Conservation

For each resolved round, credited payouts must not exceed the round pot. Refund
paths return participant stake amounts. Precision tie remainders are assigned
deterministically and no dust is intentionally lost.

Evidence:
- Code: `_resolve_updown_mode`, `_resolve_precision_mode`, refund helpers.
- Tests: `resolution.rs`, `property_invariants.rs`,
  `storage_benchmarks.rs::bench_large_round_resolves_correctly`.

### I9. Checked Arithmetic

Arithmetic that can affect balances, pending winnings, pools, windows, round
IDs, and stats must use checked operations or bounded validation. Overflow must
return a contract error and avoid partial writes in covered payout paths.

Evidence:
- Code: `checked_*`, `payout_add`, `payout_mul`.
- Tests: `overflow_tests.rs`, `edge_cases.rs`.

### I10. Oracle Payload Binding

Oracle resolution payloads must bind to the active round, contain a non-zero
price, use a fresh per-round nonce, and satisfy timestamp freshness checks.

Current payload semantics:
- `payload.round_id` is matched against `Round.start_ledger`.
- `payload.nonce` must not already be consumed for `Round.round_id`.
- `payload.timestamp` must not be future-dated.
- `payload.timestamp` must not be stale beyond the configured contract policy.

Evidence:
- Code: `resolve_round`.
- Tests: `security.rs`.
- Residual risk: `payload.round_id` naming is ambiguous because it currently
  refers to `Round.start_ledger`; this is tracked in `SECURITY_REVIEW.md`.

### I11. Cancellation and Fallback Refunds

Admin cancellation and insufficient-participant fallback paths must refund
participant stakes to pending winnings, remove active round state, and allow
future rounds to be created.

Evidence:
- Code: `cancel_round`, `_refund_under_threshold`.
- Tests: `lifecycle.rs`, `resolution.rs`, `chaos_recovery.rs`.

### I12. Storage Cleanup and Migration Compatibility

Resolution and cancellation must remove indexed participant keys and participant
lists for the completed round. Legacy map keys remain readable for migration
fallbacks but are not written by new betting paths.

Evidence:
- Code: indexed storage writes and cleanup in `contract.rs`.
- Tests: `storage_benchmarks.rs`.
- Docs: `STORAGE_DESIGN.md`.

### I13. Event Semantics

Events are an append-only observability interface. Existing event topic and
payload meanings must remain stable. Additive events are allowed when documented
in `docs/EVENT_SCHEMA.md`.

Canonical event classes:
- Round lifecycle: `("round", "created")`, `("round", "resolved")`,
  `("round", "cancelled")`, `("round", "fallback")`.
- User actions: `("mint", "initial")`, `("bet", "placed")`,
  `("predict", "price")`, `("claim", "winnings")`.
- Configuration/liveness: `("windows", "updated")`,
  `("oracle", "heartbeat")`.

Evidence:
- Code: event publishing calls in `contract.rs`.
- Tests: `lifecycle.rs`, `mode_tests.rs`, `resolution.rs`, `security.rs`.
- Docs: `docs/EVENT_SCHEMA.md`.

## Threat Model

### In Scope

- Unauthorized user, admin, or oracle calls.
- Duplicate bets and duplicate predictions.
- Late betting and premature resolution.
- Oracle payload replay across rounds or within a round.
- Stale or future-dated oracle timestamps.
- Invalid round mode usage.
- Arithmetic overflow in accounting paths.
- Storage growth and write amplification during betting.
- Indexer ambiguity caused by undocumented events.

### Out of Scope

- Malicious but authorized admin behavior.
- Compromised oracle signer submitting fresh but false prices.
- Off-chain price feed quality, exchange outages, or aggregation logic.
- Wallet UX, frontend signing prompts, and phishing resistance.
- Stellar network-level consensus failures.
- External economic/legal/regulatory risk of prediction markets.

### Accepted Trust Boundaries

- The admin can pause and cancel rounds. These controls are intended for
  recovery and are not trustless governance.
- The oracle is a single trusted signer in the current architecture.
- Resolution remains O(n) over participants; very large rounds may hit Soroban
  resource limits before protocol-level settlement logic completes.
- TypeScript bindings are generated artifacts and must be kept in parity with
  the Rust contract for safe client use.

## Upgrade and Compatibility Guarantees

- Backward-compatible documentation and additive events do not require a
  migration entry unless they alter consumer assumptions.
- Breaking changes to storage keys, public method signatures, event payload
  order, error codes, or `OraclePayload` semantics must be documented in
  `MIGRATION.md`.
- Legacy storage fallbacks may be removed only after maintainers explicitly
  decide no deployed migration window depends on them.
- Any new public contract method must be reflected in TypeScript bindings and
  parity checks.

## Invariant Coverage Matrix

| ID | Invariant | Primary code | Test coverage | Status |
|---|---|---|---|---|
| I1 | Single active round | `create_round`, `assert_no_active_round` | `guard_tests.rs`, `lifecycle.rs` | Covered |
| I2 | Role authorization | `require_auth()` gates | `initialization.rs`, `lifecycle.rs`, `pause.rs`, `windows.rs`, `security.rs` | Covered |
| I3 | Pause safety | `_ensure_not_paused` | `pause.rs`, `chaos_recovery.rs` | Covered |
| I4 | Round timing | `set_windows`, betting/resolution ledger checks | `windows.rs` | Covered |
| I5 | One position per user | indexed position keys | `betting.rs`, `mode_tests.rs` | Covered |
| I6 | Mode isolation | `RoundMode` checks | `mode_tests.rs` | Covered |
| I7 | Balance and pending accounting | `_set_balance`, `_accumulate_pending`, `claim_winnings` | `betting.rs`, `resolution.rs`, `overflow_tests.rs` | Covered |
| I8 | Settlement conservation | payout/refund helpers | `resolution.rs`, `property_invariants.rs` | Covered |
| I9 | Checked arithmetic | `checked_*`, `payout_add`, `payout_mul` | `overflow_tests.rs`, `edge_cases.rs` | Covered with noted precision-error caveat in `SECURITY_REVIEW.md` |
| I10 | Oracle payload binding | `resolve_round` | `security.rs` | Covered |
| I11 | Cancellation/fallback refunds | `cancel_round`, `_refund_under_threshold` | `lifecycle.rs`, `resolution.rs`, `chaos_recovery.rs` | Covered |
| I12 | Storage cleanup/migration | indexed cleanup and legacy fallbacks | `storage_benchmarks.rs` | Covered |
| I13 | Event semantics | event publishing calls | `lifecycle.rs`, `mode_tests.rs`, `resolution.rs`, `security.rs` | Covered; canonical schema in `docs/EVENT_SCHEMA.md` |

## Contributor Checklist

When changing protocol behavior:

1. Identify which invariant is affected.
2. Update this document if the invariant, trust boundary, or compatibility
   guarantee changes.
3. Add or update tests listed in the coverage matrix.
4. Update `docs/EVENT_SCHEMA.md` for event changes.
5. Update `MIGRATION.md` for breaking ABI, storage, event, or error changes.
6. Regenerate and validate TypeScript bindings for public ABI changes.
