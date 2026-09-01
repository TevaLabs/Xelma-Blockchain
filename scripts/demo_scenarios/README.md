# Demo Scenarios

> **Issue #426** — Demo scripts for hackathon presentations covering
> basic Up/Down settlement, precision tie, multi-feed oracle quorum,
> early cash-out, and dispute resolution.

Each script deploys the contract to a local Soroban network, walks through
a complete round lifecycle for its specific mode, and prints deterministic
assertions with expected outputs.

## Prerequisites

| Tool | Minimum version |
|------|----------------|
| **stellar CLI** | ≥ 22 (`stellar --version`) |
| **Docker** | running (`docker info`) |
| **jq** | any modern version |

## Quick Start

```bash
# Run all demos in sequence (builds WASM once, shares one network)
./scripts/demo_scenarios/run_all.sh

# Or run individual demos
./scripts/demo_scenarios/scenario_up_win.sh
./scripts/demo_scenarios/scenario_multi_feed.sh
./scripts/demo_scenarios/demo_cash_out.sh
./scripts/demo_scenarios/demo_dispute.sh
```

Each demo starts its own local Soroban network container, deploys the
contract, and tears down on exit.

### Reusing a running network

```bash
SKIP_NETWORK_START=1 ./scripts/demo_scenarios/demo_multi_feed.sh
KEEP_NETWORK=1 ./scripts/demo_scenarios/demo_cash_out.sh
```

### Custom WASM path

```bash
WASM_PATH=path/to/xelma_contract.wasm ./scripts/demo_scenarios/demo_dispute.sh
```

## Scenarios

### 1. Up-Win (`scenario_up_win.sh`)

Classic Up/Down settlement — Up bettor wins when price rises.

| Step | Action |
|------|--------|
| 1 | Deploy and initialize |
| 2 | Alice bets 500 vXLM Up, Bob bets 300 vXLM Down |
| 3 | Oracle resolves at higher price |
| 4 | Alice claims winnings (principal + Bob's loss share) |

### 2. Down-Win (`scenario_down_win.sh`)

Classic Up/Down settlement — Down bettor wins when price falls.

| Step | Action |
|------|--------|
| 1 | Deploy and initialize |
| 2 | Alice bets 400 vXLM Down, Bob bets 200 vXLM Up |
| 3 | Oracle resolves at lower price |
| 4 | Alice claims winnings |

### 3. Precision Tie (`scenario_precision_tie.sh`)

Precision mode — both users predict the same price, both win, pot split.

| Step | Action |
|------|--------|
| 1 | Deploy and initialize |
| 2 | Alice predicts 1.55 @ 500 vXLM, Bob predicts 1.55 @ 300 vXLM |
| 3 | Oracle resolves at 1.55 (exact match) |
| 4 | Both claim — combined payout == total pot |

### 4. Multi-Feed Quorum (`scenario_multi_feed.sh`)

Multi-feed oracle with quorum consensus (3-of-3 feeds agree).

| Step | Action |
|------|--------|
| 1 | Deploy and configure quorum (min=3, threshold=3) |
| 2 | Alice bets 500 vXLM Up, Bob bets 300 vXLM Down |
| 3 | Oracle submits 3 price feeds via `resolve_round_multi` |
| 4 | Quorum reached, Alice claims winnings |

### 5. Early Cash-Out (`demo_cash_out.sh`) **NEW**

User exits a round before resolution, receiving stake minus a fee.

| Step | Action |
|------|--------|
| 1 | Deploy and set early cashout fee to 500 bps (5%) |
| 2 | Both users place bets |
| 3 | User A calls `cash_out_early` mid-round |
| 4 | User A receives ~95% of stake back |
| 5 | Round resolves normally for remaining participant |

### 6. Dispute Resolution (`demo_dispute.sh`) **NEW**

Admin voids a round after the betting window, issuing full refunds.

| Step | Action |
|------|--------|
| 1 | Deploy and set dispute window to 20 ledgers |
| 2 | 3 users place bets |
| 3 | Round ends; admin calls `void_round` |
| 4 | All 3 users receive full stake refunds |

## Deterministic Expected Outputs

Each script uses fixed prices and stake amounts. Every run against the same
WASM produces identical outcomes.

### Deterministic parameters

| Parameter | Value |
|-----------|-------|
| Start price (all) | 15000000 ($1.50) |
| Resolve price (Up-Win) | 16500000 ($1.65) |
| Resolve price (Down-Win) | 13500000 ($1.35) |
| Resolve price (Precision-Tie) | 15500000 ($1.55) |
| Resolve price (Multi-Feed) | 16500000 ($1.65) |
| Cash-out fee | 500 bps (5%) |
| Dispute window | 20 ledgers |

### Verifying deterministic output

```bash
# Capture machine-readable output
./scripts/demo_scenarios/scenario_up_win.sh 2>&1 | tee demo-output.log

# Run all demos and capture output
./scripts/demo_scenarios/run_all.sh 2>&1 | tee demo-all.log
```

## Files

```
scripts/demo_scenarios/
├── README.md                 ← this file
├── lib.sh                    ← shared helpers (bootstrap, deploy, asserts)
├── run_all.sh                ← runs all scenarios sequentially
├── scenario_up_win.sh        ← Up-Win demo
├── scenario_down_win.sh      ← Down-Win demo
├── scenario_precision_tie.sh ← Precision-Tie demo
├── scenario_multi_feed.sh    ← Multi-Feed-Quorum demo
├── demo_cash_out.sh          ← Early Cash-Out demo (new)
└── demo_dispute.sh           ← Dispute Resolution demo (new)
```

## CI Integration

These demos are designed for local presentation use and require Docker +
a running local Soroban network. They are not part of the automated CI
pipeline (which uses in-memory unit tests). To run them in CI, you would
need the `stellar container` infrastructure available.

## Related

- [`scripts/e2e_smoke.sh`](../e2e_smoke.sh) — Single-round lifecycle smoke test
- [`scripts/health_probe/`](../health_probe/) — Protocol health monitoring
- [`scripts/replay/`](../replay/) — Deterministic round replay tooling
- [`docs/ROUND_LIFECYCLE.md`](../../docs/ROUND_LIFECYCLE.md) — Round state machine
- [`docs/STATUS_CODES.md`](../../docs/STATUS_CODES.md) — Status code reference
