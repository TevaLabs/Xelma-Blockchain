| "0x01" | 1 | AlreadyInitialized | Contract has already been initialized | "Contract already initialized." |
| "0x02" | 2 | AdminNotSet | Admin address not set - call initialize first | "Admin not set. Initialize contract first." |
| "0x03" | 3 | OracleNotSet | Oracle address not set - call initialize first | "Oracle not set. Initialize contract first." |
| "0x04" | 4 | InvalidBetAmount | Bet amount must be greater than zero | "Bet amount must be > 0." |
| "0x05" | 5 | NoActiveRound | No active round exists | "No active round." |
| "0x06" | 6 | RoundEnded | Round has already ended | "Round already ended." |
| "0x07" | 7 | InsufficientBalance | User has insufficient balance | "Insufficient balance." |
| "0x08" | 8 | AlreadyBet | User has already placed a bet in this round | "Bet already placed this round." |
| "0x09" | 9 | Overflow | Arithmetic overflow occurred | "Arithmetic overflow." |
| "0x0a" | 10 | InvalidPrice | Invalid price value | "Invalid price." |
| "0x0b" | 11 | InvalidDuration | Invalid duration value | "Invalid duration." |
| "0x0c" | 12 | InvalidMode | Invalid round mode (must be 0 or 1) | "Invalid round mode." |
| "0x0d" | 13 | WrongModeForPrediction | Wrong prediction type for current round mode | "Wrong prediction type for round mode." |
| "0x0e" | 14 | RoundNotEnded | Round has not reached end_ledger yet | "Round not yet ended." |
| "0x0f" | 15 | StaleOracleData | Oracle data is too old (STALE) | "Stale oracle data." |
| "0x10" | 16 | InvalidOracleRound | Oracle payload round_id doesn't match ActiveRound | "Mismatched oracle round ID." |
| "0x11" | 17 | RoundAlreadyActive | An active round already exists and cannot be overwritten | "Active round already exists." |
| "0x12" | 18 | ContractPaused | Contract is paused for emergency recovery | "Contract paused." |
| "0x13" | 19 | WindowOutOfRange | One or more window values exceed configured maximum bounds | "Window value out of range." |
| "0x14" | 20 | FutureOracleData | Oracle payload timestamp is in the future | "Future oracle timestamp." |
| "0x15" | 21 | PayoutOverflow | Arithmetic overflow in payout accumulation — no funds moved | "Payout overflow." |
| "0x16" | 22 | RoundNotCancellable | Round cannot be cancelled (no active round or already resolved) | "Round not cancellable." |
| "0x17" | 23 | StakeExceedsMax | Bet amount exceeds the configured maximum stake | "Bet exceeds max stake." |
| "0x18" | 24 | ExposureCapExceeded | User's cumulative exposure in this round exceeds the configured cap | "Exposure cap exceeded." |
| "0x19" | 25 | PendingWinningsCapExceeded | Pending winnings accumulation would exceed the configured cap | "Pending winnings cap exceeded." |
| "0x1a" | 26 | InvalidStartPrice | Unknown | "Unknown error." |
| "0x1b" | 27 | OracleNonceReused | Oracle payload nonce was already consumed for this round (replay) | "Oracle nonce reused." |
| "0x1c" | 28 | InvalidMinParticipants | Minimum participants value is out of valid range (must be 1–10000) | "Invalid min participants." |
| "0x1d" | 29 | InvalidPrecisionCap | Unknown | "Unknown error." |
| "0x1e" | 30 | PrecisionCapExceeded | Unknown | "Unknown error." |
| "0x1f" | 31 | OracleDeviationExceeded | Oracle final price deviates beyond configured threshold | "Oracle deviation exceeded." |
| "0x20" | 32 | UnsupportedSchemaVersion | Stored schema version is unknown or unsupported by this contract build | "Unsupported schema version." |
| "0x21" | 33 | MigrationActiveRound | Migration cannot run while a round is active | "Migration not allowed during active round." |
| "0x22" | 34 | CommitmentNotFound | Commitment for precision prediction not found | "Precision commitment not found." |
| "0x23" | 35 | AlreadyRevealed | Precision prediction has already been revealed | "Prediction already revealed." |
| "0x24" | 36 | InvalidRevealWindow | Attempted to reveal prediction outside the valid window | "Invalid reveal window." |
| "0x25" | 37 | HashMismatch | Revealed prediction hash does not match committed hash | "Hash mismatch." |
| "0x26" | 38 | OracleNetworkMismatch | Unknown | "Unknown error." |
| "0x27" | 39 | InvalidProtocolFeeBps | Unknown | "Unknown error." |
| "0x28" | 40 | MintLimitExceeded | Unknown | "Unknown error." |
| "0x29" | 41 | NoPendingRotation | Unknown | "Unknown error." |
| "0x2a" | 42 | RotationDelayNotElapsed | Unknown | "Unknown error." |
| "0x2b" | 43 | InvalidArchiveRetention | Unknown | "Unknown error." |
| "0x2c" | 44 | InvalidCommitment | Commitment hash is malformed (e.g. all-zero placeholder) | "Invalid commitment hash." |
| "0x2d" | 45 | InvalidSalt | Reveal salt fails minimum entropy rules | "Invalid reveal salt." |
| "0x2e" | 46 | NoRoundTemplate | No round template configured | "No round template." |
| "0x2f" | 47 | OracleTimestampOutsideWindow | Unknown | "Unknown error." |
| "0x30" | 48 | PendingWinningsNotExpired | Unknown | "Unknown error." |
| "0x31" | 49 | EpochBudgetExceeded | Unknown | "Unknown error." |
| "0x32" | 50 | OracleNotLive | Oracle heartbeat is not live and strict mode blocks settlement | "Oracle heartbeat not live." |
| "0x33" | 51 | InvalidPayoutPolicy | Unknown | "Unknown error." |
| "0x34" | 52 | BelowMinBet | Unknown | "Unknown error." |
| "0x35" | 53 | InsufficientOracleQuorum | Unknown | "Unknown error." |
| "0x36" | 54 | TooFewObservations | Unknown | "Unknown error." |
| "0x37" | 55 | OracleOutlierRejected | Unknown | "Unknown error." |
| "0x38" | 56 | DuplicateOracleSource | Unknown | "Unknown error." |
| "0x39" | 57 | InvalidObservationOrder | Unknown | "Unknown error." |
| "0x3a" | 58 | UnsupportedDataKeyForTtlTouch | Unknown | "Unknown error." |
| "0x3b" | 59 | PendingWinningsNotFound | Unknown | "Unknown error." |
| "0x3c" | 60 | ExpiryNotConfigured | Unknown | "Unknown error." |
| "0x3d" | 61 | AccessDenied | Unknown | "Unknown error." |
| "0x3e" | 62 | ProposalNotFound | Unknown | "Unknown error." |
| "0x3f" | 63 | ProposalExpired | Unknown | "Unknown error." |
| "0x40" | 64 | GovInvalidState | Unknown | "Unknown error." |
| "0x41" | 65 | GovUnauthorized | Unknown | "Unknown error." |
| "0x42" | 66 | IllegalPhaseTransition | Unknown | "Unknown error." |
| "0x43" | 67 | OracleHeartbeatUnhealthy | Unknown | "Unknown error." |
| "0x44" | 68 | ClaimBatchTooLarge | Unknown | "Unknown error." |
| "0x45" | 69 | DuplicateClaimAddress | Unknown | "Unknown error." |
| "0x46" | 70 | EarlyCashoutDisabled | Early cash-out is disabled or penalty rate is unset | "Early cash-out is currently disabled." |
| "0x47" | 71 | PositionNotFound | User has no active position in the round to cash out | "No active position found to cash out." |
| "0x48" | 72 | InvalidPhaseForCashout | Early cash-out is only permitted during the running phase | "Early cash-out only available during running phase." |
| "0x49" | 73 | WrongModeForCashout | Early cash-out is only supported for UpDown rounds | "Early cash-out is not supported in Precision mode."

## Integration Walkthroughs
### 1. Handling errors in a Freighter wallet
"""ts
import { ContractErrorDecoder } from "@xelma/contracts";

function handleError(error: any) {
  const code = error.result?.xdr?.value?.val?.code ?? 0;
  const message = ContractErrorDecoder(code);
  alert(message);
}
"""

### 2. Displaying user‑friendly messages in a React UI
"""tsx
import { ContractErrorDecoder } from "@xelma/contracts";

function ErrorBanner({code}: {code: number}) {
  return <div className="error">{ContractErrorDecoder(code)}</div>;
}
"""

### 3. Unit testing error mapping
"""ts
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
"""

### 4. Automated UI test with Selenium (JavaScript)
"""js
await driver.findElement(By.id("betButton")).click();
await driver.wait(until.elementLocated(By.css(".error")));
const errorText = await driver.findElement(By.css(".error")).getText();
assert.include(errorText, "Insufficient balance");
"""

### 5. Monitoring contract upgrades for documentation drift
Add the script "scripts/check-doc-drift.js" to your CI pipeline. If the script exits with a non‑zero status, the CI job fails, prompting a documentation update before merging.

---
*Last updated: 2026‑06‑27* |
| "0x4a" | 74 | InsuranceInvalidSplit | Unknown | "Unknown error." |
| "0x4b" | 75 | InsuranceInsufficientFund | Unknown | "Unknown error." |
| "0x4c" | 76 | InvalidAmount | Unknown | "Unknown error." |
| "0x4d" | 77 | DisputeWindowExpired | Unknown | "Unknown error." |
| "0x4e" | 78 | ClaimLocked | Unknown | "Unknown error." |
| "0x4f" | 79 | RoundStartLedgerReused | Unknown | "Unknown error." |
| "0x50" | 80 | PageSizeExceeded | Unknown | "Unknown error." |

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
*Last updated: 2026‑06‑27*


