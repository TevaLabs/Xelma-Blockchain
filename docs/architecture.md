# Architecture

How the Xelma prediction-market codebase fits together: entrypoints, layers,
storage, runtime modes, and where the Soroban boundary lies. New contributors
should read this alongside [Contributor Map](./CONTRIBUTOR_MAP.md); protocol
rules live in [../PROTOCOL_SPEC.md](../PROTOCOL_SPEC.md).

## Layout (modular, not a monolith)

```text
contracts/            Soroban smart contract (single contract crate)
  src/contract.rs     Contract entrypoints (also: betting, settlement,
                      governance, oracle_committee, insurance, leaderboard,
                      collateral, config, admin, access_control, queries)
  src/storage.rs      Ledger storage keys and accessors
  src/*_math.rs       Pure settlement math (mirrored off-chain, see below)
  src/tests/          20+ unit/property/chaos suites
replay-engine/        xelma-replay: deterministic off-chain round replay
                      for audits and disputes (no chain I/O)
bindings/             @xelma/bindings: TypeScript client for integrators
```

The contract is **one Soroban contract** with domain logic split into modules —
there is no per-domain contract deployment. `common`, `collateral`, and
`oracle_committee` are `pub` for reuse; everything else is crate-private
behind `contract.rs` entrypoints.

## Entrypoints

All state changes go through `contract.rs` (plus the domain modules it
delegates to). The stable surface:

- **Lifecycle**: `initialize`, `pause_contract` / `unpause_contract`,
  `is_paused`
- **Betting & settlement**: bet placement, round lifecycle, settlement and
  claims in `betting.rs` / `settlement.rs` (pure math in `settlement_math.rs`
  and `math_common.rs`)
- **Governance & oracle**: proposals/votes in `governance.rs`, committee
  checks in `oracle_committee.rs`, deviation guards
  (`set_oracle_max_deviation_bps`, deviation reference modes)
- **Insurance & leaderboard**: `insurance.rs`, `leaderboard.rs`
- **Reads**: `queries.rs`, archive history, pool/user views
- **Schema upgrades**: `get_schema_version`, `migrate_schema_v1_to_v2` (+ dry
  run), `migrate_schema_v2_to_v3`, `announce_next_schema` — see
  [Compatibility Policy](../COMPATIBILITY_POLICY.md) for MAJOR/MINOR/PATCH
  classification of ABI, storage, and event changes
- **Policy**: `is_action_allowed` runtime policy actions

Every entrypoint enforces access control first (`access_control.rs`,
`admin.rs`); economic invariants are asserted in code and mirrored by tests
(see [Protocol Spec](../PROTOCOL_SPEC.md)).

## Layers

| Layer | Location | Talks to chain? |
|---|---|---|
| TypeScript bindings | `bindings/` | Yes, via RPC (see [Bindings Guide](../bindings/README.md)) |
| Contract entrypoints | `contracts/src/contract.rs` + domain modules | On-chain |
| Pure math | `contracts/src/settlement_math.rs`, `math_common.rs` | No — also compiled into `replay-engine` |
| Replay/audit | `replay-engine/` | No — deterministic re-execution from events |
| Indexers/consumers | External | Via [Event Schema](./EVENT_SCHEMA.md) |

The replay engine exists so disputes and audits can re-execute rounds
without chain access; its math must stay byte-identical to the on-chain
modules (covered by parity/proptest suites).

## Storage

Ledger keys and accessors live in `contracts/src/storage.rs`. Durability and
TTL/rent policy: [Storage Lifecycle](./storage_lifecycle.md). Key rules:

- Persistent keys carry TTLs; rent-sensitive paths are covered by
  `storage_lifecycle` tests and the TTL allowlist.
- Schema changes go through the versioned migrators with dry-run support —
  never ad-hoc key rewrites (see Compatibility Policy).
- Archive participation/retention has dedicated suites
  (`tests/archive_*.rs`).

## Runtime modes

Two independent notions of "mode":

1. **On-chain runtime mode** (`get_runtime_mode` / `set_runtime_mode`):
   admin-gated behavior flags read by entrypoints.
2. **Build profiles** (`Cargo.toml`): `release` (LTO, `opt-level = "z"`,
   overflow checks on, symbols stripped) vs `release-with-logs`
   (debug assertions on for staging diagnosis).

Environments: local `cargo test --workspace` → testnet
(`deploy_testnet` workflow) → mainnet via the staged release checklist
(`docs/DEPLOYMENT_RUNBOOK.md`, `scripts/check_release_checklist.py`).
Nightly adversarial and extended-fuzz workflows run on schedule.

## Soroban boundaries

- **SDK**: `soroban-sdk = "=23.0.1"` (pinned in workspace dependencies).
- **Outbound**: contract errors surface as `ContractError` discriminants —
  integrators map them via [Wallet Error Guide](./WALLET_ERROR_GUIDE.md) and
  [Status Codes](./STATUS_CODES.md); indexers consume
  [Event Schema](./EVENT_SCHEMA.md).
- **Inbound**: TypeScript callers go through `@xelma/bindings` (ABI drift
  checked by `npm run test:parity`, mirroring the CI `bindings-test` job).
- **WASM budget**: `.wasm-size-budget` and `wasm-size-budget.md` cap binary
  size; `opt-level = "z"` + LTO keep the release build lean.

## Where to go next

- Start here: [Contributor Map](./CONTRIBUTOR_MAP.md), [Task Matrix](./CONTRIBUTOR_TASK_MATRIX.md)
- Invariants & threat model: [Protocol Spec](../PROTOCOL_SPEC.md)
- Storage & rent: [Storage Lifecycle](./storage_lifecycle.md)
- Events: [Event Schema](./EVENT_SCHEMA.md) · Errors: [Status Codes](./STATUS_CODES.md)
- Ops: [Deployment Runbook](./DEPLOYMENT_RUNBOOK.md) · [Security Review](../SECURITY_REVIEW.md)
