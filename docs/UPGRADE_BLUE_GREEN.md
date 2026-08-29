# Blue/Green Contract Migration (Issue #366)

This document is the **normative specification** for the blue/green migration
subsystem: exporting canonical economic state from a live contract (the
"blue"/vN), committing it on-chain as a Merkle root, freezing vN into a
claims-only drain mode, importing the verified state into a fresh contract
(the "green"/vN+1), and finalizing the hand-off.

The authoritative implementation lives in `contracts/src/migration.rs` with
the shared canonical encoding, and the offline witness tool lives in
`scripts/generate_migration_witness.py`. Both MUST remain byte-for-byte
compatible with this spec. A divergence between the two is a bug.

---

## 1. Motivation

An in-place schema migration (v1→v2→v3) rewrites storage in the same contract.
That works while state is small, but it keeps a long-lived, hard-to-audit
storage image and mixes protocol history with live economic state. The
blue/green path instead deploys a **clean vN+1**, moves only the **live
economic state** across (user balances, pending claims, and the contract-wide
config subset needed to keep operating), and drains the old contract.

Only the following are migrated; everything else is history, not live state:

| Migrated (live state)                  | Not migrated (history)                                  |
|----------------------------------------|---------------------------------------------------------|
| `Balance(user)` for every funded user  | round archives, outcome records, leaderboards           |
| `PendingWinnings(user)` for every claim| oracle heartbeats, attestation/quorum config            |
| The config subset (below)              | audit / governance proposal logs                        |

### Config subset

The following contract-wide parameters are part of the canonical config leaf.
They are in the documented struct order (see §3):

`protocol_fee_bps`, `fee_model`, `protocol_fee_treasury`, `bet_window_ledgers`,
`run_window_ledgers`, `close_buffer_ledgers`, `max_stake`,
`max_user_round_exposure`, `max_pending_winnings`, `min_bet`,
`min_participants`, `max_precision_participants`, `precision_payout_policy`,
`dispute_ledgers`, `early_cashout_bps`.

## 2. Security properties

1. **Authoritative on-chain reads.** Export does **not** trust the operator's
   claimed amounts. For every user the operator names, the contract re-reads
   `Balance(user)` / `PendingWinnings(user)` from its own storage. An operator
   cannot fabricate a value; they can only enumerate which addresses are
   included.
2. **Proof-verified import.** Each imported record carries a Merkle proof that
   must verify against the committed root under the canonical encoding and the
   bound `source_version`. A forged amount, a wrong address, or a cross-version
   record cannot verify.
3. **Replay protection (per record).** Each balance/pending address and the
   config are imported at most once (`ImportedBalance(addr)`,
   `ImportedPending(addr)`, `ImportedConfig` markers). Re-importing a record
   fails with `MigrationRecordAlreadyImported`.
4. **Completeness without storage iteration.** The contract cannot iterate
   keys, so it tracks an explicit `ImportedRecords` counter (and the
   `ImportedConfig` marker) incremented on every successful import. Because
   every import must verify against a committed leaf and each leaf has a
   distinct per-record guard, the counter can never exceed `leaf_count`.
   `ImportFinalized` is only reached when `ImportedRecords == leaf_count`.
5. **Freeze before migration.** Finalizing the export sets `RuntimeMode =
   ClaimsOnly` and the dedicated `Frozen` flag. State-creating entrypoints
   (`create_round`, `create_next_from_template`, betting, `mint_initial`,
   settlement that creates state, `apply_scheduled_changes`) are blocked by
   `_ensure_not_migration_frozen`, while claims and reads remain available.
   The source cannot mutate after the commitment is published.
6. **Version binding.** Every leaf preimage includes `source_version_u32le`.
   The destination binds to the expected `source_version`/`destination_version`
   pair at `import_init` and rejects any import whose version does not match.
7. **One-way, irreversible.** Once `Commitment` is written it can never be
   overwritten (`MigrationAlreadyFinalized`). Once the destination finalizes it
   can never replay (`MigrationAlreadyFinalized`).

## 3. Canonical encoding

Every leaf preimage is

```
preimage = DOMAIN_MARKER ++ u32le(source_version) ++ record
```

- `DOMAIN_MARKER = "XELMA-CPAY-V1"` (13 ASCII bytes) → domain separation.
- `u32le` = 4-byte little-endian unsigned.
- `i128le` = 16-byte little-endian two's-complement signed.
- `addr = u32le(len(addr_bytes)) ++ addr_bytes`, where `addr_bytes` is the
  StrKey ASCII (e.g. `G...`) — **not** the raw 32-byte public key — so the Rust
  (`Address::to_string()`) and Python (`str`) encodings agree exactly.
- `presence(x) = 0x00` when `x` is absent, else `0x01` followed by the encoded
  value. Optional fields are encoded with a presence byte **only** (a `None`
  value has no value bytes).

### Record tags

| tag | record                                                            |
|-----|-------------------------------------------------------------------|
| `0x00` | config leaf — `0x00 ++ config_bytes`                          |
| `0x01` | balance leaf — `0x01 ++ addr ++ i128le(amount)`               |
| `0x02` | pending leaf — `0x02 ++ addr ++ i128le(amount)`               |

`config_bytes` serializes the config struct **in field order**:

```
presence(u32 protocol_fee_bps)
++ u32le(fee_model)
++ i128le(protocol_fee_treasury)
++ u32le(bet_window_ledgers)
++ u32le(run_window_ledgers)
++ u32le(close_buffer_ledgers)
++ presence(i128 max_stake)
++ presence(i128 max_user_round_exposure)
++ presence(i128 max_pending_winnings)
++ presence(i128 min_bet)
++ presence(u32 min_participants)
++ u32le(max_precision_participants)
++ u32le(precision_payout_policy)
++ u32le(dispute_ledgers)
++ presence(u32 early_cashout_bps)
```

### Merkle tree

- **Leaf order** is deterministic and independent of storage iteration:
  config leaf first, then balance leaves sorted by their `addr` StrKey string,
  then pending leaves sorted by their `addr` string. Sort is byte-wise
  lexicographic on the ASCII StrKey.
- **Padding.** Pad to the next power of two (≥ 1) with the constant null leaf:
  `null = sha256(DOMAIN_MARKER ++ 0xFF)`.
- **Hashing.** Pair-hash bottom-up: `parent = sha256(left ++ right)`. A lone
  node at an odd top level is carried up unchanged (never occurs for a
  power-of-two-sized leaf set, but kept for completeness).
- **Empty set.** A set with no leaves commits to a single null leaf.
- **Root** is the single node remaining at the top.

## 4. Export (source / blue) flow

Admin-only; every call first passes `_require_supported_schema` and
`_ensure_not_paused`, and `dry_run` skips all writes.

1. `migration_export_start(dry_run)` — opens a session. Rejects if a
   commitment already exists or an active round exists (the snapshot must be
   stable at the exact ledger boundary).
2. `migration_export_balances(users, dry_run)` — for each user, re-reads
   `Balance(user)` (absent → `0`) and appends a canonical record. An address is
   exported at most once.
3. `migration_export_pendings(users, dry_run)` — same for `PendingWinnings`.
4. `migration_export_finalize(dry_run)` — builds the deterministic leaf set
   (config + sorted balances + sorted pendings), computes the root, stores
   `MigrationCommitment { source_version, destination_version=4, root,
   leaf_count, finalized_at_ledger }`, sets mode `ClaimsOnly`, sets the
   `Frozen` flag, and emits a `migrate:committed` event.

Reading status: `migration_get_status()` returns `frozen`, `finalized`,
`source_version`, `destination_version`, `leaf_count`, `root`.

## 5. Drain mode (post-freeze)

After `export_finalize`, the source is **claims-only**:

- **Blocked** by `_ensure_not_migration_frozen` (`MigrationFrozen`):
  `create_round`, `create_next_from_template`, `place_bet`,
  `place_precision_prediction`, `commit_prediction`, `reveal_prediction`,
  `cash_out_early`, `mint_initial`, `resolve_round`, `cancel_round`,
  `void_round`, `finalize_round`, `apply_scheduled_changes`.
- **Available:** `claim_winnings` (draining) and all read-only entrypoints.

The dedicated `Frozen` flag exists so a future `RuntimeMode` change cannot
silently re-enable mutation on a frozen source; the guard is independent of the
policy-gate matrix.

## 6. Import (destination / green) flow

Admin-only; every call passes `_require_supported_schema`.

1. `migration_import_init(expected_root, source_version, destination_version,
   leaf_count)` — binds the session to the expected commitment. `destination_version`
   must equal 4 and `0 < source_version < destination_version`. Sets
   `ExpectedCommitment` and `ImportInitialized`; rejects a second init
   (`MigrationAlreadyInitialized`).
2. `migration_import_config(cfg, proof)` — verifies the config leaf against the
   root, applies the config subset via `config::_apply_imported_config`, and
   marks `ImportedConfig`. (See §7 for what "apply" writes.)
3. `migration_import_balance(rec, proof)` — verifies the balance leaf, sets the
   destination `Balance(user)`, marks `ImportedBalance(addr)`.
4. `migration_import_pending(rec, proof)` — verifies the pending leaf, sets
   `PendingWinnings(user)` + `PendingWinningsUpdatedAt`, marks
   `ImportedPending(addr)`.
5. `migration_import_finalize()` — requires `ImportedRecords == leaf_count`
   (else `MigrationExportIncomplete`), sets `ImportFinalized`.

Each import increments `ImportedRecords`; the config contributes 1 and every
distinct balance/pending leaf contributes 1, so a correct full migration
reaches exactly `leaf_count`. A partial import cannot finalize.

## 7. Config application (`_apply_imported_config`)

Applies the canonical config subset to the destination by writing the same
storage keys the timelocked setters eventually write, but **bypassing the
timelock** because the value was already proof-verified against the source
commitment and the upgrade is gated by the source freeze. `None` optional
fields remove the corresponding key; `Some` values are range-validated
(`_validate_max_stake`, `_validate_min_bet`, protocol/fee bps bounds) exactly
as the setters do, and each write bumps TTL.

## 8. Witness tooling

`scripts/generate_migration_witness.py` computes the root and proofs for every
record from a canonical export manifest. This lets an operator generate the
`MigrationCommitment` and per-record `MerkleProof`s offline, then feed them to
the destination contract. The tool is the Python twin of `migration.rs` and
must agree byte-for-byte.

```bash
python scripts/generate_migration_witness.py \
    --source-version 3 \
    --manifest export.json \
    --output witness.json
```

Manifest shape (all fields present; absent optional fields are `null`):

```json
{
  "config": {
    "protocol_fee_bps": null, "fee_model": 0, "protocol_fee_treasury": 0,
    "bet_window_ledgers": 100, "run_window_ledgers": 150, "close_buffer_ledgers": 30,
    "max_stake": null, "max_user_round_exposure": null,
    "max_pending_winnings": null, "min_bet": null, "min_participants": null,
    "max_precision_participants": 1000, "precision_payout_policy": 0,
    "dispute_ledgers": 50, "early_cashout_bps": null
  },
  "balances": [ {"user": "G...", "amount": 1234567}, "..."],
  "pendings":  [ {"user": "G...", "amount": 7654321}, "..."]
}
```

Output `witness.json` contains `source_version`, `destination_version` (4),
`leaf_count`, `root` (hex) and a `proofs` map keyed `config`,
`balance:<addr>`, `pending:<addr>` with each proof holding `leaf_index`,
`tree_height`, and the `siblings` hex list.

## 9. Operational checklist

1. Deploy vN+1 and call `initialize` (sets admin/oracle/schema).
2. On vN: `migration_export_start(false)`; tail calls can use `dry_run=true`
   first.
3. On vN: batch `migration_export_balances` / `migration_export_pendings` for
   every funded/claiming user.
4. On vN: `migration_export_finalize(false)` → contract is frozen. Capture the
   commitment via `migration_get_status()`.
5. Build the canonical manifest from the exported records + the vN config, and
   run the witness tool to regenerate the root. **Assert it equals the on-chain
   root** before any import.
6. On vN+1: `migration_import_init(root, source_version, 4, leaf_count)`.
7. On vN+1: `migration_import_config`, then every balance/pending proof.
8. On vN+1: `migration_import_finalize()` (fails if any record is missing).
9. Smoke-test vN+1 (create round, bet, resolve, claim) and drain vN.

## 10. Compatibility with existing migrations

The blue/green flow is orthogonal to the in-place schema migrations
(`migrate_schema_v1_to_v2`, `migrate_schema_v2_to_v3`). They share
`_require_supported_schema` and the `SchemaVersion` guard, but the blue/green
system uses only the dedicated `MigrationKey` storage namespace, so it never
consumes `DataKeyCore` XDR union budget and never collides with business state.
The destination version constant is `MIGRATION_DESTINATION_VERSION = 4`.
