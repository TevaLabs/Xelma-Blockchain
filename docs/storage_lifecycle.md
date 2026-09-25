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
- **`MaxUserRoundExposure` semantics:** cumulative per-user exposure for a round counts all active stakes from Up/Down positions, revealed precision predictions, and unrevealed precision commitments. The cap is enforced through the shared round-exposure helper before each entrypoint mutates state.
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

---

## 5. Batch TTL Touch Entrypoint (`batch_touch_ttl`)

A dedicated admin-gated entrypoint, `batch_touch_ttl`, allows maintainers to
proactively extend the TTL of system-critical storage keys without waiting for
on-access extension. This is essential for production rent maintenance.

### Entrypoint

```rust
pub fn batch_touch_ttl(env: Env, keys: Vec<DataKeyCore>) -> Result<u32, ContractError>
```

- **Auth gate**: Requires admin authentication (`admin.require_auth()`).
- **Allowlist**: Only keys in the `_is_ttl_touch_allowed` allowlist are accepted.
  Supplying a non-allowlisted key fails the entire call with
  `UnsupportedDataKeyForTtlTouch`.
- **Absent keys**: Keys in the allowlist but absent from storage are silently
  skipped (counted as part of the event's `skipped` field).
- **Return value**: Number of keys whose TTL was actually extended.
- **Event**: Emits `("storage", "touch")` with `(touched: u32, skipped: u32)`.

### Allowlisted Key Classes

| Class               | Keys                                                                 |
|---------------------|----------------------------------------------------------------------|
| Core config         | `Admin`, `Oracle`, `SchemaVersion`, `Paused`                        |
| Windows             | `BetWindowLedgers`, `RunWindowLedgers`, `CloseBufferLedgers`        |
| Risk limits         | `MaxStake`, `MaxUserRoundExposure`, `MaxPendingWinnings`            |
| Matchmaking         | `MinParticipants`, `MaxPrecisionParticipants`                       |
| Oracle safety       | `OracleHeartbeat`, `OracleStaleThreshold`, `OracleMaxDeviationBps`,
|                     | `OracleDeviationOverrideArmed`, `OracleMinConfidenceBps`,
|                     | `OracleStrictMode`                                                   |
| Protocol fee        | `ProtocolFeeBps`, `ProtocolFeeTreasury`                              |
| Migration           | `MigratedToV3`                                                       |
| Archive / Keeper    | `ArchiveRetention`, `RoundTemplate`                                  |
| Leaderboard         | `LeaderboardWins`, `LeaderboardStreak`, `SeasonId`,
|                     | `SeasonLeaderboardWins`, `SeasonLeaderboardStreak`                   |
| Round counter       | `LastRoundId`                                                         |
| Rotation            | `OracleRotationProposal`                                             |
| Mint                | `MintLimitConfig`                                                    |

Per-user keys (`Balance`, `PendingWinnings`, `UserStats`) and round-scoped
keys (`ActiveRound`, `Position`, `PrecisionPosition`, `RoundParticipants`,
`ArchivedRound`, etc.) are **excluded** because they are either ephemeral
(deleted after settlement/cancel) or managed through the standard on-access
TTL extension path.

### Cadence Recommendations

| Environment | Recommended Frequency | Trigger                          |
|-------------|----------------------|----------------------------------|
| Testnet     | Weekly / on-demand   | After contract upgrades or       |
|             |                      | long idle periods                |
| Mainnet     | Every 7–10 days      | Cron job or monitoring alert     |
|             |                      | when storage TTL drops below     |
|             |                      | 7 days (~120_960 ledgers)        |

### Example Usage (Script / CLI)

```bash
# Using stellar-cli with a Soroban contract invocation:
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source-account <ADMIN_SECRET> \
  --network testnet \
  -- \
  batch_touch_ttl \
  --keys '[{"variant": "Admin"}, {"variant": "Oracle"}, {"variant": "SchemaVersion"}]'
```

> [!IMPORTANT]
> The `batch_touch_ttl` entrypoint is **not** a substitute for proper
> on-access TTL extension in the contract logic. It exists as a safety net
> and maintenance tool for emergencies and long-idle deployments. Always
> ensure read/write paths call `_extend_persistent_ttl` per the on-access
> policy (Section 2).
