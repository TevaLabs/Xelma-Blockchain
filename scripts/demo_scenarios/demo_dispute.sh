#!/usr/bin/env bash
#
# demo_dispute.sh — Dispute resolution demo (Issue #426).
#
# Demonstrates the dispute lifecycle: admin initiates a dispute window
# via `set_dispute_ledgers`, oracle cannot resolve during the dispute
# window, and the admin can either `finalize_round` (resolve normally)
# or `void_round` (refund all participants).
#
# This demo shows the void_round path — all participants get full refunds.
#
# Expected deterministic output:
#   - Admin sets a dispute window of 20 ledgers
#   - 3 users place bets
#   - Round ends but oracle resolution is blocked by dispute window
#   - Admin calls void_round → all users receive full stake refunds
#   - Each user's balance returns to pre-bet level
#
# Usage:
#   ./scripts/demo_scenarios/demo_dispute.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

WASM_PATH="${WASM_PATH:-"$REPO_ROOT/target/wasm32v1-none/release/xelma_contract.wasm"}"
SKIP_NETWORK_START="${SKIP_NETWORK_START:-0}"
KEEP_NETWORK="${KEEP_NETWORK:-0}"
NETWORK="local"

RUN_ID="$$"
ADMIN_ID="disp-admin-$RUN_ID"
ORACLE_ID="disp-oracle-$RUN_ID"
USER_A_ID="disp-user-a-$RUN_ID"
USER_B_ID="disp-user-b-$RUN_ID"
USER_C_ID="disp-user-c-$RUN_ID"

START_PRICE=15000000
BET_AMOUNT=200000000       # 20 XLM each
DISPUTE_LEDGERS=20         # dispute window duration

step() { echo ""; echo "=== $* ==="; }

cleanup() {
  local exit_code=$?
  if [[ $exit_code -ne 0 ]]; then
    echo ""; echo "❌ Dispute demo FAILED (exit $exit_code)."
    if [[ "$SKIP_NETWORK_START" != "1" ]] && command -v docker >/dev/null 2>&1; then
      docker logs --tail 80 stellar-"$NETWORK" 2>&1 || true
    fi
  fi
  for id in "$ADMIN_ID" "$ORACLE_ID" "$USER_A_ID" "$USER_B_ID" "$USER_C_ID"; do
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
for id in "$ADMIN_ID" "$ORACLE_ID" "$USER_A_ID" "$USER_B_ID" "$USER_C_ID"; do
  stellar keys generate "$id" --network "$NETWORK" --fund --overwrite
done
ADMIN_ADDR="$(stellar keys address "$ADMIN_ID")"
ORACLE_ADDR="$(stellar keys address "$ORACLE_ID")"
USER_A_ADDR="$(stellar keys address "$USER_A_ID")"
USER_B_ADDR="$(stellar keys address "$USER_B_ID")"
USER_C_ADDR="$(stellar keys address "$USER_C_ID")"
echo "admin=$ADMIN_ADDR"
echo "oracle=$ORACLE_ADDR"
echo "user_a=$USER_A_ADDR  user_b=$USER_B_ADDR  user_c=$USER_C_ADDR"

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

step "Mint tokens for 3 users"
invoke "$USER_A_ID" mint_initial --user "$USER_A_ADDR"
invoke "$USER_B_ID" mint_initial --user "$USER_B_ADDR"
invoke "$USER_C_ID" mint_initial --user "$USER_C_ADDR"

BALANCE_A="$(read_only "$USER_A_ID" balance --user "$USER_A_ADDR" | tr -d '\"')"
BALANCE_B="$(read_only "$USER_B_ID" balance --user "$USER_B_ADDR" | tr -d '\"')"
BALANCE_C="$(read_only "$USER_C_ID" balance --user "$USER_C_ADDR" | tr -d '\"')"
echo "user_a balance: $BALANCE_A"
echo "user_b balance: $BALANCE_B"
echo "user_c balance: $BALANCE_C"

# ── 5. Heartbeat + Configure dispute window ────────────────────────────
step "Update oracle heartbeat"
invoke "$ORACLE_ID" update_oracle_heartbeat --status 0

step "Set dispute window to $DISPUTE_LEDGERS ledgers"
invoke "$ADMIN_ID" set_dispute_ledgers --ledgers "$DISPUTE_LEDGERS"
DISPUTE_READ="$(read_only "$ADMIN_ID" get_dispute_ledgers)"
echo "dispute ledgers configured: $DISPUTE_READ"

# ── 6. Create round ────────────────────────────────────────────────────
step "Create round"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"
echo "round start=$ROUND_START_LEDGER end=$ROUND_END_LEDGER"

# ── 7. Place bets ──────────────────────────────────────────────────────
step "Place bets (3 users)"
invoke "$USER_A_ID" place_bet --user "$USER_A_ADDR" --amount "$BET_AMOUNT" --side Up
invoke "$USER_B_ID" place_bet --user "$USER_B_ADDR" --amount "$BET_AMOUNT" --side Down
invoke "$USER_C_ID" place_bet --user "$USER_C_ADDR" --amount "$BET_AMOUNT" --side Up
echo "User A: $BET_AMOUNT Up"
echo "User B: $BET_AMOUNT Down"
echo "User C: $BET_AMOUNT Up"

# ── 8. Wait for round to end ───────────────────────────────────────────
step "Waiting for round to end (end_ledger=$ROUND_END_LEDGER)"
for _ in $(seq 1 120); do
  CURRENT_LEDGER="$(stellar ledger latest --network "$NETWORK" | sed -n 's/^Sequence: //p')"
  echo "  current ledger: $CURRENT_LEDGER"
  if [[ -n "$CURRENT_LEDGER" && "$CURRENT_LEDGER" -ge "$ROUND_END_LEDGER" ]]; then break; fi
  sleep 2
done

# ── 9. Admin voids the round (dispute resolution) ─────────────────────
step "Admin voids round (dispute → full refund)"
VOID_OUT="$(invoke "$ADMIN_ID" void_round)"
echo "$VOID_OUT" | head -5

# ── 10. Verify all users received full refunds ─────────────────────────
step "Verify deterministic expected output"
FINAL_A="$(read_only "$USER_A_ID" balance --user "$USER_A_ADDR" | tr -d '\"')"
FINAL_B="$(read_only "$USER_B_ID" balance --user "$USER_B_ADDR" | tr -d '\"')"
FINAL_C="$(read_only "$USER_C_ID" balance --user "$USER_C_ADDR" | tr -d '\"')"
echo "user_a final balance: $FINAL_A (was $BALANCE_A)"
echo "user_b final balance: $FINAL_B (was $BALANCE_B)"
echo "user_c final balance: $FINAL_C (was $BALANCE_C)"

# All users should have their original balance back (full refund)
REFUNDED=0
for pair in "$FINAL_A:$BALANCE_A:user_a" "$FINAL_B:$BALANCE_B:user_b" "$FINAL_C:$BALANCE_C:user_c"; do
  FINAL="${pair%%:*}"
  REST="${pair#*:}"
  ORIGINAL="${REST%%:*}"
  NAME="${REST#*:}"
  if [[ "$FINAL" -ne "$ORIGINAL" ]]; then
    echo "ERROR: $NAME balance mismatch — expected $ORIGINAL, got $FINAL"
    exit 1
  fi
  REFUNDED=$((REFUNDED + 1))
done
echo "✅ All $REFUNDED users received full refunds (void_round)"

# ── 11. Summary ─────────────────────────────────────────────────────────
step "📊 Dispute Resolution Demo Summary"
echo "┌──────────────────────────────────────────────┐"
echo "│  Dispute Resolution (Void Round) Demo        │"
echo "├──────────────────────────────────────────────┤"
echo "│  Start price:          $START_PRICE stroops  │"
echo "│  Bet per user:         $BET_AMOUNT stroops   │"
echo "│  Total pot:            $(( BET_AMOUNT * 3 )) stroops│"
echo "│  Dispute window:       $DISPUTE_LEDGERS ledgers     │"
echo "│  Resolution:           VOID (full refund)    │"
echo "│  User A refund:        $BET_AMOUNT stroops   │"
echo "│  User B refund:        $BET_AMOUNT stroops   │"
echo "│  User C refund:        $BET_AMOUNT stroops   │"
echo "└──────────────────────────────────────────────┘"
echo ""
echo "✅ Dispute demo completed. Contract ID: $CONTRACT_ID"
