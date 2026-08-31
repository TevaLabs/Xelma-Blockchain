#!/usr/bin/env bash
#
# run_all.sh — One-command runner for all three demo scenarios.
#
# Builds the contract once, starts a single network container, and runs each
# scenario against the same ephemeral deployment. Every scenario is isolated
# in its own run — each deploys a fresh contract, runs its lifecycle, and
# cleans up its identities.
#
# Usage:
#   ./scripts/demo_scenarios/run_all.sh
#
# Environment:
#   SKIP_NETWORK_START   If "1", assume a `local` network container is already
#                        running (default: 0).
#   KEEP_NETWORK         If "1", leave the container running after exit (default: 0).
#   WASM_PATH            Path to the compiled contract WASM (default: auto-build).
#
# Exit codes:
#   0 — all scenarios passed
#   1 — one or more scenarios failed
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
START_TS="$(date +%s)"

echo "══════════════════════════════════════════════════════════════════"
echo "  Xelma Demo Scenario Pack"
echo "  5 scenarios: Up-Win, Down-Win, Precision-Tie, Multi-Feed-Quorum, Season-Rollover"
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

# Track results
TOTAL=0
PASSED=0
FAILED=0
RESULTS=""

run_scenario() {
  local script="$1" name="$2"
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  Running: $name"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  TOTAL=$((TOTAL + 1))

  set +e
  bash "$script" 2>&1
  local exit_code=$?
  set -e

  if [[ $exit_code -eq 0 ]]; then
    PASSED=$((PASSED + 1))
    RESULTS="$RESULTS  ✓ $name"
  else
    FAILED=$((FAILED + 1))
    RESULTS="$RESULTS  ✘ $name"
  fi
  echo ""
}

# Run all scenarios sequentially (each uses a fresh deployment)
run_scenario "$SCRIPT_DIR/scenario_up_win.sh"        "Up-Win"
run_scenario "$SCRIPT_DIR/scenario_down_win.sh"      "Down-Win"
run_scenario "$SCRIPT_DIR/scenario_precision_tie.sh" "Precision-Tie"
run_scenario "$SCRIPT_DIR/scenario_multi_feed.sh"    "Multi-Feed-Quorum"
run_scenario "$SCRIPT_DIR/scenario_season_rollover.sh" "Season-Rollover"

# ── Summary ──────────────────────────────────────────────────────────────────
END_TS="$(date +%s)"
ELAPSED=$((END_TS - START_TS))
echo ""
echo "══════════════════════════════════════════════════════════════════"
echo "  Demo Scenario Pack — Summary"
echo "══════════════════════════════════════════════════════════════════"
echo ""
echo "$RESULTS"
echo ""
echo "  Passed: $PASSED / $TOTAL"
echo "  Failed: $FAILED / $TOTAL"
echo "  Elapsed: ${ELAPSED}s"
echo ""

if [[ "$FAILED" -gt 0 ]]; then
  echo "  ❌ Some scenarios failed — review logs above."
  exit 1
else
  echo "  ✅ All scenarios passed."
  exit 0
fi
