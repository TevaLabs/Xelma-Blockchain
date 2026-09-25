# Storage Design

## Summary

The Xelma contract was rebuilt around an **indexed per-user key layout**. The
old design stored every participant's position in a single `Map<Address, T>`
blob under one key, forcing every read/write to deserialise and reserialise
the whole map. The new design stores each user's record under a composite
`(round_id, address)` key — O(1) read/write per user, regardless of round size.

## Key Layout

## Typed key taxonomy

All contract storage keys use one of the following `DataKeyCore` variants. The
taxonomy is part of the XDR contract schema and must be updated before adding a
new storage slot:

| Type | Scope | Examples | Storage class |
|---|---|---|---|
| `DataKeyCore` | singleton protocol, configuration, and schema state | `Admin`, `ActiveRound`, `OracleTimestampSkew`, `EpochMintBudget` | persistent or instance, as documented by the call site |
| `DataKeyScoped` | keys carrying an address, round, ledger, or other runtime scope | `Balance(Address)`, `PendingWinnings(Address)`, `Position(round_id, user)` | persistent or temporary, as documented by the call site |
| `DataKeyCore::Ext(DataKeyExt)` | overflow bucket for additional singleton or compound keys | leaderboard and governance metadata | persistent |

`DataKey` is not a supported storage key. New code must not introduce raw
`Symbol` keys or another monolithic enum. Event topics and temporary internal
counters may use symbols only when they are not contract state addresses.

The canonical position layout is indexed per user and round. The legacy bulk
map keys (`Positions`, `UpDownPositions`, and `PrecisionPositions`) are not
written by normal operations and are not removed as part of round cleanup;
schema migrations own any legacy reads or deletes. This prevents a caller from
observing two competing sources of truth.

| Key | Value | Purpose |
|---|---|---|---|
| `Balance(Address)` | `i128` | per-user balance (unchanged) |
| `PendingWinnings(Address)` | `i128` | per-user pending payout (unchanged) |
| `PendingWinningsUpdatedAt(Address)` | `u32` | **NEW** — ledger seq of last modify for expiry |
| `PendingWinningsExpiry` | `u32` | **NEW** — configurable expiry threshold (0 = off) |
| `UserStats(Address)` | `UserStats` | per-user wins/losses (unchanged) |
| `ActiveRound` | `Round` | currently active round metadata |
| `LastRoundId` | `u64` | monotonic round counter |
| **`Position(round_id, user)`** | `UserPosition` | **NEW** — indexed UpDown bet |
| **`PrecisionPosition(round_id, user)`** | `PrecisionPrediction` | **NEW** — indexed precision guess |
| **`RoundParticipants(round_id)`** | `Vec<Address>` | **NEW** — ordered list for resolution iteration |
| `CancelledRound(round_id)` | `bool` | marker for cancelled rounds (cleaned up on prune) |
| **`ArchivedRound(round_id)`** | `ArchivedRoundSummary` | **NEW** — compact post-settlement history |
| **`RecentArchivedRoundIds`** | `Vec<u64>` | **NEW** — FIFO index for archive retention |
| **`UserRoundOutcome(round_id, user)`** | `UserRoundOutcome` | **NEW** — per-user outcome at settlement |

## Archived Round Summaries

After every terminal round transition (`resolve_round`, admin `cancel_round`, or
minimum-participant fallback refund), the contract writes a compact
`ArchivedRoundSummary` keyed by `DataKeyScoped::ArchivedRound(round_id)`.

Each summary records:

| Field | Meaning |
|---|---|
| `round_id` | Monotonic round identifier |
| `price_start` / `price_final` | Start oracle price and settlement price (`0` when cancelled) |
| `mode` | Up/Down or Precision |
| `status` | `Resolved`, `Cancelled`, or `FallbackRefund` |
| `pool_up` / `pool_down` | Final pool totals at settlement time |
| `participant_count` | Participants recorded at settlement |
| `settled_at_ledger` | Ledger sequence when archived |

### Query API

| Query | Returns | Notes |
|---|---|---|
| `get_archived_round(round_id)` | `Option<ArchivedRoundSummary>` | Returns `None` if pruned or never existed. |
| `get_recent_archived_rounds(limit)` | `Vec<ArchivedRoundSummary>` | Newest-first list; `limit = 0` returns empty. Capped by retention limit. |
| `get_user_archived_participation(user, round_id)` | `Option<UserRoundOutcome>` | Returns `None` if the round was pruned (consistent missing-id semantics). |
| `get_round_status(round_id)` | `RoundStatus` | Returns `Unknown` for pruned/non-existent rounds; `Cancelled` if a `CancelledRound` marker exists (even if archive pruned). |

### Retention bounds

The admin may configure the archive retention limit within **`[MIN_ARCHIVE_RETENTION, MAX_ARCHIVE_RETENTION]`**
(1–10 000).  The protocol default is **`DEFAULT_ARCHIVE_RETENTION = 128`**.

```
set_archive_retention(limit: u32)    // admin only; emits archive::retention event
get_archive_retention() -> u32       // returns current limit
```

- Values below `MIN_ARCHIVE_RETENTION` (1) or above `MAX_ARCHIVE_RETENTION` (10 000) are rejected with
  `ContractError::InvalidArchiveRetention`.
- Changes apply only to **future** archive writes — existing entries are never retroactively pruned
  just because the limit was lowered.

### FIFO prune with events

When a new archive write would cause `RecentArchivedRoundIds.len()` to exceed the configured
retention limit, the contract prunes oldest entries in FIFO order, one per excess entry:

1. Remove `ArchivedRound(oldest_id)` from persistent storage.
2. Remove `CancelledRound(oldest_id)` if present (no orphaned markers).
3. Remove `oldest_id` from the front of `RecentArchivedRoundIds`.
4. Emit an `("archive", "pruned")` event with the pruned `round_id` and the current `retention_limit`.

```
event: ("archive", "pruned") → (round_id: u64, retention_limit: u32)
```

This prune loop runs **synchronously** inside the archive write, so the total number of
`ArchivedRound` entries never exceeds the retention limit after any terminal round transition.

### Missing-id semantics

All archive queries follow a consistent rule: **if a round_id's archived summary has been
pruned, every query for that round_id returns `None` (or `Unknown` for
`get_round_status`).**  Specifically:

- `get_archived_round(pruned_id)` → `None`
- `get_user_archived_participation(user, pruned_id)` → `None`
- `get_recent_archived_rounds(…)` → pruned ids never appear in the index
- `get_round_status(pruned_id)` → `Unknown` (or `Cancelled` if a `CancelledRound`
  marker still exists for an admin-cancelled round that was pruned)

This guarantees that downstream indexers and explorers see a consistent view:
once a round is pruned, it is indistinguishable from a round that never existed,
except for the `CancelledRound` marker which provides status continuity for
admin-cancelled rounds.

## Operation cost — before vs after

For a round with **N** participants:

| Path | Before (single-map blob) | After (indexed keys) |
|---|---|---|
| `place_bet` (per user) | 1 deserialise + 1 reserialise of N-entry map | 1 has-check + 1 write of single record + 1 append to participant list |
| `place_precision_prediction` | same N-cost as above | same O(1) cost as above |
| `get_user_position` | full N-entry map read | single composite-key read |
| `get_user_precision_prediction` | full N-entry map read | single composite-key read |
| `resolve_round` | 1 read of N-entry map + N stat updates | 1 read of N-entry participant list + N composite-key reads + N stat updates |
| `claim_winnings` | unchanged | unchanged |

The win is at **bet placement**: instead of paying O(N) every time someone
joins the round, the contract now pays O(1). At N = 60 (large-round test),
the old design would deserialise + reserialise a 59-entry map on every new
bet; the new design touches only the single indexed key for that user plus a
small append on the participant list.

## Per-user outcome records

At every terminal round transition the contract also writes a
`UserRoundOutcome(round_id, user)` record for each participant.  These records
preserve the user's stake, payout, prediction details, and outcome type
(`Win`, `Loss`, `Refund`, `Cancel`) so that profile / history queries can be
answered without replaying the event stream.

```rust
pub struct UserRoundOutcome {
    pub user: Address,
    pub round_mode: u32,       // 0 = UpDown, 1 = Precision
    pub prediction_side: u32,  // 0 = Up, 1 = Down, 2 = Precision
    pub predicted_price: u128,
    pub stake: i128,
    pub payout: i128,
    pub outcome: UserOutcomeType,
}
```

**Important note on storage leaks:** When a round's `ArchivedRoundSummary` is
pruned, the per-user `UserRoundOutcome` records for that round are NOT removed
from storage (the participant list is already cleaned up at resolution time, so
the contract no longer knows which users participated).  However,
`get_user_archived_participation` enforces missing-id semantics by checking for
the `ArchivedRound` key first — it returns `None` for pruned rounds regardless
of whether a `UserRoundOutcome` record still exists.

## Resolution iteration

Resolution still has to iterate every participant — there is no way around
that, regardless of layout. The participant list (`RoundParticipants(round_id)`)
preserves the iteration order so that the resolution path matches the old
behaviour exactly: same payout formula, same tie-break order, same stats
updates. Per-user position records are then read individually inside the
loop, each as an O(1) ledger entry rather than a slice of one large blob.

## Cleanup

`resolve_round` now performs targeted deletes: it walks the participant list
and removes each `Position(round_id, user)` (and `PrecisionPosition` for
precision mode), then removes the participant list entry itself. The legacy
single-map keys are also `remove`d in case they exist from pre-migration data.

## Determinism guarantees

The refactor preserves every observable output:

- Pool totals (`pool_up` / `pool_down`) are still maintained on the `Round`
  struct exactly as before.
- Refund-on-tie, proportional payout on price move, and precision-mode tie
  splitting all use the same formulas as before.
- `_update_stats_win` / `_update_stats_loss` are called for every participant
  in the same iteration order as before (driven by the participant list, which
  is appended in bet order).

Existing tests (`lifecycle`, `betting`, `resolution`, `mode_tests`, …) all
pass without functional changes. The one test that previously poked at
`DataKey::UpDownPositions` directly (`test_multiple_rounds_lifecycle`) was
updated — it now lets `place_bet` write the indexed key naturally and only
overrides the round pool totals to inject a simulated losing pool.

## Migration notes

- **Legacy keys remain readable.** `get_user_position` falls back to
  `DataKey::Positions` if no indexed entry is present. This lets a
  pre-existing deployment serve historical reads while the next round runs
  against the new layout.
- **Legacy keys are no longer written.** `place_bet` and
  `place_precision_prediction` only emit indexed keys.
- **No data migration required.** Once `resolve_round` is called for any
  in-flight round under the old layout, the contract removes the legacy
  single-map keys and all subsequent rounds use the indexed layout.

## Test coverage

### Archive retention (`contracts/src/tests/archive_retention.rs`)

| Test | What it verifies |
|---|---|
| `test_default_archive_retention` | Default retention is 128. |
| `test_set_archive_retention_below_min_fails` | Rejects `limit = 0`. |
| `test_set_archive_retention_above_max_fails` | Rejects `limit = 10_001`. |
| `test_set_archive_retention_valid` | Accepts valid limits. |
| `test_set_archive_retention_emits_event` | Setting retention emits `("archive", "retention")`. |
| `test_fifo_pruning_with_small_limit` | Prunes oldest entries in FIFO order; verifies storage removals. |
| `test_prune_event_emitted` | Each prune emits `("archive", "pruned")`. |
| `test_retention_change_applies_to_future_writes_only` | Lowering retention does not retroactively prune. |
| `test_get_archived_round_after_prune_returns_none` | Pruned rounds return `None` from `get_archived_round`. |
| `test_get_recent_archived_rounds_capped_by_retention` | `get_recent_archived_rounds(limit)` is capped by retention. |
| `test_archive_retention_cannot_be_set_by_non_admin` | Only admin can set retention. |
| `test_user_archived_participation_returns_none_after_prune` | Missing-id semantics: user outcome query returns `None` for pruned rounds. |
| `test_prune_cleans_cancelled_round_marker` | `CancelledRound` marker is cleaned up during prune. |
| `test_prune_multiple_rounds_cleans_associated_data` | Multiple prunes clean all associated data in FIFO order. |

### Storage benchmarks (`contracts/src/tests/storage_benchmarks.rs`)

| Test | What it verifies |
|---|---|
| `bench_place_bet_writes_single_user_key` | `place_bet` writes the composite-key entry and does **not** write the legacy bulk-map key. |
| `bench_place_bet_op_count_assertion` | After 10 bets, exactly 10 indexed position keys + 1 participant-list key exist. |
| `bench_resolve_cleans_indexed_keys` | After resolution, all per-user keys + the participant list are removed. |
| `bench_large_round_resolves_correctly` | 60-participant round resolves with correct payouts and full storage cleanup. |
| `bench_precision_mode_indexed_keys` | Same indexed layout used for `PrecisionPosition` keys. |
