#!/usr/bin/env bash
#
# emergency_drill_gate.sh — Release checklist gate for the claims-only
# emergency drill (Issue: "Ops: emergency drill as explicit release checklist
# gate").
#
# Usage:
#   ./scripts/emergency_drill_gate.sh [log-file]
#
# Runs the drill suite and, crucially, asserts that the drills which cover the
# release-critical `pause → claims → resume` sequence actually executed. A bare
# `cargo test tests::drill` exits 0 even when the filter matches nothing, so a
# renamed or deleted drill would otherwise silently turn this gate green.
#
# CI runs the same script from the `emergency-drill` job in
# .github/workflows/ci.yml; `ci-success` requires that job, so a failing or
# missing drill blocks the release. Release checklist:
# docs/EMERGENCY_DRILL.md (§5) and docs/RELEASE.md.
#
# Run from the repository root, or anywhere — paths below are resolved relative
# to the script's own location.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

LOG_FILE="${1:-$REPO_ROOT/emergency-drill-gate.log}"
DRILL_FILTER="tests::drill"

# Every drill the release gate requires, and the phase of the
# `pause → claims → resume` sequence it protects. Keep in sync with
# docs/EMERGENCY_DRILL.md.
REQUIRED_DRILLS=(
  "tests::drill::test_fully_paused_matrix_verification"
  "tests::drill::test_claims_only_matrix_verification"
  "tests::drill::test_emergency_incident_simulation_lifecycle"
  "tests::drill::test_chaos_recovery_migrate_active_round_pause_resume"
  "tests::drill::test_chaos_recovery_migrate_active_round_pause_cancel"
  "tests::drill_chaos_migration::drill_chaos_migration_canonical_resume"
  "tests::drill_chaos_migration::drill_chaos_migration_canonical_cancel"
  "tests::drill_chaos_migration::drill_chaos_migration_full_matrix"
  "tests::drill_chaos_migration::drill_cancel_with_insurance_eligible_reason_does_not_trap"
)

echo "Emergency drill gate — claims-only drill (pause → claims → resume)"
echo "  Filter: $DRILL_FILTER"
echo "  Log   : $LOG_FILE"
echo

cd "$REPO_ROOT"

# `set -o pipefail` keeps the test exit status authoritative through the tee.
# `set +e` around it so a red drill is reported with context instead of aborting
# silently at the pipeline.
set +e
set -o pipefail
cargo test --package xelma-contract --lib "$DRILL_FILTER" --locked -- --nocapture \
  | tee "$LOG_FILE"
test_status=${PIPESTATUS[0]}
set -e
set +o pipefail

echo
if [ "$test_status" -ne 0 ]; then
  echo "FAIL: emergency drill suite failed (exit $test_status)."
  echo "The claims-only drill is a release gate — do not cut a release until it is green."
  exit "$test_status"
fi

missing=0
for drill in "${REQUIRED_DRILLS[@]}"; do
  if ! grep -qE "^test ${drill//./\\.} \.\.\. ok$" "$LOG_FILE"; then
    echo "FAIL: required drill did not run: $drill"
    missing=$((missing + 1))
  fi
done

if [ "$missing" -ne 0 ]; then
  echo ""
  echo "FAIL: $missing required drill(s) are missing from the run."
  echo "A renamed, skipped or deleted drill silently weakens the release gate."
  echo "Restore it, or update REQUIRED_DRILLS in scripts/emergency_drill_gate.sh"
  echo "and docs/EMERGENCY_DRILL.md together with an explicit sign-off."
  exit 1
fi

echo "OK: ${#REQUIRED_DRILLS[@]} emergency drills passed."
echo "Claims-only pause → claims → resume coverage verified."
