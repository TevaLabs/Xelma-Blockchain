# Contract error registry

This is the canonical documented registry for every on-chain "ContractError".
The CI parity check compares this table with "contracts/src/errors.rs" and the
TypeScript "ContractError" map in "bindings/src/index.ts"; drift in any of the
three sources fails the bindings job.

When adding, removing, renaming, or renumbering an error:

1. Update "contracts/src/errors.rs".
2. Update "bindings/src/index.ts".
3. Update the table below.
4. Run "npm --prefix bindings run test:parity".

| Code | Variant |
|---:|---|
| 1 | "AlreadyInitialized" |
| 2 | "AdminNotSet" |
| 3 | "OracleNotSet" |
| 4 | "InvalidBetAmount" |
| 5 | "NoActiveRound" |
| 6 | "RoundEnded" |
| 7 | "InsufficientBalance" |
| 8 | "AlreadyBet" |
| 9 | "Overflow" |
| 10 | "InvalidPrice" |
| 11 | "InvalidDuration" |
| 12 | "InvalidMode" |
| 13 | "WrongModeForPrediction" |
| 14 | "RoundNotEnded" |
| 15 | "StaleOracleData" |
| 16 | "InvalidOracleRound" |
| 17 | "RoundAlreadyActive" |
| 18 | "ContractPaused" |
| 19 | "WindowOutOfRange" |
| 20 | "FutureOracleData" |
| 21 | "PayoutOverflow" |
| 22 | "RoundNotCancellable" |
| 23 | "StakeExceedsMax" |
| 24 | "ExposureCapExceeded" |
| 25 | "PendingWinningsCapExceeded" |
| 26 | "InvalidStartPrice" |
| 27 | "OracleNonceReused" |
| 28 | "InvalidMinParticipants" |
| 29 | "InvalidPrecisionCap" |
| 30 | "PrecisionCapExceeded" |
| 31 | "OracleDeviationExceeded" |
| 32 | "UnsupportedSchemaVersion" |
| 33 | "MigrationActiveRound" |
| 34 | "CommitmentNotFound" |
| 35 | "AlreadyRevealed" |
| 36 | "InvalidRevealWindow" |
| 37 | "HashMismatch" |
| 38 | "OracleNetworkMismatch" |
| 39 | "InvalidProtocolFeeBps" |
| 40 | "MintLimitExceeded" |
| 41 | "NoPendingRotation" |
| 42 | "RotationDelayNotElapsed" |
| 43 | "InvalidArchiveRetention" |
| 44 | "InvalidCommitment" |
| 45 | "InvalidSalt" |
| 46 | "NoRoundTemplate" |
| 47 | "OracleTimestampOutsideWindow" |
| 48 | "PendingWinningsNotExpired" |
| 49 | "EpochBudgetExceeded" |
| 50 | "OracleNotLive" |
| 51 | "InvalidPayoutPolicy" |
| 52 | "BelowMinBet" |
| 53 | "InsufficientOracleQuorum" |
| 54 | "TooFewObservations" |
| 55 | "OracleOutlierRejected" |
| 56 | "DuplicateOracleSource" |
| 57 | "InvalidObservationOrder" |
| 58 | "UnsupportedDataKeyForTtlTouch" |
| 59 | "PendingWinningsNotFound" |
| 60 | "ExpiryNotConfigured" |
| 61 | "AccessDenied" |
| 62 | "ProposalNotFound" |
| 63 | "ProposalExpired" |
| 64 | "GovInvalidState" |
| 65 | "GovUnauthorized" |
| 66 | "IllegalPhaseTransition" |
| 67 | "OracleHeartbeatUnhealthy" |
| 68 | "ClaimBatchTooLarge" |
| 69 | "DuplicateClaimAddress" |
| 70 | "EarlyCashoutDisabled" |
| 71 | "PositionNotFound" |
| 72 | "InvalidPhaseForCashout" |
| 73 | "WrongModeForCashout" |
| 74 | "InsuranceInvalidSplit" |
| 75 | "InsuranceInsufficientFund" |
| 76 | "InvalidAmount" |
| 77 | "DisputeWindowExpired" |
| 78 | "ClaimLocked" |
| 79 | "RoundStartLedgerReused" |
| 80 | "PageSizeExceeded" |

