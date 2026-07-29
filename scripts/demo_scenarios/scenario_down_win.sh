#!/usr/bin/env bash
#
# scenario_down_win.sh — Demo scenario: Down bet wins when XLM price falls.
#
# Flow:
#   initialize → mint (Alice, Bob) → create_round (Mode 0 Up/Down)
#   → Alice bets 400 vXLM DOWN → Bob bets 200 vXLM UP
#   → Oracle resolves at lower price → Alice claims winnings
#
# Assertions:
#   - Alice's pending winnings > 400 (principal + proportional share of Bob's loss)
#   - Bob's pending winnings == 0 (lost the bet)
#   - Alice's final balance > initial (profit realized)
#   - Round archive exists with Resolved status
#   - Protocol status transitions correctly
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCENARIO_NAME="Down-Win"

# Scenario-specific parameters
START_PRICE=15000000        # 1.5
RESOLVE_PRICE=13500000      # 1.35 — went DOWN
BET_AMOUNT=400000000        # 400 vXLM
BET_AMOUNT_BOB=200000000    # 200 vXLM

# shellcheck source=scripts/demo_scenarios/lib.sh
source "$SCRIPT_DIR/lib.sh"

# ── 1. Bootstrap ─────────────────────────────────────────────────────────────
preflight
start_network
create_identities
deploy_contract
initialize
mint_tokens

# ── 2. Check protocol status before round ───────────────────────────────────
step "Initial protocol status"
STATUS_BEFORE="$(read_only "$ADMIN_ID" get_protocol_status | tr -d '"')"
if [[ "$STATUS_BEFORE" == "ClaimsOnly" ]]; then
  ok "protocol status is ClaimsOnly before round creation"
else
  fail "expected ClaimsOnly before round, got $STATUS_BEFORE"
fi

# ── 3. Create Up/Down round ─────────────────────────────────────────────────
step "create_round (mode 0 = Up/Down)"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"
echo "active round: start_ledger=$ROUND_START_LEDGER end_ledger=$ROUND_END_LEDGER"

assert_round_phase_eq "1" "Betting phase"

STATUS_ACTIVE="$(read_only "$ADMIN_ID" get_protocol_status | tr -d '"')"
if [[ "$STATUS_ACTIVE" == "Active" ]]; then
  ok "protocol status transitions to Active after round creation"
else
  fail "expected Active after creation, got $STATUS_ACTIVE"
fi

# ── 4. Place bets (Alice bets DOWN, Bob bets UP) ───────────────────────────
step "Alice bets 400 vXLM DOWN"
ALICE_BET="$(invoke "$ALICE_ID" place_bet --user "$ALICE_ADDR" --amount "$BET_AMOUNT" --side Down)"
assert_event "$ALICE_BET" '"bet"},{"symbol":"placed"'

step "Bob bets 200 vXLM UP"
BOB_BET="$(invoke "$BOB_ID" place_bet --user "$BOB_ADDR" --amount "$BET_AMOUNT_BOB" --side Up)"
assert_event "$BOB_BET" '"bet"},{"symbol":"placed"'

# ── 5. Wait for round to end ────────────────────────────────────────────────
wait_for_round_end "$ROUND_END_LEDGER"

# ── 6. Resolve round with lower price (DOWN wins) ──────────────────────────
step "resolve_round (price went DOWN)"
RESOLVE_OUT="$(resolve_with_oracle "$RESOLVE_PRICE" "$ROUND_START_LEDGER" 1)"
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"summary"'
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"resolved"'

# ── 7. Assert end-state ─────────────────────────────────────────────────────
step "End-state assertions"

# Alice (DOWN bettor) should win
# Payout: 400 + (400/400 * 200) = 600
assert_pending_winnings_gt "$ALICE_ADDR" "$BET_AMOUNT" "Alice (DOWN) gets principal + profit"
assert_pending_winnings_eq "$BOB_ADDR" 0 "Bob (UP) gets 0 (lost)"

# Alice claims and balance grows
CLAIM_OUT="$(invoke "$ALICE_ID" claim_winnings --user "$ALICE_ADDR")"
assert_event "$CLAIM_OUT" '"claim"},{"symbol":"winnings"'
assert_balance_gt "$ALICE_ADDR" 1000000000 "Alice balance after claim > initial mint"

# Protocol status back to ClaimsOnly after resolution
STATUS_AFTER="$(read_only "$ADMIN_ID" get_protocol_status | tr -d '"')"
if [[ "$STATUS_AFTER" == "ClaimsOnly" ]]; then
  ok "protocol status returns to ClaimsOnly after resolution"
else
  fail "expected ClaimsOnly after resolve, got $STATUS_AFTER"
fi

# Pool stats function
POOL_STATS="$(read_only "$ADMIN_ID" get_round_pool_stats)"
if [[ -n "$POOL_STATS" && "$POOL_STATS" != "null" ]]; then
  ok "round pool stats returned (round recently resolved)"
else
  fail "expected pool stats after round creation"
fi

step "SUCCESS — Down-win scenario completed"
