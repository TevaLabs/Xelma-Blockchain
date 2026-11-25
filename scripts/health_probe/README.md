# Health Probe Scorecard

> **Issue #303** — Protocol health probe with OK/WARN/CRIT exit codes for
> operator dashboards, cron jobs, and CI pipelines.
>
> **Issue #855** — `--prometheus` text exposition format so the scorecard can be
> scraped directly by Prometheus / Grafana Agent.

## Quick start

```bash
# Against a local e2e test instance
./scripts/health_probe/health_probe.sh CDLZFC3SYJYDZT7K67VZ...

# Against testnet
./scripts/health_probe/health_probe.sh CDLZFC3SYJYDZT7K67VZ... --network testnet

# JSON output for machine consumption
./scripts/health_probe/health_probe.sh CDLZFC3SYJYDZT7K67VZ... --json

# Prometheus text exposition format for scraping (Issue #855)
./scripts/health_probe/health_probe.sh CDLZFC3SYJYDZT7K67VZ... --prometheus
```

Verify the Prometheus path offline (stubs the `stellar` CLI, no network):

```bash
./scripts/health_probe/test_prometheus.sh
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
| `--prometheus`, `-p`  | no       | `false`              | Output Prometheus text exposition    |
| `--help`, `-h`        | no       | —                    | Print usage help                     |

`--json` and `--prometheus` are mutually exclusive; passing both exits 3 with
usage help. Both replace the human-readable scorecard.

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

## Prometheus output (`--prometheus`)

Emits a Prometheus text exposition (format `0.0.4`) on stdout, so the probe can
be used directly as a scrape target or through the node-exporter textfile
collector. Every line carries `contract_id` and `network` labels, letting one
Prometheus instance monitor several deployments.

```console
$ ./scripts/health_probe/health_probe.sh CDLZ... --network testnet --prometheus
# HELP xelma_health_probe_up 1 when the probe collected a protocol health snapshot, 0 when the probe itself failed.
# TYPE xelma_health_probe_up gauge
xelma_health_probe_up{contract_id="CDLZ...",network="testnet"} 1
# HELP xelma_health_status_code Composite status_code returned by get_protocol_health.
# TYPE xelma_health_status_code gauge
xelma_health_status_code{contract_id="CDLZ...",network="testnet",status_label="HEALTHY",severity="OK"} 0
# HELP xelma_health_exit_code Nagios-compatible exit code the probe returns (0=OK, 1=WARN, 2=CRIT, 3=UNKNOWN).
# TYPE xelma_health_exit_code gauge
xelma_health_exit_code{contract_id="CDLZ...",network="testnet"} 0
# HELP xelma_health_paused 1 when the contract is emergency-paused (FullyPaused runtime mode).
# TYPE xelma_health_paused gauge
xelma_health_paused{contract_id="CDLZ...",network="testnet"} 0
# HELP xelma_health_oracle_live 1 when the oracle heartbeat is live and fresh.
# TYPE xelma_health_oracle_live gauge
xelma_health_oracle_live{contract_id="CDLZ...",network="testnet"} 1
# HELP xelma_health_oracle_status_code Oracle heartbeat status_code (0=active, 1=degraded, 2=offline, 3=unknown).
# TYPE xelma_health_oracle_status_code gauge
xelma_health_oracle_status_code{contract_id="CDLZ...",network="testnet",status_label="active"} 0
# HELP xelma_health_has_active_round 1 when a round is active.
# TYPE xelma_health_has_active_round gauge
xelma_health_has_active_round{contract_id="CDLZ...",network="testnet"} 1
# HELP xelma_health_round_phase_code Active round phase (0=no_round, 1=betting, 2=running, 3=resolvable).
# TYPE xelma_health_round_phase_code gauge
xelma_health_round_phase_code{contract_id="CDLZ...",network="testnet",phase_label="betting"} 1
# HELP xelma_health_schema_version On-chain storage schema version.
# TYPE xelma_health_schema_version gauge
xelma_health_schema_version{contract_id="CDLZ...",network="testnet"} 3
# HELP xelma_health_ledger_sequence Ledger sequence at which the snapshot was taken.
# TYPE xelma_health_ledger_sequence gauge
xelma_health_ledger_sequence{contract_id="CDLZ...",network="testnet"} 12345
# HELP xelma_health_ledger_timestamp Unix timestamp of the snapshot ledger.
# TYPE xelma_health_ledger_timestamp gauge
xelma_health_ledger_timestamp{contract_id="CDLZ...",network="testnet"} 1750000000
```

### Metrics

| Metric                                   | Type  | Description                                             |
|------------------------------------------|-------|---------------------------------------------------------|
| `xelma_health_probe_up`                  | gauge | `1` when a snapshot was collected, `0` when the probe itself failed (CLI missing, RPC unreachable, unparseable output). |
| `xelma_health_status_code`               | gauge | Raw composite `status_code` (see `docs/STATUS_CODES.md`); carries `status_label` and `severity` labels. |
| `xelma_health_exit_code`                 | gauge | The exit code this run returns: `0` OK, `1` WARN, `2` CRIT, `3` UNKNOWN. |
| `xelma_health_paused`                    | gauge | `1` when the contract is in `FullyPaused` runtime mode. |
| `xelma_health_oracle_live`               | gauge | `1` when the oracle heartbeat is live and fresh. |
| `xelma_health_oracle_status_code`        | gauge | Oracle heartbeat status code; carries a `status_label` label. |
| `xelma_health_has_active_round`          | gauge | `1` when a round is active. |
| `xelma_health_round_phase_code`          | gauge | Active round phase; carries a `phase_label` label. |
| `xelma_health_schema_version`            | gauge | On-chain storage schema version. |
| `xelma_health_ledger_sequence`           | gauge | Ledger sequence of the snapshot. |
| `xelma_health_ledger_timestamp`          | gauge | Unix timestamp of the snapshot ledger. |

Metric names and labels are a stable contract — dashboards depend on them, so
add new metrics rather than renaming existing ones.

### Scrape configuration

Point Prometheus at a wrapper that runs the probe on each scrape (the probe is
a one-shot CLI, so it exits after printing the exposition):

```yaml
scrape_configs:
  - job_name: xelma-health
    scrape_interval: 30s
    metrics_path: /probe
    static_configs:
      - targets: ["CDLZ..."]
    relabel_configs:
      - source_labels: [__address__]
        target_label: __param_contract_id
      - source_labels: [__param_contract_id]
        target_label: contract_id
      - target_label: __address__
        replacement: xelma-health-exporter:9100
```

If you would rather not run an exporter, cron-write the exposition to the
node-exporter textfile directory instead:

```bash
#!/usr/bin/env bash
# /etc/cron.d/xelma-health — runs every minute
OUT=/var/lib/prometheus/node-exporter/xelma_health.prom
scripts/health_probe/health_probe.sh "$CONTRACT_ID" --network mainnet --prometheus \
  > "$OUT.tmp" && mv "$OUT.tmp" "$OUT"
```

Writing to a temporary file and then renaming keeps node-exporter from reading
a half-written exposition.

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

Prefer the native `--prometheus` format documented above; it emits the full
metric set (including `xelma_health_probe_up`) without extra `jq` plumbing:

```bash
OUTFILE="${1:-/var/lib/prometheus/node-exporter/xelma_health.prom}"
scripts/health_probe/health_probe.sh "$CONTRACT_ID" --prometheus > "$OUTFILE.tmp" \
  && mv "$OUTFILE.tmp" "$OUTFILE"
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
        expr: xelma_health_status_code == 1
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Xelma contract {{ $labels.contract_id }} is paused"
          description: "Contract emergency-pause is active. Mutations are blocked."

      - alert: XelmaMultipleIssues
        expr: xelma_health_status_code == 5
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Xelma contract {{ $labels.contract_id }} has multiple issues"

      - alert: XelmaOracleStale
        expr: xelma_health_status_code == 2
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Xelma oracle heartbeat is stale for {{ $labels.contract_id }}"
          description: "Oracle has not reported a heartbeat within the configured threshold."

      - alert: XelmaRoundStale
        expr: xelma_health_status_code == 3
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Xelma round is stale for {{ $labels.contract_id }}"
          description: "Round has passed its end-ledger but has not been resolved."

      - alert: XelmaHealthProbeDown
        expr: xelma_health_probe_up == 0
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Xelma health probe failed for {{ $labels.contract_id }}"
```

## Related

- [`contracts/src/admin.rs`](../contracts/src/admin.rs) — `get_protocol_health()` implementation
- [`contracts/src/types.rs`](../contracts/src/types.rs) — `ProtocolHealthStatus` struct definition
- [`docs/STATUS_CODES.md`](../docs/STATUS_CODES.md) — Canonical status code reference
- [`docs/DEPLOYMENT_RUNBOOK.md`](../docs/DEPLOYMENT_RUNBOOK.md) — Deployment & monitoring signals
- [`docs/ORACLE_OPERATOR_RUNBOOK.md`](../docs/ORACLE_OPERATOR_RUNBOOK.md) — Oracle heartbeat management
