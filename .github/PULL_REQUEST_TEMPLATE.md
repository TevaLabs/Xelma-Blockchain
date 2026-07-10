## Summary

- What changed and why?

## Linked issues

- Closes #

## Validation

- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo fmt --all -- --check`
- [ ] `cd bindings && npm ci && npm run build`

## Governance checklist

- [ ] I reviewed `CONTRIBUTING.md` for workflow expectations
- [ ] I checked `CODEOWNERS` impact for touched paths
- [ ] I followed `SUPPORT.md` disclosure guidance for any security-sensitive change

## Smart-Contract Security Checklist

### 1. Authentication & Access Control
- [ ] Every state-mutating method verifies caller authorization using `require_auth()` or appropriate admin/oracle checks.
- [ ] Access control policies align with security specifications (e.g., admin-only vs oracle-only vs user-only).

### 2. Safe Arithmetic & Overflow Protection
- [ ] Checked/safe math operations (`checked_add`, `checked_sub`, `checked_mul`, etc.) are used for all state changes.
- [ ] Precision math operations use specialized safe helpers (e.g., `payout_add` / `payout_mul`) where applicable.

### 3. Lifecycle & State Transitions
- [ ] Mutating actions are correctly gated by runtime mode checks (e.g., disabled during emergency modes, allowed during claims-only).
- [ ] Invariants (like "exactly one active round") are preserved before/after execution.

### 4. Event Emission & Observability
- [ ] Canonical events are emitted for all key state transitions (e.g., round created, bet placed, resolution, cancellation).
- [ ] Forensic summary events are generated correctly with compact and stable metadata.

### 5. Tests & Verification
- [ ] Unit tests cover both successful executions and expected failure/rejection paths.
- [ ] Property/invariant tests or edge cases are added for new protocol changes.

---

### Critical Path Changes
- **Does this PR modify contract payout, resolution, or claim paths?**
  - [ ] No / Not Applicable
  - [ ] Yes (Provide an explicit note explaining the changes and the rationale below)
  
  *If yes, note details:*
  > [Provide details here]

- **Are there any new failure modes introduced by these changes?**
  - [ ] No / Not Applicable
  - [ ] Yes (Detail the new failure modes and how they are mitigated)
  
  *If yes, note details:*
  > [Provide details here]

## Snapshot policy

- [ ] If snapshot files under `contracts/test_snapshots/` changed, I reviewed the diff and confirmed every change is intentional
- [ ] If snapshot drift was reported in CI, I either regenerated snapshots or marked the drift as expected in the PR description
