# Architecture

This document describes the shape of the Xelma system so a new contributor can
orient themselves from one place: the entrypoints that end-users/sdks call, the
layers the code is split into, the on-chain data stores, and the boundary
between the contract and the Stellar/Soroban runtime. It is intentionally short
and points to deeper docs where relevant.

## System shape

```
┌────────────────────────────────────────────────────────────────┐
│  Consumers                                                     │
│  - End users (wallets / frontends)                             │
│  - Oracle service (price feed)                                 │
│  - Off-chain indexers / explorers (read events & state)        │
└───────────────────────────┬────────────────────────────────────┘
                            │  soroban-sdk + @stellar/stellar-sdk
┌───────────────────────────▼────────────────────────────────────┐
│  TypeScript bindings  (bindings/)                              │
│  - typed client, storage/round types, error decoder            │
│  - package `@xelma/bindings`                                   │
└───────────────────────────┬────────────────────────────────────┘
                            │  invokeContract @ Soroban RPC
┌───────────────────────────▼────────────────────────────────────┐
│  Soroban smart contract  (contracts/ , crate `xelma-contract`) │
│  - entrypoint facade   : contracts/src/contract.rs             │
│  - domain modules      : betting, settlement, config, admin,   │
│                          governance, access_control,           │
│                          leaderboard, queries, settlement_math │
│  - types & storage keys: contracts/src/types.rs                │
│  - errors               : contracts/src/errors.rs              │
│  - storage helpers      : contracts/src/storage.rs             │
└───────────────────────────┬────────────────────────────────────┘
                            │  persistent / temporary instance storage
┌───────────────────────────▼────────────────────────────────────┐
│  Stellar ledger                                               │
│  - contract storage (keys, scopes, TTL)                        │
│  - events (emitted on every terminal transition)               │
└────────────────────────────────────────────────────────────────┘
```

## Layers

### 1. TypeScript bindings (`bindings/`)

The contract's typed client and shared types for non-Rust consumers. See
[`bindings/README.md`](../bindings/README.md) for usage. Notably provides an
error decoder so contract error codes map to human-readable messages
([`docs/WALLET_ERROR_GUIDE.md`](./WALLET_ERROR_GUIDE.md)).

- Package: `@xelma/bindings` (see `bindings/package.json`).
- Depends on `@stellar/stellar-sdk` for network interaction.

### 2. Contract entrypoint facade (`contracts/src/contract.rs`)

`VirtualTokenContract` is the single public contract struct and the entrypoint
thickness over the domain modules. Nearly all functions are thin delegations;
business logic lives in the modules in layer 3. All state-mutating entrypoints
enforce auth (admin / oracle / user), the pause gate, and the runtime mode gate
before acting.

### 3. Domain modules (`contracts/src/`)

| Module | Responsibility |
|---|---|
| `betting.rs` | Bet placement, precision predictions, commit–reveal, early cash-out |
| `settlement.rs` | Round resolution, payouts, one-sided policy, archive writes |
| `settlement_math.rs` + `math_common.rs` | Payout, deviation, TWAP, fee math |
| `config.rs` | Admin configuration, windows, fees, limits, scheduled changes |
| `admin.rs` | Initialization, pause, schema migration, fee treasury |
| `governance.rs` | Proposal lifecycle and guarded actions |
| `access_control.rs` | Allowlist/denylist gating |
| `leaderboard.rs` | Lifetime + season scoped leaderboards |
| `queries.rs` | Read-only market/round/convenience queries |
| `storage.rs` | Storage key helpers and TTL handling |
| `types.rs` | On-chain types, storage keys (`DataKeyCore`/`DataKeyScoped`) |
| `errors.rs` | `ContractError` variants |

### 4. Stellar / Soroban runtime

The contract is deployed as WASM onto the Stellar ledger. It runs in the
`soroban_host` sandbox: `#![no_std]`, no I/O or network — every external input
arrives via the `Env` parameter. See [Soroban boundaries](#soroban-boundaries).

## Entrypoints and the runtime modes

### Contract entrypoints

Entrypoints are the `pub fn`s on `VirtualTokenContract` in
`contracts/src/contract.rs`, grouped into:

- **Lifecycle / admin** — `initialize`, `get_schema_version`,
  `migrate_schema_v1_to_v2`, `migrate_schema_v2_to_v3`, `pause_contract`,
  `unpause_contract`, `set_runtime_mode`, `create_round`,
  `create_next_from_template`, `set_round_template` / `clear_round_template`,
  `set_windows`, fee/limit/config setters (with `schedule_*` variants for the
  timelock path and `apply_scheduled_changes`).
- **User actions** — `mint_initial`, `place_bet`,
  `place_precision_prediction`, `predict_price`, `commit_prediction`,
  `reveal_prediction`, `cash_out_early`, `claim_winnings`, `claim_many`,
  `reclaim_expired_pending_winnings`.
- **Oracle** — `update_oracle_heartbeat`, `resolve_round`,
  `resolve_round_multi`, plus oracle config/rotquorum setters and
  `propose_oracle_rotation` / `accept_oracle_rotation` /
  `cancel_oracle_rotation`.
- **Dispute window** — `set_dispute_ledgers`, `void_round`, `finalize_round`,
  `cancel_round`.
- **Read-only queries** — `get_active_round`, `get_round_status`,
  `get_market_snapshot`, `get_protocol_health`, `get_protocol_status`,
  `get_user_stats`, `get_pending_winnings`, `get_archived_round`,
  leaderboard and season queries, and the `get_*` config getters.

### Bindings package (`bindings/package.json`)

The TypeScript package's scripts are its CI/entry contract:

| Script | Command | Purpose |
|---|---|---|
| `build` | `tsc` | Compile TS → JS |
| `lint` | `tsc --noEmit` | Type-check without emitting |
| `test:parity` | `node src/parity.js` | WASM/ABI parity check |
| `test:errors` | `vitest run tests/contract-error-parity.test.js` | Rust ↔ TS error-code parity |
| `test:wasm` | `vitest run tests/wasm-parity.test.js` | WASM bindings parity |
| `test:integration` | `vitest run tests/integration.test.ts` | Integration tests |
| `test:vitest` | `vitest run` | Full vitest suite |
| `prepublishOnly` | `npm run build && npm run test:parity && npm run test:errors` | Pre-publish gate |

### Runtime modes

The contract exposes three lifecycle modes via `set_runtime_mode` /
`get_runtime_mode` (see `RuntimeMode` in `contracts/src/types.rs` and its
implementation in `contracts/src/admin.rs`):

| Mode | Value | Behaviour |
|---|---|---|
| `Normal` | 0 | Full operation: bets, predictions, settlement, claims. |
| `ClaimsOnly` | 1 | New rounds/bets blocked; only claiming pending winnings. |
| `FullyPaused` | 2 | All mutating actions rejected; read-only queries remain. |

Every mutating entrypoint checks the current mode (and the pause flag) before
proceeding. See [`PROTOCOL_SPEC.md`](../PROTOCOL_SPEC.md) for the associated
state-transition invariants.

## Data stores

All persistent state lives in **contract instance storage** on the Stellar
ledger and is accessed through `env.storage()` under scoped keys. The full
canonical layout is in [`STORAGE_DESIGN.md`](../STORAGE_DESIGN.md) and the key
types (`DataKeyCore` / `DataKeyScoped`) in `contracts/src/types.rs`. Highlights:

| Key family | Stores | Notes |
|---|---|---|
| `Balance(Address)` | Per-user vXLM balance | |
| `PendingWinnings(Address)` | Claimable payout accumulator | |
| `ActiveRound` / `LastRoundId` | Current round + monotonic counter | |
| `Position(round_id, user)` / `PrecisionPosition(round_id, user)` | Per-user bets/predictions | Indexed, O(1) access |
| `RoundParticipants(round_id)` | Participant list for resolution | |
| `ArchivedRound(round_id)` + `RecentArchivedRoundIds` | Post-settlement summaries + FIFO index | Retention-pruned |
| `UserRoundOutcome(round_id, user)` | Per-user outcomes for history | |
| Config keys (`MaxStake`, `ProtocolFeeBps`, `DisputeLedgers`, …) | Admin configuration | Some go through timelock |
| Oracle keys (`OracleHeartbeat`, `OracleQuorum`, …) | Oracle liveness & validation config | |

Storage keys have explicit TTL/rent expectations — see
[`docs/storage_lifecycle.md`](./storage_lifecycle.md).

## Soroban boundaries

- **`#![no_std]`**: the contract `lib.rs` is `#![no_std]` (with `alloc`); tests
  opt into `std`. No standard-library I/O or networking is possible on-chain.
- **`Env` is the only input channel**: contract functions receive
  `env: Env` (and their arguments). All reads (`env.storage()`, `env.ledger()`,
  `env.current_contract_address()`) and writes flow through it. There is no
  "service" layer or HTTP/DB boundary inside the contract.
- **Auth via `Address` / `require_auth`**: role checks (admin / oracle / user)
  happen against `Address` values passed into entrypoints.
- **Virtual tokens, not native assets**: balances are `i128` stroop values in
  contract storage; the protocol does not move native XLM.
- **Ledger-sequenced time**: rounds are scheduled in ledgers (not wall-clock);
  oracle payloads carry timestamps validated against configured skew windows.
- **Events as the external log**: the contract emits canonical events
  (`("round", …)`, `("payout", "outcome")`, `("archive", "pruned")`, …) that
  off-chain indexers consume. Schema in [`docs/EVENT_SCHEMA.md`](./EVENT_SCHEMA.md).

## Related docs

- [`STORAGE_DESIGN.md`](../STORAGE_DESIGN.md) — storage key layout & rationale
- [`ROUND_LIFECYCLE.md`](../ROUND_LIFECYCLE.md) — round state machine
- [`PROTOCOL_SPEC.md`](../PROTOCOL_SPEC.md) — invariants & threat model
- [`docs/EVENT_SCHEMA.md`](./EVENT_SCHEMA.md) — on-chain events
- [`docs/storage_lifecycle.md`](./storage_lifecycle.md) — TTL/rent policy
- [`docs/CONTRIBUTOR_MAP.md`](./CONTRIBUTOR_MAP.md) — module → test → task map