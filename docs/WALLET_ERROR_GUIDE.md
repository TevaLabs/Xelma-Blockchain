# Wallet Error Integration Guide

This guide maps every live smart-contract error defined in `contracts/src/errors.rs` (and verified against `docs/CONTRACT_ERRORS.md` and `bindings/src/index.ts`) to its exact numeric/hex code, technical meaning, and consumer-friendly wallet message (e.g., Freighter).

## Error Table

| Hex Code | Decimal | Enum Identifier | Technical Meaning | Consumer-Facing Message |
|----------|--------:|-----------------|-------------------|-------------------------|
| `0x01` | 1 | `AlreadyInitialized` | Contract has already been initialized | "Contract already initialized." |
| `0x02` | 2 | `AdminNotSet` | Admin address not set — call initialize first | "Admin not set. Initialize contract first." |
| `0x03` | 3 | `OracleNotSet` | Oracle address not set — call initialize first | "Oracle not set. Initialize contract first." |
| `0x06` | 6 | `InvalidBetAmount` | Bet amount must be greater than zero | "Bet amount must be > 0." |
| `0x07` | 7 | `NoActiveRound` | No active round exists | "No active round." |
| `0x08` | 8 | `RoundEnded` | Round has already ended | "Round already ended." |
| `0x09` | 9 | `InsufficientBalance` | User has insufficient balance | "Insufficient balance." |
| `0x0a` | 10 | `AlreadyBet` | User has already placed a bet in this round | "Bet already placed this round." |
| `0x0b` | 11 | `Overflow` | Arithmetic overflow occurred | "Arithmetic overflow." |
| `0x0c` | 12 | `InvalidPrice` | Invalid price value | "Invalid price." |
| `0x0d` | 13 | `InvalidDuration` | Invalid duration value | "Invalid duration." |
| `0x0e` | 14 | `InvalidMode` | Invalid round mode (must be 0 or 1) | "Invalid round mode." |
| `0x0f` | 15 | `WrongModeForPrediction` | Wrong prediction type for current round mode | "Wrong prediction type for round mode." |
| `0x10` | 16 | `RoundNotEnded` | Round has not reached end_ledger yet | "Round not yet ended." |
| `0x12` | 18 | `StaleOracleData` | Oracle data is older than the configured staleness threshold | "Stale oracle data." |
| `0x13` | 19 | `InvalidOracleRound` | Oracle payload round_id does not match the active round | "Mismatched oracle round ID." |
| `0x14` | 20 | `RoundAlreadyActive` | An active round already exists and cannot be overwritten | "Active round already exists." |
| `0x16` | 22 | `ContractPaused` | Contract or action is paused by the runtime policy gate | "Contract paused." |
| `0x17` | 23 | `WindowOutOfRange` | One or more window or duration values are out of valid bounds | "Window value out of range." |
| `0x18` | 24 | `FutureOracleData` | Oracle payload timestamp is in the future beyond allowed skew | "Future oracle timestamp." |
| `0x19` | 25 | `PayoutOverflow` | Arithmetic overflow in payout accumulation — no funds moved | "Payout overflow." |
| `0x1b` | 27 | `RoundNotCancellable` | Round cannot be cancelled (no active round or already resolved) | "Round not cancellable." |
| `0x1c` | 28 | `StakeExceedsMax` | Bet amount exceeds the configured maximum stake | "Bet exceeds max stake." |
| `0x1d` | 29 | `ExposureCapExceeded` | User's cumulative exposure in this round exceeds the configured cap | "Exposure cap exceeded." |
| `0x1e` | 30 | `PendingWinningsCapExceeded` | Pending winnings accumulation would exceed the configured cap | "Pending winnings cap exceeded." |
| `0x1f` | 31 | `InvalidStartPrice` | Round start price is outside the permitted minimum/maximum bounds | "Invalid start price." |
| `0x21` | 33 | `OracleNonceReused` | Oracle payload nonce was already consumed for this round (replay) | "Oracle nonce reused." |
| `0x23` | 35 | `InvalidMinParticipants` | Minimum participants value is out of valid range (1–10000) | "Invalid min participants." |
| `0x26` | 38 | `InvalidPrecisionCap` | Precision participant cap is out of valid range (1–10000) | "Invalid precision participant cap." |
| `0x27` | 39 | `PrecisionCapExceeded` | Precision round has reached the configured participant cap | "Precision participant cap exceeded." |
| `0x29` | 41 | `OracleDeviationExceeded` | Oracle final price deviates beyond the configured max deviation threshold | "Oracle deviation exceeded." |
| `0x2a` | 42 | `UnsupportedSchemaVersion` | Stored schema version is unknown or unsupported by this contract build | "Unsupported schema version." |
| `0x2c` | 44 | `MigrationActiveRound` | Schema migration cannot run while a round is active | "Migration not allowed during active round." |
| `0x2d` | 45 | `CommitmentNotFound` | Commitment for precision prediction not found | "Precision commitment not found." |
| `0x2e` | 46 | `AlreadyRevealed` | Precision prediction has already been revealed | "Prediction already revealed." |
| `0x2f` | 47 | `InvalidRevealWindow` | Attempted to reveal precision prediction outside the valid reveal window | "Invalid reveal window." |
| `0x30` | 48 | `HashMismatch` | Revealed prediction hash does not match the committed SHA-256 hash | "Hash mismatch." |
| `0x31` | 49 | `OracleNetworkMismatch` | Oracle payload network_id or contract_addr does not match this deployment | "Oracle network or contract mismatch." |
| `0x33` | 51 | `InvalidProtocolFeeBps` | Protocol fee basis points exceed the maximum allowed ceiling | "Invalid protocol fee BPS." |
| `0x35` | 53 | `MintLimitExceeded` | Requested mint exceeds the per-user mint limit | "Mint limit exceeded." |
| `0x36` | 54 | `NoPendingRotation` | No pending oracle rotation proposal exists to accept or cancel | "No pending oracle rotation." |
| `0x37` | 55 | `RotationDelayNotElapsed` | Oracle rotation delay has not elapsed yet (must wait MIN_ROTATION_DELAY_SECONDS) | "Oracle rotation timelock has not elapsed." |
| `0x3e` | 62 | `InvalidArchiveRetention` | Archive retention limit is outside the valid range | "Invalid archive retention limit." |
| `0x3f` | 63 | `InvalidCommitment` | Commitment hash is malformed (e.g. all-zero placeholder) | "Invalid commitment hash." |
| `0x40` | 64 | `InvalidSalt` | Reveal salt fails minimum entropy rules | "Invalid reveal salt." |
| `0x41` | 65 | `NoRoundTemplate` | No round template is configured for create_next_from_template | "No round template configured." |
| `0x42` | 66 | `OracleTimestampOutsideWindow` | Oracle payload timestamp is outside the round-relative economic window | "Oracle timestamp outside round window." |
| `0x43` | 67 | `EpochBudgetExceeded` | Epoch mint budget has been fully consumed | "Epoch mint budget exceeded." |
| `0x44` | 68 | `OracleNotLive` | Oracle heartbeat is not live and strict mode blocks settlement | "Oracle heartbeat not live." |
| `0x45` | 69 | `InvalidPayoutPolicy` | Invalid precision payout policy identifier | "Invalid payout policy." |
| `0x46` | 70 | `BelowMinBet` | Stake amount is below the configured minimum bet (dust protection) | "Bet amount is below minimum bet." |
| `0x47` | 71 | `InsufficientOracleQuorum` | Fewer oracle observations survived outlier rejection than the required quorum | "Insufficient oracle quorum." |
| `0x48` | 72 | `TooFewObservations` | Multi-feed payload contains fewer observations than the configured minimum | "Too few oracle observations." |
| `0x49` | 73 | `OracleOutlierRejected` | Outlier observations dominate the multi-feed payload and prevent quorum | "Oracle observations rejected as outliers." |
| `0x4a` | 74 | `DuplicateOracleSource` | Multi-feed payload contains duplicate oracle source identifiers | "Duplicate oracle source in payload." |
| `0x4b` | 75 | `InvalidObservationOrder` | Multi-feed observations are not sorted or source IDs are out of range | "Invalid oracle observation order." |
| `0x4c` | 76 | `UnsupportedDataKeyForTtlTouch` | Requested storage key is not permitted for batch TTL touch operations | "Unsupported data key for TTL touch." |
| `0x4d` | 77 | `PendingWinningsNotFound` | Pending winnings entry does not exist or expiry is not configured | "No pending winnings found." |
| `0x4e` | 78 | `ExpiryNotConfigured` | Pending winnings expiry threshold is not configured (value is 0) | "Pending winnings expiry not configured." |
| `0x4f` | 79 | `AccessDenied` | Participant is blocked by the active allowlist or denylist access policy | "Access denied by participant policy." |
| `0x50` | 80 | `ProposalNotFound` | Governance proposal or amendment does not exist | "Governance proposal not found." |
| `0x51` | 81 | `ProposalExpired` | Governance proposal is past its execution deadline | "Governance proposal expired." |
| `0x52` | 82 | `GovInvalidState` | Governance proposal cannot transition from its current state | "Invalid governance proposal state." |
| `0x53` | 83 | `GovUnauthorized` | Caller is not authorized by the configured governance policy | "Unauthorized governance caller." |
| `0x54` | 84 | `IllegalPhaseTransition` | Requested action is not valid in the round's current lifecycle phase | "Action not allowed in current round phase." |
| `0x55` | 85 | `OracleHeartbeatUnhealthy` | Oracle heartbeat failed the configured freshness or health policy | "Oracle heartbeat unhealthy." |
| `0x56` | 86 | `PendingWinningsNotExpired` | Pending winnings entry has not yet reached the configured expiry threshold | "Pending winnings have not expired yet." |
| `0x57` | 87 | `ClaimBatchTooLarge` | claim_many batch size exceeds MAX_CLAIM_BATCH_SIZE | "Claim batch size exceeds maximum limit." |
| `0x58` | 88 | `DuplicateClaimAddress` | claim_many batch contains the same address more than once | "Duplicate address in claim batch." |
| `0x5b` | 91 | `DisputeWindowExpired` | Dispute window for void_round has expired or dispute windows are disabled | "Dispute window has expired." |
| `0x5c` | 92 | `ClaimLocked` | finalize_round or claim was called before the dispute window elapsed | "Claims are locked until the dispute window ends." |
| `0x5d` | 93 | `RoundStartLedgerReused` | Current ledger sequence has already backed another round start_ledger | "Wait for the next ledger before creating a round." |
| `0x5e` | 94 | `PageSizeExceeded` | Pagination limit exceeds MAX_PAGE_SIZE gas guard | "Requested page size exceeds maximum limit." |
| `0x5f` | 95 | `EarlyCashoutDisabled` | Early cash-out feature is disabled or not configured | "Early cash-out is currently disabled." |
| `0x60` | 96 | `PositionNotFound` | User does not have an active position to cash out | "No active position found to cash out." |
| `0x61` | 97 | `InvalidPhaseForCashout` | Early cash-out attempted outside the valid running phase | "Early cash-out only available during running phase." |
| `0x62` | 98 | `WrongModeForCashout` | Early cash-out is only supported for UpDown rounds | "Early cash-out is not supported in Precision mode." |
| `0x63` | 99 | `InsuranceInvalidSplit` | Proposed insurance payout split does not sum to the covered balance | "Invalid insurance payout split." |
| `0x64` | 100 | `InsuranceInsufficientFund` | Insurance backstop fund has insufficient balance to cover the claim | "Insufficient insurance fund balance." |
| `0x65` | 101 | `InvalidAmount` | Supplied token amount is invalid for the requested operation | "Invalid token amount." |
| `0x66` | 102 | `BettingClosed` | Close-buffer has frozen betting before the round betting window ends | "Betting is closed early. The round is still in progress." |

## Integration Walkthroughs

### 1. Handling errors in a Freighter wallet
```ts
import { ContractError } from "@xelma/bindings";

export function decodeContractError(code: number): string {
  return ContractError[code as keyof typeof ContractError]?.message ?? `Unknown contract error (${code})`;
}
```

### 2. Displaying user-friendly messages in a React UI
```tsx
import { decodeContractError } from "./errors";

export function ErrorBanner({ code }: { code: number }) {
  return <div className="error">{decodeContractError(code)}</div>;
}
```

### 3. Verifying parity across Rust, TypeScript bindings, and documentation
```sh
npm --prefix bindings run test:parity
npm --prefix bindings run test:errors
```
