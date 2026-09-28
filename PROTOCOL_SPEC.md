# Xelma Protocol Specification

This document defines the protocol guarantees, trust boundaries, and test
coverage map for the Xelma Soroban prediction market contract. It is intended
for maintainers, auditors, oracle operators, indexer authors, and contributors.

The implementation reference is `contracts/src/contract.rs`; the canonical data
types and errors live in `contracts/src/types.rs` and `contracts/src/errors.rs`.

## Protocol Scope

Xelma is a virtual-token prediction market for XLM price movement. Users receive
one initial vXLM allocation, stake it into one active round, and later claim
pending winnings after oracle resolution or refund paths.

The contract supports two round modes:

| Mode | Value | Settlement rule |
|---|---:|---|
| Up/Down | `0` | Correct side receives stake plus a proportional share of the losing pool. Unchanged price refunds all participants. |
| Precision | `1` | Closest price prediction wins the pot. Ties split the pot evenly; deterministic remainder goes to the first winner in resolution order (see below). |

**Remainder ordering.** "Resolution order" means participants sorted by
`Address` ascending (ties broken lexicographically by the address's
underlying bytes — `common::sort_addresses`), not bet order or generation
order. `settlement.rs` sorts `RoundParticipants` this way before handing the
list to `settlement_math::compute_precision_payouts`, so among tied
winners, the indivisible remainder from splitting the pot always goes to
whichever tied winner has the lexicographically-lowest address — regardless
of stake size or who bet first. See
`contracts/test_vectors/settlement_math.json`'s `precision_remainder_ordering`
section for pure-math golden vectors (2/3/5-way ties, including one with an
unrevealed non-winner) and
`tests::resolution::precision::test_precision_remainder_goes_to_lexicographically_lowest_winner`
(plus its 3-way sibling) for the end-to-end contract-level proof with real
addresses.

### Precision commit-reveal (user-visible rules)

Precision rounds accept either a direct `place_precision_prediction` or a
commit-reveal flow (`commit_prediction` → `reveal_prediction`).

**Commitment format.** The on-chain commitment is a 32-byte SHA-256 digest:

```text
commitment = sha256( predicted_price.to_xdr() || salt.to_xdr() )
```

- The all-zero hash is rejected at commit time (`InvalidCommitment`).
- Clients MUST sample `salt` as 32 cryptographically random bytes.
- The contract additionally rejects clearly weak salts at reveal time:
  all-zero or constant-byte salts fail with `InvalidSalt`.

**Reveal window.** Reveals are accepted only while
`bet_end_ledger ≤ ledger < end_ledger`.

**Unrevealed commitments at settlement (deterministic):**

| Situation | Policy |
|---|---|
| ≥1 prediction revealed (or placed directly) | Unrevealed commitments **forfeit to the pot** and count as losers (anti-griefing). |
| Nobody revealed | All committed stakes are **refunded** to pending winnings (conservation; no locked liquidity). |
| Admin `cancel_round` / insufficient-participants fallback | All stakes including unrevealed commitments are **refunded**. |

Pot conservation (I8) still holds: with mixed reveals,
`Σ winner payouts + protocol fee == total pot` (including forfeited
unrevealed stakes); with all-unrevealed refunds,
`Σ refunds == total pot`.

All token amounts are stored as `i128` stroops where `1 vXLM = 10_000_000`.
Prices are stored as `u128` values scaled to 4 decimal places.

## Roles and Trust Assumptions

| Role | Trust level | Authority | Assumptions |
|---|---|---|---|
| Admin | Trusted operator | Initialize roles, create rounds, configure windows and risk controls, pause/unpause, cancel active rounds. | Admin key custody is secure; admin does not maliciously configure unusable windows or cancel rounds unfairly. |
| Oracle | Trusted data signer | Resolve rounds, submit liveness heartbeat. | Oracle reports accurate prices for the intended round and submits fresh payloads. |
| Users | Untrusted | Mint once, bet/predict, claim winnings, read state. | Users may attempt invalid auth, duplicate bets, timing abuse, overflow inputs, and replay-like calls. |
| Indexers/frontends | Off-chain consumers | Read events and contract state. | Consumers treat `docs/EVENT_SCHEMA.md` as canonical and handle additive events safely. |

The protocol does not currently prove oracle correctness on-chain. A valid
oracle signature plus payload validation establishes authorization and
freshness, not price truth. Mainnet use requires an operational oracle runbook,
monitoring, and incident response policy.

## Core Invariants

### I1. Single Active Round

At most one round can be active at a time. `create_round` must fail without
mutating round state when `ActiveRound` already exists.

Evidence:
- Code: `assert_no_active_round`, `create_round`.
- Tests: `guard_tests.rs`, `lifecycle.rs::test_create_round_while_active_fails`.
- Docs: `ROUND_LIFECYCLE.md`.

### I2. Role Authorization

State-changing entrypoints require the relevant signer:

| Entrypoint class | Required signer |
|---|---|
| Admin operations | `Admin` |
| Oracle resolution and heartbeat | `Oracle` |
| User mint, bet, prediction, claim | `User` |

Unauthorized calls must fail before meaningful state mutation.

Evidence:
- Code: `require_auth()` calls in admin, oracle, and user entrypoints.
- Tests: `initialization.rs`, `lifecycle.rs`, `pause.rs`, `windows.rs`, `security.rs`.

### I3. Pause Safety

When paused, high-risk mutating operations are rejected. Read-only queries remain
available so operators can inspect contract state during an incident.

Evidence:
- Code: `_ensure_not_paused`, `pause_contract`, `unpause_contract`.
- Tests: `pause.rs`, `chaos_recovery.rs::test_chaos_pause_mid_round_then_unpause_resolve`.

### I4. Round Timing

Bets and predictions are only accepted before `bet_end_ledger`. Resolution is
only accepted at or after `end_ledger`. Admin-configured windows must be
positive, bounded, and must close betting before resolution.

Evidence:
- Code: `set_windows`, `place_bet`, `place_precision_prediction`, `resolve_round`.
- Tests: `windows.rs`.

### I5. One Position Per User Per Round

A user may hold at most one Up/Down position or one Precision prediction in the
active round. Duplicate submissions fail.

Evidence:
- Code: indexed keys `Position(round_id, user)` and
  `PrecisionPosition(round_id, user)`.
- Tests: `betting.rs::test_place_bet_twice_same_round`,
  `mode_tests.rs::test_precision_prediction_already_bet`.
- Docs: `STORAGE_DESIGN.md`.

### I6. Mode Isolation

Up/Down bets are only valid in Up/Down rounds. Precision predictions are only
valid in Precision rounds.

Evidence:
- Code: `RoundMode` checks in betting entrypoints.
- Tests: `mode_tests.rs`.

### I7. Balance and Pending-Winnings Accounting

User balances cannot go negative. Bets deduct the staked amount before storing a
position. Resolution credits payouts to `PendingWinnings(user)`. Claims move
pending winnings to balance atomically and clear the pending entry.

Evidence:
- Code: `balance`, `_set_balance`, `_accumulate_pending`, `claim_winnings`.
- Tests: `betting.rs`, `resolution.rs`, `overflow_tests.rs`.

### I8. Settlement Conservation

For each resolved round, credited payouts must not exceed the round pot. Refund
paths return participant stake amounts. Precision tie remainders are assigned
deterministically and no dust is intentionally lost.

Evidence:
- Code: `_resolve_updown_mode`, `_resolve_precision_mode`, refund helpers.
- Tests: `resolution.rs`, `property_invariants.rs`,
  `storage_benchmarks.rs::bench_large_round_resolves_correctly`.

### I9. Checked Arithmetic

Arithmetic that can affect balances, pending winnings, pools, windows, round
IDs, and stats must use checked operations or bounded validation. Overflow must
return a contract error and avoid partial writes in covered payout paths.

Evidence:
- Code: `checked_*`, `payout_add`, `payout_mul`.
- Tests: `overflow_tests.rs`, `edge_cases.rs`.

### I10. Oracle Payload Binding

Oracle resolution payloads must bind to exactly one round, contain a non-zero
price, use a fresh per-round nonce, and satisfy timestamp freshness checks.

#### Two round identifiers

A round carries two distinct identifiers, and they are not interchangeable:

| Field | Type | Meaning | Unique? |
|---|---|---|---|
| `Round.round_id` | `u64` | Monotonic counter, incremented once per created round | Yes, by construction |
| `Round.start_ledger` | `u32` | Ledger sequence at which the round was created | Only via invariant I10-A below |

`OraclePayload.round_id` is a `u32` and is matched against
**`Round.start_ledger`**, not against `Round.round_id`. The field name refers to
"the round" loosely; the value an operator must submit is the active round's
`start_ledger`. This asymmetry is deliberate and load-bearing, because the two
identifiers are used in different places:

- **Payload binding** uses `Round.start_ledger` (`payload.round_id == round.start_ledger`).
- **Nonce replay protection** uses the monotonic id, keyed as
  `ConsumedOracleNonce(Round.round_id, payload.nonce)`.
- **Attestation signatures** cover `payload.round_id` — that is, `start_ledger` —
  so a signature does not disambiguate between rounds sharing a `start_ledger`.

#### I10-A. `start_ledger` uniquely identifies a round

Because binding is by `start_ledger`, correctness requires that a ledger
sequence back at most one round. `Round.start_ledger` is
`env.ledger().sequence()` at creation time, which is **not** inherently unique:
a round can be created, then cancelled or settled, and a replacement created
within the same ledger. Both rounds would then share a `start_ledger` while
holding different `Round.round_id` values.

That combination is exploitable. A payload signed for the first round satisfies
the binding check against the second, and the nonce guard does not catch it:
consumed nonces are namespaced by the monotonic `Round.round_id`, so a nonce
burned in the first round is unconsumed in the second. The result is a
wrong-round settlement in which the replacement round settles at the previous
round's price.

The protocol therefore enforces uniqueness at round creation. `create_round`
records the owning round under `DataKeyScoped::RoundStartLedger(start_ledger)`
and rejects any later round that would reuse a claimed ledger sequence with
`RoundStartLedgerReused`. A same-ledger replacement round could not be settled
unambiguously under a `start_ledger`-keyed binding, so it is refused up front
rather than created and left unsettleable.

Operational consequence: after a cancel or settle, the replacement round must be
created in a later ledger. In normal operation consecutive transactions already
land in different ledgers; a keeper batching cancel and create into a single
ledger must retry the create once the ledger advances.

#### Validated payload semantics

- `payload.round_id` is matched against `Round.start_ledger`.
- `Round.start_ledger` is unique per round (I10-A).
- `payload.nonce` must not already be consumed for `Round.round_id`.
- `payload.timestamp` must not be future-dated.
- `payload.timestamp` must not be stale beyond the configured contract policy.

Evidence:
- Code: `resolve_round`, `resolve_round_multi`, `create_round`.
- Tests: `security.rs`, `adversarial/oracle.rs`.

### I11. Cancellation and Fallback Refunds

Admin cancellation and insufficient-participant fallback paths must refund
participant stakes to pending winnings, remove active round state, and allow
future rounds to be created.

Evidence:
- Code: `cancel_round`, `_refund_under_threshold`.
- Tests: `lifecycle.rs`, `resolution.rs`, `chaos_recovery.rs`.

### I12. Storage Cleanup and Migration Compatibility

Resolution and cancellation must remove indexed participant keys and participant
lists for the completed round. Legacy map keys remain readable for migration
fallbacks but are not written by new betting paths.

Evidence:
- Code: indexed storage writes and cleanup in `contract.rs`.
- Tests: `storage_benchmarks.rs`.
- Docs: `STORAGE_DESIGN.md`.

### I13. Event Semantics

Events are an append-only observability interface. Existing event topic and
payload meanings must remain stable. Additive events are allowed when documented
in `docs/EVENT_SCHEMA.md`.

Canonical event classes:
- Round lifecycle: `("round", "created")`, `("round", "summary")`.
- User actions: `("mint", "initial")`, `("bet", "placed")`,
  `("predict", "price")`, `("claim", "winnings")`,
  `("cashout", "early")`.
- Configuration/liveness: `("windows", "updated")`,
  `("oracle", "heartbeat")`, `("config", "ec_bps")`.

Evidence:
- Code: event publishing calls in `contract.rs`.
- Tests: `lifecycle.rs`, `mode_tests.rs`, `resolution.rs`, `security.rs`,
  `conservation.rs`.
- Docs: `docs/EVENT_SCHEMA.md`.

### I14. Early Cash-Out

When enabled by admin (`EarlyCashoutBps` set to a non-`None` penalty rate in
basis points), a bettor in an UpDown round may exit their position early during the
Running phase (`bet_end_ledger ≤ ledger < end_ledger`). The user receives
`stake * (10000 - penalty_bps) / 10000` as pending winnings; the forfeited
amount is credited to the protocol fee treasury. The full original stake is
deducted from the pool so remaining participants are unaffected.

#### Restrictions & Errors
- **Disabled by default**: Rejects with `EarlyCashoutDisabled` if `EarlyCashoutBps` is unset or `0`.
- **Mode restriction**: UpDown rounds only. Rejects with `WrongModeForCashout` if called on Precision rounds.
- **Phase restriction**: Running phase only (`bet_end_ledger ≤ ledger < end_ledger`). Rejects with `InvalidPhaseForCashout` during Betting or after `end_ledger`.
- **Position requirement**: User must hold an active position in the round. Rejects with `PositionNotFound` if no position exists or if user already cashed out.
- **Operational pause**: Blocked with `ContractPaused` if the protocol is paused or in non-Normal mode.

#### Invariant Conservation
Exact conservation holds at all times:
$$\text{cashout} + \text{forfeit} = \text{stake}$$
The full stake is deducted from the respective side's pool (`pool_up` or `pool_down`), with `cashout` credited to pending winnings and `forfeit` transferred to protocol fees. Pool totals strictly match the sum of remaining active positions.

Evidence:
- Code: `cash_out_early` in `betting.rs`, `set_early_cashout_bps` in `config.rs`, `errors.rs`.
- Tests: `conservation.rs`.
- Docs: `PROTOCOL_SPEC.md` (this section), `docs/WALLET_ERROR_GUIDE.md`.

## Threat Model

### In Scope

- Unauthorized user, admin, or oracle calls.
- Duplicate bets and duplicate predictions.
- Late betting and premature resolution.
- Oracle payload replay across rounds or within a round.
- Stale or future-dated oracle timestamps.
- Invalid round mode usage.
- Arithmetic overflow in accounting paths.
- Storage growth and write amplification during betting.
- Indexer ambiguity caused by undocumented events.

### Out of Scope

- Malicious but authorized admin behavior.
- Compromised oracle signer submitting fresh but false prices.
- Off-chain price feed quality, exchange outages, or aggregation logic.
- Wallet UX, frontend signing prompts, and phishing resistance.
- Stellar network-level consensus failures.
- External economic/legal/regulatory risk of prediction markets.

### Accepted Trust Boundaries

- The admin can pause and cancel rounds. These controls are intended for
  recovery and are not trustless governance.
- The oracle is a single trusted signer in the current architecture.
- Resolution remains O(n) over participants; very large rounds may hit Soroban
  resource limits before protocol-level settlement logic completes.
- TypeScript bindings are generated artifacts and must be kept in parity with
  the Rust contract for safe client use.

## Upgrade and Compatibility Guarantees

- Backward-compatible documentation and additive events do not require a
  migration entry unless they alter consumer assumptions.
- Breaking changes to storage keys, public method signatures, event payload
  order, error codes, or `OraclePayload` semantics must be documented in
  `MIGRATION.md`.
- Legacy storage fallbacks may be removed only after maintainers explicitly
  decide no deployed migration window depends on them.
- Any new public contract method must be reflected in TypeScript bindings and
  parity checks.

## Invariant Coverage Matrix

| ID | Invariant | Primary code | Test coverage | Status |
|---|---|---|---|---|
| I1 | Single active round | `create_round`, `assert_no_active_round` | `guard_tests.rs`, `lifecycle.rs` | Covered |
| I2 | Role authorization | `require_auth()` gates | `initialization.rs`, `lifecycle.rs`, `pause.rs`, `windows.rs`, `security.rs` | Covered |
| I3 | Pause safety | `_policy_gate`, `_ensure_not_paused` | `pause.rs`, `chaos_recovery.rs`, `policy_gate.rs`, `drill.rs`, `pause_policy_matrix.rs` (full `AdminConfig` action × mode matrix) | Covered — see `docs/EMERGENCY_DRILL.md` for the operational matrix |
| I4 | Round timing | `set_windows`, betting/resolution ledger checks | `windows.rs` | Covered |
| I5 | One position per user | indexed position keys | `betting.rs`, `mode_tests.rs` | Covered |
| I6 | Mode isolation | `RoundMode` checks | `mode_tests.rs` | Covered |
| I7 | Balance and pending accounting | `_set_balance`, `_accumulate_pending`, `claim_winnings` | `betting.rs`, `resolution.rs`, `overflow_tests.rs` | Covered |
| I8 | Settlement conservation | payout/refund helpers | `resolution.rs`, `property_invariants.rs` | Covered |
| I9 | Checked arithmetic | `checked_*`, `payout_add`, `payout_mul` | `overflow_tests.rs`, `edge_cases.rs` | Covered with noted precision-error caveat in `SECURITY_REVIEW.md` |
| I10 | Oracle payload binding | `resolve_round`, `create_round` | `security.rs`, `adversarial/oracle.rs` | Covered |
| I11 | Cancellation/fallback refunds | `cancel_round`, `_refund_under_threshold` | `lifecycle.rs`, `resolution.rs`, `chaos_recovery.rs`, `cancel_refund_matrix.rs` (UpDown + Precision, including unrevealed commitments, zero-fee, `Cancelled` archive status) | Covered |
| I12 | Storage cleanup/migration | indexed cleanup and legacy fallbacks | `storage_benchmarks.rs` | Covered |
| I13 | Event semantics | event publishing calls | `lifecycle.rs`, `mode_tests.rs`, `resolution.rs`, `security.rs` | Covered; canonical schema in `docs/EVENT_SCHEMA.md` |
| I14 | Early cash-out | `cash_out_early`, `set_early_cashout_bps` | `conservation.rs` | Covered |

## Contributor Checklist

When changing protocol behavior:

1. Identify which invariant is affected.
2. Update this document if the invariant, trust boundary, or compatibility
   guarantee changes.
3. Add or update tests listed in the coverage matrix.
4. Update `docs/EVENT_SCHEMA.md` for event changes.
5. Update `MIGRATION.md` for breaking ABI, storage, event, or error changes.
6. Regenerate and validate TypeScript bindings for public ABI changes.
