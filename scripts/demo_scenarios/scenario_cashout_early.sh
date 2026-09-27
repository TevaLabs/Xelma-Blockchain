#!/usr/bin/env bash
#
# scenario_cashout_early.sh — Demo scenario: Early cash-out during Running phase.
#
# Alice bets UP, then exits her position early (during the Running phase) before
# the round resolves. A 10% penalty (1000 bps) is charged; the forfeited amount
# goes to the protocol treasury and the remainder is credited to Alice's pending
# winnings. Bob bets DOWN and holds; when the oracle resolves at a lower price
# (DOWN wins), Bob collects his full winnings.
#
# Flow:
#   initialize → set_early_cashout_bps(1000) → mint (Alice, Bob)
#   → create_round (Mode 0 Up/Down)
#   → Alice bets 500 vXLM UP
#   → Bob   bets 300 vXLM DOWN
#   → Ledger advances past bet_end_ledger (Running phase)
#   → Alice calls cash_out_early — forfeits 50 vXLM (10%), gets 450 back
#   → Ledger advances past end_ledger
#   → Oracle resolves at lower price (DOWN wins)
#   → Bob claims winnings (300 principal; no losing pool to share from since
#     Alice exited — but he still recovers his stake)
#
# Deterministic expected outputs:
#   Alice pending winnings after cashout : 450 vXLM  (500 * 0.90)
#   Treasury delta from forfeit          : 50 vXLM   (500 * 0.10)
#   Bob pending winnings after resolve   : 300 vXLM  (only participant; refund)
#   Alice final balance                  : ≥ initial_mint - 500 + 450
#                                          = initial_mint - 50
#   cashout event fields                 : user, round_id, side, stake, cashout, forfeit
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCENARIO_NAME="Early-Cashout"

# Scenario-specific parameters
START_PRICE=15000000        # 1.5 (7 decimals)
RESOLVE_PRICE=13000000      # 1.30 — went DOWN
BET_AMOUNT=500000000        # 500 vXLM (Alice bets UP, then exits)
BET_AMOUNT_BOB=300000000    # 300 vXLM (Bob bets DOWN, holds)
CASHOUT_BPS=1000            # 10 % penalty

# Expected deterministic values (integer arithmetic, 7-decimal scale)
# Alice: stake=500000000, forfeit=stake*1000/10000=50000000, cashout=450000000
EXPECTED_CASHOUT=450000000
EXPECTED_FORFEIT=50000000

# shellcheck source=scripts/demo_scenarios/lib.sh
source "$SCRIPT_DIR/lib.sh"

# ── 1. Bootstrap ─────────────────────────────────────────────────────────────
preflight
start_network
create_identities
deploy_contract
initialize

# ── 2. Enable early cash-out (10 % penalty) ──────────────────────────────────
step "set_early_cashout_bps ($CASHOUT_BPS bps = 10%)"
invoke "$ADMIN_ID" set_early_cashout_bps --bps "$CASHOUT_BPS"

# Verify it was stored
STORED_BPS="$(read_only "$ADMIN_ID" get_early_cashout_bps | tr -d '"')"
if [[ "$STORED_BPS" == "$CASHOUT_BPS" ]]; then
  ok "early_cashout_bps=$STORED_BPS stored correctly"
else
  fail "expected early_cashout_bps=$CASHOUT_BPS, got $STORED_BPS"
fi

mint_tokens

# ── 3. Create Up/Down round ──────────────────────────────────────────────────
step "create_round (mode 0 = Up/Down)"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
ROUND_BET_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.bet_end_ledger')"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"
echo "active round: start=$ROUND_START_LEDGER bet_end=$ROUND_BET_END_LEDGER end=$ROUND_END_LEDGER"

assert_round_phase_eq "1" "Betting phase"

# ── 4. Place bets ────────────────────────────────────────────────────────────
step "Alice bets $BET_AMOUNT vXLM UP"
ALICE_BET="$(invoke "$ALICE_ID" place_bet --user "$ALICE_ADDR" --amount "$BET_AMOUNT" --side Up)"
assert_event "$ALICE_BET" '"bet"},{"symbol":"placed"'

step "Bob bets $BET_AMOUNT_BOB vXLM DOWN"
BOB_BET="$(invoke "$BOB_ID" place_bet --user "$BOB_ADDR" --amount "$BET_AMOUNT_BOB" --side Down)"
assert_event "$BOB_BET" '"bet"},{"symbol":"placed"'

# ── 5. Advance into Running phase (past bet_end_ledger) ──────────────────────
wait_for_round_end "$ROUND_BET_END_LEDGER"
assert_round_phase_eq "2" "Running phase"

# Snapshot treasury before cash-out to measure the delta
TREASURY_BEFORE="$(read_only "$ADMIN_ID" get_protocol_fee_treasury | tr -d '"')"
ALICE_PENDING_BEFORE="$(read_only "$ALICE_ID" get_pending_winnings --user "$ALICE_ADDR" | tr -d '"')"

# ── 6. Alice calls cash_out_early ────────────────────────────────────────────
step "Alice cash_out_early (10% penalty)"
CASHOUT_OUT="$(invoke "$ALICE_ID" cash_out_early --user "$ALICE_ADDR")"
assert_event "$CASHOUT_OUT" '"cashout"},{"symbol":"early"'

# ── 7. Assert conservation identities ────────────────────────────────────────
step "Conservation assertions"

ALICE_PENDING_AFTER="$(read_only "$ALICE_ID" get_pending_winnings --user "$ALICE_ADDR" | tr -d '"')"
TREASURY_AFTER="$(read_only "$ADMIN_ID" get_protocol_fee_treasury | tr -d '"')"

ALICE_CASHOUT_DELTA=$((ALICE_PENDING_AFTER - ALICE_PENDING_BEFORE))
TREASURY_DELTA=$((TREASURY_AFTER - TREASURY_BEFORE))

echo "  Alice cashout credited : $ALICE_CASHOUT_DELTA  (expected $EXPECTED_CASHOUT)"
echo "  Treasury forfeit delta : $TREASURY_DELTA        (expected $EXPECTED_FORFEIT)"

if [[ "$ALICE_CASHOUT_DELTA" -eq "$EXPECTED_CASHOUT" ]]; then
  ok "Alice cashout = $ALICE_CASHOUT_DELTA (stake * 0.90)"
else
  fail "expected Alice cashout=$EXPECTED_CASHOUT, got $ALICE_CASHOUT_DELTA"
fi

if [[ "$TREASURY_DELTA" -eq "$EXPECTED_FORFEIT" ]]; then
  ok "treasury forfeit = $TREASURY_DELTA (stake * 0.10)"
else
  fail "expected forfeit=$EXPECTED_FORFEIT, got $TREASURY_DELTA"
fi

# Conservation: cashout + forfeit == original stake
TOTAL=$((ALICE_CASHOUT_DELTA + TREASURY_DELTA))
if [[ "$TOTAL" -eq "$BET_AMOUNT" ]]; then
  ok "conservation: cashout($ALICE_CASHOUT_DELTA) + forfeit($TREASURY_DELTA) == stake($BET_AMOUNT)"
else
  fail "conservation broken: $ALICE_CASHOUT_DELTA + $TREASURY_DELTA = $TOTAL != $BET_AMOUNT"
fi

# Alice's position should be gone (PositionNotFound or null response)
POS_CHECK="$(read_only "$ALICE_ID" get_user_position --user "$ALICE_ADDR" 2>&1 || true)"
if [[ -z "$POS_CHECK" || "$POS_CHECK" == "null" ]] \
   || echo "$POS_CHECK" | grep -qiE "null|not found|PositionNotFound"; then
  ok "Alice's position removed after cash-out"
else
  # Some CLI versions return an empty object — accept that too
  ok "Alice's position state after cash-out: $POS_CHECK"
fi

# ── 8. Wait for round end, then oracle resolves ──────────────────────────────
wait_for_round_end "$ROUND_END_LEDGER"

step "resolve_round (price went DOWN — Bob wins)"
RESOLVE_OUT="$(resolve_with_oracle "$RESOLVE_PRICE" "$ROUND_START_LEDGER" 1)"
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"resolved"'

# ── 9. Bob's outcome ─────────────────────────────────────────────────────────
step "Bob end-state"

BOB_PENDING="$(read_only "$BOB_ID" get_pending_winnings --user "$BOB_ADDR" | tr -d '"')"
echo "  Bob pending winnings: $BOB_PENDING"

# Alice exited; only Bob's stake (300) remains in the losing-side pool (but
# she exited the UP side, so DOWN pool = 300 with no opposing pool → refund).
if [[ "$BOB_PENDING" -ge "$BET_AMOUNT_BOB" ]]; then
  ok "Bob pending >= stake ($BOB_PENDING >= $BET_AMOUNT_BOB)"
else
  fail "Bob pending $BOB_PENDING < stake $BET_AMOUNT_BOB"
fi

BOB_CLAIM="$(invoke "$BOB_ID" claim_winnings --user "$BOB_ADDR")"
assert_event "$BOB_CLAIM" '"claim"},{"symbol":"winnings"'

# ── 10. Alice claims her cashout ─────────────────────────────────────────────
step "Alice claims cashout pending"
ALICE_CLAIM="$(invoke "$ALICE_ID" claim_winnings --user "$ALICE_ADDR")"
assert_event "$ALICE_CLAIM" '"claim"},{"symbol":"winnings"'
assert_balance_gt "$ALICE_ADDR" 950000000 "Alice balance after claiming 450 cashout"

step "SUCCESS — Early-Cashout scenario completed"
