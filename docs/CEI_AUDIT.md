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
| 9 | `commit_prediction` | user | Not paused, amount > 0, max stake, max exposure, mode == Precision, bet window open, balance sufficient, no duplicate | Write balance, PrecisionCommitment, RoundParticipants | Emit `(commit, predict)` | PASS | |
| 10 | `reveal_prediction` | user | Not paused, mode == Precision, reveal window open, commitment exists, not already revealed, hash matches | Write commitment.revealed=true, PrecisionPosition | Emit `(reveal, predict)` | PASS | Hash verified before any write |
| 11 | `resolve_round` | oracle | Schema OK, price != 0, not paused, active round exists, round_id matches, network_id matches, contract_addr matches, timestamp not future, not stale, deviation check, nonce not consumed, round has ended | Write ConsumedOracleNonce, OracleDeviationOverrideArmed (remove), pending winnings (N users), archived round, cleanup (N positions + participants + ActiveRound) | Emit `(round, resolved)` or `(round, fallback)` or `(pool, onesided)` | PASS | Nonce is consumed (Effect) after all validation Checks but before winner accounting begins |
| 12 | `cancel_round` | admin | Schema OK, active round exists | Write pending winnings (refunds), archived round, RoundParticipants (remove), CancelledRound=true, ActiveRound (remove) | Emit `(round, cancelled)` | PASS | All state mutations complete before event |
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

### SR-2026-06-001 — `claim_winnings`: Effect ordering (Low, Mitigated)

**Before fix:**
```rust
// EFFECT: balance increased
Self::_set_balance(&env, user.clone(), new_balance);
// EFFECT: pending winnings slot removed (too late — state temporarily inconsistent)
env.storage().persistent().remove(&key);
// INTERACTION: event
env.events().publish(...);
```

**After fix (CEI-correct):**
```rust
// EFFECT: remove the claim slot FIRST (prevents double-claim in future cross-contract paths)
env.storage().persistent().remove(&key);
// EFFECT: then credit the balance
Self::_set_balance(&env, user.clone(), new_balance);
// INTERACTION: event reflects committed state
env.events().publish(...);
```

**Risk level:** Low. Soroban's execution model does not currently support reentrancy within a single invocation, but ordering the removal of the claim slot before crediting the balance is the strictly correct CEI ordering and future-proofs the code against cross-contract interaction paths.

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

## Tests Added

Two regression tests were added to `contracts/src/tests/cei_ordering.rs` to verify the corrected orderings:

| Test | Entrypoint | What it verifies |
|---|---|---|
| `test_claim_winnings_cei_pending_cleared_before_balance` | `claim_winnings` | After a successful claim, the `PendingWinnings` slot is absent and balance reflects the credited amount |
| `test_cancel_config_change_cei_key_removed_before_event` | `cancel_config_change` | After cancellation, the `PendingConfigChange` key is absent, and a second cancellation attempt fails with `CommitmentNotFound` |

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
