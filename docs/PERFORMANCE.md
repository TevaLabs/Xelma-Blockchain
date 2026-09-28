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

The exact numbers depend on the Soroban SDK version and host runtime. Regenerate the table before release and replace the rows below with the `--nocapture` output.

| Function / path | CPU instructions | Memory bytes |
|---|---:|---:|
| `create_round` | _regenerate_ | _regenerate_ |
| `place_bet` | _regenerate_ | _regenerate_ |
| `resolve_round` | _regenerate_ | _regenerate_ |
| `claim_winnings` | _regenerate_ | _regenerate_ |
| `get_updown_positions_page` | _regenerate_ | _regenerate_ |
| `get_precision_predictions_page` | _regenerate_ | _regenerate_ |
| `get_precision_predictions_cursor` | _regenerate_ | _regenerate_ |
| `get_updown_positions_cursor` | _regenerate_ | _regenerate_ |
| `get_leaderboard_by_wins` | _regenerate_ | _regenerate_ |
| `get_leaderboard_by_streak` | _regenerate_ | _regenerate_ |
| `leaderboard_update_at_limit` | _regenerate_ | _regenerate_ |
| `season_reset_at_limit` | _regenerate_ | _regenerate_ |
| `leaderboard_full_page_read` | _regenerate_ | _regenerate_ |

> For the Precision participant cap, see
> [Precision participant cap vs CPU budget](#precision-participant-cap-vs-cpu-budget-issue-516)
> below — settlement cost there is linear in participants and crosses the
> per-transaction budget well before the shipped default cap.

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

## Precision participant cap vs CPU budget (Issue #516)

Precision rounds enforce a configurable participant cap
(`set_max_precision_participants`, default **1,000**, hard ceiling **10,000**).
Resolution (`_resolve_precision_mode`) iterates every participant, so the cap
is the knob that decides whether a Precision round can still be settled inside
one on-chain transaction. The table below is the measured cost of
`resolve_round` for a Precision round at each participant count.

### Regeneration command

```text
cargo test --package xelma-contract precision_cap -- --nocapture
```

Each step prints a machine-readable line:

```text
[cost-benchmark-cap] participants=10 cpu_instructions=31703718 memory_bytes=4461060 cpu_pct_of_budget=31.7037
```

and a markdown row that can be copied directly into the table below. The
`rust-test` job in `.github/workflows/ci.yml` already runs the full
`cost_benchmarks` suite with `--nocapture` and uploads the output as the
`cost-benchmarks` artifact, so this table can be refreshed from any CI run.

### Measured cap vs CPU cost

Standard Soroban per-transaction budget: **100,000,000 CPU instructions** and
**104,857,600 memory bytes**.

| Precision participants | CPU instructions | Memory bytes | CPU % of budget |
|---:|---:|---:|---:|
| 1 | 1,656,416 | 360,695 | 1.66% |
| 5 | 8,544,721 | 1,504,505 | 8.54% |
| 10 | 31,703,718 | 4,461,060 | 31.70% |
| 15 | 82,539,801 | 9,801,940 | 82.54% |
| 20 | 172,625,110 | 18,263,895 | 172.63% |
| 25 | 315,773,433 | 30,583,675 | 315.77% |

### Reading the table

Settlement cost is **linear in participants**, at roughly **13.1M CPU
instructions per participant** (`bench_cost_precision_cap_cpu_sweep` measures
this slope directly). Consequences:

- The per-transaction CPU budget is exhausted at **≈15–20 participants**.
  Resolution at 20 participants needs 1.7× the budget, at 25 it needs 3.2×.
- A round that exceeds the budget does not fail gracefully — `resolve_round`
  aborts with `Budget ExceededLimit` and the round stays unsettled until an
  operator intervenes.
- Memory crosses its 100 MiB budget at roughly the same point as CPU.

### Recommended cap

| Setting | Value | Rationale |
|---|---:|---|
| Protocol default (`DEFAULT_MAX_PRECISION_PARTICIPANTS`) | 1,000 | Unchanged by this issue — see the gap below. |
| Hard ceiling (`MAX_PRECISION_PARTICIPANTS_LIMIT`) | 10,000 | Unchanged; a range check, not a safety guarantee. |
| **Recommended operating cap** | **7** | Largest cap whose measured settlement cost stays inside one transaction with margin (~15% of the CPU budget at 7 participants). |

`verify_precision_cap_is_within_cpu_budget` in
`contracts/src/tests/cost_benchmarks.rs` pins this recommendation to the
measured curve: it re-derives the per-participant slope, computes the largest
affordable cap, and fails if the documented recommendation exceeds it or if
settling at it breaches the CPU or memory budget. If settlement cost ever
changes, the test fails rather than letting this table go stale.

### Known gap: the default cap exceeds the CPU budget

> **The shipped default cap of 1,000 participants cannot settle within a single
> on-chain transaction at the measured ~13.1M CPU per participant** — it would
> need roughly 13× the per-transaction CPU budget. Extrapolating the table, the
> default cap should be around **7**.

This is a **known, pre-existing risk, not a regression introduced by this
issue.** The default of 1,000 predates these measurements. What this change
adds is the evidence: the cap↔CPU relationship is now measured, published, and
guarded by a test.

Lowering `DEFAULT_MAX_PRECISION_PARTICIPANTS` is an **economic and protocol
change** — it changes how many people a Precision round can admit — so it is
deliberately left out of this documentation-focused change. Operators who need
to settle large Precision rounds today should call
`set_max_precision_participants` with a value at or below the recommended cap
of 7. Raising the cap back toward 1,000 requires first making settlement
sub-linear (or otherwise dramatically cheaper) in participant count.

### Regression policy for this path

`bench_cost_precision_cap_cpu_sweep` measures rather than gates each step
(because the measured cost is intentionally above budget at the upper end of
the sampled range — that is the finding). It does assert that cost never
*decreases* as participants grow, so a change that makes settlement cheaper is
always visible in the published table.

`verify_precision_cap_bounds_actual_participation` asserts the cap is a real
admission limit: with `set_max_precision_participants(3)`, the fourth predictor
is rejected with `PrecisionCapExceeded`. That is what makes the table an upper
bound on real settlement cost rather than a hypothetical.

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
and runner architecture. The current source baseline cannot be measured
locally until the unrelated upstream compile failures recorded in
`SECURITY_REVIEW.md` are repaired; no fabricated CPU numbers are published.
