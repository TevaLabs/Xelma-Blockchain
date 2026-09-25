# Stake-Weighted Oracle Committee & On-Chain Aggregation

## Overview

The **Stake-Weighted Oracle Committee** replaces single-signer price settlement with a decentralized, cryptoeconomically secured oracle network. Registered oracle feeders lock stake to participate, and price aggregation uses stake-weighted median reporting with slashing hooks for equivocation or stale submissions.

---

## Architecture Requirements

1. **Feeder Registration**: Feeders lock collateral stake to join the committee.
2. **Stake-Weighted Quorum**: Settlements require participating stake $\ge$ configured quorum percentage (e.g. 66%).
3. **Aggregation Math**: Stake-weighted median guarantees resilience against outlier reports.
4. **Slashing Hooks**: Malicious reports or equivocation trigger stake slashing and removal from active committee.
5. **Fallback Protection**: Missing quorum prevents invalid round settlement and allows safe cancellation.

---

## Storage & Data Types

- `CommitteeMember`: `{ feeder: Address, stake: i128, active: bool, registered_at: u64 }`
- `FeederReport`: `{ feeder: Address, price: i128, timestamp: u64 }`
- `AggregatedOraclePrice`: `{ price: i128, total_stake_weight: i128, quorum_reached: bool }`
