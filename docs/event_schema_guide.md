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

### 7. Round Resolved
Emitted when a round is resolved with the final price from the oracle.
* **Topics:** `("round", "resolved")`
* **Payload:** `(round_id: u64, final_price: u128, mode: u32)`
  * `mode`: `0` for `UpDown`, `1` for `Precision`

### 8. Round Cancelled
Emitted when an active round is cancelled by the admin.
* **Topics:** `("round", "cancelled")`
* **Payload:** `(round_id: u64, reason: u32, pool_up: i128, pool_down: i128)`

### 9. Winnings Claimed
Emitted when a user claims their accumulated pending winnings.
* **Topics:** `("claim", "winnings")`
* **Payload:** `(user: Address, amount: i128)`

### 10. Initial Mint
Emitted when a new user claims their one-time initial allocation.
* **Topics:** `("mint", "initial")`
* **Payload:** `(user: Address, amount: i128)`
