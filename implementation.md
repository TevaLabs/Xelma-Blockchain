# Implementation Plan: Issues #876 and #867

This implementation plan covers:
- **Issue #876**: Assign unique stable numeric codes to every `ContractError` variant in `contracts/src/errors.rs` and sync the resulting changes to the TypeScript bindings.
- **Issue #867**: Document the bindings regeneration command clearly in `bindings/package.json` and `CONTRIBUTING.md`.

Per instructions, **no production contract logic or source files have been modified yet**. Git operations (branching, commits, pushes, merges) are handled by the repository maintainer. This plan is submitted for review and approval before proceeding with implementation.

---

## Section 1: Findings

### 1.1 Complete Audit of `ContractError` Variants in `contracts/src/errors.rs`

A full analysis of `contracts/src/errors.rs` reveals:
- **Total defined variants**: 81
- **Variants with numeric codes assigned**: 81 (100% of variants currently carry an explicit `#[contracterror]` `= <u32>` assignment)
- **Variants missing a code**: 0
- **Duplicate codes**: 0 (all 81 assigned codes are mutually unique)
- **Current highest code in use**: **102** (`BettingClosed = 102`)
- **Gaps in range 1..102**: **21 gaps** (`[4, 5, 17, 21, 26, 32, 34, 36, 37, 40, 43, 50, 52, 56, 57, 58, 59, 60, 61, 89, 90]`)
- **Source order anomalies**:
  - `PendingWinningsNotExpired = 86` is defined out-of-order at line 63 (between code 66 `OracleTimestampOutsideWindow` and code 67 `EpochBudgetExceeded`).
  - `DisputeWindowExpired = 91`, `ClaimLocked = 92`, `RoundStartLedgerReused = 93`, and `PageSizeExceeded = 94` are defined out-of-order at lines 128–139 (after code 102 `BettingClosed`).

#### Complete Variant Table (as defined in `contracts/src/errors.rs`)

| # | Variant Name | Current Code | Status / Conflict / Gap | Context & References |
|---|---|:---:|---|---|
| 1 | `AlreadyInitialized` | 1 | Stable | Baseline contract error |
| 2 | `AdminNotSet` | 2 | Stable | Initialization check |
| 3 | `OracleNotSet` | 3 | Stable | Initialization check |
| - | *(Gap 4)* | — | **Gap (4)** | Formerly `UnauthorizedAdmin` in legacy docs |
| - | *(Gap 5)* | — | **Gap (5)** | Formerly `UnauthorizedOracle` in legacy docs |
| 4 | `InvalidBetAmount` | 6 | Stable | Betting bounds |
| 5 | `NoActiveRound` | 7 | Stable | Round lifecycle |
| 6 | `RoundEnded` | 8 | Stable | Round lifecycle |
| 7 | `InsufficientBalance` | 9 | Stable | Asserted in `bindings/tests/integration.test.ts:192` |
| 8 | `AlreadyBet` | 10 | Stable | Anti-replay / participant guard |
| 9 | `Overflow` | 11 | Stable | Safe math guard |
| 10 | `InvalidPrice` | 12 | Stable | Price validation |
| 11 | `InvalidDuration` | 13 | Stable | Window validation |
| 12 | `InvalidMode` | 14 | Stable | Mode validation |
| 13 | `WrongModeForPrediction` | 15 | Stable | UpDown vs Precision mode check |
| 14 | `RoundNotEnded` | 16 | Stable | Settlement check |
| - | *(Gap 17)* | — | **Gap (17)** | Formerly `InvalidPriceScale` in legacy docs |
| 15 | `StaleOracleData` | 18 | Stable | Oracle freshness |
| 16 | `InvalidOracleRound` | 19 | Stable | Oracle round binding |
| 17 | `RoundAlreadyActive` | 20 | Stable | Lifecycle guard |
| - | *(Gap 21)* | — | **Gap (21)** | Formerly `AdminIsOracle` in legacy docs |
| 18 | `ContractPaused` | 22 | Stable | Emergency pause check |
| 19 | `WindowOutOfRange` | 23 | Stable | Window configuration bound |
| 20 | `FutureOracleData` | 24 | Stable | Oracle timestamp sanity check |
| 21 | `PayoutOverflow` | 25 | Stable | Settlement arithmetic guard |
| - | *(Gap 26)* | — | **Gap (26)** | Formerly `RoundCancelled` in legacy docs |
| 22 | `RoundNotCancellable` | 27 | Stable | Cancellation guard |
| 23 | `StakeExceedsMax` | 28 | Stable | Asserted in `bindings/tests/integration.test.ts:197` |
| 24 | `ExposureCapExceeded` | 29 | Stable | Asserted in `bindings/tests/integration.test.ts:207` |
| 25 | `PendingWinningsCapExceeded` | 30 | Stable | Economic risk control |
| 26 | `InvalidStartPrice` | 31 | Stable | Round creation price check |
| - | *(Gap 32)* | — | **Gap (32)** | Formerly `StartPriceTooHigh` in legacy docs |
| 27 | `OracleNonceReused` | 33 | Stable | Oracle replay defense |
| - | *(Gap 34)* | — | **Gap (34)** | Formerly `InsufficientParticipants` in legacy docs |
| 28 | `InvalidMinParticipants` | 35 | Stable | Resolution quorum check |
| - | *(Gaps 36, 37)* | — | **Gaps (36, 37)** | Historical gap |
| 29 | `InvalidPrecisionCap` | 38 | Stable | Precision mode config |
| 30 | `PrecisionCapExceeded` | 39 | Stable | Precision participation cap |
| - | *(Gap 40)* | — | **Gap (40)** | Historical gap |
| 31 | `OracleDeviationExceeded` | 41 | Stable | Circuit breaker |
| 32 | `UnsupportedSchemaVersion` | 42 | Stable | Storage migration guard |
| - | *(Gap 43)* | — | **Gap (43)** | Historical gap |
| 33 | `MigrationActiveRound` | 44 | Stable | Migration guard |
| 34 | `CommitmentNotFound` | 45 | Stable | Commit-reveal protocol |
| 35 | `AlreadyRevealed` | 46 | Stable | Commit-reveal protocol |
| 36 | `InvalidRevealWindow` | 47 | Stable | Commit-reveal protocol |
| 37 | `HashMismatch` | 48 | Stable | Commit-reveal verification |
| 38 | `OracleNetworkMismatch` | 49 | Stable | Multi-network payload isolation |
| - | *(Gap 50)* | — | **Gap (50)** | Historical gap |
| 39 | `InvalidProtocolFeeBps` | 51 | Stable | Fee configuration check |
| - | *(Gap 52)* | — | **Gap (52)** | Historical gap |
| 40 | `MintLimitExceeded` | 53 | Stable | Faucet rate-limit guard |
| 41 | `NoPendingRotation` | 54 | Stable | Admin/oracle rotation |
| 42 | `RotationDelayNotElapsed` | 55 | Stable | Timelock rotation delay |
| - | *(Gaps 56..61)* | — | **Gaps (56–61)** | Historical gap (6 unassigned codes) |
| 43 | `InvalidArchiveRetention` | 62 | Stable | Historical archive bounds |
| 44 | `InvalidCommitment` | 63 | Stable | Asserted in `commit_reveal_e2e.rs:760` (`== 63`) |
| 45 | `InvalidSalt` | 64 | Stable | Asserted in `commit_reveal_e2e.rs:802` (`== 64`) |
| 46 | `NoRoundTemplate` | 65 | Stable | Asserted in `bindings/tests/integration.test.ts:212` |
| 47 | `OracleTimestampOutsideWindow` | 66 | Stable | Economic window check |
| 48 | `PendingWinningsNotExpired` | **86** | **Out-of-order** | Defined between 66 and 67; code is 86 |
| 49 | `EpochBudgetExceeded` | 67 | Stable | Faucet global epoch limit |
| 50 | `OracleNotLive` | 68 | Stable | Oracle heartbeat liveliness |
| 51 | `InvalidPayoutPolicy` | 69 | Stable | Precision payout policy check |
| 52 | `BelowMinBet` | 70 | Stable | Dust protection |
| 53 | `InsufficientOracleQuorum` | 71 | Stable | Multi-feed quorum check |
| 54 | `TooFewObservations` | 72 | Stable | Multi-feed observation count |
| 55 | `OracleOutlierRejected` | 73 | Stable | Multi-feed median filtering |
| 56 | `DuplicateOracleSource` | 74 | Stable | Multi-feed source dedup |
| 57 | `InvalidObservationOrder` | 75 | Stable | Multi-feed ordering check |
| 58 | `UnsupportedDataKeyForTtlTouch` | 76 | Stable | Storage maintenance guard |
| 59 | `PendingWinningsNotFound` | 77 | Stable | Pending winnings claim |
| 60 | `ExpiryNotConfigured` | 78 | Stable | Pending winnings expiry check |
| 61 | `AccessDenied` | 79 | Stable | Asserted in `access_control.rs:58` (`== 79`) & `integration.test.ts:217` |
| 62 | `ProposalNotFound` | 80 | Stable | Governance proposal lookup |
| 63 | `ProposalExpired` | 81 | Stable | Governance timelock expiration |
| 64 | `GovInvalidState` | 82 | Stable | Governance state machine |
| 65 | `GovUnauthorized` | 83 | Stable | Governance access control |
| 66 | `IllegalPhaseTransition` | 84 | Stable | Round lifecycle transition guard |
| 67 | `OracleHeartbeatUnhealthy` | 85 | Stable | Oracle health score gate |
| 68 | `ClaimBatchTooLarge` | 87 | Stable | Batch claim size guard |
| 69 | `DuplicateClaimAddress` | 88 | Stable | Batch claim address dedup |
| - | *(Gaps 89, 90)* | — | **Gaps (89, 90)** | Historical gap |
| 70 | `EarlyCashoutDisabled` | 95 | Stable | Early cashout feature flag |
| 71 | `PositionNotFound` | 96 | Stable | Early cashout position lookup |
| 72 | `InvalidPhaseForCashout` | 97 | Stable | Early cashout lifecycle gate |
| 73 | `WrongModeForCashout` | 98 | Stable | UpDown-only cashout restriction |
| 74 | `InsuranceInvalidSplit` | 99 | Stable | Backstop split math |
| 75 | `InsuranceInsufficientFund` | 100 | Stable | Backstop liquidity check |
| 76 | `InvalidAmount` | 101 | Stable | Token transfer validation |
| 77 | `BettingClosed` | 102 | Stable | Highest assigned code; close buffer |
| 78 | `DisputeWindowExpired` | **91** | **Out-of-order** | Defined after 102; code is 91 |
| 79 | `ClaimLocked` | **92** | **Out-of-order** | Defined after 102; code is 92 |
| 80 | `RoundStartLedgerReused` | **93** | **Out-of-order** | Defined after 102; code is 93 |
| 81 | `PageSizeExceeded` | **94** | **Out-of-order** | Defined after 102; code is 94 |

---

### 1.2 Downstream Dependencies Referencing Numeric Error Codes

Renumbering or changing any existing code would break multiple active components:

1. **Rust Test Invariant Assertions**:
   - `contracts/src/tests/access_control.rs:58`:
     ```rust
     assert_eq!(ContractError::AccessDenied as u32, 79);
     ```
   - `contracts/src/tests/commit_reveal_e2e.rs:760`:
     ```rust
     assert_eq!(ContractError::InvalidCommitment as u32, 63);
     ```
   - `contracts/src/tests/commit_reveal_e2e.rs:802`:
     ```rust
     assert_eq!(ContractError::InvalidSalt as u32, 64);
     ```
   - `contracts/src/tests/initialization.rs:199, 284` & `contracts/src/tests/adversarial/sybil.rs:28, 62`:
     Use `soroban_sdk::Error::from_contract_error(ContractError::<Variant> as u32)`.

2. **TypeScript Integration Tests (`bindings/tests/integration.test.ts`)**:
   - Line 192: `wrapContractError({ code: 9, message: "n/a" })` asserts `InsufficientBalanceError`.
   - Line 197: `wrapContractError(new Error("contract error code 28"))` asserts `StakeExceedsMaxError`.
   - Line 207: `wrapContractError({ code: 29 })` asserts `ExposureCapExceededError`.
   - Line 212: `wrapContractError({ code: 65 })` asserts `NoRoundTemplateError`.
   - Line 217: `wrapContractError({ code: 79 })` asserts `AccessDeniedError`.

3. **TypeScript Generated Bindings (`bindings/src/index.ts`)**:
   - Contains `export const ContractError = { ... }` with all 81 numeric keys matching Rust codes 1..102.
   - Contains `decodeContractError(code: number)` and `formatContractError(code: number)`.

4. **Automated Error Parity Checks (`bindings/tests/contract-error-parity.test.js` & `bindings/src/parity.js`)**:
   - Both enforce strict tri-directional parity between `contracts/src/errors.rs`, `bindings/src/index.ts`, and `docs/CONTRACT_ERRORS.md`.
   - Any code mismatch or missing variant fails the test suite.

5. **Canonical Documentation (`docs/CONTRACT_ERRORS.md`)**:
   - Markdown table documenting all 81 codes.

6. **Repository Compatibility Policy (`COMPATIBILITY_POLICY.md`)**:
   - Line 44 explicitly states:
     ```markdown
     | Renumber a ContractError variant | Yes — MAJOR |
     ```
     Under repository policy, changing any existing assigned code is a **MAJOR breaking change** requiring a semver bump and migration notes.

---

### 1.3 Bindings Regeneration Script Status

1. **In `bindings/package.json`**:
   - Current scripts:
     ```json
     "scripts": {
       "build": "tsc",
       "lint": "tsc --noEmit",
       "test:parity": "node src/parity.js",
       "test:errors": "vitest run tests/contract-error-parity.test.js",
       "test:wasm": "vitest run tests/wasm-parity.test.js",
       "test:vitest": "vitest run",
       "test:integration": "vitest run tests/integration.test.ts",
       "prepublishOnly": "npm run build && npm run test:parity && npm run test:errors"
     }
     ```
   - **Finding**: No script named `generate`, `regen`, or similar exists in `bindings/package.json`.

2. **In other documentation**:
   - `README.md` (lines 298–306, 785–792, 825–831) and `docs/CONTRIBUTOR_MAP.md` document the generation command as:
     ```bash
     stellar contract bindings typescript \
       --wasm target/wasm32v1-none/release/xelma_contract.wasm \
       --output-dir ./bindings/src \
       --overwrite
     ```
   - `bindings/README.md` still contains outdated Soroban CLI template placeholders:
     `soroban contract bindings ts --rpc-url INSERT_RPC_URL_HERE ...`

---

### 1.4 `CONTRIBUTING.md` Documentation Status

- `CONTRIBUTING.md` (lines 67–72) currently documents:
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --all -- --check`
  - `cd bindings && npm ci && npm run build`
  - `cd bindings && npm run test:parity`
- **Finding**: `CONTRIBUTING.md` does **not** document `stellar contract bindings typescript` or any bindings regeneration command.

---

### 1.5 CI Workflow Analysis and Job Counts

Examination of all 6 workflow files in `.github/workflows/`:
1. `changelog.yml`: Triggers on `workflow_dispatch` only (0 PR jobs).
2. `deploy_testnet.yml`: Triggers on `workflow_dispatch` only (0 PR jobs).
3. `nightly-adversarial.yml`: Triggers on cron schedule and `workflow_dispatch` (0 PR jobs).
4. `nightly-fuzz-extended.yml`: Triggers on cron schedule and `workflow_dispatch` (0 PR jobs).
5. `release-bindings.yml`: Triggers on tag push (`v*`) and `workflow_dispatch` (0 PR jobs).
6. `ci.yml`: Triggers on `push: branches: [main, develop]` and `pull_request: branches: ["**"]`.
   - **PR Job Count**: **13 jobs** (`rust-test`, `contract-build`, `bindings-test`, `bindings-build`, `parity-check`, `bindings-vitest`, `e2e-smoke`, `coverage`, `format-check`, `license-header-check`, `security-audit`, `wasm-size-gate`, `ci-success`).
   - Note: In commit `348bc1d`, an additional job `error-code-parity` was drafted, but the file was corrupted in that commit.
- **Job Cap**: No numeric CI job cap is documented or implied in `CONTRIBUTING.md`, `.github/workflows/`, or any repository governance document.

---

### 1.6 Ambiguities and Blockers Identified

1. **State of `.github/workflows/ci.yml`**:
   Commit `348bc1d` accidentally replaced the contents of `.github/workflows/ci.yml` with a single, mangled base64-encoded line. Scope rules forbid adding CI jobs or editing files outside the issue scope without maintainer direction.
2. **Regex defect in `bindings/src/parity.js`**:
   Line 30 of `bindings/src/parity.js` contains a regex typo introduced in commit `348bc1d`:
   `line.match(/^\s*pubs\+fn\s+([a-zA-Z0-9_]+)\s*\(/)`
   Because of `pubs\+fn` instead of `pub\s+fn`, `node src/parity.js` fails with `Failed to parse contract functions`.
3. **Missing script `bindings/scripts/check-error-parity.mjs`**:
   Mentioned in `bindings/README.md` and the draft `error-code-parity` CI step, but never committed to the repo. Parity is checked by `bindings/tests/contract-error-parity.test.js`.

---

## Section 2: Proposed Implementation Plan for #876

### 2.1 Code Assignment Strategy

Because every `ContractError` variant already has an assigned numeric code (1..102) and `COMPATIBILITY_POLICY.md` designates renumbering as a **MAJOR breaking change**, we consider two strategies:

#### Option 1 (Recommended — Stable Preservation & Sequence Sorting)
- **Preserve all existing numeric codes (1..102)**.
- Reorganize `contracts/src/errors.rs` into strict ascending numerical order:
  - Move `PendingWinningsNotExpired = 86` between `OracleHeartbeatUnhealthy = 85` and `ClaimBatchTooLarge = 87`.
  - Move `DisputeWindowExpired = 91`, `ClaimLocked = 92`, `RoundStartLedgerReused = 93`, `PageSizeExceeded = 94` between `DuplicateClaimAddress = 88` and `EarlyCashoutDisabled = 95`.
- Sync the resulting sorted sequence into `bindings/src/index.ts` and `docs/CONTRACT_ERRORS.md`.
- **Blast Radius**: 0 breaking changes; 0 broken tests; backwards-compatible (MINOR under `COMPATIBILITY_POLICY.md`).

#### Option 2 (Alternative — Compact Sequential Renumbering 1..81)
If the project maintainers explicitly wish to eliminate all 21 gaps and compact codes to 1..81:
- Every variant from code 6 onwards shifts down.
- **Affected variants**: 78 variants renumbered.
- **Breaking changes**:
  - `contracts/src/tests/access_control.rs`: `AccessDenied` changes from 79 to 61 (breaks line 58).
  - `contracts/src/tests/commit_reveal_e2e.rs`: `InvalidCommitment` changes from 63 to 44 (breaks line 760); `InvalidSalt` changes from 64 to 45 (breaks line 802).
  - `bindings/tests/integration.test.ts`: breaks lines 192 (9->7), 197 (28->23), 207 (29->24), 212 (65->46), 217 (79->61).
  - All external consumers, wallet guides, indexers, and test snapshots will break.
  - Requires bumping `Cargo.toml` and updating `MIGRATION.md` for a MAJOR release.

> **Recommendation**: Implement **Option 1**. It achieves the goal of stable, unique numeric codes for all variants without introducing unnecessary breaking changes or violating the repository's semver compatibility policy.

### 2.2 Exact Proposed Variant Order for Option 1 (`contracts/src/errors.rs`)

```rust
// contracts/src/errors.rs
pub enum ContractError {
    AlreadyInitialized = 1,
    AdminNotSet = 2,
    OracleNotSet = 3,
    InvalidBetAmount = 6,
    NoActiveRound = 7,
    RoundEnded = 8,
    InsufficientBalance = 9,
    AlreadyBet = 10,
    Overflow = 11,
    InvalidPrice = 12,
    InvalidDuration = 13,
    InvalidMode = 14,
    WrongModeForPrediction = 15,
    RoundNotEnded = 16,
    StaleOracleData = 18,
    InvalidOracleRound = 19,
    RoundAlreadyActive = 20,
    ContractPaused = 22,
    WindowOutOfRange = 23,
    FutureOracleData = 24,
    PayoutOverflow = 25,
    RoundNotCancellable = 27,
    StakeExceedsMax = 28,
    ExposureCapExceeded = 29,
    PendingWinningsCapExceeded = 30,
    InvalidStartPrice = 31,
    OracleNonceReused = 33,
    InvalidMinParticipants = 35,
    InvalidPrecisionCap = 38,
    PrecisionCapExceeded = 39,
    OracleDeviationExceeded = 41,
    UnsupportedSchemaVersion = 42,
    MigrationActiveRound = 44,
    CommitmentNotFound = 45,
    AlreadyRevealed = 46,
    InvalidRevealWindow = 47,
    HashMismatch = 48,
    OracleNetworkMismatch = 49,
    InvalidProtocolFeeBps = 51,
    MintLimitExceeded = 53,
    NoPendingRotation = 54,
    RotationDelayNotElapsed = 55,
    InvalidArchiveRetention = 62,
    InvalidCommitment = 63,
    InvalidSalt = 64,
    NoRoundTemplate = 65,
    OracleTimestampOutsideWindow = 66,
    EpochBudgetExceeded = 67,
    OracleNotLive = 68,
    InvalidPayoutPolicy = 69,
    BelowMinBet = 70,
    InsufficientOracleQuorum = 71,
    TooFewObservations = 72,
    OracleOutlierRejected = 73,
    DuplicateOracleSource = 74,
    InvalidObservationOrder = 75,
    UnsupportedDataKeyForTtlTouch = 76,
    PendingWinningsNotFound = 77,
    ExpiryNotConfigured = 78,
    AccessDenied = 79,
    ProposalNotFound = 80,
    ProposalExpired = 81,
    GovInvalidState = 82,
    GovUnauthorized = 83,
    IllegalPhaseTransition = 84,
    OracleHeartbeatUnhealthy = 85,
    PendingWinningsNotExpired = 86,  // Moved to numeric order
    ClaimBatchTooLarge = 87,
    DuplicateClaimAddress = 88,
    DisputeWindowExpired = 91,      // Moved to numeric order
    ClaimLocked = 92,               // Moved to numeric order
    RoundStartLedgerReused = 93,    // Moved to numeric order
    PageSizeExceeded = 94,          // Moved to numeric order
    EarlyCashoutDisabled = 95,
    PositionNotFound = 96,
    InvalidPhaseForCashout = 97,
    WrongModeForCashout = 98,
    InsuranceInvalidSplit = 99,
    InsuranceInsufficientFund = 100,
    InvalidAmount = 101,
    BettingClosed = 102,
}
```

### 2.3 Bindings Synchronization and Verification

1. Sync `bindings/src/index.ts` so `export const ContractError` lists entries in matching ascending code order.
2. Sync `docs/CONTRACT_ERRORS.md` table to reflect matching sorted entries.
3. Run `npm --prefix bindings run test:errors` to verify tri-source parity.

### 2.4 Cargo Test Scope

- Focused scope for error assertions:
  ```bash
  cargo test --package xelma-contract --lib tests::access_control:: --locked
  cargo test --package xelma-contract --lib tests::commit_reveal_e2e::test_commit_reveal_e2e_zero_commitment_hash_is_rejected --locked
  cargo test --package xelma-contract --lib tests::commit_reveal_e2e::test_commit_reveal_e2e_weak_salt_is_rejected --locked
  ```

---

## Section 3: Proposed Implementation Plan for #867

### 3.1 Changes to `bindings/package.json`

Add a new script `regen` under `"scripts"`:

```json
{
  "scripts": {
    "build": "tsc",
    "lint": "tsc --noEmit",
    "regen": "stellar contract bindings typescript --wasm ../target/wasm32v1-none/release/xelma_contract.wasm --output-dir ./src --overwrite",
    "test:parity": "node src/parity.js",
    "test:errors": "vitest run tests/contract-error-parity.test.js",
    "test:wasm": "vitest run tests/wasm-parity.test.js",
    "test:vitest": "vitest run",
    "test:integration": "vitest run tests/integration.test.ts",
    "prepublishOnly": "npm run build && npm run test:parity && npm run test:errors"
  }
}
```

- **Command**: `stellar contract bindings typescript --wasm ../target/wasm32v1-none/release/xelma_contract.wasm --output-dir ./src --overwrite`
- **Behavior**: Uses the official Stellar CLI to generate TypeScript bindings from the release WASM artifact into `bindings/src/`, overwriting existing generated files.

### 3.2 Additions to `CONTRIBUTING.md`

Add a dedicated subsection under `## Development Setup` in `CONTRIBUTING.md`:

```markdown
### Regenerating TypeScript Bindings

When contract functions, storage types, or `ContractError` variants are modified, the TypeScript bindings must be regenerated from the compiled WASM:

1. Build the release contract WASM:
   ```bash
   cargo rustc --manifest-path contracts/Cargo.toml --crate-type=cdylib --target=wasm32v1-none --release --locked
   ```
   *(or `stellar contract build --package xelma-contract`)*

2. Regenerate TypeScript bindings:
   ```bash
   cd bindings
   npm run regen
   ```
   *This executes `stellar contract bindings typescript` pointing to `target/wasm32v1-none/release/xelma_contract.wasm`.*

3. Validate bindings parity and build:
   ```bash
   npm run build
   npm run test:errors
   npm run test:parity
   ```

4. **Commit guidelines**: Commit `bindings/src/index.ts` and documentation updates. Never commit `bindings/dist/` or other build artifacts.
```

### 3.3 Documentation Updates

- **`bindings/README.md`**: Update the generation instructions to reference `npm run regen` and `stellar contract bindings typescript` instead of the legacy Soroban CLI template.
- **`docs/CONTRACT_ERRORS.md`**: Ensure numerical ordering matches `contracts/src/errors.rs`.

---

## Section 4: Verification Plan

### 4.1 Verification Commands for #876

```bash
# 1. Verify Rust error definitions compile and targeted tests pass
cargo test --package xelma-contract --lib tests::access_control::test_admin_only_mutations_enforced --locked
cargo test --package xelma-contract --lib tests::commit_reveal_e2e::test_commit_reveal_e2e_zero_commitment_hash_is_rejected --locked
cargo test --package xelma-contract --lib tests::commit_reveal_e2e::test_commit_reveal_e2e_weak_salt_is_rejected --locked

# 2. Verify TypeScript error parity test passes (Rust <-> TS <-> Docs)
npm --prefix bindings run test:errors

# 3. Verify TypeScript integration tests
npm --prefix bindings run test:integration

# 4. Verify TypeScript builds without type errors
npm --prefix bindings run build
npm --prefix bindings run lint
```

### 4.2 Verification Commands for #867

```bash
# 1. Verify npm run regen is defined in bindings/package.json
npm --prefix bindings run regen -- --help

# 2. Verify CONTRIBUTING.md contains the exact regen command and instructions
git diff CONTRIBUTING.md
```

### 4.3 Cleanliness and Scope Verification

```bash
# Verify only expected files were modified
git status --short
git diff --stat
```
Expected modified files:
- `contracts/src/errors.rs`
- `bindings/src/index.ts`
- `bindings/package.json`
- `CONTRIBUTING.md`
- `bindings/README.md`
- `docs/CONTRACT_ERRORS.md`

No production contract logic, no unrelated tests, and no keys or addresses are touched.

---

## Section 5: Open Questions

1. **Confirmation of Code Strategy (Option 1 vs Option 2)**:
   Does the team approve **Option 1** (preserve existing assigned codes 1..102, reordering declarations in source for clarity and backwards compatibility)? Or is a breaking compaction to 1..81 preferred despite requiring a MAJOR version bump?
2. **Defect in `bindings/src/parity.js`**:
   `bindings/src/parity.js` line 30 contains a typo (`pubs\+fn` instead of `pub\s+fn`) introduced in commit `348bc1d` that causes `npm run test:parity` to fail when parsing `contracts/src/contract.rs`. Should we include the 2-character fix in `bindings/src/parity.js` as part of syncing bindings?
3. **State of `.github/workflows/ci.yml`**:
   Commit `348bc1d` left `.github/workflows/ci.yml` base64-corrupted on `main`. Per instructions ("Do not add new CI jobs unless the issue explicitly allows it"), this plan leaves CI untouched. Please confirm if CI repairs should be handled in a dedicated PR.
4. **Script Naming in `bindings/package.json`**:
   We propose `"regen"`. Should we also include `"generate"` as an alias?
