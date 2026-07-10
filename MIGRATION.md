# Migration Notes

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
