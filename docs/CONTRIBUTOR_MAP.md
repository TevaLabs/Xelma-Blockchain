# Contributor Domain Map v2

This document maps every module in this repository to its tests, starter tasks,
and focused commands. It is designed to help new and experienced contributors
navigate the modular layout and ship reviewable PRs that measurably improve
protocol safety, correctness, or operator/demo readiness on Stellar.

- **CEI Audit & Security Reference:** [docs/CEI_AUDIT.md](CEI_AUDIT.md)
- **Oracle Committee Architecture:** [docs/ORACLE_COMMITTEE.md](ORACLE_COMMITTEE.md)

---

## Quick Reference — Repository Layout

```
Xelma-Blockchain/
├── contracts/                     # Smart contract (Rust + Soroban)
│   ├── src/
│   │   ├── lib.rs                 # Crate root — module declarations & public exports
│   │   ├── contract.rs            # Core contract implementation
│   │   ├── common.rs              # Shared utilities, constants & helper functions
│   │   ├── errors.rs              # 50 contract error variants
│   │   ├── types.rs               # DataKey, Round, OraclePayload, etc.

---

### `contracts/src/common.rs`

**Purpose:** Single source of truth for contract constants (economic control limits, window bounds, oracle thresholds, protocol fee caps) and shared math/helper functions.

**Key exports:** `MIN_CAP_VALUE`, `MAX_MIN_PARTICIPANTS`, `DEFAULT_MAX_PRECISION_PARTICIPANTS`, `MAX_PAGE_SIZE`, `DEFAULT_ORACLE_STALE_THRESHOLD`, `MAX_PROTOCOL_FEE_BPS`, `BPS_DENOMINATOR`, `CURRENT_SCHEMA_VERSION`, `_accumulate_pending`, `sort_addresses`.
│   │   └── tests/
│   │       ├── mod.rs             # Test module declarations
│   │       ├── betting.rs         # Up/Down betting flow
│   │       ├── chaos_recovery.rs  # Pause/cancel/resume scenarios
│   │       ├── config_helpers.rs  # Test utilities for timelocked config
│   │       ├── config_timelock.rs # Timelocked governance tests
│   │       ├── cost_benchmarks.rs # CPU + memory benchmark tests
│   │       ├── edge_cases.rs      # Boundary/value edge cases
│   │       ├── event_coverage.rs  # On-chain event emission tests
│   │       ├── guard_tests.rs     # Access control / role enforcement
│   │       ├── initialization.rs  # Contract init and one-time setup
│   │       ├── invariant_harness.rs # Differential invariant test harness
│   │       ├── lifecycle.rs       # Round create → resolve lifecycle
│   │       ├── migration_versioning.rs # Schema migration tests
│   │       ├── mode_tests.rs      # UpDown vs Precision mode branching
│   │       ├── overflow_tests.rs  # Checked arithmetic overflow
│   │       ├── pause.rs           # Emergency pause/unpause
│   │       ├── property_invariants.rs # Protocol invariant assertions
│   │       ├── reference_model.rs # Simplified reference impl for invariants
│   │       ├── resolution.rs      # Payout execution and settlement
│   │       ├── security.rs        # Oracle auth, nonce, replay defense
│   │       ├── storage_benchmarks.rs # Storage footprint benchmarks
│   │       ├── ttl_tests.rs       # TTL extension & rent policy
│   │       └── windows.rs         # Bet/run window configuration
│   ├── Cargo.toml                 # Crate manifest
│   └── BENCHMARKS.md              # Benchmark baselines & guardrails
├── bindings/                      # TypeScript SDK bindings
│   ├── src/
│   │   ├── index.ts               # Generated contract client & types
│   │   └── parity.js              # ABI parity checker
│   ├── package.json
│   └── README.md
├── docs/
│   ├── CONTRIBUTOR_MAP.md         # ← This file
│   ├── CONTRIBUTOR_TASK_MATRIX.md # PR evidence requirements by task type
│   ├── EVENT_SCHEMA.md            # Canonical event schema for indexers
│   ├── event_schema_guide.md      # How to work with events
│   ├── archive_queries_guide.md   # Consumer guide for archive participation queries
│   └── storage_lifecycle.md       # TTL/rent policy reference
├── .github/
│   ├── workflows/ci.yml           # CI pipeline definition
│   ├── PULL_REQUEST_TEMPLATE.md   # PR checklist
│   └── ISSUE_TEMPLATE/            # Issue templates
├── PROTOCOL_SPEC.md               # Invariants I1–I13 & threat model
├── SECURITY_REVIEW.md             # Security audit findings
├── ROUND_LIFECYCLE.md             # Round state machine
├── STORAGE_DESIGN.md              # Storage architecture
├── MIGRATION.md                   # Schema version migration history
├── GOVERNANCE.md                  # Maintainer governance process
├── CONTRIBUTING.md                # General contributing guide
├── COMPATIBILITY_POLICY.md        # ABI/storage/event versioning rules
├── SUPPORT.md                     # Support channels
├── Cargo.toml                     # Workspace root manifest
└── README.md                      # Project overview & quick start
```

---

## Domain 1 — Core Contract Source

### `contracts/src/lib.rs`

**Purpose:** Crate root. Declares modules and re-exports the public API
(`VirtualTokenContract`, `ContractError`, and all type definitions).

**Related tests:** All tests transitively. If you add a new source module,
register it here.

**Key exports:**

| Symbol | Source module |
|---|---|
| `VirtualTokenContract` | `contract.rs` |
| `ContractError` | `errors.rs` |
| `DataKey`, `Round`, `OraclePayload`, `BetSide`, `RoundMode`, `UserPosition`, `UserStats`, `PrecisionPrediction`, `PrecisionCommitment`, `ArchivedRoundSummary`, `ConfigChangeKind`, `ConfigChangePayload`, `PendingConfigChange` | `types.rs` |

---

### `contracts/src/contract.rs`

**Purpose:** The core contract implementation. Contains all entrypoints, internal
helpers, and the business logic for dual-mode prediction markets.

**Lines:** ~2100+ (growing — see breakdown below)

**Key sections:**

| Section | Functions |
|---|---|
| Economic constants & defaults | `MIN_CAP_VALUE`, `DEFAULT_MAX_PRECISION_PARTICIPANTS`, `CONFIG_TIMELOCK_LEDGERS`, etc. |
| Initialization & schema | `initialize`, `get_schema_version`, `migrate_schema_v1_to_v2` |
| Pause/unpause | `is_paused`, `pause_contract`, `unpause_contract` |
| Round management | `create_round`, `get_active_round`, `get_last_round_id` |
| Archived rounds | `get_archived_round`, `get_recent_archived_rounds` |
| Access control queries | `get_admin`, `get_oracle` |
| Oracle deviation guardrails | `set_oracle_max_deviation_bps`, `get_oracle_max_deviation_bps`, `arm_oracle_deviation_override` |
| Oracle heartbeat | `update_oracle_heartbeat`, `get_oracle_heartbeat`, `is_oracle_live`, `set_oracle_stale_threshold`, `get_oracle_stale_threshold` |
| Window management | `set_windows` (schedules via timelock) |
| Economic controls | `set_max_stake`, `get_max_stake`, `set_max_user_exposure`, `get_max_user_exposure`, `set_max_pending_winnings` |
| Timelocked config governance | `schedule_windows`, `schedule_max_stake`, `schedule_max_user_exposure`, `schedule_max_pending_winnings`, `schedule_oracle_stale_threshold`, `schedule_oracle_deviation_bps`, `get_pending_config_change`, `apply_scheduled_changes`, `cancel_config_change` |
| Minimum participants | `set_min_participants`, `get_min_participants` |
| Precision participant cap | `set_max_precision_participants`, `get_max_precision_participants` |
| User stats & winnings | `get_user_stats`, `get_pending_winnings` |
| Betting (Up/Down) | `place_bet` |
| Precision prediction | `place_precision_prediction`, `predict_price` |
| Commit-reveal flow | `commit_prediction`, `reveal_prediction` |
| Position queries | `get_user_position`, `get_user_precision_prediction`, `get_precision_predictions` |
| Paginated queries | `get_updown_positions_paginated`, `get_precision_predictions_paginated`, `get_round_participants_paginated` |
| Resolution & settlement | `resolve_round` (UpDown + Precision payout logic), `cancel_round`, `_accumulate_pending` |
| Balance & minting | `balance`, `mint_initial`, `_set_balance`, `_add_balance` |
| Claim winnings | `claim_winnings` |
| Private helpers | `assert_no_active_round`, `_ensure_not_paused`, `_require_supported_schema`, `_schema_version`, `_extend_persistent_ttl`, `_validate_windows`, `_validate_max_stake`, `_validate_oracle_stale_threshold`, `_validate_oracle_max_deviation_bps`, `_schedule_config_change`, `_apply_config_payload`, `_validate_oracle_payload_context`, `_update_stats_win`, `_update_stats_loss`, `_archive_round`, `payout_add`, `payout_mul` |

**Related tests:**

| Test module | Covers |
|---|---|
| `initialization.rs` | `initialize`, schema version setup |
| `lifecycle.rs` | Round create → resolve lifecycle |
| `betting.rs` | `place_bet`, duplicate checks, balance deduction |
| `mode_tests.rs` | Mode branching in `create_round`, `place_bet`, `place_precision_prediction` |
| `resolution.rs` | `resolve_round`, payout math, refunds |
| `security.rs` | Oracle payload validation, nonce replay, deviation |
| `pause.rs` | Pause/unpause enforcement across mutating entrypoints |
| `windows.rs` | `set_windows` / `schedule_windows` |
| `config_timelock.rs` | Timelocked config scheduling, activation, cancellation |
| `event_coverage.rs` | All event emissions |
| `guard_tests.rs` | Role enforcement (admin/oracle auth) |
| `chaos_recovery.rs` | Pause-then-resume, cancel-then-create |
| `overflow_tests.rs` | Checked arithmetic in pool accumulation |
| `ttl_tests.rs` | `_extend_persistent_ttl` coverage |
| `migration_versioning.rs` | `migrate_schema_v1_to_v2` |
| `storage_benchmarks.rs` | Storage key lifecycle |
| `cost_benchmarks.rs` | CPU/memory for hot paths |

---

### `contracts/src/errors.rs`

**Purpose:** Defines the `ContractError` enum with 50 variants. Each variant has
a unique numeric code used by Stellar transaction results.

**Related tests:**

| Test module | Covers |
|---|---|
| `guard_tests.rs` | Auth errors (4, 5) |
| `betting.rs` | Bet validation errors (6–10) |
| `lifecycle.rs` | Round state errors (7, 8, 16, 20) |
| `mode_tests.rs` | Mode errors (14, 15) |
| `resolution.rs` | Resolution errors (8, 16, 24) |
| `security.rs` | Oracle errors (18, 19, 33, 49, 50) |
| `pause.rs` | ContractPaused (22) |
| `windows.rs` | WindowOutOfRange (23) |
| `overflow_tests.rs` | Overflow (11), PayoutOverflow (25) |
| `edge_cases.rs` | Various boundary error codes |
| `config_timelock.rs` | CommitmentNotFound (45), RoundNotEnded (16) |
| `migration_versioning.rs` | UnsupportedSchemaVersion (42), InvalidMigrationPath (43), MigrationActiveRound (44) |
| `event_coverage.rs` | Error-path event gaps |

---

### `contracts/src/types.rs`

**Purpose:** All `#[contracttype]` structs, enums, and the `DataKey` storage
key enum. Changing anything here is a **MAJOR version bump** (see
[COMPATIBILITY_POLICY.md](../COMPATIBILITY_POLICY.md)).

**Key types:**

| Type | Purpose |
|---|---|
| `DataKey` | All persistent storage keys — see [storage_lifecycle.md](storage_lifecycle.md) |
| `Round` | Active round state |
| `RoundMode` | UpDown (0) vs Precision (1) |
| `BetSide` | Up / Down |
| `UserPosition` | Single bet record |
| `UserStats` | Wins, losses, streaks |
| `PrecisionPrediction` | Precision mode prediction entry |
| `PrecisionCommitment` | Commit-reveal hash commitment |
| `OraclePayload` | Oracle resolution data with nonce + domain context |
| `OracleHeartbeatRecord` | Oracle liveness record |
| `ArchivedRoundSummary` | Compact post-settlement round data |
| `RoundArchiveStatus` | Resolved (0) / Cancelled (1) / FallbackRefund (2) |
| `ConfigChangeKind` | Which risk setting is pending timelocked activation |
| `ConfigChangePayload` | Payload for the pending config change |
| `PendingConfigChange` | Timelocked config with activation ledger |

**Related tests:** Almost all test modules interact with `DataKey` and type
definitions. Critical ones:

| Test module | Why |
|---|---|
| `types.rs` has no standalone test file | Covered by integration tests in all modules |
| `storage_benchmarks.rs` | Storage key footprint validation |
| `ttl_tests.rs` | Long-lived vs short-lived key lifecycle |
| `migration_versioning.rs` | DataKey additions/removals in migrations |

---

## Domain 2 — Test Suite (by category)

### 🟢 Good-First Tests

These test files are small, focused, and have clear "assert X happens" patterns.
They're excellent starting points for new contributors.

#### `contracts/src/tests/initialization.rs`

**Covers:** `initialize()`, duplicate initialization guard, schema version
being set, default window values.

**Focused command:**
```bash
cargo test --package xelma-contract initialization -- --nocapture
```

**Good-first tasks:**
- [ ] Add a test for initialization with identical admin/oracle addresses
  (expect `AdminIsOracle`)
- [ ] Verify that all default persistent keys (Admin, Oracle, Paused,
  SchemaVersion, BetWindowLedgers, RunWindowLedgers) are present after
  initialization
- [ ] Test that `get_schema_version` returns `CURRENT_SCHEMA_VERSION` after init

---

#### `contracts/src/tests/pause.rs`

**Covers:** `pause_contract`, `unpause_contract`, `is_paused`, and the
`_ensure_not_paused` guard on all mutating entrypoints.

**Focused command:**
```bash
cargo test --package xelma-contract pause -- --nocapture
```

**Good-first tasks:**
- [ ] Verify `pause_contract` requires admin auth
- [ ] Verify each mutating entrypoint is blocked when paused
- [ ] Confirm `is_paused` returns true/false correctly after pause/unpause

---

#### `contracts/src/tests/windows.rs`

**Covers:** `set_windows` / `schedule_windows`, range validation, and window
application.

**Focused command:**
```bash
cargo test --package xelma-contract windows -- --nocapture
```

**Good-first tasks:**
- [ ] Test setting windows at the exact boundary values (`1`, `MAX_BET_WINDOW_LEDGERS`, `MAX_RUN_WINDOW_LEDGERS`)
- [ ] Test that exceeding the max values returns `WindowOutOfRange`
- [ ] Verify windows are applied correctly when creating a new round

---

#### `contracts/src/tests/guard_tests.rs`

**Covers:** Role-based access control — only admin can call admin functions,
only oracle can resolve, etc.

**Focused command:**
```bash
cargo test --package xelma-contract guard_tests -- --nocapture
```

**Good-first tasks:**
- [ ] Confirm `place_bet` requires user auth
- [ ] Confirm `resolve_round` requires oracle auth
- [ ] Check `cancel_config_change` requires admin auth

---

### 🟡 Intermediate Tests

These test files involve orchestration of multiple operations and check
intermediate state transitions. Good for contributors comfortable with the
Soroban test environment.

#### `contracts/src/tests/betting.rs`

**Covers:** The Up/Down betting flow — `place_bet`, balance deduction,
duplicate bet rejection, pool accumulation.

**Focused command:**
```bash
cargo test --package xelma-contract betting -- --nocapture
```

**Starter tasks:**
- [ ] Test that betting after `bet_end_ledger` returns `RoundEnded`
- [ ] Verify pool amounts match total individual bets
- [ ] Test that a user cannot bet more than their balance

---

#### `contracts/src/tests/mode_tests.rs`

**Covers:** Mode-specific behavior — UpDown bets rejected in Precision rounds,
Precision predictions rejected in UpDown rounds.

**Focused command:**
```bash
cargo test --package xelma-contract mode_tests -- --nocapture
```

**Starter tasks:**
- [ ] Test `place_bet` in a Precision round returns `WrongModeForPrediction`
- [ ] Test `place_precision_prediction` in an UpDown round returns `WrongModeForPrediction`
- [ ] Verify `create_round` with mode > 1 returns `InvalidMode`

---

#### `contracts/src/tests/edge_cases.rs`

**Covers:** Boundary values — zero amounts, max amounts, empty states, corner
cases.

**Focused command:**
```bash
cargo test --package xelma-contract edge_cases -- --nocapture
```

**Starter tasks:**
- [ ] Test `place_bet` with amount = 0 (expect `InvalidBetAmount`)
- [ ] Test `place_bet` with amount = `i128::MAX` in various balance scenarios
- [ ] Test creation of a round with start_price at MIN_START_PRICE and MAX_START_PRICE

---

#### `contracts/src/tests/event_coverage.rs`

**Covers:** Every event emission — verifies topic pairs and payload field
positions.

**Focused command:**
```bash
cargo test --package xelma-contract event_coverage -- --nocapture
```

**Starter tasks:**
- [ ] Add test coverage for any event topic pair not yet asserted
- [ ] Verify `("round", "cancelled")` event payload fields
- [ ] Verify `("config", "applied")` event after timelocked activation

---

### 🔴 Advanced Tests

These test files involve complex multi-step scenarios, protocol invariants,
economic safety, or chaos recovery. They require deeper understanding of the
contract and the Stellar/Soroban execution model.

#### `contracts/src/tests/resolution.rs`

**Covers:** Full settlement logic — UpDown proportional payouts, Precision
closest/tie payouts, unchanged-price refunds, remainder (dust) policy.

**Focused command:**
```bash
cargo test --package xelma-contract resolution -- --nocapture
```

**Advanced tasks:**
- [ ] Add a test for a 5-way tie in Precision mode with a non-divisible pot;
  verify the remainder goes to the first predictor
- [ ] Test that unchanged-price resolution refunds all participants equally
- [ ] Verify that cancelling a round refunds all participants to their
  original balances

---

#### `contracts/src/tests/security.rs`

**Covers:** Oracle security — nonce replay prevention, stale/future timestamp
rejection, deviation guardrails, domain-context validation (network_id,
contract_addr).

**Focused command:**
```bash
cargo test --package xelma-contract security -- --nocapture
```

**Advanced tasks:**
- [ ] Test that reusing the same nonce returns `OracleNonceReused` (33)
- [ ] Test that an oracle payload with a future timestamp returns
  `FutureOracleData` (24)
- [ ] Test that an oracle payload targeting a different network is rejected
  with `OracleNetworkMismatch` (49)
- [ ] Test the one-shot deviation override — arm it, resolve (allow),
  resolve again without override (expect `OracleDeviationExceeded`)

---

#### `contracts/src/tests/property_invariants.rs`

**Covers:** Protocol invariants — token conservation, payout ≤ pool, no
double-spend, balance monotonicity.

**Focused command:**
```bash
cargo test --package xelma-contract property_invariants -- --nocapture
```

**Advanced tasks:**
- [ ] Add a proptest that randomly sequences bets, resolves, and claims,
  asserting that total_balance + total_pending = total_minted
- [ ] Verify invariant I8 (settlement conservation) across randomized
  multi-round scenarios

---

#### `contracts/src/tests/invariant_harness.rs`

**Covers:** Differential invariant testing against the reference model
implemented in `reference_model.rs`. The harness drives both the real
contract and the simplified model through identical sequences and asserts
they agree on observable outcomes.

**Focused command:**
```bash
cargo test --package xelma-contract invariant_harness -- --nocapture
```

**Advanced tasks:**
- [ ] Add a new action variant that exercises the commit-reveal flow
- [ ] Verify the harness catches intentional mismatches between the
  contract and reference model

---

#### `contracts/src/tests/chaos_recovery.rs`

**Covers:** Operational resilience — pause mid-round, cancel and recreate,
rapid successions of create/resolve cycles.

**Focused command:**
```bash
cargo test --package xelma-contract chaos_recovery -- --nocapture
```

**Advanced tasks:**
- [ ] Test pausing during an active round, then unpausing and resolving
- [ ] Test cancelling a round while paused
- [ ] Test rapid create → cancel → create → resolve cycles

---

#### `contracts/src/tests/overflow_tests.rs`

**Covers:** Checked arithmetic for edge values of pool accumulation, payout
math, and balance operations.

**Focused command:**
```bash
cargo test --package xelma-contract overflow_tests -- --nocapture
```

**Advanced tasks:**
- [ ] Verify that `pool_up + bet` overflow returns `Overflow` (11) not a panic
- [ ] Verify that payout arithmetic overflow returns `PayoutOverflow` (25)
- [ ] Test that `i128::MAX` stakes with extreme participant counts do not
  produce silent overflows

---

#### `contracts/src/tests/cost_benchmarks.rs`

**Covers:** CPU instruction and memory usage for hot paths — `create_round`,
`place_bet`, `resolve_round`, `claim_winnings`. Outputs are compared against
Soroban budget limits.

**Focused command:**
```bash
cargo test --package xelma-contract cost_benchmarks -- --nocapture
```

**Advanced tasks:**
- [ ] Run benchmarks with N=100 participants and ensure CPU stays within
  Soroban budget (`100_000_000` CPU instructions, `100 MiB` memory)
- [ ] Profile Precision mode resolution with N=500 precision predictions and
  record worst-case CPU/memory

---

#### `contracts/src/tests/migration_versioning.rs`

**Covers:** Schema version migration — v1 → v2 migration path, guardrails
(no active round during migration), unknown version rejection.

**Focused command:**
```bash
cargo test --package xelma-contract migration_versioning -- --nocapture
```

**Advanced tasks:**
- [ ] Test that migration is rejected when a round is active
  (`MigrationActiveRound` 44)
- [ ] Test that an unsupported schema version returns
  `UnsupportedSchemaVersion` (42)
- [ ] Test that `migrate_schema_v1_to_v2` emits `("schema", "migrated")`

---

#### `contracts/src/tests/config_timelock.rs`

**Covers:** Timelocked governance — scheduling, activation delay enforcement,
cancellation, and application of all six `ConfigChangeKind` variants.

**Focused command:**
```bash
cargo test --package xelma-contract config_timelock -- --nocapture
```

**Advanced tasks:**
- [ ] Test that config cannot be applied before its activation ledger
- [ ] Test cancellation before activation is allowed; after activation is
  rejected with `RoundNotCancellable`
- [ ] Verify the `("config", "applied")` event is emitted on application

---

#### `contracts/src/tests/ttl_tests.rs`

**Covers:** Storage TTL extension — verifies that long-lived keys get extended
on access and short-lived keys are cleaned up at resolution/cancellation.

**Focused command:**
```bash
cargo test --package xelma-contract ttl_tests -- --nocapture
```

**Advanced tasks:**
- [ ] Verify that `Balance`, `UserStats`, and `PendingWinnings` keys are
  extended when accessed
- [ ] Verify that `Position` and `PrecisionPosition` keys are removed after
  round resolution
- [ ] Test that admin/oracle config keys are extended when read

---

#### `contracts/src/tests/storage_benchmarks.rs`

**Covers:** Storage footprint — verifies storage key creation and cleanup
during round lifecycle.

**Focused command:**
```bash
cargo test --package xelma-contract storage_benchmarks -- --nocapture
```

**Advanced tasks:**
- [ ] Measure the number of persistent keys after a round with N participants
- [ ] Verify that all per-round keys are removed after resolution
- [ ] Profile storage cost with N=1,000 participants

---

## Domain 3 — TypeScript Bindings

### `bindings/src/index.ts`

**Purpose:** Generated TypeScript bindings providing a type-safe SDK for
interacting with the Xelma contract from JavaScript/TypeScript applications.

### `bindings/src/parity.js`

**Purpose:** Checks public method parity between the contract ABI and the
TypeScript bindings. Run via `npm run test:parity`.

**Focused command:**
```bash
cd bindings && npm run test:parity
```

**Starter tasks:**
- [ ] If a new public entrypoint is added to the contract, regenerate
  bindings and verify parity passes
- [ ] If a new `ContractError` variant is added, manually update the error
  map in `bindings/src/index.ts` (the parity script does not check enum
  parity — see SR-2026-04-001)

**Regeneration commands:**
```bash
cargo rustc --manifest-path=contracts/Cargo.toml --crate-type=cdylib --target=wasm32v1-none --release --locked
stellar contract bindings typescript \
  --wasm target/wasm32v1-none/release/xelma_contract.wasm \
  --output-dir ./bindings/src \
  --overwrite
cd bindings && npm install && npm run build
```

---

## Domain 4 — Documentation

| Document | Purpose | When to update |
|---|---|---|
| `README.md` | Project overview, quick start, function list | New features, deprecations |
| `CONTRIBUTING.md` | General contributor guide | Workflow/process changes |
| **`docs/CONTRIBUTOR_MAP.md`** | **This file** — module → test → task map | New modules, new test files |
| `docs/CONTRIBUTOR_TASK_MATRIX.md` | PR evidence requirements by task type | New task categories, new domain rules |
| `docs/EVENT_SCHEMA.md` | Canonical event topic/payload schema | New/changed events |
| `docs/event_schema_guide.md` | How to work with events | Event tooling changes |
| `docs/archive_queries_guide.md` | Consumer guide for archive participation queries | Query or indexer changes |
| `docs/storage_lifecycle.md` | TTL/rent policy for persistent keys | New DataKey variants |
| `PROTOCOL_SPEC.md` | Invariants I1–I13, threat model | Protocol changes, new invariants |
| `SECURITY_REVIEW.md` | Security audit findings | New findings, mitigations |
| `ROUND_LIFECYCLE.md` | Round state machine | Lifecycle rule changes |
| `STORAGE_DESIGN.md` | Storage architecture | Storage layout changes |
| `MIGRATION.md` | Schema version migration history | Schema version bumps |
| `GOVERNANCE.md` | Maintainer governance | Process changes |
| `COMPATIBILITY_POLICY.md` | ABI/storage/event versioning rules | Versioning rule changes |

---

## Focused Test Commands — Cheat Sheet

Run these from the repository root to test specific domains without running
the entire suite.

```bash
# === Good-first entry points ===
cargo test --package xelma-contract initialization -- --nocapture
cargo test --package xelma-contract pause -- --nocapture
cargo test --package xelma-contract windows -- --nocapture
cargo test --package xelma-contract guard_tests -- --nocapture

# === Intermediate ===
cargo test --package xelma-contract betting -- --nocapture
cargo test --package xelma-contract mode_tests -- --nocapture
cargo test --package xelma-contract edge_cases -- --nocapture
cargo test --package xelma-contract event_coverage -- --nocapture

# === Advanced ===
cargo test --package xelma-contract resolution -- --nocapture
cargo test --package xelma-contract security -- --nocapture
cargo test --package xelma-contract property_invariants -- --nocapture
cargo test --package xelma-contract invariant_harness -- --nocapture
cargo test --package xelma-contract chaos_recovery -- --nocapture
cargo test --package xelma-contract overflow_tests -- --nocapture
cargo test --package xelma-contract cost_benchmarks -- --nocapture
cargo test --package xelma-contract migration_versioning -- --nocapture
cargo test --package xelma-contract config_timelock -- --nocapture
cargo test --package xelma-contract ttl_tests -- --nocapture
cargo test --package xelma-contract storage_benchmarks -- --nocapture

# === Full suite ===
cargo test --workspace --locked

# === Clippy ===
cargo clippy --workspace --all-targets --locked -- -D warnings

# === Format check ===
cargo fmt --all -- --check

# === Format fix ===
cargo fmt --all

# === Security audit ===
cargo audit --deny warnings

# === Bindings parity ===
cd bindings && npm run test:parity
```

---

## Recommended First PRs

If you're new to this codebase, here are suggested first PRs in order of
increasing difficulty:

### Level 1 — Documentation (no Rust required)

1. **Add a missing test coverage note** — Find a test module that doesn't
   cover an error code listed in `errors.rs`. Add a test that triggers it.
2. **Update README function list** — The README lists 12 functions; the
   contract has ~40+ entrypoints. Add the missing ones to the README.

### Level 2 — Low-risk test additions

1. **Add a boundary test in `edge_cases.rs`** — Test a value at exactly the
   minimum or maximum of an allowed range.
2. **Add an event assertion in `event_coverage.rs`** — Verify that a
   previously unasserted event topic pair is emitted with the correct fields.

### Level 3 — Protocol improvement

1. **Add a invariant test in `property_invariants.rs`** — Write a proptest
   that randomly sequences operations and checks a conservation invariant.
2. **Add an oracle security test in `security.rs`** — Cover a new oracle
   payload rejection path.

### Level 4 — Feature work

1. **Add a new configurable parameter** — Requires updates to `types.rs`,
   `contract.rs`, tests, bindings, and docs.
2. **Add a new event** — Requires `contract.rs` changes, `event_coverage.rs`
   test, and `docs/EVENT_SCHEMA.md` update.

---

## Known Technical Debt & Rebuild Starters

The following curated issues highlight key architectural refinement tasks and good-first-issue starter entrypoints:

| Issue | Title / Focus Area | Domain | Description |
|---|---|---|---|
| [#411](https://github.com/TevaLabs/Xelma-Blockchain/issues/411) | Expand lifecycle fuzz beyond UpDown happy path | `contracts/src/tests/fuzz_lifecycle.rs` | Property fuzzing for Precision prediction, commit-reveal, cash-out, and access-control denials. |
| [#434](https://github.com/TevaLabs/Xelma-Blockchain/issues/434) | Refresh SECURITY_REVIEW.md metrics and open findings | `SECURITY_REVIEW.md` | Update LOC, module counts, test counts, and severity tables for security audits. |
| [#435](https://github.com/TevaLabs/Xelma-Blockchain/issues/435) | Contributor map modular layout & debt starters | `docs/CONTRIBUTOR_MAP.md` | Maintain modular domain mapping and explicit known debt pointers for new contributors. |
| [#439](https://github.com/TevaLabs/Xelma-Blockchain/issues/439) | Split contract.rs facade from duplicated constants | `contracts/src/contract.rs`, `contracts/src/common.rs` | Single-source all protocol limits and windows constants into `common.rs`. |

---

## Cross-References

| For this... | See... |
|---|---|
| PR evidence requirements | `docs/CONTRIBUTOR_TASK_MATRIX.md` |
| Protocol invariants (I1–I13) | `PROTOCOL_SPEC.md` |
| Event field order & units | `docs/EVENT_SCHEMA.md` |
| Storage key lifetimes | `docs/storage_lifecycle.md` |
| Schema migration history | `MIGRATION.md` |
| Versioning rules | `COMPATIBILITY_POLICY.md` |
| Security findings | `SECURITY_REVIEW.md` |
| Round state machine | `ROUND_LIFECYCLE.md` |
| Storage architecture | `STORAGE_DESIGN.md` |
| PR template | `.github/PULL_REQUEST_TEMPLATE.md` |

---

*Last updated: 2026-08-31 — schema v3*
