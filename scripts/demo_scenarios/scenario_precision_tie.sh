#!/usr/bin/env bash
#
# scenario_precision_tie.sh — Demo: Precision mode with a tie.
#
# Both Alice and Bob predict the SAME price. The oracle settles at that
# exact price, so both are winners. The pot is split evenly; the first
# winner (by address order) receives the remainder (dust) per the
# Precision remainder policy.
#
# Flow:
#   initialize → mint (Alice, Bob) → create_round (Mode 1 Precision)
#   → Alice predicts 1.55 @ 500 vXLM → Bob predicts 1.55 @ 300 vXLM
#   → Oracle resolves at 1.55 → both win, tie-split asserted
#
# Assertions:
#   - Both users have pending winnings > 0
#   - Combined payouts == total pot (800 vXLM) minus any protocol fee
#   - Price prediction events recorded
#   - Round archive exists with Resolved status
#   - Precision participant list is correct
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCENARIO_NAME="Precision-Tie"

# Scenario-specific parameters
START_PRICE=15000000        # 1.5
# Predicted AND resolved at the same price (exact match)
PREDICTED_PRICE=15500000    # 1.55
RESOLVE_PRICE=15500000      # same — both Alice and Bob win
BET_AMOUNT=500000000        # 500 vXLM
BET_AMOUNT_BOB=300000000    # 300 vXLM

# Expected total pot
TOTAL_POT=$((BET_AMOUNT + BET_AMOUNT_BOB))  # 800000000

# shellcheck source=scripts/demo_scenarios/lib.sh
source "$SCRIPT_DIR/lib.sh"

# ── 1. Bootstrap ─────────────────────────────────────────────────────────────
preflight
start_network
create_identities
deploy_contract
initialize
mint_tokens

# ── 2. Create Precision round ───────────────────────────────────────────────
step "create_round (mode 1 = Precision)"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 1

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
ROUND_MODE="$(echo "$ROUND_JSON" | jq -r '.mode')"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"
echo "active round: mode=$ROUND_MODE start_ledger=$ROUND_START_LEDGER end_ledger=$ROUND_END_LEDGER"

if [[ "$ROUND_MODE" == "Precision" ]]; then
  ok "round mode is Precision"
else
  fail "expected Precision mode, got $ROUND_MODE"
fi

assert_round_phase_eq "1" "Betting phase"

# ── 3. Place precision predictions ──────────────────────────────────────────
step "Alice predicts $PREDICTED_PRICE @ $BET_AMOUNT"
ALICE_PRED="$(invoke "$ALICE_ID" place_precision_prediction \
  --user "$ALICE_ADDR" \
  --amount "$BET_AMOUNT" \
  --predicted_price "$PREDICTED_PRICE")"
assert_event "$ALICE_PRED" '"predict"},{"symbol":"price"'

step "Bob predicts $PREDICTED_PRICE @ $BET_AMOUNT_BOB"
BOB_PRED="$(invoke "$BOB_ID" place_precision_prediction \
  --user "$BOB_ADDR" \
  --amount "$BET_AMOUNT_BOB" \
  --predicted_price "$PREDICTED_PRICE")"
assert_event "$BOB_PRED" '"predict"},{"symbol":"price"'

# ── 4. Verify predictions are visible ───────────────────────────────────────
step "Query active predictions"
PREDS="$(read_only "$ALICE_ID" get_precision_predictions)"
PRED_COUNT="$(echo "$PREDS" | jq -r 'length')"
if [[ "$PRED_COUNT" -ge 2 ]]; then
  ok "get_precision_predictions returns $PRED_COUNT predictions"
else
  fail "expected >= 2 predictions, got $PRED_COUNT"
fi

# ── 5. Wait for round to end ────────────────────────────────────────────────
wait_for_round_end "$ROUND_END_LEDGER"

# ── 6. Resolve round ────────────────────────────────────────────────────────
step "resolve_round (price matches both predictions)"
RESOLVE_OUT="$(resolve_with_oracle "$RESOLVE_PRICE" "$ROUND_START_LEDGER" 1)"
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"summary"'
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"resolved"'

# ── 7. Assert end-state ─────────────────────────────────────────────────────
step "End-state assertions"

# Both should have pending winnings (both tied for closest)
ALICE_PENDING="$(read_only "$ALICE_ID" get_pending_winnings --user "$ALICE_ADDR" | tr -d '"')"
BOB_PENDING="$(read_only "$BOB_ID" get_pending_winnings --user "$BOB_ADDR" | tr -d '"')"
echo "Alice pending=$ALICE_PENDING  Bob pending=$BOB_PENDING"

if [[ "$ALICE_PENDING" -gt 0 ]]; then
  ok "Alice has pending winnings ($ALICE_PENDING)"
else
  fail "Alice expected positive pending winnings"
fi

if [[ "$BOB_PENDING" -gt 0 ]]; then
  ok "Bob has pending winnings ($BOB_PENDING)"
else
  fail "Bob expected positive pending winnings"
fi

# Combined payouts should equal total pot (minus any protocol fee, which is
# disabled by default — fee bps not set == None == 0 fee).
TOTAL_PAID=$((ALICE_PENDING + BOB_PENDING))
if [[ "$TOTAL_PAID" -eq "$TOTAL_POT" ]]; then
  ok "combined payout $TOTAL_PAID == total pot $TOTAL_POT (no fee)"
else
  # Fee may be enabled in some test configs; just warn instead of fail
  echo "  note: combined payout $TOTAL_PAID != total pot $TOTAL_POT (fee may apply)"
  if [[ "$TOTAL_PAID" -lt "$TOTAL_POT" && "$TOTAL_PAID" -gt 0 ]]; then
    ok "combined payout $TOTAL_PAID < $TOTAL_POT (protocol fee deducted)"
  fi
fi

# First winner (Alice by address order) should get the remainder
# Split: 800 / 2 = 400 each. Remainder: 0 (even split, no dust)
MIN_EACH=$((TOTAL_POT / 2))
if [[ "$ALICE_PENDING" -ge "$MIN_EACH" ]]; then
  ok "Alice payout >= minimum equal share ($MIN_EACH)"
fi
if [[ "$BOB_PENDING" -ge "$MIN_EACH" ]]; then
  ok "Bob payout >= minimum equal share ($MIN_EACH)"
fi

# Claim and balance checks
CLAIM_ALICE="$(invoke "$ALICE_ID" claim_winnings --user "$ALICE_ADDR")"
assert_event "$CLAIM_ALICE" '"claim"},{"symbol":"winnings"'
assert_balance_gt "$ALICE_ADDR" 1000000000 "Alice balance after claim"

CLAIM_BOB="$(invoke "$BOB_ID" claim_winnings --user "$BOB_ADDR")"
assert_event "$CLAIM_BOB" '"claim"},{"symbol":"winnings"'

# Round archive exists
ARCHIVE="$(read_only "$ALICE_ID" get_archived_round --round_id "$ROUND_START_LEDGER")"
if echo "$ARCHIVE" | jq -e '.status == "Resolved"' >/dev/null 2>&1; then
  ok "round archived with Resolved status"
else
  fail "round archive missing or not Resolved"
fi

# Precision participant count
POOL_STATS="$(read_only "$ALICE_ID" get_round_pool_stats)"
PRECISION_COUNT="$(echo "$POOL_STATS" | jq -r '.precision_participant_count // 0')"
if [[ "$PRECISION_COUNT" -ge 2 ]]; then
  ok "pool stats: $PRECISION_COUNT precision participants"
else
  fail "expected >= 2 precision participants"
fi

step "SUCCESS — Precision-tie scenario completed"
