# Indexer Guide: Archived UserRoundOutcome Queries (Schema v3)

> **Addresses:** GitHub issue #570 — Observability: indexer guide for archived
> UserRoundOutcome queries.

## Table of Contents

1. [Overview](#1-overview)
2. [Storage Architecture](#2-storage-architecture)
3. [Type Reference](#3-type-reference)
   - 3.1 [UserRoundOutcome](#31-userroundoutcome)
   - 3.2 [ArchivedRoundSummary](#32-archivedroundsummary)
   - 3.3 [RoundArchiveStatus](#33-roundarchivestatus)
   - 3.4 [UserOutcomeType](#34-useroutcometype)
   - 3.5 [RoundMode](#35-roundmode)
4. [Query Functions](#4-query-functions)
   - 4.1 [get_user_archived_participation](#41-get_user_archived_participation)
   - 4.2 [get_user_archive_history](#42-get_user_archive_history)
   - 4.3 [get_archived_round](#43-get_archived_round)
   - 4.4 [get_recent_archived_rounds](#44-get_recent_archived_rounds)
5. [Consistency Invariants](#5-consistency-invariants)
6. [Pagination Semantics](#6-pagination-semantics)
7. [Example Query Sequences](#7-example-query-sequences)
   - 7.1 [Complete Lifecycle: Bet → Resolve → Query](#71-complete-lifecycle-bet--resolve--query)
   - 7.2 [Pruned Round Scenario](#72-pruned-round-scenario)
   - 7.3 [Multi-round History with Pagination](#73-multi-round-history-with-pagination)
   - 7.4 [Cancelled Round Scenario](#74-cancelled-round-scenario)
8. [Indexer Integration Guide](#8-indexer-integration-guide)
   - 8.1 [Recommended Polling Patterns](#81-recommended-polling-patterns)
   - 8.2 [Handling None Returns](#82-handling-none-returns)
   - 8.3 [Off-chain Indexer Storage Schema](#83-off-chain-indexer-storage-schema)
9. [Performance Notes](#9-performance-notes)
10. [Common Pitfalls and FAQs](#10-common-pitfalls-and-faqs)
11. [Appendix: Storage Key Reference](#11-appendix-storage-key-reference)

---

## 1. Overview

The **archive participation system** (schema v3) provides a durable, on-chain
record of every user's outcome for every completed round. When a round is
finalized — via normal oracle settlement, admin cancellation, fallback refund,
or dispute-window void — the contract writes:

1. One `ArchivedRoundSummary` record keyed by `round_id`.
2. One `UserRoundOutcome` record per participating address, keyed by
   `(round_id, user)`.
3. An updated per-user index (`UserArchivedRoundIds`) that maintains the ordered
   list of every round the user has participated in.
4. A global recent-round index (`RecentArchivedRoundIds`) used by
   `get_recent_archived_rounds`.

These records are stored in **persistent storage** with a configurable
retention window (default 128 rounds, see §9). Once a round falls outside the
retention window its `ArchivedRound` key is pruned and both archive-query
functions return `None` / skip that round_id — providing a safe, consistent
"record not found" signal to off-chain consumers.

This guide explains the four public archive query functions, their semantics,
edge cases, and recommended patterns for building a reliable off-chain indexer
on top of them.

---

## 2. Storage Architecture

### 2.1 Key Taxonomy

The contract uses two enumerated storage key families:

| Key family      | Enum variant                               | Description                                          |
|-----------------|--------------------------------------------|------------------------------------------------------|
| `DataKeyScoped` | `ArchivedRound(round_id: u64)`             | Per-round summary record                             |
| `DataKeyScoped` | `UserRoundOutcome(round_id: u64, user: Address)` | Per-user outcome record for one archived round |
| `DataKeyScoped` | `UserArchivedRoundIds(user: Address)`      | Ordered `Vec<u64>` of round IDs for one user         |
| `DataKeyCore`   | `RecentArchivedRoundIds`                   | Global `Vec<u64>` of recently archived round IDs     |
| `DataKeyCore`   | `ArchiveRetention`                         | Configurable max retained rounds (default 128)       |

### 2.2 Write Path (Settlement)

Every settlement path writes the archive records:

```
finalize_round()
  └─ write ArchivedRound(round_id)            → ArchivedRoundSummary
  └─ for each participant:
       write UserRoundOutcome(round_id, user) → UserRoundOutcome
       append round_id → UserArchivedRoundIds(user)
  └─ append round_id → RecentArchivedRoundIds (oldest pruned once
       ArchiveRetention is exceeded)
```

The same pattern applies for `cancel_round`, `fallback_refund`, and
`void_round` — they all produce `ArchivedRoundSummary` records tagged with the
appropriate `RoundArchiveStatus`.

### 2.3 Pruning

When the `RecentArchivedRoundIds` list would exceed the configured retention
limit, the oldest entry is removed and its `ArchivedRound(round_id)` key is
deleted from storage. **Individual `UserRoundOutcome` records are not eagerly
deleted** — only the `ArchivedRound` key is removed. The query functions use
the presence of the `ArchivedRound` key as the authoritative "is this round
still retained?" gate (see §5).

### 2.4 Storage Tier

All archive records live in **persistent storage**. Unlike temporary storage,
persistent entries survive beyond a single ledger and are retained as long as
their TTL is maintained. Callers extending TTL on-demand is not required for
archive records — the contract's settlement path sets appropriate TTLs.

---

## 3. Type Reference

### 3.1 UserRoundOutcome

Defined in `contracts/src/types.rs`:

```rust
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct UserRoundOutcome {
    /// The participant's address.
    pub user: Address,

    /// The round's game mode, encoded as a u32 discriminant.
    /// 0 = UpDown, 1 = Precision. See RoundMode.
    pub round_mode: u32,

    /// The user's predicted direction or side, encoded as a u32 discriminant.
    /// For UpDown: 0 = Up, 1 = Down.
    /// For Precision: encodes the prediction bracket; use predicted_price for
    /// the exact value.
    pub prediction_side: u32,

    /// The user's exact price prediction (Precision mode).
    /// For UpDown mode this field is 0.
    /// Stored as a fixed-point u128 (7 decimal places; divide by 1_000_000_0
    /// to get the human-readable price).
    pub predicted_price: u128,

    /// The amount staked in this round, in contract token units (i128).
    /// Negative values must not occur in practice; any negative reading
    /// indicates a storage corruption and should be flagged by the indexer.
    pub stake: i128,

    /// The amount paid out to this user, in contract token units (i128).
    /// For a loss this is 0. For a refund/cancel/void this equals stake.
    /// For a win this is stake + winnings (after protocol fee deduction).
    pub payout: i128,

    /// The result classification for this user in this round.
    /// See UserOutcomeType.
    pub outcome: UserOutcomeType,
}
```

**Field notes:**

- `round_mode` is the raw u32 discriminant. Compare against the `RoundMode`
  enum constants: `0 = UpDown`, `1 = Precision`.
- `prediction_side` meaning depends on `round_mode`. In UpDown mode `0 = Up`,
  `1 = Down`. In Precision mode it is not used for the primary prediction value
  — use `predicted_price` instead.
- `predicted_price` uses a fixed-point representation with 7 decimal places
  (the same encoding as oracle prices). Divide by `10_000_000` to obtain the
  human-readable price.
- `stake` and `payout` are both `i128`. The contract token's smallest unit is
  the stroope (1 XLM = 10_000_000 stropes). Always store these as 64-bit
  integers in your indexer database; do not cast to float without explicit
  precision handling.

### 3.2 ArchivedRoundSummary

Defined in `contracts/src/types.rs`:

```rust
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ArchivedRoundSummary {
    /// The unique monotonically increasing round identifier.
    pub round_id: u64,

    /// The oracle price at round start (fixed-point, 7 decimal places).
    pub price_start: u128,

    /// The oracle price at round settlement (fixed-point, 7 decimal places).
    /// For cancelled/voided rounds this is the price at the time of
    /// cancellation or void; it may be 0 if no oracle price was ever recorded.
    pub price_final: u128,

    /// The game mode for this round. See RoundMode.
    pub mode: RoundMode,

    /// How this round ended. See RoundArchiveStatus.
    pub status: RoundArchiveStatus,

    /// Total stake on the Up side at settlement, in contract token units.
    pub pool_up: i128,

    /// Total stake on the Down side at settlement, in contract token units.
    /// For Precision rounds, pool_down is 0 (all stake tracked in pool_up).
    pub pool_down: i128,

    /// Total number of distinct participants in this round.
    pub participant_count: u32,

    /// The ledger sequence number at which this round was settled/archived.
    pub settled_at_ledger: u32,
}
```

**Field notes:**

- `price_start` and `price_final` use the same 7-decimal fixed-point encoding
  as `UserRoundOutcome.predicted_price`.
- In Precision mode, `pool_up` holds the total precision staking pool and
  `pool_down` is always `0`.
- `settled_at_ledger` is the Stellar ledger sequence number, not a Unix
  timestamp. To convert to a wall-clock time, use the Horizon API's ledger
  endpoint or compute from a known ledger/time anchor plus the ~5-second
  per-ledger cadence.

### 3.3 RoundArchiveStatus

Defined in `contracts/src/types.rs`:

```rust
pub enum RoundArchiveStatus {
    /// Oracle settlement completed (normal resolution path).
    Resolved = 0,
    /// Admin cancelled the round and refunded participants.
    Cancelled = 1,
    /// Settlement aborted due to insufficient participants; stakes refunded.
    FallbackRefund = 2,
    /// Dispute window ended via void; all participants refunded their stake.
    Voided = 3,
}
```

| Discriminant | Name            | Trigger                                  | User outcome    |
|:---:|:---|:---|:---|
| 0 | `Resolved`      | Normal oracle settlement                 | Win / Loss       |
| 1 | `Cancelled`     | Admin cancels the round                  | Cancel (refund)  |
| 2 | `FallbackRefund`| Insufficient participants at settlement  | Refund           |
| 3 | `Voided`        | Dispute window void path                 | Void (refund)    |

All non-`Resolved` statuses produce a full stake refund for every participant.

### 3.4 UserOutcomeType

Defined in `contracts/src/types.rs`:

```rust
pub enum UserOutcomeType {
    Win    = 0,
    Loss   = 1,
    Refund = 2,
    Cancel = 3,
    Void   = 4,
}
```

| Discriminant | Name     | `payout` value                      | When set                                         |
|:---:|:---|:---|:---|
| 0 | `Win`    | `stake + net_winnings`              | Correct prediction in a `Resolved` round         |
| 1 | `Loss`   | `0`                                 | Incorrect prediction in a `Resolved` round       |
| 2 | `Refund` | `stake`                             | `FallbackRefund` settlement                      |
| 3 | `Cancel` | `stake`                             | `Cancelled` round                                |
| 4 | `Void`   | `stake`                             | `Voided` round                                   |

For `Refund`, `Cancel`, and `Void` outcomes, `payout == stake` always holds.
Indexers should validate this invariant when ingesting records.

### 3.5 RoundMode

Defined in `contracts/src/types.rs`:

```rust
pub enum RoundMode {
    UpDown    = 0,  // Simple up/down predictions
    Precision = 1,  // Exact price predictions (Legends mode)
}
```

The `round_mode` field in `UserRoundOutcome` stores the raw discriminant (`0`
or `1`), not the enum variant. Use the discriminant values above when comparing
in query results or off-chain logic.

---

## 4. Query Functions

All four functions are pure read-only queries (no auth required, no state
mutation, no fee). They are exposed as contract methods accessible via the
Soroban SDK, Stellar Horizon simulations, or the TypeScript bindings package.

### 4.1 `get_user_archived_participation`

**Signature:**

```rust
pub fn get_user_archived_participation(
    env: Env,
    user: Address,
    round_id: u64,
) -> Option<UserRoundOutcome>
```

**Parameters:**

| Parameter  | Type      | Description                                    |
|:-----------|:----------|:-----------------------------------------------|
| `env`      | `Env`     | Soroban execution environment (auto-injected)  |
| `user`     | `Address` | The participant address to query               |
| `round_id` | `u64`     | The archived round identifier                  |

**Returns:** `Option<UserRoundOutcome>`

- `Some(outcome)` — the round exists in the archive *and* the user participated
  in it.
- `None` — either:
  1. The `ArchivedRound(round_id)` key is absent (round does not exist or was
     pruned), **or**
  2. The user did not participate in the specified round.

**Semantics:**

This function performs a two-step lookup:

1. Check for the presence of `DataKeyScoped::ArchivedRound(round_id)` in
   persistent storage. If absent, immediately return `None`.
2. If the round key exists, look up
   `DataKeyScoped::UserRoundOutcome(round_id, user)` and return the result.

**Critical invariant:** Step 1 acts as a pruning-safety gate. Even if a
`UserRoundOutcome` record physically remains in storage (because individual
user records are not eagerly deleted during pruning), this function returns
`None` once the round's `ArchivedRound` key has been removed. This provides
consistent "not found" semantics for all archive queries regardless of internal
storage state. See §5 for full details.

**Usage example (TypeScript bindings):**

```typescript
const outcome = await client.getUserArchivedParticipation({
  user: "GXXXXXX...",
  round_id: 42n,
});

if (outcome === null) {
  console.log("Round 42 is not in the archive or user did not participate");
} else {
  console.log(`User outcome: ${outcome.outcome}, payout: ${outcome.payout}`);
}
```

**Usage example (Stellar CLI):**

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <KEYPAIR> \
  --network testnet \
  -- get_user_archived_participation \
  --user GXXXXXX... \
  --round_id 42
```

---

### 4.2 `get_user_archive_history`

**Signature:**

```rust
pub fn get_user_archive_history(
    env: Env,
    user: Address,
    offset: u32,
    limit: u32,
) -> Vec<ArchivedRoundSummary>
```

**Parameters:**

| Parameter | Type      | Description                                                      |
|:----------|:----------|:-----------------------------------------------------------------|
| `env`     | `Env`     | Soroban execution environment (auto-injected)                    |
| `user`    | `Address` | The participant address whose history to retrieve                |
| `offset`  | `u32`     | Zero-based page offset (0 = most recent page)                    |
| `limit`   | `u32`     | Maximum records per page; silently clamped to `MAX_PAGE_SIZE` (100) |

**Returns:** `Vec<ArchivedRoundSummary>`

A vector of archived round summaries in **newest-first** order (most recently
archived round appears first in the returned vector). The vector may be shorter
than `limit` if fewer records exist at the requested offset, or if some round
IDs in the user's index have been pruned.

**Semantics:**

1. Fetches the user's `UserArchivedRoundIds` index (`Vec<u64>`) from persistent
   storage. If absent, returns an empty vector.
2. Clamps `limit` to `MAX_PAGE_SIZE` (100).
3. Calculates the slice of round IDs corresponding to the requested
   `offset`/`limit` window, traversing the index vector from **newest to
   oldest** (index is appended in chronological order, so newest = highest
   index position).
4. For each round ID in the window, attempts to load
   `DataKeyScoped::ArchivedRound(round_id)`. If the key is missing (pruned),
   the round is silently **skipped** rather than producing an error or a null
   entry.
5. Returns the collected summaries.

**Important:** Because pruned rounds are silently skipped, a page response may
contain fewer items than `limit` even when there are more rounds beyond the
`offset`. See §6 for recommended pagination strategies.

---

### 4.3 `get_archived_round`

**Signature:**

```rust
pub fn get_archived_round(env: Env, round_id: u64) -> Option<ArchivedRoundSummary>
```

**Parameters:**

| Parameter  | Type  | Description                                  |
|:-----------|:------|:---------------------------------------------|
| `env`      | `Env` | Soroban execution environment (auto-injected) |
| `round_id` | `u64` | The archived round identifier                |

**Returns:** `Option<ArchivedRoundSummary>`

- `Some(summary)` — the round is currently retained in the archive.
- `None` — the round does not exist or has been pruned beyond the retention
  window.

**Semantics:**

A direct key lookup against `DataKeyScoped::ArchivedRound(round_id)`. This is
the cheapest way to check whether a specific round is still retained and to
retrieve its aggregate summary without needing any particular user address.

Use this function when you already know the `round_id` and only need the
round-level aggregate data (prices, pool sizes, participant count, status), not
per-user outcomes.

---

### 4.4 `get_recent_archived_rounds`

**Signature:**

```rust
pub fn get_recent_archived_rounds(env: Env, limit: u32) -> Vec<ArchivedRoundSummary>
```

**Parameters:**

| Parameter | Type  | Description                                                          |
|:----------|:------|:---------------------------------------------------------------------|
| `env`     | `Env` | Soroban execution environment (auto-injected)                         |
| `limit`   | `u32` | Maximum number of recent rounds to return                            |

**Returns:** `Vec<ArchivedRoundSummary>`

A vector of the most recently archived round summaries, in **newest-first**
order (most recent round first). The vector contains at most
`min(limit, ArchiveRetention)` entries.

**Semantics:**

1. Fetches the global `RecentArchivedRoundIds` index from persistent storage.
   If absent, returns an empty vector.
2. Reads the configured `ArchiveRetention` limit (default 128; set by admin via
   governance). Caps `limit` to this value.
3. Traverses the global index from newest to oldest, loading each
   `ArchivedRound(round_id)` record. Records for missing keys are silently
   skipped (consistent with pruning-safe semantics).
4. Returns the collected summaries.

**Use case:** This is the recommended starting point for an indexer bootstrap
— call `get_recent_archived_rounds` with the full retention limit to seed your
local database with all on-chain retained rounds, then switch to event-driven
polling (see §8).

---

## 5. Consistency Invariants

### 5.1 Pruning-Safe Gate

The most important invariant in the archive system is:

> **`get_user_archived_participation` returns `None` whenever
> `DataKeyScoped::ArchivedRound(round_id)` is absent in storage — even if the
> `UserRoundOutcome` record still physically exists.** (It also returns `None`
> for a retained round when the user did not participate; see §5.3 for how to
> distinguish the two cases.)

This is a deliberate design choice. When a round is pruned, only the
`ArchivedRound` key is deleted. The per-user `UserRoundOutcome` records may
linger in storage until their TTL expires. Without the gate, a query for a
pruned round might return stale data, while `get_archived_round` for the same
round would return `None` — an inconsistency that would confuse indexers.

The gate ensures: for any given `round_id`, either **all** archive queries
return data or **none** of them do.

### 5.2 Round Existence Check

Before calling `get_user_archived_participation` for a set of users, you can
use `get_archived_round` as a cheap existence check. If `get_archived_round`
returns `None`, all per-user queries for that round will also return `None`.

```
round present? = get_archived_round(round_id) != None
if round_present:
    for user in users:
        outcome = get_user_archived_participation(user, round_id)
```

### 5.3 Participation vs. Presence

`get_user_archived_participation` returning `None` for an existing round
(where `get_archived_round` returns `Some`) means the user **did not
participate** in that round. This is a distinct semantic from round pruning.
Indexers must differentiate between these two cases:

| `get_archived_round(r)` | `get_user_archived_participation(u, r)` | Interpretation              |
|:-----------------------:|:---------------------------------------:|:----------------------------|
| `None`                  | `None`                                  | Round pruned or never existed |
| `Some(_)`               | `None`                                  | User did not participate     |
| `Some(_)`               | `Some(outcome)`                         | User participated; outcome available |

---

## 6. Pagination Semantics

### 6.1 Offset/Limit Model

`get_user_archive_history` uses a zero-based offset/limit model:

- `offset=0, limit=10` → most recent 10 rounds
- `offset=10, limit=10` → rounds 11–20 (second page)
- `offset=20, limit=10` → rounds 21–30 (third page)

### 6.2 Maximum Page Size

`limit` is silently clamped to `MAX_PAGE_SIZE = 100`. Passing any value greater
than 100 is equivalent to passing 100. Do not rely on receiving more than 100
records per call.

### 6.3 Newest-First Ordering

The index is maintained in **append order** (oldest entry at index 0, newest at
the highest index). The query functions traverse it in reverse, so the returned
vector always presents **newest to oldest** (index 0 = most recently archived
round).

### 6.4 Sparse Pages Due to Pruning

Because pruned round IDs are silently skipped during page construction, a page
may contain fewer items than `limit` even when the user has participated in more
rounds beyond the current `offset`. This means:

- Do NOT use `result.length < limit` as a signal that you have reached the
  last page.
- Instead, check whether `offset + result.length >= total_known_rounds`, where
  `total_known_rounds` is tracked locally by your indexer.
- Alternatively, keep incrementing `offset` by `limit` until you receive an
  empty vector.

### 6.5 Archive Retention Boundary

A user's `UserArchivedRoundIds` index can contain round IDs that have been
pruned from the `ArchivedRound` key space. These IDs will silently produce no
results. The number of active (non-pruned) rounds visible per user is bounded
by `ArchiveRetention` (default 128). Very active users may have thousands of
entries in their `UserArchivedRoundIds` index, but only the most recent 128
will have associated `ArchivedRound` keys.

---

## 7. Example Query Sequences

### 7.1 Complete Lifecycle: Bet → Resolve → Query

This example traces the complete path from user bet placement through oracle
resolution, archive write, and subsequent read queries.

**Scenario:** Alice bets Up on round 17 (UpDown mode). Bob bets Down. The oracle
settles with a price that went up; Alice wins, Bob loses.

**Step 1 — Bet placement (during BetWindow):**

```
alice: place_bet(amount=50_0000000, side=Up)    → positions recorded
bob:   place_bet(amount=50_0000000, side=Down)  → positions recorded
```

**Step 2 — Oracle resolution (after RunWindow expires):**

```
oracle: resolve_round(price=2_0000000, nonce=1)
  → settlement writes:
       ArchivedRound(17) = {
           round_id: 17,
           price_start: 1_0000000,
           price_final: 2_0000000,
           mode: UpDown,
           status: Resolved,
           pool_up: 50_0000000,
           pool_down: 50_0000000,
           participant_count: 2,
           settled_at_ledger: 1024,
       }
       UserRoundOutcome(17, alice) = {
           user: alice,
           round_mode: 0,        // UpDown
           prediction_side: 0,   // Up
           predicted_price: 0,   // unused in UpDown mode
           stake: 50_0000000,
           payout: 97_5000000,   // ~95% of pool (5% protocol fee)
           outcome: Win,
       }
       UserRoundOutcome(17, bob) = {
           user: bob,
           round_mode: 0,
           prediction_side: 1,   // Down
           predicted_price: 0,
           stake: 50_0000000,
           payout: 0,
           outcome: Loss,
       }
```

**Step 3 — Query Alice's outcome by round:**

```
get_user_archived_participation(alice, round_id=17)
  → checks ArchivedRound(17) → present ✓
  → loads UserRoundOutcome(17, alice)
  → returns Some(UserRoundOutcome { outcome: Win, payout: 97_5000000, ... })
```

**Step 4 — Query Alice's history (newest first):**

```
get_user_archive_history(alice, offset=0, limit=10)
  → fetches UserArchivedRoundIds(alice) = [17]
  → loads ArchivedRound(17)
  → returns Vec[ArchivedRoundSummary { round_id: 17, status: Resolved, ... }]
```

**Step 5 — Query aggregate round summary:**

```
get_archived_round(round_id=17)
  → returns Some(ArchivedRoundSummary {
       round_id: 17,
       status: Resolved,
       participant_count: 2,
       pool_up: 50_0000000,
       pool_down: 50_0000000,
       ...
    })
```

**Step 6 — Query recent archived rounds:**

```
get_recent_archived_rounds(limit=5)
  → returns Vec[ArchivedRoundSummary { round_id: 17, ... }, ...]
     (round 17 appears first if it is the most recently archived)
```

---

### 7.2 Pruned Round Scenario

**Scenario:** The archive retention limit is configured to 10. Rounds 1–10 were
archived; when round 11 was archived the limit was exceeded, so the oldest
archived round(s) — starting with round 1 — were pruned. An indexer queries
round 1 for Alice.

**State after round 11 is archived:**

```
ArchivedRound(1)   → DELETED (pruned)
ArchivedRound(2…)  → retained (at most `ArchiveRetention` summaries remain)
UserRoundOutcome(1, alice) → may still physically exist (TTL not yet expired)
UserArchivedRoundIds(alice) = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
```

**Query attempt for pruned round:**

```
get_archived_round(round_id=1)
  → storage.has(ArchivedRound(1)) → false
  → returns None

get_user_archived_participation(alice, round_id=1)
  → storage.has(ArchivedRound(1)) → false  ← pruning-safety gate
  → returns None  (even though UserRoundOutcome(1, alice) may exist in storage)
```

**Indexer guidance:** The `None` return for a previously valid round_id
indicates pruning, not an error. The indexer should mark the local record as
`pruned` (not delete it — it should retain its own historical copy) and not
retry the query.

**History pagination with pruned round:**

```
get_user_archive_history(alice, offset=0, limit=5)
  → rounds window from index 10 down to 6: [11, 10, 9, 8, 7]
  → all have ArchivedRound keys present
  → returns Vec[summary_11, summary_10, summary_9, summary_8, summary_7]

get_user_archive_history(alice, offset=10, limit=5)
  → rounds window from index 0 down: only [1] remains
  → ArchivedRound(1) is pruned → skipped
  → returns Vec[]  ← sparse page; no error
```

---

### 7.3 Multi-round History with Pagination

**Scenario:** Carol has participated in 25 consecutive rounds (round_ids 100–124,
all resolved). Retention limit is 128 so none are pruned. Indexer needs to
page through all of them.

**Page 1 (most recent 10):**

```
get_user_archive_history(carol, offset=0, limit=10)
  → returns rounds [124, 123, 122, 121, 120, 119, 118, 117, 116, 115]
  → 10 items returned
```

**Page 2:**

```
get_user_archive_history(carol, offset=10, limit=10)
  → returns rounds [114, 113, 112, 111, 110, 109, 108, 107, 106, 105]
  → 10 items returned
```

**Page 3 (last page, only 5 remaining):**

```
get_user_archive_history(carol, offset=20, limit=10)
  → returns rounds [104, 103, 102, 101, 100]
  → 5 items returned (< limit) — this signals the final page
```

**Page 4 (beyond end):**

```
get_user_archive_history(carol, offset=30, limit=10)
  → offset(30) >= total(25) → returns Vec[]  ← empty vector signals end
```

**Termination condition in pseudocode:**

```python
def fetch_all_history(user):
    records = []
    offset = 0
    PAGE_SIZE = 10
    while True:
        page = get_user_archive_history(user, offset=offset, limit=PAGE_SIZE)
        if len(page) == 0:
            break  # empty page = exhausted
        records.extend(page)
        if len(page) < PAGE_SIZE:
            break  # partial page = last page (when no pruning)
        offset += PAGE_SIZE
    return records
```

> **Note:** The above termination condition assumes no pruning. In production
> with an active pruning window, continue even on partial pages — see §6.4 and
> the robust version in §8.1.

---

### 7.4 Cancelled Round Scenario

**Scenario:** Dave placed a bet in round 55. Before the round ended, the admin
cancelled it. Dave should receive a full refund.

**Settlement path:**

```
admin: cancel_round(reason_code=1)
  → writes:
       ArchivedRound(55) = {
           round_id: 55,
           price_start: 1_5000000,
           price_final: 0,         // no oracle price at cancellation
           mode: UpDown,
           status: Cancelled,      // discriminant = 1
           pool_up: 100_0000000,
           pool_down: 0,
           participant_count: 1,
           settled_at_ledger: 2000,
       }
       UserRoundOutcome(55, dave) = {
           user: dave,
           round_mode: 0,
           prediction_side: 0,     // Up
           predicted_price: 0,
           stake: 100_0000000,
           payout: 100_0000000,    // full refund (payout == stake)
           outcome: Cancel,        // discriminant = 3
       }
```

**Query:**

```
get_user_archived_participation(dave, round_id=55)
  → returns Some(UserRoundOutcome {
       outcome: Cancel,     // 3
       stake: 100_0000000,
       payout: 100_0000000, // payout == stake ← validate this invariant
       ...
    })

get_archived_round(round_id=55)
  → returns Some(ArchivedRoundSummary {
       status: Cancelled,    // 1
       price_final: 0,       // no final oracle price
       participant_count: 1,
       ...
    })
```

**Indexer handling:** Detect `outcome == Cancel` (or check `status == Cancelled`
on the summary) and mark the user's participation record as refunded. Verify the
`payout == stake` invariant.

The same pattern applies to `FallbackRefund` (outcome=`Refund`, status=
`FallbackRefund`) and `Voided` (outcome=`Void`, status=`Voided`).

---

## 8. Indexer Integration Guide

### 8.1 Recommended Polling Patterns

#### Bootstrap (Initial Sync)

1. Call `get_recent_archived_rounds(limit=128)` to retrieve all on-chain
   retained rounds.
2. For each round in the response, call `get_archived_round(round_id)` to
   confirm freshness (optional, since you already have the summary), then page
   through participants if needed.
3. For each known user address, call `get_user_archive_history(user, 0, 100)`
   and paginate until empty.
4. For each `(user, round_id)` pair discovered in step 3, call
   `get_user_archived_participation(user, round_id)` to retrieve the individual
   outcome record.
5. Store everything locally. Record the highest `settled_at_ledger` seen as
   your sync cursor.

#### Ongoing Polling

Poll for new rounds using one of two strategies:

**Strategy A — Event-driven (recommended):**

Subscribe to the contract's settlement event topic (e.g., `round_archived`,
`round_resolved`). When an event fires, extract `round_id` from the event data
and call `get_archived_round(round_id)` + per-user `get_user_archived_participation`.

**Strategy B — Ledger-based polling:**

```python
POLL_INTERVAL_SECONDS = 30  # ~6 ledgers

def poll_loop(last_known_ledger):
    while True:
        recent = get_recent_archived_rounds(limit=128)
        for summary in recent:
            if summary.settled_at_ledger > last_known_ledger:
                ingest_round(summary)
                last_known_ledger = summary.settled_at_ledger
        sleep(POLL_INTERVAL_SECONDS)

def ingest_round(summary):
    # Store the aggregate summary
    db.upsert_round_summary(summary)
    # Fetch per-user outcomes
    participants = get_known_participants(summary.round_id)
    for user in participants:
        outcome = get_user_archived_participation(user, summary.round_id)
        if outcome is not None:
            db.upsert_user_outcome(outcome)
```

**Robust paginator (handles sparse pages from pruning):**

```python
def fetch_all_user_history(user, known_total_rounds=None):
    records = []
    offset = 0
    PAGE_SIZE = 100
    prev_len = -1

    while True:
        page = get_user_archive_history(user, offset=offset, limit=PAGE_SIZE)
        records.extend(page)
        page_len = len(page)

        # Empty page always means end
        if page_len == 0:
            break

        # If we have a known total, stop when we've covered enough offsets
        if known_total_rounds and (offset + PAGE_SIZE) >= known_total_rounds:
            break

        # Guard: if two consecutive pages have the same non-full length,
        # we've likely hit the end (sparse page termination)
        if page_len == prev_len and page_len < PAGE_SIZE:
            break

        prev_len = page_len
        offset += PAGE_SIZE

    return records
```

### 8.2 Handling None Returns

Always treat `None` returns as one of two distinct cases:

| Situation | Recommended action |
|:----------|:-------------------|
| `get_archived_round(r)` returns `None` for a `round_id` you have a prior record for | Mark local record as `pruned=true`. Do not delete. Do not retry. |
| `get_archived_round(r)` returns `None` for a `round_id` you have never seen | The round does not exist yet, or `round_id` is invalid. Log and skip. |
| `get_user_archived_participation(u, r)` returns `None` and `get_archived_round(r)` returns `Some` | User did not participate in that round. Store a `no_participation` marker if needed for completeness. |
| `get_user_archived_participation(u, r)` returns `None` and `get_archived_round(r)` returns `None` | Round is pruned or never existed. Do not retry for this `round_id`. |

**Never** treat `None` as a transient error that should be retried immediately.
Persistent storage is deterministic — if the key is missing, it is missing.
Retrying against the same ledger height will always return `None`.

If you believe a round should be present but is returning `None`, check:
1. Has the `ArchiveRetention` limit been reduced by a governance action?
2. Is the `round_id` actually correct (off-by-one in your tracker)?
3. Is the node you are querying behind the current ledger?

### 8.3 Off-chain Indexer Storage Schema

Recommended relational schema for a PostgreSQL-based indexer:

```sql
-- Aggregate per-round data
CREATE TABLE archived_rounds (
    round_id             BIGINT PRIMARY KEY,
    price_start          NUMERIC(38, 7) NOT NULL,   -- fixed-point, 7 decimals
    price_final          NUMERIC(38, 7) NOT NULL,
    mode                 SMALLINT NOT NULL,          -- 0=UpDown, 1=Precision
    status               SMALLINT NOT NULL,          -- 0=Resolved,1=Cancelled,2=FallbackRefund,3=Voided
    pool_up              BIGINT NOT NULL,            -- i128 → use NUMERIC for large values
    pool_down            BIGINT NOT NULL,
    participant_count    INT NOT NULL,
    settled_at_ledger    INT NOT NULL,
    pruned               BOOLEAN NOT NULL DEFAULT false,
    indexed_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Per-user outcome records
CREATE TABLE user_round_outcomes (
    round_id             BIGINT NOT NULL REFERENCES archived_rounds(round_id),
    user_address         VARCHAR(64) NOT NULL,
    round_mode           SMALLINT NOT NULL,
    prediction_side      SMALLINT NOT NULL,
    predicted_price      NUMERIC(38, 7) NOT NULL,
    stake                BIGINT NOT NULL,
    payout               BIGINT NOT NULL,
    outcome              SMALLINT NOT NULL,          -- 0=Win,1=Loss,2=Refund,3=Cancel,4=Void
    indexed_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (round_id, user_address)
);

-- Indexes for common access patterns
CREATE INDEX idx_user_outcomes_address ON user_round_outcomes(user_address);
CREATE INDEX idx_user_outcomes_address_round
    ON user_round_outcomes(user_address, round_id DESC);
CREATE INDEX idx_archived_rounds_ledger
    ON archived_rounds(settled_at_ledger DESC);
CREATE INDEX idx_archived_rounds_status
    ON archived_rounds(status);
```

**Notes on numeric types:**

- `stake` and `payout` are `i128` on-chain. BIGINT (64-bit) is sufficient for
  practical values but if you expect extremely large pools, use `NUMERIC(39, 0)`.
- `price_start`, `price_final`, and `predicted_price` are `u128` fixed-point
  with 7 decimal places. Store as `NUMERIC(38, 7)` for exact representation.
  Do not convert to FLOAT — floating-point precision loss is unacceptable for
  financial records.

---

## 9. Performance Notes

### 9.1 Archive Retention Limit

The maximum number of `ArchivedRound` records retained on-chain is controlled
by `DataKeyCore::ArchiveRetention`, which defaults to:

```
DEFAULT_ARCHIVE_RETENTION = 128
```

This constant is defined in `contracts/src/common.rs`. The admin can adjust
this value via governance. A higher retention limit keeps more history on-chain
but increases storage costs (each persistent entry incurs a rent fee on
Stellar).

**Implication for indexers:** Do not rely on the on-chain archive for
long-term history beyond 128 rounds. Build a local archive. The on-chain
archive is designed for short-term observability and dispute resolution, not
permanent history storage.

### 9.2 MAX_PAGE_SIZE

```
MAX_PAGE_SIZE = 100
```

Defined in `contracts/src/common.rs`. This limit applies to
`get_user_archive_history` (the `limit` parameter): requests for more than 100
items are silently clamped. `get_recent_archived_rounds` is instead capped only
by the configured `ArchiveRetention` (see §4.4 and §9.1). To retrieve more than
100 records of a user's history, use pagination (§6).

### 9.3 Query Cost

All four archive query functions are read-only and perform only persistent
storage lookups. On Stellar/Soroban, read-only simulation does not consume
network fees. However, each persistent storage `.get()` has a compute cost in
CPU instructions. For large pages (100 entries), the compute cost is bounded
but non-trivial. Avoid calling these in tight loops without rate limiting.

### 9.4 UserArchivedRoundIds Growth

A highly active user who has participated in thousands of rounds will have a
large `UserArchivedRoundIds` vector. Reading and deserializing this vector is
the most expensive part of `get_user_archive_history`. The query must load
the entire vector to correctly compute the offset/limit slice. For users with
very long histories, consider:

- Caching the vector length off-chain to avoid repeatedly computing page
  boundaries.
- Batching calls rather than making per-user calls in tight loops.

---

## 10. Common Pitfalls and FAQs

### Q1: Why does `get_user_archived_participation` return `None` when I can see the `UserRoundOutcome` key in raw storage?

**A:** The function checks for the presence of `DataKeyScoped::ArchivedRound(round_id)`
before reading the user record. If the round's summary key has been pruned (even
if the per-user record still physically exists), the function returns `None`.
This is the pruning-safety gate (§5.1). Your raw storage inspection found the
stale user record — it has not been actively deleted but it is no longer
considered valid.

### Q2: I passed `limit=200` to `get_user_archive_history` but got at most 100 results.

**A:** `limit` is silently clamped to `MAX_PAGE_SIZE = 100`. Use `offset` to
paginate through larger result sets. See §6.

### Q3: I'm getting fewer results than `limit` even though the user has participated in many rounds.

**A:** Some rounds in the user's `UserArchivedRoundIds` index may have been
pruned. Pruned rounds are silently skipped, producing sparse pages. Do not
treat `result.length < limit` as a termination condition. See §6.4.

### Q4: What does `price_final = 0` mean on an `ArchivedRoundSummary`?

**A:** A `price_final` of `0` indicates the round was cancelled or voided
before a final oracle price was recorded. Check `status`:
- `Cancelled` (1) — admin cancelled; no oracle price.
- `Voided` (3) — dispute-window void; may or may not have a price.
- `FallbackRefund` (2) — fallback; may have a partial settlement price.

For `Resolved` rounds, `price_final` will always be non-zero (the oracle
settlement price).

### Q5: `payout` on a `Win` outcome seems lower than expected.

**A:** The `payout` value is the **net payout after protocol fee deduction**.
To compute the gross payout, you need to know the `protocol_fee_bps` at the
time of settlement. The protocol fee is subtracted from the winnings pool
before distribution. For a loss, `payout = 0`. For a win in an UpDown round:

```
gross_winnings = user_stake * (pool_down / pool_up)
fee = gross_winnings * protocol_fee_bps / 10000
net_winnings = gross_winnings - fee
payout = stake + net_winnings
```

The exact settlement math is in `contracts/src/settlement_math.rs`.

### Q6: Should I store `UserRoundOutcome.round_mode` as an enum or integer?

**A:** Store as an integer (SMALLINT) and decode to a human-readable enum only
at the presentation layer. The on-chain representation is a raw u32
discriminant. Future schema versions may add new modes; storing integers makes
forward compatibility easier.

### Q7: Can I use `get_user_archive_history` as a leaderboard substitute?

**A:** No. `get_user_archive_history` returns `ArchivedRoundSummary` records
(round-level aggregates), not per-user outcome details. To get per-user stats
for leaderboard purposes, use `get_user_stats` for aggregate win/loss counts,
or use `get_user_archived_participation` to retrieve individual outcomes and
aggregate them off-chain.

### Q8: What happens if I call `get_recent_archived_rounds` with `limit=0`?

**A:** The function immediately returns an empty vector. This is defined
behavior. Always pass `limit >= 1`.

### Q9: How do I determine whether a round was a Precision or UpDown round from archived data?

**A:** Check the `mode` field on `ArchivedRoundSummary`:
- `RoundMode::UpDown = 0`
- `RoundMode::Precision = 1`

The same information is available as `round_mode` (u32) in `UserRoundOutcome`.

### Q10: Is the order of results from `get_user_archive_history` guaranteed?

**A:** Yes. The results are always in **newest-first order** (most recently
archived round first). This ordering is a contractual guarantee of the API, not
an implementation detail. Indexers may rely on this for incremental sync: if
the first element of the response matches your local most-recent record, you
are up to date.

---

## 11. Appendix: Storage Key Reference

Complete listing of archive-related storage keys:

| Key                                          | Type                    | Description                                               |
|:---------------------------------------------|:------------------------|:----------------------------------------------------------|
| `DataKeyScoped::ArchivedRound(round_id)`     | `ArchivedRoundSummary`  | Per-round aggregate record; absent if pruned             |
| `DataKeyScoped::UserRoundOutcome(round_id, user)` | `UserRoundOutcome` | Per-user outcome for one round; may persist after pruning |
| `DataKeyScoped::UserArchivedRoundIds(user)`  | `Vec<u64>`              | Ordered list of all round IDs a user participated in      |
| `DataKeyCore::RecentArchivedRoundIds`        | `Vec<u64>`              | Global list of recent round IDs (newest last)             |
| `DataKeyCore::ArchiveRetention`              | `u32`                   | Max retained rounds; default `128`                        |

**Constants:**

| Constant                    | Value | Location                              |
|:----------------------------|:-----:|:--------------------------------------|
| `MAX_PAGE_SIZE`             | `100` | `contracts/src/common.rs` line 34     |
| `DEFAULT_ARCHIVE_RETENTION` | `128` | `contracts/src/common.rs` line 89     |

**Related source files:**

| File                                              | Relevance                                          |
|:--------------------------------------------------|:---------------------------------------------------|
| `contracts/src/types.rs`                          | Struct/enum definitions for all archive types      |
| `contracts/src/queries.rs`                        | Implementation of all four query functions         |
| `contracts/src/storage.rs`                        | Storage key helpers and round cleanup              |
| `contracts/src/settlement.rs`                     | Archive write path (all settlement variants)       |
| `contracts/src/common.rs`                         | Constants (`MAX_PAGE_SIZE`, `DEFAULT_ARCHIVE_RETENTION`) |
| `contracts/src/tests/archive_participation.rs`    | Integration tests for archive queries              |
| `contracts/src/tests/archive_retention.rs`        | Tests for pruning and retention limit behavior     |

---

*This document is part of the Xelma-Blockchain schema v3 documentation.
For related topics see [`STORAGE_DESIGN.md`](../STORAGE_DESIGN.md),
[`ROUND_LIFECYCLE.md`](../ROUND_LIFECYCLE.md), and
[`docs/storage_lifecycle.md`](storage_lifecycle.md).*
