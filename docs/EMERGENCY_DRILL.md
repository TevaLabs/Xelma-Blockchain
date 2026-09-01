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

### Executing the Emergency Drill

Run the drill suite using cargo test:

```bash
cargo test --lib tests::drill
```

To run all tests in the workspace:

```bash
cargo test --all-targets
```

---

## 4. Operator Emergency Checklist

### Pre-Incident Readiness
- [ ] Confirm emergency operator keys are initialized with multi-sig or timelock permissions.
- [ ] Ensure CI pipeline passes all `tests::drill` targets.

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
