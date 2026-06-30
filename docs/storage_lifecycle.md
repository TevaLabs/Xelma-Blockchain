# Storage Lifecycle & Rent Policy

This document defines the storage Time-To-Live (TTL) and rent policy for the Xelma prediction market contract. Soroban smart contracts require explicit rent management to prevent state bloat and ensure that long-lived data remains accessible in production.

---

## 1. Soroban Storage Types

The contract uses two main categories of storage key-value pairs depending on their intended lifetime:

### Long-Lived Storage (Persistent)
Keys that must survive indefinitely or persist across multiple rounds:
- **`Admin` & `Oracle` addresses:** Access control configuration.
- **`SchemaVersion`:** Migration version tracking.
- **`Paused`:** Emergency circuit-breaker state.
- **`BetWindowLedgers` & `RunWindowLedgers`:** Duration configuration.
- **`MaxStake`, `MaxUserRoundExposure`, `MaxPendingWinnings`:** Risk limits.
- **`MinParticipants` & `MaxPrecisionParticipants`:** Matchmaking limits.
- **`OracleStaleThreshold` & `OracleMaxDeviationBps`:** Oracle safety config.
- **`OracleHeartbeat`:** Heartbeat liveness record.
- **`Balance(Address)` & `PendingWinnings(Address)`:** Financial balances.
- **`UserStats(Address)`:** User performance history.

### Short-Lived Storage (Persistent / Ephemeral Lifecycle)
Keys created dynamically for a single round's duration:
- **`ActiveRound` & `LastRoundId`:** Active round state and counter.
- **`Position(round_id, user)` & `PrecisionPosition(round_id, user)`:** User-placed bets.
- **`PrecisionCommitment(round_id, user)`:** Secret prediction commits.
- **`RoundParticipants(round_id)`:** Active participant index.

> [!NOTE]
> All short-lived position and commitment keys are explicitly deleted during `resolve_round` or `cancel_round` to reclaim storage rent and keep the ledger clean.

---

## 2. TTL Extension Policy

To ensure long-lived data is never archived due to expiration, the contract enforces an **on-access extension strategy** using the following parameters:

| Parameter | Value (Ledgers) | Duration (Approx. Real Time) |
|---|---|---|
| **`TTL_BUMP_THRESHOLD`** | `17,280` | ~24 Hours (1 Day) |
| **`TTL_BUMP_AMOUNT`** | `518,400` | ~30 Days (1 Month) |

### Mechanism
Every time a long-lived persistent key is read, updated, or written, its remaining TTL is inspected:
- If remaining TTL is **less than 1 day** (`17,280` ledgers), it is bumped to **30 days** (`518,400` ledgers) from the current ledger sequence.
- This checks are performed automatically using the internal `_extend_persistent_ttl` helper.

```rust
fn _extend_persistent_ttl(env: &Env, key: &DataKey) {
    if env.storage().persistent().has(key) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
    }
}
```

---

## 3. Extension Touchpoints

TTL bumps are integrated into the following paths in `contracts/src/contract.rs`:

1. **Contract Initialization:**
   - `initialize` sets default configs and bumps TTLs for `Admin`, `Oracle`, `Paused`, `SchemaVersion`, `BetWindowLedgers`, and `RunWindowLedgers`.
2. **Economic / Governance Configuration:**
   - Any getter/setter for contract parameters (such as `set_max_stake`, `set_oracle_stale_threshold`, `set_min_participants`, etc.) extends the TTL of the modified configuration key.
3. **User Access Paths:**
   - **`balance` / `mint_initial`:** Bumps the user's `Balance` key.
   - **`get_user_stats` / `_update_stats_*`:** Bumps the user's `UserStats` key.
   - **`get_pending_winnings` / `claim_winnings` / `_accumulate_pending`:** Bumps the user's `PendingWinnings` key.
4. **Oracle Interaction Paths:**
   - **`update_oracle_heartbeat` / `is_oracle_live`:** Bumps `OracleHeartbeat` and `OracleStaleThreshold`.

---

## 4. Developer Guidelines

When adding new persistent keys or modifying storage layouts:
1. Determine if the new key is **long-lived** or **short-lived**.
2. If it is long-lived:
   - Ensure that every read/write to the key calls `Self::_extend_persistent_ttl(&env, &key)` immediately after access.
   - If the access is inside a common getter, make sure it is extended there.
3. If it is short-lived:
   - Ensure that the key is explicitly cleared (`env.storage().persistent().remove(...)`) when its lifecycle completes.
4. Always add verification tests in `contracts/src/tests/ttl_tests.rs`.
