# Operator Dry-Run Migration Playbook

**Issues:** [#885](https://github.com/TevaLabs/Xelma-Blockchain/issues/885)
**Audience:** protocol operators, release owners, incident leads.
**Companion docs:** [`MIGRATION.md`](../MIGRATION.md) (schema changes),
[`docs/DEPLOYMENT_RUNBOOK.md`](./DEPLOYMENT_RUNBOOK.md) (deploy/rollback),
[`docs/EMERGENCY_DRILL.md`](./EMERGENCY_DRILL.md) (pause and incident modes),
[`docs/indexer_guide_user_round_outcome.md`](./indexer_guide_user_round_outcome.md)
(schema v3 query guide for downstream consumers).

This playbook is the repeatable, testnet-first procedure for executing a schema
migration. Every migration entrypoint is **dry-runnable**, so the same command
sequence is used on testnet and mainnet; only the network, the approval gate, and
the comms differ.

---

## 1. Scope and contract surface

| Migration | Entrypoint | Required source schema | Target | Marker |
| --- | --- | --- | --- | --- |
| v1 → v2 | `migrate_schema_v1_to_v2(dry_run: bool)` | `1` (absent version counts as `1`) | `2` | `SchemaVersion = 2` |
| v2 → v3 | `migrate_schema_v2_to_v3(dry_run: bool)` | `2` | `3` | `SchemaVersion = 3` + `MigratedToV3 = true` |

Related, non-gating entrypoints:

| Entrypoint | Auth | Effect |
| --- | --- | --- |
| `get_schema_version()` | none | Returns the active schema version. |
| `announce_next_schema(target_version)` | admin | Records `NextSchemaVersion`; informational only. Rejects `0` and any version `<= CURRENT_SCHEMA_VERSION` (currently `3`), so it announces *future* work, not the migration being executed. Emits `("schema", "next_ann")`. |
| `get_next_schema()` | none | Returns the announced target, if any. |
| `clear_next_schema()` | admin | Clears the announcement. Rejects when unset. Emits `("schema", "next_clr")`. |

**Dry-run semantics are identical across migrations:**

- Admin auth, pause guard, active-round guard, and source-version match all run.
- No storage writes and **no events** are emitted when `dry_run = true`.
- Returns success (`Ok`) when the real migration would pass validation.
- The active-round guard applies in dry-run too — you cannot "preview" a migration
  while a round is live.

---

## 2. Roles and go/no-go owners

| Role | Responsibility |
| --- | --- |
| Release owner | Approves the migration window; signs the final go/no-go. |
| Operator | Executes dry-run, real migration, and verification commands. |
| Indexer/consumer owner | Confirms downstream backfill and query readiness (schema v3 archive records). |
| Incident lead | Owns abort/rollback and external comms if verification fails. |

Nothing in this playbook is executed on mainnet without a named release owner and a
recorded dry-run result from the **same artifact hash** that will be migrated.

---

## 3. Pre-flight prerequisites

- [ ] Admin key available and confirmed to be the contract's current `Admin`.
- [ ] A read-only key/account available for verification calls.
- [ ] Target network and passphrase confirmed (`testnet` / `mainnet`; never infer).
- [ ] WASM artifact built from the reviewed commit:
      `target/wasm32v1-none/release/xelma_contract.wasm` (`wasm32v1-none`, `--locked`).
- [ ] SHA-256 of the artifact recorded in the change ticket.
- [ ] No round is active, and round creation is frozen for the window.
- [ ] Contract is not paused (a paused contract rejects migrations with `ContractPaused`).
- [ ] Indexer/frontend owners notified of the window and the expected `schema`,
      `payout`, and `archive` events.
- [ ] Rollback owner and previous artifact hash recorded.

---

## 4. Command reference

All examples use the Stellar CLI's contract invoke form; the `--` separates client
flags from contract arguments.

```bash
# Read-only checks
stellar contract invoke --id "$CONTRACT_ID" --source "$READ_ONLY" --network "$NETWORK" \
  -- get_schema_version

stellar contract invoke --id "$CONTRACT_ID" --source "$READ_ONLY" --network "$NETWORK" \
  -- get_active_round

stellar contract invoke --id "$CONTRACT_ID" --source "$READ_ONLY" --network "$NETWORK" \
  -- is_paused

stellar contract invoke --id "$CONTRACT_ID" --source "$READ_ONLY" --network "$NETWORK" \
  -- get_admin

# Dry run (no writes, no events)
stellar contract invoke --id "$CONTRACT_ID" --source "$ADMIN" --network "$NETWORK" \
  -- migrate_schema_v2_to_v3 --dry_run true

# Real migration (same validation, then writes the version + marker)
stellar contract invoke --id "$CONTRACT_ID" --source "$ADMIN" --network "$NETWORK" \
  -- migrate_schema_v2_to_v3 --dry_run false
```

`--dry_run` is a boolean contract argument. The CLI also accepts the flag shorthand
(`--dry_run` means `true`); use the explicit `true`/`false` form in runbooks and CI
so the intent is unambiguous.

**Expected results**

| Call | Success signature |
| --- | --- |
| Dry run | `Ok` / empty result, **zero** new events, `get_schema_version()` unchanged. |
| Real migration | `Ok`, event `("schema", "migrated")` with `(from, target)`. |
| Re-run of the real migration | `UnsupportedSchemaVersion` (source version no longer matches) — this is expected, not a partial migration. |

---

## 5. Testnet checklist (required before mainnet)

Run this end-to-end on testnet using a contract that mirrors production
configuration. Do not skip the negative drills: they are what proves the guards work.

### Phase A — Validate the candidate artifact on testnet

> **Important:** `initialize` stamps `SchemaVersion = CURRENT_SCHEMA_VERSION` (`3`
> today). A **freshly deployed** testnet contract therefore starts at schema `3`, and
> the v2 → v3 migration is neither needed nor runnable there
> (`UnsupportedSchemaVersion`). Phase A below validates build, deploy, and
> round-lifecycle behaviour only.
>
> The migration rehearsal (Phases B–E) needs a contract whose **storage is still at
> schema `2`** — normally an existing testnet deployment carried forward through the
> release's upgrade path. If your deployment tooling cannot upgrade an existing
> contract to the candidate artifact, the migration cannot be rehearsed on-chain
> (there is no way to seed `SchemaVersion = 2` on a freshly initialized contract); use
> the Rust harness fallback at the end of Phase E and record that limitation.

```bash
# 1. Build exactly as CI does
cargo rustc --manifest-path=contracts/Cargo.toml \
  --crate-type=cdylib --target=wasm32v1-none --release --locked

# 2. Record the artifact hash and compare with the reviewed build
sha256sum target/wasm32v1-none/release/xelma_contract.wasm

# 3. Validate the whole deployment path without broadcasting
./scripts/deploy_testnet.sh --dry-run

# 4. Deploy for real to testnet (authorized testnet deployment only)
./scripts/deploy_testnet.sh
```

The GitHub Actions `Deploy Testnet` workflow (`dry_run: true` first, then
`dry_run: false`) performs the same steps and attaches the deployment summary to the
run. Use whichever path your team has authorized; the artifact hash must match.

- [ ] `scripts/deploy_testnet.sh --dry-run` completes with no broadcast.
- [ ] Testnet deployment succeeds and the contract ID is recorded in the ticket.
- [ ] `initialize` has been run with the intended admin/oracle pair.

### Phase B — Establish the baseline on the v2 rehearsal contract

Use the contract ID of a schema-2 deployment (not the freshly initialized contract
from Phase A). If none is available, skip to the harness fallback at the end of
Phase E and mark Phases B–E as harness-only in the change ticket.

```bash
NETWORK=testnet
CONTRACT_ID="$V2_REHEARSAL_CONTRACT_ID"
stellar contract invoke --id "$CONTRACT_ID" --source "$READ_ONLY" --network $NETWORK \
  -- get_schema_version        # EXPECT: 2 — abort if this is 3
stellar contract invoke --id "$CONTRACT_ID" --source "$READ_ONLY" --network $NETWORK \
  -- get_active_round          # EXPECT: null / no active round
stellar contract invoke --id "$CONTRACT_ID" --source "$READ_ONLY" --network $NETWORK \
  -- is_paused                 # EXPECT: false
```

- [ ] Source schema version is exactly `2` (`migrate_schema_v2_to_v3` is the only
      supported source; a v1 contract must run `migrate_schema_v1_to_v2` first).
- [ ] No active round.
- [ ] Contract not paused.
- [ ] Optional: announce the **next planned** schema (a version strictly greater
      than the code's `CURRENT_SCHEMA_VERSION`, which is `3`, e.g.
      `announce_next_schema 4`), then confirm `get_next_schema` returns it.
      Announcing `3` is rejected with `UnsupportedSchemaVersion` because
      `target_version <= CURRENT_SCHEMA_VERSION`.

### Phase C — Dry-run

```bash
stellar contract invoke --id "$CONTRACT_ID" --source "$ADMIN" --network testnet \
  -- migrate_schema_v2_to_v3 --dry_run true
```

- [ ] Call returns success.
- [ ] `get_schema_version()` is **unchanged**.
- [ ] No `("schema", "migrated")` event was emitted for this transaction.
- [ ] Submission/explorer record shows no ledger state change beyond fees.

Record the transaction hash and the dry-run result in the change ticket.

### Phase D — Negative drills (still on testnet)

These confirm the guards fail closed. Use a separate throwaway v2 deployment for the
drills that mutate state (the "run twice" drill permanently migrates its target).
Alternatively run the double-run drill against the rehearsal contract *after*
Phase E.

| Drill | Setup | Expected error |
| --- | --- | --- |
| Active round blocks migration | `create_round`, then dry-run | `MigrationActiveRound` (44) — **also in dry-run** |
| Paused contract blocks migration | `pause_contract`, then dry-run | `ContractPaused` (22) |
| Wrong source version | Run `migrate_schema_v2_to_v3` twice, or run it on a v3 contract | `UnsupportedSchemaVersion` (42) |
| Non-admin caller | Invoke with a non-admin source | Authorization failure (admin `require_auth`) |
| Uninitialized contract | Invoke before `initialize` | `AdminNotSet` (2) |

- [ ] Every drill produces the expected error and no state change.
- [ ] Unpause / cancel the test round before continuing.

### Phase E — Real migration on testnet, then verify

```bash
stellar contract invoke --id "$CONTRACT_ID" --source "$ADMIN" --network testnet \
  -- migrate_schema_v2_to_v3 --dry_run false

stellar contract invoke --id "$CONTRACT_ID" --source "$READ_ONLY" --network testnet \
  -- get_schema_version        # EXPECT: 3
```

- [ ] `get_schema_version()` returns `3`.
- [ ] Event `("schema", "migrated")` observed with `(2, 3)`.
- [ ] `MigratedToV3 = true` marker confirmed (storage inspection, or covered by the
      contract's archive tests).
- [ ] Run one full round lifecycle end-to-end on testnet:
      `create_round` → `place_bet` → `resolve_round` → `claim_winnings`.
- [ ] Query `get_user_archived_participation(user, round_id)` and confirm a record is
      returned for a participant of the new round.
- [ ] Query `get_user_archive_history(user, 0, 10)` and confirm pagination works
      (`limit = 0` and `limit = 101` must fail with `PageSizeExceeded`).
- [ ] Indexer consumes `("payout", "outcome")` and `("round", "summary")` for the
      test round without errors.

If no schema-2 testnet contract is available, record that on-chain rehearsal was not
possible and cover the same guards with the Rust harness before the mainnet window:

```bash
cargo test --package xelma-contract --lib migration_versioning --locked -- --nocapture
cargo test --package xelma-contract --lib test_chaos_recovery_migrate_active_round --locked -- --nocapture
```

Only after every box above (or the documented harness fallback) is checked is the
migration considered rehearsed.

---

## 6. Mainnet execution window

### 6.1 Go/no-go

- [ ] Testnet rehearsal (Phase A–E) completed against the same artifact hash.
- [ ] Change freeze active: no round creation, no config changes scheduled inside the
      window (scheduled config changes could collide with the migration).
- [ ] Dry-run scheduled and approved by the release owner.
- [ ] Downstream consumers have acknowledged the window.

### 6.2 Sequence

1. **Freeze and confirm baseline** — run the Phase B read-only checks on mainnet.
   Abort if any value differs from the approved change record.
2. **Announce (optional)** — announce the next *planned* schema, not the version
   being applied now (`announce_next_schema 4`); confirm `get_next_schema` and the
   `("schema", "next_ann")` event. This is informational and does not gate
   anything. Announcing `3` is rejected by design (`<= CURRENT_SCHEMA_VERSION`).
3. **Dry run** — `migrate_schema_v2_to_v3 --dry_run true`. Abort on any error; do not
   retry blindly. Record the hash.
4. **Real migration** — `migrate_schema_v2_to_v3 --dry_run false`.
5. **Verify** — `get_schema_version()` must be `3`; `("schema", "migrated")` must be
   present with `(2, 3)`.
6. **Smoke round** — create and settle a small round, then confirm the archive query
   returns the participant outcome.
7. **Resume** — lift the freeze, confirm round creation resumes, and hand off to the
   indexer owner.

### 6.3 Verification queries (copy/paste)

```bash
stellar contract invoke --id "$CONTRACT_ID" --source "$READ_ONLY" --network mainnet \
  -- get_schema_version

stellar contract invoke --id "$CONTRACT_ID" --source "$READ_ONLY" --network mainnet \
  -- get_next_schema

stellar contract invoke --id "$CONTRACT_ID" --source "$READ_ONLY" --network mainnet \
  -- get_user_archived_participation --user "$USER" --round_id "$ROUND_ID"
```

---

## 7. Failure decision matrix

| Symptom | Likely cause | Action |
| --- | --- | --- |
| Dry run: `MigrationActiveRound` (44) | A round is live | Wait for settlement or cancel; do not work around the guard. |
| Dry run: `ContractPaused` (22) | Protocol paused | Resolve the incident first; migration is not a recovery tool. |
| Dry run: `UnsupportedSchemaVersion` (42) | Already migrated or wrong source version | Stop; re-read `get_schema_version()` and the change record. |
| Dry run: `AdminNotSet` (2) | Contract not initialized | Stop; deployment is incomplete. |
| Auth failure | Wrong source key | Confirm the admin key; never use a different role to force it. |
| Real migration succeeded but verification fails | RPC lag or wrong network | Re-query on the same network/RPC; check the transaction result and events before any retry. |
| Migration succeeded, smoke round fails | Post-migration regression | Pause, preserve state, escalate to the incident lead, follow the rollback path. |

**Do not** retry a failed real migration with a different argument, a different key,
or a newly rebuilt artifact. Diagnose the first failing guard, then decide.

---

## 8. Rollback and recovery

The migrations are **additive and idempotent on their guards**:

- v2 → v3 writes `SchemaVersion = 3` and `MigratedToV3 = true`, then emits the event.
  Re-running it after success returns `UnsupportedSchemaVersion` — nothing is
  re-applied.
- v1 → v2 likewise refuses to run when the source version is no longer `1`.
- There is **no downgrade entrypoint**. A schema downgrade requires deploying the
  previous artifact to a new contract and migrating state out-of-band; do not attempt
  to force `SchemaVersion` backwards.

Recovery options, in order of preference:

1. **Retry the same step** only if the transaction did not land (check the result and
   events first). The version guard makes a duplicate harmless.
2. **Pause the contract** if post-migration behaviour is wrong
   (`docs/EMERGENCY_DRILL.md`). Claims-only mode keeps user funds claimable.
3. **Redeploy the previous reviewed artifact** and re-run the smoke suite before
   re-enabling activity (`docs/DEPLOYMENT_RUNBOOK.md` §4).
4. **Escalate** to the release owner and incident lead; notify indexers and frontends
   with the current schema version and the recovery timeline.

Rollback must never delete data or rewrite archive keys; the archive is append-only
by design.

---

## 9. Post-migration handoff

- [ ] Record the final schema version, transaction hash, ledger, and artifact hash.
- [ ] Publish a short release note: schema version, new query surface
      (`get_user_archived_participation`), and the backfill expectation for pre-v3
      rounds.
- [ ] Indexer owner confirms backfill for historical rounds from
      `("payout", "outcome")` events.
- [ ] Frontend owner confirms "My History" views use the archive queries and handle
      `None` (unknown, pruned, or non-participant).
- [ ] Clear any stale announcement with `clear_next_schema` if the next target changed.
- [ ] Close the change ticket with the dry-run and real-migration transaction hashes.

---

## 10. One-page checklist

```text
PRE-FLIGHT
[ ] artifact hash matches reviewed build
[ ] admin + read-only keys confirmed
[ ] no active round, contract not paused
[ ] round creation frozen, consumers notified
[ ] rollback artifact + owner recorded

TESTNET REHEARSAL
[ ] deploy candidate to testnet
[ ] baseline reads match expectations
[ ] dry run succeeds, no writes/events
[ ] negative drills fail with expected codes
[ ] real migration succeeds, version == 3
[ ] full round + archive queries verified
[ ] indexer consumes new events cleanly

MAINNET WINDOW
[ ] go/no-go signed by release owner
[ ] baseline re-confirmed on mainnet
[ ] dry run succeeds and is recorded
[ ] real migration succeeds
[ ] version == 3 and ("schema","migrated") observed
[ ] smoke round settled and outcome query returns data
[ ] freeze lifted, handoff posted, ticket closed
```
