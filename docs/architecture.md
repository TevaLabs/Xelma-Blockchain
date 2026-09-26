# Architecture Overview

This guide is a starting map for contributors. Xelma is a Rust workspace with a Soroban contract crate, a TypeScript RPC client, and a separate off-chain replay tool. The contract is **modular**: its Soroban entrypoint facade routes work to domain modules rather than containing all protocol logic in one implementation file.

For deeper references, see the [contributor module map](CONTRIBUTOR_MAP.md), [protocol specification](../PROTOCOL_SPEC.md), [storage lifecycle](storage_lifecycle.md), [round lifecycle](../ROUND_LIFECYCLE.md), and [status codes](STATUS_CODES.md).

## Workspace at a glance

| Component | Location | Role |
|---|---|---|
| Soroban contract | `contracts/` | On-chain protocol logic and persistent state. Built as `xelma-contract` (`cdylib` and `rlib`). |
| TypeScript bindings | `bindings/` | Typed Soroban RPC client and convenience helpers for applications; not a second implementation of the protocol. |
| Replay engine | `replay-engine/` | Off-chain deterministic replay, transcript, and audit tooling; not deployed as a contract. |
| Documentation and operations | `docs/`, root Markdown files, `scripts/` | Specifications, operator guidance, checks, deployment and test tooling. |

The root [Cargo workspace manifest](../Cargo.toml) includes the contract and replay-engine crates. The contract crate is built for Soroban's `wasm32v1-none` target; deployment guidance and the canonical build command are in the [deployment runbook](DEPLOYMENT_RUNBOOK.md).

## Contract entrypoint and layers

The crate root, [contracts/src/lib.rs](../contracts/src/lib.rs), declares the modules and exports `VirtualTokenContract` and contract-facing types. The public Soroban ABI is the set of methods on `VirtualTokenContract` marked by `#[contractimpl]` in [contracts/src/contract.rs](../contracts/src/contract.rs). That file is the dispatch/facade boundary: most entrypoints delegate into domain modules, while a few queries or small pieces of orchestration are implemented there. An ordinary `pub` Rust function or module is **not** automatically a Soroban entrypoint.

A useful way to follow an operation is:

1. A wallet or application calls a method through the generated TypeScript client and Soroban RPC.
2. Soroban dispatches the call to a `VirtualTokenContract` ABI method. The contract validates authorization, schema and policy as applicable.
3. The entrypoint delegates to a focused domain module, which applies business rules and updates storage through Soroban's `Env` APIs.
4. The invocation returns a typed result or `ContractError`; state changes and published events are part of the on-chain transaction.

### Entrypoint families

These are examples of the public methods exposed through the facade; use `contract.rs` as the source of truth for the full ABI.

| Entry point family | Example ABI methods | Main implementation |
|---|---|---|
| Initialization and lifecycle | `initialize`, `create_round`, `get_active_round` | `admin.rs`, `betting.rs` |
| Participation | `mint_initial`, `place_bet`, `place_precision_prediction`, `commit_prediction`, `reveal_prediction` | `betting.rs` |
| Oracle and settlement | `update_oracle_heartbeat`, `resolve_round`, `cancel_round`, `claim_winnings` | `admin.rs`, `settlement.rs` |
| Configuration and recovery | `set_runtime_mode`, `pause_contract`, `schedule_*`, `apply_scheduled_changes` | `admin.rs`, `config.rs` |
| Reads and status | `get_round_status`, `get_protocol_status`, `get_archived_round`, `get_user_stats` | `contract.rs`, `queries.rs` |

### Domain modules

| Concern | Main module(s) | What to look at |
|---|---|---|
| Public ABI / dispatch | [contract.rs](../contracts/src/contract.rs) | `VirtualTokenContract`, `#[contractimpl]`; exported method signatures and delegation. |
| Round participation | [betting.rs](../contracts/src/betting.rs), [access_control.rs](../contracts/src/access_control.rs) | Round creation, Up/Down bets, Precision predictions and commit/reveal, minting, cash-out, allowlist/denylist checks. |
| Resolution and claims | [settlement.rs](../contracts/src/settlement.rs), [settlement_math.rs](../contracts/src/settlement_math.rs) | Oracle-driven settlement, cancellation/refunds, dispute paths, payout calculations and claims. |
| Administration and runtime controls | [admin.rs](../contracts/src/admin.rs) | Initialization, schema migration, roles, pause modes and oracle safety controls. |
| Configuration and fees | [config.rs](../contracts/src/config.rs) | Risk settings, timelocked changes, protocol fee configuration, treasury and round templates. |
| Read paths | [queries.rs](../contracts/src/queries.rs) | Position, round, archive, history, stats, leaderboard and simulation queries. |
| User and season rankings | [leaderboard.rs](../contracts/src/leaderboard.rs) | Bounded lifetime and season indexes, season snapshots. |
| Insurance | [insurance.rs](../contracts/src/insurance.rs) | Insurance pool accounting and coverage-related operations. |
| Governance | [governance.rs](../contracts/src/governance.rs) | Protected-action proposals and governance/constitution operations. |
| Shared primitives | [common.rs](../contracts/src/common.rs), [math_common.rs](../contracts/src/math_common.rs), [storage.rs](../contracts/src/storage.rs) | Constants and shared helpers, checked arithmetic, canonical round-position cleanup. |
| Contract data and errors | [types.rs](../contracts/src/types.rs), [errors.rs](../contracts/src/errors.rs) | Soroban types/storage-key enums and stable numeric contract errors. |

The crate also declares [collateral.rs](../contracts/src/collateral.rs) and [oracle_committee.rs](../contracts/src/oracle_committee.rs). Their existence as Rust modules does not make their interfaces contract entrypoints; check the `#[contractimpl]` surface and call sites before treating a helper as part of a live on-chain flow.

## Storage model

Contract state is represented by Soroban-serializable types in `types.rs`, including `DataKeyCore`, parameterized `DataKeyScoped`, and extension keys. These are split into key enums in part to stay within Soroban/XDR union-size limits. Most protocol, user, and round records use Soroban persistent storage; selected compact configuration values use instance storage, and short-lived rate-limit counters use temporary storage. Storage choice is part of each key's lifecycle:

- **Protocol-wide state:** admin/oracle addresses, schema and runtime mode, configuration, oracle health, fee/governance state, and counters.
- **User state:** virtual-token balances, pending winnings, and performance statistics.
- **Round-scoped state:** active round, participant index, bets/predictions/commitments, and consumed oracle nonces.
- **Historical state:** compact archived round summaries and participation/outcome indexes, subject to archive retention and storage TTL.

Long-lived persistent keys use the shared TTL-extension helpers in `common.rs` (threshold and bump policies are documented in [storage_lifecycle.md](storage_lifecycle.md)). Round positions and commitments are explicitly removed when the round lifecycle ends via the canonical cleanup helpers in `storage.rs`; archive retention bounds on-chain round summaries. Temporary counters expire by ledger lifecycle. Events are an important consumer/indexer boundary, especially for historical data no longer retained on-chain; see [EVENT_SCHEMA.md](EVENT_SCHEMA.md).

When changing a storage key or serialized type, check migration and compatibility implications in [MIGRATION.md](../MIGRATION.md) and [COMPATIBILITY_POLICY.md](../COMPATIBILITY_POLICY.md), and update the related lifecycle tests.

## Runtime modes and round lifecycle

`RuntimeMode` is persisted under the protocol pause key and has three values:

- **Normal (0):** ordinary policy applies; participation and administration depend on round state and entrypoint preconditions.
- **ClaimsOnly (1):** restricted operating mode intended to preserve recovery and user exit paths. Claims remain available, while policy-gated operations vary by action class; some settlement, cancellation, or administrative recovery paths are intentionally available.
- **FullyPaused (2):** blocks ordinary mutations, including claims. Read-only queries and the controls required to change the runtime mode remain available.

The central policy gate classifies actions, so do not infer access only from `is_paused()`—that boolean is true only for `FullyPaused`. Also distinguish global protocol mode from an individual round's phase: Betting, Running/reveal, and Resolvable are derived from ledger sequence and round windows. The canonical per-entrypoint matrix and status projections are in [STATUS_CODES.md](STATUS_CODES.md).

The common transaction path is round creation → participation → resolution, cancellation, or refund → user claim. Up/Down bets and Precision predictions use separate per-round storage and rules; Precision additionally supports commit/reveal. Oracle submission and safety checks gate resolution. See [ROUND_LIFECYCLE.md](../ROUND_LIFECYCLE.md) and [ORACLE_OPERATOR_RUNBOOK.md](ORACLE_OPERATOR_RUNBOOK.md) for details.

## Soroban and integration boundaries

- **On-chain boundary:** only exported `#[contractimpl]` methods define the contract ABI. Contract execution is deterministic and bounded by Soroban resource limits; persistent storage, ledger sequence/time, auth, and events are accessed through the Soroban SDK `Env`.
- **Authorization boundary:** contract methods enforce Soroban address authorization for privileged and user actions. The oracle is an authorized transaction actor for resolution; oracle payload validation and configured guardrails are contract logic, not a trusted client-side check.
- **Token boundary:** vXLM balances are an internal virtual-token ledger held in contract storage. They are not Stellar native XLM and are not, by themselves, an issued Stellar asset or transfer through a token contract.
- **Client boundary:** `bindings/` provides typed calls, error helpers, and convenience flows over RPC. It does not bypass contract authorization or validation. The parity checks help keep its ABI/error surface aligned with the Rust contract.
- **Replay boundary:** `replay-engine/` consumes recorded round/transcript data to independently reproduce and diagnose outcomes. It is an audit aid, not an oracle, contract runtime, or source of on-chain state.
- **Build/deploy boundary:** build and deploy the `xelma-contract` artifact only; the replay binary and TypeScript package are off-chain artifacts. See [DEPLOYMENT_RUNBOOK.md](DEPLOYMENT_RUNBOOK.md) and [wasm-size-budget.md](wasm-size-budget.md).

## Where contributors should start

1. Read the relevant domain module from the table above, then trace its public entrypoint in `contract.rs`.
2. Find focused coverage under `contracts/src/tests/`; the [contributor map](CONTRIBUTOR_MAP.md) maps modules to tests and tasks.
3. For ABI, storage, or event changes, review the compatibility policy and update bindings/parity or migration materials where needed.
4. Run the checks appropriate to the change, starting with the commands in [CONTRIBUTING.md](../CONTRIBUTING.md) and the task-specific evidence in [CONTRIBUTOR_TASK_MATRIX.md](CONTRIBUTOR_TASK_MATRIX.md).
