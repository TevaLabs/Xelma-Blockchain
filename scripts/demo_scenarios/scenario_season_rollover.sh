#!/usr/bin/env bash
#
# scenario_season_rollover.sh — Demo scenario: leaderboard season reset/archive.
#
# Flow:
#   initialize → mint (Alice, Bob) → two Up/Down rounds in season 1
#   (Alice wins both, Bob loses both) → admin resets the season
#   → third Up/Down round in season 2 (Bob wins)
#
# Assertions:
#   - Season 1 leaderboard shows Alice on top (2 wins) before reset
#   - reset_leaderboard_season emits ("season", "reset") and advances the id
#   - get_current_season_id reflects the new season
#   - Season 1 is frozen: get_season_archive(1) has the pre-reset top-N and
#     participant_count, and stays queryable via get_season_leaderboard_by_wins
#   - get_season_user_stats(1, alice) is untouched by the reset (still 2 wins)
#   - Season 2 starts empty and isolated: Bob's season-1 losses don't carry
#     over — his season-2 stats start fresh, and season 2's leaderboard shows
#     his new win rather than Alice's season-1 history
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCENARIO_NAME="Season-Rollover"

# Scenario-specific parameters
START_PRICE=15000000        # 1.5 (7 decimals)
UP_PRICE=16500000           # 1.65 — went UP
DOWN_PRICE=13500000         # 1.35 — went DOWN
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

# ── 2. Season 1 — Round 1: Alice UP wins, Bob DOWN loses ───────────────────
step "Season 1 / Round 1: create_round (mode 0 = Up/Down)"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"

invoke "$ALICE_ID" place_bet --user "$ALICE_ADDR" --amount "$BET_AMOUNT" --side Up >/dev/null
invoke "$BOB_ID" place_bet --user "$BOB_ADDR" --amount "$BET_AMOUNT_BOB" --side Down >/dev/null

wait_for_round_end "$ROUND_END_LEDGER"

RESOLVE_OUT="$(resolve_with_oracle "$UP_PRICE" "$ROUND_START_LEDGER" 1)"
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"resolved"'

invoke "$ALICE_ID" claim_winnings --user "$ALICE_ADDR" >/dev/null
invoke "$BOB_ID" claim_winnings --user "$BOB_ADDR" >/dev/null

# ── 3. Season 1 — Round 2: Alice DOWN wins again, Bob UP loses again ───────
step "Season 1 / Round 2: create_round (mode 0 = Up/Down)"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"

invoke "$ALICE_ID" place_bet --user "$ALICE_ADDR" --amount "$BET_AMOUNT" --side Down >/dev/null
invoke "$BOB_ID" place_bet --user "$BOB_ADDR" --amount "$BET_AMOUNT_BOB" --side Up >/dev/null

wait_for_round_end "$ROUND_END_LEDGER"

RESOLVE_OUT="$(resolve_with_oracle "$DOWN_PRICE" "$ROUND_START_LEDGER" 1)"
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"resolved"'

invoke "$ALICE_ID" claim_winnings --user "$ALICE_ADDR" >/dev/null
invoke "$BOB_ID" claim_winnings --user "$BOB_ADDR" >/dev/null

# ── 4. Pre-reset assertions (season 1 still active) ─────────────────────────
step "Pre-reset assertions"

SEASON_BEFORE="$(read_only "$ALICE_ID" get_current_season_id | tr -d '"')"
if [[ "$SEASON_BEFORE" == "1" ]]; then
  ok "active season is 1 before reset"
else
  fail "expected active season 1 before reset, got $SEASON_BEFORE"
fi

ALICE_SEASON1_STATS="$(read_only "$ALICE_ID" get_season_user_stats --season_id 1 --user "$ALICE_ADDR")"
ALICE_SEASON1_WINS="$(echo "$ALICE_SEASON1_STATS" | jq -r '.total_wins')"
if [[ "$ALICE_SEASON1_WINS" -eq 2 ]]; then
  ok "Alice season-1 stats: total_wins=$ALICE_SEASON1_WINS"
else
  fail "expected Alice season-1 total_wins=2, got $ALICE_SEASON1_WINS"
fi

SEASON1_WINS_BOARD="$(read_only "$ALICE_ID" get_season_leaderboard_by_wins --season_id 1 --offset 0 --limit 10)"
TOP_USER="$(echo "$SEASON1_WINS_BOARD" | jq -r '.[0].user')"
if [[ "$TOP_USER" == "$ALICE_ADDR" ]]; then
  ok "Alice tops the live season-1 wins leaderboard"
else
  fail "expected Alice on top of season-1 wins leaderboard, got $TOP_USER"
fi

# ── 5. Reset the season (admin only) ────────────────────────────────────────
step "reset_leaderboard_season (admin)"
RESET_OUT="$(invoke "$ADMIN_ID" reset_leaderboard_season)"
assert_event "$RESET_OUT" '"season"},{"symbol":"reset"'

NEW_SEASON_ID="$(read_only "$ALICE_ID" get_current_season_id | tr -d '"')"
if [[ "$NEW_SEASON_ID" == "2" ]]; then
  ok "active season advanced to 2"
else
  fail "expected active season 2 after reset, got $NEW_SEASON_ID"
fi

# ── 6. Season 1 is frozen — archive + historical queries still work ────────
step "Post-reset: season 1 archive assertions"

ARCHIVE="$(read_only "$ALICE_ID" get_season_archive --season_id 1)"
ARCHIVE_PARTICIPANTS="$(echo "$ARCHIVE" | jq -r '.participant_count')"
if [[ "$ARCHIVE_PARTICIPANTS" -eq 2 ]]; then
  ok "season-1 archive participant_count=$ARCHIVE_PARTICIPANTS"
else
  fail "expected season-1 archive participant_count=2, got $ARCHIVE_PARTICIPANTS"
fi

ARCHIVE_TOP_USER="$(echo "$ARCHIVE" | jq -r '.wins[0].user')"
if [[ "$ARCHIVE_TOP_USER" == "$ALICE_ADDR" ]]; then
  ok "season-1 archive top wins entry is Alice"
else
  fail "expected season-1 archive top wins entry to be Alice, got $ARCHIVE_TOP_USER"
fi

# get_season_user_stats(1, alice) must be untouched by the reset
ALICE_SEASON1_STATS_AFTER="$(read_only "$ALICE_ID" get_season_user_stats --season_id 1 --user "$ALICE_ADDR")"
ALICE_SEASON1_WINS_AFTER="$(echo "$ALICE_SEASON1_STATS_AFTER" | jq -r '.total_wins')"
if [[ "$ALICE_SEASON1_WINS_AFTER" -eq 2 ]]; then
  ok "Alice season-1 stats unchanged after reset: total_wins=$ALICE_SEASON1_WINS_AFTER"
else
  fail "expected Alice season-1 total_wins still 2 after reset, got $ALICE_SEASON1_WINS_AFTER"
fi

# The now-past season 1's leaderboard query transparently serves the frozen
# archive rather than an empty live index.
SEASON1_WINS_AFTER="$(read_only "$ALICE_ID" get_season_leaderboard_by_wins --season_id 1 --offset 0 --limit 10)"
SEASON1_TOP_AFTER="$(echo "$SEASON1_WINS_AFTER" | jq -r '.[0].user')"
if [[ "$SEASON1_TOP_AFTER" == "$ALICE_ADDR" ]]; then
  ok "season-1 wins leaderboard still serves Alice on top (from archive)"
else
  fail "expected season-1 wins leaderboard to still show Alice on top, got $SEASON1_TOP_AFTER"
fi

# ── 7. Season 2 — Round 3: Bob wins, starts from a clean slate ─────────────
step "Season 2 / Round 3: create_round (mode 0 = Up/Down)"
invoke "$ADMIN_ID" create_round --start_price "$START_PRICE" --mode 0

ROUND_JSON="$(read_only "$ADMIN_ID" get_active_round)"
ROUND_START_LEDGER="$(echo "$ROUND_JSON" | jq -r '.start_ledger')"
ROUND_END_LEDGER="$(echo "$ROUND_JSON" | jq -r '.end_ledger')"

invoke "$ALICE_ID" place_bet --user "$ALICE_ADDR" --amount "$BET_AMOUNT" --side Down >/dev/null
invoke "$BOB_ID" place_bet --user "$BOB_ADDR" --amount "$BET_AMOUNT_BOB" --side Up >/dev/null

wait_for_round_end "$ROUND_END_LEDGER"

RESOLVE_OUT="$(resolve_with_oracle "$UP_PRICE" "$ROUND_START_LEDGER" 1)"
assert_event "$RESOLVE_OUT" '"round"},{"symbol":"resolved"'

invoke "$ALICE_ID" claim_winnings --user "$ALICE_ADDR" >/dev/null
invoke "$BOB_ID" claim_winnings --user "$BOB_ADDR" >/dev/null

step "Season 2 isolation assertions"

BOB_SEASON2_STATS="$(read_only "$BOB_ID" get_season_user_stats --season_id 2 --user "$BOB_ADDR")"
BOB_SEASON2_WINS="$(echo "$BOB_SEASON2_STATS" | jq -r '.total_wins')"
BOB_SEASON2_LOSSES="$(echo "$BOB_SEASON2_STATS" | jq -r '.total_losses')"
if [[ "$BOB_SEASON2_WINS" -eq 1 && "$BOB_SEASON2_LOSSES" -eq 0 ]]; then
  ok "Bob season-2 stats start fresh: wins=$BOB_SEASON2_WINS losses=$BOB_SEASON2_LOSSES (season-1 losses did not carry over)"
else
  fail "expected Bob season-2 wins=1 losses=0, got wins=$BOB_SEASON2_WINS losses=$BOB_SEASON2_LOSSES"
fi

SEASON2_WINS_BOARD="$(read_only "$ALICE_ID" get_season_leaderboard_by_wins --season_id 2 --offset 0 --limit 10)"
SEASON2_TOP_USER="$(echo "$SEASON2_WINS_BOARD" | jq -r '.[0].user')"
if [[ "$SEASON2_TOP_USER" == "$BOB_ADDR" ]]; then
  ok "season-2 wins leaderboard shows Bob's new win, not Alice's season-1 history"
else
  fail "expected Bob on top of season-2 wins leaderboard, got $SEASON2_TOP_USER"
fi

step "SUCCESS — Season-Rollover scenario completed"
