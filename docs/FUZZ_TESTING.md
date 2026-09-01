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

---

## Extending the Harness

- **Adding New Actions**: Add a variant to `LifecycleAction` in `contracts/src/tests/fuzz_lifecycle.rs`, update `action_generator()`, and handle execution in `fuzz_protocol_lifecycle_invariants()`.
- **Adding New Invariants**: Implement assertion logic inside the action execution loop in `fuzz_protocol_lifecycle_invariants()`.

---

# Differential Verification — settlement_math

This document details the formal differential verification harness (`contracts/src/tests/diff_verify.rs`) — Issue #362.

The harness executes a **trusted Rust reference model** (standalone re-implementation of settlement math) and the **contract's compiled `settlement_math` functions** on identical randomized oracle cases, asserting bitwise (stroop-level) equality for every output.

## Architecture

```text
┌──────────────────────────────────────┐
│  CaseGenerator (deterministic RNG)   │
│  prices, stakes, fees, modes, seeds  │
└───────────────┬──────────────────────┘
                │
   ┌────────────┴────────────┐
   ▼                         ▼
┌────────────────┐  ┌────────────────────────┐
│ Reference Model │  │ Contract settlement_math│
│ (pure reimpl.)  │  │ (imported functions)   │
└───────┬────────┘  └──────────┬─────────────┘
        │                      │
        └──────────┬───────────┘
                   ▼
        ┌──────────────────┐
        │  Stroop-equality │
        │  assertion       │
        └──────────────────┘
```

The reference model (`§ 2` in `diff_verify.rs`) is a **fresh re-implementation** of every `settlement_math` function, deliberately written in a different style to minimize shared systematic bugs.  The two implementations use the same types and return values so the harness can compare bit-for-bit.

## Execution Modes

Controlled by the `DIFF_VERIFY_MODE` env var:

| Mode       | Cases | Description                                      |
|------------|-------|--------------------------------------------------|
| `fast`     | 100   | Default for PR CI — finishes in < 5 s            |
| `extended` | 1 000 | Nightly / local stress — ≥ 1 000 randomized cases |

```bash
# Fast mode (default for PRs)
DIFF_VERIFY_MODE=fast cargo test --package xelma-contract --lib \
  tests::diff_verify::differential_verify_fuzz -- --nocapture

# Extended mode (nightly / local stress)
DIFF_VERIFY_MODE=extended cargo test --package xelma-contract --lib \
  tests::diff_verify::differential_verify_fuzz -- --nocapture
```

## Coverage Matrix

The harness covers every settlement_math function across all scenarios:

| Function                              | Scenarios Tested                                        |
|---------------------------------------|---------------------------------------------------------|
| `classify_price_direction`            | Up, Down, Unchanged                                     |
| `is_one_sided_pool`                   | Both empty, one empty, neither empty                    |
| `compute_updown_fee`                  | No fee, 0.01%, 2.5%, 10%, thin losing pool spillover   |
| `compute_precision_fee`               | No fee, zero pot, large pot, edge values                |
| `compute_updown_payouts`              | Price up/down/unchanged, 1–8 winners, one-sided refund |
| `find_precision_winners` / `_with_policy` | AbsoluteDistance, RelativeDistance, confidence band |
| `compute_precision_payouts`           | Equal/StakeWeighted payout, ties, unrevealed entries   |
| `split_pot_among_winners`             | Even, remainder to first, zero pot                     |
| `compute_deviation_bps`               | 0%, 5%, 10%+ deviation, boundary values                |
| `total_pot_updown` / `total_pot_precision` | Sum verification across random inputs             |

## Fixed Regression Cases

Thirteen manually crafted edge cases target historically tricky code paths:

1. Thin losing pool fee spillover
2. Tie refund with fee configured
3. One-sided pool refund
4. All-unrevealed precision refund
5. Mixed reveal + fee
6. Stake-weighted precision tie
7. Relative distance scoring
8. Confidence band multiple winners
9. Max fee (10%)
10. Single participant UpDown win
11. Large stakes near overflow
12. Five-way precision tie remainder
13. Deviation bps boundary

These always run (regardless of mode) via `differential_verify_fixed_regression`.

## Seed Reproduction

Every case carries a deterministic seed derived from `(base_seed, case_idx)`.  On failure, the harness prints the exact seed and a minimal reproduction command:

```text
SEED=1234567890 cargo test --package xelma-contract --lib \
  tests::diff_verify::differential_verify_fuzz -- --nocapture
```

To replay a specific CI failure, set `SEED` and `DIFF_VERIFY_MODE` to the reported values.

## Automatic Case Minimisation

When a mismatch is found, the harness attempts to narrow the input to the smallest case that still reproduces the failure:

1. **Participant removal**: removes each participant one at a time, preserving the mismatch
2. **Stake halving**: reduces all stakes while keeping direction
3. **Fee removal**: tests whether the mismatch persists without fees
4. **Tie forcing**: sets `final_price = start_price` to test tie paths

The minimised case is printed alongside the original failure for easy debugging.

## CI Integration

The differential verification runs automatically via `.github/workflows/diff-verify.yml`:

- **PR CI**: Fixed regression + 100 fast-mode cases (on push/PR touching `settlement_math.rs` or `diff_verify.rs`)
- **Nightly**: Fixed regression + 1 000 extended-mode cases
- **Manual dispatch**: selectable mode and seed

## Contributor Workflow

When modifying `settlement_math.rs`:

1. **Run the diff verify harness**:
   ```sh
   cargo test --package xelma-contract --lib \
     tests::diff_verify -- --nocapture
   ```

2. **If a mismatch is found**, the harness reports:
   - The failing seed and case index
   - The exact field where contract and reference diverge
   - A minimised reproduction case
   - A command to replay

3. **If the behavior change is intentional** (new feature, not a regression):
   - Update the reference model in `diff_verify.rs` to match the new behavior
   - Add a new fixed regression case for the changed path
   - Run the harness again to confirm

4. **If the behavior is a regression**: fix `settlement_math.rs` and re-run

5. **For nightly coverage**, use extended mode:
   ```sh
   DIFF_VERIFY_MODE=extended cargo test --package xelma-contract --lib \
     tests::diff_verify -- --nocapture
   ```

6. **To reproduce a CI failure**, use the exact seed and mode reported:
   ```sh
   SEED=<reported_seed> DIFF_VERIFY_MODE=<reported_mode> cargo test \
     --package xelma-contract --lib tests::diff_verify -- --nocapture
   ```
