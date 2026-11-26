#!/usr/bin/env bash
#
# test_prometheus.sh — offline smoke test for `health_probe.sh --prometheus`.
#
# Stubs the `stellar` CLI with a fake that returns a fixed
# `ProtocolHealthStatus`, then asserts the Prometheus text exposition and the
# Nagios exit code. No network, no deployed contract, no real Stellar CLI.
#
# Usage:
#   ./scripts/health_probe/test_prometheus.sh
#
# Issue: #855
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROBE="$SCRIPT_DIR/health_probe.sh"
# C + 55 uppercase alphanumerics, per the probe's address validation.
CONTRACT_ID="C$(printf 'A%.0s' $(seq 1 55))"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

# Fake `stellar`: answers `keys` and `contract invoke` with a paused protocol
# snapshot (status_code = 1 → PAUSED → CRIT → exit 2).
mkdir -p "$TMP_DIR/bin"
cat > "$TMP_DIR/bin/stellar" <<'STELLAR'
#!/usr/bin/env bash
case "$1" in
  keys)
    echo "GDXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
    ;;
  contract)
    cat <<'JSON'
{"paused":true,"oracle_live":true,"oracle_status":0,"has_active_round":false,"active_round_phase":0,"schema_version":3,"ledger_sequence":42,"ledger_timestamp":1750000000,"status_code":1}
JSON
    ;;
  *)
    echo "unexpected stellar args: $*" >&2
    exit 1
    ;;
esac
STELLAR
chmod +x "$TMP_DIR/bin/stellar"

failures=0
check_contains() {
  local needle="$1"
  if ! grep -Fqx -- "$needle" <<< "$OUTPUT"; then
    echo "FAIL: expected line not found: $needle" >&2
    failures=$((failures + 1))
  fi
}

set +e
OUTPUT="$(PATH="$TMP_DIR/bin:$PATH" "$PROBE" "$CONTRACT_ID" --network local --prometheus 2>/dev/null)"
STATUS=$?
set -e

if [[ "$STATUS" -ne 2 ]]; then
  echo "FAIL: expected exit 2 (CRIT) for status_code=1, got $STATUS" >&2
  failures=$((failures + 1))
fi

check_contains 'xelma_health_probe_up{contract_id="'"$CONTRACT_ID"'",network="local"} 1'
check_contains 'xelma_health_status_code{contract_id="'"$CONTRACT_ID"'",network="local",status_label="PAUSED",severity="CRIT"} 1'
check_contains 'xelma_health_exit_code{contract_id="'"$CONTRACT_ID"'",network="local"} 2'
check_contains 'xelma_health_paused{contract_id="'"$CONTRACT_ID"'",network="local"} 1'
check_contains 'xelma_health_oracle_live{contract_id="'"$CONTRACT_ID"'",network="local"} 1'
check_contains 'xelma_health_has_active_round{contract_id="'"$CONTRACT_ID"'",network="local"} 0'
check_contains 'xelma_health_ledger_sequence{contract_id="'"$CONTRACT_ID"'",network="local"} 42'

# Every HELP line must be followed by a matching TYPE line (exposition format).
if ! grep -c '^# HELP ' <<< "$OUTPUT" | grep -qx "$(grep -c '^# TYPE ' <<< "$OUTPUT")"; then
  echo "FAIL: HELP/TYPE line counts differ" >&2
  failures=$((failures + 1))
fi

# --json and --prometheus must not be combinable.
set +e
PATH="$TMP_DIR/bin:$PATH" "$PROBE" "$CONTRACT_ID" --json --prometheus >/dev/null 2>&1
STATUS=$?
set -e
if [[ "$STATUS" -ne 3 ]]; then
  echo "FAIL: expected exit 3 when --json and --prometheus are combined, got $STATUS" >&2
  failures=$((failures + 1))
fi

# An unreachable contract must still emit a scrapeable `up 0`.
cat > "$TMP_DIR/bin/stellar" <<'STELLAR'
#!/usr/bin/env bash
case "$1" in
  keys) echo "GDXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX" ;;
  *) echo "rpc error: connection refused" >&2; exit 1 ;;
esac
STELLAR
chmod +x "$TMP_DIR/bin/stellar"

set +e
OUTPUT="$(PATH="$TMP_DIR/bin:$PATH" "$PROBE" "$CONTRACT_ID" --prometheus 2>/dev/null)"
STATUS=$?
set -e
if [[ "$STATUS" -ne 3 ]]; then
  echo "FAIL: expected exit 3 when the contract is unreachable, got $STATUS" >&2
  failures=$((failures + 1))
fi
check_contains 'xelma_health_probe_up{contract_id="'"$CONTRACT_ID"'",network="local"} 0'

if [[ "$failures" -ne 0 ]]; then
  echo "$failures assertion(s) failed" >&2
  exit 1
fi

echo "OK: health_probe --prometheus output and exit codes verified."
