# Xelma Contract — Canonical Event Schema

This document is the authoritative reference for all events emitted by the Xelma smart contract.
Indexers, explorers, and client libraries must use these definitions to avoid incompatible
field assumptions.

---

## Versioning strategy

Events are versioned by **schema version tag** in the documentation rather than on-chain.
Breaking field changes will increment the schema version and are announced in `MIGRATION.md`.
Additive changes (new optional events) do not increment the version.

Current schema version: **v1**

---

## Encoding conventions

| Field type   | On-chain encoding                                      |
|--------------|--------------------------------------------------------|
| `u32`        | Raw unsigned 32-bit integer                            |
| `u64`        | Raw unsigned 64-bit integer                            |
| `u128`       | Raw unsigned 128-bit integer                           |
| `i128`       | Raw signed 128-bit integer                             |
| `Address`    | Soroban `Address` (account or contract)                |
| `bool`       | Soroban boolean                                        |

**Amount units**: all token amounts are in stroops (1 vXLM = 10 000 000 stroops, 7 decimal places).

**Price units**: prices are scaled to **4 decimal places** (e.g., 0.2297 XLM → `2297`).

**Ledger sequence**: `u32` counter that increments with each Stellar ledger (~5 s/ledger).

**Timestamp**: Unix epoch seconds (`u64`), sourced from oracle payload or `env.ledger().timestamp()`.

---

## Topic encoding

Each event carries exactly **two topics**, both `Symbol` values.  
They form the canonical `(namespace, action)` pair used for filtering.

---

## Events

### `("round", "created")`

Emitted when a new prediction round is opened.

| Position | Field            | Type   | Description                                                     |
|----------|------------------|--------|-----------------------------------------------------------------|
| 0        | `round_id`       | `u64`  | Monotonically increasing round identifier                       |
| 1        | `start_price`    | `u128` | Starting XLM price (4 decimal places)                          |
| 2        | `start_ledger`   | `u32`  | Ledger sequence number when the round was created               |
| 3        | `bet_end_ledger` | `u32`  | Last ledger at which new bets are accepted                      |
| 4        | `end_ledger`     | `u32`  | Ledger at or after which the round can be resolved              |
| 5        | `mode`           | `u32`  | Round mode: `0` = UpDown, `1` = Precision                      |

---

### `("bet", "placed")`

Emitted when a user places an Up/Down bet.

| Position | Field      | Type      | Description                                      |
|----------|------------|-----------|--------------------------------------------------|
| 0        | `user`     | `Address` | User who placed the bet                          |
| 1        | `round_id` | `u64`     | Round the bet belongs to                         |
| 2        | `amount`   | `i128`    | Bet amount in stroops                            |
| 3        | `side`     | `u32`     | Prediction side: `0` = Up, `1` = Down            |

---

### `("predict", "price")`

Emitted when a user submits a Precision mode price prediction.

| Position | Field             | Type      | Description                                          |
|----------|-------------------|-----------|------------------------------------------------------|
| 0        | `user`            | `Address` | User who submitted the prediction                    |
| 1        | `round_id`        | `u64`     | Round the prediction belongs to                      |
| 2        | `predicted_price` | `u128`    | Predicted price (4 decimal places)                   |
| 3        | `amount`          | `i128`    | Bet amount in stroops                                |

---

### `("round", "resolved")`

Emitted when a round is settled competitively by the oracle.

| Position | Field         | Type   | Description                                      |
|----------|---------------|--------|--------------------------------------------------|
| 0        | `round_id`    | `u64`  | Round that was resolved                          |
| 1        | `final_price` | `u128` | Closing price reported by the oracle (4 dec.)    |
| 2        | `mode`        | `u32`  | Round mode: `0` = UpDown, `1` = Precision        |

---

### `("outcome", "loss")`

*Additive change added by Issue #168 — schema version stays at **v1**
(additive events do not trigger a version bump per the versioning policy
at the top of this file).*

Emitted per losing participant whenever a round settles competitively
(Issue #168).  Complements the implicit "winner" signal from pending-winnings
accumulation and the explicit `("round", "fallback")` refund event so that
analytics, user notifications, and indexers can detect losses without
inferring them from the absence of payout events.

The payload shape is unified across both modes; the `mode` field selects which
metadata field is meaningful:

- **UpDown mode (`mode = 0`):** `side` is the user's losing direction
  (`0` = Up, `1` = Down). `predicted_price` is fixed at `0`.
- **Precision mode (`mode = 1`):** `predicted_price` is the user's guess in the
  4-decimal price scale. `side` is fixed at `0`. Participants who only
  committed (and did not reveal) carry `predicted_price = 0` because the
  guess is unknowable on-chain until reveal.

Emitted for every participant who placed a bet/prediction and was on the
losing side of a competitive settlement. **Not** emitted for refund paths
(price-unchanged, one-sided pool, min-participants fallback, or admin
cancellation) — those cases use their respective refund events instead.

| Position | Field            | Type      | Description                                                              |
|----------|------------------|-----------|--------------------------------------------------------------------------|
| 0        | `user`           | `Address` | Address of the losing participant                                        |
| 1        | `round_id`       | `u64`     | Round the loss occurred in                                               |
| 2        | `mode`           | `u32`     | Round mode: `0` = UpDown, `1` = Precision                                |
| 3        | `amount`         | `i128`    | Stake amount the user committed (in stroops); the amount they lose       |
| 4        | `side`           | `u32`     | UpDown losing side (`0` = Up, `1` = Down). `0` for Precision mode        |
| 5        | `predicted_price`| `u128`    | Precision guess (4 decimal places). `0` for UpDown mode or unrevealed   |

---

### `("round", "cancelled")`

Emitted when an admin explicitly cancels an active round. All stakes are refunded.

| Position | Field       | Type   | Description                                             |
|----------|-------------|--------|---------------------------------------------------------|
| 0        | `round_id`  | `u64`  | Round that was cancelled                                |
| 1        | `reason`    | `u32`  | Admin-supplied reason code (application-defined)        |
| 2        | `pool_up`   | `i128` | Total Up-side pool at cancellation time (in stroops)    |
| 3        | `pool_down` | `i128` | Total Down-side pool at cancellation time (in stroops)  |

---

### `("round", "fallback")`

Emitted when a round ends below the configured minimum-participants threshold.
All stakes are refunded; no competitive settlement occurs.

| Position | Field               | Type  | Description                                         |
|----------|---------------------|-------|-----------------------------------------------------|
| 0        | `round_id`          | `u64` | Round that triggered the fallback                   |
| 1        | `participant_count` | `u32` | Actual number of participants at resolution time    |
| 2        | `min_required`      | `u32` | Configured minimum that was not met                 |

---

### `("claim", "winnings")`

Emitted when a user successfully claims pending winnings.

| Position | Field    | Type      | Description                             |
|----------|-----------|-----------|-----------------------------------------|
| 0        | `user`   | `Address` | User who claimed                        |
| 1        | `amount` | `i128`    | Amount credited to balance (in stroops) |

---

### `("mint", "initial")`

Emitted when a new user mints their one-time initial vXLM allocation.

| Position | Field    | Type      | Description                             |
|----------|-----------|-----------|-----------------------------------------|
| 0        | `user`   | `Address` | User who received the allocation        |
| 1        | `amount` | `i128`    | Minted amount (always 10 000 000 000 stroops = 1 000 vXLM) |

---

### `("windows", "updated")`

Emitted when the admin reconfigures the bet and run window lengths.

| Position | Field                | Type  | Description                                  |
|----------|----------------------|-------|----------------------------------------------|
| 0        | `bet_window_ledgers` | `u32` | New bet-acceptance window in ledger counts    |
| 1        | `run_window_ledgers` | `u32` | New round-duration window in ledger counts    |

---

### `("oracle", "heartbeat")`

Emitted when the oracle records an on-chain liveness heartbeat.

| Position | Field       | Type  | Description                                                  |
|----------|-------------|-------|--------------------------------------------------------------|
| 0        | `timestamp` | `u64` | Unix epoch seconds when the heartbeat was recorded on-chain  |
| 1        | `status`    | `u32` | Oracle status: `0` = active, `1` = degraded, `2` = offline  |

---

## Example decode mappings

### JavaScript / TypeScript example: filter for losses

```typescript
import { xdr, scValToNative } from "@stellar/stellar-sdk";

function decodeOutcomeLoss(contractEvent: xdr.DiagnosticEvent) {
  const topics = contractEvent.event().body().v0().topics();
  const ns = scValToNative(topics[0]);
  const action = scValToNative(topics[1]);
  if (ns !== "outcome" || action !== "loss") return null;
  const data = scValToNative(contractEvent.event().body().v0().data());
  // [user, round_id, mode, amount, side, predicted_price]
  return { type: "loss", ...data };
}
```

---

### JavaScript / TypeScript (Stellar SDK)

```typescript
import { xdr, scValToNative } from "@stellar/stellar-sdk";

function decodeEvent(contractEvent: xdr.DiagnosticEvent) {
  const topics = contractEvent.event().body().v0().topics();
  const ns = scValToNative(topics[0]) as string;    // e.g. "round"
  const action = scValToNative(topics[1]) as string; // e.g. "created"
  const data = scValToNative(contractEvent.event().body().v0().data());
  return { ns, action, data };
}
```

### Rust (soroban-sdk test utilities)

```rust
use soroban_sdk::{symbol_short, testutils::{Events, TryIntoVal}, Env};

let events = env.events().all();
let resolved = events.iter().find(|(_, topics, _)| {
    topics.get(0).and_then(|t| t.try_into_val(&env).ok()) == Some(symbol_short!("round"))
        && topics.get(1).and_then(|t| t.try_into_val(&env).ok()) == Some(symbol_short!("resolved"))
});
```

---

## `("protocol", "fee_collected")` — Competitive-settlement fee accrual

Emitted by every competitive-settlement path (UpDown indexed/legacy, Precision
indexed/legacy) when the protocol fee is enabled (Issue #162). NOT emitted on
refund / cancel / fallback paths — those return users' full stake and the
treasury stays flat.

| Field         | Type      | Description                                                |
|---------------|-----------|------------------------------------------------------------|
| `round_id`    | `u64`     | The id of the settled round.                                |
| `fee_amount`  | `i128`    | Stroops routed to the on-chain treasury this round.         |
| `treasury_balance` | `i128` | Cumulative treasury balance AFTER this round's credit.     |
| `bps_active`  | `u32`     | The fee's bps that produced `fee_amount` (echoes storage).  |

**Topics**: `("protocol", "fee_collected")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `_record_winnings_indexed`, `_record_winnings_legacy`,
`_resolve_precision_mode`, `_resolve_precision_legacy`.

The conservation invariant
`Σ payout_i + fee_amount == total_pot` holds for every emission. In the
UpDown pathological case `fee > losing_pool` (very thin losing-side
liquidity near the bps cap) the spillover is deducted from `winning_pool`
so the invariant still holds and winners receive only their residual
principal — documented inline in `_apply_protocol_fee_updown`.

---

## `("protocol", "fee_bps_set")` — Timelocked fee schedule applied

Emitted exactly once when a previously-scheduled `ProtocolFeeBps` change
passes its `activation_ledger` and is written to storage (Issue #162).

| Field   | Type          | Description                                              |
|---------|---------------|----------------------------------------------------------|
| `bps`   | `Option<u32>` | New fee (None = fee disabled; Some(bps) = active).       |

**Topics**: `("protocol", "fee_bps_set")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `_apply_config_payload` arm for `ConfigChangeKind::ProtocolFeeBps`.

---

## `("protocol", "fee_withdrawn")` — Treasury drain to recipient

Emitted when the admin drains accumulated fees to an on-chain recipient
(Issue #162). Recipient receives the credited amount through the existing
`PendingWinnings` → `claim_winnings` flow used by competitive payouts and
refunds, so no new authorization surface is added.

| Field           | Type     | Description                                              |
|-----------------|----------|----------------------------------------------------------|
| `recipient`     | `Address`| The credited account.                                    |
| `amount`        | `i128`   | Stroops transferred out of the treasury this call.       |
| `new_treasury`  | `i128`   | Treasury balance after withdrawal.                       |

**Topics**: `("protocol", "fee_withdrawn")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `withdraw_protocol_fee`.

---

---

## Field units quick reference

| Concept        | Unit       | Scale factor | Example                              |
|----------------|------------|--------------|--------------------------------------|
| Token amount   | stroops    | × 10 000 000 | 1 vXLM = `10_000_000`               |
| XLM price      | 4 dec.     | × 10 000     | 0.2297 XLM = `2297`                 |
| Duration       | ledgers    | ~5 s/ledger  | 12 ledgers ≈ 60 seconds             |
| Timestamp      | Unix epoch | seconds      | `1_700_000_000` = 2023-11-14 ~22:13 |
