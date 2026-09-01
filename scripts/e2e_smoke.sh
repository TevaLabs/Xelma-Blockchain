#!/usr/bin/env bash
#
# e2e_smoke.sh — Local Soroban RPC smoke test for the full round lifecycle.
#
# Unit tests under contracts/src/tests/ exercise contract logic in-process via
# `soroban_sdk::testutils` and never touch a real RPC, transaction signing, or
# wasm-validation path. This script deploys the *actual compiled WASM* to a
# real local Soroban network and drives one full round through:
#
#   initialize -> mint_initial -> create_round -> place_bet -> resolve_round
#   -> claim_winnings
#
# asserting balances and on-chain events at each step. It exists to catch
# deploy/auth/RPC-integration failures that in-memory unit tests cannot see
# (see issue #247) — e.g. it caught that `wasm32-unknown-unknown` builds on
# modern Rust toolchains enable wasm `reference-types` by default, producing
# a WASM module the Soroban host rejects at deploy time (see COMPATIBILITY
# note in contracts build config / CI for the fix: build with the
# `wasm32v1-none` target instead).
#
# Usage:
#   ./scripts/e2e_smoke.sh
#
# Environment variables:
#   WASM_PATH            Path to the compiled contract WASM to deploy.
#                         Default: target/wasm32v1-none/release/xelma_contract.wasm
#                         Built automatically with `stellar contract build` if
#                         it doesn't already exist.
#   SKIP_NETWORK_START    If "1", assume a `local` network container is
#                         already running instead of starting/stopping one.
#                         Useful for iterating locally without repeatedly
#                         paying container-startup cost.
#   KEEP_NETWORK          If "1", leave the local network container running
#                         after the script exits (implies no `container stop`).
#
# Prerequisites: stellar CLI (>=22, with `stellar container` support), Docker
# (running), jq.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

WASM_PATH="${WASM_PATH:-"$REPO_ROOT/target/wasm32v1-none/release/xelma_contract.wasm"}"
SKIP_NETWORK_START="${SKIP_NETWORK_START:-0}"
KEEP_NETWORK="${KEEP_NETWORK:-0}"
NETWORK="local"

RUN_ID="$$"
ADMIN_ID="e2e-admin-$RUN_ID"
ORACLE_ID="e2e-oracle-$RUN_ID"
USER_ID="e2e-user-$RUN_ID"

START_PRICE=15000000    # 1.5 (7 decimals), arbitrary
RESOLVE_PRICE=16500000  # price goes up -> the "Up" bet below must win
BET_AMOUNT=500000000

step() { echo ""; echo "=== $* ==="; }

cleanup() {
  local exit_code=$?
  if [[ $exit_code -ne 0 ]]; then
    echo ""
    echo "❌ e2e smoke test FAILED (exit $exit_code)."
    if [[ "$SKIP_NETWORK_START" != "1" ]] && command -v docker >/dev/null 2>&1; then
      echo "--- recent local network container logs ---"
      # `stellar container logs` follows indefinitely like `docker logs -f`
      # with no bounded/non-follow flag, so it can never be used here.
      docker logs --tail 100 stellar-"$NETWORK" 2>&1 || true
    fi
  fi

  for id in "$ADMIN_ID" "$ORACLE_ID" "$USER_ID"; do
    stellar keys rm "$id" --force >/dev/null 2>&1 || true
  done

  if [[ "$SKIP_NETWORK_START" != "1" && "$KEEP_NETWORK" != "1" ]]; then
    echo "Stopping local network container..."
    stellar container stop "$NETWORK" >/dev/null 2>&1 || true
  fi

  exit $exit_code
}
trap cleanup EXIT

sha256_hex() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | cut -d' ' -f1
  else
    shasum -a 256 | cut -d' ' -f1
  fi
}

invoke() {
  local source="$1"
  shift
  # Event log lines (the "📅 ... Event: ..." lines) are written to stderr;
  # only the decoded return value goes to stdout. Merge both here so event
  # assertions on the captured output work, and still echo everything live.
  stellar contract invoke --id "$CONTRACT_ID" --source "$source" --network "$NETWORK" -- "$@" 2>&1
}

read_only() {
  local source="$1"
  shift
  stellar contract invoke --id "$CONTRACT_ID" --source "$source" --network "$NETWORK" --send=no -- "$@"
}

# ── 0. Preflight ─────────────────────────────────────────────────────────
step "Preflight"
command -v stellar >/dev/null 2>&1 || { echo "stellar CLI not found in PATH"; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "jq not found in PATH"; exit 1; }

if [[ ! -f "$WASM_PATH" ]]; then
  echo "WASM not found at $WASM_PATH — building with 'stellar contract build'..."
  (cd "$REPO_ROOT/contracts" && stellar contract build --package xelma-contract)
  if [[ ! -f "$WASM_PATH" ]]; then
    echo "ERROR: build did not produce $WASM_PATH"
    exit 1
  fi
fi
echo "Using WASM: $WASM_PATH ($(wc -c < "$WASM_PATH") bytes)"

# ── 1. Start local network ───────────────────────────────────────────────
if [[ "$SKIP_NETWORK_START" != "1" ]]; then
  step "Starting local Soroban network container"
  stellar container start "$NETWORK"
fi

step "Waiting for RPC health"
# The container's RPC port goes through a rough startup sequence (connection
# reset -> 502 -> "data stores are not initialized" -> healthy) and can even
# report a single spurious "Healthy" in the middle of that window before
# settling down, which is enough to break the very next real request
# (funding/deploy). A fixed warm-up delay plus several *consecutive*
# successful checks avoids racing that transient window.
sleep 15
CONSECUTIVE_OK=0
NETWORK_READY=0
for _ in $(seq 1 60); do
  if stellar network health --network "$NETWORK" >/dev/null 2>&1; then
    CONSECUTIVE_OK=$((CONSECUTIVE_OK + 1))
  else
    CONSECUTIVE_OK=0
  fi
  if [[ "$CONSECUTIVE_OK" -ge 3 ]]; then
    NETWORK_READY=1
    break
  fi
  sleep 2
done
if [[ "$NETWORK_READY" != "1" ]]; then
  echo "ERROR: local network did not become healthy in time"
  exit 1
fi
echo "Network healthy."

# ── 2. Identities ────────────────────────────────────────────────────────
step "Generating and funding identities"
stellar keys generate "$ADMIN_ID" --network "$NETWORK" --fund --overwrite
stellar keys generate "$ORACLE_ID" --network "$NETWORK" --fund --overwrite
stellar keys generate "$USER_ID" --network "$NETWORK" --fund --overwrite

ADMIN_ADDR="$(stellar keys address "$ADMIN_ID")"
ORACLE_ADDR="$(stellar keys address "$ORACLE_ID")"
USER_ADDR="$(stellar keys address "$USER_ID")"
echo "admin=$ADMIN_ADDR"
echo "oracle=$ORACLE_ADDR"
echo "user=$USER_ADDR"

NETWORK_PASSPHRASE="$(stellar network ls --long | awk -v RS='' '/Name: local/' | sed -n 's/^Network passphrase: //p')"
if [[ -z "$NETWORK_PASSPHRASE" ]]; then
  echo "ERROR: could not resolve passphrase for network '$NETWORK'"
  exit 1
fi
NETWORK_ID_HEX="$(printf '%s' "$NETWORK_PASSPHRASE" | sha256_hex)"
echo "network passphrase: $NETWORK_PASSPHRASE"
echo "network_id (sha256): $NETWORK_ID_HEX"

# ── 3. Deploy ────────────────────────────────────────────────────────────
step "Deploying contract"
# The container reports RPC health before its soroban network resource-config
# upgrade has necessarily finished applying, which can make this first
# on-chain write fail with a transient `Budget/ExceededLimit` error. Retry
# this specific step a few times rather than chasing longer fixed sleeps.
#
# Split deploy into upload + deploy (two transactions) so the large WASM
# upload (≈160–190 KB) doesn't share its instruction budget with the
# contract instantiation.  Each step gets its own resource budget.
CONTRACT_ID=""
for attempt in $(seq 1 5); do
  WASM_HASH="$(stellar contract upload --wasm "$WASM_PATH" --source "$ADMIN_ID" --network "$NETWORK" --resource-fee 50000000 2>/dev/null | tail -n1)" || WASM_HASH=""
  if [[ -z "$WASM_HASH" || ! "$WASM_HASH" =~ ^[a-f0-9]{64}$ ]]; then
    echo "Upload attempt $attempt failed (got: '$WASM_HASH'), retrying in 5s..."
    WASM_HASH=""
    sleep 5
    continue
  fi
  echo "WASM hash: $WASM_HASH"
  CONTRACT_ID="$(stellar contract deploy --wasm-hash "$WASM_HASH" --source "$ADMIN_ID" --network "$NETWORK" --resource-fee 10000000 2>/dev/null | tail -n1)" || CONTRACT_ID=""
  if [[ "$CONTRACT_ID" =~ ^C[A-Z0-9]{55}$ ]]; then
    break
  fi
  echo "Deploy attempt $attempt failed (got: '$CONTRACT_ID'), retrying in 5s..."
  CONTRACT_ID=""
  WASM_HASH=""
  sleep 5
done
if [[ -z "$CONTRACT_ID" ]]; then
  echo "ERROR: could not deploy contract after retries"
  exit 1
fi
echo "Contract ID: $CONTRACT_ID"

# ── 4. initialize -> mint -> create_round -> place_bet ──────────────────
step "initialize"
invoke "$ADMIN_ID" initialize --admin "$ADMIN_ADDR" --oracle "$ORACLE_ADDR"

step "mint_initial"
MINT_OUT="$(invoke "$USER_ID" mint_initial --user "$USER_ADDR")"
echo "$MINT_OUT" | grep -q '"symbol":"mint"' || { echo "ERROR: expected mint event not found"; exit 1; }
BALANCE_AFTER_MINT="$(read_only "$USER_ID" balance --user "$USER_ADDR" | tr -d '"')"
echo "balance after mint: $BALANCE_AFTER_MINT"
if [[ "$BALANCE_AFTER_MINT" -le 0 ]]; then
  echo "ERROR: expected positive balance after mint_initial"
  exit 1
fi

step "create_round"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
echo "active round: $ROUND_JSON"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"
if [[ -z "$ROUND_START_LEDGER" || "$ROUND_START_LEDGER" == "null" ]]; then
  echo "ERROR: no active round found after create_round"
  exit 1
fi
echo "round start_ledger=$ROUND_START_LEDGER end_ledger=$ROUND_END_LEDGER"

step "place_bet"
BET_OUT="$(invoke "$USER_ID" place_bet --user "$USER_ADDR" --amount "$BET_AMOUNT" --side Up)"
echo "$BET_OUT" | grep -q '"symbol":"bet"' || { echo "ERROR: expected bet event not found"; exit 1; }

# ── 5. Wait for the round to end, then resolve via the oracle ───────────
step "Waiting for round to reach end_ledger=$ROUND_END_LEDGER"
for _ in $(seq 1 120); do
  CURRENT_LEDGER="$(stellar ledger latest --network "$NETWORK" | sed -n 's/^Sequence: //p')"
  echo "  current ledger: $CURRENT_LEDGER"
  if [[ -n "$CURRENT_LEDGER" && "$CURRENT_LEDGER" -ge "$ROUND_END_LEDGER" ]]; then
    break
  fi
  sleep 2
done
if [[ -z "${CURRENT_LEDGER:-}" || "$CURRENT_LEDGER" -lt "$ROUND_END_LEDGER" ]]; then
  echo "ERROR: timed out waiting for ledger to reach $ROUND_END_LEDGER"
  exit 1
fi

step "resolve_round"
# Timestamp uses a small backdate margin: the RPC's simulated "current time"
# reflects the last *closed* ledger, which can lag a locally-read wall clock
# by a second or two on a fast-closing local network, otherwise tripping the
# contract's "future oracle data" rejection.
ORACLE_TS=$(( $(date +%s) - 10 ))
PAYLOAD=$(jq -nc \
  --arg price "$RESOLVE_PRICE" \
  --argjson timestamp "$ORACLE_TS" \
  --argjson round_id "$ROUND_START_LEDGER" \
  --arg network_id "$NETWORK_ID_HEX" \
  --arg contract_addr "$CONTRACT_ID" \
  '{price: $price, timestamp: $timestamp, round_id: $round_id, nonce: 1, network_id: $network_id, contract_addr: $contract_addr, confidence: null}')
echo "oracle payload: $PAYLOAD"

RESOLVE_OUT="$(invoke "$ORACLE_ID" resolve_round --payload "$PAYLOAD")"
echo "$RESOLVE_OUT" | grep -q '"round"},{"symbol":"summary"' || {
  echo "ERROR: expected canonical (round, summary) event not found in resolve_round output"
  exit 1
}
echo "$RESOLVE_OUT" | grep -q '"round"},{"symbol":"resolved"' || {
  echo "ERROR: expected (round, resolved) event not found in resolve_round output"
  exit 1
}

# ── 6. Claim and assert balances ─────────────────────────────────────────
step "claim_winnings"
PENDING="$(read_only "$USER_ID" get_pending_winnings --user "$USER_ADDR" | tr -d '"')"
echo "pending winnings: $PENDING"
if [[ "$PENDING" -le 0 ]]; then
  echo "ERROR: expected positive pending winnings for the winning Up bet"
  exit 1
fi

CLAIM_OUT="$(invoke "$USER_ID" claim_winnings --user "$USER_ADDR")"
echo "$CLAIM_OUT" | grep -q '"claim"},{"symbol":"winnings"' || {
  echo "ERROR: expected (claim, winnings) event not found in claim_winnings output"
  exit 1
}

FINAL_BALANCE="$(read_only "$USER_ID" balance --user "$USER_ADDR" | tr -d '"')"
echo "final balance: $FINAL_BALANCE"
if [[ "$FINAL_BALANCE" -lt "$BALANCE_AFTER_MINT" ]]; then
  echo "ERROR: final balance ($FINAL_BALANCE) is lower than post-mint balance ($BALANCE_AFTER_MINT) after a winning claim"
  exit 1
fi

step "SUCCESS"
echo "Full lifecycle (initialize -> mint -> create_round -> place_bet -> resolve_round -> claim_winnings)"
echo "completed against a real local Soroban RPC. Contract ID: $CONTRACT_ID"
