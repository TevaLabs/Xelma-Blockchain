# Schema Migration Playbook (Operator)

> **Issue #517.** Guarded migration entrypoints and the operator dry-run
> procedure: **pause → dry-run → migrate**. This is the operational companion
> to the schema history in [`../MIGRATION.md`](../MIGRATION.md).
>
> Every behaviour below is enforced by
> [`contracts/src/tests/migration_versioning.rs`](../contracts/src/tests/migration_versioning.rs)
> and the end-to-end chaos drill in
> [`contracts/src/tests/drill_chaos_migration.rs`](../contracts/src/tests/drill_chaos_migration.rs).

Conventions used in the commands:

```bash
export NET=testnet                 # or mainnet
export CID=<CONTRACT_ID>
export ADMIN=<admin identity>      # stellar keys alias holding the admin key
invoke() { stellar contract invoke --id "$CID" --network "$NET" --source "$ADMIN" -- "$@"; }
view()   { stellar contract invoke --id "$CID" --network "$NET" --source "$ADMIN" --send=no -- "$@"; }
```

Error numbers are the `ContractError` discriminants from
[`contracts/src/errors.rs`](../contracts/src/errors.rs); the CLI prints them as
`Error(Contract, #N)`.

---

## 1. Entrypoints

All migration entrypoints are admin-authenticated and take a `dry_run: bool`.

| Entrypoint                    | Purpose                                             |
|-------------------------------|-----------------------------------------------------|
| `get_schema_version`          | Reads the active schema version (`1` if never set). |
| `migrate_schema_v1_to_v2`     | `1 → 2`. Validates, then bumps `SchemaVersion`.     |
| `migrate_schema_v2_to_v3`     | `2 → 3`. Validates, bumps `SchemaVersion`, writes the `MigratedToV3` marker. |
| `announce_next_schema`        | Records the *planned* next version (informational only). |
| `get_next_schema`             | Reads the announced next version, if any.           |
| `clear_next_schema`           | Clears the announcement.                            |

`dry_run = true` runs **every** validation check and returns `Ok(())` on
success, but performs **no storage writes and emits no events**. It is safe to
run against production: the chaos drill asserts a full user-visible state
snapshot is byte-for-byte identical before and after a successful dry-run.

## 2. Guardrails

A real migration is refused unless *all* of the following hold. The same checks
run in dry-run, so a passing dry-run is a faithful preview.

| Guard                            | Rejection                    | Error                          |
|----------------------------------|------------------------------|--------------------------------|
| Admin authentication             | non-admin caller             | (auth)                         |
| Contract not `FullyPaused`       | runtime mode `2`             | `ContractPaused` (#22)         |
| No active round                  | `ActiveRound` is set         | `MigrationActiveRound` (#44)   |
| Source version matches expected  | wrong stored/legacy version  | `UnsupportedSchemaVersion` (#42) |

> **Pause ≠ FullyPaused.** A migration is an `AdminConfig` action: it is
> blocked by `FullyPaused` but allowed in `ClaimsOnly`. To "pause" for a
> migration, move to `ClaimsOnly` (mode `1`), **not** `pause_contract`
> (`FullyPaused`, mode `2`). See
> [`DEPLOYMENT_RUNBOOK.md`](./DEPLOYMENT_RUNBOOK.md) for the incident pause path.

## 3. Testnet checklist (pause → dry-run → migrate)

Run this top-to-bottom on testnet before touching mainnet. The chaos drill
mirrors it under simulated failures (repeated pauses, escalation and
de-escalation, dry-runs in every mode).

- [ ] **1. Confirm the target migration and expected source version.**
  `view get_schema_version` must equal the `from` of the migration you intend to
  run (a missing key is treated as legacy `1`).

- [ ] **2. Coordinate a quiet window so no round is created.**
  `view get_active_round` must be `None`. The contract refuses migration while
  a round is live (`MigrationActiveRound`).

- [ ] **3. Pause new activity in `ClaimsOnly`.**
  `invoke set_runtime_mode --mode 1`.
  `view get_runtime_mode` → `1`. Claims remain available; migration is allowed.
  Do **not** use `pause_contract` here — `FullyPaused` blocks migration.

- [ ] **4. Dry-run the migration.**
  `invoke migrate_schema_v2_to_v3 --dry_run true`.
  Expect `Ok(())`. A failure here means the real migration would fail for the
  same reason — stop and fix before proceeding.

- [ ] **5. Confirm the dry-run mutated nothing.**
  `view get_schema_version` is unchanged; no `("schema", "migrated")` event was
  emitted. (The chaos drill asserts this atomically for the whole state.)

- [ ] **6. Run the real migration.**
  `invoke migrate_schema_v2_to_v3 --dry_run false`.

- [ ] **7. Verify post-conditions.**
  - `view get_schema_version` → `3`.
  - `("schema", "migrated")` emitted with `(from, to)` = `(2, 3)`.
  - For `v2 → v3`, the `MigratedToV3` marker is present.

- [ ] **8. Smoke-test the new query surface.**
  For `v2 → v3`, pick a recently resolved round and confirm
  `view get_user_archived_participation --user <G...> --round_id <id>` returns a
  record for a known participant.

- [ ] **9. Resume normal operation.**
  `invoke set_runtime_mode --mode 0`; `view get_runtime_mode` → `0`.

- [ ] **10. (Optional) Announce the next planned version** while the window is
  open: `invoke announce_next_schema --target_version 4`, and clear it with
  `clear_next_schema` once the plan firms up.

## 4. Announcing a planned version

`announce_next_schema` is purely informational — it never changes the active
schema or gates any entrypoint. It writes `DataKey::NextSchemaVersion` and
emits `("schema", "next_ann")` with `(current_version, target_version)`.

| Rule                                        | Behaviour                        |
|---------------------------------------------|----------------------------------|
| `target_version == 0`                       | `UnsupportedSchemaVersion` (#42) |
| `target_version <= CURRENT_SCHEMA_VERSION`  | `UnsupportedSchemaVersion` (#42) |
| Admin authentication                        | required                         |
| `clear_next_schema` when nothing announced  | `UnsupportedSchemaVersion` (#42) |

## 5. Rollback & safety

- Migrations are **additive**: no existing field is removed or re-interpreted.
- `SchemaVersion` is written only after an explicit source-version check, so
  re-invoking the same migration is a no-op returning
  `UnsupportedSchemaVersion` rather than corrupting state.
- If a run halts mid-transaction, Soroban reverts the whole invocation — no
  partial schema state is written.
- Replaying a `v2 → v3` migration skips already-persisted `UserRoundOutcome`
  keys (`_persist_user_outcome` is idempotent).

## 6. Chaos coverage

`contracts/src/tests/drill_chaos_migration.rs` drives the procedure end-to-end:

- active round × pause × migration dry-run × claims-only, asserting the
  dry-run is refused atomically and leaves every user-visible amount and the
  schema intact;
- after the round is cleared, the deferred dry-run passes **without mutating
  state**, then the real migration commits and normal mode resumes.
