# Oracle Operator Runbook

> **Canonical sources:** [PROTOCOL_SPEC.md](../PROTOCOL_SPEC.md) (invariants, trust model) |
> [EVENT_SCHEMA.md](./EVENT_SCHEMA.md) (event formats) |
> [contract.rs](../contracts/src/contract.rs) (implementation)

---

## Table of Contents

1. [Oracle Role & Responsibilities](#1-oracle-role--responsibilities)
2. [OraclePayload Field Reference](#2-oraclepayload-field-reference)
3. [Payload Templates](#3-payload-templates)
   - [3.6 Payload Simulator CLI](#36-payload-simulator-cli-scriptsoracle_payload_simpy)
4. [Heartbeat & Liveness](#4-heartbeat--liveness)
5. [Resolution Flow (Step by Step)](#5-resolution-flow-step-by-step)
6. [Troubleshooting Matrix](#6-troubleshooting-matrix)
7. [Escalation Procedures](#7-escalation-procedures)
8. [Operational Playbooks](#8-operational-playbooks)

---

## 1. Oracle Role & Responsibilities

The oracle is a **trusted signer** that resolves rounds by submitting the final
settlement price. It is a single address set during `initialize(admin, oracle)`
and must be distinct from the admin address.

**Duties:**

- Report accurate XLM prices for the active round within the freshness window.
- Maintain a regular on-chain heartbeat to prove liveness.
- Respond to deviation guardrails and admin-issued overrides.

**Trust boundary (from PROTOCOL_SPEC.md §Accepted Trust Boundaries):**

> The oracle is a single trusted signer in the current architecture.

---

## 2. OraclePayload Field Reference

The contract resolves rounds via `resolve_round(payload: OraclePayload)`. Each
field is validated in order; a failure at any step rejects the entire submission.

### Payload struct

| Field           | Type        | Required | Description |
|-----------------|-------------|----------|-------------|
| `price`         | `u128`      | Yes      | Final settlement price, 4 decimals (e.g. `2297` = $0.2297). Must be > 0. |
| `timestamp`     | `u64`       | Yes      | Unix epoch seconds when the price was observed. |
| `round_id`      | `u32`       | Yes      | Must match the active round's `start_ledger` (not `round_id`). |
| `nonce`         | `u64`       | Yes      | Per-round replay protection. Must be unique per round. |
| `network_id`    | `BytesN<32>`| Yes      | SHA-256 hash of the network passphrase. Prevents cross-network replay. |
| `contract_addr` | `Address`   | Yes      | The contract this payload targets. Prevents cross-contract replay. |
| `attestation`   | `BytesN<64>` | Only when a key is configured | Detached ed25519 signature over the domain-separated message (see attestation threat note below). Rejected if missing or invalid when an attestation key is set. |
| `confidence`    | `Option<u32>` | No      | Advisory belief strength in basis points. **Not** covered by the attestation signature. |

> **Attestation threat note (Issue #263):** when `set_attestation_key` is configured, the
> oracle signs a domain-separated message (`XELMA_ORACLE_ATTESTATION_V1` + the XDR encoding
> of network_id, contract_addr, round_id, price, timestamp, nonce) with a key that may differ
> from the submitting Soroban account. Verification is fail-closed: an invalid signature
> aborts the transaction before any state change, and the round is left resolvable. When no
> key is configured this gate is a no-op, preserving pre-#263 behaviour. The signature vouches
> for *binding* (correct network/contract/round/price/time/nonce), not for the correctness of
> the price itself — the oracle remains a trusted signer for price accuracy (§1).

### Validation order (`settlement.rs`)

```
1. price ≠ 0                          → InvalidPrice
2. oracle.require_auth()              → UnauthorizedOracle
3. contract not paused                → ContractPaused
4. active round exists                → NoActiveRound
5. round_id == ActiveRound.start_ledger → InvalidOracleRound
6. network_id matches env               → OracleNetworkMismatch
7. contract_addr matches                → OracleContractMismatch
8. timestamp ≤ now                     → FutureOracleData
9. timestamp inside round window
   [start_ts - skew, end_ts + skew]    → OracleTimestampOutsideWindow
10. deviation ≤ OracleMaxDeviationBps  → OracleDeviationExceeded
11. nonce not consumed                 → OracleNonceReused
12. current_ledger ≥ end_ledger        → RoundNotEnded
13. min-participants check             → fallback refund (not an error)
### Validation order (`contract.rs`)

```
1. price ≠ 0                         → InvalidPrice
2. oracle.require_auth()             → UnauthorizedOracle
3. contract not paused               → ContractPaused
4. heartbeat health (Issue #264)     → OracleHeartbeatUnhealthy
5. active round exists               → NoActiveRound
6. round_id == ActiveRound.start_ledger → InvalidOracleRound
7. network_id matches env              → OracleNetworkMismatch
8. contract_addr matches               → OracleContractMismatch
9. timestamp ≤ now                    → FutureOracleData
10. now - timestamp ≤ 300 s            → StaleOracleData
11. deviation ≤ OracleMaxDeviationBps  → OracleDeviationExceeded
12. nonce not consumed                → OracleNonceReused
13. current_ledger ≥ end_ledger       → RoundNotEnded
14. min-participants check            → fallback refund (not an error)
```

**Round window calculation:**
- `round_start` = timestamp recorded at round creation
- `round_end_estimate` = `round_start + (end_ledger - start_ledger) × 5s`
- `skew` = configurable `OracleTimestampSkew` (default 300s, range 0–86400s)
- `lower_bound` = `max(0, round_start - skew)`
- `upper_bound` = `round_end_estimate + skew`

### Field requirements in detail

**`price`**
- Scale: 4 decimal places. `1.2345 XLM` → `12345`.
- Must be non-zero. A zero price is always rejected (`InvalidPrice`).
- Compared against `round.price_start` for deviation guardrails.

**`timestamp`**
- Unix epoch **seconds** (not milliseconds or ledger sequence).
- Must not exceed `env.ledger().timestamp()` (rejects future data).
- Must fall within the round-relative economic window:
  `[round_start - skew, round_end_estimate + skew]`.
  This replaces the old absolute 300s freshness check and prevents
  wrong-phase prices from outside the round's active period.

**`round_id`**
- Must equal the **active round's** `start_ledger` (not the monotonically
  increasing `round_id` field in the `Round` struct). This is a known naming
  ambiguity tracked in `SECURITY_REVIEW.md` (SR-2026-04-003).
- The `Round.start_ledger` value is available via `get_active_round()`.

**`nonce`**
- 64-bit value, unique **per round**. The contract records
  `ConsumedOracleNonce(round_id, nonce)` after all validation passes and rejects
  any reuse.
- Recommended: a monotonic counter per round (0, 1, 2, …) or a random `u64`.
- Nonce collisions within a round cause `OracleNonceReused`. The rejected nonce
  is **not** consumed — you can retry with a different nonce.

**`network_id`**
- SHA-256 hash of the Stellar network passphrase:
  - Testnet: `"Test SDF Network ; September 2015"`
  - Future mainnet: `"Public Global Stellar Network ; September 2015"`
- Obtain at runtime via `env.ledger().network_id()` or the Stellar CLI.
- A mismatch produces `OracleNetworkMismatch`.

**`contract_addr`**
- The contract's own address (`env.current_contract_address()`).
- Obtain via `stellar contract id` or the SDK after deploy.
- A mismatch produces `OracleContractMismatch`.

---

## 3. Payload Templates

### 3.1 Up/Down Round — valid payload

```rust
OraclePayload {
    price: 12345,          // $1.2345 XLM (4 decimals)
    timestamp: 1700000000, // Unix epoch seconds
    round_id: 1234567,     // ActiveRound.start_ledger
    nonce: 1,              // First submission for this round
    network_id: BytesN::from_array(&env, &[/* SHA-256 of "Test SDF Network ; September 2015" */]),
    contract_addr: contract_id, // from deploy
}
```

### 3.2 Precision Round — valid payload

Precision rounds use the same `OraclePayload` type. The contract branches on
`Round.mode` internally.

```rust
OraclePayload {
    price: 12550,          // $1.2550 XLM — closest prediction wins
    timestamp: 1700000100,
    round_id: 1234567,     // ActiveRound.start_ledger
    nonce: 0,              // unique per round
    network_id: BytesN::from_array(&env, &[/* SHA-256 of "Test SDF Network ; September 2015" */]),
    contract_addr: contract_id,
}
```

### 3.3 Heartbeat — valid call

```rust
// Status: 0 = active, 1 = degraded, 2 = offline
update_oracle_heartbeat(env, 0); // "I am alive"
```

### 3.4 Admin deviation override — arming the one-shot

Before calling `resolve_round` with a price that exceeds the deviation
threshold, the admin must arm the override:

```rust
arm_oracle_deviation_override(env);
// Then the oracle can submit a deviating payload.
// Override is consumed after one use.
```

### 3.5 Fetching network_id (off-chain helper)

```typescript
import { hash, xdr } from '@stellar/stellar-sdk';

function networkIdFor(networkPassphrase: string): Buffer {
  return hash(Buffer.from(networkPassphrase, 'utf-8'));
}
// Testnet: networkIdFor("Test SDF Network ; September 2015")
```

### 3.6 Payload Simulator CLI (`scripts/oracle_payload_sim.py`)

A CLI tool to build, validate, and preview `OraclePayload`s *before* submitting a
real on-chain transaction. Catches common mistakes — wrong `network_id`, stale
timestamps, deviation overruns, malformed addresses — without consuming gas or
burning nonces.

**Usage:**

```bash
python3 scripts/oracle_payload_sim.py \
  --price 12345 \
  --round-id <start_ledger> \
  --network-id-from-passphrase "Test SDF Network ; September 2015" \
  --contract-addr <CONTRACT_ID>
```

**What it validates (all local, no RPC call needed):**

| Check | Requires | Rejects if |
|-------|----------|------------|
| Price > 0 | `--price` | Zero or negative |
| Timestamp freshness | `--timestamp` (default: now) | Future or >300 s old |
| round_id range | `--round-id` | Outside u32 |
| nonce range | `--nonce` (default: 1) | Outside u64 |
| network_id format | `--network-id` or `--network-id-from-passphrase` | Not 64 hex chars |
| contract_addr format | `--contract-addr` | Not a valid C… address |
| Confidence range | `--confidence` | Outside 0–10000 bps |
| Deviation guardrails | `--start-price` + `--max-deviation-bps` | Exceeds threshold |
| Confidence floor | `--confidence` + `--min-confidence-bps` | Below minimum |

**Example — full validation with deviation guardrails:**

```bash
python3 scripts/oracle_payload_sim.py \
  --price 15500 \
  --round-id 100 \
  --network-id-from-passphrase "Test SDF Network ; September 2015" \
  --contract-addr CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABLF4 \
  --start-price 15000 \
  --max-deviation-bps 5000 \
  --stellar-cli \
  --contract-id CCJZ5NJGZKP5I5TRLXU3M6YIQKU7D24B5E6Z6F6J4X5Y6Z7J6K5L4M3N \
  --oracle-key my-oracle-key
```

**Output** includes a copy-paste `stellar contract invoke` command at the bottom,
so the operator can paste it directly into their terminal.

**Exit codes:** `0` = all checks passed, `2` = one or more validation failures.

**No secrets:** The tool never requires a secret key — it is purely a pre-flight
check. The `--oracle-key` flag is only used for the copy-paste CLI command
template, not for signing.

---

## 4. Heartbeat & Liveness

The oracle should call `update_oracle_heartbeat(status)` at regular intervals to
prove liveness. **As of Issue #264, heartbeat liveness is enforced at settlement**
— a stale or unhealthy oracle cannot resolve rounds without an admin override.

| Status | Meaning |
|--------|---------|
| `0`    | Active — oracle is operating normally |
| `1`    | Degraded — partial price-feed outage, manual fallback in use |
| `2`    | Offline — oracle service is down |

**Staleness threshold:** configurable 60–86400 s (default 3600 s).
`is_oracle_live()` returns `false` if no heartbeat exists, status is `2`, or the
heartbeat is older than the threshold.

**Heartbeat health gate (strict mode, Issue #264):** When `HbGateConfig.strict_mode`
is enabled by the admin, `resolve_round` blocks settlement if the oracle heartbeat
is not live. An admin-armed one-shot override (`arm_hb_override`) or a configured
grace period (`HbGateConfig.grace_seconds`) can allow settlement through the gate.

**Recommended interval:** every 15–30 minutes for a 1-hour threshold.

### 4.1 Heartbeat health enforcement (Issue #264)

`resolve_round` now enforces heartbeat health **before** any state mutation
occurs. The enforcement gate is checked after oracle auth and pause checks,
but before nonce consumption and settlement logic.

**Allow/Deny Matrix:**

| Heartbeat        | Strict Mode | In Grace Window | Override Armed | Result         |
|------------------|-------------|-----------------|----------------|----------------|
| Active + Fresh   | any         | any             | any            | ALLOW          |
| Degraded + Fresh | off         | any             | any            | ALLOW          |
| Degraded + Fresh | on          | any             | no             | DENY (66)      |
| Stale            | off         | yes             | no             | ALLOW (grace)  |
| Stale            | off         | no              | no             | DENY (66)      |
| Stale            | on          | any             | no             | DENY (66)      |
| Offline (2)      | any         | any             | no             | DENY (66)      |
| No heartbeat     | n/a         | n/a             | no             | DENY (66)      |
| ANY              | any         | any             | yes            | ALLOW*         |

\* Override is consumed (one-shot), emitting `(oracle, hb_override)`.

**Key parameters:**

| Parameter | Default | Range | Query |
|-----------|---------|-------|-------|
| Stale threshold | 3600 s | 60–86400 s | `get_oracle_stale_threshold()` |
| Grace period | 600 s | 0–86400 s | `get_oracle_heartbeat_grace()` |
| Strict mode | false | bool | `get_oracle_heartbeat_strict_mode()` |
| Override armed | false | bool | `is_oracle_heartbeat_override_armed()` |

**Grace period** is an additional window beyond the stale threshold.
For example, with a 3600 s stale threshold and 600 s grace:
- 0–3600 s after heartbeat: fresh → ALLOW
- 3600–4200 s after heartbeat: stale but within grace → ALLOW (non-strict)
- 4200+ s after heartbeat: stale beyond grace → DENY

**Strict mode** eliminates the grace period and blocks degraded-status
settlement entirely. When enabled, only active+fresh heartbeats allow
settlement (plus the admin override).

---

## 5. Resolution Flow (Step by Step)

### 5.1 Legacy Single-Feed Path (`resolve_round`)

1. **Verify round eligibility**
   - `get_active_round()` returns a round.
   - `env.ledger().sequence() >= round.end_ledger`.

2. **Obtain settlement price**
   - Fetch XLM/USD (or XLM/whatever) from your price feed.
   - Scale to 4 decimal places: `Math.round(price * 10000)`.

3. **Build payload**
   - `round_id` = `active_round.start_ledger`.
   - `nonce` = next unused value for this round (start at 0, increment).
   - `network_id` = SHA-256 of the network passphrase.
   - `contract_addr` = the deployed contract address.

4. **Submit resolve_round**
   - Sign with the oracle key.
   - If `Ok(())` → round resolved. Monitor the `("round", "summary")` event.
   - If `Err(...)` → see [Troubleshooting Matrix](#6-troubleshooting-matrix).

5. **Handle fallback**
   - If `Ok(())` but the round emitted `("round", "summary")` with status `2 (FallbackRefund)`, the round had
     too few participants and stakes were refunded. No competitive settlement
     occurred. This is not an error.

6. **Advance nonce**
   - If the call fails with `OracleNonceReused`, increment the nonce and retry.
   - If the call fails for any other reason, the nonce is **not** consumed and
     can be reused.

### 5.2 Multi-Feed Path (`resolve_round_multi`) — Issue #262

When `OracleQuorumConfig` is configured by the admin, the oracle SHOULD use
`resolve_round_multi` for improved settlement safety. This path accepts N
independent feed observations, computes the median price, rejects outliers,
and requires a quorum of feeds to agree.

**MultiFeedPayload structure:**

| Field           | Type          | Required | Description |
|-----------------|---------------|----------|-------------|
| `prices`        | `Vec<u128>`   | Yes      | N feed prices, 4 decimals, all non-zero. |
| `sources`       | `Vec<u32>`    | Yes      | N feed identifiers, 0-based, must all be unique. |
| `round_id`      | `u32`         | Yes      | Must match `ActiveRound.start_ledger`. |
| `nonce`         | `u64`         | Yes      | Per-round replay protection, unique per round. |
| `network_id`    | `BytesN<32>`  | Yes      | SHA-256 of network passphrase. |
| `contract_addr` | `Address`     | Yes      | Target contract address. |
| `timestamp`     | `u64`         | Yes      | Unix epoch seconds when observations were collected. |

**Quorum configuration (set by admin):**

| Parameter             | Range    | Default | Description |
|-----------------------|----------|---------|-------------|
| `min_observations`    | 3–32     | 3       | Minimum feeds in payload. |
| `quorum_threshold`    | 3–min_obs| 3       | Minimum survivors for settlement. |
| `outlier_threshold_bps`| 1–10000 | 500     | Max deviation from median (5% default). |

**Multi-feed resolution steps:**

1. **Collect feed observations**
   - Query N independent price feeds (e.g., Coinbase, Binance, Kraken).
   - Each feed must have a unique `source` identifier.
   - All prices scaled to 4 decimal places, non-zero.

2. **Build payload**
   - `prices`: ordered list of feed prices.
   - `sources`: corresponding feed source IDs (0, 1, 2, …).
   - Same `round_id`, `nonce`, `network_id`, `contract_addr` conventions
     as the legacy path.

3. **Submit `resolve_round_multi(payload)`**
   - Signed with the oracle key (same key as legacy path).

4. **On-chain processing:**
   a. Validates payload (lengths match, prices > 0, sources unique).
   b. Checks `n >= min_observations` and `n <= 32` (gas cap).
   c. Sorts prices, computes **median**.
   d. Applies deviation guardrail (same `OracleMaxDeviationBps` as legacy).
   e. **Parity protections**:
      - **Domain binding**: `network_id` and `contract_addr` verified against execution context.
      - **Nonce tracking**: `(round_id, nonce)` uniqueness enforced against `ConsumedOracleNonce`.
      - **Round binding**: `payload.round_id` must match `round.start_ledger`.
      - **Timestamp window**: must fall inside `[start - skew, end + skew]` (`OracleTimestampOutsideWindow`).
      - **Heartbeat gate**: strict mode requires live heartbeat (`OracleNotLive`) unless admin override is armed.
   f. **Outlier rejection**: each feed's price is compared to the median.
      If `|price - median| / median * 10000 > outlier_threshold_bps`,
      the observation is rejected.
   g. **Quorum check**: surviving count must be `>= quorum_threshold`.
   h. If quorum passes, the **median price** is used for settlement.

5. **Events emitted:**
   - `("oracle", "multisum")` — summary of multi-feed resolution (round_id,
     observation_count, survivor_count, median_price, quorum_threshold).
   - `("round", "resolved")` — standard resolved event.
   - If quorum fails: `("oracle", "nofed")` — failure details.

**Demo scenario:**
Run the end-to-end multi-feed demo scenario via:
```bash
./scripts/demo_scenarios/scenario_multi_feed.sh
```

**Example multi-feed payload (3 feeds):**

```rust
MultiFeedPayload {
    // Feed 0: $0.2300, Feed 1: $0.2310, Feed 2: $0.2295
    prices: vec![&env, 2300u128, 2310u128, 2295u128],
    sources: vec![&env, 0u32, 1u32, 2u32],
    round_id: 1234567,       // ActiveRound.start_ledger
    nonce: 1,
    network_id: BytesN::from_array(&env, &[/* SHA-256 of passphrase */]),
    contract_addr: contract_id,
    timestamp: 1700000000,
}
```

---

## 6. Troubleshooting Matrix

| Error | Code | Likely Cause | Check | Fix |
|-------|------|--------------|-------|-----|
| `OracleTimestampOutsideWindow` | 66 | Payload timestamp is outside the round-relative window `[start - skew, end + skew]` | Compare `payload.timestamp` against `get_active_round()` timestamps and `get_oracle_timestamp_skew()`. | Ensure the price was observed during the round's active window. The timestamp must be after round creation and before the estimated round end + skew. |
| `InvalidOracleRound` | 19 | `payload.round_id` does not match `ActiveRound.start_ledger` | Call `get_active_round()` — verify `start_ledger`. Note: it's `start_ledger`, not `round_id`! | Set `payload.round_id = start_ledger` from the active round. |
| `FutureOracleData` | 24 | `payload.timestamp > env.ledger().timestamp()` | Check system clock skew vs ledger time. Oracle machine's clock may be ahead. | Use `Date.now() / 1000` or NTP-synchronised time; never fabricate timestamps. |
| `OracleNonceReused` | 33 | `(round_id, nonce)` pair was already consumed | Check the oracle's nonce tracking for this round. | Increment the nonce value and resubmit. |
| `OracleDeviationExceeded` | 41 | Price deviation > configured `OracleMaxDeviationBps` | Compute `diff_bps = abs(price - start_price) * 10000 / start_price`. | Either wait for market stability, ask admin to [arm the override](#34-admin-deviation-override--arming-the-one-shot), or adjust `OracleMaxDeviationBps` via config timelock. |
| `OracleNetworkMismatch` | 49 | `payload.network_id` does not match runtime network | Verify which network the contract is deployed on. | Hash the correct passphrase. |
| `OracleContractMismatch` | 50 | `payload.contract_addr` does not match this contract | Confirm the contract address used in the payload. | Update the payload with the correct address. |
| `UnauthorizedOracle` | 5 | Caller is not the configured oracle address | `get_oracle()` returns the authorised signer. | Check which key is signing. |
| `ContractPaused` | 22 | Admin has paused the contract | `is_paused()` returns `true`. | Contact admin to unpause. Do not submit while paused (waste of gas). |
| `NoActiveRound` | 7 | No round is currently active | `get_active_round()` returns `None`. | Verify the round hasn't already been resolved or cancelled. Check `LastRoundId` to see if a round recently ended. |
| `RoundNotEnded` | 16 | `current_ledger < round.end_ledger` | Query `get_active_round()` and compare `end_ledger` with the latest ledger. | Wait for the round to reach `end_ledger` before submitting. |
| `InvalidPrice` | 12 | `payload.price == 0` | Check the price feed output. | Ensure price is > 0 before building the payload. |
| `OracleNotSet` | 3 | Oracle address was never initialised | `get_oracle()` returns nothing. | Contact admin to call `initialize(admin, oracle)`. |
| `OracleHeartbeatUnhealthy` | 66 | Heartbeat is stale, offline, degraded (strict), or missing | Query `get_oracle_heartbeat()`, `get_oracle_heartbeat_grace()`, `get_oracle_heartbeat_strict_mode()`. | Restore heartbeat service and re-submit, or ask admin to [arm the heartbeat override](#75-heartbeat-override-admin-arms-oracle-uses). |

### Error recursion risk

Most validation failures (except `OracleNonceReused`) do **not** consume the
nonce. You can safely retry with the same nonce after fixing the underlying
issue.

---

## 7. Escalation Procedures

### 7.1 When to escalate

- Oracle service is unable to fetch a price (feed outage, exchange downtime).
- Price deviation guardrail is blocking a legitimate settlement.
- Contract is paused and the admin is unreachable.
- Repeated `OracleTimestampOutsideWindow` despite fresh payloads (severe clock drift or bug).

### 7.2 Pause the contract (admin only)

Freezes all mutation. Use when a price-feed outage or bug is actively causing
harm.

```
admin calls: pause_contract()
recovery:    unpause_contract() when safe
```

Events are still readable. Do not submit resolution payloads while paused
(they will fail with `ContractPaused`).

### 7.3 Cancel the active round (admin only)

Refunds all participant stakes. Use when the round cannot be resolved
(e.g. prolonged oracle outage, contract bug).

```
admin calls: cancel_round(reason)
```

Cancelled rounds emit `("round", "summary")` with status `1 (Cancelled)` and are archived. A cancelled
round **cannot** be resolved later — any `resolve_round` targeting it will fail.

### 7.4 Deviation override (admin arms, oracle uses)

When a legitimate price movement exceeds the configured deviation threshold,
the admin can arm a one-shot override:

```
admin calls: arm_oracle_deviation_override()
oracle calls: resolve_round(payload)          // bypasses deviation check once
```

The override is consumed after one successful settlement. It does **not**
persist across rounds.

**When to use:**
- High volatility where the price moves beyond the BPS threshold.
- The threshold was set too tight and a timelock change would be too slow.

**When NOT to use:**
- As a workflow bypass. Prefer adjusting `OracleMaxDeviationBps` via timelock
  for persistent changes.

### 7.5 Heartbeat override (admin arms, oracle uses)

When the oracle heartbeat is unhealthy (stale, degraded, offline, or missing)
and the round must still be settled, the admin can arm a one-shot heartbeat
override:

```
admin calls: arm_oracle_heartbeat_override()
oracle calls: resolve_round(payload)          // bypasses heartbeat check once
```

**Events:**
- `(oracle, hb_arm_ovr)` — emitted when the override is armed
- `(oracle, hb_override)` — emitted when the override is consumed during settlement

The override is consumed after one settlement. It does **not** persist across
rounds and does **not** bypass deviation or confidence guardrails — it only
suppresses `OracleHeartbeatUnhealthy (66)`.

**When to use:**
- Oracle service is experiencing a brief outage but the price data is valid.
- Heartbeat infrastructure failed while the oracle itself is healthy.
- Emergency settlement needed during a heartbeat system migration.

**When NOT to use:**
- As a permanent workflow bypass. Restore heartbeat infrastructure promptly.
- When the oracle's price data may also be unreliable (use `cancel_round` instead).

### 7.6 Config timelock (admin initiates)

Most oracle safety parameters are changed via the timelock. The change is
scheduled and activates after a cooldown.

| Parameter | Schedule function | Range |
|-----------|-------------------|-------|
| Oracle stale threshold | `set_oracle_stale_threshold(seconds)` / `schedule_oracle_stale_threshold(seconds)` | 60–86400 s |
| Oracle max deviation BPS | `schedule_oracle_max_deviation_bps(bps)` | 1–100000 bp |
| Oracle timestamp skew | `schedule_oracle_timestamp_skew(seconds)` | 0–86400 s |

---

## 8. Operational Playbooks

### Playbook A: Normal round resolution

```
1. Wait for current_ledger >= round.end_ledger
2. Fetch price from primary feed
3. Build OraclePayload (template §3.1 or §3.2)
4. Submit resolve_round(payload)
5. On Ok(()):  Verify ("round", "summary") event
6. On Err(e):  Consult troubleshooting matrix, fix, retry
```

### Playbook B: Timestamp outside round window

```
Symptom:  resolve_round returns OracleTimestampOutsideWindow
Diagnosis:
  - Round economic window: [round_start - skew, round_end_estimate + skew]
  - round_start = timestamp recorded at round creation
  - round_end_estimate = round_start + (end_ledger - start_ledger) * 5s
  - skew = get_oracle_timestamp_skew() (default 300s)
  - Verify payload.timestamp is within this window.
Fix:
  1. Ensure the price observation timestamp falls after round creation
     and before the estimated round end + skew.
  2. If the round took longer than expected (e.g. slow ledgers), the
     end_estimate may underestimate the true end — admin can increase
     OracleTimestampSkew via timelock if this is recurring.
  3. Do NOT fabricate timestamps — use the actual observation time
     from your price feed.
```

### Playbook C: Deviation guardrail trip

```
Symptom:  resolve_round returns OracleDeviationExceeded
Diagnosis:
  1. Compute: diff_bps = abs(price - start_price) * 10000 / start_price
  2. Query OracleMaxDeviationBps to confirm threshold.
Decision:
  - Is the price legitimate (not a feed error)?
    YES → Option A (admin arms override), then resubmit.
    NO  → Fix the feed, rebuild payload with correct price.
  - Is this a persistent condition? → Admin should schedule a higher
    OracleMaxDeviationBps via timelock.
```

### Playbook D: Oracle service goes down

```
1. Log heartbeat as "degraded" (status = 1) if partial outage.
2. Attempt to restore price feed.
3. If restore takes longer than the active round's end:
   - Contact admin to cancel the round (admin cancel_round).
   - **OR** admin arms heartbeat override: `arm_oracle_heartbeat_override()`
     then oracle resolves. Override is one-shot.
   - After cancel, users can claim their refunded stakes.
4. Once service is fully restored:
   - Log heartbeat as "active" (status = 0).
   - Admin may unpause if paused.
   - Resume normal resolution for future rounds.
```

### Playbook E: Nonce collision

```
Symptom:  OracleNonceReused
Cause:    Duplicate submission or nonce counter bug.
Fix:      Increment nonce and retry. The failed nonce is NOT consumed.
Prevention:
  - Use a monotonic counter per round stored in your oracle service.
  - After a successful resolution, persist the consumed nonce off-chain
    so the next round starts fresh.
```

### Playbook F: Multi-feed quorum failure

```
Symptom:  resolve_round_multi returns InsufficientOracleQuorum
Diagnosis:
  1. Check the ("oracle", "nofed") event for survivor_count vs quorum_threshold.
  2. Determine which feeds are producing outlier prices.
Decision:
  - One feed consistently out of line? Disable it from the payload.
  - Market conditions causing wide spreads?
    → Admin can increase outlier_threshold_bps via set_oracle_quorum_config.
    → Or reduce quorum_threshold if fewer feeds are available.
  - All feeds diverging? Consider falling back to legacy resolve_round.
```

### Playbook G: Multi-feed duplicate sources

```
Symptom:  resolve_round_multi returns DuplicateOracleSource
Cause:    Two or more observations share the same source identifier.
Fix:      Ensure each feed has a unique source ID (0, 1, 2, …).
Prevention:
  - Maintain a mapping of feed name → source ID in your oracle service.
  - Validate sources are unique before building the payload.
```

---

## 9. Oracle Rotation (Two-Step with Mandatory Delay)

The oracle address can be rotated through a two-step process: first the admin
proposes a new oracle, then **any caller** can accept the proposal after a
**mandatory 1-hour delay** has elapsed. This delay prevents quiet one-block
takeovers — even if the admin key is compromised, the community has a full hour
to observe the `(oracle, propose)` event and react before the oracle actually
changes.

### 9.1 Rotation lifecycle

```
Admin proposes ──→ (1 hour delay) ──→ Anyone accepts ──→ Oracle rotated
     │                                       │
     └── Admin can cancel at any time ───────┘
```

### 9.2 Constants

| Constant | Value | Meaning |
|----------|-------|---------|
| `MIN_ROTATION_DELAY_SECONDS` | 3 600 (1 hour) | Minimum time between proposal and acceptance |
| `MIN_ROTATION_EXPIRY_SECONDS` | 60 (1 minute) | Minimum expiry window after delay |

### 9.3 Entry points

**`propose_oracle_rotation(new_oracle, expires_in_seconds)`** (admin only)
- Proposes a new oracle address.
- `expires_in_seconds` must be ≥ 3 600 (the mandatory delay).
- Emits `(oracle, propose)`.

**`accept_oracle_rotation()`** (any caller)
- Accepts the pending proposal after the delay has elapsed.
- Fails with `RotationDelayNotElapsed` if called too early.
- Fails with `NoPendingRotation` if the proposal has expired.
- Emits `(oracle, accept)` on success.

**`cancel_oracle_rotation()`** (admin only)
- Cancels a pending proposal at any time.
- Emits `(oracle, cancel)`.

### 9.4 Events

| Event | Emitted when |
|-------|-------------|
| `(oracle, propose)` | Admin proposes a new oracle |
| `(oracle, accept)` | Rotation is successfully accepted after delay |
| `(oracle, cancel)` | Admin cancels a pending proposal |
| `(oracle, expired)` | Proposal expires (auto-cleaned) |
| `(oracle, early)` | Acceptance attempted before delay elapsed |

### 9.5 Monitoring checklist

Operators and monitoring dashboards should:
1. Watch for `(oracle, propose)` events — these signal an impending rotation.
2. If the proposed address is unexpected, escalate to the admin within the
   1-hour delay window.
3. Watch for `(oracle, early)` events — these indicate someone is trying to
   bypass the delay.
4. After `(oracle, accept)`, verify the new oracle by calling `get_oracle()`.

---

## Related Documents

| Document | Contents |
|----------|----------|
| [PROTOCOL_SPEC.md](../PROTOCOL_SPEC.md) | Protocol invariants I1–I13, trust boundaries, threat model |
| [EVENT_SCHEMA.md](./EVENT_SCHEMA.md) | All 10 on-chain event types with field encodings |
| [SECURITY_REVIEW.md](../SECURITY_REVIEW.md) | Accepted risks (single oracle, round_id ambiguity) |
| [ROUND_LIFECYCLE.md](../ROUND_LIFECYCLE.md) | Round state machine from creation through resolution |
| [STORAGE_DESIGN.md](../STORAGE_DESIGN.md) | On-chain key layout including oracle heartbeat and nonce entries |
| [contract.rs](../contracts/src/contract.rs) | `resolve_round` implementation (line 1488) |
| [errors.rs](../contracts/src/errors.rs) | All 50 `ContractError` variants |
| [types.rs](../contracts/src/types.rs) | `OraclePayload` struct definition |
| [oracle_payload_sim.py](../scripts/oracle_payload_sim.py) | Pre-flight payload validation CLI |
