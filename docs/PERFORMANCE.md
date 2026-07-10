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

## Regression policy

Every benchmark asserts the measured CPU instructions and memory bytes stay within the standard Soroban per-transaction resource budget. Treat any benchmark failure as a hard regression. If a passing run still shows a spike of more than 20% versus the last published table, call it out in the pull request and either optimize the path or document the reason for the higher cost.

## CI artifact guidance

CI should run the generation command above with `--nocapture` and upload the captured test log as a `cost-benchmarks` artifact. If the project does not publish artifacts in a given workflow, paste the emitted markdown rows into this file in the same change that modifies benchmark-sensitive code.
