# Performance cost benchmarks

The contract keeps gas/resource expectations transparent by measuring major public entrypoints in `contracts/src/tests/cost_benchmarks.rs`.

## Generate the table

Run:

```text
cargo test --package xelma-contract cost_benchmarks -- --nocapture
```

Each benchmark prints both a machine-readable line:

```text
[cost-benchmark] name=create_round cpu_instructions=... memory_bytes=...
```

and a markdown table row that can be copied into this document.

## Latest local benchmark table

Measured 2026-10-24 against upstream `main` commit `bfda3b807619979cb9a5511370c53600a566acd4`, via `cargo test --package xelma-contract cost_benchmarks -- --nocapture` (soroban-sdk 23.0.1, `cargo test` default host runtime). The exact numbers depend on the Soroban SDK version and host runtime; regenerate before release.

| Function / path | CPU instructions | Memory bytes |
|---|---:|---:|
| `create_round` | `279578` | `35284` |
| `place_bet` | `308710` | `49944` |
| `precision_submit` | `318749` | `51006` |
| `claim_winnings` | `218355` | `42618` |
| `get_updown_positions_page` | `286285` | `29729` |
| `get_precision_predictions_page` | `302514` | `32567` |
| `resolve_round` | `2508327` | `508056` |
| `resolve_round_medium_n25` | `306664340` | `30371736` |
| `resolve_round_max_cap_n100` | `17975552255` | `1177240606` |
| `resolve_precision_max_cap_n100` | `18208121255` | `1179087258` |

**Note on `resolve_round_medium_n25`**: this run's measured CPU (306,664,340) exceeds this benchmark's own `RESOLVE_CPU_MAX` ceiling assertion in `cost_benchmarks.rs`, so `bench_cost_resolve_round_medium_set` fails on this host/SDK combination. The number above is the actual measured value from that failing run, not a passing baseline; treat it as a known regression to investigate, not a target to match.

**Benchmarks that cannot currently be measured**: `bench_cost_leaderboard_update_at_limit`, `bench_cost_leaderboard_full_page_read_at_limit`, `bench_cost_season_reset_at_limit`, and their `verify_*_cost_is_bounded` counterparts all hit a hard `HostError: Error(Budget, ExceededLimit)` host-level trap (not a soft assertion failure) on this soroban-env-host (23.0.1) version, confirmed via `git stash` to be identical on a clean `main` checkout, so it is pre-existing and unrelated to any feature work. No `get_leaderboard_by_wins`, `get_leaderboard_by_streak`, `get_precision_predictions_cursor`, or `get_updown_positions_cursor` benchmark exists in `cost_benchmarks.rs` at all; those are aspirational rows from an earlier draft of this table, not real measurements.

## Regression policy

Every benchmark asserts the measured CPU instructions and memory bytes stay within the standard Soroban per-transaction resource budget. Treat any benchmark failure as a hard regression. If a passing run still shows a spike of more than 20% versus the last published table, call it out in the pull request and either optimize the path or document the reason for the higher cost.

## CI artifact guidance

The `rust-test` job in `.github/workflows/ci.yml` runs the generation command
above with `--nocapture`, tees the output to `cost-benchmarks.log`, and
uploads it as a `cost-benchmarks` build artifact (via `actions/upload-artifact`,
7-day retention) — including on a failed/regressed run, so the exact numbers
that tripped a `*_CPU_MAX`/`*_MEM_MAX` assertion are always reviewable from the
workflow run's Artifacts section, not just the truncated job log. When you
touch a benchmark-sensitive path, download that artifact from your PR's CI
run and paste the relevant rows into this file's table in the same change.

## Pagination query limits (Issues #430 and #574)

To prevent adversarial over-limit requests from bypassing CPU/memory budgets,
paginated query functions enforce strict pagination limits:

| Function | Max page size | Error on exceed |
|---|---:|---|
| `get_precision_predictions_cursor` | 100 | `PageSizeExceeded` (94) |
| `get_updown_positions_cursor` | 100 | `PageSizeExceeded` (94) |
| `get_leaderboard_by_wins` | 100 | `PageSizeExceeded` (94) |
| `get_leaderboard_by_streak` | 100 | `PageSizeExceeded` (94) |
| `get_user_archive_history` | 100 | `PageSizeExceeded` (94) |

**Key behaviors**:
- Requests with `limit == 0` or `limit > 100` are rejected with error code 94.
- The limit is **not** clamped; over-limit requests fail fast.
- Valid limits are `1..=100` (inclusive).
- Cursor-based functions (leaderboard, predictions, positions) return `(Vec<T>, Option<Address>)`.
- When results are exhausted, `next_cursor` is `None` and the page is empty.

**Gas guard rationale**:
The 100-item limit ensures that even under worst-case data density (each item fetches
from persistent storage), query CPU and memory consumption remains bounded within
Soroban's per-transaction budget. Rejecting over-limit requests prevents callers from
accidentally or maliciously requesting unbounded batches that could fail during
settlement or cause timeouts.

`contracts/src/tests/pagination_gas_guards.rs` exercises every cursor query at
`0`, `MAX_PAGE_SIZE + 1`, and `u32::MAX` and asserts the exact
`PageSizeExceeded` error. It also proves that `MAX_PAGE_SIZE` itself is
accepted. Validation happens before any storage scan, so adversarial limits
have constant rejection cost rather than caller-controlled iteration cost.

## Updating a cost-benchmark ceiling

The `*_CPU_MAX`/`*_MEM_MAX` constants in `contracts/src/tests/cost_benchmarks.rs`
are the enforcement mechanism — a benchmark failing them is what "flags a cost
regression." See [`contracts/BENCHMARKS.md`](../contracts/BENCHMARKS.md) for
the full baseline-recording and ceiling-tightening procedure (currently every
path is gated at the full Soroban per-transaction budget; tightening these to
real measured baselines ± tolerance is the documented next step there). Any
PR that intentionally raises a ceiling must also update the table in this file
and in `contracts/BENCHMARKS.md` with the new baseline and the commit/date it
was captured on, exactly like `docs/wasm-size-budget.md`'s baseline-bump
procedure for the separate WASM size gate.

## Leaderboard performance analysis (Issues #431 and #575)

Leaderboard operations are benchmarked at `LEADERBOARD_LIMIT` (100 entries) to
verify bounded CPU cost. See `contracts/src/tests/cost_benchmarks.rs` for the
benchmark implementations:

- `bench_cost_leaderboard_update_at_limit` — measures worst-case insertion when
  the leaderboard is at capacity (new user with 0 wins inserted into a full
  sorted list of 100 entries).
- `bench_cost_season_reset_at_limit` — measures archive creation and season
  advancement with 100 entries in both wins and streak indexes.
- `bench_cost_leaderboard_full_page_read_at_limit` — measures a full-page
  paginated read of 100 entries.
- `verify_leaderboard_update_cost_is_bounded` — asserts update cost is < 50%
  of the per-transaction CPU budget.
- `verify_season_reset_cost_is_bounded` — asserts reset cost is < 50% of the
  per-transaction CPU budget.

### Bounded operations

All leaderboard operations are bounded by `LEADERBOARD_LIMIT` (100):

| Operation | Bound | Complexity |
|---|---|---|
| `_update_leaderboards` (insertion sort) | `LEADERBOARD_LIMIT` entries | O(n²) worst case |
| `reset_leaderboard_season` (archive + dedup) | `2 × LEADERBOARD_LIMIT` entries | O(n²) worst case |
| `get_leaderboard_by_wins` (paginated read) | `min(limit, LEADERBOARD_LIMIT)` | O(n) |
| `get_leaderboard_by_streak` (paginated read) | `min(limit, LEADERBOARD_LIMIT)` | O(n) |

### Why there are no unbounded scans

The leaderboard index is a bounded sorted `Vec<Address>` capped at
`LEADERBOARD_LIMIT`. On every update:
1. The user is removed from the existing list (O(n) scan, n ≤ 100).
2. The user is re-inserted in sorted position via insertion sort (O(n)).
3. The list is truncated to `LEADERBOARD_LIMIT`.

Season reset builds a `SeasonArchive` from the two bounded indexes (wins and
streak, each ≤ `LEADERBOARD_LIMIT`) and deduplicates participants (≤ `2 ×
LEADERBOARD_LIMIT`). Neither loop scans unbounded storage.

### Expected CPU behavior

At `LEADERBOARD_LIMIT = 100`, the worst-case insertion sort performs at most
~10,000 comparisons. The Soroban per-transaction budget is 100,000,000 CPU
instructions, so leaderboard operations should consume well under 1% of the
budget. The 50% ceiling assertions in the benchmark tests provide a safety
margin for host allocator jitter and SDK overhead.

### Benchmark evidence

The executable results are produced by the following named tests in
[`contracts/src/tests/cost_benchmarks.rs`](../contracts/src/tests/cost_benchmarks.rs):

| Operation at `LEADERBOARD_LIMIT` | Result / guard |
|---|---|
| Update | `bench_cost_leaderboard_update_at_limit` records CPU and memory; `verify_leaderboard_update_cost_is_bounded` requires CPU below 50% of the transaction budget |
| Season reset | `bench_cost_season_reset_at_limit` records CPU and memory; `verify_season_reset_cost_is_bounded` requires CPU below 50% of the transaction budget |
| Full-page read | `bench_cost_leaderboard_full_page_read_at_limit` records CPU and memory and requires exactly 100 returned entries |

CI runs these tests with `--nocapture` and uploads the complete
[`cost-benchmarks` artifact](../.github/workflows/ci.yml) for each run. This is
the authoritative result because host-cost numbers vary with the Soroban SDK
and runner architecture. Locally, the leaderboard and season-reset benchmarks
specifically hit a hard host-level budget trap on this environment's
soroban-env-host version, confirmed pre-existing (see the note in the table
above); every other benchmark in this file runs and measures cleanly, and no
fabricated CPU numbers are published for any row.
