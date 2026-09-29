# CEI Pattern Audit — XLM Prediction Market Contract

**Issue:** [#195](https://github.com/TevaLabs/Xelma-Blockchain/issues/195)  
**Audit date:** 2026-06-27  
**Reviewer:** Security maintainer (automated review + manual inspection)  
**Contract file:** `contracts/src/contract.rs`  
**Schema version at audit time:** 2  
**Soroban note:** Soroban's deterministic, single-tenant execution model eliminates classical EVM reentrancy via external calls, but CEI compliance remains critical to ensure consistent on-chain accounting across complex multi-step flows and future cross-contract interaction paths.

---

## What is CEI?

**Checks-Effects-Interactions (CEI)** is a state-ordering discipline:

1. **Checks** — Validate all preconditions (auth, paused guard, balances, timing).
2. **Effects** — Apply all state changes (storage writes, balance mutations, status flags).
3. **Interactions** — Emit events, call external contracts.

Violating CEI means that storage can be left in an intermediate/inconsistent state if an Interaction panics, or that an emitted event does not accurately reflect the final committed state.

---

## Audit Scope

All `pub fn` entrypoints in `contracts/src/contract.rs` that **mutate on-chain state**.

Read-only functions (`get_*`, `is_*`, `balance`) are excluded.

---

## Mutating Entrypoints — CEI Compliance Table

| # | Function | Auth | Checks | Effects | Interactions | CEI Status | Notes |
|---|---|---|---|---|---|---|---|
| 1 | `initialize` | admin | Schema not set, admin != oracle | Write Admin, Oracle, Paused, SchemaVersion, windows | None | PASS | One-time init guard is checked first |
| 2 | `migrate_schema_v1_to_v2` | admin | Schema version == 1, no active round | Write SchemaVersion | Emit `(schema, migrated)` | PASS | |
| 3 | `pause_contract` | admin | Schema OK | Write Paused=true | None | PASS | No interaction after effects |
| 4 | `unpause_contract` | admin | Schema OK | Write Paused=false | None | PASS | |
| 5 | `create_round` | admin | Schema OK, not paused, no active round, price bounds, mode valid | Write LastRoundId, ActiveRound | Emit `(round, created)` | PASS | All checks before all writes before event |
| 6 | `place_bet` | user | Schema OK, not paused, amount > 0, max stake, max exposure, mode == UpDown, bet window open, balance sufficient, no duplicate | Write balance, Position, RoundParticipants, ActiveRound (pool totals) | Emit `(bet, placed)` | PASS | |
| 7 | `place_precision_prediction` | user | Schema OK, not paused, amount > 0, max stake, price scale, max exposure, mode == Precision, bet window open, participant cap, balance sufficient, no duplicate | Write balance, PrecisionPosition, RoundParticipants | Emit `(predict, price)` | PASS | |
| 8 | `predict_price` | user | (delegates to `place_precision_prediction`) | (same as above) | (same as above) | PASS | Thin alias |
| 9 | `commit_prediction` | user | Not paused, non-zero commitment hash, amount > 0, max stake, max exposure, mode == Precision, bet window open, balance sufficient, no duplicate | Write balance, PrecisionCommitment, RoundParticipants | Emit `(commit, predict)` | PASS | All-zero hash → `InvalidCommitment` before writes |
| 10 | `reveal_prediction` | user | Not paused, salt entropy OK, mode == Precision, reveal window open, commitment exists, not already revealed, hash matches | Write commitment.revealed=true, PrecisionPosition | Emit `(reveal, predict)` | PASS | Weak salt → `InvalidSalt`; hash verified before any write |
| 11 | `resolve_round` | oracle | Schema OK, price != 0, not paused, active round exists, round_id matches, network_id matches, contract_addr matches, timestamp not future, not stale, deviation check, nonce not consumed, round has ended | Write ConsumedOracleNonce, OracleDeviationOverrideArmed (remove), pending winnings (N users), archived round, cleanup (N positions + participants + ActiveRound) | Emit `(round, summary)` or `(pool, onesided)` | PASS | Nonce is consumed (Effect) after all validation Checks but before winner accounting begins |
| 12 | `cancel_round` | admin | Schema OK, active round exists | Write pending winnings (refunds), archived round, RoundParticipants (remove), CancelledRound=true, ActiveRound (remove) | Emit `(round, summary)` | PASS | All state mutations complete before event |
| 13 | `claim_winnings` | user | Schema OK, not paused, pending > 0 | Remove PendingWinnings key, write new balance | Emit `(claim, winnings)` | PASS (fixed) | **CEI fix applied**: pending winnings slot now removed before balance is increased. See SR-2026-06-001. |
| 14 | `arm_oracle_deviation_override` | admin | Admin set, not paused | Write OracleDeviationOverrideArmed=true | None | PASS | No interaction after effect |
| 15 | `update_oracle_heartbeat` | oracle | Schema OK, status <= 2, oracle set | Write OracleHeartbeat record | Emit `(oracle, heartbeat)` | PASS | |
| 16 | `set_min_participants` | admin | Schema OK, not paused, value bounds | Write or remove MinParticipants | None | PASS | |
| 17 | `set_max_precision_participants` | admin | Not paused, value in 1..=10_000 | Write MaxPrecisionParticipants | None | PASS | |
| 18 | `schedule_windows` | admin | Schema OK, window bounds valid | Write PendingConfigChange(Windows) | None | PASS | |
| 19 | `schedule_max_stake` | admin | Schema OK, value bounds | Write PendingConfigChange(MaxStake) | None | PASS | |
| 20 | `schedule_max_user_exposure` | admin | Schema OK, value bounds | Write PendingConfigChange(MaxUserRoundExposure) | None | PASS | |
| 21 | `schedule_max_pending_winnings` | admin | Schema OK, value bounds | Write PendingConfigChange(MaxPendingWinnings) | None | PASS | |
| 22 | `schedule_oracle_stale_threshold` | admin | Schema OK, seconds in valid range | Write PendingConfigChange(OracleStaleThreshold) | None | PASS | |
| 23 | `schedule_oracle_deviation_bps` | admin | Schema OK, bps within bounds | Write PendingConfigChange(OracleMaxDeviationBps) | None | PASS | |
| 24 | `set_oracle_max_deviation_bps` | admin | (delegates to `schedule_oracle_deviation_bps`) | (same as above) | None | PASS | Thin alias |
| 25 | `set_oracle_stale_threshold` | admin | (delegates to `schedule_oracle_stale_threshold`) | (same as above) | None | PASS | Thin alias |
| 26 | `set_windows` | admin | (delegates to `schedule_windows`) | (same as above) | None | PASS | Thin alias |
| 27 | `set_max_stake` | admin | (delegates to `schedule_max_stake`) | (same as above) | None | PASS | Thin alias |
| 28 | `set_max_user_exposure` | admin | (delegates to `schedule_max_user_exposure`) | (same as above) | None | PASS | Thin alias |
| 29 | `set_max_pending_winnings` | admin | (delegates to `schedule_max_pending_winnings`) | (same as above) | None | PASS | Thin alias |
| 30 | `apply_scheduled_changes` | any | Schema OK, not paused, pending change exists, activation_ledger reached | Apply config payload (write actual config key), remove PendingConfigChange | Emit `(config, applied)` | PASS | |
| 31 | `cancel_config_change` | admin | Schema OK, not paused, pending change exists, activation ledger not yet reached | Remove PendingConfigChange | Emit `(config, cancelled)` | PASS (fixed) | **CEI fix applied**: storage removal now occurs before event emission. See SR-2026-06-002. |
| 32 | `mint_initial` | none | Checks if user already minted (balance > 0) | Write balance | None | PASS | |

**Total mutating entrypoints audited: 32 (including aliases)**
**CEI violations found: 2**
**CEI violations fixed: 2**
**Remaining open items: 0**

---

## Violations Fixed

### SR-2026-06-001 — `claim_winnings`: Effect ordering (Low, Mitigated → Hardened #260)

**Original issue:** Before the CEI fix, the balance was credited *before* the pending
winnings slot was removed, leaving a window where state was temporarily inconsistent.

**After first fix (issue #195):**
```rust
// EFFECT: remove the claim slot FIRST (prevents double-claim in future cross-contract paths)
env.storage().persistent().remove(&key);
// EFFECT: then credit the balance
Self::_set_balance(&env, user.clone(), new_balance);
// INTERACTION: event reflects committed state
env.events().publish(...);
```

**After hardening (issue #260):**
```rust
// ── Checks ────────────────────────────────────────────────────────────
_require_supported_schema(&env)?;
user.require_auth();
_ensure_not_paused(&env)?;  // rejects FullyPaused; allows Normal & ClaimsOnly

let key = DataKey::PendingWinnings(user.clone());
let pending: i128 = env.storage().persistent().get(&key).unwrap_or(0);

if pending == 0 { return Ok(0); }  // idempotent zero-pending fast path

let current_balance = balance(env.clone(), user.clone());
let new_balance = payout_add(current_balance, pending)?;  // overflow guard

// ── Effects ───────────────────────────────────────────────────────────
// 1. Remove the pending-winnings claim slot FIRST (prevent double-claim).
env.storage().persistent().remove(&key);

// 2. Credit the user balance only after the claim slot is cleared.
_set_balance(&env, user.clone(), new_balance);

// ── Interactions ──────────────────────────────────────────────────────
// Structured event: (user, amount_claimed, balance_before, balance_after)
env.events().publish(
    (symbol_short!("claim"), symbol_short!("winnings")),
    (user.clone(), pending, current_balance, new_balance),
);
```

**Key improvements in the hardened version (Issue #260):**

1. **Inline CEI documentation** — Every block (`Checks`, `Effects`, `Interactions`) is
   explicitly commented so reviewers can verify ordering at a glance.

2. **Richer structured event** — The event now emits the full 4-tuple
   `(user, amount_claimed, balance_before, balance_after)` so indexers can
   reconstruct the exact state transition without an extra balance read.

3. **Overflow-proof arithmetic** — `payout_add(current_balance, pending)` returns
   `PayoutOverflow` if the sum exceeds `i128::MAX`. No storage writes occur on
   overflow (all-or-nothing guarantee). Tested with:
   - `balance + pending` at the exact boundary `i128::MAX` (succeeds)
   - `balance + pending` at `i128::MAX + 1` (returns `PayoutOverflow`)
   - `i128::MAX + 0` = `i128::MAX` (succeeds with zero balance)
   - Repeat idempotency after a successful claim

4. **Mode compatibility** — The function uses `_ensure_not_paused()` which allows
   `Normal` (0) and `ClaimsOnly` (1) modes while rejecting `FullyPaused` (2).

**Risk level:** Low. Soroban's execution model does not currently support reentrancy
within a single invocation, but ordering the removal of the claim slot before crediting
the balance is the strictly correct CEI ordering and future-proofs the code against
cross-contract interaction paths.

---

### SR-2026-06-002 — `cancel_config_change`: Interaction before Effect (Low, Mitigated)

**Before fix:**
```rust
let cancelled_at = env.ledger().sequence();
// INTERACTION: event emitted while key still exists in storage
env.events().publish((symbol_short!("config"), symbol_short!("cancelled")), (kind, cancelled_at));
// EFFECT: key removed (too late)
env.storage().persistent().remove(&key);
```

**After fix (CEI-correct):**
```rust
let cancelled_at = env.ledger().sequence();
// EFFECT: remove key first — committed state change
env.storage().persistent().remove(&key);
// INTERACTION: event emitted only after state is fully settled
env.events().publish((symbol_short!("config"), symbol_short!("cancelled")), (kind, cancelled_at));
```

**Risk level:** Low. The event emission cannot reenter the contract in Soroban's model, but the prior ordering meant the emitted event did not accurately represent a completed state transition (the key was still present in storage at the moment the event fired). The fix ensures state is finalized before any observable side-effect.

---

## Tests (Updated for Issue #260 Hardening)

### CEI Ordering Tests (`contracts/src/tests/cei_ordering.rs`)

| Test | Entrypoint | What it verifies |
|---|---|---|
| `test_claim_winnings_cei_pending_cleared_after_claim` | `claim_winnings` | After a successful claim, the `PendingWinnings` slot is absent, balance reflects the credited amount, AND the structured event contains `(user, amount, balance_before, balance_after)` |
| `test_cancel_config_change_cei_key_removed_before_event` | `cancel_config_change` | After cancellation, the `PendingConfigChange` key is absent, and a second cancellation attempt fails with `CommitmentNotFound` |
| `test_cancel_config_change_rejected_after_activation` | `cancel_config_change` | Cancellation is rejected once the activation ledger is reached |
| `test_claim_winnings_respects_runtime_mode` | `claim_winnings` | Claim succeeds in Normal and ClaimsOnly modes; rejected with `ContractPaused` in FullyPaused mode |

### Overflow Tests (`contracts/src/tests/overflow_tests.rs`)

| Test | What it verifies |
|---|---|
| `test_claim_winnings_happy_path` | Normal claim: pending winnings accumulate correctly, no overflow |
| `test_claim_winnings_overflow_returns_payout_overflow` | `pending = i128::MAX + balance = 1_000_0000000` → `PayoutOverflow`, storage unchanged |
| `test_claim_winnings_overflow_balance_at_max` | `balance = i128::MAX + pending = 1` → `PayoutOverflow`, storage unchanged |
| `test_claim_winnings_near_max_succeeds` | `balance = 0 + pending = i128::MAX` → success (boundary, no overflow) |
| `test_claim_winnings_boundary_max_exact` | `balance = i128::MAX - 100 + pending = 100` → success (exact i128::MAX) |
| `test_claim_winnings_boundary_max_minus_one` | `balance = 0 + pending = i128::MAX - 1` → success (i128::MAX - 1) |
| `test_claim_winnings_repeat_idempotent` | Second claim after successful claim returns 0, state unchanged |
| `test_claim_winnings_zero_pending_no_mutation` | Claim with zero pending returns 0 without any storage writes |
| `test_record_winnings_mul_overflow_returns_payout_overflow` | `amount * losing_pool` overflows → `PayoutOverflow` |
| `test_record_refunds_overflow_returns_payout_overflow` | Existing pending + refund overflows → `PayoutOverflow` |

### Event Coverage Tests (`contracts/src/tests/event_coverage.rs`)

| Test | What it verifies |
|---|---|
| `test_event_coverage_claim_winnings` | Updated to validate the 4-element structured event `(user, amount, balance_before, balance_after)` |

---

## CEI Guidelines for Future Entrypoints

When adding a new mutating entrypoint:

1. **Auth first.** Call `require_auth()` before reading state it should gatekeep.
2. **All checks before any writes.** No storage write should precede a check that could cause an early return — that leaves orphaned state.
3. **Remove/invalidate claim slots before crediting.** In any payout or claim flow, zero-out or remove the pending balance key before writing the new balance.
4. **Events last.** `env.events().publish(...)` is always the final statement before `Ok(())`.
5. **Nonce consumption is an Effect.** Mark/consume replay-guard keys after all validation checks but before winner accounting or balance updates.

---

*This document is linked from [`SECURITY_REVIEW.md`](../SECURITY_REVIEW.md) and satisfies the acceptance criteria for issue #195.*

## September 2026 Re-Audit (Issue #553)

Scope expanded from the original single-file audit (contract.rs, 32 entrypoints)
to all modular entrypoints: admin.rs, betting.rs, settlement.rs, config.rs,
governance.rs (~60 mutating entrypoints total).

### Violations Found & Fixed

| ID | Function | File | Issue | Fix |
|----|----------|------|-------|-----|
| SR-2026-09-001 | batch_touch_ttl | admin.rs | Per-item Checks interleaved with per-item Effects inside one loop (extend_ttl ran before later keys were validated) | Split into a validation-only pass, then an effects-only pass |
| SR-2026-09-002 | finalize_round | settlement.rs | _remove_pending_dispute (Effect) ran after _complete_settlement's internal ound.resolved event (Interaction) | Moved _remove_pending_dispute before the _complete_settlement call |
| SR-2026-09-003 | _record_winnings_indexed, _resolve_precision_mode | settlement.rs | outcome.loss (and orfeit.predict) events published before _update_stats_loss/_persist_user_outcome ran | Reordered so stats/outcome writes complete before the events fire |

### Accepted (No Fix)

- **get_amendment** (governance.rs): persists a lazy-expiry status write as a
  side effect of an unauthenticated getter, unlike get_gov_proposal which
  computes expiry in-memory only. Not a CEI ordering violation (no user-facing
  state changes, no funds involved) — noted for consistency, not fixed.
- **_record_winnings_legacy / _resolve_precision_legacy** (settlement.rs):
  share the same loss-event-before-effects pattern as SR-2026-09-003, but are
  gated behind the deprecated legacy-map-settlement feature flag, scheduled
  for removal by 2026-12-31. Not fixed given imminent removal.
- **_complete_settlement** (settlement.rs): contains a redundant, dead second
  removal of DataKeyCore::ActiveRound (leftover from a merge), harmless but
  worth cleaning up in a future pass.

### Modules Reviewed

- **admin.rs** (~20 entrypoints): full read, 1 violation (fixed).
- **governance.rs** (~13 entrypoints incl. constitution/amendment lifecycle): full read, 1 informational note (accepted).
- **settlement.rs** (full 2636-line file, all entrypoints incl. resolve_round, resolve_round_multi, cancel_round, void_round, finalize_round, claim_winnings, claim_many): full read, 2 violations (fixed).
- **config.rs / betting.rs**: spot-checked rather than re-read line-by-line (mostly unchanged since the June audit, which passed them); cash_out_early (new since June) checked directly — Checks/Effects ordering correct, no violations found.
