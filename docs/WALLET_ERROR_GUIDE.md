# Wallet Error Integration Guide

This guide maps each smart‑contract error defined in `contracts/src/errors.rs` to a consumer‑friendly message and usage example for wallet integrations (e.g., Freighter).

## Error Table
| Hex Code | Decimal | Enum Identifier | Technical Meaning | Consumer‑Facing Message |
|----------|---------|----------------|-------------------|------------------------|
| `0x01` | 1 | AlreadyInitialized | Contract has already been initialized | "Contract already initialized."
| `0x02` | 2 | AdminNotSet | Admin address not set - call initialize first | "Admin not set. Initialize contract first."
| `0x03` | 3 | OracleNotSet | Oracle address not set - call initialize first | "Oracle not set. Initialize contract first."
| `0x04` | 4 | UnauthorizedAdmin | Only admin can perform this action | "Admin only action."
| `0x05` | 5 | UnauthorizedOracle | Only oracle can perform this action | "Oracle only action."
| `0x06` | 6 | InvalidBetAmount | Bet amount must be greater than zero | "Bet amount must be > 0."
| `0x07` | 7 | NoActiveRound | No active round exists | "No active round."
| `0x08` | 8 | RoundEnded | Round has already ended | "Round already ended."
| `0x09` | 9 | InsufficientBalance | User has insufficient balance | "Insufficient balance."
| `0x0a` | 10 | AlreadyBet | User has already placed a bet in this round | "Bet already placed this round."
| `0x0b` | 11 | Overflow | Arithmetic overflow occurred | "Arithmetic overflow."
| `0x0c` | 12 | InvalidPrice | Invalid price value | "Invalid price."
| `0x0d` | 13 | InvalidDuration | Invalid duration value | "Invalid duration."
| `0x0e` | 14 | InvalidMode | Invalid round mode (must be 0 or 1) | "Invalid round mode."
| `0x0f` | 15 | WrongModeForPrediction | Wrong prediction type for current round mode | "Wrong prediction type for round mode."
| `0x10` | 16 | RoundNotEnded | Round has not reached end_ledger yet | "Round not yet ended."
| `0x11` | 17 | InvalidPriceScale | Invalid price scale (must represent 4 decimal places) | "Invalid price scale."
| `0x12` | 18 | StaleOracleData | Oracle data is too old (STALE) | "Stale oracle data."
| `0x13` | 19 | InvalidOracleRound | Oracle payload round_id doesn't match ActiveRound | "Mismatched oracle round ID."
| `0x14` | 20 | RoundAlreadyActive | An active round already exists and cannot be overwritten | "Active round already exists."
| `0x15` | 21 | AdminIsOracle | Admin and Oracle addresses cannot be identical | "Admin cannot be Oracle."
| `0x16` | 22 | ContractPaused | Contract is paused for emergency recovery | "Contract paused."
| `0x17` | 23 | WindowOutOfRange | One or more window values exceed configured maximum bounds | "Window value out of range."
| `0x18` | 24 | FutureOracleData | Oracle payload timestamp is in the future | "Future oracle timestamp."
| `0x19` | 25 | PayoutOverflow | Arithmetic overflow in payout accumulation — no funds moved | "Payout overflow."
| `0x1a` | 26 | RoundCancelled | Round has been cancelled and cannot be resolved | "Round cancelled."
| `0x1b` | 27 | RoundNotCancellable | Round cannot be cancelled (no active round or already resolved) | "Round not cancellable."
| `0x1c` | 28 | StakeExceedsMax | Bet amount exceeds the configured maximum stake | "Bet exceeds max stake."
| `0x1d` | 29 | ExposureCapExceeded | User's cumulative exposure in this round exceeds the configured cap | "Exposure cap exceeded."
| `0x1e` | 30 | PendingWinningsCapExceeded | Pending winnings accumulation would exceed the configured cap | "Pending winnings cap exceeded."
| `0x1f` | 31 | StartPriceTooLow | Start price is below the minimum allowed value | "Start price too low."
| `0x20` | 32 | StartPriceTooHigh | Start price exceeds the maximum allowed value | "Start price too high."
| `0x21` | 33 | OracleNonceReused | Oracle payload nonce was already consumed for this round (replay) | "Oracle nonce reused."
| `0x22` | 34 | InsufficientParticipants | Round has fewer participants than the configured minimum for competitive settlement | "Insufficient participants."
| `0x23` | 35 | InvalidMinParticipants | Minimum participants value is out of valid range (must be 1–10000) | "Invalid min participants."
| `0x24` | 36 | InvalidOracleStatus | Oracle heartbeat status is out of range (must be 0, 1, or 2) | "Invalid oracle status."
| `0x25` | 37 | InvalidStaleThreshold | Oracle stale threshold is out of valid range (must be 60–86400 seconds) | "Invalid stale threshold."
| `0x26` | 38 | InvalidOracleDeviationBps | Oracle max deviation bps is invalid (must be > 0) | "Invalid oracle deviation BPS."
| `0x27` | 39 | OracleDeviationExceeded | Oracle final price deviates beyond configured threshold | "Oracle deviation exceeded."
| `0x28` | 40 | UnsupportedSchemaVersion | Stored schema version is unknown or unsupported by this contract build | "Unsupported schema version."
| `0x29` | 41 | InvalidMigrationPath | Migration path is invalid for the stored schema version | "Invalid migration path."
| `0x2a` | 42 | MigrationActiveRound | Migration cannot run while a round is active | "Migration not allowed during active round."
| `0x2b` | 43 | CommitmentNotFound | Commitment for precision prediction not found | "Precision commitment not found."
| `0x2c` | 44 | AlreadyRevealed | Precision prediction has already been revealed | "Prediction already revealed."
| `0x2d` | 45 | InvalidRevealWindow | Attempted to reveal prediction outside the valid window | "Invalid reveal window."
| `0x2e` | 46 | HashMismatch | Revealed prediction hash does not match committed hash | "Hash mismatch."
| `0x2f` | 47 | PrecisionParticipantCapExceeded | Precision round has reached the configured participant cap | "Precision participant cap exceeded."
| `0x30` | 48 | InvalidPrecisionParticipantCap | Precision participant cap is out of range (must be 1–10000) | "Invalid precision participant cap."

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
