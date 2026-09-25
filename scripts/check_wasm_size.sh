#!/usr/bin/env bash
#
# check_wasm_size.sh — Measure the release contract WASM and fail if it
# exceeds the budget recorded in .wasm-size-baseline (Issue #287).
#
# Usage:
#   ./scripts/check_wasm_size.sh [path-to-wasm]
#
# With no argument, builds the contract in release mode the same way CI's
# `wasm-size-gate` job does (see .github/workflows/ci.yml and the crate-type
# note below) and measures that output. Pass an existing .wasm path to skip
# the build and just check an already-built artifact.
#
# Budget policy and baseline-bump procedure: docs/wasm-size-budget.md.
#
# Run from the repository root, or anywhere — paths below are resolved
# relative to the script's own location.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

WASM_PATH="${1:-$REPO_ROOT/target/wasm32v1-none/release/xelma_contract.wasm}"
BASELINE_FILE="$REPO_ROOT/.wasm-size-baseline"

if [ ! -f "$WASM_PATH" ]; then
  echo "No WASM found at $WASM_PATH — building release contract..."
  # `cargo rustc --crate-type=cdylib` (rather than plain `cargo build`, which
  # also builds the `rlib` crate-type listed in contracts/Cargo.toml) is
  # required to get the same lean output `stellar contract build` produces —
  # building both crate-types in one pass roughly doubles the WASM size.
  cargo rustc \
    --manifest-path="$REPO_ROOT/contracts/Cargo.toml" \
    --crate-type=cdylib \
    --target=wasm32v1-none \
    --release \
    --locked
fi

if [ ! -f "$WASM_PATH" ]; then
  echo "ERROR: WASM file still not found at $WASM_PATH after build." >&2
  exit 1
fi

WASM_SIZE=$(wc -c < "$WASM_PATH")
BASELINE=$(cat "$BASELINE_FILE")
BUDGET=$(( BASELINE + BASELINE / 20 ))

echo "WASM size report"
echo "  File    : $WASM_PATH"
echo "  Current : ${WASM_SIZE} bytes"
echo "  Baseline: ${BASELINE} bytes"
echo "  Budget  : ${BUDGET} bytes (+5%)"
echo "  Delta   : $(( WASM_SIZE - BASELINE )) bytes"

if [ "${WASM_SIZE}" -gt "${BUDGET}" ]; then
  echo ""
  echo "ERROR: WASM size ${WASM_SIZE} exceeds budget ${BUDGET} bytes."
  echo "If this growth is intentional, update .wasm-size-baseline to ${WASM_SIZE}"
  echo "(see docs/wasm-size-budget.md for the full bump procedure)."
  exit 1
fi

echo "OK: WASM size is within budget."
