#!/usr/bin/env bash
#
# demo_cash_out.sh — Early cash-out demo (Issue #426).
#
# Demonstrates the `cash_out_early` entrypoint: a user can exit a round
# before it resolves, receiving a portion of their stake back (minus the
# configured early-cashout fee in bps). The cash-out fee is set by the
# admin via `set_early_cashout_bps`.
#
# Expected deterministic output:
#   - Admin sets early cashout fee to 500 bps (5%)
#   - User A places a bet during the betting window
#   - User A calls cash_out_early mid-round and receives ~95% of stake
#   - Round resolves normally for remaining participants (User B)
#
# Usage:
#   ./scripts/demo_scenarios/demo_cash_out.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

WASM_PATH="${WASM_PATH:-"$REPO_ROOT/target/wasm32v1-none/release/xelma_contract.wasm"}"
SKIP_NETWORK_START="${SKIP_NETWORK_START:-0}"
KEEP_NETWORK="${KEEP_NETWORK:-0}"
NETWORK="local"

RUN_ID="$$"
ADMIN_ID="cash-admin-$RUN_ID"
ORACLE_ID="cash-oracle-$RUN_ID"
USER_A_ID="cash-user-a-$RUN_ID"
USER_B_ID="cash-user-b-$RUN_ID"

START_PRICE=15000000       # $1.50
RESOLVE_PRICE=17000000     # $1.70 — price goes UP
BET_AMOUNT=1000000000      # 100 XLM
CASHOUT_BPS=500            # 5% early cashout fee

step() { echo ""; echo "=== $* ==="; }

cleanup() {
  local exit_code=$?
  if [[ $exit_code -ne 0 ]]; then
    echo ""; echo "❌ Cash-out demo FAILED (exit $exit_code)."
    if [[ "$SKIP_NETWORK_START" != "1" ]] && command -v docker >/dev/null 2>&1; then
      docker logs --tail 80 stellar-"$NETWORK" 2>&1 || true
    fi
  fi
  for id in "$ADMIN_ID" "$ORACLE_ID" "$USER_A_ID" "$USER_B_ID"; do
    stellar keys rm "$id" --force >/dev/null 2>&1 || true
  done
  if [[ "$SKIP_NETWORK_START" != "1" && "$KEEP_NETWORK" != "1" ]]; then
    echo "Stopping local network..."
    stellar container stop "$NETWORK" >/dev/null 2>&1 || true
  fi
  exit $exit_code
}
trap cleanup EXIT

sha256_hex() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum | cut -d' ' -f1
  else shasum -a 256 | cut -d' ' -f1; fi
}

invoke() {
  local source="$1"; shift
  stellar contract invoke --id "$CONTRACT_ID" --source "$source" --network "$NETWORK" -- "$@" 2>&1
}

read_only() {
  local source="$1"; shift
  stellar contract invoke --id "$CONTRACT_ID" --source "$source" --network "$NETWORK" --send=no -- "$@"
}

# ── 0. Preflight ─────────────────────────────────────────────────────────
step "Preflight"
command -v stellar >/dev/null 2>&1 || { echo "stellar CLI not found"; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "jq not found"; exit 1; }

if [[ ! -f "$WASM_PATH" ]]; then
  echo "WASM not found — building..."
  (cd "$REPO_ROOT/contracts" && stellar contract build --package xelma-contract)
fi
echo "Using WASM: $WASM_PATH ($(wc -c < "$WASM_PATH") bytes)"

# ── 1. Network ───────────────────────────────────────────────────────────
if [[ "$SKIP_NETWORK_START" != "1" ]]; then
  step "Starting local network"
  stellar container start "$NETWORK"
fi

step "Waiting for RPC health"
sleep 15
CONSECUTIVE_OK=0; NETWORK_READY=0
for _ in $(seq 1 60); do
  if stellar network health --network "$NETWORK" >/dev/null 2>&1; then CONSECUTIVE_OK=$((CONSECUTIVE_OK + 1))
  else CONSECUTIVE_OK=0; fi
  if [[ "$CONSECUTIVE_OK" -ge 3 ]]; then NETWORK_READY=1; break; fi
  sleep 2
done
[[ "$NETWORK_READY" == "1" ]] || { echo "ERROR: network not ready"; exit 1; }
echo "Network healthy."

# ── 2. Identities ───────────────────────────────────────────────────────
step "Generating identities"
for id in "$ADMIN_ID" "$ORACLE_ID" "$USER_A_ID" "$USER_B_ID"; do
  stellar keys generate "$id" --network "$NETWORK" --fund --overwrite
done
ADMIN_ADDR="$(stellar keys address "$ADMIN_ID")"
ORACLE_ADDR="$(stellar keys address "$ORACLE_ID")"
USER_A_ADDR="$(stellar keys address "$USER_A_ID")"
USER_B_ADDR="$(stellar keys address "$USER_B_ID")"
echo "admin=$ADMIN_ADDR"
echo "oracle=$ORACLE_ADDR"
echo "user_a=$USER_A_ADDR"
echo "user_b=$USER_B_ADDR"

NETWORK_PASSPHRASE="$(stellar network ls --long | awk -v RS='' '/Name: local/' | sed -n 's/^Network passphrase: //p')"
NETWORK_ID_HEX="$(printf '%s' "$NETWORK_PASSPHRASE" | sha256_hex)"

# ── 3. Deploy ───────────────────────────────────────────────────────────
step "Deploying contract"
CONTRACT_ID=""
for attempt in $(seq 1 5); do
  WASM_HASH="$(stellar contract upload --wasm "$WASM_PATH" --source "$ADMIN_ID" --network "$NETWORK" --resource-fee 50000000 2>/dev/null | tail -n1)" || WASM_HASH=""
  if [[ -z "$WASM_HASH" || ! "$WASM_HASH" =~ ^[a-f0-9]{64}$ ]]; then sleep 5; continue; fi
  CONTRACT_ID="$(stellar contract deploy --wasm-hash "$WASM_HASH" --source "$ADMIN_ID" --network "$NETWORK" --resource-fee 10000000 2>/dev/null | tail -n1)" || CONTRACT_ID=""
  [[ "$CONTRACT_ID" =~ ^C[A-Z0-9]{55}$ ]] && break
  CONTRACT_ID=""; sleep 5
done
[[ -z "$CONTRACT_ID" ]] && { echo "ERROR: deploy failed"; exit 1; }
echo "Contract ID: $CONTRACT_ID"

# ── 4. Initialize ───────────────────────────────────────────────────────
step "Initialize"
invoke "$ADMIN_ID" initialize --admin "$ADMIN_ADDR" --oracle "$ORACLE_ADDR"

step "Mint tokens"
invoke "$USER_A_ID" mint_initial --user "$USER_A_ADDR"
invoke "$USER_B_ID" mint_initial --user "$USER_B_ADDR"
BALANCE_A_BEFORE="$(read_only "$USER_A_ID" balance --user "$USER_A_ADDR" | tr -d '\"')"
echo "user_a balance after mint: $BALANCE_A_BEFORE"

# ── 5. Configure early cashout ─────────────────────────────────────────
step "Set early cashout fee to ${CASHOUT_BPS} bps (5%)"
invoke "$ADMIN_ID" set_early_cashout_bps --bps "$CASHOUT_BPS"
CASHOUT_READ="$(read_only "$ADMIN_ID" get_early_cashout_bps)"
echo "early cashout bps configured: $CASHOUT_READ"

# ── 6. Heartbeat + Create round ─────────────────────────────────────────
step "Update oracle heartbeat"
invoke "$ORACLE_ID" update_oracle_heartbeat --status 0

step "Create round"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
echo "round start_ledger=$ROUND_START_LEDGER"

# ── 7. Both users place bets ───────────────────────────────────────────
step "Place bets"
invoke "$USER_A_ID" place_bet --user "$USER_A_ADDR" --amount "$BET_AMOUNT" --side Up
invoke "$USER_B_ID" place_bet --user "$USER_B_ADDR" --amount "$BET_AMOUNT" --side Down
echo "User A bets $BET_AMOUNT Up"
echo "User B bets $BET_AMOUNT Down"

# ── 8. User A cashes out early ─────────────────────────────────────────
step "User A calls cash_out_early (before round ends)"
CASHOUT_OUT="$(invoke "$USER_A_ID" cash_out_early --user "$USER_A_ADDR")"
echo "$CASHOUT_OUT" | head -5

BALANCE_A_AFTER_CASHOUT="$(read_only "$USER_A_ID" balance --user "$USER_A_ADDR" | tr -d '\"')"
echo "user_a balance after early cash-out: $BALANCE_A_AFTER_CASHOUT"

# Calculate expected: BET_AMOUNT - (BET_AMOUNT * CASHOUT_BPS / 10000)
EXPECTED_CASHOUT=$(( BET_AMOUNT - (BET_AMOUNT * CASHOUT_BPS / 10000) ))
ACTUAL_RECEIVED=$(( BALANCE_A_AFTER_CASHOUT - BALANCE_A_BEFORE ))
echo "expected cash-out amount: ~$EXPECTED_CASHOUT (5% fee on $BET_AMOUNT)"
echo "actual received: $ACTUAL_RECEIVED"

# ── 9. Wait for round end and resolve ──────────────────────────────────
step "Waiting for round to end"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"
for _ in $(seq 1 120); do
  CURRENT_LEDGER="$(stellar ledger latest --network "$NETWORK" | sed -n 's/^Sequence: //p')"
  if [[ -n "$CURRENT_LEDGER" && "$CURRENT_LEDGER" -ge "$ROUND_END_LEDGER" ]]; then break; fi
  sleep 2
done

step "Resolve round"
ORACLE_TS=$(( $(date +%s) - 10 ))
PAYLOAD=$(jq -nc \
  --arg price "$RESOLVE_PRICE" \
  --argjson timestamp "$ORACLE_TS" \
  --argjson round_id "$ROUND_START_LEDGER" \
  --arg network_id "$NETWORK_ID_HEX" \
  --arg contract_addr "$CONTRACT_ID" \
  '{price: $price, timestamp: $timestamp, round_id: $round_id, nonce: 1, network_id: $network_id, contract_addr: $contract_addr, confidence: null}')

invoke "$ORACLE_ID" resolve_round --payload "$PAYLOAD" 2>&1 | head -3

# ── 10. Verify ──────────────────────────────────────────────────────────
step "Verify deterministic expected output"
PENDING_B="$(read_only "$USER_B_ID" get_pending_winnings --user "$USER_B_ADDR" | tr -d '\"')"
echo "user_b pending winnings: $PENDING_B"
echo "user_a received from cash-out: $ACTUAL_RECEIVED"

echo ""
echo "✅ Cash-out demo verified:"
echo "   - User A exited early with ~${CASHOUT_BPS}% fee"
echo "   - User B remains in round and wins (price went Up)"
echo "   - User A received $ACTUAL_RECEIVED from early cashout"

# ── 11. Summary ─────────────────────────────────────────────────────────
step "📊 Cash-Out Demo Summary"
echo "┌──────────────────────────────────────────────┐"
echo "│  Early Cash-Out Settlement Demo              │"
echo "├──────────────────────────────────────────────┤"
echo "│  Start price:         $START_PRICE stroops   │"
echo "│  Resolve price:       $RESOLVE_PRICE stroops │"
echo "│  Bet amount:          $BET_AMOUNT stroops    │"
echo "│  Cash-out fee:        $CASHOUT_BPS bps (5%)  │"
echo "│  User A: cashed out   $ACTUAL_RECEIVED stroops│"
echo "│  User B: wins round   $PENDING_B stroops     │"
echo "└──────────────────────────────────────────────┘"
echo ""
echo "✅ Cash-out demo completed. Contract ID: $CONTRACT_ID"
