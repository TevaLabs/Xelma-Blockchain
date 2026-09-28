# Differential Verification Harness — settlement_math

**Issues:** [#520](https://github.com/TevaLabs/Xelma-Blockchain/issues/520) /
[#362](https://github.com/TevaLabs/Xelma-Blockchain/issues/362)
**Harness:** `contracts/src/tests/diff_verify.rs`
**CI:** `.github/workflows/diff-verify.yml`

## What it does

The harness executes a **trusted Rust reference model** and the **contract
under test** on identical randomized oracle cases and asserts **bitwise
(stroop-level) equality** on every outcome. A single mismatched stroop
fails the run with the seed, a full case dump, a minimized failing case,
and an exact reproduction command.

This is infrastructure-grade assurance: a systematic bug that silently
drops or fabricates value (fee rounding, remainder assignment, refund
paths, WASM codegen drift) cannot pass.

## Tiers

| Tier | Subject under test | Comparator | Catches |
|------|--------------------|------------|---------|
| **A** | `settlement_math` pure engine functions | `ref_a_*` reference functions | Arithmetic regressions in the audited pure engine (fee splits, payout floors, winner selection, scoring, deviation bps) |
| **B** | Full native contract settlement (Soroban testutils) | `ref_prod_*` production reference model | Settlement-orchestration regressions: fee incidence models (`FeeOnPot` / `FeeOnWinnings`), participant ordering, unrevealed-forfeit policy, tie remainders |
| **C** | The **compiled WASM binary** (`WASM_PATH`) | Tier B native run + reference model | Divergence introduced by the `wasm32v1-none` release pipeline itself (optimizations, target codegen) — the same WASM shape that gets deployed on-chain |

Tier C registers the *actual built WASM* in the Soroban test environment
(`Env::register(<wasm bytes>)`), drives the canonical round through it,
and requires stroop-identical results versus both the native build and
the reference model. If the release WASM ever behaves differently from
the source-level contract, this tier fails.

## Case generator

A single canonical generator (`generate_cases`) produces every case,
deterministically from `(base_seed, case_index)` via an LCG-mixed
`StdRng` stream. Cases randomize:

- start / final oracle prices (incl. ~20% unchanged-price tie cases)
- protocol fee: none, 1 bp, 250 bps, 1 000 bps (the 10% maximum)
- fee incidence model: `FeeOnPot` (default) / `FeeOnWinnings`
- Precision payout policy: Equal / StakeWeighted
- 2–8 Up/Down participants (incl. thin losing pools for fee spillover)
- 1–8 Precision entries (incl. ~25% commit-only/unrevealed entries so
  the anti-griefing forfeit and all-unrevealed refund branches run)
- deviation-bps reference price

Stakes are sized to pass every on-chain validation gate (min-bet unset,
max-stake unset, mint balance ample).

Fixed regression cases (`fixed_cases`) always run in every mode and pin
historically tricky branches: thin-pool fee spillover, tie refund with
fee configured, one-sided refunds, `FeeOnWinnings`, precision ties with
remainder assignment, stake-weighted splits, all-unrevealed refund,
mixed-reveal forfeit, and the 500-bps deviation boundary.

## Failure diagnostics

On a mismatch the harness prints:

1. Tier, case index, and per-case seed
2. Field that mismatched, got vs expected values
3. Full case dump (all positions/predictions/fees) so the case can be
   turned into a fixed regression entry
4. A minimized failing case (tier A): prices halved, fee removed, price
   tied, positions truncated — each step kept only if the mismatch still
   reproduces
5. An exact reproduction command:

```text
SEED=<reported_seed> cargo test --package xelma-contract --lib tests::diff_verify -- --nocapture
```

## Execution modes

| Mode | Tier A cases | Tier B cases | Typical runtime | Where it runs |
|------|--------------|--------------|-----------------|---------------|
| `fast` (default) | 400 | 100 | seconds | every PR via the `diff-verify` workflow |
| `extended` | 2 000 | 1 000 | ~1 minute | nightly schedule (05:00 UTC) or manual `workflow_dispatch` |

Tier C runs on every PR after the release WASM is built in-workflow.

CI pins the seed `3735928559` (0xDEADBEEF) so PR failures are
deterministic and shareable; the nightly sweep rotates the seed (run ID)
to widen coverage over time.

## Local usage

```bash
# Fast mode (default), fixed seed
cargo test --package xelma-contract --lib tests::diff_verify -- --nocapture

# Extended mode (≥ 1,000 randomized cases, per issue #362 acceptance criteria)
DIFF_VERIFY_MODE=extended cargo test --package xelma-contract --lib \
  tests::diff_verify -- --nocapture

# Reproduce a reported failure exactly
SEED=<reported_seed> cargo test --package xelma-contract --lib \
  tests::diff_verify -- --nocapture

# Tier C — differential against the compiled WASM binary
cargo rustc --manifest-path=contracts/Cargo.toml --crate-type=cdylib \
  --target=wasm32v1-none --release --locked
WASM_PATH=target/wasm32v1-none/release/xelma_contract.wasm \
  cargo test --package xelma-contract --lib \
  tests::diff_verify::differential_tier_c_wasm_binary -- --nocapture
```

## Notes for maintainers

- The harness is **verification tooling only**: no contract behaviour,
  storage layout, or public API is changed.
- Tier B/C compares per-participant outcomes read from the archive API
  (`get_user_archived_participation`), keyed by `(round_id, user)`, so
  the comparison is strict per stake regardless of iteration order.
- The reference model deliberately mirrors *production* settlement
  semantics (`settlement.rs`), which differ from the pure engine in fee
  incidence and participant ordering; do not "simplify" it back to
  engine semantics.
- If a new settlement branch is added (e.g. a new fee model), extend
  `ref_prod_*` and the fixed regression list in the same PR.
