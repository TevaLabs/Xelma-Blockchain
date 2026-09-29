# Emergency Incident Mode & Claims-Only Operational Matrix

This runbook documents the Xelma protocol's incident-mode protections, operational matrix across runtime states, automated emergency drill procedures, and the operator checklist for managing emergency mode transitions and protocol recovery.

---

## 1. Protocol Operational Matrix

The protocol supports three distinct operational runtime states:

| Operation Category | Specific Function | Normal (Mode 0) | ClaimsOnly (Mode 1) | FullyPaused (Mode 2) |
|---|---|---|---|---|
| **Deposits & Onboarding** | `mint_initial` | ✅ Allowed | ❌ Blocked (`ContractPaused`) | ❌ Blocked (`ContractPaused`) |
| **Trading & Predictions** | `place_bet` | ✅ Allowed | ❌ Blocked (`ContractPaused`) | ❌ Blocked (`ContractPaused`) |
| | `place_precision_prediction` | ✅ Allowed | ❌ Blocked (`ContractPaused`) | ❌ Blocked (`ContractPaused`) |
| | `commit_prediction` | ✅ Allowed | ❌ Blocked (`ContractPaused`) | ❌ Blocked (`ContractPaused`) |
| | `reveal_prediction` | ✅ Allowed | ❌ Blocked (`ContractPaused`) | ❌ Blocked (`ContractPaused`) |
| **Claims & Withdrawals** | `claim_winnings` | ✅ Allowed | ✅ Allowed (In-flight claims protected) | ❌ Blocked (`ContractPaused`) |
| | `get_pending_winnings` | ✅ Allowed | ✅ Allowed | ✅ Allowed |
| **Market Operations** | `create_round` | ✅ Allowed | ✅ Allowed | ❌ Blocked (`ContractPaused`) |
| | `cancel_round` | ✅ Allowed | ✅ Allowed | ❌ Blocked (`ContractPaused`) |
| | `resolve_round` | ✅ Allowed | ✅ Allowed (Settlement allowed) | ❌ Blocked (`ContractPaused`) |
| **Admin & Governance** | `withdraw_protocol_fee` | ✅ Allowed | ✅ Allowed | ❌ Blocked (`ContractPaused`) |
| | Config updates (`set_windows`, etc.) | ✅ Allowed | ✅ Allowed | ❌ Blocked (`ContractPaused`) |
| | Emergency mode transitions | ✅ Allowed | ✅ Allowed | ✅ Allowed (`unpause_contract`) |
| **Queries & Diagnostics** | Read-only state queries | ✅ Allowed | ✅ Allowed | ✅ Allowed |

---

## 2. Incident Lifecycle & Escalation Path

```
                    ┌─────────────────────────┐
                    │    Normal Mode (0)      │
                    │ Full functionality open  │
                    └────────────┬────────────┘
                                 │
                   Incident Detected (e.g. Price anomaly / front-end issue)
                                 │
                                 ▼
                    ┌─────────────────────────┐
                    │   ClaimsOnly Mode (1)   │
                    │ New bets/mints blocked  │
                    │ Pending claims allowed  │
                    └────────────┬────────────┘
                                 │
                 Major Incident / System Vulnerability
                                 │
                                 ▼
                    ┌─────────────────────────┐
                    │   FullyPaused Mode (2)  │
                    │ All operations locked   │
                    └────────────┬────────────┘
                                 │
                     Remediation & Sign-off
                                 │
                                 ▼
                    ┌─────────────────────────┐
                    │    Normal Mode (0)      │
                    │ Unpaused & Reset        │
                    └─────────────────────────┘
```

---

## 3. Automated Emergency Drill Suite

The protocol's incident behavior is validated deterministically in `contracts/src/tests/drill.rs`.

### Drill Test Coverage

1. **`test_claims_only_matrix_verification`**:
   Verifies that when runtime mode is set to `1` (`ClaimsOnly`), deposit and betting functions return `ContractError::ContractPaused`, while pending winnings claims, market cancellation, round settlement, protocol fee withdrawals, and administrative config updates remain functional.

2. **`test_fully_paused_matrix_verification`**:
   Verifies that when the contract is paused via `pause_contract()` (mode `2`), all mutating operations including claims, settlements, and admin settings are strictly rejected.

3. **`test_emergency_incident_simulation_lifecycle`**:
   Simulates a full real-world emergency workflow:
   - Initial normal operation with active bets.
   - Sudden incident detection triggering transition to `ClaimsOnly`.
   - Blocked new deposits and trades verified.
   - Successful in-flight round resolution and payout claiming for existing participants.
   - Escalation to `FullyPaused` mode locking all contract interactions.
   - Successful recovery via `unpause_contract()`, restoring minting, market creation, and trading.

4. **`test_chaos_recovery_migrate_active_round_pause_resume`** (Issue #417):
   Chaos recovery drill walking `create round → pause → migration dry-run → claims-only → resolve → claim`:
   - Migration dry-run with an active round is refused (`MigrationActiveRound`) with no storage or fund movement.
   - `pause_contract()` locks trading/claiming, and a migration dry-run while paused is refused (`ContractPaused`).
   - Transition to `ClaimsOnly` blocks new bets while still allowing the in-flight round to be resolved and claimed.
   - **No-funds-stuck invariant**: the sum of all pending winnings equals the total staked, and after claiming, every balance reconciles exactly to the initial mints.

5. **`test_chaos_recovery_migrate_active_round_pause_cancel`** (Issue #417):
   Same chaos sequence ending in the **cancel** path instead of resolution:
   - After `pause → claims-only`, `cancel_round` refunds every stake in full during `ClaimsOnly` mode.
   - **No-funds-stuck invariant**: refunds equal the total staked and balances reconcile exactly after claims.
   - Recovery to `Normal` restores round creation and trading.

6. **`drill_chaos_migration::*`** (Issue #565, `contracts/src/tests/drill_chaos_migration.rs`):
   Table-driven chaos drill that starts from a contract with a **pending
   migration** (schema v2) and a live round, then runs every combination of:

   | Entry sequence (while the round is live)                              | Exit                                                        |
   |-----------------------------------------------------------------------|-------------------------------------------------------------|
   | `pause → dry-run → claims-only` (canonical)                           | resolve in `ClaimsOnly`                                     |
   | `dry-run → pause → dry-run → claims-only`                             | resume to `Normal`, then resolve                            |
   | `claims-only → dry-run → pause → dry-run → claims-only`               | cancel in `ClaimsOnly` (generic reason)                     |
   | `pause → pause → claims-only → pause → dry-run`                       | cancel in `ClaimsOnly` for oracle outage (insurance payout) |
   |                                                                       | resume to `Normal`, then cancel                             |

   After **every** step it asserts value conservation across balances, pending
   winnings, the active round pot, the fee treasury and the insurance fund.
   Refused dry-runs must leave every amount, the schema and the mode
   untouched. After the exit, the drill proves **no funds are stuck**: every
   holder claims to zero pending, the deferred dry-run passes without mutating
   state, the real `migrate_schema_v2_to_v3` completes, and a fresh round can be
   created and traded.

   `drill_cancel_with_insurance_eligible_reason_does_not_trap` is a regression
   guard: insurance storage keys were built in a foreign `Env`, so
   `cancel_round(1..=3)` trapped and an active round could not be cancelled.

### Executing the Emergency Drill

Run the drill suite using cargo test (the filter also matches
`tests::drill_chaos_migration`, so one invocation covers both drill modules):

```bash
cargo test --package xelma-contract --lib tests::drill -- --nocapture
```

For the **release gate** — same suite plus assertions that every required
drill actually ran — use the gate script described in §4:

```bash
./scripts/emergency_drill_gate.sh
```

In CI this is the `Emergency Drill Gate` job (`emergency-drill` in
[`.github/workflows/ci.yml`](../.github/workflows/ci.yml)), which `ci-success`
requires.

To run all tests in the workspace:

```bash
cargo test --all-targets
```

---

## 4. Release Checklist Gate

The claims-only drill is an **explicit release gate**, not just another test
target. Nothing ships until it is green, because a release that regresses the
pause → claims → resume sequence is a release that can strand user funds.

### The gate

| Gate | Where | What it does |
|---|---|---|
| **Emergency Drill Gate** (CI, required) | `emergency-drill` job in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) | Runs `scripts/emergency_drill_gate.sh` on every push and PR, and is a required dependency of the `ci-success` job. |
| **Manual gate** (pre-release) | `./scripts/emergency_drill_gate.sh` | Same script, same assertions; run it locally before tagging. |
| **Manual re-run** (release branch/tag) | Actions → CI → **Run workflow** (`workflow_dispatch`) | Re-runs the full gate, drill included, against the exact ref being released. |

`scripts/emergency_drill_gate.sh` is the single source of truth for both paths.
Beyond running the suite it asserts that every drill named in `REQUIRED_DRILLS`
actually executed. A bare `cargo test --lib tests::drill` exits `0` even when
the filter matches nothing, so a renamed or deleted drill would otherwise turn
this gate green while quietly removing the coverage.

### Drill coverage: pause → claims → resume

| Phase | What must hold | Drills that prove it |
|---|---|---|
| **Pause** | `pause_contract()` reaches `FullyPaused`; trading *and* claiming are rejected; refusals move no funds. | `test_fully_paused_matrix_verification`, `test_chaos_recovery_migrate_active_round_pause_resume`, `test_chaos_recovery_migrate_active_round_pause_cancel`, `drill_chaos_migration_full_matrix` |
| **Claims** | In `ClaimsOnly` the in-flight round still resolves/cancels and every holder claims; `Σ pending == total staked`; no funds are stuck. | `test_claims_only_matrix_verification`, `test_emergency_incident_simulation_lifecycle`, `test_chaos_recovery_migrate_active_round_pause_resume`, `drill_chaos_migration_canonical_resume`, `drill_chaos_migration_canonical_cancel` |
| **Resume** | Returning to `Normal` re-enables minting, round creation and betting; balances reconcile exactly to the initial mints. | `test_emergency_incident_simulation_lifecycle`, `test_chaos_recovery_migrate_active_round_pause_resume`, `test_chaos_recovery_migrate_active_round_pause_cancel`, `drill_chaos_migration_full_matrix` |

Plus `drill_cancel_with_insurance_eligible_reason_does_not_trap`, which guards
the cancel path from a trap that used to make an active round uncancellable.

### Release checklist

Run this list before cutting a release. Items marked **gate** block the
release; a bypass needs a written sign-off from the Release Owner and the
Incident Lead, recorded in the release notes.

- [ ] **gate** `Emergency Drill Gate` job is green on the release commit (required check; `ci-success` depends on it).
- [ ] **gate** Drill log artifact (`emergency-drill-log`) is attached to the release run for later audit.
- [ ] **gate** `./scripts/emergency_drill_gate.sh` passes locally on the release ref and reports every drill as `ok`.
- [ ] Confirm the drill coverage above still matches the tests in `contracts/src/tests/drill.rs` and `contracts/src/tests/drill_chaos_migration.rs`.
- [ ] Confirm no drill was `#[ignore]`d, renamed or deleted to make the gate pass. If one must change, update `REQUIRED_DRILLS` in `scripts/emergency_drill_gate.sh` and the table above in the same PR.
- [ ] Confirm the release keeps `pause → claims → resume` working end to end: no release may narrow the `ClaimsOnly` matrix in §1 without an accompanying drill update.
- [ ] Walk the live-operator checklist in §5 once against staging (or the last testnet deployment) so the on-chain `set_runtime_mode` / `pause_contract` / `unpause_contract` path matches the drilled behaviour.
- [ ] Record the drill result and the sign-off in the release notes (see [`RELEASE.md`](RELEASE.md)) and in the deployment checklist ([`DEPLOYMENT_RUNBOOK.md`](DEPLOYMENT_RUNBOOK.md) §1).

If the gate is red, do **not** cut the release. Fix the regression, or roll the
incident-mode behavior back per [`DEPLOYMENT_RUNBOOK.md`](DEPLOYMENT_RUNBOOK.md)
§4, then re-run the gate.

---

## 5. Operator Emergency Checklist

### Pre-Incident Readiness
- [ ] Confirm emergency operator keys are initialized with multi-sig or timelock permissions.
- [ ] Ensure the `Emergency Drill Gate` CI job passes (it runs all `tests::drill` targets).
- [ ] Ensure the §4 release checklist is complete for the current release.

### Phase 1: Incident Detection & ClaimsOnly Containment
- [ ] Receive alert (oracle stale price, front-end anomaly, or exploit report).
- [ ] **Action**: Transition contract to `ClaimsOnly` mode:
  ```bash
  soroban contract invoke --id <CONTRACT_ID> --fn set_runtime_mode -- --mode 1
  ```
- [ ] Verify `get_protocol_status()` returns `ClaimsOnly`.
- [ ] Confirm new user minting and betting operations are rejected.
- [ ] Monitor existing users successfully claiming pending winnings for resolved rounds.

### Phase 2: Major Incident Escalation (If Required)
- [ ] If vulnerability threatens liquidity pools or contract balance, escalate to `FullyPaused` mode:
  ```bash
  soroban contract invoke --id <CONTRACT_ID> --fn pause_contract
  ```
- [ ] Confirm `is_paused()` returns `true` and `get_protocol_status()` returns `Paused`.
- [ ] Verify all claim and settlement operations are fully locked.

### Phase 3: Investigation & Patch Deployment
- [ ] Perform root-cause analysis.
- [ ] If contract logic patch is required, deploy updated WASM following release runbook.
- [ ] Verify state consistency across active and archived rounds.

### Phase 4: Recovery & Unpausing
- [ ] Obtain sign-off from Release Owner and Incident Lead.
- [ ] Unpause contract to restore `Normal` mode (mode 0):
  ```bash
  soroban contract invoke --id <CONTRACT_ID> --fn unpause_contract
  ```
- [ ] Verify `get_protocol_status()` returns `Active` (mode 0).
- [ ] Execute smoke test round (`create_round`, `place_bet`, `resolve_round`).
- [ ] Notify community and publish incident post-mortem.
