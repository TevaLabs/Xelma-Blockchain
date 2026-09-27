#!/usr/bin/env bash
#
# scenario_dispute_void.sh — Demo scenario: Dispute window void-to-refund.
#
# The oracle resolves a round but a 10-ledger dispute window is configured.
# During that window anyone can call `void_round` to cancel the result and
# refund all participants their full stakes. This scenario exercises that path:
# the oracle resolves, Mallory (reusing Alice's identity here) calls void_round
# before the window closes, and both Alice and Bob receive full refunds.
#
# Flow:
#   initialize → set_dispute_ledgers(10) → mint (Alice, Bob)
#   → create_round (Mode 0 Up/Down)
#   → Alice bets 500 vXLM UP → Bob bets 300 vXLM DOWN
#   → Ledger advances past end_ledger
#   → Oracle calls resolve_round (staged, result NOT yet finalized)
#   → void_round called during dispute window
#   → Both Alice and Bob receive full stake refunds (Voided)
#
# Deterministic expected outputs:
#   Alice pending after void : 500 vXLM  (full stake back)
#   Bob   pending after void : 300 vXLM  (full stake back)
#   Round archive status     : Voided
#   get_round_status result  : 7  (Voided)
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCENARIO_NAME="Dispute-Void"

# Scenario-specific parameters
START_PRICE=15000000        # 1.5 (7 decimals)
RESOLVE_PRICE=16500000      # 1.65 — would be UP win, but will be voided
BET_AMOUNT=500000000        # 500 vXLM
BET_AMOUNT_BOB=300000000    # 300 vXLM
DISPUTE_LEDGERS=10          # 10-ledger dispute window

# Deterministic expected outputs
EXPECTED_ALICE_REFUND="$BET_AMOUNT"       # 500000000
EXPECTED_BOB_REFUND="$BET_AMOUNT_BOB"    # 300000000

# shellcheck source=scripts/demo_scenarios/lib.sh
source "$SCRIPT_DIR/lib.sh"

# ── 1. Bootstrap ─────────────────────────────────────────────────────────────
preflight
start_network
create_identities
deploy_contract
initialize

# ── 2. Configure dispute window ──────────────────────────────────────────────
step "set_dispute_ledgers ($DISPUTE_LEDGERS ledgers)"
invoke "$ADMIN_ID" set_dispute_ledgers --ledgers "$DISPUTE_LEDGERS"

STORED_DL="$(read_only "$ADMIN_ID" get_dispute_ledgers | tr -d '"')"
if [[ "$STORED_DL" == "$DISPUTE_LEDGERS" ]]; then
  ok "dispute_ledgers=$STORED_DL stored correctly"
else
  fail "expected dispute_ledgers=$DISPUTE_LEDGERS, got $STORED_DL"
fi

mint_tokens

# ── 3. Create Up/Down round ──────────────────────────────────────────────────
step "create_round (mode 0 = Up/Down)"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"
ROUND_ID="$(echo "$ROUND_JSON" | jq -r '.round_id')"
echo "active round: id=$ROUND_ID start=$ROUND_START_LEDGER end=$ROUND_END_LEDGER"

assert_round_phase_eq "1" "Betting phase"

# ── 4. Place bets ────────────────────────────────────────────────────────────
step "Alice bets $BET_AMOUNT vXLM UP"
ALICE_BET="$(invoke "$ALICE_ID" place_bet --user "$ALICE_ADDR" --amount "$BET_AMOUNT" --side Up)"
assert_event "$ALICE_BET" '"bet"},{"symbol":"placed"'

step "Bob bets $BET_AMOUNT_BOB vXLM DOWN"
BOB_BET="$(invoke "$BOB_ID" place_bet --user "$BOB_ADDR" --amount "$BET_AMOUNT_BOB" --side Down)"
assert_event "$BOB_BET" '"bet"},{"symbol":"placed"'

# ── 5. Wait for round to end ─────────────────────────────────────────────────
wait_for_round_end "$ROUND_END_LEDGER"

# ── 6. Oracle resolves — result is staged inside dispute window ──────────────
step "resolve_round (staged — dispute window opens)"
RESOLVE_OUT="$(resolve_with_oracle "$RESOLVE_PRICE" "$ROUND_START_LEDGER" 1)"
# During dispute window, resolve emits the summary event but winnings are
# deferred — finalize_round or void_round must be called next.
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"summary"'

echo "  Dispute window is now open ($DISPUTE_LEDGERS ledgers)"

# Snapshot balances — should still be zero pending (staged, not finalized)
ALICE_PENDING_STAGED="$(read_only "$ALICE_ID" get_pending_winnings --user "$ALICE_ADDR" | tr -d '"')"
BOB_PENDING_STAGED="$(read_only "$BOB_ID" get_pending_winnings --user "$BOB_ADDR" | tr -d '"')"
echo "  Pending (staged, before void): Alice=$ALICE_PENDING_STAGED Bob=$BOB_PENDING_STAGED"

# ── 7. Void the round during the dispute window ──────────────────────────────
# void_round is permissionless — anyone can call it while the window is open.
step "void_round (called during dispute window)"
VOID_OUT="$(invoke "$ALICE_ID" void_round --round_id "$ROUND_ID")"
assert_event "$VOID_OUT" '"round"},{"symbol":"voided"'

# ── 8. Assert full-refund end-state ──────────────────────────────────────────
step "End-state assertions — full refund after void"

ALICE_PENDING="$(read_only "$ALICE_ID" get_pending_winnings --user "$ALICE_ADDR" | tr -d '"')"
BOB_PENDING="$(read_only "$BOB_ID" get_pending_winnings --user "$BOB_ADDR" | tr -d '"')"
echo "  Alice pending: $ALICE_PENDING  (expected $EXPECTED_ALICE_REFUND)"
echo "  Bob   pending: $BOB_PENDING    (expected $EXPECTED_BOB_REFUND)"

if [[ "$ALICE_PENDING" -eq "$EXPECTED_ALICE_REFUND" ]]; then
  ok "Alice full-stake refund: pending=$ALICE_PENDING == stake=$EXPECTED_ALICE_REFUND"
else
  fail "Alice refund mismatch: pending=$ALICE_PENDING != stake=$EXPECTED_ALICE_REFUND"
fi

if [[ "$BOB_PENDING" -eq "$EXPECTED_BOB_REFUND" ]]; then
  ok "Bob full-stake refund: pending=$BOB_PENDING == stake=$EXPECTED_BOB_REFUND"
else
  fail "Bob refund mismatch: pending=$BOB_PENDING != stake=$EXPECTED_BOB_REFUND"
fi

# Conservation: total refunded == total staked
TOTAL_REFUNDED=$((ALICE_PENDING + BOB_PENDING))
TOTAL_STAKED=$((BET_AMOUNT + BET_AMOUNT_BOB))
if [[ "$TOTAL_REFUNDED" -eq "$TOTAL_STAKED" ]]; then
  ok "conservation: total_refunded($TOTAL_REFUNDED) == total_staked($TOTAL_STAKED)"
else
  fail "conservation broken: refunded=$TOTAL_REFUNDED != staked=$TOTAL_STAKED"
fi

# Round archive status must be Voided (7)
ROUND_STATUS="$(read_only "$ALICE_ID" get_round_status --round_id "$ROUND_ID" | tr -d '"')"
echo "  get_round_status=$ROUND_STATUS (expected 7 = Voided)"
if [[ "$ROUND_STATUS" == "7" ]]; then
  ok "get_round_status == 7 (Voided)"
else
  fail "expected round_status=7 (Voided), got $ROUND_STATUS"
fi

# Archive record must show Voided status
ARCHIVE="$(read_only "$ALICE_ID" get_archived_round --round_id "$ROUND_START_LEDGER")"
if echo "$ARCHIVE" | jq -e '.status == "Voided"' >/dev/null 2>&1; then
  ok "round archived with Voided status"
else
  fail "round archive missing or not Voided (got: $ARCHIVE)"
fi

# ── 9. Both participants claim their refund ───────────────────────────────────
step "Claim refunds"

ALICE_CLAIM="$(invoke "$ALICE_ID" claim_winnings --user "$ALICE_ADDR")"
assert_event "$ALICE_CLAIM" '"claim"},{"symbol":"winnings"'
# Alice's balance should equal initial mint (1000) since she got full stake back
assert_balance_eq "$ALICE_ADDR" 1000000000 "Alice balance restored to initial after void refund"

BOB_CLAIM="$(invoke "$BOB_ID" claim_winnings --user "$BOB_ADDR")"
assert_event "$BOB_CLAIM" '"claim"},{"symbol":"winnings"'
assert_balance_eq "$BOB_ADDR" 1000000000 "Bob balance restored to initial after void refund"

step "SUCCESS — Dispute-Void scenario completed"
