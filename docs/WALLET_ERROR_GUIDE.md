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
| `0x23` | 35 | InvalidMinParticipants | Minimum participants value is out of valid range (must be 1–10000) | "Invalid min participants."| `0x24` | 36 | InvalidOracleStatus | Oracle heartbeat status is out of range (must be 0, 1, or 2) | "Invalid oracle status." |
| `0x42` | 66 | OracleNotLive | Oracle heartbeat is not live and strict mode blocks settlement | "Oracle heartbeat not live." | `0x25` | 37 | InvalidStaleThreshold | Oracle stale threshold is out of valid range (must be 60–86400 seconds) | "Invalid stale threshold."
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
| `0x3f` | 63 | InvalidCommitment | Commitment hash is malformed (e.g. all-zero placeholder) | "Invalid commitment hash."
| `0x40` | 64 | InvalidSalt | Reveal salt fails minimum entropy rules | "Invalid reveal salt."
| `0x41` | 65 | NoRoundTemplate | No round template configured | "No round template."
| `0x5f` | 95 | EarlyCashoutDisabled | Early cash-out is disabled or penalty rate is unset | "Early cash-out is currently disabled."
| `0x60` | 96 | PositionNotFound | User has no active position in the round to cash out | "No active position found to cash out."
| `0x61` | 97 | InvalidPhaseForCashout | Early cash-out is only permitted during the running phase | "Early cash-out only available during running phase."
| `0x62` | 98 | WrongModeForCashout | Early cash-out is only supported for UpDown rounds | "Early cash-out is not supported in Precision mode."

| `0x66` | 102 | BettingClosed | Close-buffer window has frozen; betting is closed early but the round has not ended | "Betting is closed early. The round is still in progress." |

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
