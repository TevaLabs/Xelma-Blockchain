# Xelma Demo Scenarios

End-to-end shell scripts that walk through the full round lifecycle against a
live local Soroban network. Each scenario deploys a fresh contract, runs its
flow, asserts deterministic expected outputs, and cleans up its identities.

---

## Prerequisites

| Tool | Purpose |
|------|---------|
| `stellar` CLI | Contract deployment, invocation, network management |
| `jq` | JSON parsing in assertions |
| Docker (or Podman) | Local Soroban network container |

Build the contract WASM once before running:

```bash
cd contracts
stellar contract build --package xelma-contract
```

---

## Running all scenarios

```bash
./scripts/demo_scenarios/run_all.sh
```

The runner builds the WASM once, starts a single `local` network container,
and executes every scenario sequentially against separate fresh deployments.

**Options (environment variables):**

| Variable | Default | Description |
|----------|---------|-------------|
| `SKIP_NETWORK_START` | `0` | Set to `1` if a local container is already running |
| `KEEP_NETWORK` | `0` | Set to `1` to leave the container up after exit |
| `WASM_PATH` | auto-built | Override path to compiled `.wasm` file |

```bash
# Reuse an existing running container
SKIP_NETWORK_START=1 ./scripts/demo_scenarios/run_all.sh

# Keep the container alive after the run (useful for debugging)
KEEP_NETWORK=1 ./scripts/demo_scenarios/run_all.sh
```

Exit codes: `0` = all passed, `1` = one or more failed.

---

## Running a single scenario

Each script is fully self-contained and can be run in isolation:

```bash
./scripts/demo_scenarios/scenario_up_win.sh
./scripts/demo_scenarios/scenario_down_win.sh
./scripts/demo_scenarios/scenario_precision_tie.sh
./scripts/demo_scenarios/scenario_multi_feed.sh
./scripts/demo_scenarios/scenario_cashout_early.sh
./scripts/demo_scenarios/scenario_dispute_void.sh
./scripts/demo_scenarios/scenario_one_sided_refund.sh
```

When run standalone each script manages its own network container lifecycle
(starts on entry, stops on exit) unless `SKIP_NETWORK_START=1` is set.

---

## Scenario catalogue

### 1. Up-Win (`scenario_up_win.sh`)

Alice bets **500 vXLM UP**, Bob bets **300 vXLM DOWN**.  
Oracle resolves at a **higher** price — Alice wins.

| Assertion | Expected value |
|-----------|---------------|
| Alice pending winnings | `> 500 000 000` (principal + Bob's loss share) |
| Bob pending winnings | `0` |
| Alice final balance | `> 1 000 000 000` (grew past initial mint) |
| Round archive status | `Resolved` |
| Alice `total_wins` stat | `≥ 1` |
| Bob `total_losses` stat | `≥ 1` |

---

### 2. Down-Win (`scenario_down_win.sh`)

Alice bets **400 vXLM DOWN**, Bob bets **200 vXLM UP**.  
Oracle resolves at a **lower** price — Alice wins.

| Assertion | Expected value |
|-----------|---------------|
| Alice pending winnings | `> 400 000 000` |
| Bob pending winnings | `0` |
| Protocol status before round | `ClaimsOnly` |
| Protocol status during round | `Active` |
| Protocol status after resolve | `ClaimsOnly` |
| Round archive status | `Resolved` |

---

### 3. Precision-Tie (`scenario_precision_tie.sh`)

Both Alice and Bob predict **exactly** 1.55; oracle settles at 1.55 — both win.

| Assertion | Expected value |
|-----------|---------------|
| Alice pending winnings | `> 0` |
| Bob pending winnings | `> 0` |
| Combined payout | `= 800 000 000` (no protocol fee by default) |
| Each payout | `≥ 400 000 000` (even split) |
| Precision participant count | `≥ 2` |
| Round archive status | `Resolved` |

---

### 4. Multi-Feed-Quorum (`scenario_multi_feed.sh`)

Oracle quorum configured at **min=3, threshold=3, outlier=500 bps**.  
Three feeds all report a higher price → Alice (UP) wins.

| Assertion | Expected value |
|-----------|---------------|
| `oracle multisum` event | emitted with survivor count ≥ quorum |
| Alice pending winnings | `> 500 000 000` |
| Bob pending winnings | `0` |
| Alice final balance | `> 1 000 000 000` |

---

### 5. Early-Cashout (`scenario_cashout_early.sh`)  *(new — issue #519)*

Alice bets **500 vXLM UP**, then exits during the **Running phase** with a
**10 % (1 000 bps) penalty**. Bob bets **300 vXLM DOWN** and holds.

| Assertion | Expected value |
|-----------|---------------|
| `cashout early` event | emitted |
| Alice cashout credited | `450 000 000` (500 × 0.90) |
| Treasury forfeit delta | `50 000 000` (500 × 0.10) |
| Conservation identity | `cashout + forfeit = stake` → `450 + 50 = 500` |
| Alice's position after exit | removed (`null`) |
| Bob pending (DOWN wins, UP pool empty → refund) | `≥ 300 000 000` |
| Alice final balance after claim | `> 950 000 000` |

---

### 6. Dispute-Void (`scenario_dispute_void.sh`)  *(new — issue #519)*

Dispute window set to **10 ledgers**. Oracle resolves (stages result); before
the window closes, `void_round` is called — all stakes are fully refunded.

| Assertion | Expected value |
|-----------|---------------|
| `round voided` event | emitted |
| Alice pending after void | `500 000 000` (full stake) |
| Bob pending after void | `300 000 000` (full stake) |
| Conservation identity | `total_refunded = total_staked` → `800 = 800` |
| `get_round_status` | `7` (Voided) |
| Round archive status | `Voided` |
| Alice final balance after claim | `1 000 000 000` (fully restored) |
| Bob final balance after claim | `1 000 000 000` (fully restored) |

---

### 7. One-Sided-Refund (`scenario_one_sided_refund.sh`)  *(new — issue #519)*

Only Alice bets (UP). Nobody takes the opposing side. Oracle resolves →
contract detects a one-sided pool and triggers **FallbackRefund**.

| Assertion | Expected value |
|-----------|---------------|
| Down pool at betting close | `0` |
| Alice pending after resolve | `500 000 000` (full stake refunded) |
| `get_round_status` | `6` (FallbackRefund) |
| Round archive status | `FallbackRefund` |
| Alice final balance after claim | `1 000 000 000` (initial mint restored) |

---

## Price / amount scale

All amounts use **7 decimal places** (1 vXLM = `10 000 000` units).  
All prices use **7 decimal places** (XLM price 1.50 = `15 000 000`).

| Human value | Wire value |
|-------------|-----------|
| 500 vXLM | `500 000 000` |
| 300 vXLM | `300 000 000` |
| XLM @ 1.50 | `15 000 000` |
| XLM @ 1.65 | `16 500 000` |

---

## Identity & cleanup

Each scenario generates ephemeral key pairs prefixed `demo-<role>-<pid>`.
They are removed automatically in the `trap cleanup EXIT` handler in
`lib.sh` — even if the scenario fails mid-run.

To inspect identities while a scenario is running:

```bash
stellar keys ls
```

---

## Shared library (`lib.sh`)

`lib.sh` provides helpers used by every scenario:

| Helper | Purpose |
|--------|---------|
| `preflight` | Checks `stellar` and `jq` are in `PATH`; builds WASM if missing |
| `start_network` | Starts the `local` Docker container and waits for RPC health |
| `create_identities` | Generates admin / oracle / alice / bob key pairs |
| `deploy_contract` | Deploys the WASM and retries up to 5× on transient failures |
| `initialize` | Calls `initialize(admin, oracle)` |
| `mint_tokens` | Calls `mint_initial` for Alice and Bob |
| `wait_for_round_end` | Polls ledger sequence until `≥ end_ledger` |
| `resolve_with_oracle` | Submits a single-feed oracle payload |
| `resolve_with_oracle_multi` | Submits a three-feed multi-source payload |
| `assert_event` | `grep`-checks an event string in CLI output |
| `assert_balance_eq` / `_gt` | Reads on-chain balance and compares |
| `assert_pending_winnings_eq` / `_gt` | Reads pending winnings and compares |
| `assert_round_phase_eq` | Reads and compares the current round phase |
| `cleanup` | Removes key pairs; stops network unless `KEEP_NETWORK=1` |
