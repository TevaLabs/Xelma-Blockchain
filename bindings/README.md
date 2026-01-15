# Xelma - Decentralized XLM Price Prediction Market

**TypeScript Bindings for Xelma Smart Contract**

## 🎯 What is Xelma?

Xelma is a **decentralized prediction market** built on the Stellar blockchain (Soroban) that allows users to bet on XLM price movements. It combines the excitement of price prediction with blockchain transparency and fairness.

## 🔥 Problem We're Solving

### Traditional Prediction Markets Are:
- ❌ **Centralized** - Single point of failure, can be shut down
- ❌ **Opaque** - Users can't verify fair payouts or manipulation
- ❌ **High barriers** - Require KYC, bank accounts, or specific payment methods
- ❌ **Slow payouts** - Days or weeks to withdraw winnings
- ❌ **Trust-based** - You must trust the operator won't steal funds

### Xelma Solves This With:
- ✅ **Decentralized** - Runs on Stellar blockchain, unstoppable
- ✅ **Transparent** - All bets, rounds, and payouts are on-chain and verifiable
- ✅ **Permissionless** - Anyone with a Stellar wallet can participate
- ✅ **Instant payouts** - Claim winnings immediately after round resolution
- ✅ **Trustless** - Smart contract logic ensures fair, automated payouts
- ✅ **Low fees** - Stellar's minimal transaction costs (~0.00001 XLM)

## 🎮 How It Works

### For Users (Bettors):

1. **Get Virtual Tokens** - Receive 1000 vXLM (virtual XLM) on first interaction
2. **Join Active Round** - Place bets on whether XLM price will go UP or DOWN
3. **Wait for Resolution** - Oracle resolves round with actual price data
4. **Claim Winnings** - Winners split the losing pool proportionally
5. **Track Performance** - View your win/loss stats and streaks

### For Admins:

- **Create Rounds** - Set start price and duration (e.g., 60 ledgers ≈ 5 minutes)
- **Manage System** - One-time initialization of contract

### For Oracles:

- **Resolve Rounds** - Submit final XLM price from trusted data source
- **Trigger Payouts** - Smart contract automatically calculates and distributes winnings

## 🏗️ Architecture

```
┌─────────────────┐
│   Frontend      │  ← React/Next.js UI
│   (React App)   │
└────────┬────────┘
         │ imports @tevalabs/xelma-bindings
         ↓
┌─────────────────┐
│   Bindings      │  ← This package (TypeScript types + client)
│  (TS Library)   │
└────────┬────────┘
         │ calls via Stellar SDK
         ↓
┌─────────────────┐
│ Smart Contract  │  ← Rust/Soroban contract on Stellar
│   (Blockchain)  │
└─────────────────┘
```

## 💡 Key Features

### Virtual Token System
- **1000 vXLM** initial balance per user
- No real money required to start
- Balance tracked on-chain

### Fair Proportional Payouts
```
Winner's Payout = Their Bet + (Their Bet / Winning Pool) × Losing Pool

Example:
- Alice bets 100 vXLM UP, Bob bets 200 vXLM UP (Total UP: 300)
- Charlie bets 150 vXLM DOWN (Total DOWN: 150)
- Price goes UP → Alice and Bob win
- Alice gets: 100 + (100/300) × 150 = 150 vXLM (50% profit)
- Bob gets: 200 + (200/300) × 150 = 300 vXLM (50% profit)
```

### Security Features
- ✅ **Role-based access** - Admin, Oracle, User permissions
- ✅ **Overflow protection** - Checked arithmetic prevents exploits
- ✅ **Input validation** - All parameters validated before execution
- ✅ **One bet per round** - Prevents double-betting attacks
- ✅ **Claim-based withdrawal** - Users control when to claim winnings

### User Statistics
- Total wins / losses
- Current winning streak
- Best streak ever achieved

## 📦 Using These Bindings

### Installation

\`\`\`bash
npm install @tevalabs/xelma-bindings
# or
yarn add @tevalabs/xelma-bindings
\`\`\`

### Quick Start

\`\`\`typescript
import { Client, BetSide } from '@tevalabs/xelma-bindings';

// Initialize client
const client = new Client({
  contractId: 'YOUR_CONTRACT_ID',
  networkPassphrase: Networks.TESTNET,
  rpcUrl: 'https://soroban-testnet.stellar.org'
});

// Mint initial tokens
await client.mint_initial({ user: userAddress });

// Check balance
const balance = await client.balance({ user: userAddress });
console.log('Balance:', balance); // 10000000000 (1000 vXLM in stroops)

// Get active round
const round = await client.get_active_round();
if (round) {
  console.log('Start price:', round.price_start);
  console.log('UP pool:', round.pool_up);
  console.log('DOWN pool:', round.pool_down);
}

// Place a bet
await client.place_bet({
  user: userAddress,
  amount: 100_0000000n, // 100 vXLM
  side: BetSide.Up
});

// Check stats
const stats = await client.get_user_stats({ user: userAddress });
console.log('Wins:', stats.total_wins);
console.log('Current streak:', stats.current_streak);

// Claim winnings
const claimed = await client.claim_winnings({ user: userAddress });
console.log('Claimed:', claimed);
\`\`\`

## 🛠️ Development Setup

### Install Dependencies

\`\`\`bash
npm install
# or
yarn install
\`\`\`

### Build TypeScript

\`\`\`bash
npm run build
# or
yarn build
\`\`\`

### Type Definitions

All types are exported and fully documented:

\`\`\`typescript
import { 
  Client,          // Main contract client
  BetSide,         // Enum: Up | Down
  Round,           // Active round interface
  UserStats,       // User performance stats
  UserPosition,    // User's bet in a round
  ContractError    // Error codes (1-13)
} from '@tevalabs/xelma-bindings';
\`\`\`

## 🔗 Related Repositories

- **Smart Contract**: [github.com/TevaLabs/Xelma-Blockchain](https://github.com/TevaLabs/Xelma-Blockchain)
- **Frontend**: [Coming Soon]
- **Backend/Oracle**: [Coming Soon]

## 🌟 Use Cases

### Entertainment
- Predict XLM price movements in short rounds (5-15 minutes)
- Compete with friends on leaderboards
- Track winning streaks

### Education
- Learn about prediction markets without financial risk
- Understand blockchain interactions
- Practice trading psychology

### Future Extensions
- Real money markets (with proper licensing)
- Multiple asset predictions (BTC, ETH, stocks)
- Longer-term rounds (hours, days)
- Tournament modes with prizes

## 🎯 Roadmap

- [x] Core prediction market contract
- [x] TypeScript bindings generation
- [x] Security review and testing (26/26 tests passing)
- [ ] Frontend UI (React/Next.js)
- [ ] Oracle service for price feeds
- [ ] Leaderboard system
- [ ] Mobile app (React Native)
- [ ] Mainnet deployment

## 📄 License

MIT License - See LICENSE file for details

## 🤝 Contributing

We welcome contributions! Please check the main repository for:
- Open issues labeled \`good-first-issue\`
- Contribution guidelines
- Code of conduct

## 📧 Contact

- **GitHub**: [@TevaLabs](https://github.com/TevaLabs)
- **Issues**: Report bugs or request features in respective repos

---

**Built with ❤️ on Stellar Blockchain**
