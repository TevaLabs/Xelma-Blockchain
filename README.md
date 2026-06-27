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
- ✅ Custom error handling (20 error types)
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
│   │       └── windows.rs
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
│   └── wasm32-unknown-unknown/
│       └── release/
│           └── xelma_contract.wasm  # Compiled contract
│
├── docs/
│   └── EVENT_SCHEMA.md        # Canonical on-chain event schema for indexers
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
cargo build --target wasm32-unknown-unknown --release
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
  --wasm target/wasm32-unknown-unknown/release/xelma_contract.wasm \
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
- ✅ **20 Custom Error Types** - Clear, debuggable error codes
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

#### 4. Round Resolved
Emitted when oracle resolves a round with final price.

```rust
Topic: ("round", "resolved")
Payload: (
  round_id: u64,          // Round identifier
  final_price: u128,      // Actual final price (4 decimals)
  mode: u32               // 0 = Up/Down, 1 = Precision
)
```

**Use Case**: Trigger winner calculations, update leaderboards, notify users of results.

#### 5. Winnings Claimed
Emitted when user claims their pending winnings.

```rust
Topic: ("claim", "winnings")
Payload: (
  user: Address,          // User claiming winnings
  amount: i128            // Amount claimed in stroops
)
```

**Use Case**: Track payouts, display claim history, calculate platform volume.

#### 6. Windows Updated
Emitted when admin updates bet/run window durations.

```rust
Topic: ("windows", "updated")
Payload: (
  bet_window_ledgers: u32,  // Number of ledgers for betting phase
  run_window_ledgers: u32   // Total ledgers until resolution
)
```

**Use Case**: Update frontend timers, recalculate round schedules.

#### 7. Initial Mint
Emitted when new user mints their first 1000 vXLM.

```rust
Topic: ("mint", "initial")
Payload: (
  user: Address,          // New user address
  amount: i128            // Minted amount (1000 * 10^7 stroops)
)
```

**Use Case**: Track new users, display welcome messages, analytics.

#### 8. Round Cancelled
Emitted when admin cancels an active round; all stakes are refunded.

```rust
Topic: ("round", "cancelled")
Payload: (
  round_id: u64,   // Cancelled round
  reason: u32,     // Admin-supplied reason code
  pool_up: i128,   // Up-side pool at cancellation (stroops)
  pool_down: i128  // Down-side pool at cancellation (stroops)
)
```

#### 9. Round Fallback (insufficient participants)
Emitted when a round ends below the minimum-participants threshold; all stakes are refunded.

```rust
Topic: ("round", "fallback")
Payload: (
  round_id: u64,          // Round that triggered the fallback
  participant_count: u32, // Actual participant count
  min_required: u32       // Configured minimum that was not met
)
```

#### 10. Oracle Heartbeat
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

### User Functions:
- `mint_initial(user)` - Get 1000 vXLM on first use
- `balance(user)` - Query current balance
- `place_bet(user, amount, side)` - Bet on UP or DOWN (Mode 0)
- `place_precision_prediction(user, amount, predicted_price)` - Predict exact price (Mode 1)
- `predict_price(user, guessed_price, amount)` - Alias for `place_precision_prediction`
- `claim_winnings(user)` - Withdraw pending winnings
- `get_user_stats(user)` - View wins, losses, streaks
- `get_user_position(user)` - Check bet in current round (Mode 0)
- `get_user_precision_prediction(user)` - Check prediction in current round (Mode 1)

### Admin Functions:
- `initialize(admin, oracle)` - One-time contract setup
- `create_round(start_price, mode)` - Start new betting round (mode: 0=Up/Down, 1=Precision)
- `set_windows(bet_ledgers, run_ledgers)` - Configure round timing windows

### Oracle Functions:
- `resolve_round(payload)` - Resolve round and trigger payouts (requires `OraclePayload` with price, timestamp, and round ID)

### Query Functions:
- `get_active_round()` - View current round details (includes mode)
- `get_last_round_id()` - Query the latest round ID
- `get_admin()` - Query admin address
- `get_oracle()` - Query oracle address
- `get_pending_winnings(user)` - Check claimable amount
- `get_max_precision_participants()` - Check the active Precision participant cap
- `get_precision_predictions()` - View all predictions in current Precision round
- `get_updown_positions()` - View all positions in current Up/Down round

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

- [CONTRIBUTING.md](./CONTRIBUTING.md)
- [GOVERNANCE.md](./GOVERNANCE.md)
- [SUPPORT.md](./SUPPORT.md)
- [COMPATIBILITY_POLICY.md](./COMPATIBILITY_POLICY.md) — ABI/storage/event versioning rules
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
cargo build --target wasm32-unknown-unknown --release
```

**2. Regenerate TypeScript Bindings:**
After building the contract, generate the bindings from the WASM file:
```bash
cd ../
stellar contract bindings typescript \
  --wasm target/wasm32-unknown-unknown/release/xelma_contract.wasm \
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
   cargo build --target wasm32-unknown-unknown --release --package xelma-contract
   
   # Regenerate bindings
   stellar contract bindings typescript \
     --wasm target/wasm32-unknown-unknown/release/xelma_contract.wasm \
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

- **[Smart Contract](./contracts/src/)** - Modular Rust code (contract, types, errors)
- **[Protocol Spec](./PROTOCOL_SPEC.md)** - Formal invariants, threat model, and test traceability
- **[Security Review](./SECURITY_REVIEW.md)** - Security analysis and best practices
- **[Bindings Guide](./bindings/README.md)** - TypeScript integration guide
- **[Test Suite](./contracts/src/tests/)** - Comprehensive test examples

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
