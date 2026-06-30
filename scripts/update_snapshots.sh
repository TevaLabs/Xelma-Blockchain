#!/usr/bin/env bash
#
# update_snapshots.sh — Regenerate snapshot golden files deterministically.
#
# Usage:
#   ./scripts/update_snapshots.sh
#
# This script:
#   1. Sets SNAPSHOT_UPDATE=1 so snapshot tests overwrite golden files.
#   2. Runs snapshot-related tests under contracts/.
#   3. Reports which files changed.
#
# Run from the repository root.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

SNAPSHOT_DIR="$REPO_ROOT/contracts/test_snapshots"

echo "============================================"
echo "  Snapshot Golden Update"
echo "============================================"
echo ""

# Ensure the snapshot directory exists
mkdir -p "$SNAPSHOT_DIR"

# Record current snapshot hashes before regeneration
echo "[1/3] Recording current snapshot state …"
PRE_HASH=""
if ls "$SNAPSHOT_DIR"/*.snap 1>/dev/null 2>&1; then
  PRE_HASH=$(sha256sum "$SNAPSHOT_DIR"/*.snap | sort)
fi

# Run snapshot tests with update mode enabled
echo "[2/3] Regenerating snapshots …"
SNAPSHOT_UPDATE=1 \
  cargo test --package xelma-contract --locked -- --nocapture \
  2>&1 | tail -20

# Report what changed
echo ""
echo "[3/3] Checking for changes …"
POST_HASH=""
if ls "$SNAPSHOT_DIR"/*.snap 1>/dev/null 2>&1; then
  POST_HASH=$(sha256sum "$SNAPSHOT_DIR"/*.snap | sort)
fi

if [[ "$PRE_HASH" != "$POST_HASH" ]]; then
  echo "  Snapshot files updated:"
  (diff <(echo "$PRE_HASH") <(echo "$POST_HASH") 2>/dev/null || true) | grep '^[<>]' || echo "  (new snapshots created)"
  echo ""
  echo "  Review the diff with: git diff $SNAPSHOT_DIR"
else
  echo "  No snapshot changes detected."
fi

echo ""
echo "============================================"
echo "  Done. Run 'cargo test --workspace' to verify."
echo "============================================"
