# Health Probe Scorecard

> **Issue #303** — Protocol health probe with OK/WARN/CRIT exit codes for
> operator dashboards, cron jobs, and CI pipelines.
>
> **Issue #717** — JSON output extended with `paused`, `policy` and
> `dispute_ledgers`.
>
> **Issue #719** — Native Prometheus text exposition output (`--prometheus`).

## Quick start

```bash
# Against a local e2e test instance
./scripts/health_probe/health_probe.sh CDLZFC3SYJYDZT7K67VZ...

# Against testnet
./scripts/health_probe/health_probe.sh CDLZFC3SYJYDZT7K67VZ... --network testnet

# JSON output for machine consumption
./scripts/health_probe/health_probe.sh CDLZFC3SYJYDZT7K67VZ... --json

# Prometheus text exposition format (Issue #719)
./scripts/health_probe/health_probe.sh CDLZFC3SYJYDZT7K67VZ... --prometheus
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
| `--prometheus`, `-p`  | no       | `false`              | Output Prometheus text format        |
| `--help`, `-h`        | no       | —                    | Print usage help                     |

Environment variables: `NETWORK`, `SOURCE_ID` (overridden by CLI flags).

## Exit code policy

| Exit | Severity | status_code        | Meaning                                              |
|------|----------|--------------------|------------------------------------------------------|
| 0    | OK       | 0 (HEALTHY)        | All subsystems nominal — oracle live, not paused.    |
| 0    | OK       | 4 (NO_ACTIVE_ROUND)| No active round; oracle is live. Idle is healthy.    |
| 1    | WARN     | 2 (ORACLE_STALE)   | Oracle heartbeat is stale or offline.                |
| 1    | WARN     | 3 (ROUND_STALE)    | Round past end-ledger but unresolved.                |
| 1    | WARN     | 6 (CLAIMS_ONLY)    | Protocol in ClaimsOnly mode (emergency restriction).  |
| 2    | CRIT     | 1 (PAUSED)         | Contract is emergency-paused.                        |
| 2    | CRIT     | 5 (MULTIPLE_ISSUES)| Two or more issues detected simultaneously.          |
| 3    | UNKNOWN  | —                  | CLI missing, contract unreachable, or parse failure. |

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
  "policy": {
    "code": 0,
    "label": "Equal"
  },
  "dispute_ledgers": 0,
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
| `policy.code`                  | u32     | Precision payout policy: 0=Equal, 1=StakeWeighted, `null` if unreadable |
| `policy.label`                 | string  | Human-readable policy label (Equal / StakeWeighted)  |
| `dispute_ledgers`              | u32     | Configured dispute-window length in ledgers, `null` if unreadable |
| `schema_version`               | u32     | On-chain storage schema version                      |
| `ledger.sequence`              | u32     | Ledger at which this snapshot was taken              |
| `ledger.timestamp`             | u64     | Unix timestamp of the ledger                         |

> **Issue #717**: `paused`, `policy` and `dispute_ledgers` are always present in
> the JSON output. `policy` and `dispute_ledgers` are sourced from the
> `get_precision_payout_policy` / `get_dispute_ledgers` contract getters. If
> either supplementary read fails, the field is emitted as `null` and the
> probe's core health verdict is unaffected.

## Prometheus text format (`--prometheus`)

`--prometheus` (`-p`) emits the scrapeable [Prometheus text exposition
format](https://prometheus.io/docs/instrumenting/exposition_formats/) directly
to stdout (Issue #719). It requires no `jq`, so it can be piped straight into a
textfile collector or a Prometheus `node_exporter` `.prom` file.

```
# HELP xelma_health_up 1 if the protocol health probe reached the contract, else 0.
# TYPE xelma_health_up gauge
xelma_health_up{contract="CDLZ...",network="local"} 1
# HELP xelma_health_status_code Composite ProtocolHealthStatus status code.
# TYPE xelma_health_status_code gauge
xelma_health_status_code{contract="CDLZ...",network="local"} 0
# HELP xelma_health_severity OK=0, WARN=1, CRIT=2, UNKNOWN=3.
# TYPE xelma_health_severity gauge
xelma_health_severity{contract="CDLZ...",network="local"} 0
# HELP xelma_health_paused 1 if the contract is emergency-paused.
# TYPE xelma_health_paused gauge
xelma_health_paused{contract="CDLZ...",network="local"} 0
# HELP xelma_health_oracle_live 1 if the oracle heartbeat is non-stale and online.
# TYPE xelma_health_oracle_live gauge
xelma_health_oracle_live{contract="CDLZ...",network="local"} 1
# HELP xelma_health_oracle_status Raw oracle status: 0=active, 1=degraded, 2=offline, 3=unknown.
# TYPE xelma_health_oracle_status gauge
xelma_health_oracle_status{contract="CDLZ...",network="local"} 0
# HELP xelma_health_has_active_round 1 if a round is currently active.
# TYPE xelma_health_has_active_round gauge
xelma_health_has_active_round{contract="CDLZ...",network="local"} 1
# HELP xelma_health_round_phase 0=no_round, 1=betting, 2=running, 3=resolvable.
# TYPE xelma_health_round_phase gauge
xelma_health_round_phase{contract="CDLZ...",network="local"} 1
# HELP xelma_health_policy Precision payout policy: 0=Equal, 1=StakeWeighted.
# TYPE xelma_health_policy gauge
xelma_health_policy{contract="CDLZ...",network="local"} 0
# HELP xelma_health_dispute_ledgers Configured dispute-window length in ledgers.
# TYPE xelma_health_dispute_ledgers gauge
xelma_health_dispute_ledgers{contract="CDLZ...",network="local"} 0
# HELP xelma_health_schema_version On-chain storage schema version.
# TYPE xelma_health_schema_version gauge
xelma_health_schema_version{contract="CDLZ...",network="local"} 3
# HELP xelma_health_ledger_sequence Ledger sequence of this snapshot.
# TYPE xelma_health_ledger_sequence gauge
xelma_health_ledger_sequence{contract="CDLZ...",network="local"} 12345
# HELP xelma_health_ledger_timestamp Unix timestamp of the ledger.
# TYPE xelma_health_ledger_timestamp gauge
xelma_health_ledger_timestamp{contract="CDLZ...",network="local"} 1750000000
```

| Metric                          | Type  | Description                                                     |
|---------------------------------|-------|-----------------------------------------------------------------|
| `xelma_health_up`               | gauge | 1 when the probe reached the contract, 0 otherwise              |
| `xelma_health_status_code`      | gauge | Composite `status_code` (0=HEALTHY … 7=ACCESS_RESTRICTED)       |
| `xelma_health_severity`         | gauge | OK=0, WARN=1, CRIT=2, UNKNOWN=3                                 |
| `xelma_health_paused`           | gauge | 1 when emergency-paused                                        |
| `xelma_health_oracle_live`      | gauge | 1 when the oracle heartbeat is non-stale and online            |
| `xelma_health_oracle_status`    | gauge | Raw oracle status: 0=active, 1=degraded, 2=offline, 3=unknown  |
| `xelma_health_has_active_round` | gauge | 1 when a round is active                                       |
| `xelma_health_round_phase`      | gauge | 0=no_round, 1=betting, 2=running, 3=resolvable                 |
| `xelma_health_policy`           | gauge | Precision payout policy: 0=Equal, 1=StakeWeighted              |
| `xelma_health_dispute_ledgers`  | gauge | Configured dispute-window length in ledgers (0 if unreadable)  |
| `xelma_health_schema_version`   | gauge | On-chain storage schema version                                |
| `xelma_health_ledger_sequence`  | gauge | Ledger sequence of the snapshot                                |
| `xelma_health_ledger_timestamp` | gauge | Unix timestamp of the ledger                                   |

All metrics carry `contract` and `network` labels. When the contract is
unreachable the script exits 3 (UNKNOWN) and emits no metric lines, so pair
`xelma_health_up` with an alert on scrape staleness (see
`XelmaHealthProbeDown` in the sample rules below) rather than relying on it
being present.

### Prometheus textfile collector

```bash
#!/usr/bin/env bash
# Writes the scrapeable metrics for node_exporter's textfile collector.
CONTRACT_ID="${1:?}"
OUTFILE="${2:-/var/lib/prometheus/node-exporter/xelma_health.prom}"
TMPFILE="${OUTFILE}.tmp"

# Exit code is ignored — the metrics themselves carry the verdict.
scripts/health_probe/health_probe.sh "$CONTRACT_ID" --network mainnet --prometheus \
  > "$TMPFILE" 2>/dev/null || true
mv "$TMPFILE" "$OUTFILE"
```

## Integration examples

### Cron-based heartbeat

```bash
#!/usr/bin/env bash
# /etc/cron.d/xelma-health — runs every minute
CONTRACT_ID="CDLZFC3SYJYDZT7K67VZ..."
/opt/xelma/scripts/health_probe/health_probe.sh "$CONTRACT_ID" --network mainnet --json \
  | tee /var/log/xelma/health.ndjson
```

### Prometheus textfile collector (native)

See the [`--prometheus` section](#prometheus-text-format---prometheus) above for
the recommended, dependency-free textfile collector that pipes the native metric
output straight into `node_exporter`.

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
        expr: xelma_health_status_code == 1
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Xelma contract {{ $labels.instance }} is paused"
          description: "Contract emergency-pause is active. Mutations are blocked."

      - alert: XelmaMultipleIssues
        expr: xelma_health_status_code == 5
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Xelma contract {{ $labels.instance }} has multiple issues"

      - alert: XelmaOracleStale
        expr: xelma_health_status_code == 2
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Xelma oracle heartbeat is stale for {{ $labels.instance }}"
          description: "Oracle has not reported a heartbeat within the configured threshold."

      - alert: XelmaRoundStale
        expr: xelma_health_status_code == 3
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
