# Xelma - Decentralized XLM Price Prediction Market

[![CI](https://github.com/TevaLabs/Xelma-Blockchain/actions/workflows/ci.yml/badge.svg)](https://github.com/TevaLabs/Xelma-Blockchain/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.94.0-orange.svg)](https://www.rust-lang.org/)
[![Soroban](https://img.shields.io/badge/Soroban-23.5.3-blue.svg)](https://soroban.stellar.org/)
[![Tests](https://img.shields.io/badge/tests-passing-brightgreen.svg)]()
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> A trustless, transparent, and decentralized prediction market for XLM price movements built on Stellar blockchain using Soroban smart contracts.

---

## 🎯 What is Xelma?

**Xelma** is a blockchain-based prediction market with dual prediction modes:
- **Up/Down Mode**: Bet on whether XLM price will go UP or DOWN
- **Precision Mode (Legends)**: Predict the exact price - closest guess wins!

Unlike traditional prediction markets, Xelma is:

- 🔓 **Permissionless** - Anyone with a Stellar wallet can participate
- 🔍 **Transparent** - All bets, rounds, and payouts are verifiable on-chain
- ⚡ **Instant** - Claim your winnings immediately after round resolution
- 🛡️ **Secure** - Smart contract logic ensures fair, automated payouts
- 💰 **Low-cost** - Stellar's minimal transaction fees (~0.00001 XLM)

---

## 🔥 The Problem We're Solving

### Traditional Prediction Markets Fail Users:

| Issue | Traditional Markets | Xelma Solution |
|-------|-------------------|----------------|
| **Centralization** | Single point of failure, can be shut down | Runs on blockchain, unstoppable |
| **Transparency** | Opaque calculations, potential manipulation | All logic on-chain, fully auditable |
| **Access** | Requires KYC, bank accounts, specific locations | Just need a Stellar wallet |
| **Payout Speed** | Days or weeks to withdraw | Instant claims after resolution |
| **Trust** | Must trust the operator won't steal funds | Trustless smart contract execution |
| **Fees** | High fees (5-10%+) | Minimal blockchain fees (~0.00001 XLM) |

### Why This Matters:

1. **Financial Inclusion**: No KYC barriers, anyone globally can participate
2. **Verifiable Fairness**: Every bet, pool, and payout is transparent
3. **No Counterparty Risk**: Smart contract holds funds, not a company
4. **Educational**: Learn prediction markets without real money risk (virtual tokens)
5. **Building Block**: Foundation for decentralized derivatives/prediction markets

---

## 🏗️ How It Works

### System Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Users (Bettors)                       │
│  - Mint virtual tokens (1000 vXLM)                      │
│  - Place bets (UP/DOWN on XLM price)                    │
│  - Track stats (wins, losses, streaks)                  │
│  - Claim winnings                                        │
└──────────────────────┬──────────────────────────────────┘
                       │ Interacts via Frontend/SDK
                       ↓
┌─────────────────────────────────────────────────────────┐
│              TypeScript Bindings (SDK)                   │
│  - Type-safe contract calls                             │
│  - All contract functions exposed                        │
│  - Error handling with custom types                     │
└──────────────────────┬──────────────────────────────────┘
                       │ Calls via Stellar SDK
                       ↓
┌─────────────────────────────────────────────────────────┐
│           Soroban Smart Contract (Rust)                  │
│                                                          │
│  [Virtual Token System]                                  │
│  • Mint 1000 vXLM per new user                          │
│  • Track balances on-chain                              │
│                                                          │
│  [Round Management]                                      │
│  • Admin creates rounds (start price + mode + duration) │
│  • Mode 0 (Up/Down): Bet UP or DOWN                     │
│  • Mode 1 (Precision): Predict exact price              │
│  • Oracle resolves rounds (final price)                 │
│                                                          │
│  [Payout Logic - Dual Mode]                             │
│  • Up/Down: Winners split losers' pool proportionally   │
│  • Precision: Closest guess wins full pot (ties split)  │
│  • Unchanged price → everyone gets refund               │
│  • Claim-based withdrawal (user controlled)             │
│                                                          │
│  [User Statistics]                                       │
│  • Track wins, losses, streaks                          │
│  • On-chain leaderboard data                            │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ↓
┌─────────────────────────────────────────────────────────┐
│            Stellar Blockchain (Storage)                  │
│  - Persistent storage for all contract data             │
│  - Ledger: ~5 seconds per block                         │
│  - Network: Testnet (development) / Mainnet (future)    │
└─────────────────────────────────────────────────────────┘
```

### User Flow Example:

1. **Alice connects wallet** → Receives 1000 vXLM automatically
2. **Admin creates round** → Start price: 1.0 XLM, Duration: 60 ledgers (~5 min)
3. **Alice bets 100 vXLM UP**, **Bob bets 200 vXLM UP**, **Charlie bets 150 vXLM DOWN**
4. **Oracle resolves** → Final price: 1.5 XLM (price went UP!)
5. **Payouts calculated**:
   - Winning pool (UP): 300 vXLM
   - Losing pool (DOWN): 150 vXLM
   - Alice gets: 100 + (100/300) × 150 = **150 vXLM** (50% profit!)
   - Bob gets: 200 + (200/300) × 150 = **300 vXLM** (50% profit!)
   - Charlie loses his 150 vXLM
6. **Alice & Bob claim winnings** → Instant balance update

---

## 🛠️ Technical Stack

### Smart Contract (Rust + Soroban)
- **Language**: Rust 1.94.0
- **Framework**: Soroban SDK 23.5.3
- **Blockchain**: Stellar (Testnet)
- **Testing**: Full suite passing (run `cargo test --workspace` to verify)
- **Canonical crate**: `xelma-contract` (used by CI, build, and artifact paths)

### Key Features:
- ✅ Custom error handling (50 error types)
- ✅ Emergency pause/recovery controls for incident response
- ✅ Overflow protection (checked arithmetic)
- ✅ Role-based access control (Admin, Oracle, User)
- ✅ Input validation on all functions
- ✅ Claim-based withdrawal pattern
- ✅ Proportional payout algorithm
- ✅ User statistics tracking
- ✅ Precision remainder policy (first-winner receives dust)

### Precision Mode Payout Policy

When multiple users tie in **Precision Mode**, the total pot is split evenly using integer division. Any remainder (dust) from the division is awarded to the **first winner** (by prediction order).

**Example:**
- Total pot: 100 vXLM
- 3-way tie (Alice, Bob, Charlie)
- Division: 100 ÷ 3 = 33.33...
- Payouts:
  - Alice: 33 + 1 (remainder) = **34 vXLM**
  - Bob: **33 vXLM**
  - Charlie: **33 vXLM**
- **Total distributed: 100 vXLM** ✅ (no dust lost)

This ensures:
- ✅ **Zero dust loss** - Every stroops is accounted for
- ✅ **Simple & predictable** - First predictor gets the remainder
- ✅ **Fair distribution** - Close to equal split, minimal advantage

### Precision Commit-Reveal Flow

To prevent front-running and copy-trading, Precision rounds support a two-step commit-reveal flow:
1. **Commit**: Users submit a SHA-256 hash of their `predicted_price` and a `salt`. This locks their stake without revealing the guess. The commitment hash must be valid (not all-zeros).
2. **Reveal**: During the reveal window, users submit the plaintext `predicted_price` and `salt`. The contract verifies the hash and enforces minimum salt entropy (to prevent trivial grinding).

**Unrevealed Policy**:
- **Anti-Griefing**: If at least one prediction is revealed (or placed directly), any unrevealed commitments are forfeited to the pot and count as losers.
- **Conservation**: If *nobody* reveals in the round, all committed stakes are fully refunded to the users.

### Oracle Operator Runbook

Oracle mistakes are a top incident source. See
[docs/ORACLE_OPERATOR_RUNBOOK.md](./docs/ORACLE_OPERATOR_RUNBOOK.md) for:
- Payload field-by-field requirements and copy-paste templates.
- Troubleshooting matrix for stale, future, deviation, and nonce errors.
- Escalation steps for pause, cancel, and deviation override.
- Operational playbooks covering both Up/Down and Precision round resolution.

### Emergency Pause and Recovery

The contract includes an admin-controlled emergency pause for incidents such as oracle outages or critical bugs.

When paused:
- Mutating operations are rejected, including round creation, betting, resolution, claims, and token minting.
- Read-only queries remain available so operators and users can inspect state.

Recovery workflow:
1. Admin calls `pause_contract()` to freeze high-risk operations.
2. Investigate and fix the incident off-chain or in a patched deployment.
3. Admin calls `unpause_contract()` once the system is safe again.
4. Resume normal round creation and user interaction.

Use `is_paused()` to verify the current contract state before attempting recovery actions.

### Precision Participant Cap

Precision rounds enforce a configurable participant cap to keep storage growth and resolution cost predictable. The default cap is **1,000 participants** per Precision round. Admins can tune it with `set_max_precision_participants(max)` within the supported range of `1` to `10,000`; use `get_max_precision_participants()` to confirm the active value.

Operator guidance:
- Lower the cap for short rounds, high-volatility windows, or constrained infrastructure.
- Raise the cap only after benchmark evidence shows resolution remains within Soroban resource limits.
- Treat cap changes as operational risk controls; announce material changes to indexers and frontends before opening new rounds.

### TypeScript Bindings
- **Language**: TypeScript 5.6.2
- **SDK**: Stellar SDK 14.1.1
- **Package**: `@tevalabs/xelma-bindings`
- **Types**: Fully typed contract interface

---

## 📦 Repository Structure

```
Xelma-Blockchain/
├── contracts/                 # Main prediction market contract
│   ├── src/
│   │   ├── lib.rs            # Crate root and module declarations
│   │   ├── contract.rs       # Core contract implementation (~820 lines)
│   │   ├── errors.rs         # Custom error types (20 variants)
│   │   ├── types.rs          # Contract types and storage keys
│   │   └── tests/            # Test suite
│   │       ├── mod.rs
│   │       ├── betting.rs
│   │       ├── edge_cases.rs
│   │       ├── initialization.rs
│   │       ├── lifecycle.rs
│   │       ├── mode_tests.rs
│   │       ├── property_invariants.rs
│   │       ├── resolution.rs
│   │       ├── security.rs
│   │       ├── storage_benchmarks.rs
│   │       ├── ttl_tests.rs
│   │       └── windows.rs
│   │       └── ... (20+ test files total — see docs/CONTRIBUTOR_MAP.md)
│   ├── Cargo.toml            # Rust dependencies
│   └── test_snapshots/       # Test execution records
│
├── bindings/                  # TypeScript bindings (auto-generated)
│   ├── src/
│   │   └── index.ts          # Contract types & client (~640 lines)
│   ├── dist/                  # Compiled JavaScript
│   ├── package.json           # NPM package config
│   └── README.md              # Bindings usage guide
│
├── target/                    # Build artifacts
│   └── wasm32v1-none/
│       └── release/
│           └── xelma_contract.wasm  # Compiled contract
│
├── docs/
│   ├── CONTRIBUTOR_MAP.md     # Module → test → task map for contributors
│   ├── CONTRIBUTOR_TASK_MATRIX.md # PR evidence requirements by task type
│   ├── EVENT_SCHEMA.md        # Canonical on-chain event schema for indexers
│   └── storage_lifecycle.md   # TTL/rent policy reference
├── SECURITY_REVIEW.md         # Comprehensive security audit
├── Cargo.toml                 # Workspace configuration
└── README.md                  # This file
```

---

## 🚀 Quick Start

### Prerequisites

- Rust 1.94.0+
- Stellar CLI (soroban-cli)
- Node.js 20+ (for bindings)

### 1. Clone Repository

```bash
git clone https://github.com/TevaLabs/Xelma-Blockchain.git
cd Xelma-Blockchain
```

### 2. Build Smart Contract

```bash
cd contracts
stellar contract build
```

### 3. Run Tests

```bash
cargo test --workspace --locked
# All tests should pass
```

### 4. Generate & Build Bindings

```bash
cd ../../
stellar contract bindings typescript \
  --wasm target/wasm32v1-none/release/xelma_contract.wasm \
  --output-dir ./bindings \
  --overwrite

cd bindings
npm install
npm run build
```

### 5. Use in Your Project

```typescript
import { Client, BetSide } from '@tevalabs/xelma-bindings';

const client = new Client({
  contractId: 'YOUR_CONTRACT_ID',
  networkPassphrase: Networks.TESTNET,
  rpcUrl: 'https://soroban-testnet.stellar.org'
});

// Mint initial tokens
await client.mint_initial({ user: userAddress });

// Place a bet
await client.place_bet({
  user: userAddress,
  amount: 100_0000000n, // 100 vXLM (in stroops)
  side: BetSide.Up
});

// Check stats
const stats = await client.get_user_stats({ user: userAddress });
console.log(`Wins: ${stats.total_wins}, Streak: ${stats.current_streak}`);
```

---

## 🔐 Security

We take security seriously. The contract has undergone comprehensive hardening:

### Security Features:
- ✅ **50 Custom Error Types** - Clear, debuggable error codes
- ✅ **Checked Arithmetic** - All math operations use `checked_*` to prevent overflow
- ✅ **Role-Based Access** - Admin creates rounds, Oracle resolves, Users bet
- ✅ **Input Validation** - All parameters validated (amount > 0, round active, etc.)
- ✅ **No Reentrancy Risk** - CEI pattern (Checks-Effects-Interactions)
- ✅ **State Consistency** - Prevents double betting, validates round lifecycle
- ✅ **Full Test Suite Passing** - Comprehensive coverage of edge cases and attack vectors

### Audited:
- [SECURITY_REVIEW.md](./SECURITY_REVIEW.md) - Complete security analysis
- [PROTOCOL_SPEC.md](./PROTOCOL_SPEC.md) - Formal invariants, threat model, and trust boundaries

**Status**: ✅ Production-ready for testnet  
**Recommendation**: External audit recommended before mainnet deployment

---

## 📡 Event Schema

All major state transitions emit standardized events for indexers and frontend consumers. Events follow a consistent format: `(topic_1, topic_2)` → `(payload...)`.

> **Indexer reference**: See [docs/EVENT_SCHEMA.md](./docs/EVENT_SCHEMA.md) for the full canonical schema, field semantics, units, versioning strategy, and example decode mappings.

### Event Types:

#### 1. Round Created
Emitted when admin creates a new prediction round.

```rust
Topic: ("round", "created")
Payload: (
  round_id: u64,           // Unique round identifier
  start_price: u128,       // Initial XLM price (4 decimals: 2297 = $0.2297)
  start_ledger: u32,       // Ledger when round begins
  bet_end_ledger: u32,     // Last ledger to place bets
  end_ledger: u32,         // Ledger when round can be resolved
  mode: u32                // 0 = Up/Down, 1 = Precision
)
```

**Use Case**: Index new rounds, display active games, trigger notifications.

#### 2. Bet Placed (Up/Down Mode)
Emitted when user places a bet in Up/Down mode.

```rust
Topic: ("bet", "placed")
Payload: (
  user: Address,           // User who placed the bet
  round_id: u64,          // Round identifier
  amount: i128,           // Bet amount in stroops (1 vXLM = 10^7 stroops)
  side: u32               // 0 = Up, 1 = Down
)
```

**Use Case**: Track user bets, calculate pool sizes, display live betting activity.

#### 3. Price Prediction (Precision Mode)
Emitted when user submits a price prediction in Precision/Legends mode.

```rust
Topic: ("predict", "price")
Payload: (
  user: Address,           // User who made prediction
  round_id: u64,          // Round identifier
  predicted_price: u128,  // Predicted price (4 decimals: 2297 = $0.2297)
  amount: i128            // Stake amount in stroops
)
```

**Use Case**: Track predictions, show leaderboard before resolution, display user guesses.

#### 4. Round Summary (Canonical Terminal Event)
Emitted exactly once per terminal round transition — competitive resolution,
admin cancellation, or min-participants fallback. Replaces the previously
separate `("round", "resolved")`, `("round", "cancelled")`, and
`("round", "fallback")` events.

```rust
Topic: ("round", "summary")
Payload: (
  version: u32,              // Schema version (0 for this layout)
  round_id: u64,             // Round identifier
  status: u32,               // 0 = Resolved, 1 = Cancelled, 2 = FallbackRefund
  mode: u32,                 // 0 = Up/Down, 1 = Precision
  price_start: u128,         // Starting price (4 decimals)
  price_final: u128,         // Settlement price or 0 for cancel/fallback
  pool_up: i128,             // Up-side pool at terminal time (stroops)
  pool_down: i128,           // Down-side pool at terminal time (stroops)
  participant_count: u32,    // Total unique participants
  total_pot: i128,           // Total accumulated round pot (stroops)
  fee_amount: i128,          // Protocol fees collected (stroops)
  settled_at_ledger: u32,    // Ledger sequence when archived
  confidence: Option<u32>    // Oracle confidence in bps (None for cancel/fallback)
)
```

**Use Case**: Single canonical event for all terminal round states. Indexers should listen for this event and ignore legacy topic names.

#### 5. Participant Payout Outcome
Emitted once per participant during round resolution.

```rust
Topic: ("payout", "outcome")
Payload: (
  round_id: u64,          // Round identifier
  mode: u32,              // 0 = Up/Down, 1 = Precision
  user: Address,          // Participant address
  gross_payout: i128,     // Pending winnings credited in stroops; 0 for losses
  outcome_type: u32       // 0 = loss, 1 = win, 2 = refund
)
```

**Use Case**: Reconstruct participant-level settlement outcomes for analytics, UX, and dispute forensics without replaying storage reads.

#### 6. Winnings Claimed
Emitted when user claims their pending winnings.

```rust
Topic: ("claim", "winnings")
Payload: (
  user: Address,          // User claiming winnings
  amount: i128            // Amount claimed in stroops
)
```

**Use Case**: Track payouts, display claim history, calculate platform volume.

#### 7. Windows Updated
Emitted when admin updates bet/run window durations.

```rust
Topic: ("windows", "updated")
Payload: (
  bet_window_ledgers: u32,  // Number of ledgers for betting phase
  run_window_ledgers: u32   // Total ledgers until resolution
)
```

**Use Case**: Update frontend timers, recalculate round schedules.

#### 8. Initial Mint
Emitted when new user mints their first 1000 vXLM.

```rust
Topic: ("mint", "initial")
Payload: (
  user: Address,          // New user address
  amount: i128            // Minted amount (1000 * 10^7 stroops)
)
```

**Use Case**: Track new users, display welcome messages, analytics.

#### 9. Oracle Heartbeat
Emitted when the oracle records an on-chain liveness heartbeat.

```rust
Topic: ("oracle", "heartbeat")
Payload: (
  timestamp: u64,  // Unix epoch seconds of the heartbeat
  status: u32      // 0 = active, 1 = degraded, 2 = offline
)
```

### Event Consumption

#### TypeScript Example (Frontend/Indexer):
```typescript
import { SorobanRpc } from '@stellar/stellar-sdk';

const server = new SorobanRpc.Server('https://soroban-testnet.stellar.org');

// Get transaction events
const txResult = await server.getTransaction(txHash);
const events = txResult.events;

// Parse round created event
const roundCreatedEvent = events.find(e => 
  e.topic[0] === 'round' && e.topic[1] === 'created'
);

if (roundCreatedEvent) {
  const [roundId, startPrice, startLedger, betEnd, endLedger, mode] = roundCreatedEvent.value;
  console.log(`New round ${roundId} created at price ${startPrice}`);
}
```

#### Event Subscription Pattern:
```typescript
// Poll for new rounds
async function watchForNewRounds(contractId: string) {
  const latestLedger = await server.getLatestLedger();
  
  const events = await server.getEvents({
    startLedger: latestLedger.sequence - 100,
    filters: [{
      type: 'contract',
      contractIds: [contractId],
      topics: [['round'], ['created']]
    }]
  });
  
  return events.events.map(parseRoundCreatedEvent);
}
```

### Indexer Implementation Notes:

1. **Event Ordering**: Events are emitted in transaction order within each ledger.
2. **Uniqueness**: Use `(round_id, user, event_type)` for deduplication.
3. **Decimal Handling**: All prices use 4 decimal places (divide by 10000 for display).
4. **Stroops**: All amounts are in stroops (1 vXLM = 10^7 stroops).
5. **Address Format**: Stellar addresses are 56-character G-prefixed strings.

---

## 📊 Contract Functions

Complete list of public contract entrypoints (183 total — one per `pub fn` in the `#[contractimpl]` block of [contracts/src/contract.rs](contracts/src/contract.rs)), grouped by area. This replaces the previous abbreviated User/Admin/Oracle/Query grouping (~30 entries) with a full enumeration so the table stays in parity with the contract source.

### Initialization & Schema

- `initialize(admin: Address, oracle: Address)` — Initializes the contract with admin and oracle addresses (one-time only)
- `get_schema_version()` — Returns the stored schema version.
- `migrate_schema_v1_to_v2(dry_run: bool)` — Migrates legacy schema version 1 → version 2 (admin only).
- `migrate_schema_v2_to_v3(dry_run: bool)` — Migrates schema version 2 → version 3 (admin only).
- `announce_next_schema(target_version: u32)` — Announces a target schema version for the next planned migration (admin only).
- `get_next_schema()` — Returns the announced next schema version, if any.
- `clear_next_schema()` — Clears a previously announced next schema version (admin only).

### Runtime Mode, Pause & Policy Gate

- `is_paused()` — Returns whether the contract is currently paused
- `pause_contract()` — Pauses the contract for emergency recovery (admin only)
- `unpause_contract()` — Unpauses the contract after recovery (admin only)
- `get_runtime_mode()` — Returns the current runtime mode (0 = Normal, 1 = ClaimsOnly, 2 = FullyPaused)
- `set_runtime_mode(mode: u32)` — Sets the runtime mode of the contract (admin only)
- `is_action_allowed(action: PolicyAction)` — Returns whether `action` is currently permitted under the PolicyGate for the contract's runtime mode (Issue #261).
- `get_protocol_health()` — Returns a composite protocol health status
- `get_protocol_status()` — Returns the global status of the protocol.

### Admin & Oracle Identity

- `get_admin()` — Query admin address.
- `get_oracle()` — Query oracle address.

### Oracle Configuration & Health

- `set_oracle_max_deviation_bps(bps: Option<u32>)` — Schedules a timelocked oracle deviation update
- `get_oracle_max_deviation_bps()` — Returns the configured oracle max deviation bps, if set.
- `set_deviation_ref_mode(mode: DeviationReferenceMode, window_samples: u32)` — Sets the oracle deviation reference mode — `StartPrice` (default) or `Twap` — and, for `Twap`, the trailing sample window size (admin only, Issue #266).
- `get_deviation_ref_mode()` — Returns the configured deviation reference mode (default `StartPrice`, Issue #266).
- `get_deviation_window_samples()` — Returns the configured TWAP window size in samples (Issue #266).
- `get_twap_samples()` — Returns the recorded TWAP price samples, most-recent last (Issue #266).
- `set_attestation_key(key: Option<BytesN<32>>)` — Sets (or clears) the ed25519 public key used to verify oracle attestation signatures (admin only, Issue #263).
- `get_attestation_key()` — Returns the configured attestation signing key, if enabled (Issue #263).
- `arm_oracle_deviation_override()` — Arms a one-shot override to bypass deviation checks for the next settlement (admin only).
- `set_oracle_min_confidence_bps(min_bps: Option<u32>)` — Sets the minimum oracle confidence threshold in basis points (admin only).
- `set_oracle_strict_mode(enabled: bool)` — Enables or disables strict mode for oracle confidence (admin only).
- `get_oracle_min_confidence_bps()` — Returns the configured minimum oracle confidence bps, if set.
- `get_oracle_strict_mode()` — Returns whether oracle strict mode is enabled.
- `set_hb_strict_mode(enabled: bool)` — Enables or disables strict mode for oracle heartbeat health at settlement (admin only, Issue #264).
- `get_hb_strict_mode()` — Returns whether oracle heartbeat strict mode is enabled (Issue #264).
- `arm_hb_override()` — Arms a one-shot override to bypass the heartbeat health gate for the next settlement (admin only, Issue #264).
- `get_hb_override_armed()` — Returns whether the oracle heartbeat override is currently armed (Issue #264).
- `set_hb_grace_seconds(seconds: u64)` — Sets the grace period in seconds between heartbeat staleness and settlement block (admin only, Issue #264).
- `get_hb_grace_seconds()` — Returns the configured heartbeat grace period in seconds (default 0, Issue #264).
- `update_oracle_heartbeat(status: u32)` — Records an oracle heartbeat (oracle only).
- `get_oracle_heartbeat()` — Returns the most recent oracle heartbeat record, if any.
- `is_oracle_live()` — Returns `true` if the oracle has a non-stale heartbeat with status not offline (2).
- `set_oracle_stale_threshold(seconds: u64)` — Schedules a timelocked stale threshold update
- `get_oracle_stale_threshold()` — Returns the configured oracle stale threshold, or the default (3600 s) if not set.
- `propose_oracle_rotation(new_oracle: Address, expires_in_seconds: u64)` — Proposes a new oracle address with an expiry window (admin only).
- `accept_oracle_rotation()` — Accepts a pending oracle rotation proposal before expiry (any caller).
- `cancel_oracle_rotation()` — Cancels a pending oracle rotation proposal before it expires (admin only).
- `get_oracle_rotation_proposal()` — Returns the pending oracle rotation proposal, if any.
- `schedule_oracle_stale_threshold(seconds: u64)` — Schedules a timelocked oracle stale-threshold update.
- `schedule_oracle_deviation_bps(bps: Option<u32>)` — Schedules a timelocked oracle max-deviation-bps update.
- `schedule_oracle_timestamp_skew(seconds: u64)` — Schedules a timelocked update to the oracle timestamp skew (admin only).
- `get_oracle_timestamp_skew()` — Returns the configured oracle timestamp skew, or the default (300 s) if not set.
- `set_oracle_quorum_config(config: Option<OracleQuorumConfig>)` — Sets the multi-feed oracle quorum configuration (admin only).
- `get_oracle_quorum_config()` — Returns the configured multi-feed oracle quorum config, if any.

### Access Control (Allow/Deny List)

- `set_access_control_enabled(enabled: bool)` — Enables or disables allow/deny-list enforcement (admin only).
- `is_access_control_enabled()` — Returns whether allow/deny-list enforcement is enabled.
- `add_allowlisted(user: Address)` — Adds a user to the allowlist (admin only).
- `remove_allowlisted(user: Address)` — Removes a user from the allowlist (admin only).
- `add_denylisted(user: Address)` — Adds a user to the denylist (admin only).
- `remove_denylisted(user: Address)` — Removes a user from the denylist (admin only).
- `is_allowlisted(user: Address)` — Returns whether a user is allowlisted.
- `is_denylisted(user: Address)` — Returns whether a user is denylisted.

### Governance & Constitution

- `set_gov_approver(approver: Address)` — Configures the secondary governance approver (admin only).
- `get_gov_approver()` — Returns the configured secondary governance approver address, if set.
- `set_gov_proposal_ttl(ttl_ledgers: u32)` — Sets default proposal TTL in ledgers (admin only).
- `get_gov_proposal_ttl()` — Returns default proposal TTL in ledgers.
- `propose_gov_action(proposer: Address, action: GovAction, custom_ttl: Option<u32>)` — Proposes a protected administrative action (governance admin/approver only).
- `approve_gov_proposal(approver: Address, proposal_id: u64)` — Approves a pending governance proposal (governance admin/approver only, distinct from proposer).
- `execute_gov_proposal(executor: Address, proposal_id: u64)` — Executes an approved governance proposal (governance admin/approver only).
- `cancel_gov_proposal(canceller: Address, proposal_id: u64)` — Cancels an unexecuted governance proposal (governance admin/approver only).
- `get_gov_proposal(proposal_id: u64)` — Queries details for a governance proposal.
- `establish_constitution(veto_window_ledgers: u32, timelock_ledgers: u32, dual_approval_required: bool)` — Establishes the on-chain constitution with governance rules (admin only).
- `get_constitution()` — Returns the on-chain constitution metadata, if established.
- `propose_amendment(proposer: Address, parameter_name: Symbol, new_value: i128)` — Proposes a parameter amendment with timelock and optional veto window.
- `veto_amendment(vetoer: Address, amendment_id: u64)` — Vetoes a pending amendment before its veto window expires.
- `activate_amendment(activator: Address, amendment_id: u64)` — Activates an amendment after timelock expires.
- `get_amendment(amendment_id: u64)` — Retrieves an amendment proposal record by ID.

### Protocol Configuration (Timelocked Admin Params)

- `batch_touch_ttl(keys: Vec<DataKeyCore>)` — Auth-gated batch TTL extension for allowlisted storage keys (admin only).
- `get_access_state(user: Address)` — Returns the raw allow/deny state for a user.
- `get_access_policy(user: Address)` — Returns the effective access policy for a user.
- `set_windows(bet_ledgers: u32, run_ledgers: u32)` — Schedules a timelocked windows update (alias for `schedule_windows`).
- `set_max_stake(max_amount: Option<i128>)` — Schedules a timelocked maximum per-bet stake update.
- `get_max_stake()` — Returns the configured maximum per-bet stake, if any.
- `set_min_bet(min_amount: Option<i128>)` — Schedules a timelocked minimum-bet (dust protection) update (Issue #269).
- `schedule_min_bet(min_amount: Option<i128>)` — Schedules a timelocked minimum-bet update.
- `get_min_bet()` — Returns the configured minimum bet, if enabled (Issue #269).
- `set_max_user_exposure(max_exposure: Option<i128>)` — Schedules a timelocked per-user exposure cap update.
- `get_max_user_exposure()` — Returns the configured per-user exposure cap, if any.
- `set_max_pending_winnings(max_pending: Option<i128>)` — Schedules a timelocked maximum pending-winnings cap update.
- `schedule_windows(bet_ledgers: u32, run_ledgers: u32)` — Schedules a timelocked betting/run window update.
- `schedule_max_stake(max_amount: Option<i128>)` — Schedules a timelocked maximum per-bet stake update.
- `schedule_max_user_exposure(max_exposure: Option<i128>)` — Schedules a timelocked per-user exposure cap update.
- `schedule_max_pending_winnings(max_pending: Option<i128>)` — Schedules a timelocked maximum pending-winnings cap update.
- `schedule_protocol_fee_bps(bps: Option<u32>)` — Schedules a timelocked protocol fee bps update.
- `set_protocol_fee_bps(bps: Option<u32>)` — Schedules a timelocked protocol fee bps update (alias for `schedule_protocol_fee_bps`).
- `get_protocol_fee_bps()` — Returns the configured protocol fee in basis points, if any.
- `get_protocol_fee_treasury()` — Returns the accumulated protocol fee treasury balance.
- `withdraw_protocol_fee(recipient: Address, amount: i128)` — Withdraws from the protocol fee treasury to a recipient (admin only).
- `get_pending_config_change(kind: ConfigChangeKind)` — Returns the pending timelocked change for a given config kind, if any.
- `apply_scheduled_changes(kind: ConfigChangeKind)` — Applies a pending timelocked config change once its activation ledger has passed.
- `cancel_config_change(kind: ConfigChangeKind)` — Cancels a pending timelocked config change (admin only).
- `get_max_pending_winnings()` — Returns the configured maximum pending-winnings cap, if any.
- `set_min_participants(min: Option<u32>)` — Sets the minimum participant count required to avoid a fallback refund (admin only).
- `get_min_participants()` — Returns the configured minimum participant count, if any.
- `set_max_precision_participants(max: u32)` — Sets the maximum number of Precision-mode participants per round (admin only).
- `get_max_precision_participants()` — Check the active Precision participant cap
- `set_precision_payout_policy(policy: u32)` — Sets the Precision-mode payout policy (admin only).
- `get_precision_payout_policy()` — Returns the configured Precision-mode payout policy.
- `set_mint_limit(limit: u32)` — Sets the per-call faucet mint limit (admin only).
- `get_mint_limit()` — Returns the configured faucet mint limit.
- `set_epoch_mint_budget(budget: i128)` — Sets the total faucet mint budget per epoch (admin only).
- `get_epoch_mint_budget()` — Returns the configured epoch mint budget.
- `set_archive_retention(limit: u32)` — Sets how many archived rounds are retained (admin only).
- `get_archive_retention()` — Returns the configured archive retention limit.
- `set_pending_winnings_expiry(ledgers: u32)` — Schedules a timelocked pending-winnings expiry update.
- `schedule_pending_winnings_expiry(ledgers: u32)` — Schedules a timelocked pending-winnings expiry update.
- `get_pending_winnings_expiry()` — Returns the configured pending-winnings expiry in ledgers, if enabled.
- `reclaim_expired_pending_winnings(user: Address)` — Sweeps a user's expired, unclaimed pending winnings back to the treasury.
- `set_close_buffer_ledgers(buffer_ledgers: u32)` — Sets the close-buffer window before round end during which new bets are rejected (admin only).
- `get_close_buffer_ledgers()` — Returns the configured close-buffer length in ledgers.
- `get_bet_window_ledgers()` — Returns the configured betting-window length in ledgers.
- `get_run_window_ledgers()` — Returns the configured run-window length in ledgers.
- `set_early_cashout_bps(bps: Option<u32>)` — Sets the early cash-out penalty rate in basis points (admin only).
- `get_early_cashout_bps()` — Returns the configured early cash-out penalty bps, if enabled.
- `set_dispute_ledgers(ledgers: u32)` — Sets the dispute window length in ledgers (admin only).
- `get_dispute_ledgers()` — Returns the configured dispute window length in ledgers.
- `set_fee_model(model: FeeModel)` — Sets the fee incidence model (admin only).
- `get_fee_model()` — Returns the configured fee incidence model, defaulting to `FeeOnPot`.

### Round Templates & Creation

- `create_round(start_price: u128, mode: Option<u32>)` — Creates a new prediction round (admin only)
- `set_round_template(start_price: u128, mode: Option<u32>)` — Stores the admin's blueprint for `create_next_from_template` (admin only).
- `clear_round_template()` — Removes the configured round template (admin only).
- `get_round_template()` — Returns the configured round template, if any.
- `create_next_from_template()` — Creates the next round from the configured template (admin only).

### User Actions (Betting / Prediction)

- `place_bet(user: Address, amount: i128, side: BetSide)` — Bet on UP or DOWN (Mode 0).
- `place_precision_prediction(user: Address, amount: i128, predicted_price: u128)` — Predict exact price (Mode 1).
- `predict_price(user: Address, guessed_price: u128, amount: i128)` — Alias for `place_precision_prediction`.
- `commit_prediction(user: Address, hash: BytesN<32>, amount: i128)` — Commits a hashed Precision-mode prediction (commit-reveal flow).
- `reveal_prediction(user: Address, predicted_price: u128, salt: BytesN<32>)` — Reveals a previously committed Precision-mode prediction.
- `mint_initial(user: Address)` — Mints 1000 vXLM for new users (one-time only)

### Settlement, Claims & Disputes

- `resolve_round(payload: OraclePayload)` — Resolves round and triggers payouts (requires `OraclePayload` with price, timestamp, and round ID).
- `resolve_round_multi(payload: MultiFeedPayload)` — Resolves the active round using a multi-feed oracle payload with median settlement and quorum-based outlier rejection.
- `cancel_round(reason: u32)` — Cancels the active round and refunds all stakes (admin only).
- `is_round_cancelled(round_id: u64)` — Returns whether a round was cancelled.
- `claim_winnings(user: Address)` — Withdraws pending winnings.
- `claim_many(users: Vec<Address>)` — Claims pending winnings for up to `MAX_CLAIM_BATCH_SIZE` users in one call.
- `cash_out_early(user: Address)` — Early cash-out during the Running phase for UpDown rounds.
- `void_round(round_id: u64)` — Anyone may call `void_round` during the dispute window to refund all participants their full stakes (void-to-refund path).
- `finalize_round(round_id: u64)` — Anyone may call `finalize_round` after the dispute window expires to distribute winnings to winners (normal settlement outcome).

### Round & Protocol Queries

- `get_user_archive_history(user: Address, offset: u32, limit: u32)` — Returns paginated archived participation history for a user (newest first).
- `get_round_status(round_id: u64)` — Returns the status of a specific round identified by `round_id`.
- `get_active_round()` — View current round details (includes mode).
- `get_one_sided_policy()` — Returns the active one-sided (degenerate market) settlement policy.
- `get_round_pool_stats()` — Returns pool composition stats for the active round.
- `get_round_phase()` — Returns the current phase of the active round.
- `get_market_snapshot()` — Returns a single-read composite snapshot of current market state: round phase, pool composition, timing buffers, and fee configuration.
- `get_last_round_id()` — Query the latest round ID.
- `get_archived_round(round_id: u64)` — Returns the archived record for a settled round.
- `get_recent_archived_rounds(limit: u32)` — Returns the most recently archived rounds.
- `get_user_archived_participation(user: Address, round_id: u64)` — Returns a user's archived participation record for a specific round.

### User Queries

- `get_user_stats(user: Address)` — View wins, losses, streaks.
- `get_pending_winnings(user: Address)` — Check claimable amount.
- `get_user_position(user: Address)` — Check bet in current round (Mode 0).
- `get_user_precision_prediction(user: Address)` — Check prediction in current round (Mode 1).
- `get_precision_predictions()` — View all predictions in current Precision round.
- `get_updown_positions()` — View all positions in current Up/Down round.
- `get_precision_predictions_page(offset: u32, limit: u32)` — Paginated view of predictions in the current Precision round.
- `get_updown_positions_page(offset: u32, limit: u32)` — Paginated view of positions in the current Up/Down round.
- `balance(user: Address)` — Returns user's vXLM balance.
- `simulate_payout(final_price: u128)` — Estimates payouts for the active round given a hypothetical final price.

### Insurance Fund

- `set_insurance_split_bps(bps: u32)` — Sets the insurance accrual split: how many basis points of each protocol fee are directed to the insurance fund (admin only).
- `get_insurance_split_bps()` — Returns the configured insurance split in basis points.
- `set_insurance_coverage_bps(bps: u32)` — Sets the insurance coverage payout rate in basis points (admin only).
- `get_insurance_coverage_bps()` — Returns the configured insurance coverage payout rate.
- `set_insurance_eligible_events(events: Vec<u32>)` — Sets the whitelist of eligible insurance event types (admin only).
- `get_insurance_eligible_events()` — Returns the list of eligible insurance event type discriminants.
- `get_insurance_fund_balance()` — Returns the current insurance fund balance.
- `top_up_insurance_fund(amount: i128)` — Top-ups the insurance fund from the caller's vXLM balance (admin only).
- `withdraw_insurance_fund(recipient: Address, amount: i128)` — Withdraws from the insurance fund to a recipient (admin only, requires governance dual-control when approver is set).

### Leaderboard & Seasons

- `get_leaderboard_by_wins(cursor: Option<Address>, limit: u32)` — Cursor-based page of the global leaderboard ordered by total wins descending.
- `get_leaderboard_by_streak(cursor: Option<Address>, limit: u32)` — Cursor-based page of the global leaderboard ordered by best streak descending.
- `get_current_season_id()` — Id of the currently-active leaderboard season (default 1).
- `get_season_user_stats(season_id: u32, user: Address)` — A user's win/loss/streak stats scoped to one season (active or archived).
- `reset_leaderboard_season()` — Archive the active leaderboard season's rankings and advance to the next season (admin only).
- `get_season_archive(season_id: u32)` — The frozen snapshot of a past season's final rankings.
- `get_season_leaderboard_by_wins(season_id: u32, offset: u32, limit: u32)` — Paginated wins leaderboard for `season_id` — live for the active season, frozen archive for any past season.
- `get_season_leaderboard_by_streak(season_id: u32, offset: u32, limit: u32)` — Paginated best-streak leaderboard for `season_id` — live for the active season, frozen archive for any past season.

---

## 🎮 Use Cases

### 🎯 Entertainment
- Short-term price predictions (5-15 minute rounds)
- Friendly competition and leaderboards
- Track and improve prediction skills

### 📚 Education
- Learn prediction markets risk-free (virtual tokens)
- Understand blockchain interactions
- Practice trading psychology

### 🏦 Future Financial Products
- Expand to real money markets (with proper licensing)
- Multi-asset predictions (BTC, ETH, stocks)
- Longer time horizons (hourly, daily rounds)
- Tournament modes with prizes

---

## 🗺️ Roadmap

### ✅ Phase 1: Core Contract (Completed)
- [x] Virtual token system
- [x] Dual-mode round management (Up/Down + Precision)
- [x] Hybrid resolution logic
- [x] Up/Down betting mechanism with proportional payouts
- [x] Precision prediction mechanism (closest guess wins)
- [x] User statistics tracking
- [x] Comprehensive testing (full suite passing)
- [x] Security hardening
- [x] TypeScript bindings

### 🚧 Phase 2: Infrastructure (In Progress)
- [ ] Deploy to Stellar testnet
- [ ] Oracle service (price feed integration)
- [ ] Backend API
- [ ] Monitoring & analytics

### 📅 Phase 3: Frontend (Q1 2026)
- [ ] React/Next.js web app
- [ ] Wallet integration (Freighter, Albedo)
- [ ] Real-time round updates
- [ ] User dashboard & leaderboards

### 🎯 Phase 4: Production (Q2 2026)
- [ ] External security audit
- [ ] Mainnet deployment
- [ ] Mobile app (React Native)
- [ ] Community features (social betting, tournaments)

---

## 🚀 Testnet Deployment

### GitHub Actions Workflow

The repository includes a controlled deployment workflow at `.github/workflows/deploy_testnet.yml` with two modes:

| Mode | Trigger | Behavior |
|------|---------|----------|
| **Dry-run** | `workflow_dispatch` with `dry_run: true` | Builds WASM, validates config, checks secrets — **no transaction broadcast** |
| **Deploy** | `workflow_dispatch` with `dry_run: false` | Full deployment via `scripts/deploy_testnet.sh` (restricted to maintainers) |

### Required GitHub Secrets

Configure these in the repository **Settings → Secrets and variables → Actions**:

| Secret | Purpose |
|--------|---------|
| `SOROBAN_RPC_URL` | Testnet RPC endpoint (e.g. `https://soroban-testnet.stellar.org`) |
| `SOROBAN_NETWORK_PASSPHRASE` | `Test SDF Network ; September 2015` |
| `DEPLOYER_SECRET_KEY` | Secret key of the account paying deployment fees |
| `SOROBAN_ADMIN_ADDRESS` | Public Stellar address of the contract admin |
| `ORACLE_ADDRESS` | Public Stellar address of the oracle signer |

### Workflow Usage

1. Navigate to **Actions → Deploy Testnet** in the GitHub UI.
2. Click **Run workflow**.
3. Set **dry_run** to `true` for validation, `false` for actual deployment.
4. Deployment mode requires the triggering actor to be a member of `TevaLabs/maintainers`.

### Local Dry-Run

Run the script locally to validate configuration without broadcasting:

```bash
export SOROBAN_RPC_URL="https://soroban-testnet.stellar.org"
export SOROBAN_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
export DEPLOYER_SECRET_KEY="your-secret-key"
export SOROBAN_ADMIN_ADDRESS="G..."
export ORACLE_ADDRESS="G..."

./scripts/deploy_testnet.sh --dry-run
```

### Deployment Checklist

- [ ] All required secrets configured in GitHub repository
- [ ] Deployer account funded with testnet XLM (use [Friendbot](https://friendbot.stellar.org))
- [ ] Admin and oracle addresses are correct Stellar `G...` public keys
- [ ] Contract builds and tests pass (`cargo test --workspace --locked`)
- [ ] Dry-run passes with `--dry-run` flag (no errors)
- [ ] `SOROBAN_NETWORK_PASSPHRASE` matches the target network
- [ ] WASM hash recorded for provenance tracking
- [ ] Post-deployment: call `initialize` with admin + oracle addresses
- [ ] Post-deployment: configure round windows with `set_windows()`
- [ ] Post-deployment: verify with `get_admin()` and `get_oracle()`

For the full staged deployment and incident response playbook, see [docs/DEPLOYMENT_RUNBOOK.md](./docs/DEPLOYMENT_RUNBOOK.md). Operators can execute the machine-checkable checklist with `python3 scripts/check_release_checklist.py --network mainnet --strict`.

### Deployment Script

`scripts/deploy_testnet.sh` performs the following steps:

1. **Build** — Compiles the contract to WASM via `cargo build`
2. **Hash** — Computes SHA-256 of the WASM artifact for provenance
3. **Validate** — Checks all required env vars, secrets, and paths
4. **Deploy** — Uses the Stellar CLI to deploy the contract (skipped in dry-run)
5. **Output** — Prints contract ID, WASM hash, network, and initialization checklist

Safety guarantees:
- Never deploys with missing secrets (fails with clear errors)
- Never broadcasts transactions in dry-run mode
- Non-testnet passphrase triggers a warning
- Deployer secret key is written to a temporary identity file cleaned up on exit

---

## 🤝 Contributing

We welcome contributions from the community! Start with the maintainer workflow docs:

- **[CONTRIBUTOR_MAP.md](./docs/CONTRIBUTOR_MAP.md)** — Module → test → task map (start here!)
- [CONTRIBUTING.md](./CONTRIBUTING.md)
- [GOVERNANCE.md](./GOVERNANCE.md)
- [SUPPORT.md](./SUPPORT.md)
- [COMPATIBILITY_POLICY.md](./COMPATIBILITY_POLICY.md) — ABI/storage/event versioning rules
- [docs/CONTRIBUTOR_MAP.md](./docs/CONTRIBUTOR_MAP.md) — protocol areas, files, tests, and starter tasks
- [CODEOWNERS](./.github/CODEOWNERS)

Here's how you can help:

### Ways to Contribute:
1. **Report Bugs** - Open an issue with reproduction steps
2. **Suggest Features** - Share your ideas for improvements
3. **Submit PRs** - Fix bugs or add features
4. **Improve Docs** - Help make documentation clearer
5. **Write Tests** - Expand test coverage

### Getting Started:
1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Generated Files and Regeneration

This repository contains both source files and generated artifacts. Understanding which files are generated and how to regenerate them is essential for contributions.

#### Files That Are Committed (Source Files):
- **`contracts/src/**`** - Rust source code (manually written)
- **`bindings/src/index.ts`** - TypeScript bindings source (auto-generated but committed for convenience)
- **`Cargo.toml`**, **`package.json`** - Dependency manifests
- **Configuration files** - `.gitignore`, `tsconfig.json`, etc.

#### Files That Are NOT Committed (Build Artifacts):
- **`target/`** - Rust build outputs (WASM binaries, compiled Rust)
- **`bindings/dist/`** - Compiled TypeScript output (JavaScript + type definitions)
- **`node_modules/`** - npm dependencies
- **`contracts/test_snapshots/`** - Test execution records (generated during tests)
- **`contracts/proptest-regressions/`** - Property test regression files (generated during tests)
- **`.soroban/`** - Soroban CLI artifacts

#### How to Regenerate Build Artifacts:

**1. Build the Smart Contract:**
```bash
cd contracts
stellar contract build
```

**2. Regenerate TypeScript Bindings:**
After building the contract, generate the bindings from the WASM file:
```bash
cd ../
stellar contract bindings typescript \
  --wasm target/wasm32v1-none/release/xelma_contract.wasm \
  --output-dir ./bindings/src \
  --overwrite
```

**3. Build TypeScript Bindings:**
Compile the TypeScript bindings to JavaScript:
```bash
cd bindings
npm install
npm run build
```

**4. Run Tests (regenerates test artifacts):**
```bash
cd ../contracts
cargo test
```

> **Note:** Test snapshots and proptest regressions are automatically generated when running tests. These files help ensure test consistency but should not be committed.

#### Before Submitting a PR:

1. **Verify no build artifacts are staged:**
   ```bash
   git status
   # Ensure target/, bindings/dist/, node_modules/, test_snapshots/, proptest-regressions/ are not listed
   ```

2. **If you modified the contract**, regenerate bindings:
   ```bash
   # Build contract
   stellar contract build --package xelma-contract
   
   # Regenerate bindings
   stellar contract bindings typescript \
     --wasm target/wasm32v1-none/release/xelma_contract.wasm \
     --output-dir ./bindings/src \
     --overwrite
   
   # Build bindings
   cd bindings && npm run build && cd ..
   ```

3. **Commit only source files**, not build artifacts:
   - ✅ Commit: `bindings/src/index.ts` (regenerated bindings source)
   - ❌ Don't commit: `bindings/dist/` (compiled output)

### Good First Issues:
Check issues labeled [`good-first-issue`](https://github.com/TevaLabs/Xelma-Blockchain/labels/good-first-issue) to get started!

---

## 📚 Documentation

- **[Contributor Map](./docs/CONTRIBUTOR_MAP.md)** — Module → test → task map (new contributors start here!)
- **[Contributor Task Matrix](./docs/CONTRIBUTOR_TASK_MATRIX.md)** — PR evidence requirements for every task type
- **[Smart Contract](./contracts/src/)** - Modular Rust code (contract, types, errors)
- **[Protocol Spec](./PROTOCOL_SPEC.md)** - Formal invariants, threat model, and test traceability
- **[Security Review](./SECURITY_REVIEW.md)** - Security analysis and best practices
- **[Event Schema](./docs/EVENT_SCHEMA.md)** — Canonical on-chain event schema for indexers
- **[Protocol Fee Incidence](./docs/FEE_MODEL.md)** — FeeOnPot vs FeeOnWinnings, who bears the fee in each round mode, and the conservation invariants
- **[Storage Lifecycle](./docs/storage_lifecycle.md)** — TTL/rent policy for persistent keys
- **[Bindings Guide](./bindings/README.md)** - TypeScript integration guide
- **[Wallet Error Guide](./docs/WALLET_ERROR_GUIDE.md)** - Mapping of contract error codes to UI messages
- **[Test Suite](./contracts/src/tests/)** - Comprehensive test examples
- **[Demo Scenarios](./docs/DEMO.md)** - Scripted Up-win, Down-win, and Precision-tie demos with end-state assertions

---

## 🔗 Related Projects

### Separate Repositories (Coming Soon):
- **Xelma-Frontend** - React web application for users
- **Xelma-Backend** - Oracle service and API
- **Xelma-Mobile** - React Native mobile app

### Technology:
- [Stellar](https://stellar.org/) - Blockchain platform
- [Soroban](https://soroban.stellar.org/) - Smart contract framework
- [Rust](https://www.rust-lang.org/) - Contract language

---

## 📜 License

This project is licensed under the **MIT License** - see the [LICENSE](LICENSE) file for details.

---

## 📧 Contact & Support

- **GitHub**: [@TevaLabs](https://github.com/TevaLabs)
- **Repository**: [Xelma-Blockchain](https://github.com/TevaLabs/Xelma-Blockchain)
- **Issues**: [Report bugs or request features](https://github.com/TevaLabs/Xelma-Blockchain/issues)

---

## 🔧 Maintenance Checklist

When making contract changes, update the following to keep this README in sync:

- [ ] **Test count** — re-run `cargo test` and update badge + inline counts
- [ ] **Error types** — if new `ContractError` variants are added, update the error-type count
- [ ] **Function list** — add/remove entries under *Contract Functions* section
- [ ] **Build artifact name** — if the crate name changes, update `Cargo.toml`, CI workflow, and the binding generation command
- [ ] **SDK version** — after bumping `soroban-sdk`, update the Soroban badge and *Technical Stack* section
- [ ] **Repository structure** — reflect any new source files or directories

---

## 🔄 Upgrade & Storage Schema Versioning

The contract tracks an on-chain **storage schema version** to make upgrades auditable and migration-safe.

- New deployments set `SchemaVersion = 2` deterministically during `initialize`.
- If `SchemaVersion` is missing, the contract treats it as legacy **version 1** for compatibility.
- If `SchemaVersion` is unknown or greater than what the contract supports, mutating entrypoints fail with `UnsupportedSchemaVersion`.

### Migration (v1 → v2)

For legacy deployments (no schema version set), operators can run:

- `migrate_schema_v1_to_v2()`

Guards:
- Migration is blocked while a round is active (prevents partial state interpretation changes).
- The migration emits `("schema","migrated")` with `(from_version, to_version)` for indexers.

---

## 🌟 Acknowledgments

- **Stellar Development Foundation** - For the Soroban platform
- **Rust Community** - For excellent tooling and support
- **Contributors** - Thank you to everyone who helps improve Xelma!

---

<div align="center">

**Built with ❤️ on Stellar Blockchain**

[⭐ Star this repo](https://github.com/TevaLabs/Xelma-Blockchain) | [🐛 Report Bug](https://github.com/TevaLabs/Xelma-Blockchain/issues) | [💡 Request Feature](https://github.com/TevaLabs/Xelma-Blockchain/issues)

</div>
