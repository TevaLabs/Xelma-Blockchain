# bindings JS

JS library for interacting with [Soroban](https://soroban.stellar.org/) smart contract `bindings` via Soroban RPC.

`src/index.ts` is generated from the contract WASM. The checked-in
`src/helpers.ts` module is intentionally hand-maintained and provides typed
convenience wrappers for common wallet flows.

## Generate bindings

Install the [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/stellar-cli),
install the bindings dependencies, and build the contract first:

```bash
cargo rustc --manifest-path=contracts/Cargo.toml \
  --crate-type=cdylib --target=wasm32v1-none --release --locked
npm --prefix bindings ci
npm --prefix bindings run generate \
  -- --wasm target/wasm32v1-none/release/xelma_contract.wasm
```

To verify that the committed generated file matches a WASM artifact without
rewriting it, run:

```bash
npm --prefix bindings run check:generated
```

The script uses `WASM_PATH` when set; otherwise it reads the default release
artifact path shown above. CI runs this check against the WASM built in the
same workflow. Regenerate and commit `bindings/src/index.ts` whenever the
contract ABI changes.

# To publish or not to publish

This library is suitable for publishing to NPM. You can publish it to NPM using the `npm publish` command.

But you don't need to publish this library to NPM to use it. You can add it to your project's `package.json` using a file path:

```json
"dependencies": {
  "bindings": "./path/to/this/folder"
}
```

However, we've actually encountered [frustration](https://github.com/stellar/soroban-example-dapp/pull/117#discussion_r1232873560) using local libraries with NPM in this way. Though it seems a bit messy, we suggest generating the library directly to your `node_modules` folder automatically after each install by using a `postinstall` script. We've had the least trouble with this approach. NPM will automatically remove what it sees as erroneous directories during the `install` step, and then regenerate them when it gets to your `postinstall` step, which will keep the library up-to-date with your contract.

```json
"scripts": {
  "postinstall": "soroban contract bindings ts --rpc-url INSERT_RPC_URL_HERE --network-passphrase \"INSERT_NETWORK_PASSPHRASE_HERE\" --id INSERT_CONTRACT_ID_HERE --name bindings"
}
```

Obviously you need to adjust the above command based on the actual command you used to generate the library.

# Use it

Now that you have your library up-to-date and added to your project, you can import it in a file and see inline documentation for all of its exported methods:

```js
import { Contract, networks } from "bindings"

const contract = new Contract({
  ...networks.futurenet, // for example; check which networks this library exports
  rpcUrl: '...', // use your own, or find one for testing at https://soroban.stellar.org/docs/reference/rpc#public-rpc-providers
})

contract.|
```

As long as your editor is configured to show JavaScript/TypeScript documentation, you can pause your typing at that `|` to get a list of all exports and inline-documentation for each. It exports a separate [async](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements/async_function) function for each method in the smart contract, with documentation for each generated from the comments the contract's author included in the original source code.

# Decode contract errors

When a transaction fails, the contract returns a `u32` error code. Use the helpers
below to translate that code into a structured result or a user-facing string
suitable for wallet UX.

```ts
import { decodeContractError, formatContractError } from "bindings"

// Structured result for programmatic handling
const decoded = decodeContractError(79)
if (decoded) {
  console.log(`Error ${decoded.code}: ${decoded.variant} — ${decoded.message}`)
  // => "Error 79: AccessDenied — AccessDenied"
}

// One-liner for display in a wallet or bot
console.log(formatContractError(9) ?? "Unknown error")
// => "InsufficientBalance (code 9)"
```

Use `decodeContractError` to surface contextual information in your wallet or
bot, and fall back to `"Unknown contract error (code N)"` for codes that the
current bindings do not yet recognise.

# High-level helpers

The `@xelma/bindings/helpers` module provides typed convenience wrappers
around the generated client for common wallet flows.

## `mintIfNeeded`

Checks a user's vXLM balance and mints initial tokens if the balance is zero.
One-time per user (enforced by the contract).

```ts
import { mintIfNeeded } from "@xelma/bindings/helpers"

const { minted, balance } = await mintIfNeeded(client, "GABCDEF123...")
if (minted) {
  console.log(`Minted initial tokens. Balance: ${balance}`)
}
```

## `placeBetChecked`

Places a bet after running pre-flight validation:
1. Contract is not paused
2. An active round exists
3. User has sufficient balance

Throws typed exceptions for known failure modes.

```ts
import { placeBetChecked } from "@xelma/bindings/helpers"

try {
  const tx = await placeBetChecked(client, {
    user: "GABCDEF123...",
    amount: 500_000_000n,   // 50 vXLM (7-digit precision)
    side: { tag: "Up", values: undefined },
  })
  console.log("Bet placed!")
} catch (err) {
  if (err instanceof ContractPausedError) {
    console.log("Betting is paused right now.")
  } else if (err instanceof InsufficientBalanceError) {
    console.log("Not enough vXLM — try minting first.")
  } else if (err instanceof NoActiveRoundError) {
    console.log("No round is currently active.")
  } else {
    console.error("Unexpected error:", err)
  }
}
```

## `claimIfPending`

Checks for pending winnings and claims them if any exist.

```ts
import { claimIfPending } from "@xelma/bindings/helpers"

const { claimed, amount } = await claimIfPending(client, "GABCDEF123...")
if (claimed) {
  console.log(`Claimed ${amount} vXLM`)
}
```

## `simulateBet`

Simulates a bet without broadcasting to estimate resource fees and decode errors.

```ts
import { simulateBet } from "@xelma/bindings/helpers"

const { simulated, minResourceFee, error } = await simulateBet(client, {
  user: "GABCDEF123...",
  amount: 500_000_000n,
  side: { tag: "Up", values: undefined },
})
if (simulated) {
  console.log(`Estimated min resource fee: ${minResourceFee}`)
} else {
  console.error("Simulation failed:", error)
}
```

## Error reference

| Exception                  | Contract code | Meaning                          |
|----------------------------|---------------|----------------------------------|
| `InsufficientBalanceError` | 9             | Not enough vXLM to place bet     |
| `NoActiveRoundError`       | 7             | No round is currently active     |
| `ContractPausedError`      | 22            | Contract paused for maintenance  |
| `AlreadyBetError`          | 10            | User already bet in this round   |
| `StakeExceedsMaxError`     | 28            | Bet exceeds the max stake cap    |
| `ExposureCapExceededError` | 29            | User exposure exceeds round cap  |
| `NoRoundTemplateError`     | 65            | No round template configured     |

All typed exceptions extend `XelmaError` (which extends `Error`), so you
can catch specific types or the base class.
