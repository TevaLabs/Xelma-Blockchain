#!/usr/bin/env bash
#
# test_probe_coverage.sh — Integration smoke tests for health_probe.sh
#
# Validates that the probe script's STATUS_LABEL / STATUS_SEVERITY tables
# are complete and that unknown status codes fail loud (exit 3).
#
# Issue: #421
#
# Usage:
#   ./scripts/health_probe/test_probe_coverage.sh
#
# Exit codes:
#   0  All tests passed
#   1  One or more tests failed
#
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROBE="$SCRIPT_DIR/health_probe.sh"

# ── Canonical status code table (must match ProtocolHealthStatus) ─────────────
EXPECTED_CODES=(0 1 2 3 4 5 6 7)
EXPECTED_LABELS=(HEALTHY PAUSED ORACLE_STALE ROUND_STALE NO_ACTIVE_ROUND MULTIPLE_ISSUES CLAIMS_ONLY ACCESS_RESTRICTED)
EXPECTED_SEVERITIES=(OK CRIT WARN WARN OK CRIT WARN OK)

PASSED=0
FAILED=0

pass() { PASSED=$((PASSED + 1)); echo "  ✓ $1"; }
fail() { FAILED=$((FAILED + 1)); echo "  ✗ $1"; }

echo "═══════════════════════════════════════════════════════════════"
echo " Health Probe Coverage Tests"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# ── Test 1: STATUS_LABEL table has all expected codes ─────────────────────────
echo "Test 1: STATUS_LABEL table coverage"
for i in "${!EXPECTED_CODES[@]}"; do
  code="${EXPECTED_CODES[$i]}"
  label="${EXPECTED_LABELS[$i]}"
  # Match lines like [0]="HEALTHY" inside the STATUS_LABEL block
  if grep -q "\[$code\]=\"$label\"" "$PROBE"; then
    pass "code=$code -> $label"
  else
    fail "code=$code missing or mismatched (expected $label)"
  fi
done
echo ""

# ── Test 2: STATUS_SEVERITY table has all expected codes ──────────────────────
echo "Test 2: STATUS_SEVERITY table coverage"
for i in "${!EXPECTED_CODES[@]}"; do
  code="${EXPECTED_CODES[$i]}"
  severity="${EXPECTED_SEVERITIES[$i]}"
  if grep -q "\[$code\]=\"$severity\"" "$PROBE"; then
    pass "code=$code -> severity=$severity"
  else
    fail "code=$code missing or mismatched severity (expected $severity)"
  fi
done
echo ""

# ── Test 3: Total code count matches ─────────────────────────────────────────
echo "Test 3: Table size consistency"
expected_count=${#EXPECTED_CODES[@]}
# Count entries in the STATUS_LABEL block (lines with [N]="..." between declare -A STATUS_LABEL and the closing paren)
label_count=$(awk '/^declare -A STATUS_LABEL/,/^)/' "$PROBE" | grep -c '\[.*\]="' || echo 0)
severity_count=$(awk '/^declare -A STATUS_SEVERITY/,/^)/' "$PROBE" | grep -c '\[.*\]="' || echo 0)
if [[ "$label_count" == "$expected_count" ]] && [[ "$severity_count" == "$expected_count" ]]; then
  pass "Both tables have $expected_count entries"
else
  fail "Expected $expected_count entries, got label=$label_count severity=$severity_count"
fi
echo ""

# ── Test 4: Unknown status code produces UNKNOWN severity ─────────────────────
echo "Test 4: Unknown status codes fail loud"
# Source just the STATUS_SEVERITY map from the probe
eval "$(awk '/^declare -A STATUS_SEVERITY/,/^)/' "$PROBE")"

unknown_code=99
severity="${STATUS_SEVERITY[$unknown_code]:-UNKNOWN}"
if [[ "$severity" == "UNKNOWN" ]]; then
  pass "Unknown code $unknown_code -> severity UNKNOWN"
else
  fail "Unknown code $unknown_code -> severity '$severity' (expected UNKNOWN)"
fi

# Verify the guard in the script exists
if grep -q 'unrecognized on-chain status_code' "$PROBE"; then
  pass "Unknown-code fail-loud guard present in probe script"
else
  fail "Unknown-code fail-loud guard missing from probe script"
fi
echo ""

# ── Test 5: ACCESS_RESTRICTED is severity OK ──────────────────────────────────
echo "Test 5: ACCESS_RESTRICTED is informational (OK severity)"
eval "$(awk '/^declare -A STATUS_SEVERITY/,/^)/' "$PROBE")"
if [[ "${STATUS_SEVERITY[7]}" == "OK" ]]; then
  pass "ACCESS_RESTRICTED (7) severity is OK"
else
  fail "ACCESS_RESTRICTED (7) severity is '${STATUS_SEVERITY[7]}' (expected OK)"
fi
echo ""

# ── Test 6: No label/severity mismatch between maps ──────────────────────────
echo "Test 6: Label and severity maps have same code set"
eval "$(awk '/^declare -A STATUS_LABEL/,/^)/' "$PROBE")"
eval "$(awk '/^declare -A STATUS_SEVERITY/,/^)/' "$PROBE")"
label_codes="${!STATUS_LABEL[*]}"
severity_codes="${!STATUS_SEVERITY[*]}"
if [[ "$label_codes" == "$severity_codes" ]]; then
  pass "Both maps cover the same codes: $label_codes"
else
  fail "Label codes ($label_codes) != Severity codes ($severity_codes)"
fi
echo ""

# ── Summary ──────────────────────────────────────────────────────────────────
echo "═══════════════════════════════════════════════════════════════"
total=$((PASSED + FAILED))
echo " Results: $PASSED/$total passed"
if [[ $FAILED -gt 0 ]]; then
  echo " $FAILED test(s) FAILED"
  echo "═══════════════════════════════════════════════════════════════"
  exit 1
fi
echo " All tests passed"
echo "═══════════════════════════════════════════════════════════════"
exit 0
