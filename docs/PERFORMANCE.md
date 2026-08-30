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

Measured on commit `fbc2ec4` (2026-08-30) with `cargo test --package xelma-contract cost_benchmarks -- --nocapture`.

To refresh, run the command above and replace the table rows with the `[cost-benchmark]` output.

| Function / path | CPU instructions | Memory bytes |
|---|---:|---:|
| `create_round` | 238,185 | 28,297 |
| `place_bet` | 308,350 | 49,060 |
| `precision_submit` | 317,168 | 50,054 |
| `resolve_round` | 2,500,588 | 502,344 |
| `claim_winnings` | 216,792 | 42,210 |
| `get_updown_positions_page` | 286,563 | 29,661 |
| `get_precision_predictions_page` | 303,654 | 32,499 |
| `get_leaderboard_by_wins` | 26,885 | 10,517 |
| `get_leaderboard_by_streak` | 26,887 | 10,517 |

> **Note:** `get_precision_predictions_cursor` and `get_updown_positions_cursor` are defined in
> `queries.rs` but are not wired into the contract client and therefore cannot be benchmarked
> end-to-end. They will be added once the public contract interface is extended.

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
