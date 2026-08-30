# Health Probe Scorecard

> **Issue #303** — Protocol health probe with OK/WARN/CRIT exit codes for
> operator dashboards, cron jobs, and CI pipelines.

## Quick start

```bash
# Against a local e2e test instance
./scripts/health_probe/health_probe.sh CDLZFC3SYJYDZT7K67VZ...

# Against testnet
./scripts/health_probe/health_probe.sh CDLZFC3SYJYDZT7K67VZ... --network testnet

# JSON output for machine consumption
./scripts/health_probe/health_probe.sh CDLZFC3SYJYDZT7K67VZ... --json
```

## Prerequisites

- **stellar CLI** ≥ 22 (`stellar --version`)
- **jq** (any modern version)

## Arguments

| Argument              | Required | Default              | Description                          |
|-----------------------|----------|----------------------|--------------------------------------|
| `CONTRACT_ID`         | **yes**  | —                    | Stellar contract address             |
| `--network`, `-n`     | no       | `local`              | Soroban network to query             |
| `--source`, `-s`      | no       | auto-generated       | Identity alias for the source account|
| `--json`, `-j`        | no       | `false`              | Output machine-readable JSON         |
| `--help`, `-h`        | no       | —                    | Print usage help                     |

Environment variables: `NETWORK`, `SOURCE_ID` (overridden by CLI flags).

## Exit code policy

| Exit | Severity | status_code        | Meaning                                              |
|------|----------|--------------------|------------------------------------------------------|
| 0    | OK       | 0 (HEALTHY)        | All subsystems nominal — oracle live, not paused.    |
| 0    | OK       | 4 (NO_ACTIVE_ROUND)| No active round; oracle is live. Idle is healthy.    |
| 0    | OK       | 7 (ACCESS_RESTRICTED)| Allowlist mode enabled; protocol healthy but gated. |
| 1    | WARN     | 2 (ORACLE_STALE)   | Oracle heartbeat is stale or offline.                |
| 1    | WARN     | 3 (ROUND_STALE)    | Round past end-ledger but unresolved.                |
| 1    | WARN     | 6 (CLAIMS_ONLY)    | Protocol in ClaimsOnly mode (emergency restriction). |
| 2    | CRIT     | 1 (PAUSED)         | Contract is emergency-paused.                        |
| 2    | CRIT     | 5 (MULTIPLE_ISSUES)| Two or more issues detected simultaneously.          |
| 3    | UNKNOWN  | —                  | CLI missing, contract unreachable, parse failure,    |
|      |          |                    | or unrecognized status_code (probe out of date).     |

### No-round policy

`status_code = 4` (`NO_ACTIVE_ROUND`) exits **0 (OK)**. An idle protocol
with a live oracle and no pause is a normal operational state — it does not
indicate a problem. If your use case requires alerting on "no active round"
(e.g. for a live demo or hackathon), monitor the `has_active_round` field
via `--json` output instead of treating the exit code as authoritative.

## JSON output schema

```json
{
  "contract_id": "CDLZ...",
  "network": "local",
  "source": "health-probe-12345",
  "paused": false,
  "oracle": {
    "live": true,
    "status_code": 0,
    "status_label": "active"
  },
  "round": {
    "active": true,
    "phase_code": 1,
    "phase_label": "betting"
  },
  "schema_version": 3,
  "ledger": {
    "sequence": 12345,
    "timestamp": 1750000000
  },
  "health": {
    "code": 0,
    "label": "HEALTHY",
    "severity": "OK"
  }
}
```

| Field                          | Type    | Description                                          |
|--------------------------------|---------|------------------------------------------------------|
| `health.code`                  | u32     | Raw `status_code` from the contract                  |
| `health.label`                 | string  | Human-readable label (HEALTHY, PAUSED, etc.)         |
| `health.severity`              | string  | OK / WARN / CRIT / UNKNOWN                           |
| `paused`                       | bool    | Contract emergency-pause state                       |
| `oracle.live`                  | bool    | Oracle heartbeat is non-stale and not offline        |
| `oracle.status_code`           | u32     | Raw oracle status: 0=active, 1=degraded, 2=offline, 3=unknown |
| `round.active`                 | bool    | Whether a round is currently active                  |
| `round.phase_code`             | u32     | 0=no_round, 1=betting, 2=running, 3=resolvable       |
| `schema_version`               | u32     | On-chain storage schema version                      |
| `ledger.sequence`              | u32     | Ledger at which this snapshot was taken              |
| `ledger.timestamp`             | u64     | Unix timestamp of the ledger                         |

## Integration examples

### Cron-based heartbeat

```bash
#!/usr/bin/env bash
# /etc/cron.d/xelma-health — runs every minute
CONTRACT_ID="CDLZFC3SYJYDZT7K67VZ..."
/opt/xelma/scripts/health_probe/health_probe.sh "$CONTRACT_ID" --network mainnet --json \
  | tee /var/log/xelma/health.ndjson
```

### Prometheus textfile collector

```bash
#!/usr/bin/env bash
# Writes to /var/lib/prometheus/node-exporter/xelma_health.prom
CONTRACT_ID="${1:?}"
OUTFILE="${2:-/var/lib/prometheus/node-exporter/xelma_health.prom}"

JSON=$(scripts/health_probe/health_probe.sh "$CONTRACT_ID" --json 2>/dev/null) || true
if [[ -z "$JSON" ]]; then
  echo "xelma_health_up 0" > "$OUTFILE"
  exit 0
fi

echo "xelma_health_up 1" > "$OUTFILE"
echo "xelma_health_code $(echo "$JSON" | jq '.health.code')" >> "$OUTFILE"
echo "xelma_health_oracle_live $(echo "$JSON" | jq '.oracle.live | if . then 1 else 0 end')" >> "$OUTFILE"
echo "xelma_health_has_active_round $(echo "$JSON" | jq '.round.active | if . then 1 else 0 end')" >> "$OUTFILE"
echo "xelma_health_ledger_sequence $(echo "$JSON" | jq '.ledger.sequence')" >> "$OUTFILE"
```

### CI smoke gate

```yaml
# .github/workflows/ci.yml (excerpt)
- name: Deploy & probe health
  run: |
    bash scripts/e2e_smoke.sh &
    SMOKE_PID=$!
    # ... run other tests in parallel ...
    wait $SMOKE_PID

- name: Health probe
  run: |
    # CONTRACT_ID is printed by the deploy step; capture it from the log
    CONTRACT_ID=$(grep -oP 'Contract ID: \K.*' <(bash scripts/e2e_smoke.sh))
    scripts/health_probe/health_probe.sh "$CONTRACT_ID" --network local
```

### Sample alert rules (Prometheus / Alertmanager)

```yaml
groups:
  - name: xelma
    rules:
      - alert: XelmaContractPaused
        expr: xelma_health_code == 1
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Xelma contract {{ $labels.instance }} is paused"
          description: "Contract emergency-pause is active. Mutations are blocked."

      - alert: XelmaMultipleIssues
        expr: xelma_health_code == 5
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Xelma contract {{ $labels.instance }} has multiple issues"

      - alert: XelmaOracleStale
        expr: xelma_health_code == 2
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Xelma oracle heartbeat is stale for {{ $labels.instance }}"
          description: "Oracle has not reported a heartbeat within the configured threshold."

      - alert: XelmaRoundStale
        expr: xelma_health_code == 3
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Xelma round is stale for {{ $labels.instance }}"
          description: "Round has passed its end-ledger but has not been resolved."

      - alert: XelmaHealthProbeDown
        expr: xelma_health_up == 0
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Xelma health probe failed for {{ $labels.instance }}"
```

## Related

- [`contracts/src/admin.rs`](../contracts/src/admin.rs) — `get_protocol_health()` implementation
- [`contracts/src/types.rs`](../contracts/src/types.rs) — `ProtocolHealthStatus` struct definition
- [`docs/STATUS_CODES.md`](../docs/STATUS_CODES.md) — Canonical status code reference
- [`docs/DEPLOYMENT_RUNBOOK.md`](../docs/DEPLOYMENT_RUNBOOK.md) — Deployment & monitoring signals
- [`docs/ORACLE_OPERATOR_RUNBOOK.md`](../docs/ORACLE_OPERATOR_RUNBOOK.md) — Oracle heartbeat management
