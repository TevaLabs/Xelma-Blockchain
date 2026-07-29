#!/usr/bin/env bash
#
# scenario_up_win.sh — Demo scenario: Up bet wins when XLM price rises.
#
# Flow:
#   initialize → mint (Alice, Bob) → create_round (Mode 0 Up/Down)
#   → Alice bets 500 vXLM UP → Bob bets 300 vXLM DOWN
#   → Oracle resolves at higher price → Alice claims winnings
#
# Assertions:
#   - Alice's pending winnings > 500 (principal + proportional share of Bob's loss)
#   - Bob's pending winnings == 0 (lost the bet)
#   - Alice's final balance > initial (profit realized)
#   - Round archive exists with Resolved status
#   - User stats recorded (Alice +1 win, Bob +1 loss)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCENARIO_NAME="Up-Win"

# Scenario-specific parameters
START_PRICE=15000000        # 1.5 (7 decimals)
RESOLVE_PRICE=16500000      # 1.65 — went UP
BET_AMOUNT=500000000        # 500 vXLM
BET_AMOUNT_BOB=300000000    # 300 vXLM

# shellcheck source=scripts/demo_scenarios/lib.sh
source "$SCRIPT_DIR/lib.sh"

# ── 1. Bootstrap ─────────────────────────────────────────────────────────────
preflight
start_network
create_identities
deploy_contract
initialize
mint_tokens

# ── 2. Create Up/Down round ─────────────────────────────────────────────────
step "create_round (mode 0 = Up/Down)"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"
echo "active round: start_ledger=$ROUND_START_LEDGER end_ledger=$ROUND_END_LEDGER"

assert_round_phase_eq "1" "Betting phase"

# ── 3. Place bets ───────────────────────────────────────────────────────────
step "Alice bets 500 vXLM UP"
ALICE_BET="$(invoke "$ALICE_ID" place_bet --user "$ALICE_ADDR" --amount "$BET_AMOUNT" --side Up)"
assert_event "$ALICE_BET" '"bet"},{"symbol":"placed"'

step "Bob bets 300 vXLM DOWN"
BOB_BET="$(invoke "$BOB_ID" place_bet --user "$BOB_ADDR" --amount "$BET_AMOUNT_BOB" --side Down)"
assert_event "$BOB_BET" '"bet"},{"symbol":"placed"'

# ── 4. Wait for round to end ────────────────────────────────────────────────
wait_for_round_end "$ROUND_END_LEDGER"

# ── 5. Resolve round with higher price (UP wins) ────────────────────────────
step "resolve_round (price went UP)"
RESOLVE_OUT="$(resolve_with_oracle "$RESOLVE_PRICE" "$ROUND_START_LEDGER" 1)"
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"summary"'
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"resolved"'

# ── 6. Assert end-state ─────────────────────────────────────────────────────
step "End-state assertions"

# Alice (UP bettor) should have pending winnings
# Payout formula: principal + (principal/winning_pool * losing_pool)
# Alice: 500 + (500/500 * 300) = 800
assert_pending_winnings_gt "$ALICE_ADDR" "$BET_AMOUNT" "Alice gets principal + profit"
assert_pending_winnings_eq "$BOB_ADDR" 0 "Bob gets 0 (lost)"

# Alice claims and total balance exceeds initial mint
CLAIM_OUT="$(invoke "$ALICE_ID" claim_winnings --user "$ALICE_ADDR")"
assert_event "$CLAIM_OUT" '"claim"},{"symbol":"winnings"'
assert_balance_gt "$ALICE_ADDR" 1000000000 "Alice balance after claim > initial mint"

# Bob claims (should be 0 since he lost)
BOB_CLAIM="$(invoke "$BOB_ID" claim_winnings --user "$BOB_ADDR")"
assert_pending_winnings_eq "$BOB_ADDR" 0 "Bob pending 0 after claim"

# Round archive exists
ARCHIVE="$(read_only "$ALICE_ID" get_archived_round --round_id "$ROUND_START_LEDGER")"
if echo "$ARCHIVE" | jq -e '.status == "Resolved"' >/dev/null 2>&1; then
  ok "round archived with Resolved status"
else
  fail "round archive missing or not Resolved"
fi

# User stats recorded
ALICE_STATS="$(read_only "$ALICE_ID" get_user_stats --user "$ALICE_ADDR")"
ALICE_WINS="$(echo "$ALICE_STATS" | jq -r '.total_wins')"
if [[ "$ALICE_WINS" -ge 1 ]]; then
  ok "Alice stats: total_wins=$ALICE_WINS"
else
  fail "Alice stats missing wins"
fi

BOB_STATS="$(read_only "$BOB_ID" get_user_stats --user "$BOB_ADDR")"
BOB_LOSSES="$(echo "$BOB_STATS" | jq -r '.total_losses')"
if [[ "$BOB_LOSSES" -ge 1 ]]; then
  ok "Bob stats: total_losses=$BOB_LOSSES"
else
  fail "Bob stats missing losses"
fi

step "SUCCESS — Up-win scenario completed"
