# Migration Notes

> **Schema versioning is the contract's upgrade safety mechanism.**  
> The on-chain `SchemaVersion` storage key tracks which data-layout rules the
> contract enforces. Every mutating entrypoint rejects calls when the schema
> version is unknown or unsupported, protecting operators from deploying
> incompatible contract upgrades.

---

## Schema version history

| version | introduced by | key changes |
|---------|--------------|-------------|
| 1 | initial deploy | Legacy layout (no explicit version stored; absent = v1) |
| 2 | `migrate_schema_v1_to_v2` | Explicit `SchemaVersion` key persisted |
| 3 | `migrate_schema_v2_to_v3` | Per-user `UserRoundOutcome` archived participation records; `MigratedToV3` marker |

---

## Dry-run mode (v3+)

**Every migration entrypoint** accepts a `dry_run: bool` parameter.

When `dry_run = true`:

- All validation checks run: admin auth, **normal mode** guard, active-round guard, source version match.
- **No storage writes occur** — schema version, `MigratedToV3` flag, and all other keys are untouched.
- **No events are emitted** — the event log remains clean.
- Returns `Ok(())` on success, `Err` on any validation failure.
- **Strictly read-only**: safe to call any number of times with zero side effects.

### Migration guards (applied in both dry-run and real mode)

| guard | error | when |
|-------|-------|------|
| Admin auth required | `AdminNotSet` | Contract not initialized |
| Normal mode required | `ContractPaused` | Contract is `FullyPaused` or `ClaimsOnly` |
| No active round | `MigrationActiveRound` | A round is currently live |
| Correct source version | `UnsupportedSchemaVersion` | Schema version doesn't match the expected source |

> **ClaimsOnly is blocked**: migrations are rejected in both `FullyPaused` and
> `ClaimsOnly` modes. The contract must be in `Normal` mode with no active round
> before any migration is attempted. This prevents schema changes during
> transitional states.

### Example usage

```rust
// Preview — no writes, no events
client.migrate_schema_v2_to_v3(true);

// Real migration
client.migrate_schema_v2_to_v3(false);

// Verify
assert_eq!(client.get_schema_version(), 3);
```

---

## Operator migration checklist

This is the **authoritative step-by-step procedure** for operators performing
a schema migration on testnet or mainnet. Follow every step in order.

### Pre-flight

1. **Confirm contract version and schema**
   ```rust
   let version = client.get_schema_version();
   // Must match the expected source version for the target migration
   ```

2. **Confirm no active round**
   ```rust
   let round = client.get_active_round();
   assert!(round.is_none(), "active round exists — cannot migrate");
   ```

3. **Confirm Normal mode** (not Paused, not ClaimsOnly)
   ```rust
   let mode = client.get_runtime_mode();
   assert_eq!(mode, 0, "must be Normal mode (0)");
   ```

4. **Pause new round creation** — coordinate with all operators to stop
   calling `create_round` until migration completes.

### Dry-run

5. **Run the dry-run migration**
   ```rust
   let result = client.try_migrate_schema_v2_to_v3(&true);
   assert_eq!(result, Ok(Ok(())), "dry-run failed: {:?}", result);
   ```

6. **Verify state was NOT mutated**
   ```rust
   assert_eq!(client.get_schema_version(), expected_source_version);
   ```

### Execute

7. **Run the real migration**
   ```rust
   client.migrate_schema_v2_to_v3(&false);
   ```

8. **Verify schema version updated**
   ```rust
   assert_eq!(client.get_schema_version(), expected_target_version);
   ```

9. **Verify migration marker (v3 only)**
   ```rust
   let marker = env.as_contract(&contract_id, || {
       env.storage().persistent().get::<_, bool>(&DataKeyCore::MigratedToV3)
   });
   assert_eq!(marker, Some(true));
   ```

### Post-flight

10. **Smoke test a critical entrypoint**
    ```rust
    // Create a round and verify normal operation
    client.create_round(&1_0000000, &None);
    assert!(client.get_active_round().is_some());
    ```

11. **Resume round creation** — notify operators that `create_round` is
    safe to call again.

12. **Announce next migration (optional)** — if a future schema version
    is planned:
    ```rust
    client.announce_next_schema(&4);
    ```

### Rollback / safety

- **Migrations are additive only** — no existing fields are removed or re-interpreted.
- If the contract halts mid-migration, simply re-run the migration function; it is
  idempotent on the `SchemaVersion` write (guarded by explicit version check) and
  skips already-persisted keys.
- **Dry-run can be repeated** as many times as needed with no side effects.

---

## V-Next schema template (v3+)

Admins may announce a planned future schema version via `announce_next_schema`:

```rust
// Announce that v4 is the next planned migration
client.announce_next_schema(4);
```

This writes the target version to a dedicated storage slot
(`DataKeyCore::NextSchemaVersion`) and emits `("schema", "next_ann")` with
`(current_version, target_version)`.

The announced version is **purely informational** — it does not change the active
schema version or gate any entrypoints. Operators and monitoring dashboards
can inspect it:

```rust
let next = client.get_next_schema(); // Some(4)
```

To clear the announcement before the migration executes:

```rust
client.clear_next_schema();
assert_eq!(client.get_next_schema(), None);
```

### Validation rules

| rule | behaviour |
|------|-----------|
| `target_version == 0` | rejected (`UnsupportedSchemaVersion`) |
| `target_version <= CURRENT_SCHEMA_VERSION` | rejected (`UnsupportedSchemaVersion`) |
| Admin authentication | required |
| Clear when not set | rejected (`UnsupportedSchemaVersion`) |

---

## Schema v2 → v3: per-user archived round outcome records

### What changed

Schema v3 adds `UserRoundOutcome` per-user records persisted at settlement so that
integrators can query a user's participation and outcome for any archived round
without replaying the full event stream.

The new public entrypoint is:

```rust
pub fn get_user_archived_participation(
    env: Env,
    user: Address,
    round_id: u64,
) -> Option<UserRoundOutcome>
```

The returned record carries:

| field              | meaning                                                  |
|--------------------|----------------------------------------------------------|
| `round_mode`       | `0` = UpDown, `1` = Precision                            |
| `prediction_side`  | `0` = Up, `1` = Down, `2` = Precision                    |
| `predicted_price`  | guess in scaled units (meaningful only for Precision)     |
| `stake`            | amount staked by the user in stroops                     |
| `payout`           | amount the user actually received (0 on loss)            |
| `outcome`          | `0` = Win, `1` = Loss, `2` = Refund, `3` = Cancel        |

Missing data returns `None` cleanly.

### Operator checklist for v2 → v3

1. **Confirm no active round** (see pre-flight above)
2. **Dry-run** `migrate_schema_v2_to_v3(true)` — verify `Ok(())`
3. **Run** `migrate_schema_v2_to_v3(false)`
4. **Verify** `get_schema_version() == 3`
5. **Verify** `MigratedToV3 == true`
6. **Smoke test** `get_user_archived_participation` on a known resolved round
7. **Resume** normal operations

---

## Package Rename: `@tevalabs/xelma-bindings` → `@xelma/bindings`

**Introduced in:** `fix/bindings-package-metadata`

### What changed

The npm package name was updated from the placeholder org-scoped name
`@tevalabs/xelma-bindings` to the canonical Xelma namespace `@xelma/bindings`.

| Field | Before | After |
|-------|--------|-------|
| `name` | `@tevalabs/xelma-bindings` | `@xelma/bindings` |
| `repository` | _(absent)_ | `https://github.com/TevaLabs/Xelma-Blockchain` |
| `author` | _(absent)_ | `TevaLabs` |
| `license` | _(absent)_ | `MIT` |

### Migration steps for consumers

```sh
npm uninstall @tevalabs/xelma-bindings
npm install @xelma/bindings
```

```diff
- import { Client } from '@tevalabs/xelma-bindings';
+ import { Client } from '@xelma/bindings';
```

Only the package name changed. All exported symbols remain identical.
