# Operator Playbook: Archive Retention & Expired Pending Winnings

> **Issue #571.** Commands, expected events and failure modes for the two
> storage-hygiene jobs an operator runs: trimming the on-chain round archive
> and reclaiming pending winnings nobody has claimed.
> Every behaviour below is checked by
> [`contracts/src/tests/ops_archive_reclaim.rs`](../contracts/src/tests/ops_archive_reclaim.rs).

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

## 1. Archive retention and pruning

### How it works

- Every resolved, cancelled, fallback-refunded or voided round writes an
  `ArchivedRoundSummary` and appends its id to a FIFO index
  (`RecentArchivedRoundIds`).
- **Pruning is automatic.** There is no separate prune entrypoint. On each
  archive write, while the index holds more than `archive_retention` ids, the
  oldest id is removed together with its `ArchivedRound(id)` entry and its
  `CancelledRound(id)` marker, and `("archive", "pruned")` is emitted.
- Lowering the retention deletes nothing immediately. The **next** archive
  write prunes the whole backlog in that one transaction (one `pruned` event
  per round). Budget that transaction for the extra deletes. When lowering
  from a very large value, step down in stages (e.g. 1000 → 500 → 250),
  letting one round settle between steps.
- `get_user_archived_participation` returns `None` for a pruned round, but
  the per-user `UserRoundOutcome` entries are **not** deleted. They keep
  paying rent until their TTL lapses. Retention bounds the round index, not
  every per-user record.

### Commands

| Goal                           | Command                                               |
|--------------------------------|-------------------------------------------------------|
| Read current retention         | `view get_archive_retention`                          |
| Change retention (immediate)   | `invoke set_archive_retention --limit 256`            |
| Inspect newest archived rounds | `view get_recent_archived_rounds --limit 20`          |
| Look up one archived round     | `view get_archived_round --round_id 42`               |
| Check a round after pruning    | `view get_round_status --round_id 42` → `Unknown` (0) |
| Check a user's archived outcome | `view get_user_archived_participation --user <G...> --round_id 42` |

Default retention is `128`; valid range is `1..=10000`.

### Events to watch

| Event                        | Payload                          | Meaning                                  |
|------------------------------|----------------------------------|------------------------------------------|
| `("archive", "retention")`   | `(limit)`                        | Retention changed.                       |
| `("config", "updated")`      | `(ArchiveRetention, old, new)`   | Same change, generic config stream.      |
| `("archive", "pruned")`      | `(round_id, retention_limit)`    | One archived round deleted from storage. |

Indexers must capture `("round", "summary")` **before** a round is pruned.
After pruning, the chain can no longer answer questions about that round.

### Failure modes

| Symptom                                   | Cause                                                        | Action                                                                  |
|-------------------------------------------|--------------------------------------------------------------|-------------------------------------------------------------------------|
| `#23 WindowOutOfRange`                    | `limit` is `0` or `> 10000`.                                 | Choose a value in range.                                                |
| `#22 ContractPaused`                      | Runtime mode is `FullyPaused` (2).                           | Wait, or move to `ClaimsOnly` (1); the setter works in `Normal` and `ClaimsOnly`. |
| `#2 AdminNotSet` / auth failure           | Contract not initialised or wrong `--source`.                | Sign with the admin key.                                                |
| Retention lowered but nothing was deleted | Expected. Pruning runs on the next archive write.            | Settle or cancel a round, then check for `pruned` events.               |
| Settlement tx near budget after lowering  | Backlog pruned in one write.                                 | Lower retention in steps.                                               |
| `get_round_status` returns `Unknown` for an old round | Round pruned; its cancel marker is pruned with it. | Use the indexer (`round/summary`) for history.                          |
| `get_recent_archived_rounds` returns fewer than `limit` | Capped at `archive_retention`.                  | Expected.                                                               |

---

## 2. Reclaiming expired pending winnings

### How it works

- Every credit to a user's pending balance (win, refund, insurance coverage)
  records the ledger of that credit. **Any new credit resets the timer for the
  user's whole pending balance.** A user who keeps receiving credits never
  expires.
- `claim_winnings` clears both the balance and the timer.
- With expiry set to `E` ledgers (default `0` = disabled), the admin may call
  `reclaim_expired_pending_winnings(user)` once `current_ledger - last_credit
  >= E`. The **entire** pending balance moves to the **admin's vXLM balance**
  (not the fee treasury), and `("claim", "expired")` is emitted.
- Reclaim is per user, and the last-credit ledger has no getter. Build the
  candidate list off-chain from the indexer: users with a positive
  `get_pending_winnings` whose latest settled or cancelled round
  (`("round", "summary")` → `settled_at_ledger`) is older than `E` ledgers.
  Simulate first, because the contract is the source of truth (see `#86` below).

### Enabling or changing the expiry (timelocked)

Expiry changes go through the config timelock (`CONFIG_TIMELOCK_LEDGERS` =
1440 ledgers, about 2 h at 5 s per ledger).

```bash
# 1. Schedule (admin). Valid: 0 (disable) or 128..=1_000_000 ledgers.
invoke schedule_pending_winnings_expiry --ledgers 518400      # ~30 days
# 2. Inspect: note activation_ledger
view get_pending_config_change --kind PendingWinningsExpiry
# 3. After activation_ledger, apply (any account may submit; needs Normal mode)
invoke apply_scheduled_changes --kind PendingWinningsExpiry
view get_pending_winnings_expiry
# Abort before activation (admin)
invoke cancel_config_change --kind PendingWinningsExpiry
```

`set_pending_winnings_expiry` is an alias of `schedule_pending_winnings_expiry`.
It does **not** apply immediately.

### Reclaiming

```bash
view get_pending_winnings --user <G...>           # amount at stake
invoke reclaim_expired_pending_winnings --user <G...>
```

Each call handles one user and returns the amount reclaimed. Before submitting,
simulate with `--send=no` and skip users that return `#86`.

### Events to watch

| Event                        | Payload                          | Meaning                                  |
|------------------------------|----------------------------------|------------------------------------------|
| `("config", "sched")`        | `(PendingWinningsExpiry, activation_ledger)` | Change scheduled.            |
| `("pending", "expiry")`      | `(ledgers)`                      | New expiry is live.                      |
| `("config", "applied")`      | `(PendingWinningsExpiry, activation_ledger)` | Same, generic config stream. |
| `("claim", "expired")`       | `(user, amount, admin)`          | Pending balance reclaimed to admin.      |

### Failure modes

| Symptom                              | Cause                                                            | Action                                                              |
|--------------------------------------|------------------------------------------------------------------|---------------------------------------------------------------------|
| `#78 ExpiryNotConfigured`            | Expiry is `0` (default) or the scheduled change is not applied yet. | Schedule, wait for activation, then apply.                          |
| `#77 PendingWinningsNotFound`        | User has nothing pending (already claimed or reclaimed).         | Drop from the candidate list.                                       |
| `#86 PendingWinningsNotExpired`      | Last credit is younger than the expiry. A recent credit resets the timer. | Retry after `last_credit + expiry`.                        |
| `#22 ContractPaused` on reclaim      | Runtime mode `FullyPaused` (2). Reclaim **is** allowed in `ClaimsOnly`. | Wait for de-escalation.                                        |
| `#22 ContractPaused` on apply        | `apply_scheduled_changes` requires `Normal` (0).                 | Apply after returning to `Normal`.                                  |
| `#16 RoundNotEnded` on apply         | `activation_ledger` not reached.                                 | Wait.                                                               |
| `#45 CommitmentNotFound` on apply/cancel | No change of that kind is scheduled.                         | Check `get_pending_config_change`.                                  |
| `#20 RoundAlreadyActive` on schedule | A change of that kind is already pending.                        | Cancel it first or wait for it to apply.                            |
| `#27 RoundNotCancellable` on cancel  | Activation ledger already reached.                               | Apply it, then schedule a new value.                                |
| `#13 InvalidDuration` on schedule    | `ledgers` outside `128..=1_000_000` and not `0`.                 | Fix the value.                                                      |
| `#25 PayoutOverflow`                 | Admin balance would overflow `i128`.                             | Move admin funds, then retry.                                       |
| `#42 UnsupportedSchemaVersion`       | Storage schema newer than the code.                              | Upgrade the contract before running ops.                            |

### Safety notes

- Reclaim is custodial: users lose the claim on-chain. Publish the expiry
  period, and give notice before enabling it or shortening it.
- Reclaimed value stays inside the protocol (admin balance). Conservation
  holds; nothing is burned.
- Disabling the expiry (`0`) stops future reclaims. Earlier reclaims are not reversed.

---

## 3. Linked entrypoints

| Entrypoint                                | Public wrapper                                                         | Implementation                                                             |
|-------------------------------------------|------------------------------------------------------------------------|----------------------------------------------------------------------------|
| `set_archive_retention`                   | [`contract.rs:899`](../contracts/src/contract.rs#L899)                 | [`config.rs:637`](../contracts/src/config.rs#L637)                         |
| `get_archive_retention`                   | [`contract.rs:903`](../contracts/src/contract.rs#L903)                 | [`config.rs`](../contracts/src/config.rs)                                  |
| Automatic FIFO prune                      | —                                                                      | [`settlement.rs` `_archive_round`](../contracts/src/settlement.rs#L2348)   |
| `get_archived_round`                      | [`contract.rs:1164`](../contracts/src/contract.rs#L1164)               | [`queries.rs`](../contracts/src/queries.rs)                                |
| `get_recent_archived_rounds`              | [`contract.rs:1168`](../contracts/src/contract.rs#L1168)               | [`queries.rs`](../contracts/src/queries.rs)                                |
| `get_user_archived_participation`         | [`contract.rs:1172`](../contracts/src/contract.rs#L1172)               | [`queries.rs`](../contracts/src/queries.rs)                                |
| `get_round_status`                        | [`contract.rs:332`](../contracts/src/contract.rs#L332)                 | —                                                                          |
| `schedule_pending_winnings_expiry`        | [`contract.rs:911`](../contracts/src/contract.rs#L911)                 | [`config.rs`](../contracts/src/config.rs)                                  |
| `set_pending_winnings_expiry` (alias)     | [`contract.rs:907`](../contracts/src/contract.rs#L907)                 | [`config.rs`](../contracts/src/config.rs)                                  |
| `get_pending_winnings_expiry`             | [`contract.rs:915`](../contracts/src/contract.rs#L915)                 | [`config.rs`](../contracts/src/config.rs)                                  |
| `get_pending_config_change`               | [`contract.rs:840`](../contracts/src/contract.rs#L840)                 | [`config.rs`](../contracts/src/config.rs)                                  |
| `apply_scheduled_changes`                 | [`contract.rs:847`](../contracts/src/contract.rs#L847)                 | [`config.rs`](../contracts/src/config.rs)                                  |
| `cancel_config_change`                    | [`contract.rs:851`](../contracts/src/contract.rs#L851)                 | [`config.rs`](../contracts/src/config.rs)                                  |
| `reclaim_expired_pending_winnings`        | [`contract.rs:919`](../contracts/src/contract.rs#L919)                 | [`admin.rs:1211`](../contracts/src/admin.rs#L1211)                         |
| `get_pending_winnings`                    | [`contract.rs:1184`](../contracts/src/contract.rs#L1184)               | [`queries.rs`](../contracts/src/queries.rs)                                |
| `claim_winnings`                          | [`contract.rs:1082`](../contracts/src/contract.rs#L1082)               | [`settlement.rs`](../contracts/src/settlement.rs)                          |
| Pending credit + timer (`_accumulate_pending`) | —                                                                 | [`common.rs:143`](../contracts/src/common.rs#L143)                         |
| `set_runtime_mode`                        | [`contract.rs:124`](../contracts/src/contract.rs#L124)                 | [`admin.rs`](../contracts/src/admin.rs)                                    |

Related: [`STATUS_CODES.md`](STATUS_CODES.md) (runtime modes),
[`archive_queries_guide.md`](archive_queries_guide.md),
[`storage_lifecycle.md`](storage_lifecycle.md),
[`EVENT_SCHEMA.md`](EVENT_SCHEMA.md),
[`DEPLOYMENT_RUNBOOK.md`](DEPLOYMENT_RUNBOOK.md).
