#!/usr/bin/env bash
#
# health_probe.sh — Protocol health probe scorecard for Xelma contracts.
#
# Queries the on-chain `get_protocol_health` endpoint and maps the returned
# composite `status_code` to standard Nagios-compatible exit codes so
# operators and automation (cron, CI, monitoring dashboards) can quickly
# triage protocol liveness.
#
# Usage:
#   ./scripts/health_probe/health_probe.sh <CONTRACT_ID> [--network <net>] [--source <id>] [--json | --prometheus]
#
# Output formats:
#   default     human-readable scorecard
#   --json      one JSON object (see README.md for the schema)
#   --prometheus Prometheus text exposition format (v0.0.4) for scraping
#
# Exit codes:
#   0  OK     — HEALTHY (0) or NO_ACTIVE_ROUND (4)
#   1  WARN   — ORACLE_STALE (2), ROUND_STALE (3), or CLAIMS_ONLY (6)
#   2  CRIT   — PAUSED (1) or MULTIPLE_ISSUES (5)
#   3  UNKNOWN — CLI not found / contract unreachable / parse failure
#
# No-round policy:
#   `status_code = 4` (NO_ACTIVE_ROUND) is treated as OK (exit 0) because
#   an idle protocol with a live oracle and no pause is a normal operational
#   state that does not require alerting. If you need an alert on "no round"
#   for a demo or hackathon flow, set an explicit threshold in your monitor.
#
# Prerequisites: stellar CLI (>=22), jq.
#
# Issue: #303
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROG="$(basename "$0")"

# ── Defaults ──────────────────────────────────────────────────────────────────
NETWORK="${NETWORK:-local}"
SOURCE_ID="${SOURCE_ID:-health-probe-$$}"
JSON_OUT=0
PROM_MODE=0

# ── Colour helpers ────────────────────────────────────────────────────────────
RED='\033[0;31m'
YELLOW='\033[0;33m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Colour

# ── Status code ↔ label table ─────────────────────────────────────────────────
# Mirrors the status_code field of ProtocolHealthStatus (contracts/src/types.rs).
declare -A STATUS_LABEL=(
  [0]="HEALTHY"
  [1]="PAUSED"
  [2]="ORACLE_STALE"
  [3]="ROUND_STALE"
  [4]="NO_ACTIVE_ROUND"
  [5]="MULTIPLE_ISSUES"
  [6]="CLAIMS_ONLY"
)

declare -A STATUS_SEVERITY=(
  [0]="OK"
  [1]="CRIT"
  [2]="WARN"
  [3]="WARN"
  [4]="OK"
  [5]="CRIT"
  [6]="WARN"
)

declare -A ORACLE_STATUS_LABEL=(
  [0]="active"
  [1]="degraded"
  [2]="offline"
  [3]="unknown"
)

declare -A PHASE_LABEL=(
  [0]="no_round"
  [1]="betting"
  [2]="running"
  [3]="resolvable"
)

# ── Help ──────────────────────────────────────────────────────────────────────
usage() {
  cat <<EOF
Usage: $PROG <CONTRACT_ID> [OPTIONS]

Query the on-chain protocol health of a deployed Xelma contract and exit with
a Nagios-compatible status code suitable for monitoring pipelines.

Arguments:
  CONTRACT_ID          Stellar contract address (e.g. CDLZFC3SYJYDZT7K67VZ...)

Options:
  --network, -n NAME   Soroban network to query (default: local)
  --source, -s ID      Identity alias for the source account (default: auto-generated)
  --json, -j           Output JSON to stdout instead of human-readable text
  --prometheus, -p     Output Prometheus text exposition format (scrape target)
  --help, -h           Show this help and exit

Output formats are mutually exclusive: --json and --prometheus cannot be
combined, and both replace the human-readable scorecard.

Environment:
  NETWORK              Alias for --network (overridden by CLI flag)
  SOURCE_ID            Alias for --source  (overridden by CLI flag)

Exit codes:
  0  OK     — HEALTHY or NO_ACTIVE_ROUND (protocol nominal / idle)
  1  WARN   — ORACLE_STALE or ROUND_STALE (attention needed soon)
  2  CRIT   — PAUSED or MULTIPLE_ISSUES (immediate operator action required)
  3  UNKNOWN — Prerequisite missing or contract unreachable

Sample alerts (Prometheus-style; requires the --prometheus output, or use the
JSON field documented in README.md):
  # Critical: contract is paused
  - alert: XelmaContractPaused
    expr: health_probe_exit_code == 2
    for: 1m
    annotations:
      summary: "Xelma contract {{ \$labels.contract }} is paused or has multiple issues"

  # Warning: oracle heartbeat is stale
  - alert: XelmaOracleStale
    expr: health_probe_exit_code == 1
    for: 5m
    annotations:
      summary: "Xelma oracle heartbeat is stale for {{ \$labels.contract }}"
EOF
}

# ── JSON emitter ──────────────────────────────────────────────────────────────
emit_json() {
  local paused="$1" oracle_live="$2" oracle_status="$3"
  local has_active_round="$4" active_round_phase="$5"
  local schema_version="$6" ledger_sequence="$7" ledger_timestamp="$8"
  local status_code="$9" severity="${10}" contract_id="${11}" source="${12}"

  local oracle_status_str="${ORACLE_STATUS_LABEL[$oracle_status]:-unknown}"
  local phase_str="${PHASE_LABEL[$active_round_phase]:-unknown}"
  local label="${STATUS_LABEL[$status_code]:-UNKNOWN}"

  jq -nc \
    --arg contract_id "$contract_id" \
    --arg network "$NETWORK" \
    --arg source "$source" \
    --argjson paused "$paused" \
    --argjson oracle_live "$oracle_live" \
    --argjson oracle_status "$oracle_status" \
    --arg oracle_status_label "$oracle_status_str" \
    --argjson has_active_round "$has_active_round" \
    --argjson active_round_phase "$active_round_phase" \
    --arg round_phase_label "$phase_str" \
    --argjson schema_version "$schema_version" \
    --argjson ledger_sequence "$ledger_sequence" \
    --argjson ledger_timestamp "$ledger_timestamp" \
    --argjson status_code "$status_code" \
    --arg status_label "$label" \
    --arg severity "$severity" \
    '{
      contract_id: $contract_id,
      network: $network,
      source: $source,
      paused: $paused,
      oracle: { live: $oracle_live, status_code: $oracle_status, status_label: $oracle_status_label },
      round: { active: $has_active_round, phase_code: $active_round_phase, phase_label: $round_phase_label },
      schema_version: $schema_version,
      ledger: { sequence: $ledger_sequence, timestamp: $ledger_timestamp },
      health: { code: $status_code, label: $status_label, severity: $severity }
    }'
}

# ── Human-readable emitter ────────────────────────────────────────────────────
emit_text() {
  local paused="$1" oracle_live="$2" oracle_status="$3"
  local has_active_round="$4" active_round_phase="$5"
  local schema_version="$6" ledger_sequence="$7" ledger_timestamp="$8"
  local status_code="$9" severity="${10}" contract_id="${11}"

  local label="${STATUS_LABEL[$status_code]:-UNKNOWN}"
  local oracle_str="${ORACLE_STATUS_LABEL[$oracle_status]:-unknown}"
  local phase_str="${PHASE_LABEL[$active_round_phase]:-unknown}"

  case "$severity" in
    OK)   local colour="$GREEN";;
    WARN) local colour="$YELLOW";;
    CRIT) local colour="$RED";;
    *)    local colour="$NC";;
  esac

  echo ""
  echo -e "${BOLD}╔══════════════════════════════════════════════════════╗${NC}"
  echo -e "${BOLD}║${NC}  ${CYAN}Xelma Protocol Health Probe${NC}"
  echo -e "${BOLD}╠══════════════════════════════════════════════════════╣${NC}"
  printf "${BOLD}║${NC}  Contract:     %-35s ${BOLD}║${NC}\n" "$contract_id"
  printf "${BOLD}║${NC}  Network:      %-35s ${BOLD}║${NC}\n" "$NETWORK"
  printf "${BOLD}║${NC}  Ledger:       %-35s ${BOLD}║${NC}\n" "${ledger_sequence:-?} (ts=${ledger_timestamp:-?})"
  echo -e "${BOLD}╠══════════════════════════════════════════════════════╣${NC}"
  printf "${BOLD}║${NC}  Status:       ${colour}%-28s${NC} ${BOLD}║${NC}\n" "$label (code=$status_code)"
  printf "${BOLD}║${NC}  Severity:     ${colour}%-28s${NC} ${BOLD}║${NC}\n" "$severity"
  echo -e "${BOLD}╠══════════════════════════════════════════════════════╣${NC}"

  if [[ "$paused" == "true" ]]; then
    printf "${BOLD}║${NC}  ${RED}Paused:        YES — contract is emergency-paused${NC}   ${BOLD}║${NC}\n"
  else
    printf "${BOLD}║${NC}  Paused:        NO%-34s ${BOLD}║${NC}\n" ""
  fi

  if [[ "$oracle_live" == "true" ]]; then
    printf "${BOLD}║${NC}  Oracle:        ${GREEN}LIVE${NC} (status=%s)%-23s ${BOLD}║${NC}\n" "$oracle_str" ""
  else
    printf "${BOLD}║${NC}  Oracle:        ${RED}STALE/OFFLINE${NC} (status=%s)%-13s ${BOLD}║${NC}\n" "$oracle_str" ""
  fi

  if [[ "$has_active_round" == "true" ]]; then
    printf "${BOLD}║${NC}  Active Round:  YES (phase=%s)%-25s ${BOLD}║${NC}\n" "$phase_str" ""
  else
    printf "${BOLD}║${NC}  Active Round:  NO%-34s ${BOLD}║${NC}\n" ""
  fi

  printf "${BOLD}║${NC}  Schema v:      %-35s ${BOLD}║${NC}\n" "$schema_version"
  echo -e "${BOLD}╚══════════════════════════════════════════════════════╝${NC}"
  echo ""
}

# ── Prometheus emitters ───────────────────────────────────────────────────────
# Emits a complete, valid text exposition (see
# https://prometheus.io/docs/instrumenting/exposition_formats/). Metric names
# are stable API: do not rename them without updating README.md and dashboards.

# Probe could not collect a snapshot — still scrapeable, `up` reads 0.
prometheus_probe_down() {
  [[ "$PROM_MODE" -eq 1 ]] || return 0
  cat <<EOF
# HELP xelma_health_probe_up 1 when the probe collected a protocol health snapshot, 0 when the probe itself failed.
# TYPE xelma_health_probe_up gauge
xelma_health_probe_up{contract_id="${CONTRACT_ID:-unknown}",network="${NETWORK}"} 0
EOF
}

# Report UNKNOWN and exit 3, keeping Prometheus output parseable.
fail_unknown() {
  prometheus_probe_down
  echo "UNKNOWN: $1" >&2
  exit 3
}

emit_prometheus() {
  local paused="$1" oracle_live="$2" oracle_status="$3"
  local has_active_round="$4" active_round_phase="$5"
  local schema_version="$6" ledger_sequence="$7" ledger_timestamp="$8"
  local status_code="$9" severity="${10}" contract_id="${11}"
  local exit_code=3

  local paused_v=0 oracle_v=0 round_v=0
  if [[ "$paused" == "true" ]]; then paused_v=1; fi
  if [[ "$oracle_live" == "true" ]]; then oracle_v=1; fi
  if [[ "$has_active_round" == "true" ]]; then round_v=1; fi

  case "$severity" in
    OK)   exit_code=0;;
    WARN) exit_code=1;;
    CRIT) exit_code=2;;
  esac

  local label="${STATUS_LABEL[$status_code]:-UNKNOWN}"
  local oracle_str="${ORACLE_STATUS_LABEL[$oracle_status]:-unknown}"
  local phase_str="${PHASE_LABEL[$active_round_phase]:-unknown}"
  local base="contract_id=\"$contract_id\",network=\"$NETWORK\""

  cat <<EOF
# HELP xelma_health_probe_up 1 when the probe collected a protocol health snapshot, 0 when the probe itself failed.
# TYPE xelma_health_probe_up gauge
xelma_health_probe_up{$base} 1
# HELP xelma_health_status_code Composite status_code returned by get_protocol_health.
# TYPE xelma_health_status_code gauge
xelma_health_status_code{$base,status_label="$label",severity="$severity"} $status_code
# HELP xelma_health_exit_code Nagios-compatible exit code the probe returns (0=OK, 1=WARN, 2=CRIT, 3=UNKNOWN).
# TYPE xelma_health_exit_code gauge
xelma_health_exit_code{$base} $exit_code
# HELP xelma_health_paused 1 when the contract is emergency-paused (FullyPaused runtime mode).
# TYPE xelma_health_paused gauge
xelma_health_paused{$base} $paused_v
# HELP xelma_health_oracle_live 1 when the oracle heartbeat is live and fresh.
# TYPE xelma_health_oracle_live gauge
xelma_health_oracle_live{$base} $oracle_v
# HELP xelma_health_oracle_status_code Oracle heartbeat status_code (0=active, 1=degraded, 2=offline, 3=unknown).
# TYPE xelma_health_oracle_status_code gauge
xelma_health_oracle_status_code{$base,status_label="$oracle_str"} $oracle_status
# HELP xelma_health_has_active_round 1 when a round is active.
# TYPE xelma_health_has_active_round gauge
xelma_health_has_active_round{$base} $round_v
# HELP xelma_health_round_phase_code Active round phase (0=no_round, 1=betting, 2=running, 3=resolvable).
# TYPE xelma_health_round_phase_code gauge
xelma_health_round_phase_code{$base,phase_label="$phase_str"} $active_round_phase
# HELP xelma_health_schema_version On-chain storage schema version.
# TYPE xelma_health_schema_version gauge
xelma_health_schema_version{$base} $schema_version
# HELP xelma_health_ledger_sequence Ledger sequence at which the snapshot was taken.
# TYPE xelma_health_ledger_sequence gauge
xelma_health_ledger_sequence{$base} $ledger_sequence
# HELP xelma_health_ledger_timestamp Unix timestamp of the snapshot ledger.
# TYPE xelma_health_ledger_timestamp gauge
xelma_health_ledger_timestamp{$base} $ledger_timestamp
EOF
}

# ── Parse args ────────────────────────────────────────────────────────────────
CONTRACT_ID=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --network|-n)
      NETWORK="$2"; shift 2;;
    --source|-s)
      SOURCE_ID="$2"; shift 2;;
    --json|-j)
      JSON_OUT=1; shift;;
    --prometheus|-p)
      PROM_MODE=1; shift;;
    --help|-h)
      usage; exit 0;;
    -*)
      echo "Unknown option: $1" >&2; usage >&2; exit 3;;
    *)
      if [[ -z "$CONTRACT_ID" ]]; then
        CONTRACT_ID="$1"; shift
      else
        echo "Unexpected argument: $1" >&2; usage >&2; exit 3
      fi;;
  esac
done

if [[ "$JSON_OUT" -eq 1 && "$PROM_MODE" -eq 1 ]]; then
  echo "ERROR: --json and --prometheus are mutually exclusive." >&2
  usage >&2
  exit 3
fi

if [[ -z "$CONTRACT_ID" ]]; then
  echo "ERROR: CONTRACT_ID is required." >&2
  echo "" >&2
  echo "Provide the Stellar contract address as the first positional argument:" >&2
  echo "  $PROG CDLZFC3SYJYDZT7K67VZ..." >&2
  echo "" >&2
  echo "If no contract is deployed yet, first run:" >&2
  echo "  scripts/deploy_testnet.sh" >&2
  echo "or use the e2e smoke test to get a local instance:" >&2
  echo "  scripts/e2e_smoke.sh" >&2
  exit 3
fi

# Validate contract ID format (Stellar contract addresses start with 'C')
if [[ ! "$CONTRACT_ID" =~ ^C[A-Z0-9]{55}$ ]]; then
  echo "ERROR: CONTRACT_ID '$CONTRACT_ID' does not look like a Stellar contract address." >&2
  echo "Expected format: C followed by 55 uppercase alphanumeric characters." >&2
  exit 3
fi

# ── Preflight ─────────────────────────────────────────────────────────────────
if ! command -v stellar >/dev/null 2>&1; then
  fail_unknown "stellar CLI not found in PATH"
fi

if ! command -v jq >/dev/null 2>&1; then
  fail_unknown "jq not found in PATH"
fi

# Ensure the source identity exists (create a throwaway if needed)
if ! stellar keys address "$SOURCE_ID" --network "$NETWORK" >/dev/null 2>&1; then
  # For read-only calls we don't need a funded account — any keypair works.
  stellar keys generate "$SOURCE_ID" --network "$NETWORK" --overwrite >/dev/null 2>&1 || true
fi

# ── Query ─────────────────────────────────────────────────────────────────────
RAW_OUTPUT=""
if ! RAW_OUTPUT=$(stellar contract invoke \
    --id "$CONTRACT_ID" \
    --source "$SOURCE_ID" \
    --network "$NETWORK" \
    --send=no \
    -- \
    get_protocol_health 2>&1); then
  echo "$RAW_OUTPUT" >&2
  fail_unknown "contract invocation failed for $CONTRACT_ID on network '$NETWORK'"
fi

# ── Parse ─────────────────────────────────────────────────────────────────────
# The stellar CLI emits event-log lines on stderr (merged via 2>&1).
# The return value is the last line or a standalone value. Extract the JSON
# object that contains "status_code".
HEALTH_JSON=""
# `-c` keeps the object on one line: without it `jq` pretty-prints the status
# across several lines and `head -n1` would only capture the opening brace.
HEALTH_JSON=$(echo "$RAW_OUTPUT" | jq -ec 'select(.status_code != null)' 2>/dev/null | head -n1 || true)

if [[ -z "$HEALTH_JSON" ]]; then
  echo "Raw output:" >&2
  echo "$RAW_OUTPUT" >&2
  fail_unknown "could not parse ProtocolHealthStatus from contract output"
fi

# Extract fields
PAUSED=$(echo "$HEALTH_JSON" | jq -r '.paused')
ORACLE_LIVE=$(echo "$HEALTH_JSON" | jq -r '.oracle_live')
ORACLE_STATUS=$(echo "$HEALTH_JSON" | jq -r '.oracle_status')
HAS_ACTIVE_ROUND=$(echo "$HEALTH_JSON" | jq -r '.has_active_round')
ACTIVE_ROUND_PHASE=$(echo "$HEALTH_JSON" | jq -r '.active_round_phase')
SCHEMA_VERSION=$(echo "$HEALTH_JSON" | jq -r '.schema_version')
LEDGER_SEQUENCE=$(echo "$HEALTH_JSON" | jq -r '.ledger_sequence')
LEDGER_TIMESTAMP=$(echo "$HEALTH_JSON" | jq -r '.ledger_timestamp')
STATUS_CODE=$(echo "$HEALTH_JSON" | jq -r '.status_code')

# ── Severity mapping ──────────────────────────────────────────────────────────
SEVERITY="${STATUS_SEVERITY[$STATUS_CODE]:-UNKNOWN}"

# ── Output ────────────────────────────────────────────────────────────────────
if [[ "$JSON_OUT" -eq 1 ]]; then
  emit_json \
    "$PAUSED" "$ORACLE_LIVE" "$ORACLE_STATUS" \
    "$HAS_ACTIVE_ROUND" "$ACTIVE_ROUND_PHASE" \
    "$SCHEMA_VERSION" "$LEDGER_SEQUENCE" "$LEDGER_TIMESTAMP" \
    "$STATUS_CODE" "$SEVERITY" "$CONTRACT_ID" "$SOURCE_ID"
elif [[ "$PROM_MODE" -eq 1 ]]; then
  emit_prometheus \
    "$PAUSED" "$ORACLE_LIVE" "$ORACLE_STATUS" \
    "$HAS_ACTIVE_ROUND" "$ACTIVE_ROUND_PHASE" \
    "$SCHEMA_VERSION" "$LEDGER_SEQUENCE" "$LEDGER_TIMESTAMP" \
    "$STATUS_CODE" "$SEVERITY" "$CONTRACT_ID"
else
  emit_text \
    "$PAUSED" "$ORACLE_LIVE" "$ORACLE_STATUS" \
    "$HAS_ACTIVE_ROUND" "$ACTIVE_ROUND_PHASE" \
    "$SCHEMA_VERSION" "$LEDGER_SEQUENCE" "$LEDGER_TIMESTAMP" \
    "$STATUS_CODE" "$SEVERITY" "$CONTRACT_ID"
fi

# ── Exit ──────────────────────────────────────────────────────────────────────
case "$SEVERITY" in
  OK)   exit 0;;
  WARN) exit 1;;
  CRIT) exit 2;;
  *)    exit 3;;
esac
