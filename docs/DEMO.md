# Demo Scenario Pack — Xelma Protocol

A scripted, reproducible demo of the core prediction-market outcomes,
designed for judges, reviewers, and operators who need fast proof of
correctness.

## Scenarios

| # | Scenario | Mode | Price Movement | Winner |
|---|----------|------|----------------|--------|
| 1 | **Up-Win** | Up/Down (mode 0) | Rises (1.5 → 1.65) | UP bettors split the DOWN pool |
| 2 | **Down-Win** | Up/Down (mode 0) | Falls (1.5 → 1.35) | DOWN bettors split the UP pool |
| 3 | **Precision-Tie** | Precision (mode 1) | Hits predicted price (1.55) | Both predict the same winning price → tie-split with remainder to first |
| 4 | **Multi-Feed-Quorum** | Up/Down (mode 0) | Rises (1.5 → 1.65), 3-feed median | UP bettors split the DOWN pool, settled via oracle quorum |
| 5 | **Season-Rollover** | Up/Down (mode 0), 2 rounds | Round 1 rises, round 2 falls | Leaderboard season is archived and advances between rounds |

## Quick Start

```bash
# One command — builds WASM, starts local network, runs all 5 scenarios
./scripts/demo_scenarios/run_all.sh

# Run a single scenario
./scripts/demo_scenarios/scenario_up_win.sh
./scripts/demo_scenarios/scenario_down_win.sh
./scripts/demo_scenarios/scenario_precision_tie.sh
./scripts/demo_scenarios/scenario_multi_feed.sh
./scripts/demo_scenarios/scenario_season_rollover.sh
```

## Prerequisites

- Stellar CLI (`stellar` ≥22.0, with `stellar container` support)
- Docker (running)
- `jq`
- Rust 1.94.0+ (for WASM build)

Verify:

```bash
stellar --version
docker info >/dev/null && echo "Docker OK"
jq --version
```

## What Each Scenario Asserts

### Up-Win (`scenario_up_win.sh`)

**Setup**: Alice bets 500 vXLM **UP**, Bob bets 300 vXLM **DOWN**.

**Expected payout**: Alice wins 800 vXLM (500 principal + 300 from Bob's pool).
Bob loses his 300 vXLM.

**Assertions**:
- `place_bet` emits `(bet, placed)` events for both users
- Alice's `pending_winnings` > 500 (principal + profit)
- Bob's `pending_winnings` == 0 (lost)
- Alice claims and her balance exceeds 1000 (initial mint)
- Round archive contains `Resolved` status
- Alice stats: `total_wins >= 1`
- Bob stats: `total_losses >= 1`

### Down-Win (`scenario_down_win.sh`)

**Setup**: Alice bets 400 vXLM **DOWN**, Bob bets 200 vXLM **UP**.

**Expected payout**: Alice wins 600 vXLM (400 principal + 200 from Bob's pool).
Bob loses his 200 vXLM.

**Assertions**:
- Protocol status transitions: `ClaimsOnly → Active → ClaimsOnly`
- Alice's `pending_winnings` > 400 (principal + profit)
- Bob's `pending_winnings` == 0 (lost)
- Pool stats query returns data (non-null)

### Precision-Tie (`scenario_precision_tie.sh`)

**Setup**: Both predict 1.55 @ 500 vXLM (Alice) and 300 vXLM (Bob).
Oracle resolves at 1.55 — both tie for closest.

**Expected payout**: Total pot = 800 vXLM. Split 400/400 (no remainder on
even split). First winner (by address sort order) gets dust if any.

**Assertions**:
- Round created with `mode == Precision`
- `place_precision_prediction` emits `(predict, price)` events
- `get_precision_predictions` returns both entries
- Both users have `pending_winnings > 0`
- Combined payout == total pot (800 vXLM) — fee disabled by default
- Pool stats report `precision_participant_count >= 2`

### Multi-Feed-Quorum (`scenario_multi_feed.sh`)

**Setup**: Oracle quorum configured for 3 feeds. Alice bets 500 vXLM **UP**,
Bob bets 300 vXLM **DOWN**. Oracle resolves with a 3-observation payload
(median price, outlier check) instead of a single price.

**Assertions**:
- `multisum` event emitted with survivor count ≥ the configured quorum threshold
- Alice's `pending_winnings` > 500 (UP won)
- Bob's `pending_winnings` == 0 (lost)
- Alice successfully claims her winnings

### Season-Rollover (`scenario_season_rollover.sh`)

**Setup**: Round 1 (season 1) resolves UP — Alice wins, Bob loses. Admin
calls `reset_leaderboard_season`. Round 2 (season 2) resolves DOWN — Bob
wins, Alice loses.

**Assertions**:
- `get_current_season_id` advances `1 → 2` on reset, and the reset emits
  `(season, reset)`
- Season 1's frozen archive (`get_season_archive`) matches what was live
  just before the reset: 2 participants, Alice ranked first
- `get_season_leaderboard_by_wins(season_id=1, ...)` keeps serving those 2
  frozen entries after the reset — the same query path transparently
  switches from live index to archive
- Season 2 starts empty, then reflects only Bob's win from round 2 —
  Bob's season-1 stats stay at 0 wins throughout (seasons never leak into
  each other)
- The lifetime leaderboard (`get_user_stats`) reflects both rounds combined:
  Alice and Bob each show exactly 1 win overall

## Troubleshooting

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| `stellar: command not found` | CLI not installed or not in PATH | Install [Stellar CLI](https://developers.stellar.org/docs/soroban/cli) |
| `stellar container start` fails | Docker not running | `docker info` — start Docker Desktop |
| Network container not healthy | RPC startup race | `SKIP_NETWORK_START=1 ./run_all.sh` after `stellar container start local` |
| `WASM not found` | Contract not built | `cd contracts && stellar contract build` |
| Deploy retries exhausted | RPC not ready after container start | Increase sleep in `lib.sh::start_network` or run `stellar network health --network local` manually |
| `FutureOracleData` error | Timestamp drift | The oracle payload timestamp uses a backdate margin; ensure your system clock is accurate |
| `OracleNonceReused` | Duplicate resolve call | Use a unique `nonce` per call; the demos use `nonce=1` per round |
| `ContractPaused` | Previous scenario left paused state | Run `run_all.sh` instead of individual scripts (each starts fresh) |

### Per-Scenario Debugging

Each scenario logs every step with the scenario name prefix:

```bash
◆ Up-Win :: Preflight
◆ Up-Win :: Starting local Soroban network
...
◆ Up-Win :: End-state assertions
  ✓ event 'bet','placed' found
  ✘ pending(GA...)=0 not greater than 500000000
```

The last line before failure pinpoints the exact assertion. Common failure
modes for judges:

1. **Balance assertions fail**: Check that `mint_initial` returned 1000 vXLM
   and that bet amounts do not exceed balance.
2. **Event not found**: Contract output format may change with Soroban SDK
   versions. Inspect raw output with `echo "$OUTPUT" | head -20`.
3. **Round phase wrong**: Local network ledger advances faster than wall
   clock; the `wait_for_round_end` loop should handle this. If phase is
   `3` (Resolvable) when expecting `1` (Betting), the round window is too
   short — increase `DEFAULT_RUN_WINDOW_LEDGERS` in `contract.rs`.

## Adding a New Scenario

1. Create `scripts/demo_scenarios/scenario_<name>.sh`
2. Source `lib.sh` at the top
3. Set `SCENARIO_NAME` and scenario-specific parameters
4. Use the lifecycle helpers: `preflight`, `start_network`, `create_identities`,
   `deploy_contract`, `initialize`, `mint_tokens`
5. Drive the round: `create_round`, place bets/predictions, `wait_for_round_end`,
   `resolve_with_oracle`
6. Assert end-state with `assert_*` helpers
7. Add to `run_all.sh` via the `run_scenario` function

## Environment Reference

| Variable | Default | Description |
|----------|---------|-------------|
| `WASM_PATH` | `target/wasm32v1-none/release/xelma_contract.wasm` | Path to compiled WASM |
| `SKIP_NETWORK_START` | `0` | Set to `1` if network container already running |
| `KEEP_NETWORK` | `0` | Set to `1` to keep container alive after exit (single-scenario) |
| `KEEP_NETWORK_AFTER` | `0` | `run_all.sh` only: set to `1` to leave container running after all scenarios |
| `NETWORK` | `local` | Stellar network name |

## Design Notes

- **Deterministic addresses**: Each run generates fresh Stellar keys, so
  address ordering used in Precision remainder policy is reproducible per run.
- **No protocol fee by default**: Fee bps is `None` (disabled). If an admin
  sets a fee in a modified test, the combined-payout assertion in
  Precision-Tie adjusts gracefully (warns on mismatch but does not fail).
- **One-shot nonce**: Each resolution uses `nonce=1` (unique per round because
  round IDs are monotonic). Multi-resolve scenarios would cycle nonces.
