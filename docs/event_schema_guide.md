# Event Schema Guide

This guide documents the event system for the XLM Price Prediction Market. Indexers and off-chain services should subscribe to these events to track the active state, user bets, outcomes, and emergency operations.

---

## Event Schema Definitions

All events are published under the contract address and contain structured topics and a data payload.

### 1. Round Created
Emitted when a new prediction round is created by the admin.
* **Topics:** `("round", "created")` (represented as short symbols)
* **Payload:** `(round_id: u64, start_price: u128, start_ledger: u32, bet_end_ledger: u32, end_ledger: u32, mode: u32)`
  * `mode`: `0` for `UpDown`, `1` for `Precision`

### 2. Windows Updated
Emitted when the betting or running windows are modified by the admin.
* **Topics:** `("windows", "updated")`
* **Payload:** `(bet_window_ledgers: u32, run_window_ledgers: u32)`

### 3. Bet Placed
Emitted when a user places a directional bet in `UpDown` mode.
* **Topics:** `("bet", "placed")`
* **Payload:** `(user: Address, round_id: u64, amount: i128, side: u32)`
  * `side`: `0` for `Up`, `1` for `Down`

### 4. Prediction Committed
Emitted when a user commits a hashed prediction in `Precision` mode.
* **Topics:** `("commit", "predict")`
* **Payload:** `(user: Address, round_id: u64, hash: BytesN<32>, amount: i128)`

### 5. Prediction Revealed
Emitted when a user successfully reveals their prediction in `Precision` mode.
* **Topics:** `("reveal", "predict")`
* **Payload:** `(user: Address, round_id: u64, predicted_price: u128, amount: i128)`

### 6. Legacy Prediction Placed
Emitted when a user places a prediction directly (without commit-reveal) in `Precision` mode.
* **Topics:** `("predict", "price")`
* **Payload:** `(user: Address, round_id: u64, predicted_price: u128, amount: i128)`

### 7. Round Summary (Canonical Terminal Event)
Emitted exactly once per terminal round transition — competitive resolution,
admin cancellation, or min-participants fallback. Replaces the previously
separate `("round", "resolved")`, `("round", "cancelled")`, and
`("round", "fallback")` events.
* **Topics:** `("round", "summary")`
* **Payload:** `(version: u32, round_id: u64, status: u32, mode: u32, price_start: u128, price_final: u128, pool_up: i128, pool_down: i128, participant_count: u32, total_pot: i128, fee_amount: i128, settled_at_ledger: u32, confidence: Option<u32>)`
  * `version`: Schema version (`0` for this layout)
  * `status`: `0` for `Resolved`, `1` for `Cancelled`, `2` for `FallbackRefund`
  * `mode`: `0` for `UpDown`, `1` for `Precision`
  * `confidence`: Oracle confidence in basis points (`None` for cancel/fallback)

### 8. Outcome Loss
Emitted per losing participant when a round settles competitively
(Issue #168). Complements the existing payout/refund events so that
analytics, user notifications, and indexers no longer need to infer losses
from the absence of payout events.
* **Topics:** `("outcome", "loss")`
* **Payload:** `(user: Address, round_id: u64, mode: u32, amount: i128, side: u32, predicted_price: u128)`
  * `mode`: `0` for `UpDown`, `1` for `Precision`
  * UpDown losers: `side` is `0` for Up or `1` for Down; `predicted_price` is `0`
  * Precision losers: `side` is `0`; `predicted_price` is the user's guess (`0` if only committed and unrevealed)

Not emitted on refund paths (price-unchanged refund, one-sided pool
refund, min-participants fallback, admin cancellation) — those paths use
their respective refund-prone events instead.

### 8. Winnings Claimed
Emitted when a user claims their accumulated pending winnings.
* **Topics:** `("claim", "winnings")`
* **Payload:** `(user: Address, amount: i128)`

### 10. Initial Mint
Emitted when a new user claims their one-time initial allocation.
* **Topics:** `("mint", "initial")`
* **Payload:** `(user: Address, amount: i128)`

### 11. Runtime Mode Transition
Emitted when the contract's emergency runtime mode is changed by the admin.
* **Topics:** `("mode", "transition")`
* **Payload:** `(old_mode: u32, new_mode: u32)`
  * `mode`: `0` for `Normal`, `1` for `ClaimsOnly`, `2` for `FullyPaused`

## Section: Protocol fee events (Issue #162)

The optional protocol fee introduces a new top-level event namespace
`("protocol", ...)` for treasury-related observability. Three event types
emitted, all gated on admin-controlled timelock activation:

### `("protocol", "fee_coll")` — competitive settlement fee accrued

> The second topic is the short symbol `fee_coll`, not `fee_collected`:
> Soroban `Symbol`s are limited to 9 characters. Scan for `fee_coll`.

Emitted exactly once per competitive settlement (UpDown indexed/legacy,
Precision indexed/legacy) when `get_protocol_fee_bps` returns
`Some(active_bps)`. Payload `(round_id: u64, fee_amount: i128,
treasury_balance: i128, bps_active: u32, fee_model: u32)`.

Conservation `Σ payouts + fee_amount == total_pot` is enforced in
`_apply_protocol_fee_*`. UpDown conservatively deducts from the losing
pool first, then spills over into the winning pool — so winners
receive their remaining principal when the fee exceeds losing liquidity.
Refund paths (UpDown unchanged-price, one-sided pool, min-participants
fallback, admin cancellation, dispute-window void, Precision zero-reveal)
do NOT emit this event and MUST NOT move the treasury; every participant
receives their exact stake back. `resolution::fees` asserts this with a
full event-stream scan.

### `("protocol", "fee_bps_set")` — timelock applied

Emitted exactly once when a previously-scheduled `ProtocolFeeBps` change
is written to storage at its `activation_ledger`. Payload is
`(Option<u32>,)` — `None` means "fee disabled again", `Some(bps)` carries
the new active bps.

### `("protocol", "fee_withdrawn")` — treasury drained to recipient

Admin-only. Payload `(recipient: Address, amount: i128,
new_treasury: i128)`. Recipient is credited via the existing
`PendingWinnings` ledger — so claim semantics are identical to
competitive winnings, and no additional surface is needed for users
to spend the credited amount.

### Indexer guidance

A fee-aware indexer can rely on `fee_coll` events as the canonical
record of fee accrual. Treasury balance computations should:

1. Subscribe to `("protocol", fee_coll)` for per-round accruals.
2. Subscribe to `("protocol", fee_withdrawn)` for treasury drains.
3. Optionally cross-reference `("config", applied)` events associated
   with `("protocol", fee_bps_set)` for rate changes.

Conservations across event streams:

* For each `fee_coll` event: Σ of `("claim","winnings")` for the
  same round's winners + `fee_amount` == `round.pool_up + round.pool_down`
  (UpDown) or `Σ prediction.amount` (Precision mode, including
  unrevealed-commitment stakes that forfeited to the pot).
* When a Round resolves on a refund path (including an UpDown round whose
  price is unchanged and a Precision round with **zero reveals**), stakes are
  refunded instead of fee-split: `Σ refund amounts == pot` and no `fee_coll`
  event is emitted for that path.
* Treasury balance monotonically increases across `fee_coll`
  events and monotonically decreases across `fee_withdrawn` events.

