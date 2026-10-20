# Wallet Error Integration Guide

This guide maps each smart‑contract error defined in `contracts/src/errors.rs` to a consumer‑friendly message and usage example for wallet integrations (e.g., Freighter).

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
| `0x12` | 18 | StaleOracleData | Oracle data is too old (STALE) | "Stale oracle data." |
| `0x13` | 19 | InvalidOracleRound | Oracle payload round_id doesn't match ActiveRound | "Mismatched oracle round ID." |
| `0x14` | 20 | RoundAlreadyActive | An active round already exists and cannot be overwritten | "Active round already exists." |
| `0x16` | 22 | ContractPaused | Contract is paused for emergency recovery | "Contract paused." |
| `0x17` | 23 | WindowOutOfRange | One or more window values exceed configured maximum bounds | "Window value out of range." |
| `0x18` | 24 | FutureOracleData | Oracle payload timestamp is in the future | "Future oracle timestamp." |
| `0x19` | 25 | PayoutOverflow | Arithmetic overflow in payout accumulation — no funds moved | "Payout overflow." |
| `0x1b` | 27 | RoundNotCancellable | Round cannot be cancelled (no active round or already resolved) | "Round not cancellable." |
| `0x1c` | 28 | StakeExceedsMax | Bet amount exceeds the configured maximum stake | "Bet exceeds max stake." |
| `0x1d` | 29 | ExposureCapExceeded | User's cumulative exposure in this round exceeds the configured cap | "Exposure cap exceeded." |
| `0x1e` | 30 | PendingWinningsCapExceeded | Pending winnings accumulation would exceed the configured cap | "Pending winnings cap exceeded." |
| `0x1f` | 31 | InvalidStartPrice | Start price is outside the allowed range | "Invalid start price." |
| `0x21` | 33 | OracleNonceReused | Oracle payload nonce was already consumed for this round (replay) | "Oracle nonce reused." |
| `0x23` | 35 | InvalidMinParticipants | Minimum participants value is out of valid range (must be 1–10000) | "Invalid min participants." |
| `0x26` | 38 | InvalidPrecisionCap | Precision participant cap is out of range (must be 1–10000) | "Invalid precision participant cap." |
| `0x27` | 39 | PrecisionCapExceeded | Precision round has reached the configured participant cap | "Precision participant cap exceeded." |
| `0x29` | 41 | OracleDeviationExceeded | Oracle final price deviates beyond configured threshold | "Oracle deviation exceeded." |
| `0x2a` | 42 | UnsupportedSchemaVersion | Stored schema version is unknown or unsupported by this contract build | "Unsupported schema version." |
| `0x2c` | 44 | MigrationActiveRound | Migration cannot run while a round is active | "Migration not allowed during active round." |
| `0x2d` | 45 | CommitmentNotFound | Commitment for precision prediction not found | "Precision commitment not found." |
| `0x2e` | 46 | AlreadyRevealed | Precision prediction has already been revealed | "Prediction already revealed." |
| `0x2f` | 47 | InvalidRevealWindow | Attempted to reveal prediction outside the valid window | "Invalid reveal window." |
| `0x30` | 48 | HashMismatch | Revealed prediction hash does not match committed hash | "Hash mismatch." |
| `0x31` | 49 | OracleNetworkMismatch | Oracle payload network_id does not match the runtime network | "Oracle network mismatch." |
| `0x33` | 51 | InvalidProtocolFeeBps | Protocol fee bps is outside the allowed range | "Invalid protocol fee BPS." |
| `0x35` | 53 | MintLimitExceeded | Rate limit for minting in the current ledger has been exceeded | "Mint rate limit exceeded." |
| `0x36` | 54 | NoPendingRotation | No pending oracle rotation proposal to accept or cancel | "No pending oracle rotation." |
| `0x37` | 55 | RotationDelayNotElapsed | Oracle rotation delay has not elapsed yet | "Oracle rotation delay not elapsed." |
| `0x3e` | 62 | InvalidArchiveRetention | Invalid archive retention limit | "Invalid archive retention limit." |
| `0x3f` | 63 | InvalidCommitment | Commitment hash is malformed (e.g. all-zero placeholder) | "Invalid commitment hash." |
| `0x40` | 64 | InvalidSalt | Reveal salt fails minimum entropy rules | "Invalid reveal salt." |
| `0x41` | 65 | NoRoundTemplate | No round template configured | "No round template." |
| `0x42` | 66 | OracleTimestampOutsideWindow | Oracle payload timestamp is outside the round-relative economic window | "Oracle timestamp outside window." |
| `0x43` | 67 | EpochBudgetExceeded | Epoch mint budget has been fully consumed | "Epoch mint budget exceeded." |
| `0x44` | 68 | OracleNotLive | Oracle heartbeat is not live and strict mode blocks settlement | "Oracle heartbeat not live." |
| `0x45` | 69 | InvalidPayoutPolicy | Invalid precision payout policy | "Invalid payout policy." |
| `0x46` | 70 | BelowMinBet | Stake amount is below the configured minimum bet | "Stake is below minimum bet." |
| `0x47` | 71 | InsufficientOracleQuorum | Too few oracle observations survived quorum validation | "Insufficient oracle quorum." |
| `0x48` | 72 | TooFewObservations | Multi-feed payload has too few observations | "Too few oracle observations." |
| `0x49` | 73 | OracleOutlierRejected | Oracle observation was rejected as an outlier | "Oracle outlier rejected." |
| `0x4a` | 74 | DuplicateOracleSource | Multi-feed payload contains a duplicate source | "Duplicate oracle source." |
| `0x4b` | 75 | InvalidObservationOrder | Multi-feed observations are in an invalid order | "Invalid oracle observation order." |
| `0x4c` | 76 | UnsupportedDataKeyForTtlTouch | Data key is not allowed for batch TTL touch | "Unsupported data key for TTL touch." |
| `0x4d` | 77 | PendingWinningsNotFound | Pending winnings entry was not found | "Pending winnings not found." |
| `0x4e` | 78 | ExpiryNotConfigured | Pending winnings expiry is not configured | "Pending winnings expiry not configured." |
| `0x4f` | 79 | AccessDenied | Participant is blocked by the active access-control policy | "Access denied." |
| `0x50` | 80 | ProposalNotFound | Governance proposal was not found | "Governance proposal not found." |
| `0x51` | 81 | ProposalExpired | Governance proposal has expired | "Governance proposal expired." |
| `0x52` | 82 | GovInvalidState | Governance proposal is in an invalid state | "Invalid governance proposal state." |
| `0x53` | 83 | GovUnauthorized | Caller is unauthorized by the governance policy | "Unauthorized governance action." |
| `0x54` | 84 | IllegalPhaseTransition | Action is invalid in the current round lifecycle phase | "Illegal round phase transition." |
| `0x55` | 85 | OracleHeartbeatUnhealthy | Oracle heartbeat failed the configured health policy | "Oracle heartbeat unhealthy." |
| `0x56` | 86 | PendingWinningsNotExpired | Pending winnings entry has not yet reached the configured expiry threshold | "Pending winnings have not expired yet." |
| `0x57` | 87 | ClaimBatchTooLarge | claim_many batch size exceeds MAX_CLAIM_BATCH_SIZE | "Claim batch size too large." |
| `0x58` | 88 | DuplicateClaimAddress | claim_many batch contains a duplicate address | "Duplicate address in claim batch." |
| `0x5b` | 91 | DisputeWindowExpired | The dispute window for voiding the round has expired | "Dispute window expired." |
| `0x5c` | 92 | ClaimLocked | The round cannot be finalized before its dispute window elapses | "Claims locked during dispute window." |
| `0x5d` | 93 | RoundStartLedgerReused | The current ledger already identifies another round | "Round start ledger already used." |
| `0x5e` | 94 | PageSizeExceeded | Pagination limit is zero or exceeds MAX_PAGE_SIZE | "Pagination limit exceeded." |
| `0x5f` | 95 | EarlyCashoutDisabled | Early cash-out is disabled or penalty rate is unset | "Early cash-out is currently disabled." |
| `0x60` | 96 | PositionNotFound | User has no active position in the round to cash out | "No active position found to cash out." |
| `0x61` | 97 | InvalidPhaseForCashout | Early cash-out is only permitted during the running phase | "Early cash-out only available during running phase." |
| `0x62` | 98 | WrongModeForCashout | Early cash-out is only supported for UpDown rounds | "Early cash-out is not supported in Precision mode." |
| `0x63` | 99 | InsuranceInvalidSplit | An insurance payout split does not match the covered balance | "Invalid insurance payout split." |
| `0x64` | 100 | InsuranceInsufficientFund | The insurance fund cannot cover the requested payout | "Insufficient insurance fund balance." |
| `0x65` | 101 | InvalidAmount | The supplied token amount is invalid | "Invalid token amount." |
| `0x66` | 102 | BettingClosed | Close-buffer window has frozen; betting is closed early but the round has not ended | "Betting is closed early. The round is still in progress." |

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
*Last updated: 2026‑10‑20*
