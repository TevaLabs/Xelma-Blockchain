# Xelma - Decentralized XLM Price Prediction Market

[![Rust](https://img.shields.io/badge/Rust-1.92.0-orange.svg)](https://www.rust-lang.org/)
[![Soroban](https://img.shields.io/badge/Soroban-23.4.0-blue.svg)](https://soroban.stellar.org/)
[![Tests](https://img.shields.io/badge/tests-26%2F26%20passing-brightgreen.svg)]()
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> A trustless, transparent, and decentralized prediction market for XLM price movements built on Stellar blockchain using Soroban smart contracts.

---

## 🎯 What is Xelma?

**Xelma** is a blockchain-based prediction market that allows users to bet on whether the price of Stellar Lumens (XLM) will go **UP** or **DOWN** within a specific time frame. Unlike traditional prediction markets, Xelma is:

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
│  • Admin creates rounds (start price + duration)        │
│  • Users place bets (amount + side: UP/DOWN)           │
│  • Oracle resolves rounds (final price)                 │
│                                                          │
│  [Payout Logic]                                         │
│  • Winners split losers' pool proportionally            │
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
- **Language**: Rust 1.92.0
- **Framework**: Soroban SDK 23.4.0
- **Blockchain**: Stellar (Testnet)
- **Testing**: 26/26 tests passing (100% coverage)

### Key Features:
- ✅ Custom error handling (13 error types)
- ✅ Overflow protection (checked arithmetic)
- ✅ Role-based access control (Admin, Oracle, User)
- ✅ Input validation on all functions
- ✅ Claim-based withdrawal pattern
- ✅ Proportional payout algorithm
- ✅ User statistics tracking

### TypeScript Bindings
- **Language**: TypeScript 5.6.2
- **SDK**: Stellar SDK v13
- **Package**: `@tevalabs/xelma-bindings`
- **Types**: Fully typed contract interface

---

## 📦 Repository Structure

```
Xelma-Blockchain/
├── contracts/                 # Main prediction market contract
│   ├── src/
│   │   └── lib.rs            # Smart contract implementation (1,600 lines)
│   ├── Cargo.toml            # Rust dependencies
│   └── test_snapshots/       # Test execution records
│
├── bindings/                  # TypeScript bindings (auto-generated)
│   ├── src/
│   │   └── index.ts          # Contract types & client (640 lines)
│   ├── dist/                  # Compiled JavaScript
│   ├── package.json           # NPM package config
│   └── README.md              # Bindings usage guide
│
├── target/                    # Build artifacts
│   └── wasm32-unknown-unknown/
│       └── release/
│           └── hello_world.wasm  # Compiled contract
│
├── SECURITY_REVIEW.md         # Comprehensive security audit
├── Cargo.toml                 # Workspace configuration
└── README.md                  # This file
```

---

## 🚀 Quick Start

### Prerequisites

- Rust 1.92.0+
- Stellar CLI (soroban-cli)
- Node.js 18+ (for bindings)

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
cargo test
# Output: 26 passed; 0 failed
```

### 4. Generate & Build Bindings

```bash
cd ../../
stellar contract bindings typescript \
  --wasm target/wasm32-unknown-unknown/release/hello_world.wasm \
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
- ✅ **13 Custom Error Types** - Clear, debuggable error codes
- ✅ **Checked Arithmetic** - All math operations use `checked_*` to prevent overflow
- ✅ **Role-Based Access** - Admin creates rounds, Oracle resolves, Users bet
- ✅ **Input Validation** - All parameters validated (amount > 0, round active, etc.)
- ✅ **No Reentrancy Risk** - CEI pattern (Checks-Effects-Interactions)
- ✅ **State Consistency** - Prevents double betting, validates round lifecycle
- ✅ **26/26 Tests Passing** - Full coverage of edge cases and attack vectors

### Audited:
- [SECURITY_REVIEW.md](./SECURITY_REVIEW.md) - Complete security analysis

**Status**: ✅ Production-ready for testnet  
**Recommendation**: External audit recommended before mainnet deployment

---

## 📊 Contract Functions

### User Functions:
- `mint_initial(user)` - Get 1000 vXLM on first use
- `balance(user)` - Query current balance
- `place_bet(user, amount, side)` - Bet on UP or DOWN
- `claim_winnings(user)` - Withdraw pending winnings
- `get_user_stats(user)` - View wins, losses, streaks
- `get_user_position(user)` - Check bet in current round

### Admin Functions:
- `initialize(admin, oracle)` - One-time contract setup
- `create_round(start_price, duration)` - Start new betting round

### Oracle Functions:
- `resolve_round(final_price)` - Resolve round and trigger payouts

### Query Functions:
- `get_active_round()` - View current round details
- `get_admin()` - Query admin address
- `get_oracle()` - Query oracle address
- `get_pending_winnings(user)` - Check claimable amount

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
- [x] Round management
- [x] Betting mechanism
- [x] Proportional payouts
- [x] User statistics
- [x] Comprehensive testing (26/26)
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

## 🤝 Contributing

We welcome contributions from the community! Here's how you can help:

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

### Good First Issues:
Check issues labeled [`good-first-issue`](https://github.com/TevaLabs/Xelma-Blockchain/labels/good-first-issue) to get started!

---

## 📚 Documentation

- **[Smart Contract](./contracts/src/lib.rs)** - Fully commented Rust code
- **[Security Review](./SECURITY_REVIEW.md)** - Security analysis and best practices
- **[Bindings Guide](./bindings/README.md)** - TypeScript integration guide
- **[Test Suite](./contracts/src/lib.rs#tests)** - Comprehensive test examples

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

## 🌟 Acknowledgments

- **Stellar Development Foundation** - For the Soroban platform
- **Rust Community** - For excellent tooling and support
- **Contributors** - Thank you to everyone who helps improve Xelma!

---

<div align="center">

**Built with ❤️ on Stellar Blockchain**

[⭐ Star this repo](https://github.com/TevaLabs/Xelma-Blockchain) | [🐛 Report Bug](https://github.com/TevaLabs/Xelma-Blockchain/issues) | [💡 Request Feature](https://github.com/TevaLabs/Xelma-Blockchain/issues)

</div>
