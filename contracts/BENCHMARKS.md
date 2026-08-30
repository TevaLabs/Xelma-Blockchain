# Performance Benchmarks & Regression Guardrails

This crate ships a gas/cost benchmark suite that measures the host
CPU-instruction and memory cost of the critical contract paths and gates them
against a documented ceiling, so performance drift is caught early.

Benchmarks live in [`src/tests/cost_benchmarks.rs`](src/tests/cost_benchmarks.rs).

## Covered paths

| Path                | Benchmark                      |
| ------------------- | ------------------------------ |
| `create_round`      | `bench_cost_create_round`      |
| `place_bet`         | `bench_cost_place_bet`         |
| precision submit    | `bench_cost_precision_submit`  |
| `resolve_round`     | `bench_cost_resolve_round`     |
| `claim_winnings`    | `bench_cost_claim_winnings`    |
| updown positions (paged) | `bench_cost_get_updown_positions_page` |
| precision predictions (paged) | `bench_cost_get_precision_predictions_page` |
| leaderboard by wins | `bench_cost_get_leaderboard_by_wins` |
| leaderboard by streak | `bench_cost_get_leaderboard_by_streak` |

## Running locally

```bash
cargo test --package xelma-contract cost_benchmarks -- --nocapture
```

The `--nocapture` flag prints a `[bench]` line per path with the measured CPU
instructions and memory bytes, e.g.:

```text
[bench] create_round             cpu=      ...... mem=      ......
[bench] place_bet                cpu=      ...... mem=      ......
```

## Baselines and tolerances

Each path is asserted to stay within the **standard Soroban per-transaction
resource budget** (`100,000,000` CPU instructions and `100 MiB` memory). This
is a hard guardrail: a path that exceeds it would fail on-chain. The benchmark
output records the actual per-path cost.

To tighten the guardrail toward true regression detection:

1. Run the suite with `--nocapture` on a clean `main`.
2. Record the printed `cpu`/`mem` numbers below as the baseline.
3. Lower the `*_CPU_MAX` / `*_MEM_MAX` constants in `cost_benchmarks.rs` to
   `baseline × tolerance` (a 15–25% tolerance absorbs allocator/host jitter).
4. Update this table in the same PR that changes the constants.

| Path             | Baseline CPU | Baseline MEM | Captured on |
| ---------------- | ------------ | ------------ | ----------- |
| create_round     | 238,185      | 28,297       | fbc2ec4 / 2026-08-30 |
| place_bet        | 308,350      | 49,060       | fbc2ec4 / 2026-08-30 |
| precision submit | 317,168      | 50,054       | fbc2ec4 / 2026-08-30 |
| resolve_round    | 2,500,588    | 502,344      | fbc2ec4 / 2026-08-30 |
| claim_winnings   | 216,792      | 42,210       | fbc2ec4 / 2026-08-30 |
| updown positions (paged) | 286,563 | 29,661 | fbc2ec4 / 2026-08-30 |
| precision predictions (paged) | 303,654 | 32,499 | fbc2ec4 / 2026-08-30 |
| leaderboard by wins | 26,885 | 10,517 | fbc2ec4 / 2026-08-30 |
| leaderboard by streak | 26,887 | 10,517 | fbc2ec4 / 2026-08-30 |

## CI integration

The CI `rust-test` job runs the full workspace test suite (which includes these
benchmarks, so a breach fails the build) and additionally runs a dedicated
`Benchmark report` step with `--nocapture` to surface the measured cost numbers
in the workflow logs for drift review.
