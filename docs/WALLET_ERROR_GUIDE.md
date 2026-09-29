# Wallet Error Integration Guide

This guide maps each smart-contract error defined in `contracts/src/errors.rs` to a consumer-friendly message and suggested wallet UX copy for wallet integrations (e.g., Freighter).

## Error Table

| Hex Code | Decimal | Enum Identifier | Technical Meaning | Suggested Wallet Copy |
|----------|---------|------------------|--------------------|------------------------|
| `0x01` | 1 | AlreadyInitialized | Contract has already been initialized | "This contract is already set up." |
| `0x02` | 2 | AdminNotSet | Admin address not set — call `initialize` first | "Contract isn't configured yet. Try again later." |
| `0x03` | 3 | OracleNotSet | Oracle address not set — call `initialize` first | "Contract isn't configured yet. Try again later." |
| `0x06` | 6 | InvalidBetAmount | Bet amount must be greater than zero | "Enter a bet amount greater than 0." |
| `0x07` | 7 | NoActiveRound | No active round exists | "No round is currently open for betting." |
| `0x08` | 8 | RoundEnded | Round has already ended | "This round has already ended." |
| `0x09` | 9 | InsufficientBalance | User has insufficient balance | "Insufficient balance for this action." |
| `0x0a` | 10 | AlreadyBet | User has already placed a bet in this round | "You've already placed a bet this round." |
| `0x0b` | 11 | Overflow | Arithmetic overflow occurred | "Something went wrong with that amount. Try a smaller value." |
| `0x0c` | 12 | InvalidPrice | Invalid price value | "Invalid price value." |
| `0x0d` | 13 | InvalidDuration | Invalid duration value | "Invalid round duration." |
| `0x0e` | 14 | InvalidMode | Invalid round mode (must be 0 or 1) | "Invalid round mode." |
| `0x0f` | 15 | WrongModeForPrediction | Wrong prediction type for current round mode | "This action isn't available for this round type." |
| `0x10` | 16 | RoundNotEnded | Round has not reached its end ledger yet | "This round hasn't ended yet." |
| `0x12` | 18 | StaleOracleData | Oracle data is too old | "Price data is stale — please try again shortly." |
| `0x13` | 19 | InvalidOracleRound | Oracle payload round_id doesn't match the active round | "Mismatched round — please refresh and try again." |
| `0x14` | 20 | RoundAlreadyActive | An active round already exists and cannot be overwritten | "A round is already in progress." |
| `0x16` | 22 | ContractPaused | Contract is paused for emergency recovery | "The platform is temporarily paused. Please try again later." |
| `0x17` | 23 | WindowOutOfRange | One or more window values exceed configured bounds | "That value is out of the allowed range." |
| `0x18` | 24 | FutureOracleData | Oracle payload timestamp is in the future | "Price data timestamp is invalid — please try again." |
| `0x19` | 25 | PayoutOverflow | Arithmetic overflow while accumulating a payout — no funds moved | "Couldn't calculate payout — please contact support." |
| `0x1b` | 27 | RoundNotCancellable | Round cannot be cancelled (no active round or already resolved) | "This round can't be cancelled." |
| `0x1c` | 28 | StakeExceedsMax | Bet amount exceeds the configured maximum stake | "That amount exceeds the maximum bet allowed." |
| `0x1d` | 29 | ExposureCapExceeded | User's cumulative exposure in this round exceeds the configured cap | "You've reached the maximum exposure allowed for this round." |
| `0x1e` | 30 | PendingWinningsCapExceeded | Pending winnings accumulation would exceed the configured cap | "Your pending winnings balance is at its limit. Please claim first." |
| `0x1f` | 31 | InvalidStartPrice | Start price is outside the allowed bounds | "Invalid starting price for this round." |
| `0x21` | 33 | OracleNonceReused | Oracle payload nonce was already consumed for this round (replay) | "This price update was already processed." |
| `0x23` | 35 | InvalidMinParticipants | Minimum participants value is out of valid range | "Invalid minimum participants setting." |
| `0x26` | 38 | InvalidPrecisionCap | Precision round participant cap is out of range | "Invalid participant cap for this round." |
| `0x27` | 39 | PrecisionCapExceeded | Precision round has reached its configured participant cap | "This round is full." |
| `0x29` | 41 | OracleDeviationExceeded | Oracle final price deviates beyond the configured threshold | "Price moved outside the allowed range for settlement." |
| `0x2a` | 42 | UnsupportedSchemaVersion | Stored schema version is unknown or unsupported by this contract build | "This contract needs an update. Please try again later." |
| `0x2c` | 44 | MigrationActiveRound | Migration cannot run while a round is active | "Can't update the contract while a round is active." |
| `0x2d` | 45 | CommitmentNotFound | Commitment for a precision prediction was not found | "No prediction commitment found." |
| `0x2e` | 46 | AlreadyRevealed | Precision prediction has already been revealed | "You've already revealed your prediction." |
| `0x2f` | 47 | InvalidRevealWindow | Attempted to reveal a prediction outside the valid window | "The reveal window for this prediction has closed." |
| `0x30` | 48 | HashMismatch | Revealed prediction hash does not match the committed hash | "Your reveal doesn't match your original commitment." |
| `0x31` | 49 | OracleNetworkMismatch | Oracle payload's network or contract ID doesn't match this deployment | "Price data doesn't match this network — please refresh." |
| `0x33` | 51 | InvalidProtocolFeeBps | Protocol fee value is out of valid range | "Invalid fee configuration." |
| `0x35` | 53 | MintLimitExceeded | Epoch mint limit has been exceeded | "Minting limit reached for this period." |
| `0x36` | 54 | NoPendingRotation | No oracle rotation is currently pending | "No pending oracle change to act on." |
| `0x37` | 55 | RotationDelayNotElapsed | Oracle rotation delay has not elapsed yet | "This change isn't ready yet — please wait." |
| `0x3e` | 62 | InvalidArchiveRetention | Invalid archive retention limit | "Invalid retention setting." |
| `0x3f` | 63 | InvalidCommitment | Commitment hash is malformed (e.g. all-zero placeholder) | "Invalid prediction commitment." |
| `0x40` | 64 | InvalidSalt | Reveal salt fails minimum entropy rules | "Invalid reveal salt — please try again." |
| `0x41` | 65 | NoRoundTemplate | No round template is configured | "No round template is set up yet." |
| `0x42` | 66 | OracleTimestampOutsideWindow | Oracle payload timestamp is outside the round-relative economic window | "Price data is outside the valid time window for this round." |
| `0x43` | 67 | EpochBudgetExceeded | Epoch mint budget has been fully consumed | "Minting budget for this period is used up." |
| `0x44` | 68 | OracleNotLive | Oracle heartbeat is not live and strict mode blocks settlement | "Price feed is currently unavailable — settlement is paused." |
| `0x45` | 69 | InvalidPayoutPolicy | Invalid precision payout policy | "Invalid payout configuration." |
| `0x46` | 70 | BelowMinBet | Stake amount is below the configured minimum bet | "Bet amount is below the minimum allowed." |
| `0x47` | 71 | InsufficientOracleQuorum | Fewer observations survived outlier rejection than the required quorum | "Not enough price sources agreed — settlement delayed." |
| `0x48` | 72 | TooFewObservations | Multi-feed payload has fewer observations than the configured minimum | "Not enough price data to settle this round." |
| `0x49` | 73 | OracleOutlierRejected | Too many outlier observations to form a quorum | "Price sources disagreed too much — settlement delayed." |
| `0x4a` | 74 | DuplicateOracleSource | Multi-feed payload contains duplicate source identifiers | "Duplicate price source detected." |
| `0x4b` | 75 | InvalidObservationOrder | Multi-feed observations are unsorted or out of expected range | "Invalid price data format." |
| `0x4c` | 76 | UnsupportedDataKeyForTtlTouch | The requested data key isn't allowed for batch TTL-touch operations | "Invalid maintenance request." |
| `0x4d` | 77 | PendingWinningsNotFound | Pending winnings entry doesn't exist, or expiry isn't configured | "No pending winnings found." |
| `0x4e` | 78 | ExpiryNotConfigured | Pending winnings expiry is not configured | "This feature isn't configured yet." |
| `0x4f` | 79 | AccessDenied | Participant is blocked by the active allowlist/denylist policy | "You don't have access to this action." |
| `0x50` | 80 | ProposalNotFound | Governance proposal does not exist | "Proposal not found." |
| `0x51` | 81 | ProposalExpired | Governance proposal is past its execution deadline | "This proposal has expired." |
| `0x52` | 82 | GovInvalidState | Governance proposal cannot transition from its current state | "This proposal can't be actioned right now." |
| `0x53` | 83 | GovUnauthorized | Caller is not authorized by the configured governance policy | "You're not authorized to perform this governance action." |
| `0x54` | 84 | IllegalPhaseTransition | Requested action is not valid in the round's current lifecycle phase | "This action isn't available at this stage of the round." |
| `0x55` | 85 | OracleHeartbeatUnhealthy | Oracle heartbeat failed the configured freshness/health policy | "Price feed health check failed. Please try again shortly." |
| `0x56` | 86 | PendingWinningsNotExpired | Pending winnings haven't reached the configured expiry threshold yet | "These winnings can't be reclaimed yet — please wait." |
| `0x57` | 87 | ClaimBatchTooLarge | `claim_many` batch size exceeds the configured maximum | "Too many claims at once — please split into smaller batches." |
| `0x58` | 88 | DuplicateClaimAddress | `claim_many` batch contains the same address more than once | "Duplicate address in claim batch." |
| `0x5b` | 91 | DisputeWindowExpired | The dispute window for `void_round` has expired, or dispute windows aren't configured | "The dispute window for this round has closed." |
| `0x5c` | 92 | ClaimLocked | `finalize_round` was called before the dispute window elapsed | "Winnings are locked until the dispute window closes." |
| `0x5d` | 93 | RoundStartLedgerReused | A round can't be created because the ledger sequence already backs another round's start | "Please try creating the round again in a moment." |
| `0x5e` | 94 | PageSizeExceeded | Pagination limit exceeds the maximum page size | "Requested too many results at once — please reduce the page size." |
| `0x5f` | 95 | EarlyCashoutDisabled | Early cash-out is disabled or its penalty rate isn't configured | "Early cash-out isn't available right now." |
| `0x60` | 96 | PositionNotFound | User has no active position in this round to cash out | "No active bet found to cash out." |
| `0x61` | 97 | InvalidPhaseForCashout | Early cash-out attempted outside the valid running phase | "Early cash-out is only available while the round is running." |
| `0x62` | 98 | WrongModeForCashout | Early cash-out is only supported for UpDown rounds | "Early cash-out isn't available for Precision rounds." |
| `0x63` | 99 | InsuranceInvalidSplit | A proposed insurance payout split doesn't sum to the covered balance | "Invalid insurance payout configuration." |
| `0x64` | 100 | InsuranceInsufficientFund | The insurance backstop fund has insufficient balance to cover the claim | "Insurance fund can't cover this claim right now." |
| `0x65` | 101 | InvalidAmount | The supplied token amount is invalid for the requested operation | "Invalid amount entered." |

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

### 2. Displaying user-friendly messages in a React UI
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
    expect(ContractErrorDecoder(1)).to.equal("This contract is already set up.");
    expect(ContractErrorDecoder(22)).to.equal("The platform is temporarily paused. Please try again later.");
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
Add the script `scripts/check-doc-drift.js` to your CI pipeline. If the script exits with a non-zero status, the CI job fails, prompting a documentation update before merging.

---
*Last updated: 2026-09-28*
