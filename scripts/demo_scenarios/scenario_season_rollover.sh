#!/usr/bin/env bash
#
# scenario_season_rollover.sh — Demo scenario: leaderboard season rollover.
#
# Flow:
#   initialize → mint (Alice, Bob) → Round 1 (Alice wins UP, Bob loses)
#   → reset_leaderboard_season (admin) → Round 2 in the new season
#   (Bob wins UP this time) → verify season 1 is frozen/queryable,
#   season 2 starts empty and only reflects its own round, and the
#   lifetime leaderboard reflects both rounds combined.
#
# Assertions:
#   - get_current_season_id advances 1 -> 2 on reset
#   - season 1's frozen archive matches what was live before the reset
#     (participant_count, wins ranking) and stays queryable after the reset
#   - season 1's live-query path (get_season_leaderboard_by_wins) transparently
#     serves the frozen archive once no longer active
#   - season 2 starts empty, then reflects only its own round's outcome
#   - the lifetime leaderboard is untouched by the season boundary — it
#     reflects wins from both season 1 and season 2
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCENARIO_NAME="Season-Rollover"

# Scenario-specific parameters
START_PRICE=15000000        # 1.5 (7 decimals)
RESOLVE_PRICE_UP=16500000   # 1.65 — UP wins
RESOLVE_PRICE_DOWN=13500000 # 1.35 — DOWN wins
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

assert_season_id_eq() {
  local expected="$1" label="${2:-}"
  local id
  id="$(read_only "$ALICE_ID" get_current_season_id | tr -d '"')"
  if [[ "$id" == "$expected" ]]; then
    ok "current_season_id=$id == $expected ${label:+($label)}"
  else
    fail "current_season_id=$id != $expected ${label:+($label)}"
  fi
}

run_updown_round() {
  local resolve_price="$1" nonce="$2"
  step "create_round (mode 0 = Up/Down)"
  invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0
  local round_json start_ledger end_ledger
  round_json="$(read_only "$ADMIN_ID" get_active_round)"
  start_ledger="$(echo "$round_json" | jq -r '.start_ledger')"
  end_ledger="$(echo "$round_json" | jq -r '.end_ledger')"

  step "Alice bets UP, Bob bets DOWN"
  invoke "$ALICE_ID" place_bet --user "$ALICE_ADDR" --amount "$BET_AMOUNT" --side Up >/dev/null
  invoke "$BOB_ID" place_bet --user "$BOB_ADDR" --amount "$BET_AMOUNT_BOB" --side Down >/dev/null

  wait_for_round_end "$end_ledger"

  step "resolve_round (resolve_price=$resolve_price)"
  local resolve_out
  resolve_out="$(resolve_with_oracle "$resolve_price" "$start_ledger" "$nonce")"
  assert_event "$resolve_out" '"round"},{"symbol":"resolved"'
}

# ── 2. Season 1: Alice wins a round ─────────────────────────────────────────
assert_season_id_eq 1 "starts at season 1"
run_updown_round "$RESOLVE_PRICE_UP" 1

ALICE_SEASON1_STATS="$(read_only "$ALICE_ID" get_season_user_stats --season_id 1 --user "$ALICE_ADDR")"
ALICE_SEASON1_WINS="$(echo "$ALICE_SEASON1_STATS" | jq -r '.total_wins')"
if [[ "$ALICE_SEASON1_WINS" -eq 1 ]]; then
  ok "Alice season-1 wins=$ALICE_SEASON1_WINS"
else
  fail "Alice season-1 wins=$ALICE_SEASON1_WINS != 1"
fi

# ── 3. Roll the season over ─────────────────────────────────────────────────
step "reset_leaderboard_season (admin)"
RESET_OUT="$(invoke "$ADMIN_ID" reset_leaderboard_season)"
assert_event "$RESET_OUT" '"season"},{"symbol":"reset"'

assert_season_id_eq 2 "advances to season 2 after reset"

# Season 1's frozen archive must exist and match what was live pre-reset.
ARCHIVE_S1="$(read_only "$ALICE_ID" get_season_archive --season_id 1)"
ARCHIVE_S1_COUNT="$(echo "$ARCHIVE_S1" | jq -r '.participant_count')"
ARCHIVE_S1_TOP_USER="$(echo "$ARCHIVE_S1" | jq -r '.wins[0].user')"
if [[ "$ARCHIVE_S1_COUNT" -eq 2 && "$ARCHIVE_S1_TOP_USER" == "$ALICE_ADDR" ]]; then
  ok "season 1 archived: participant_count=$ARCHIVE_S1_COUNT top=$ARCHIVE_S1_TOP_USER"
else
  fail "season 1 archive mismatch: $ARCHIVE_S1"
fi

# The paginated query for season 1 must now transparently serve the archive.
S1_PAGE="$(read_only "$ALICE_ID" get_season_leaderboard_by_wins --season_id 1 --offset 0 --limit 10)"
S1_PAGE_LEN="$(echo "$S1_PAGE" | jq 'length')"
if [[ "$S1_PAGE_LEN" -eq 2 ]]; then
  ok "season 1 leaderboard query still serves 2 frozen entries post-reset"
else
  fail "season 1 leaderboard query returned $S1_PAGE_LEN entries, expected 2"
fi

# Season 2 must start empty.
S2_PAGE="$(read_only "$ALICE_ID" get_season_leaderboard_by_wins --season_id 2 --offset 0 --limit 10)"
S2_PAGE_LEN="$(echo "$S2_PAGE" | jq 'length')"
if [[ "$S2_PAGE_LEN" -eq 0 ]]; then
  ok "season 2 starts empty"
else
  fail "season 2 not empty at rollover: $S2_PAGE"
fi

# ── 4. Season 2: Bob wins a round ───────────────────────────────────────────
# Round 2 resolves DOWN this time (Bob's side), so Bob — who lost round 1 —
# picks up his first win in the new season while Alice takes a loss.
run_updown_round "$RESOLVE_PRICE_DOWN" 2

BOB_SEASON2_STATS="$(read_only "$BOB_ID" get_season_user_stats --season_id 2 --user "$BOB_ADDR")"
BOB_SEASON2_WINS="$(echo "$BOB_SEASON2_STATS" | jq -r '.total_wins')"
if [[ "$BOB_SEASON2_WINS" -eq 1 ]]; then
  ok "Bob season-2 wins=$BOB_SEASON2_WINS"
else
  fail "Bob season-2 wins=$BOB_SEASON2_WINS != 1"
fi

# Season 1's frozen numbers for Bob must be untouched by season 2 activity.
BOB_SEASON1_STATS="$(read_only "$BOB_ID" get_season_user_stats --season_id 1 --user "$BOB_ADDR")"
BOB_SEASON1_WINS="$(echo "$BOB_SEASON1_STATS" | jq -r '.total_wins')"
if [[ "$BOB_SEASON1_WINS" -eq 0 ]]; then
  ok "Bob season-1 wins unchanged at $BOB_SEASON1_WINS"
else
  fail "Bob season-1 wins mutated to $BOB_SEASON1_WINS after season-2 activity"
fi

# ── 5. Lifetime leaderboard spans both seasons ──────────────────────────────
LIFETIME_ALICE="$(read_only "$ALICE_ID" get_user_stats --user "$ALICE_ADDR")"
LIFETIME_ALICE_WINS="$(echo "$LIFETIME_ALICE" | jq -r '.total_wins')"
LIFETIME_BOB="$(read_only "$BOB_ID" get_user_stats --user "$BOB_ADDR")"
LIFETIME_BOB_WINS="$(echo "$LIFETIME_BOB" | jq -r '.total_wins')"
if [[ "$LIFETIME_ALICE_WINS" -eq 1 && "$LIFETIME_BOB_WINS" -eq 1 ]]; then
  ok "lifetime wins span both seasons: Alice=$LIFETIME_ALICE_WINS Bob=$LIFETIME_BOB_WINS"
else
  fail "lifetime wins wrong: Alice=$LIFETIME_ALICE_WINS Bob=$LIFETIME_BOB_WINS"
fi

step "SUCCESS — Season-rollover scenario completed"
