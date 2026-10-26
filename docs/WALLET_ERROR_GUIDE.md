# Wallet Error Integration Guide

This guide maps each smart‑contract error defined in `contracts/src/errors.rs` to a consumer‑friendly message and usage example for wallet integrations (e.g., Freighter).

The table below is a 1:1 enumeration of every `ContractError` variant (81 total, matching `contracts/src/errors.rs`) sorted by decimal code. Codes are not contiguous — gaps correspond to variants that were removed in earlier iterations of the contract and are permanently retired to avoid re-litigating already-observed on-chain error codes.

## Error Table
| Hex Code | Decimal | Enum Identifier | Technical Meaning | Consumer‑Facing Message |
|----------|---------|----------------|-------------------|------------------------|
| `0x01` | 1 | AlreadyInitialized | Contract has already been initialized | "Contract already initialized." |
| `0x02` | 2 | AdminNotSet | Admin address not set - call initialize first | "Admin not set. Initialize contract first." |
| `0x03` | 3 | OracleNotSet | Oracle address not set - call initialize first | "Oracle not set. Initialize contract first." |
| `0x06` | 6 | InvalidBetAmount | Bet amount must be greater than zero | "Bet amount must be > 0." |
| `0x07` | 7 | NoActiveRound | No active round exists | "No active round." |
| `0x08` | 8 | RoundEnded | Round has already ended | "Round already ended." |
| `0x09` | 9 | InsufficientBalance | User has insufficient balance | "Insufficient balance." |
| `0x0a` | 10 | AlreadyBet | User has already placed a bet in this round | "Bet already placed this round." |
| `0x0b` | 11 | Overflow | Arithmetic overflow occurred | "Arithmetic overflow." |
| `0x0c` | 12 | InvalidPrice | Invalid price value | "Invalid price." |
| `0x0d` | 13 | InvalidDuration | Invalid duration value | "Invalid duration." |
| `0x0e` | 14 | InvalidMode | Invalid round mode (must be 0 or 1) | "Invalid round mode." |
| `0x0f` | 15 | WrongModeForPrediction | Wrong prediction type for current round mode | "Wrong prediction type for round mode." |
| `0x10` | 16 | RoundNotEnded | Round has not reached end_ledger yet | "Round not yet ended." |
| `0x12` | 18 | StaleOracleData | Oracle data is too old (stale) | "Stale oracle data." |
| `0x13` | 19 | InvalidOracleRound | Oracle payload round_id doesn't match the active round | "Mismatched oracle round ID." |
| `0x14` | 20 | RoundAlreadyActive | An active round already exists and cannot be overwritten | "Active round already exists." |
| `0x16` | 22 | ContractPaused | Contract is paused for emergency recovery | "Contract paused." |
| `0x17` | 23 | WindowOutOfRange | One or more window values exceed configured maximum bounds | "Window value out of range." |
| `0x18` | 24 | FutureOracleData | Oracle payload timestamp is in the future | "Future oracle timestamp." |
| `0x19` | 25 | PayoutOverflow | Arithmetic overflow in payout accumulation — no funds moved | "Payout overflow." |
| `0x1b` | 27 | RoundNotCancellable | Round cannot be cancelled (no active round or already resolved) | "Round not cancellable." |
| `0x1c` | 28 | StakeExceedsMax | Bet amount exceeds the configured maximum stake | "Bet exceeds max stake." |
| `0x1d` | 29 | ExposureCapExceeded | User's cumulative exposure in this round exceeds the configured cap | "Exposure cap exceeded." |
| `0x1e` | 30 | PendingWinningsCapExceeded | Pending winnings accumulation would exceed the configured cap | "Pending winnings cap exceeded." |
| `0x1f` | 31 | InvalidStartPrice | Start price is zero or otherwise invalid for the configured round mode | "Invalid start price." |
| `0x21` | 33 | OracleNonceReused | Oracle payload nonce was already consumed for this round (replay) | "Oracle nonce reused." |
| `0x23` | 35 | InvalidMinParticipants | Minimum participants value is out of valid range | "Invalid minimum participants." |
| `0x26` | 38 | InvalidPrecisionCap | Precision participant cap is out of range | "Invalid precision participant cap." |
| `0x27` | 39 | PrecisionCapExceeded | Precision round has reached the configured participant cap | "Precision participant cap exceeded." |
| `0x29` | 41 | OracleDeviationExceeded | Oracle final price deviates beyond the configured threshold | "Oracle deviation exceeded." |
| `0x2a` | 42 | UnsupportedSchemaVersion | Stored schema version is unknown or unsupported by this contract build | "Unsupported schema version." |
| `0x2c` | 44 | MigrationActiveRound | Migration cannot run while a round is active | "Migration not allowed during active round." |
| `0x2d` | 45 | CommitmentNotFound | Commitment for a precision prediction was not found | "Precision commitment not found." |
| `0x2e` | 46 | AlreadyRevealed | Precision prediction has already been revealed | "Prediction already revealed." |
| `0x2f` | 47 | InvalidRevealWindow | Attempted to reveal a prediction outside the valid window | "Invalid reveal window." |
| `0x30` | 48 | HashMismatch | Revealed prediction hash does not match the committed hash | "Hash mismatch." |
| `0x31` | 49 | OracleNetworkMismatch | Oracle payload network_id doesn't match the contract's network | "Oracle network mismatch." |
| `0x33` | 51 | InvalidProtocolFeeBps | Protocol fee bps exceeds the allowed maximum | "Invalid protocol fee." |
| `0x35` | 53 | MintLimitExceeded | Requested mint amount exceeds the configured per-call or epoch limit | "Mint limit exceeded." |
| `0x36` | 54 | NoPendingRotation | No oracle rotation proposal is pending | "No pending oracle rotation." |
| `0x37` | 55 | RotationDelayNotElapsed | Oracle rotation delay has not elapsed yet (must wait MIN_ROTATION_DELAY_SECONDS) | "Oracle rotation delay has not elapsed yet." |
| `0x3e` | 62 | InvalidArchiveRetention | Invalid archive retention limit | "Invalid archive retention limit." |
| `0x3f` | 63 | InvalidCommitment | Commitment hash is malformed (e.g. all-zero placeholder) | "Invalid commitment hash." |
| `0x40` | 64 | InvalidSalt | Reveal salt fails minimum entropy rules | "Invalid reveal salt." |
| `0x41` | 65 | NoRoundTemplate | No round template configured | "No round template." |
| `0x42` | 66 | OracleTimestampOutsideWindow | Oracle payload timestamp is outside the round-relative economic window | "Oracle timestamp outside the allowed window." |
| `0x43` | 67 | EpochBudgetExceeded | Epoch mint budget has been fully consumed | "Mint epoch budget exceeded." |
| `0x44` | 68 | OracleNotLive | Oracle heartbeat is not live and strict mode blocks settlement (Issue #264) | "Oracle heartbeat not live." |
| `0x45` | 69 | InvalidPayoutPolicy | Invalid precision payout policy | "Invalid precision payout policy." |
| `0x46` | 70 | BelowMinBet | Stake amount is below the configured minimum bet (dust protection, Issue #269) | "Bet amount below the minimum." |
| `0x47` | 71 | InsufficientOracleQuorum | Multi-feed resolution: fewer observations survived outlier rejection than the configured quorum threshold | "Not enough oracle sources agreed on a price." |
| `0x48` | 72 | TooFewObservations | Multi-feed resolution: payload contains fewer observations than the configured minimum | "Too few oracle observations submitted." |
| `0x49` | 73 | OracleOutlierRejected | Multi-feed resolution: outlier observations would dominate the result (too many rejected, cannot form quorum) | "Too many oracle observations were rejected as outliers." |
| `0x4a` | 74 | DuplicateOracleSource | Multi-feed payload contains duplicate source identifiers | "Duplicate oracle source in payload." |
| `0x4b` | 75 | InvalidObservationOrder | Multi-feed payload has observations that are not sorted or sources are out of expected range | "Oracle observations are out of order." |
| `0x4c` | 76 | UnsupportedDataKeyForTtlTouch | The requested data key is not allowed for batch TTL touch operations | "This storage key cannot have its TTL extended this way." |
| `0x4d` | 77 | PendingWinningsNotFound | Pending winnings entry does not exist or expiry is not configured | "No pending winnings found to reclaim." |
| `0x4e` | 78 | ExpiryNotConfigured | Pending winnings expiry is not configured (value is 0) | "Pending winnings expiry is not configured." |
| `0x4f` | 79 | AccessDenied | Participant is blocked by the active allowlist or denylist policy | "You are not allowed to perform this action." |
| `0x50` | 80 | ProposalNotFound | Governance proposal does not exist | "Governance proposal not found." |
| `0x51` | 81 | ProposalExpired | Governance proposal is past its execution deadline | "Governance proposal has expired." |
| `0x52` | 82 | GovInvalidState | Governance proposal cannot transition from its current state | "Governance proposal is not in a valid state for this action." |
| `0x53` | 83 | GovUnauthorized | Caller is not authorized by the configured governance policy | "Not authorized to perform this governance action." |
| `0x54` | 84 | IllegalPhaseTransition | Requested action is not valid in the round's current lifecycle phase | "This action is not valid in the round's current phase." |
| `0x55` | 85 | OracleHeartbeatUnhealthy | Oracle heartbeat failed the configured freshness or health policy | "Oracle heartbeat is unhealthy." |
| `0x56` | 86 | PendingWinningsNotExpired | Pending winnings entry exists but has not yet reached the configured expiry threshold — caller must wait before reclaiming | "Pending winnings have not expired yet." |
| `0x57` | 87 | ClaimBatchTooLarge | claim_many batch size exceeds MAX_CLAIM_BATCH_SIZE (Issue #277) | "Too many users in one claim batch." |
| `0x58` | 88 | DuplicateClaimAddress | claim_many batch contains the same address more than once (Issue #277) | "Duplicate address in claim batch." |
| `0x5b` | 91 | DisputeWindowExpired | The dispute window for `void_round` has expired, or dispute windows are not configured (`dispute_ledgers == 0`) | "Dispute window has expired or is not configured." |
| `0x5c` | 92 | ClaimLocked | `finalize_round` was called before the dispute window elapsed | "Claims are locked until the dispute window elapses." |
| `0x5d` | 93 | RoundStartLedgerReused | A round cannot be created because the current ledger sequence has already backed another round's `start_ledger` | "Round cannot start on a ledger already used by another round." |
| `0x5e` | 94 | PageSizeExceeded | Pagination limit exceeds MAX_PAGE_SIZE (Issue #430, gas guard) | "Requested page size is too large." |
| `0x5f` | 95 | EarlyCashoutDisabled | Early cash-out feature is disabled or not configured | "Early cash-out is currently disabled." |
| `0x60` | 96 | PositionNotFound | User does not have an active position to cash out | "No active position found to cash out." |
| `0x61` | 97 | InvalidPhaseForCashout | Early cash-out attempted outside the valid running phase | "Early cash-out only available during the running phase." |
| `0x62` | 98 | WrongModeForCashout | Early cash-out is only supported for UpDown rounds | "Early cash-out is not supported in Precision mode." |
| `0x63` | 99 | InsuranceInvalidSplit | A proposed insurance payout split does not sum to the covered balance | "Insurance payout split does not add up to the covered balance." |
| `0x64` | 100 | InsuranceInsufficientFund | The insurance backstop fund has insufficient balance to cover the claim | "Insurance fund has insufficient balance to cover this claim." |
| `0x65` | 101 | InvalidAmount | The supplied token amount is invalid for the requested operation | "Invalid token amount." |
| `0x66` | 102 | BettingClosed | The close-buffer has frozen betting before the round's betting window ends | "Betting is closed early. The round is still in progress." |

## Integration Walkthroughs
### 1. Handling errors in a Freighter wallet
```ts
import { ContractErrorDecoder } from "@xelma/contracts";

function handleError(error: any) {
  const code = error.result?.xdr?.value?.val?.code ?? 0;
  const message = ContractErrorDecoder(code);
  alert(message);
}
```

### 2. Displaying user‑friendly messages in a React UI
```tsx
import { ContractErrorDecoder } from "@xelma/contracts";

function ErrorBanner({code}: {code: number}) {
  return <div className="error">{ContractErrorDecoder(code)}</div>;
}
```

### 3. Unit testing error mapping
```ts
import { ContractErrorDecoder } from "@xelma/contracts";
import { expect } from "chai";

describe("ContractErrorDecoder", () => {
  it("maps known codes", () => {
    expect(ContractErrorDecoder(1)).to.equal("Contract already initialized.");
    expect(ContractErrorDecoder(22)).to.equal("Contract paused.");
  });
  it("handles unknown codes", () => {
    expect(ContractErrorDecoder(999)).to.match(/Unknown error/);
  });
});
```

### 4. Automated UI test with Selenium (JavaScript)
```js
await driver.findElement(By.id("betButton")).click();
await driver.wait(until.elementLocated(By.css(".error")));
const errorText = await driver.findElement(By.css(".error")).getText();
assert.include(errorText, "Insufficient balance");
```

### 5. Monitoring contract upgrades for documentation drift
Add the script `scripts/check-doc-drift.js` to your CI pipeline. If the script exits with a non‑zero status, the CI job fails, prompting a documentation update before merging.

---
*Last updated: 2026‑10‑23*
