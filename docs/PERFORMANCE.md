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

## Precision participant-cap vs CPU budget

Operators call `set_max_precision_participants` to limit how many predictions a Precision round can accept. The table below shows how the **resolve** CPU cost scales with participant count, so operators can choose a safe cap that leaves headroom for the rest of the transaction budget.

The standard Soroban per-transaction CPU budget is **100,000,000 instructions**. Every value below that limit means headroom is available for create-round, oracle submission, and other entrypoints sharing the same transaction budget.

| Participants | CPU instructions | Memory bytes | CPU headroom | Memory headroom |
|---:|---:|---:|---:|---:|
| 1 (submit only) | _regenerate_ | _regenerate_ | _regenerate_ | _regenerate_ |
| 10 | _regenerate_ | _regenerate_ | _regenerate_ | _regenerate_ |
| 25 | _regenerate_ | _regenerate_ | _regenerate_ | _regenerate_ |
| 50 | _regenerate_ | _regenerate_ | _regenerate_ | _regenerate_ |
| 100 | _regenerate_ | _regenerate_ | _regenerate_ | _regenerate_ |

### Reading the table

- **CPU headroom** = `100,000,000 − CPU instructions`. This is the budget remaining for other contract logic within the same transaction.
- **Memory headroom** = `104,857,600 − memory bytes`. Soroban memory is per-transaction, not per-entrypoint.
- If headroom drops below ~20% of the budget, the cap is too aggressive for production use — choose the next lower row.
- Regenerate with the same command as the entrypoint table; the `resolve_precision_*` benchmarks produce these rows.

### Operator guidance

1. Start with a conservative cap (e.g. **25 participants**) and monitor actual CPU usage in CI artifact logs.
2. Raise the cap only after confirming the headroom column stays above 20% of the budget in your target Soroban host version.
3. The cap applies globally to all Precision rounds — there is no per-round override.
4. Changing the cap does **not** affect rounds that are already open; only new rounds respect the updated value.

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
