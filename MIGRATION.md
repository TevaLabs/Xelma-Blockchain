# Migration Notes

## Typed storage key split (Schema v4)

The contract now uses the `DataKeyCore` / `DataKeyScoped` / `DataKeyExt`
taxonomy exclusively. The following state paths changed from ad-hoc or legacy
keys to typed keys:

| Previous path | Canonical path | Notes |
|---|---|---|
| instance `otskew` symbol | `DataKeyCore::OracleTimestampSkew` | oracle timestamp skew |
| instance `EpMintBgt` symbol | `DataKeyCore::EpochMintBudget` | epoch mint budget |
| persistent `DisputeLedgers` symbol | `DataKeyCore::DisputeLedgers` | dispute window |
| persistent `PendingWinningsExpiryKey(())` | `DataKeyCore::PendingWinningsExpiry` | pending-winnings expiry |
| `DataKey::PendingWinnings(Address)` | `DataKeyScoped::PendingWinnings(Address)` | user pending winnings |

This release does not dual-write old and new paths. Operators upgrading an
existing deployment must run an explicit migration that reads each old path,
writes the corresponding typed path, verifies the value, and only then removes
the old path. The migration must be gated by the schema version and blocked
while a round is active. Deployments with no value at an old optional path can
accept the documented default without creating a compatibility key.

The legacy bulk-map position keys remain migration-only compatibility paths;
normal betting and cleanup use only `DataKeyScoped::Position`,
`PrecisionPosition`, `PrecisionCommitment`, and `RoundParticipants`.

## Dry-run mode (Schema v3+)

**Introduced in:** `feat/292-upgradeability-migration-dry-run-next-schema-template`

Every migration entrypoint accepts a `dry_run: bool` parameter. When `true`:

- All validation checks run (admin auth, paused guard, active round guard, source version match)
- No storage writes or event emissions occur
- Returns `Ok(())` on success, `Err` on any validation failure

This allows operators to confirm a migration would succeed before committing to it:

```rust
// Preview — no writes
client.migrate_schema_v1_to_v2(true);
// Real migration
client.migrate_schema_v1_to_v2(false);
```

### Active round guard

Migrations are **always** blocked while a round is active, even in dry-run mode:

```rust
// Both fail with MigrationActiveRound if a round is live
client.migrate_schema_v1_to_v2(true);
client.migrate_schema_v1_to_v2(false);
```

### Checklist

1. **Dry-run the migration first**
   ```rust
   client.migrate_schema_v2_to_v3(true);
   // If Ok(()), the real migration will pass validation.
   ```

2. **Run the real migration**
   ```rust
   client.migrate_schema_v2_to_v3(false);
   ```

3. **Verify schema version**
   ```rust
   assert_eq!(client.get_schema_version(), 3u32);
   ```

## V-Next schema template (Schema v3+)

**Introduced in:** `feat/292-upgradeability-migration-dry-run-next-schema-template`

Admins may announce a planned future schema version via `announce_next_schema`:

```rust
// Announce that v4 is the next planned migration
client.announce_next_schema(4);
```

This writes the target version to a dedicated storage slot
(`DataKey::NextSchemaVersion`) and emits `("schema", "next_ann")` with
`(current_version, target_version)`.

The announced version is purely informational — it does not change the active
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

## Schema v2 → v3: add per-user archived round outcome records

**Introduced in:** `fix/migration-v2-to-v3`

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

### Operator checklist

1. **Confirm no active round**
   ```bash
   # The contract rejects migration while an active round exists, but
   # operators should also coordinate with the oracle/admin to avoid
   # creating rounds during the migration window.
   client.get_active_round()  # must be None
   ```

2. **Run the migration**
   ```rust
   client.migrate_schema_v2_to_v3();
   ```

3. **Verify schema version**
   ```rust
   assert_eq!(client.get_schema_version(), 3u32);
   ```

4. **Confirm migration marker**
   ```rust
   let marker = env.as_contract(...).storage()
       .persistent().get::<_, bool>(&DataKey::MigratedToV3);
   assert_eq!(marker, Some(true));
   ```

5. **Query archived participation (smoke test)**
   Pick a recently-resolved round and confirm the query returns data for a known participant:
   ```rust
   let outcome = client.get_user_archived_participation(user, round_id);
   assert!(outcome.is_some());
   ```

6. **Resume normal operations**
   Once verified, operators may resume creating rounds normally.

### Rollback / safety

This migration is additive only — no existing fields are removed or re-interpreted.
If the contract halts during migration, simply replay `migrate_schema_v2_to_v3()`;
it is idempotent on the `SchemaVersion` write (guarded by explicit version check)
and skips already-persisted `UserRoundOutcome` keys.

---

## Package Rename: `@tevalabs/xelma-bindings` → `@xelma/bindings`

**Introduced in:** `fix/bindings-package-metadata`

### What changed

The npm package name was updated from the placeholder org-scoped name
`@tevalabs/xelma-bindings` to the canonical Xelma namespace `@xelma/bindings`.

The following metadata fields were also added or corrected:

| Field | Before | After |
|-------|--------|-------|
| `name` | `@tevalabs/xelma-bindings` | `@xelma/bindings` |
| `repository` | _(absent)_ | `https://github.com/TevaLabs/Xelma-Blockchain` |
| `author` | _(absent)_ | `TevaLabs` |
| `license` | _(absent)_ | `MIT` |

### Migration steps for consumers

1. **Uninstall the old package** (if previously published under the old name):

   ```sh
   npm uninstall @tevalabs/xelma-bindings
   ```

2. **Install the new package:**

   ```sh
   npm install @xelma/bindings
   ```

3. **Update all import statements:**

   ```diff
   - import { Client } from '@tevalabs/xelma-bindings';
   + import { Client } from '@xelma/bindings';
   ```

### Import path impact

Only the package name changed. All exported symbols (`Client`, `ContractError`,
`BetSide`, `RoundMode`, `UserPosition`, etc.) remain identical — no code changes
are required beyond updating the import path.
