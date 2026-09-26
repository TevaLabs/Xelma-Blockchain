# Contract error registry

This is the canonical documented registry for every on-chain `ContractError`.
The CI parity check compares this table with `contracts/src/errors.rs` and the
TypeScript `ContractError` map in `bindings/src/index.ts`; drift in any of the
three sources fails the bindings job.

When adding, removing, renaming, or renumbering an error:

1. Update `contracts/src/errors.rs`.
2. Update `bindings/src/index.ts`.
3. Update the table below.
4. Run `npm --prefix bindings run test:parity`.

| Code | Variant |
|---:|---|
| 1 | `AlreadyInitialized` |
| 2 | `AdminNotSet` |
| 3 | `OracleNotSet` |
| 6 | `InvalidBetAmount` |
| 7 | `NoActiveRound` |
| 8 | `RoundEnded` |
| 9 | `InsufficientBalance` |
| 10 | `AlreadyBet` |
| 11 | `Overflow` |
| 12 | `InvalidPrice` |
| 13 | `InvalidDuration` |
| 14 | `InvalidMode` |
| 15 | `WrongModeForPrediction` |
| 16 | `RoundNotEnded` |
| 18 | `StaleOracleData` |
| 19 | `InvalidOracleRound` |
| 20 | `RoundAlreadyActive` |
| 22 | `ContractPaused` |
| 23 | `WindowOutOfRange` |
| 24 | `FutureOracleData` |
| 25 | `PayoutOverflow` |
| 27 | `RoundNotCancellable` |
| 28 | `StakeExceedsMax` |
| 29 | `ExposureCapExceeded` |
| 30 | `PendingWinningsCapExceeded` |
| 31 | `InvalidStartPrice` |
| 33 | `OracleNonceReused` |
| 35 | `InvalidMinParticipants` |
| 38 | `InvalidPrecisionCap` |
| 39 | `PrecisionCapExceeded` |
| 41 | `OracleDeviationExceeded` |
| 42 | `UnsupportedSchemaVersion` |
| 44 | `MigrationActiveRound` |
| 45 | `CommitmentNotFound` |
| 46 | `AlreadyRevealed` |
| 47 | `InvalidRevealWindow` |
| 48 | `HashMismatch` |
| 49 | `OracleNetworkMismatch` |
| 51 | `InvalidProtocolFeeBps` |
| 53 | `MintLimitExceeded` |
| 54 | `NoPendingRotation` |
| 55 | `RotationDelayNotElapsed` |
| 62 | `InvalidArchiveRetention` |
| 63 | `InvalidCommitment` |
| 64 | `InvalidSalt` |
| 65 | `NoRoundTemplate` |
| 66 | `OracleTimestampOutsideWindow` |
| 67 | `EpochBudgetExceeded` |
| 68 | `OracleNotLive` |
| 69 | `InvalidPayoutPolicy` |
| 70 | `BelowMinBet` |
| 71 | `InsufficientOracleQuorum` |
| 72 | `TooFewObservations` |
| 73 | `OracleOutlierRejected` |
| 74 | `DuplicateOracleSource` |
| 75 | `InvalidObservationOrder` |
| 76 | `UnsupportedDataKeyForTtlTouch` |
| 77 | `PendingWinningsNotFound` |
| 78 | `ExpiryNotConfigured` |
| 79 | `AccessDenied` |
| 80 | `ProposalNotFound` |
| 81 | `ProposalExpired` |
| 82 | `GovInvalidState` |
| 83 | `GovUnauthorized` |
| 84 | `IllegalPhaseTransition` |
| 85 | `OracleHeartbeatUnhealthy` |
| 86 | `PendingWinningsNotExpired` |
| 87 | `ClaimBatchTooLarge` |
| 88 | `DuplicateClaimAddress` |
| 91 | `DisputeWindowExpired` |
| 92 | `ClaimLocked` |
| 93 | `RoundStartLedgerReused` |
| 94 | `PageSizeExceeded` |
| 95 | `EarlyCashoutDisabled` |
| 96 | `PositionNotFound` |
| 97 | `InvalidPhaseForCashout` |
| 98 | `WrongModeForCashout` |
| 99 | `InsuranceInvalidSplit` |
| 100 | `InsuranceInsufficientFund` |
| 101 | `InvalidAmount` |

## Overflow vs. PayoutOverflow

Two variants both signal arithmetic overflow, but they mean different things to a
wallet or indexer and must not be conflated:

| Code | Variant | Meaning | Retry? |
|---:|---|---|---|
| 11 | `Overflow` | **Non-financial** arithmetic overflow: round-ID counters, ledger/timing arithmetic, median/price math, outlier rejection, season keys. The operation is not a payout. | Usually not applicable to the failing call; re-check inputs. |
| 25 | `PayoutOverflow` | **Payout/fee arithmetic** overflow: pot accumulation, fee multiplication, stake-weighted split, or indexed Precision fee calculation. A settlement that hits this aborts atomically — no partial payouts are written and the round is left untouched. | Safe to retry `resolve_round` later if the underlying pot/fee inputs change; the contract state is unchanged. |

`PayoutOverflow` is the code a wallet should key off to recognize a
settlement-payout failure (Issue #405, #556). All payout aggregation routes
through `payout_add` / `payout_mul` (`contract.rs:1460-1467`) so it maps to
`PayoutOverflow`; the indexed Precision fee path
(`config::calculate_protocol_fee_precision`) does the same. `Overflow` is
reserved for everything else.
