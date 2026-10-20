#!/usr/bin/env bash
#
# run_all.sh — Keeper loop runner for Xelma demo scenarios.
#
# Builds the contract once, starts a single network container, and continuously
# runs demo scenarios in a keeper loop. After each round settles, the loop
# automatically calls create_next_from_template to cycle into the next scenario
# template (Up-Win, Down-Win, Precision-Tie, Multi-Feed-Quorum).
#
# The loop runs continuously until explicitly stopped (e.g. via SIGINT / Ctrl+C
# or SIGTERM). Errors during settlement are handled gracefully without crashing
# the loop, and state is cleaned up between cycles to remain idempotent-safe.
#
# Usage:
#   ./scripts/demo_scenarios/run_all.sh
#
# Environment:
#   MAX_ROUNDS           Maximum number of scenario rounds to execute before
#                        stopping (default: 0 = continuous loop until stopped).
#   LOOP_DELAY           Delay in seconds between round cycles (default: 3).
#   SKIP_NETWORK_START   If "1", assume a `local` network container is already
#                        running (default: 0).
#   KEEP_NETWORK         If "1", leave the container running after exit (default: 0).
#   KEEP_NETWORK_AFTER   If "1", leave the network container running after script exit (default: 0).
#   WASM_PATH            Path to the compiled contract WASM (default: auto-build).
#
# Exit codes:
#   0 — all executed scenarios passed (or loop stopped gracefully)
#   1 — one or more scenarios failed
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
START_TS="$(date +%s)"

echo "══════════════════════════════════════════════════════════════════"
echo "  Xelma Demo Scenario Pack — Keeper Loop"
echo "  Templates: Up-Win, Down-Win, Precision-Tie, Multi-Feed-Quorum"
echo "  Started: $(date -d @"$START_TS" 2>/dev/null || date -r "$START_TS")"
echo "══════════════════════════════════════════════════════════════════"
echo ""

# Ensure WASM is built once before any scenario runs
WASM_PATH="${WASM_PATH:-"$REPO_ROOT/target/wasm32v1-none/release/xelma_contract.wasm"}"
if [[ ! -f "$WASM_PATH" ]]; then
  echo "[build] Compiling contract WASM..."
  (cd "$REPO_ROOT/contracts" && stellar contract build --package xelma-contract)
  if [[ ! -f "$WASM_PATH" ]]; then
    echo "ERROR: build did not produce $WASM_PATH"
    exit 1
  fi
fi
echo "[build] Using WASM: $WASM_PATH ($(wc -c < "$WASM_PATH") bytes)"
echo ""

# ── Start shared network container ──────────────────────────────────────────
if [[ "${SKIP_NETWORK_START:-0}" != "1" ]]; then
  echo "[network] Starting local Soroban network container..."
  stellar container start local
  echo "[network] Waiting for RPC health..."
  sleep 15
  ready=0
  for _ in $(seq 1 60); do
    if stellar network health --network local >/dev/null 2>&1; then
      ready=1
      break
    fi
    sleep 2
  done
  if [[ "$ready" != "1" ]]; then
    echo "ERROR: local network did not become healthy"
    exit 1
  fi
  echo "[network] Ready."
fi

# Export shared env so each scenario can inherit it.
# run_all.sh manages the container lifecycle itself so scenarios never
# stop/start it independently — they all share one container session.
# We force KEEP_NETWORK=1 so scenario cleanups don't race to stop the
# shared container; run_all.sh's own trap below handles final teardown.
export WASM_PATH
export SKIP_NETWORK_START=1
export KEEP_NETWORK=1

# Own cleanup: stop the network only once at the very end, but only
# if the user did NOT explicitly ask to keep it running.
CLEANUP_NETWORK="${KEEP_NETWORK_AFTER:-0}"  # 0 = stop, 1 = leave
cleanup_all() {
  local exit_code=$?
  if [[ "$CLEANUP_NETWORK" != "1" ]]; then
    echo "Stopping local network container..."
    stellar container stop local >/dev/null 2>&1 || true
  fi
  exit $exit_code
}
trap cleanup_all EXIT

# Available scenario templates for continuous keeper cycling
SCENARIO_TEMPLATES=(
  "$SCRIPT_DIR/scenario_up_win.sh:Up-Win"
  "$SCRIPT_DIR/scenario_down_win.sh:Down-Win"
  "$SCRIPT_DIR/scenario_precision_tie.sh:Precision-Tie"
  "$SCRIPT_DIR/scenario_multi_feed.sh:Multi-Feed-Quorum"
)

# Loop control & state tracking
TOTAL=0
PASSED=0
FAILED=0
RESULTS=""
ROUND_COUNT=0
CURRENT_TEMPLATE_IDX=0
MAX_ROUNDS="${MAX_ROUNDS:-0}"     # 0 = run continuously until explicitly stopped
LOOP_DELAY="${LOOP_DELAY:-3}"     # seconds between round cycles
RUNNING=1

stop_keeper() {
  echo ""
  echo "══════════════════════════════════════════════════════════════════"
  echo "  Keeper loop stopped by user (signal received). Exiting..."
  echo "══════════════════════════════════════════════════════════════════"
  RUNNING=0
}
trap stop_keeper SIGINT SIGTERM

cleanup_stale_identities() {
  if command -v stellar >/dev/null 2>&1; then
    for stale_id in $(stellar keys ls 2>/dev/null | grep -E '^demo-(admin|oracle|alice|bob)-' || true); do
      stellar keys rm "$stale_id" --force >/dev/null 2>&1 || true
    done
  fi
}

# create_next_from_template
#
# Instantiates and executes the next scenario from the template blueprint.
# Called automatically after settlement completes so the demo cycles continuously.
# Idempotent-safe: resets any lingering state or keys before and after execution.
# Error-resilient: catches failures during settlement, records them, and allows
# the keeper loop to continue running.
create_next_from_template() {
  local template_entry="${1:-"${SCENARIO_TEMPLATES[$CURRENT_TEMPLATE_IDX]}"}"
  local script="${template_entry%%:*}"
  local name="${template_entry##*:}"

  # Advance template pointer when using default round progression
  if [[ -z "${1:-}" ]]; then
    CURRENT_TEMPLATE_IDX=$(( (CURRENT_TEMPLATE_IDX + 1) % ${#SCENARIO_TEMPLATES[@]} ))
  fi

  ROUND_COUNT=$((ROUND_COUNT + 1))
  TOTAL=$((TOTAL + 1))

  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  [Keeper] Round #$ROUND_COUNT — Template: $name"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  # Idempotent-safe cleanup before launching scenario
  cleanup_stale_identities

  set +e
  bash "$script" 2>&1
  local exit_code=$?
  set -e

  if [[ $exit_code -eq 0 ]]; then
    PASSED=$((PASSED + 1))
    RESULTS="${RESULTS}  ✓ Round #$ROUND_COUNT: $name"$'\n'
    echo ""
    echo "  [Keeper] Round #$ROUND_COUNT ($name) settled and completed successfully."
  else
    FAILED=$((FAILED + 1))
    RESULTS="${RESULTS}  ✘ Round #$ROUND_COUNT: $name (failed with exit code $exit_code)"$'\n'
    echo ""
    echo "  [Keeper] WARNING: Round #$ROUND_COUNT ($name) encountered an error (exit code $exit_code)."
    echo "  [Keeper] Continuing keeper loop gracefully without crashing..."
  fi

  # Idempotent-safe cleanup after scenario completes
  cleanup_stale_identities
}

echo "[keeper] Starting keeper loop..."
echo "[keeper] Templates loaded: ${#SCENARIO_TEMPLATES[@]}"
if [[ "$MAX_ROUNDS" -gt 0 ]]; then
  echo "[keeper] Mode: bounded ($MAX_ROUNDS rounds)"
else
  echo "[keeper] Mode: continuous (runs indefinitely until stopped with Ctrl+C)"
fi
echo ""

# Main Keeper Loop
while [[ $RUNNING -eq 1 ]]; do
  create_next_from_template

  # Check if bounded run limit reached
  if [[ "$MAX_ROUNDS" -gt 0 && "$ROUND_COUNT" -ge "$MAX_ROUNDS" ]]; then
    echo ""
    echo "[keeper] Reached maximum round limit ($MAX_ROUNDS). Exiting keeper loop."
    break
  fi

  # Delay before creating next scenario round
  if [[ $RUNNING -eq 1 ]]; then
    echo ""
    echo "[keeper] Settlement cycle finished. Creating next scenario from template in ${LOOP_DELAY}s..."
    sleep "$LOOP_DELAY"
  fi
done

# ── Summary ──────────────────────────────────────────────────────────────────
END_TS="$(date +%s)"
ELAPSED=$((END_TS - START_TS))
echo ""
echo "══════════════════════════════════════════════════════════════════"
echo "  Demo Scenario Pack — Keeper Loop Summary"
echo "══════════════════════════════════════════════════════════════════"
echo ""
echo "$RESULTS"
echo "  Total Rounds: $TOTAL"
echo "  Passed:       $PASSED / $TOTAL"
echo "  Failed:       $FAILED / $TOTAL"
echo "  Elapsed:      ${ELAPSED}s"
echo ""

if [[ "$FAILED" -gt 0 ]]; then
  echo "  ❌ Some scenario rounds failed — review logs above."
  exit 1
else
  echo "  ✅ All completed scenario rounds passed."
  exit 0
fi
