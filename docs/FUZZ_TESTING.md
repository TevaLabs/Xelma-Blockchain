# Protocol Lifecycle Property-Based Fuzz Testing

This document details the property-based fuzz testing harness (`contracts/src/tests/fuzz_lifecycle.rs`) designed to validate core protocol invariants across randomized, multi-step lifecycle operation sequences.

---

## Execution Modes

The fuzz harness supports two execution modes controlled via the `FUZZ_MODE` environment variable:

### 1. Fast Mode (Default for PR CI)
Optimized for rapid feedback in standard pull request CI runs. Runs a compact set of randomized sequences with deterministic execution.

```bash
FUZZ_MODE=fast cargo test --package xelma-contract --lib tests::fuzz_lifecycle
```

### 2. Extended Mode (Nightly & Local Stress Testing)
Runs extended randomized action sequences (longer trace depth, higher case count) for deep state exploration. This mode runs automatically every night via [.github/workflows/nightly-fuzz-extended.yml](file:///C:/Users/SOSA/Downloads/od/Xelma-Blockchain/.github/workflows/nightly-fuzz-extended.yml) or on manual `workflow_dispatch`.

```bash
FUZZ_MODE=extended cargo test --package xelma-contract --lib tests::fuzz_lifecycle
```

---

## Failure Reproduction & Seed Replay

When an invariant violation occurs, the harness outputs structured failure diagnostics including the exact random seed, failing step index, violated invariant, and complete action sequence trace:

```text
================ PROPERTY FUZZ INVARIANT VIOLATION ================
Mode: fast
Seed: Some(1847291048291)
Failing Step Index: 12
Violated Invariant: Asset Conservation
Failed Action: PlaceBet { user_idx: 1, amount: 50000000, side: Up }
State Diagnostic:
Conservation Leak: accounted=50000001, total_minted=50000000
Action Trace History: [...]
===================================================================
```

To replay and debug a specific failing run deterministically, supply the reported `SEED` environment variable:

```bash
SEED=1847291048291 cargo test --package xelma-contract --lib tests::fuzz_lifecycle -- --nocapture
```

---

## Core Invariants Enforced

1. **Asset Conservation**:
   `total_user_balances + total_pending_winnings + protocol_fee_treasury + active_round_pot <= total_minted_tokens`
2. **Non-Negative Balances**:
   Every user's balance and pending winnings remain $\ge 0$.
3. **Treasury Fee Consistency**:
   Protocol fee treasury balance remains $\ge 0$ and consistent with protocol fee logic and withdrawals.
4. **Round Lifecycle Finality**:
   When no round is active, attempting to place a bet fails cleanly without mutating user balances or contract state.
5. **Claim Protection & Idempotency**:
   Calling `claim_winnings` for a user with 0 pending winnings yields `0` payout and leaves user balance unchanged.
6. **Early Cashout Consistency**:
   `cash_out_early` on an active round with a user position yields a payout consistent with the early cashout formula, decrements the active round pot accordingly, and leaves all balances non-negative. Calling `cash_out_early` when no round is active or the user has no position fails cleanly without mutating state.

---

## Extending the Harness

- **Adding New Actions**: Add a variant to `LifecycleAction` in `contracts/src/tests/fuzz_lifecycle.rs`, update `action_generator()`, and handle execution in `fuzz_protocol_lifecycle_invariants()`.
- **Early Cashout Action**: The `CashOutEarly` variant is generated in `action_generator()` and executed in `fuzz_protocol_lifecycle_invariants()`, exercising the early cashout path alongside place-bet, resolve, claim, and withdraw actions.
- **Adding New Invariants**: Implement assertion logic inside the action execution loop in `fuzz_protocol_lifecycle_invariants()`.
