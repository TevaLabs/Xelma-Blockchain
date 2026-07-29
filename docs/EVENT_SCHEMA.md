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
| 3        | `protocol_fee_bps` | `Option<u32>` | Active protocol fee in basis points (if set)     |
| 4        | `precision_payout_policy` | `u32` | Payout distribution policy used for Precision round: `0` = Equal, `1` = StakeWeighted |

---

### `("payout", "outcome")`

Emitted once per participant during round resolution after that participant's settlement
outcome is known. Indexers can use these events to reconstruct the complete participant-level
result of a round without replaying contract storage reads.

| Position | Field          | Type      | Description                                               |
|----------|----------------|-----------|-----------------------------------------------------------|
| 0        | `round_id`     | `u64`     | Round that produced the participant outcome               |
| 1        | `mode`         | `u32`     | Round mode: `0` = UpDown, `1` = Precision                 |
| 2        | `user`         | `Address` | Participant address                                       |
| 3        | `gross_payout` | `i128`    | Amount credited to pending winnings, in stroops           |
| 4        | `outcome_type` | `u32`     | `0` = loss, `1` = win, `2` = refund                       |

`gross_payout` is `0` for losses. For refunds, it equals the participant's refunded stake.
For wins, it equals the full pending payout credited by the resolver, including returned
stake and any profit share.

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
(price-unchanged, one-sided pool, min-participants fallback, admin
cancellation, or Precision rounds where **nobody revealed** and all
commitments are refunded) — those cases use their respective refund
outcomes instead. When ≥1 Precision prediction is revealed, unrevealed
commitments forfeit to the pot and **do** emit this loss event
(`predicted_price = 0`).

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

### `("round", "summary")`

Emitted when a round is resolved, cancelled, or refunded. Contains compact settlement data.

| Position | Field               | Type   | Description                                                           |
|----------|---------------------|--------|-----------------------------------------------------------------------|
| 0        | `round_id`          | `u64`  | Round identifier                                                      |
| 1        | `mode`              | `u32`  | Round mode: `0` = UpDown, `1` = Precision                             |
| 2        | `price_start`       | `u128` | Opening price at round start (4 dec.)                                 |
| 3        | `price_final`       | `u128` | Settlement price (or `0` for administrative cancellation) (4 dec.)    |
| 4        | `participant_count` | `u32`  | Total unique user participants in the round                           |
| 5        | `total_pot`         | `i128` | Total accumulated round pot (in stroops)                              |
| 6        | `fee_amount`        | `i128` | Total protocol fees collected from the round pot (in stroops)         |
| 7        | `status`            | `u32`  | Round status: `0` = Resolved, `1` = Cancelled, `2` = FallbackRefund   |

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


### `("config", "updated")`

Emitted for every admin configuration mutation when a value is actually written, including immediate setters and timelocked changes when they are applied. This is the canonical audit event for reconstructing configuration history from events alone.

| Position | Field       | Type                  | Description                                      |
|----------|-------------|-----------------------|--------------------------------------------------|
| 0        | `kind`      | `ConfigChangeKind`    | Configuration key that changed.                  |
| 1        | `old_value` | `ConfigChangePayload` | Value observed immediately before the mutation.  |
| 2        | `new_value` | `ConfigChangePayload` | Value written by the mutation.                   |

Example payload for a windows update: `(Windows, Windows(6, 12), Windows(10, 20))`.

`ConfigChangeKind` values currently include `Windows`, `MaxStake`, `MaxUserRoundExposure`, `MaxPendingWinnings`, `OracleStaleThreshold`, `OracleMaxDeviationBps`, `ProtocolFeeBps`, `MinParticipants`, `MaxPrecisionParticipants`, `MintLimit`, and `ArchiveRetention`.

---

### `("windows", "updated")`

Emitted when the admin reconfigures the bet and run window lengths.

| Position | Field                | Type  | Description                                  |
|----------|----------------------|-------|----------------------------------------------|
| 0        | `bet_window_ledgers` | `u32` | New bet-acceptance window in ledger counts    |
| 1        | `run_window_ledgers` | `u32` | New round-duration window in ledger counts    |

---

#### `("action", "rejct")` — Diagnostic rejected-action event (Issue #196)

Emitted when a privileged action (admin or oracle) is rejected due to an
auth failure, paused contract, invalid state, or validation error. Enables
operators to diagnose failed privileged transactions from on-chain events
without relying on off-chain error logs.

**Privacy**: the payload contains only the `actor` Address, an `action`
Symbol, and a numeric `reason` code (a `ContractError` variant). No
personally identifiable information, financial amounts, or internal state
is exposed. Operators can match reason codes against the `ContractError`
enum variants in `contracts/src/errors.rs`.

| Position | Field    | Type      | Description                                                        |
|----------|----------|-----------|--------------------------------------------------------------------|
| 0        | `actor`  | `Address` | Address of the authenticated caller whose action was rejected       |
| 1        | `action` | `Symbol`  | Short name of the privileged action (e.g. `"create"`, `"resolve"`) |
| 2        | `reason` | `u32`     | Numeric error code matching a `ContractError` variant               |

**Example action symbols**: `"create"`, `"resolve"`, `"cancel"`, `"migrate"`,
`"withdraw"`, `"hbeat"`, `"arm_ovr"`, `"set_arch"`, `"sched"`,
`"cncl_cfg"`, `"min_par"`, `"max_prec"`, `"mint_lim"`.

**Reason codes** are the integer values of `ContractError` — see
`contracts/src/errors.rs`.

---

## `("oracle", "heartbeat")`

Emitted when the oracle records an on-chain liveness heartbeat.

| Position | Field       | Type  | Description                                                  |
|----------|-------------|-------|--------------------------------------------------------------|
| 0        | `timestamp` | `u64` | Unix epoch seconds when the heartbeat was recorded on-chain  |
| 1        | `status`    | `u32` | Oracle status: `0` = active, `1` = degraded, `2` = offline  |

---

## Oracle rotation events (two-step with mandatory delay)

Oracle rotation uses a two-step flow with a **mandatory 1-hour delay**
(`MIN_ROTATION_DELAY_SECONDS = 3_600`) between proposal and acceptance.
This prevents quiet takeovers — even with admin key compromise, operators
have a full hour to observe the proposal and react.

### `("oracle", "propose")`

Emitted when the admin proposes a new oracle address with an expiry window.

| Position | Field         | Type      | Description                                            |
|----------|---------------|-----------|--------------------------------------------------------|
| 0        | `new_oracle`  | `Address` | Proposed new oracle address                            |
| 1        | `expires_at`  | `u64`     | Unix timestamp when the proposal expires               |

### `("oracle", "accept")`

Emitted when a pending rotation proposal is successfully accepted (after the
mandatory delay has elapsed and before expiry).

| Position | Field             | Type      | Description                               |
|----------|-------------------|-----------|-------------------------------------------|
| 0        | `previous_oracle` | `Address` | Oracle address before the rotation         |
| 1        | `new_oracle`      | `Address` | New oracle address after the rotation      |

### `("oracle", "cancel")`

Emitted when the admin cancels a pending rotation proposal.

| Position | Field         | Type      | Description                                      |
|----------|---------------|-----------|--------------------------------------------------|
| 0        | `new_oracle`  | `Address` | The proposed oracle address that was cancelled    |

### `("oracle", "expired")`

Emitted when an expired proposal is cleaned up (auto-clean in
`get_oracle_rotation_proposal` or during `accept_oracle_rotation`).

| Position | Field         | Type  | Description                                      |
|----------|---------------|-------|--------------------------------------------------|
| 0        | `new_oracle`  | `Address` | The proposed oracle address that expired       |
| 1        | `proposed_at` | `u64` | Unix timestamp when the proposal was created      |
| 2        | `expires_at`  | `u64` | Unix timestamp when the proposal expired          |

### `("oracle", "early")`

Emitted when an attempt to accept a rotation proposal is rejected because the
mandatory delay (`MIN_ROTATION_DELAY_SECONDS`) has not yet elapsed.

| Position | Field           | Type  | Description                                          |
|----------|-----------------|-------|------------------------------------------------------|
| 0        | `new_oracle`    | `Address` | The proposed oracle address                       |
| 1        | `current_ts`    | `u64` | Current ledger timestamp at rejection time            |
| 2        | `earliest_accept` | `u64` | Timestamp at which acceptance will be allowed       |

---

### `("mode", "transition")`

Emitted when the contract's emergency runtime mode is changed by the admin.

| Position | Field      | Type  | Description                                                         |
|----------|------------|-------|---------------------------------------------------------------------|
| 0        | `old_mode` | `u32` | Previous runtime mode: `0` = Normal, `1` = ClaimsOnly, `2` = Paused |
| 1        | `new_mode` | `u32` | New runtime mode: `0` = Normal, `1` = ClaimsOnly, `2` = Paused      |

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

## `("template", "set")` — Round template stored

Emitted when the admin stores or overwrites the round-creation blueprint
used by `create_next_from_template`.

| Field         | Type          | Description                                    |
|---------------|---------------|-------------------------------------------------|
| `start_price` | `u128`        | Starting price the next round will use.         |
| `mode`        | `u32`         | Round mode: `0` = UpDown, `1` = Precision.       |

**Topics**: `("template", "set")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `set_round_template`.

---

## `("template", "cleared")` — Round template removed

Emitted when the admin clears the configured round template.

| Field     | Type  | Description                                  |
|-----------|-------|-----------------------------------------------|
| `ledger`  | `u32` | Ledger sequence number at which it was cleared.|

**Topics**: `("template", "cleared")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `clear_round_template`.

---

## `("template", "applied")` — Next round created from template

Emitted when `create_next_from_template` successfully creates a round from
the stored blueprint. Always accompanied by a `("round", "created")` event
from the underlying `create_round` call.

| Field         | Type   | Description                                   |
|---------------|--------|------------------------------------------------|
| `round_id`    | `u64`  | Id of the newly created round.                  |
| `start_price` | `u128` | Starting price used (from the template).        |
| `mode`        | `u32`  | Round mode used: `0` = UpDown, `1` = Precision. |

**Topics**: `("template", "applied")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `create_next_from_template`.

---

## `("season", "reset")` — Leaderboard season archived and advanced

Emitted when the admin resets the active leaderboard season. The ending
season's bounded rankings are frozen into a permanent `SeasonArchive` before
the season id advances; per-user season stats (`SeasonUserStats`) for the
ended season are never deleted and remain independently queryable.

| Field               | Type  | Description                                          |
|---------------------|-------|-------------------------------------------------------|
| `season_id`         | `u32` | Id of the season that was just archived.               |
| `new_season_id`     | `u32` | Id of the newly-active season (`season_id + 1`).        |
| `ended_at_ledger`    | `u32` | Ledger sequence number at which the season ended.       |
| `participant_count` | `u32` | Distinct addresses present in the archived rankings.    |

**Topics**: `("season", "reset")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `reset_leaderboard_season`.

---

## `("oracle", "hblocked")` — Heartbeat health gate blocked settlement

Emitted when `resolve_round` is blocked by the heartbeat health gate in strict mode
(Issue #264). The round remains active; the oracle or admin must address the heartbeat
state before retrying.

| Position | Field       | Type   | Description                                    |
|----------|-------------|--------|------------------------------------------------|
| 0        | `round_id`  | `u64`  | Round id that was blocked from settlement       |

**Topics**: `("oracle", "hblocked")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `resolve_round` (heartbeat health gate, strict mode).

---

## `("oracle", "hoverride")` — Heartbeat health gate override consumed

Emitted when the admin-armed heartbeat health override is consumed during
`resolve_round`, allowing settlement to proceed past the gate (Issue #264).

| Position | Field       | Type   | Description                                    |
|----------|-------------|--------|------------------------------------------------|
| 0        | `round_id`  | `u64`  | Round id for which the override was consumed    |

**Topics**: `("oracle", "hoverride")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `resolve_round` (heartbeat health gate override path).

---

---

## Field units quick reference

| Concept        | Unit       | Scale factor | Example                              |
|----------------|------------|--------------|--------------------------------------|
| Token amount   | stroops    | × 10 000 000 | 1 vXLM = `10_000_000`               |
| XLM price      | 4 dec.     | × 10 000     | 0.2297 XLM = `2297`                 |
| Duration       | ledgers    | ~5 s/ledger  | 12 ledgers ≈ 60 seconds             |
| Timestamp      | Unix epoch | seconds      | `1_700_000_000` = 2023-11-14 ~22:13 |
