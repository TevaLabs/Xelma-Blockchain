# Wallet Error Integration Guide

How wallet and frontend integrations should decode, classify, and display the
errors returned by the Xelma prediction-market contract.

This guide is generated from the live `ContractError` enum. The single source of
truth is [`contracts/src/errors.rs`](../contracts/src/errors.rs); the canonical
machine-checked registry is [`docs/CONTRACT_ERRORS.md`](./CONTRACT_ERRORS.md),
and the TypeScript mirror lives in [`bindings/src/index.ts`](../bindings/src/index.ts)
(`export const ContractError`).

Two scripts keep this page honest — run both after touching any error code:

```bash
node scripts/check-doc-drift.js   # this page <-> contracts/src/errors.rs
node bindings/src/parity.js       # Rust enum <-> TS map <-> docs/CONTRACT_ERRORS.md
```

**Coverage:** all 81 variants currently declared in `ContractError`
(highest discriminant `102`).

## How a contract error reaches a wallet

1. The contract returns `Result<_, ContractError>`. Soroban encodes the
   `#[repr(u32)]` discriminant, so the host reports a `ScError::Contract(u32)`.
2. The RPC/SDK surfaces that as an `InvokeHostFunction` failure whose result XDR
   carries `error.result.xdr.value.val.code` — the decimal value in the table
   below.
3. Wallets map that integer to the variant, then to a message. Never match on
   message text: the numeric code is the stable ABI. Codes are never reused, and
   gaps in the range are intentional.

Unknown codes must be handled gracefully — a newer contract may emit an error
this guide has not seen yet:

```ts
import { decodeContractError } from "@xelma/bindings";

const decoded = decodeContractError(code);
const message = decoded?.message ?? `Unknown contract error (code ${code})`;
```

## Error Table

| Hex Code | Decimal | Enum Identifier | Technical Meaning | Consumer-Facing Message |
|----------|---------|-----------------|-------------------|-------------------------|
| `0x01` | 1 | `AlreadyInitialized` | `initialize` was already called; admin/oracle is pinned | This market contract is already set up. |
| `0x02` | 2 | `AdminNotSet` | No admin address is stored for this contract | Contract is not configured yet (admin missing). |
| `0x03` | 3 | `OracleNotSet` | No oracle address is stored for this contract | Contract is not configured yet (oracle missing). |
| `0x06` | 6 | `InvalidBetAmount` | Bet amount is zero, negative, or otherwise unusable | Enter a bet amount greater than zero. |
| `0x07` | 7 | `NoActiveRound` | There is no open round to act on | There is no open round right now. Try again shortly. |
| `0x08` | 8 | `RoundEnded` | The round's betting window has already closed | Betting for this round has closed. |
| `0x09` | 9 | `InsufficientBalance` | Caller's token balance is below the requested amount | You do not have enough balance for this transaction. |
| `0x0a` | 10 | `AlreadyBet` | Caller already holds a position in the current round | You already placed a bet in this round. |
| `0x0b` | 11 | `Overflow` | An arithmetic operation overflowed and was rejected | The amount is too large to process. |
| `0x0c` | 12 | `InvalidPrice` | Price value failed validation | The supplied price is not valid. |
| `0x0d` | 13 | `InvalidDuration` | Round duration is outside the allowed range | The round duration is not valid. |
| `0x0e` | 14 | `InvalidMode` | Round mode is not `UpDown` (0) or `Precision` (1) | Unsupported round mode. |
| `0x0f` | 15 | `WrongModeForPrediction` | Prediction type does not match the round's mode | This round does not accept that kind of prediction. |
| `0x10` | 16 | `RoundNotEnded` | Settlement was attempted before `end_ledger` | The round has not ended yet. |
| `0x12` | 18 | `StaleOracleData` | Oracle price is older than the configured staleness threshold | The oracle price is stale. Resolution is paused. |
| `0x13` | 19 | `InvalidOracleRound` | Oracle payload `round_id` does not match the active round | Oracle report does not match this round. |
| `0x14` | 20 | `RoundAlreadyActive` | A round is already active and cannot be replaced | A round is already in progress. |
| `0x16` | 22 | `ContractPaused` | A runtime mode (`ClaimsOnly` / `FullyPaused`) blocks this action | The protocol is paused for this action. Only claims may be available. |
| `0x17` | 23 | `WindowOutOfRange` | Bet/run window exceeds configured maximum bounds | The requested timing window is out of range. |
| `0x18` | 24 | `FutureOracleData` | Oracle payload timestamp is ahead of the ledger clock | The oracle report is from the future and was rejected. |
| `0x19` | 25 | `PayoutOverflow` | Payout accumulation overflowed; no funds were moved | The payout could not be calculated safely. Nothing was charged. |
| `0x1b` | 27 | `RoundNotCancellable` | Round cannot be cancelled in its current state | This round cannot be cancelled. |
| `0x1c` | 28 | `StakeExceedsMax` | Bet exceeds the configured per-bet maximum stake | Your bet is above the maximum stake for this market. |
| `0x1d` | 29 | `ExposureCapExceeded` | Caller's cumulative exposure in the round exceeds the cap | You have reached the exposure limit for this round. |
| `0x1e` | 30 | `PendingWinningsCapExceeded` | Crediting winnings would exceed the pending-winnings cap | Your pending winnings limit was reached. Claim before betting again. |
| `0x1f` | 31 | `InvalidStartPrice` | Start price is outside the allowed bounds | The round start price is out of range. |
| `0x21` | 33 | `OracleNonceReused` | Oracle payload nonce was already consumed (replay attempt) | This oracle report was already submitted. |
| `0x23` | 35 | `InvalidMinParticipants` | `min_participants` is outside the allowed range (1–10000) | Invalid minimum-participant setting. |
| `0x26` | 38 | `InvalidPrecisionCap` | Precision participant cap is outside the allowed range (1–10000) | Invalid precision participant cap. |
| `0x27` | 39 | `PrecisionCapExceeded` | Precision round already has the maximum number of participants | This precision round is full. |
| `0x29` | 41 | `OracleDeviationExceeded` | Reported price deviates beyond the configured threshold | The oracle price moved too far and was rejected. |
| `0x2a` | 42 | `UnsupportedSchemaVersion` | Stored schema version is unknown to this contract build | This contract build does not support the stored data version. |
| `0x2c` | 44 | `MigrationActiveRound` | Migration cannot run while a round is active | Wait for the active round to finish before migrating. |
| `0x2d` | 45 | `CommitmentNotFound` | No precision commitment exists for the caller | No committed prediction was found for this address. |
| `0x2e` | 46 | `AlreadyRevealed` | Precision prediction was already revealed | This prediction was already revealed. |
| `0x2f` | 47 | `InvalidRevealWindow` | Reveal attempted outside the reveal window | Predictions can no longer be revealed for this round. |
| `0x30` | 48 | `HashMismatch` | Revealed price/salt hash does not match the stored commitment | The reveal does not match your committed prediction. |
| `0x31` | 49 | `OracleNetworkMismatch` | Network identifier in the oracle payload does not match, or admin equals oracle | Oracle report belongs to a different network. |
| `0x33` | 51 | `InvalidProtocolFeeBps` | Protocol fee bps is outside the allowed range | Invalid protocol fee configuration. |
| `0x35` | 53 | `MintLimitExceeded` | Epoch/mint limit reached for the caller | The mint limit for this period has been reached. |
| `0x36` | 54 | `NoPendingRotation` | No oracle rotation is pending | There is no pending oracle rotation. |
| `0x37` | 55 | `RotationDelayNotElapsed` | `accept_oracle_rotation` called before `MIN_ROTATION_DELAY_SECONDS` | The oracle rotation delay has not elapsed yet. |
| `0x3e` | 62 | `InvalidArchiveRetention` | Archive retention limit is out of range | Invalid archive retention configuration. |
| `0x3f` | 63 | `InvalidCommitment` | Commitment hash is malformed (for example all-zero) | Your commitment hash is not valid. |
| `0x40` | 64 | `InvalidSalt` | Reveal salt does not meet the minimum entropy rules | Your reveal salt does not meet the minimum requirements. |
| `0x41` | 65 | `NoRoundTemplate` | `create_next_from_template` called with no template configured | No round template is configured. |
| `0x42` | 66 | `OracleTimestampOutsideWindow` | Payload timestamp is outside the round-relative economic window | The oracle report is outside the accepted time window. |
| `0x43` | 67 | `EpochBudgetExceeded` | Epoch mint budget is fully consumed | The mint budget for this epoch is exhausted. |
| `0x44` | 68 | `OracleNotLive` | Heartbeat is not live and strict mode blocks settlement | Oracle heartbeat is not live. Resolution is blocked. |
| `0x45` | 69 | `InvalidPayoutPolicy` | Precision payout policy is not recognized | Unsupported payout policy. |
| `0x46` | 70 | `BelowMinBet` | Bet is below the configured minimum (dust protection) | Your bet is below the minimum allowed amount. |
| `0x47` | 71 | `InsufficientOracleQuorum` | Too few observations survived outlier rejection for quorum | Not enough oracle sources to resolve safely. |
| `0x48` | 72 | `TooFewObservations` | Multi-feed payload has fewer observations than the minimum | The oracle payload has too few observations. |
| `0x49` | 73 | `OracleOutlierRejected` | Outliers dominate the payload so no quorum can be formed | The oracle payload was rejected as inconsistent. |
| `0x4a` | 74 | `DuplicateOracleSource` | Multi-feed payload repeats a source identifier | The oracle payload contains duplicate sources. |
| `0x4b` | 75 | `InvalidObservationOrder` | Observations are not sorted or a source id is out of range | The oracle payload ordering is invalid. |
| `0x4c` | 76 | `UnsupportedDataKeyForTtlTouch` | Requested storage key is not allowlisted for batch TTL touch | This storage key cannot be TTL-touched. |
| `0x4d` | 77 | `PendingWinningsNotFound` | No pending-winnings entry exists for the caller | You have no pending winnings to claim. |
| `0x4e` | 78 | `ExpiryNotConfigured` | Pending-winnings expiry is not configured (value is 0) | Pending winnings expiry is not configured. |
| `0x4f` | 79 | `AccessDenied` | Caller is blocked by the active allowlist/denylist policy | This address is not allowed to use the market. |
| `0x50` | 80 | `ProposalNotFound` | Governance proposal id does not exist | That governance proposal was not found. |
| `0x51` | 81 | `ProposalExpired` | Governance proposal is past its execution deadline | That governance proposal has expired. |
| `0x52` | 82 | `GovInvalidState` | Governance proposal cannot transition from its current state | The governance proposal is in the wrong state for this action. |
| `0x53` | 83 | `GovUnauthorized` | Caller is not authorized by the governance policy | You are not authorized for this governance action. |
| `0x54` | 84 | `IllegalPhaseTransition` | Action is not valid in the round's lifecycle phase | That action is not available in the round's current phase. |
| `0x55` | 85 | `OracleHeartbeatUnhealthy` | Heartbeat failed the freshness/health policy | The oracle heartbeat is unhealthy. |
| `0x56` | 86 | `PendingWinningsNotExpired` | Pending winnings exist but the expiry threshold has not passed | These pending winnings cannot be reclaimed yet. |
| `0x57` | 87 | `ClaimBatchTooLarge` | `claim_many` batch exceeds `MAX_CLAIM_BATCH_SIZE` | Too many addresses in one claim batch. |
| `0x58` | 88 | `DuplicateClaimAddress` | `claim_many` batch repeats an address | The claim batch contains a duplicate address. |
| `0x5b` | 91 | `DisputeWindowExpired` | Dispute window elapsed, or dispute windows are not configured | The dispute window for this round has closed. |
| `0x5c` | 92 | `ClaimLocked` | `finalize_round` called before the dispute window elapsed | Claims are locked until the dispute window elapses. |
| `0x5d` | 93 | `RoundStartLedgerReused` | A round already bound this ledger as its `start_ledger` | A round was just created. Retry once the ledger advances. |
| `0x5e` | 94 | `PageSizeExceeded` | Pagination limit exceeds `MAX_PAGE_SIZE` | Requested page size is too large. |
| `0x5f` | 95 | `EarlyCashoutDisabled` | Early cash-out is disabled or its penalty rate is unset | Early cash-out is not available in this market. |
| `0x60` | 96 | `PositionNotFound` | Caller has no active position in the current round | You have no position to cash out in this round. |
| `0x61` | 97 | `InvalidPhaseForCashout` | Cash-out attempted outside the round's Running phase | Early cash-out is only possible while the round is running. |
| `0x62` | 98 | `WrongModeForCashout` | Cash-out attempted on a non-`UpDown` round | Early cash-out is only supported for Up/Down rounds. |
| `0x63` | 99 | `InsuranceInvalidSplit` | Insurance payout split does not sum to the covered balance | The insurance payout split is invalid. |
| `0x64` | 100 | `InsuranceInsufficientFund` | Insurance backstop cannot cover the requested payout | The insurance fund cannot cover this claim right now. |
| `0x65` | 101 | `InvalidAmount` | Token amount is invalid for the requested operation | The supplied amount is not valid for this operation. |
| `0x66` | 102 | `BettingClosed` | Close-buffer has frozen betting before `end_ledger` | Betting is closed early for this round. |

Codes absent from the table (4, 5, 17, 21, 26, 32, 34, 36, 37, 40, 43, 50, 52,
56–61, 89, 90) are retired or reserved discriminants. Treat them as unknown
instead of reusing them for wallet-specific semantics.

## Integration Walkthroughs

### 1. Handling errors in a Freighter-style wallet

```ts
import { ContractErrorDecoder } from "@xelma/bindings";

function handleError(error: any) {
  const code = error?.result?.xdr?.value?.val?.code ?? 0;
  const message = ContractErrorDecoder(code);
  // -> "Unknown contract error code 999" for codes outside the table.
  alert(message);
}
```

The bindings also expose structured helpers:

```ts
import { decodeContractError, formatContractError } from "@xelma/bindings";

const decoded = decodeContractError(code); // { code, variant, message } | null
const text = formatContractError(code);    // "AlreadyInitialized (code 1)"
```

`decodeContractError` returns `null` for codes the bindings do not know yet.
`formatContractError` and `ContractErrorDecoder` fall back to an
`Unknown contract error ...` string, so wallets never render `undefined`.

### 2. Displaying user-friendly messages in a React UI

The bindings map currently returns the variant name (`"InsufficientBalance"`),
so product copy belongs in a wallet-owned layer keyed by the numeric code. The
"Consumer-Facing Message" column above is the recommended default for each code:

```tsx
import { decodeContractError } from "@xelma/bindings";

const WALLET_COPY: Record<number, string> = {
  9: "You do not have enough balance for this transaction.",
  22: "The protocol is paused for this action. Only claims may be available.",
  70: "Your bet is below the minimum allowed amount.",
};

function ErrorBanner({ code }: { code: number }) {
  const known = decodeContractError(code);
  const text = WALLET_COPY[code] ?? known?.message ?? `Unknown error (code ${code})`;
  return <div className="error" role="alert">{text}</div>;
}
```

### 3. Retry vs. terminal classification

Decide whether to offer a retry from the code, never from the message:

| Class | Codes | Wallet behaviour |
|-------|-------|------------------|
| Transient / retryable | `NoActiveRound` (7), `RoundNotEnded` (16), `StaleOracleData` (18), `OracleNotLive` (68), `OracleHeartbeatUnhealthy` (85), `PendingWinningsNotExpired` (86), `DisputeWindowExpired` (91), `ClaimLocked` (92), `RoundStartLedgerReused` (93), `RotationDelayNotElapsed` (55) | Offer "try again" once the round or ledger advances. |
| Policy / operational | `ContractPaused` (22), `AccessDenied` (79), `IllegalPhaseTransition` (84), `EarlyCashoutDisabled` (95), `BettingClosed` (102) | Explain the current protocol state; do not retry automatically. |
| User-actionable | `InsufficientBalance` (9), `AlreadyBet` (10), `InvalidBetAmount` (6), `BelowMinBet` (70), `StakeExceedsMax` (28), `ExposureCapExceeded` (29), `PendingWinningsCapExceeded` (30), `PositionNotFound` (96), `InvalidPhaseForCashout` (97), `WrongModeForCashout` (98) | Show inline validation and let the user correct the input. |
| Programmer / integration bug | `Overflow` (11), `PayoutOverflow` (25), `HashMismatch` (48), `InvalidCommitment` (63), `InvalidSalt` (64), `UnsupportedSchemaVersion` (42), `InvalidAmount` (101) | Log the code with the transaction hash; do not blame the user. |

### 4. Unit testing your error mapping

```ts
import { ContractError, ContractErrorDecoder } from "@xelma/bindings";
import { expect } from "chai";

describe("ContractErrorDecoder", () => {
  it("maps known codes", () => {
    expect(ContractErrorDecoder(1)).to.equal("AlreadyInitialized");
    expect(ContractErrorDecoder(22)).to.equal("ContractPaused");
    expect(ContractErrorDecoder(102)).to.equal("BettingClosed");
  });

  it("handles unknown codes", () => {
    expect(ContractErrorDecoder(999)).to.match(/Unknown contract error/);
  });

  it("covers every enum variant", () => {
    for (const [code, entry] of Object.entries(ContractError)) {
      expect(ContractErrorDecoder(Number(code))).to.equal(entry.message);
    }
  });
});
```

### 5. Automated UI test with Selenium (JavaScript)

```js
await driver.findElement(By.id("betButton")).click();
await driver.wait(until.elementLocated(By.css(".error")));
const errorText = await driver.findElement(By.css(".error")).getText();
assert.include(errorText, "enough balance");
```

### 6. Keeping this page in sync

`scripts/check-doc-drift.js` parses the enum out of `contracts/src/errors.rs`
and the table above, then fails when:

- a variant has no row here (missing documentation), or
- a row names a code or variant that the contract no longer declares (stale
  row, e.g. the retired `UnauthorizedAdmin`/`StartPriceTooLow` entries), or
- a row's decimal and hex columns disagree.

`bindings/src/parity.js` additionally cross-checks the TypeScript
`ContractError` map and `docs/CONTRACT_ERRORS.md`, so Rust, TypeScript and the
documented registry all agree. When an error is added, renamed, renumbered or
removed, update `contracts/src/errors.rs`, `bindings/src/index.ts`,
`docs/CONTRACT_ERRORS.md` and this page together, then run both scripts.

---
*Last updated: 2026-11-25 — regenerated from `contracts/src/errors.rs`
(81 variants, highest discriminant 102).*
