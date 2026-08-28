# Wallet Error Integration Guide

This guide maps each smart-contract error defined in `contracts/src/errors.rs` to a consumer-friendly message and usage example for wallet integrations.

## Error Table
| Hex Code | Decimal | Enum Identifier | Technical Meaning | Consumer-Facing Message |
|----------|---------|-----------------|------------------|------------------------|
| `0x01` | 1 | AlreadyInitialized | Contract has already been initialized | "Contract already initialized." |
| `0x02` | 2 | AdminNotSet | Admin address is not set | "Admin not set. Initialize contract first." |
| `0x03` | 3 | OracleNotSet | Oracle address is not set | "Oracle not set. Initialize contract first." |
| `0x06` | 6 | InvalidBetAmount | Bet amount must be greater than zero | "Bet amount must be > 0." |
| `0x07` | 7 | NoActiveRound | No active round exists | "No active round." |
| `0x08` | 8 | RoundEnded | Round has already ended | "Round already ended." |
| `0x09` | 9 | InsufficientBalance | User has insufficient balance | "Insufficient balance." |
| `0x0a` | 10 | AlreadyBet | User already placed a bet | "Bet already placed this round." |
| `0x0b` | 11 | Overflow | Arithmetic overflow occurred | "Arithmetic overflow." |
| `0x0c` | 12 | InvalidPrice | Invalid price value | "Invalid price." |
| `0x0d` | 13 | InvalidDuration | Invalid duration value | "Invalid duration." |
| `0x0e` | 14 | InvalidMode | Invalid round mode | "Invalid round mode." |
| `0x0f` | 15 | WrongModeForPrediction | Wrong prediction type for the round mode | "Wrong prediction type for round mode." |
| `0x10` | 16 | RoundNotEnded | Round has not reached its end ledger | "Round not yet ended." |
| `0x12` | 18 | StaleOracleData | Oracle data is too old | "Stale oracle data." |
| `0x13` | 19 | InvalidOracleRound | Oracle round does not match the active round | "Mismatched oracle round ID." |
| `0x14` | 20 | RoundAlreadyActive | An active round already exists | "Active round already exists." |
| `0x16` | 22 | ContractPaused | Contract is paused | "Contract paused." |
| `0x17` | 23 | WindowOutOfRange | Window value exceeds configured bounds | "Window value out of range." |
| `0x18` | 24 | FutureOracleData | Oracle timestamp is in the future | "Future oracle timestamp." |
| `0x19` | 25 | PayoutOverflow | Payout arithmetic overflowed | "Payout overflow." |
| `0x1b` | 27 | RoundNotCancellable | Round cannot be cancelled | "Round not cancellable." |
| `0x1c` | 28 | StakeExceedsMax | Bet exceeds maximum stake | "Bet exceeds max stake." |
| `0x1d` | 29 | ExposureCapExceeded | User exposure exceeds its cap | "Exposure cap exceeded." |
| `0x1e` | 30 | PendingWinningsCapExceeded | Pending winnings exceed their cap | "Pending winnings cap exceeded." |
| `0x1f` | 31 | InvalidStartPrice | Start price is invalid | "Invalid start price." |
| `0x21` | 33 | OracleNonceReused | Oracle nonce was already consumed | "Oracle nonce reused." |
| `0x23` | 35 | InvalidMinParticipants | Minimum participants value is invalid | "Invalid min participants." |
| `0x26` | 38 | InvalidPrecisionCap | Precision participant cap is invalid | "Invalid precision participant cap." |
| `0x27` | 39 | PrecisionCapExceeded | Precision participant cap was reached | "Precision participant cap exceeded." |
| `0x29` | 41 | OracleDeviationExceeded | Oracle price deviation exceeds the configured threshold | "Oracle deviation exceeded." |
| `0x2a` | 42 | UnsupportedSchemaVersion | Stored schema version is unsupported | "Unsupported schema version." |
| `0x2c` | 44 | MigrationActiveRound | Migration is blocked during an active round | "Migration not allowed during active round." |
| `0x2d` | 45 | CommitmentNotFound | Precision commitment was not found | "Precision commitment not found." |
| `0x2e` | 46 | AlreadyRevealed | Prediction was already revealed | "Prediction already revealed." |
| `0x2f` | 47 | InvalidRevealWindow | Reveal was attempted outside the valid window | "Invalid reveal window." |
| `0x30` | 48 | HashMismatch | Revealed prediction does not match its commitment | "Hash mismatch." |
| `0x31` | 49 | OracleNetworkMismatch | Oracle payload targets another network | "Oracle network mismatch." |
| `0x33` | 51 | InvalidProtocolFeeBps | Protocol fee is outside the allowed range | "Invalid protocol fee." |
| `0x35` | 53 | MintLimitExceeded | Mint rate limit was exceeded | "Mint limit exceeded." |
| `0x36` | 54 | NoPendingRotation | No pending oracle rotation exists | "No pending oracle rotation." |
| `0x37` | 55 | RotationDelayNotElapsed | Oracle rotation delay has not elapsed | "Oracle rotation delay has not elapsed." |
| `0x3e` | 62 | InvalidArchiveRetention | Archive retention limit is invalid | "Invalid archive retention limit." |
| `0x3f` | 63 | InvalidCommitment | Commitment hash is malformed | "Invalid commitment hash." |
| `0x40` | 64 | InvalidSalt | Reveal salt fails minimum entropy rules | "Invalid reveal salt." |
| `0x41` | 65 | NoRoundTemplate | No round template is configured | "No round template." |
| `0x43` | 67 | EpochBudgetExceeded | Epoch mint budget was fully consumed | "Epoch mint budget exceeded." |
| `0x44` | 68 | OracleNotLive | Oracle heartbeat is not live | "Oracle heartbeat not live." |
| `0x45` | 69 | InvalidPayoutPolicy | Precision payout policy is invalid | "Invalid payout policy." |
| `0x46` | 70 | BelowMinBet | Stake is below the configured minimum bet | "Bet is below the minimum amount." |
| `0x47` | 71 | InsufficientOracleQuorum | Too few observations survived outlier rejection | "Insufficient oracle quorum." |
| `0x48` | 72 | TooFewObservations | Oracle payload contains too few observations | "Too few oracle observations." |
| `0x49` | 73 | OracleOutlierRejected | Oracle outlier rejection prevented settlement | "Oracle outlier rejected." |
| `0x4a` | 74 | DuplicateOracleSource | Oracle payload contains duplicate sources | "Duplicate oracle source." |
| `0x4b` | 75 | InvalidObservationOrder | Oracle observations are not in the required order | "Invalid oracle observation order." |
| `0x4c` | 76 | UnsupportedDataKeyForTtlTouch | Data key is not supported for TTL touch | "Unsupported data key for TTL touch." |
| `0x4d` | 77 | PendingWinningsNotFound | Pending winnings entry does not exist | "Pending winnings not found." |
| `0x4e` | 78 | ExpiryNotConfigured | Pending winnings expiry is not configured | "Pending winnings expiry is not configured." |
| `0x4f` | 79 | EarlyCashoutDisabled | Early cash-out is disabled | "Early cash-out is currently disabled." |
| `0x50` | 80 | PositionNotFound | User has no active position to cash out | "No active position found to cash out." |
| `0x51` | 81 | InvalidPhaseForCashout | Cash-out is outside the running phase | "Early cash-out only available during running phase." |
| `0x52` | 82 | WrongModeForCashout | Early cash-out is only supported for UpDown rounds | "Early cash-out is not supported in Precision mode." |

## Integration Walkthroughs

```ts
import { ContractErrorDecoder } from "@xelma/contracts";

function handleError(error: any) {
  const code = error.result?.xdr?.value?.val?.code ?? 0;
  alert(ContractErrorDecoder(code));
}
```

Use `decodeContractError(code)` when the wallet needs the numeric code and enum identifier, and use `formatContractError(code)` for a compact user-facing fallback. Unknown codes must remain actionable rather than being silently discarded.

---
*Last updated: 2026-08-28*
