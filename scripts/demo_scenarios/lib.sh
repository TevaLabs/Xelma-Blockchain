#!/usr/bin/env bash
#
# lib.sh — Shared helpers for demo scenario scripts.
#
# Source this file from each scenario script rather than duplicating the
# bootstrap, deploy, ledger-wait, and cleanup logic.
#
# Required environment (set by the caller or run_all.sh):
#   SCENARIO_NAME  — human-readable label for logging
#   WASM_PATH      — path to the compiled contract WASM
#
# Exports after setup (consumers use these):
#   ADMIN_ID, ORACLE_ID, ALICE_ID, BOB_ID
#   ADMIN_ADDR, ORACLE_ADDR, ALICE_ADDR, BOB_ADDR
#   CONTRACT_ID, NETWORK, NETWORK_ID_HEX
#   START_PRICE, BET_AMOUNT — scenario-specific values
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

CONTRACT_DIR="$REPO_ROOT/contracts"
DEMO_DIR="$SCRIPT_DIR"

# ── Defaults ─────────────────────────────────────────────────────────────────
WASM_PATH="${WASM_PATH:-"$REPO_ROOT/target/wasm32v1-none/release/xelma_contract.wasm"}"
SKIP_NETWORK_START="${SKIP_NETWORK_START:-0}"
KEEP_NETWORK="${KEEP_NETWORK:-0}"
NETWORK="${NETWORK:-local}"
RUN_ID="demo-$$"
SCENARIO_NAME="${SCENARIO_NAME:-unnamed}"

ADMIN_ID="demo-admin-$RUN_ID"
ORACLE_ID="demo-oracle-$RUN_ID"
ALICE_ID="demo-alice-$RUN_ID"
BOB_ID="demo-bob-$RUN_ID"

# Scenario parameters (overridable by each scenario before sourcing lib.sh)
START_PRICE="${START_PRICE:-15000000}"    # default 1.5
BET_AMOUNT="${BET_AMOUNT:-500000000}"     # default 500 vXLM
BET_AMOUNT_BOB="${BET_AMOUNT_BOB:-300000000}"  # default 300 vXLM

PASS=0
FAIL=0

# ── Helpers ──────────────────────────────────────────────────────────────────
step()  { echo ""; echo "◆ $SCENARIO_NAME :: $*"; }
ok()    { echo "  ✓ $*"; PASS=$((PASS + 1)); }
fail()  { echo "  ✘ $*"; FAIL=$((FAIL + 1)); }

sha256_hex() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | cut -d' ' -f1
  else
    shasum -a 256 | cut -d' ' -f1
  fi
}

invoke() {
  local source="$1"; shift
  stellar contract invoke \
    --id "$CONTRACT_ID" --source "$source" --network "$NETWORK" -- "$@" 2>&1
}

read_only() {
  local source="$1"; shift
  stellar contract invoke \
    --id "$CONTRACT_ID" --source "$source" --network "$NETWORK" --send=no -- "$@"
}

assert_event() {
  local output="$1" expected="$2"
  if echo "$output" | grep -qF "$expected"; then
    ok "event '$expected' found"
  else
    fail "expected event '$expected' not found in output"
  fi
}

assert_balance_gt() {
  local user="$1" min="$2" label="${3:-}"
  local bal
  bal="$(read_only "$ALICE_ID" balance --user "$user" | tr -d '"')"
  if [[ "$bal" -gt "$min" ]]; then
    ok "balance($user)=$bal > $min ${label:+($label)}"
  else
    fail "balance($user)=$bal not greater than $min ${label:+($label)}"
  fi
}

assert_balance_eq() {
  local user="$1" expected="$2" label="${3:-}"
  local bal
  bal="$(read_only "$ALICE_ID" balance --user "$user" | tr -d '"')"
  if [[ "$bal" -eq "$expected" ]]; then
    ok "balance($user)=$bal == $expected ${label:+($label)}"
  else
    fail "balance($user)=$bal != $expected ${label:+($label)}"
  fi
}

assert_pending_winnings_gt() {
  local user="$1" min="$2" label="${3:-}"
  local pending
  pending="$(read_only "$ALICE_ID" get_pending_winnings --user "$user" | tr -d '"')"
  if [[ "$pending" -gt "$min" ]]; then
    ok "pending($user)=$pending > $min ${label:+($label)}"
  else
    fail "pending($user)=$pending not greater than $min ${label:+($label)}"
  fi
}

assert_pending_winnings_eq() {
  local user="$1" expected="$2" label="${3:-}"
  local pending
  pending="$(read_only "$ALICE_ID" get_pending_winnings --user "$user" | tr -d '"')"
  if [[ "$pending" -eq "$expected" ]]; then
    ok "pending($user)=$pending == $expected ${label:+($label)}"
  else
    fail "pending($user)=$pending != $expected ${label:+($label)}"
  fi
}

assert_round_phase_eq() {
  local expected="$1" label="${2:-}"
  local phase
  phase="$(read_only "$ALICE_ID" get_round_phase | tr -d '"')"
  if [[ "$phase" == "$expected" ]]; then
    ok "round_phase=$phase == $expected ${label:+($label)}"
  else
    fail "round_phase=$phase != $expected ${label:+($label)}"
  fi
}

assert_no_active_round() {
  local out
  out="$(read_only "$ALICE_ID" get_active_round | tr -d '\n')"
  if [[ "$out" == "null" || -z "$out" ]]; then
    ok "no active round (expected)"
  else
    fail "expected no active round but got: $out"
  fi
}

# ── Lifecycle ────────────────────────────────────────────────────────────────

preflight() {
  step "Preflight"
  command -v stellar >/dev/null 2>&1 || { echo "stellar CLI not found"; exit 1; }
  command -v jq >/dev/null 2>&1 || { echo "jq not found"; exit 1; }

  if [[ ! -f "$WASM_PATH" ]]; then
    echo "WASM not found at $WASM_PATH — building..."
    (cd "$CONTRACT_DIR" && stellar contract build --package xelma-contract)
    if [[ ! -f "$WASM_PATH" ]]; then
      echo "ERROR: build did not produce $WASM_PATH"; exit 1
    fi
  fi
  echo "WASM: $WASM_PATH ($(wc -c < "$WASM_PATH") bytes)"
}

start_network() {
  if [[ "$SKIP_NETWORK_START" != "1" ]]; then
    step "Starting local Soroban network"
    stellar container start "$NETWORK"
    sleep 15
    local ready=0
    for _ in $(seq 1 60); do
      if stellar network health --network "$NETWORK" >/dev/null 2>&1; then
        ready=1; break
      fi
      sleep 2
    done
    if [[ "$ready" != "1" ]]; then
      echo "ERROR: local network did not become healthy"; exit 1
    fi
    echo "Network healthy."
  fi
}

create_identities() {
  step "Generating identities"
  for id in "$ADMIN_ID" "$ORACLE_ID" "$ALICE_ID" "$BOB_ID"; do
    stellar keys generate "$id" --network "$NETWORK" --fund --overwrite
  done
  ADMIN_ADDR="$(stellar keys address "$ADMIN_ID")"
  ORACLE_ADDR="$(stellar keys address "$ORACLE_ID")"
  ALICE_ADDR="$(stellar keys address "$ALICE_ID")"
  BOB_ADDR="$(stellar keys address "$BOB_ID")"
  echo "admin=$ADMIN_ADDR  oracle=$ORACLE_ADDR  alice=$ALICE_ADDR  bob=$BOB_ADDR"

  NETWORK_PASSPHRASE="$(stellar network ls --long | awk -v RS='' '/Name: local/' | sed -n 's/^Network passphrase: //p')"
  NETWORK_ID_HEX="$(printf '%s' "$NETWORK_PASSPHRASE" | sha256_hex)"
  echo "passphrase: $NETWORK_PASSPHRASE"
}

deploy_contract() {
  step "Deploying contract"
  CONTRACT_ID=""
  for attempt in $(seq 1 5); do
    if CONTRACT_ID="$(stellar contract deploy --wasm "$WASM_PATH" --source "$ADMIN_ID" --network "$NETWORK" -- | tail -n1)" \
        && [[ "$CONTRACT_ID" =~ ^C[A-Z0-9]{55}$ ]]; then
      break
    fi
    echo "Deploy attempt $attempt failed, retrying in 5s..."
    CONTRACT_ID=""
    sleep 5
  done
  if [[ -z "$CONTRACT_ID" ]]; then
    echo "ERROR: could not deploy contract"; exit 1
  fi
  echo "Contract ID: $CONTRACT_ID"
}

initialize() {
  step "initialize"
  invoke "$ADMIN_ID" initialize --admin "$ADMIN_ADDR" --oracle "$ORACLE_ADDR"
}

mint_tokens() {
  step "mint_initial"
  local out
  out="$(invoke "$ALICE_ID" mint_initial --user "$ALICE_ADDR")"
  assert_event "$out" '"mint"},{"symbol":"initial"' || true
  out="$(invoke "$BOB_ID" mint_initial --user "$BOB_ADDR")"
  assert_event "$out" '"mint"},{"symbol":"initial"' || true
}

# Wait for the current round to pass its end_ledger.
wait_for_round_end() {
  local end_ledger="$1"
  step "Waiting for ledger >= $end_ledger"
  for _ in $(seq 1 90); do
    local current
    current="$(stellar ledger latest --network "$NETWORK" | sed -n 's/^Sequence: //p')"
    if [[ -n "$current" && "$current" -ge "$end_ledger" ]]; then
      echo "  reached ledger $current"
      return
    fi
    sleep 2
  done
  echo "ERROR: timed out waiting for ledger $end_ledger"; exit 1
}

resolve_with_oracle() {
  local final_price="$1" round_start_ledger="$2" nonce="${3:-1}"
  local ts
  ts=$(( $(date +%s) - 10 ))
  local payload
  payload="$(jq -nc \
    --arg price "$final_price" \
    --argjson timestamp "$ts" \
    --argjson round_id "$round_start_ledger" \
    --arg network_id "$NETWORK_ID_HEX" \
    --arg contract_addr "$CONTRACT_ID" \
    '{price: $price, timestamp: $timestamp, round_id: $round_id, nonce: $nonce, network_id: $network_id, contract_addr: $contract_addr, confidence: null}')"
  echo "oracle payload: $payload"
  invoke "$ORACLE_ID" resolve_round --payload "$payload"
}

cleanup() {
  local exit_code=$?
  if [[ $exit_code -ne 0 ]]; then
    echo ""
    echo "❌ $SCENARIO_NAME FAILED (exit $exit_code)."
  fi

  for id in "$ADMIN_ID" "$ORACLE_ID" "$ALICE_ID" "$BOB_ID"; do
    stellar keys rm "$id" --force >/dev/null 2>&1 || true
  done

  if [[ "$SKIP_NETWORK_START" != "1" && "$KEEP_NETWORK" != "1" ]]; then
    echo "Stopping local network container..."
    stellar container stop "$NETWORK" >/dev/null 2>&1 || true
  fi

  echo ""
  echo "════════════════════════════════════════"
  echo "  $SCENARIO_NAME: $PASS passed, $FAIL failed"
  echo "════════════════════════════════════════"

  if [[ $exit_code -ne 0 || $FAIL -gt 0 ]]; then
    exit 1
  fi
  exit 0
}

trap cleanup EXIT
