# Indexer Guide: Archived `UserRoundOutcome` (Schema v3)

**Issues:** [#884](https://github.com/TevaLabs/Xelma-Blockchain/issues/884)
**Audience:** indexers, analytics pipelines, wallets, and UI clients that need per-user
historical participation data without replaying the full event stream.
**Related:** [`docs/archive_queries_guide.md`](./archive_queries_guide.md) (archive + pagination overview),
[`docs/EVENT_SCHEMA.md`](./EVENT_SCHEMA.md) (canonical event schema),
[`docs/storage_lifecycle.md`](./storage_lifecycle.md) (TTL/rent policy),
[`MIGRATION.md`](../MIGRATION.md) (schema v2 → v3 migration).

---

## 1. What a `UserRoundOutcome` record is

Starting with **schema v3**, the contract persists one compact record per
`(round_id, user)` pair whenever a round settles, is cancelled, or a user exits
their position early. The record answers a single question cheaply:

> "What did this user stake, what did they predict, and what did they get out of
> round `round_id`?"

Before schema v3 an indexer had to reconstruct this from `bet`/`reveal`/`payout`/`refund`
events (or from the deprecated full participant maps). The archived record is the
canonical, queryable source going forward. It is written by
`_persist_user_outcome` in [`contracts/src/settlement.rs`](../contracts/src/settlement.rs)
and exposed through
`get_user_archived_participation` in [`contracts/src/queries.rs`](../contracts/src/queries.rs).

Records are **additive only**: schema v3 does not remove or reinterpret any existing
field, so old data and old clients keep working.

---

## 2. Record schema

The Rust definition lives at `contracts/src/types.rs`:

```rust
#[contracttype]
pub struct UserRoundOutcome {
    pub user: Address,
    pub round_mode: u32,
    pub prediction_side: u32,
    pub predicted_price: u128,
    pub stake: i128,
    pub payout: i128,
    pub outcome: UserOutcomeType,
}
```

| Field | XDR type | Units / meaning |
| --- | --- | --- |
| `user` | `Address` | The participant's Stellar account (strkey `G...`). Also part of the storage key, so it always matches the queried user. |
| `round_mode` | `u32` | `0` = UpDown, `1` = Precision. |
| `prediction_side` | `u32` | `0` = Up, `1` = Down, `2` = Precision. |
| `predicted_price` | `u128` | Guess scaled to **4 decimals** (e.g. `0.2297` → `2297`). Only meaningful for Precision; `0` for UpDown and for early cash-outs. |
| `stake` | `i128` | Amount the user put at risk, in **stroops** (7 decimals). For early cash-outs this is the original stake. |
| `payout` | `i128` | Amount credited to the user's pending winnings for this round, in stroops. `0` on a loss or a fully-forfeited cash-out. |
| `outcome` | `UserOutcomeType` | Terminal classification, encoded as `u32` (see below). |

### 2.1 `outcome` values (`UserOutcomeType`)

| Value | Variant | Meaning | Typical `payout` |
| --- | --- | --- | --- |
| `0` | `Win` | Prediction correct; user received a share of the pot. | `> 0` |
| `1` | `Loss` | Prediction incorrect. | `0` |
| `2` | `Refund` | Tie, one-sided round, under-threshold fallback, **or early cash-out**. | `stake` (or `cashout` for early exit) |
| `3` | `Cancel` | Round cancelled by admin/oracle. | `stake` |
| `4` | `Void` | Prediction voided. | varies |

> **Indexer tip:** because early cash-outs are recorded as `Refund`, always read
> `stake` and `payout` together instead of assuming `payout == stake` whenever
> `outcome == 2`. For an early exit `payout = stake − forfeit`, and the forfeit is
> reported by the `("cashout", "early")` event.

### 2.2 `round_mode` / `prediction_side` quick decode

| `round_mode` | Mode | Valid `prediction_side` |
| --- | --- | --- |
| `0` | UpDown | `0` (Up), `1` (Down) |
| `1` | Precision | `2` (Precision) |

---

## 3. When records are created

A record is written exactly once per `(round_id, user)`, at settlement or at an early
exit. The write is **first-write-wins** and idempotent: `_persist_user_outcome` returns early if the
key already exists, so re-processing a settlement (or a retried transaction) cannot
duplicate or overwrite an outcome.

| Terminal transition | Outcome written |
| --- | --- |
| Competitive resolution (UpDown) | `Win` or `Loss` per participant |
| Competitive resolution (Precision) | `Win` / `Loss` for revealed predictions |
| Tie / one-sided round refund | `Refund` |
| Min-participants fallback refund | `Refund` |
| Admin/oracle cancellation | `Cancel` |
| Early cash-out during Running phase | `Refund` (with `payout = cashout`) |

Every write also emits the compensating event
`("payout", "outcome")` with payload
`(round_id: u64, round_mode: u32, user: Address, payout: i128, outcome: u32)`.
That event is the recommended backfill source for rounds settled **before** schema v3.

Early cash-outs additionally emit
`("cashout", "early")` with
`(user: Address, round_id: u64, side: u32, stake: i128, cashout: i128, forfeit: i128)`.
Join the two on `(round_id, user)` to recover the exact forfeit.

---

## 4. Storage layout

All keys are persistent and TTL-extended on write/read
(see [`docs/storage_lifecycle.md`](./storage_lifecycle.md)).

| Key | Value | Purpose |
| --- | --- | --- |
| `DataKey::UserRoundOutcome(round_id, user)` | `UserRoundOutcome` | The per-user record documented here. |
| `DataKey::ArchivedRound(round_id)` | `ArchivedRoundSummary` | Presence gates participation queries; removing it hides the round. |
| `DataKey::UserArchivedRoundIds(user)` | `Vec<u64>` | Append-only index used by paginated history. |
| `DataKey::RecentArchivedRoundIds` | `Vec<u64>` | Newest-last global index, trimmed to the retention limit. |

`contracts/src/types.rs::DataKey` is the source of truth for these variants. Note
that the generated TypeScript `DataKey` union only lists keys clients need to
construct; purely internal keys (such as `UserRoundOutcome`) may be absent from the
TS union. Use the query entrypoints rather than constructing storage keys client-side.

---

## 5. Query API

The four read entrypoints below are **read-only** and safe to call from indexers,
dashboards, and wallets. All are also callable in simulation via
`stellar contract invoke` (they return values without state changes).

| Entrypoint | Arguments | Returns | Errors |
| --- | --- | --- | --- |
| `get_user_archived_participation` | `user: Address, round_id: u64` | `Option<UserRoundOutcome>` | none (returns `None` for missing data) |
| `get_user_archive_history` | `user: Address, offset: u32, limit: u32` | `Vec<ArchivedRoundSummary>` | `PageSizeExceeded` when `limit == 0` or `limit > 100` |
| `get_recent_archived_rounds` | `limit: u32` | `Vec<ArchivedRoundSummary>` (newest first) | none |
| `get_archived_round` | `round_id: u64` | `Option<ArchivedRoundSummary>` | none |

### 5.1 `None` semantics for `get_user_archived_participation`

The contract deliberately returns `None` in all of these cases:

| Situation | Result |
| --- | --- |
| `round_id` was never archived | `None` |
| `round_id` was archived but has since been pruned by retention | `None` — **even if the `UserRoundOutcome` key still exists** |
| Round exists but this user did not participate | `None` |
| Round was settled before schema v3 and no record was backfilled | `None` |

This "pruned round ⇒ `None`" rule keeps missing-id behaviour consistent across
`get_archived_round` and `get_user_archived_participation`, so clients can treat any
`None` as "not available on-chain" and fall back to events.

### 5.2 Pagination contract

`get_user_archive_history` is newest-first with a **zero-based offset**:

- `limit` must be `1..=100` (`MAX_PAGE_SIZE = 100`); outside that range the call fails
  with `PageSizeExceeded` (do not confuse with an empty page).
- `offset = 0, limit = 10` → 10 most recent rounds.
- `offset = 10, limit = 10` → the next 10.
- If `offset >= total`, the call returns an **empty vector**, not an error.
- Rounds that were pruned are skipped; pages may therefore contain fewer than `limit`
  summaries while still advancing the offset. Keep paging until an empty page.

`get_recent_archived_rounds` clamps `limit` to the configured retention
(`min(limit, ArchiveRetention)`), and `limit = 0` returns an empty vector.

---

## 6. Retention: what survives pruning

`ArchiveRetention` defaults to **128** rounds (`DEFAULT_ARCHIVE_RETENTION` in
`contracts/src/common.rs`) and is configurable by the admin through
`set_archive_retention` / `get_archive_retention`.

On every archive write the contract trims `RecentArchivedRoundIds` back to the
retention limit and, for each evicted round:

1. removes `ArchivedRound(round_id)`,
2. removes the `CancelledRound(round_id)` marker if present,
3. emits `("archive", "pruned")` with `(round_id: u64, retention_limit: u32)`.

`UserRoundOutcome` records and `UserArchivedRoundIds` entries are **not** deleted by
this trim. They become unreachable through the query API because the round summary is
gone — which is why `get_user_archived_participation` checks
`ArchivedRound(round_id)` first. If you need permanent history, persist outcomes in
your own store while the round is still retained.

Monitoring recommendation: alert on `("archive", "pruned")` and keep your indexer's
committed watermark ahead of the oldest retained round.

---

## 7. Usage examples

### 7.1 Stellar CLI

```bash
# Single user outcome for a specific round
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source "$READ_ONLY_ACCOUNT" \
  --network testnet \
  -- get_user_archived_participation \
  --user "$USER_ADDRESS" \
  --round_id 42

# Page 1 of a user's history (newest first)
stellar contract invoke \
  --id "$CONTRACT_ID" --source "$READ_ONLY_ACCOUNT" --network testnet \
  -- get_user_archive_history \
  --user "$USER_ADDRESS" --offset 0 --limit 10

# Latest archived round summaries (clamped by retention)
stellar contract invoke \
  --id "$CONTRACT_ID" --source "$READ_ONLY_ACCOUNT" --network testnet \
  -- get_recent_archived_rounds --limit 5
```

### 7.2 TypeScript (`@xelma/bindings`)

```ts
import { Client, Networks } from "@xelma/bindings";

const client = new Client({
  contractId: CONTRACT_ID,
  networkPassphrase: Networks.TESTNET,
  rpcUrl: RPC_URL,
});

// A single archived outcome. `result` is null when unavailable on-chain.
const { result: outcome } = await client.get_user_archived_participation({
  user: USER_ADDRESS,
  round_id: 42n,
});

if (outcome) {
  const won = outcome.outcome === 0;
  const exitedEarly = outcome.outcome === 2 && outcome.payout < outcome.stake;
  console.log({
    roundId: 42n,
    mode: outcome.round_mode === 0 ? "UpDown" : "Precision",
    side: ["Up", "Down", "Precision"][outcome.prediction_side],
    stake: outcome.stake,
    payout: outcome.payout,
    won,
    exitedEarly,
  });
}

// Newest-first history page. Throws on limit outside 1..=100.
const { result: history } = await client.get_user_archive_history({
  user: USER_ADDRESS,
  offset: 0,
  limit: 10,
});
```

### 7.3 Handling `None` plus backfill

```ts
const archived = await client.get_user_archived_participation({
  user: USER_ADDRESS,
  round_id: roundId,
});

if (!archived.result) {
  // Not on-chain (unknown, pruned, non-participant, or pre-v3).
  // Fall back to your event store, keyed on (round_id, user).
  return readOutcomeFromEventStore(roundId, USER_ADDRESS);
}
```

For pre-v3 rounds, backfill from `("payout", "outcome")` events and join the round
metadata from `("round", "summary")`. Deduplicate on `(round_id, user, tx_hash)` and
prefer the on-chain record when both sources exist.

---

## 8. Reconciling outcomes with claims

`payout` is credited to the user's **pending winnings**, not pushed to their account.
The aggregate is exposed by `get_pending_winnings(user)`, and the user withdraws it
with `claim_winnings(user)`.

To reconcile a round, compare three independent sources and require them to agree
on the per-user payout:

1. the `UserRoundOutcome.payout` value,
2. the amount the round's settlement credited to pending winnings,
3. the `(round_id, user)` entry in the `("payout", "outcome")` event stream.

The exact distributable-pot formula differs by mode (UpDown pools vs Precision pot)
and by whether early cash-outs occurred; the conservation tests in
`contracts/src/tests/conservation.rs` pin those invariants. The per-user record tells
you *what* a user earned in a round; the claim state is aggregated and decoupled, so
never infer "already claimed" from `UserRoundOutcome` alone.

---

## 9. Migration and backfill notes

- Schema v3 is **additive**. Existing keys keep their meaning; new
  `UserRoundOutcome` records are written from the first settlement after migration.
- Rounds settled before the migration have **no** records. Backfill them from
  `("payout", "outcome")` events and mark the source in your store (for example
  `source: "event" | "archive"`).
- The migration is dry-runnable and gated on "no active round". See
  [`docs/OPERATOR_MIGRATION_PLAYBOOK.md`](./OPERATOR_MIGRATION_PLAYBOOK.md) for the
  operator procedure and [`MIGRATION.md`](../MIGRATION.md) for the schema diff.
- `announce_next_schema` / `get_next_schema` let operators advertise a future target
  version; indexers may surface it but must not treat it as the active schema.

---

## 10. Verification

| Check | Command / location |
| --- | --- |
| Record write + query behaviour | `cargo test --package xelma-contract --lib archive_participation --locked` |
| Archive retention behaviour | `cargo test --package xelma-contract --lib archive_retention --locked` |
| Bindings ABI parity | `cd bindings && npm run test:parity && npm run test:wasm` |
| Live schema version | `stellar contract invoke ... -- get_schema_version` → `3` |

If `get_user_archived_participation` returns `None` for a round you know was settled
after the migration, check in this order:

1. `get_schema_version()` — confirm the contract is on v3.
2. `get_archived_round(round_id)` — confirm the summary has not been pruned.
3. `get_user_archive_history(user, 0, 10)` — confirm the user is indexed.
4. Your event store — confirm a `("payout", "outcome")` event exists for the pair.
