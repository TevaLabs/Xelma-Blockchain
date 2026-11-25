#!/usr/bin/env bash
#
# scenario_multi_feed.sh — Demo scenario: Multi-feed oracle quorum settlement.
#
# Flow:
#   initialize → set_oracle_quorum_config → mint (Alice, Bob)
#   → create_round (Mode 0 Up/Down)
#   → Alice bets 500 vXLM UP → Bob bets 300 vXLM DOWN
#   → Wait for round end
#   → Oracle resolves with multi-feed payload (3 feeds, median calculation, outlier check)
#   → Alice claims winnings
#
# Assertions:
#   - multisum event emitted with survivor count >= quorum threshold
#   - Alice's pending winnings > 500 (UP won)
#   - Bob's pending winnings == 0
#   - Alice successfully claims winnings
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCENARIO_NAME="Multi-Feed-Quorum"

# Scenario-specific parameters
START_PRICE=15000000        # 1.5 (7 decimals)
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

# ── 2. Configure multi-feed quorum ───────────────────────────────────────────
step "set_oracle_quorum_config (min=3, quorum=3, outlier_bps=500)"
QUORUM_CFG="$(jq -nc '{min_observations: 3, quorum_threshold: 3, outlier_threshold_bps: 500}')"
invoke "$ADMIN_ID" set_oracle_quorum_config --cfg "$QUORUM_CFG"

# ── 3. Create Up/Down round ─────────────────────────────────────────────────
step "create_round (mode 0 = Up/Down)"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"
echo "active round: start_ledger=$ROUND_START_LEDGER end_ledger=$ROUND_END_LEDGER"

assert_round_phase_eq "1" "Betting phase"

# ── 4. Place bets ───────────────────────────────────────────────────────────
step "Alice bets 500 vXLM UP"
ALICE_BET="$(invoke "$ALICE_ID" place_bet --user "$ALICE_ADDR" --amount "$BET_AMOUNT" --side Up)"
assert_event "$ALICE_BET" '"bet"},{"symbol":"placed"'

step "Bob bets 300 vXLM DOWN"
BOB_BET="$(invoke "$BOB_ID" place_bet --user "$BOB_ADDR" --amount "$BET_AMOUNT_BOB" --side Down)"
assert_event "$BOB_BET" '"bet"},{"symbol":"placed"'

# ── 5. Wait for round to end ────────────────────────────────────────────────
wait_for_round_end "$ROUND_END_LEDGER"

# ── 6. Resolve round with multi-feed oracle ──────────────────────────────────
step "resolve_round_multi (3 feeds agree price went UP)"
RESOLVE_OUT="$(resolve_with_oracle_multi "$ROUND_START_LEDGER" 1)"
assert_event "$RESOLVE_OUT" '"oracle"},{"symbol":"multisum"'
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"resolved"'

# ── 7. Assert end-state ─────────────────────────────────────────────────────
step "End-state assertions"

ALICE_PENDING="$(read_only "$ALICE_ID" get_pending_winnings --user "$ALICE_ADDR")"
BOB_PENDING="$(read_only "$BOB_ID" get_pending_winnings --user "$BOB_ADDR")"
echo "pending winnings: Alice=$ALICE_PENDING Bob=$BOB_PENDING"

if [[ "$ALICE_PENDING" -gt "$BET_AMOUNT" ]]; then
  ok "Alice pending winnings ($ALICE_PENDING) > bet ($BET_AMOUNT)"
else
  fail "expected Alice pending > $BET_AMOUNT, got $ALICE_PENDING"
fi

if [[ "$BOB_PENDING" -eq 0 ]]; then
  ok "Bob pending winnings == 0"
else
  fail "expected Bob pending == 0, got $BOB_PENDING"
fi

step "Alice claims winnings"
CLAIM_OUT="$(invoke "$ALICE_ID" claim_winnings --user "$ALICE_ADDR")"
assert_event "$CLAIM_OUT" '"claim"},{"symbol":"winnings"'

ALICE_POST_BAL="$(read_only "$ALICE_ID" balance --user "$ALICE_ADDR")"
echo "Alice post-claim balance: $ALICE_POST_BAL"
if [[ "$ALICE_POST_BAL" -gt 1000000000 ]]; then
  ok "Alice final balance ($ALICE_POST_BAL) > initial 1000 vXLM"
else
  fail "Alice final balance not > 1000 vXLM"
fi
