# Oracle Operator Runbook

> **Canonical sources:** [PROTOCOL_SPEC.md](../PROTOCOL_SPEC.md) (invariants, trust model) |
> [EVENT_SCHEMA.md](./EVENT_SCHEMA.md) (event formats) |
> [contract.rs](../contracts/src/contract.rs) (implementation)

---

## Table of Contents

1. [Oracle Role & Responsibilities](#1-oracle-role--responsibilities)
2. [OraclePayload Field Reference](#2-oraclepayload-field-reference)
3. [Payload Templates](#3-payload-templates)
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

### Validation order (`contract.rs:1488`)

```
1. price ≠ 0                         → InvalidPrice
2. oracle.require_auth()             → UnauthorizedOracle
3. contract not paused               → ContractPaused
4. active round exists               → NoActiveRound
5. round_id == ActiveRound.start_ledger → InvalidOracleRound
6. network_id matches env              → OracleNetworkMismatch
7. contract_addr matches               → OracleContractMismatch
8. timestamp ≤ now                    → FutureOracleData
9. now - timestamp ≤ 300 s            → StaleOracleData
10. deviation ≤ OracleMaxDeviationBps  → OracleDeviationExceeded
11. nonce not consumed                → OracleNonceReused
12. current_ledger ≥ end_ledger       → RoundNotEnded
13. min-participants check            → fallback refund (not an error)
```

### Field requirements in detail

**`price`**
- Scale: 4 decimal places. `1.2345 XLM` → `12345`.
- Must be non-zero. A zero price is always rejected (`InvalidPrice`).
- Compared against `round.price_start` for deviation guardrails.

**`timestamp`**
- Unix epoch **seconds** (not milliseconds or ledger sequence).
- Must not exceed `env.ledger().timestamp()` (rejects future data).
- Must be within 300 seconds of `env.ledger().timestamp()` (300 s = 5 min stale window).

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

---

## 4. Heartbeat & Liveness

The oracle should call `update_oracle_heartbeat(status)` at regular intervals to
prove liveness.

| Status | Meaning |
|--------|---------|
| `0`    | Active — oracle is operating normally |
| `1`    | Degraded — partial price-feed outage, manual fallback in use |
| `2`    | Offline — oracle service is down |

**Staleness threshold:** configurable 60–86400 s (default 3600 s).
`is_oracle_live()` returns `false` if no heartbeat exists, status is `2`, or the
heartbeat is older than the threshold.

**Recommended interval:** every 15–30 minutes for a 1-hour threshold.

---

## 5. Resolution Flow (Step by Step)

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
   - If `Ok(())` → round resolved. Monitor the `("round", "resolved")` event.
   - If `Err(...)` → see [Troubleshooting Matrix](#6-troubleshooting-matrix).

5. **Handle fallback**
   - If `Ok(())` but the round emitted `("round", "fallback")`, the round had
     too few participants and stakes were refunded. No competitive settlement
     occurred. This is not an error.

6. **Advance nonce**
   - If the call fails with `OracleNonceReused`, increment the nonce and retry.
   - If the call fails for any other reason, the nonce is **not** consumed and
     can be reused.

---

## 6. Troubleshooting Matrix

| Error | Code | Likely Cause | Check | Fix |
|-------|------|--------------|-------|-----|
| `StaleOracleData` | 18 | Payload timestamp is >300 s older than `env.ledger().timestamp()` | Compare `payload.timestamp` vs ledger timestamp. Ledger may be slow. | Fetch a fresh price from the feed and rebuild the payload. |
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
- Repeated `StaleOracleData` despite fresh payloads (severe clock drift or bug).

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

Cancelled rounds emit `("round", "cancelled")` and are archived. A cancelled
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

### 7.5 Config timelock (admin initiates)

Most oracle safety parameters are changed via the timelock. The change is
scheduled and activates after a cooldown.

| Parameter | Schedule function | Range |
|-----------|-------------------|-------|
| Oracle stale threshold | `set_oracle_stale_threshold(seconds)` / `schedule_oracle_stale_threshold(seconds)` | 60–86400 s |
| Oracle max deviation BPS | `schedule_oracle_max_deviation_bps(bps)` | 1–100000 bp |

---

## 8. Operational Playbooks

### Playbook A: Normal round resolution

```
1. Wait for current_ledger >= round.end_ledger
2. Fetch price from primary feed
3. Build OraclePayload (template §3.1 or §3.2)
4. Submit resolve_round(payload)
5. On Ok(()):  Verify ("round", "resolved") event
6. On Err(e):  Consult troubleshooting matrix, fix, retry
```

### Playbook B: Stale payload on retry

```
Symptom:  resolve_round returns StaleOracleData even after refreshing
Diagnosis:
  - Check that your price-fetch timestamp is current (not cached).
  - Check ledger timestamp via get_active_round() and env.ledger().timestamp().
Fix:
  1. Re-fetch price and record fresh timestamp immediately before building payload.
  2. Ensure network latency doesn't push the total round-trip > 300 s.
  3. If unavoidable, reduce OracleStaleThreshold or batch submissions as
     early as possible after end_ledger.
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
