# Performance Benchmarks & Regression Guardrails

This crate ships a gas/cost benchmark suite that measures the host
CPU-instruction and memory cost of the critical contract paths and gates them
against a documented ceiling, so performance drift is caught early.

Benchmarks live in [`src/tests/cost_benchmarks.rs`](src/tests/cost_benchmarks.rs).

## Covered paths

| Path                | Benchmark                              |
| ------------------- | -------------------------------------- |
| `create_round`      | `bench_cost_create_round`              |
| `place_bet`         | `bench_cost_place_bet`                 |
| precision submit    | `bench_cost_precision_submit`          |
| `resolve_round`     | `bench_cost_resolve_round`             |
| `claim_winnings`    | `bench_cost_claim_winnings`            |

### Precision participant-cap scaling (Issue #428)

These benchmarks measure how `resolve_round` cost scales with participant count in Precision rounds. They feed the operator-facing cap↔CPU table in `docs/PERFORMANCE.md`.

| Participants | Benchmark                                       |
| -----------: | ----------------------------------------------- |
| 10           | `bench_cost_resolve_precision_round_n10`         |
| 25           | `bench_cost_resolve_precision_round_n25`         |
| 50           | `bench_cost_resolve_precision_round_n50`         |
| 100          | `bench_cost_resolve_precision_round_max_cap`     |

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
| create_round     | _record_     | _record_     | _date/sha_  |
| place_bet        | _record_     | _record_     | _date/sha_  |
| precision submit | _record_     | _record_     | _date/sha_  |
| resolve_round    | _record_     | _record_     | _date/sha_  |
| claim_winnings   | _record_     | _record_     | _date/sha_  |

## CI integration

The CI `rust-test` job runs the full workspace test suite (which includes these
benchmarks, so a breach fails the build) and additionally runs a dedicated
`Benchmark report` step with `--nocapture` to surface the measured cost numbers
in the workflow logs for drift review.
