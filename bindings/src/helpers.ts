import { Client } from "./index.js";
import type {
  AssembledTransaction,
  MethodOptions,
} from "@stellar/stellar-sdk/contract";
import type { i128 } from "@stellar/stellar-sdk/contract";

// Re-export BetSide from the generated client for convenience
import type { BetSide } from "./index.js";
import { RoundPhase } from "./index.js";

// ─── Typed Exceptions ──────────────────────────────────────────

export class XelmaError extends Error {
  constructor(
    message: string,
    public readonly code?: number,
    public readonly variant?: string,
  ) {
    super(message);
    this.name = "XelmaError";
  }
}

export class InsufficientBalanceError extends XelmaError {
  constructor() {
    super("User has insufficient vXLM balance to place a bet", 9, "InsufficientBalance");
    this.name = "InsufficientBalanceError";
  }
}

export class NoActiveRoundError extends XelmaError {
  constructor() {
    super("No active round is available for betting", 7, "NoActiveRound");
    this.name = "NoActiveRoundError";
  }
}

export class ContractPausedError extends XelmaError {
  constructor() {
    super("Contract is currently paused — no betting allowed", 22, "ContractPaused");
    this.name = "ContractPausedError";
  }
}

export class AlreadyBetError extends XelmaError {
  constructor() {
    super("User has already placed a bet in the current round", 10, "AlreadyBet");
    this.name = "AlreadyBetError";
  }
}

export class StakeExceedsMaxError extends XelmaError {
  constructor() {
    super("Bet amount exceeds the configured maximum stake", 28, "StakeExceedsMax");
    this.name = "StakeExceedsMaxError";
  }
}

export class ExposureCapExceededError extends XelmaError {
  constructor() {
    super("User's cumulative exposure in this round exceeds the configured cap", 29, "ExposureCapExceeded");
    this.name = "ExposureCapExceededError";
  }
}

export class NoRoundTemplateError extends XelmaError {
  constructor() {
    super("No round template configured", 65, "NoRoundTemplate");
    this.name = "NoRoundTemplateError";
  }
}

export class AccessDeniedError extends XelmaError {
  constructor() {
    super(
      "Participant is blocked by the active access-control policy",
      79,
      "AccessDenied",
    );
    this.name = "AccessDeniedError";
  }
}

// ─── Error Mapping ─────────────────────────────────────────────

const ERROR_CODE_TO_CLASS: Record<number, new () => XelmaError> = {
  7: NoActiveRoundError,
  9: InsufficientBalanceError,
  10: AlreadyBetError,
  22: ContractPausedError,
  28: StakeExceedsMaxError,
  29: ExposureCapExceededError,
  65: NoRoundTemplateError,
  79: AccessDeniedError,
};

/**
 * Wraps an unknown error thrown from a Soroban RPC call into a typed
 * `XelmaError` subclass when the error contains a recognized contract
 * error code.  Unrecognized / non-contract errors pass through unchanged.
 */
export function wrapContractError(err: unknown): Error {
  if (err instanceof XelmaError) return err;

  let code: number | undefined;

  if (err && typeof err === "object") {
    if ("code" in err && typeof (err as Record<string, unknown>).code === "number") {
      code = (err as Record<string, number>).code;
    }
    if (code === undefined && "message" in err) {
      const msg = String((err as Record<string, unknown>).message);
      const match = msg.match(/contract error code (\d+)/);
      if (match) code = parseInt(match[1], 10);
    }
  }

  if (code !== undefined && ERROR_CODE_TO_CLASS[code]) {
    return new (ERROR_CODE_TO_CLASS[code])();
  }

  return err instanceof Error ? err : new Error(String(err));
}

// ─── Mint Helper ───────────────────────────────────────────────

export interface MintResult {
  minted: boolean;
  balance: i128;
}

/**
 * Checks a user's vXLM balance and mints the initial 1000 vXLM if the
 * balance is zero.  The contract enforces one-time mint per user.
 *
 * @example
 * const { minted, balance } = await mintIfNeeded(client, "GABCDEF123...")
 * if (minted) console.log(`Minted initial tokens. New balance: ${balance}`)
 */
export async function mintIfNeeded(
  client: Client,
  user: string,
  options?: MethodOptions,
): Promise<MintResult> {
  const { result: balance } = await client.balance({ user }, options);

  if (BigInt(balance) > 0n) {
    return { minted: false, balance };
  }

  await client.mint_initial({ user }, options).then((tx) => tx.signAndSend());

  const { result: newBalance } = await client.balance({ user }, options);
  return { minted: true, balance: newBalance };
}

// ─── Bet Helper ────────────────────────────────────────────────

export interface PlaceBetParams {
  user: string;
  amount: i128;
  side: BetSide;
}

/**
 * Places a bet with pre-flight validation:
 *   1. Contract is not paused
 *   2. An active round exists
 *   3. User has sufficient vXLM balance
 *
 * Throws typed `XelmaError` subclasses for known failure modes so
 * integrators can handle errors by type rather than parsing raw codes.
 *
 * @example
 * import { placeBetChecked } from "@xelma/bindings/helpers"
 *
 * await placeBetChecked(client, {
 *   user: "GABCDEF123...",
 *   amount: 500_000_000n,  // 50 vXLM (7-digit precision)
 *   side: { tag: "Up", values: undefined },
 * })
 */
export async function placeBetChecked(
  client: Client,
  params: PlaceBetParams,
  options?: MethodOptions,
) {
  const { user, amount, side } = params;

  const { result: paused } = await client.is_paused(options);
  if (paused) throw new ContractPausedError();

  const { result: activeRound } = await client.get_active_round(options);
  if (!activeRound) throw new NoActiveRoundError();

  const { result: balance } = await client.balance({ user }, options);
  if (BigInt(balance) < BigInt(amount)) throw new InsufficientBalanceError();

  const betTx = await client.place_bet({ user, amount, side }, options);
  try {
    return await betTx.signAndSend();
  } catch (err) {
    throw wrapContractError(err);
  }
}

// ─── Claim Helper ──────────────────────────────────────────────

export interface ClaimResult {
  claimed: boolean;
  amount?: i128;
}

/**
 * Checks for pending winnings and automatically claims them if the
 * balance is greater than zero.
 *
 * @example
 * const { claimed, amount } = await claimIfPending(client, "GABCDEF123...")
 * if (claimed) console.log(`Claimed ${amount} vXLM`)
 */
export async function claimIfPending(
  client: Client,
  user: string,
  options?: MethodOptions,
): Promise<ClaimResult> {
  const { result: pending } = await client.get_pending_winnings({ user }, options);

  if (BigInt(pending) > 0n) {
    const claimTx = await client.claim_winnings({ user }, options);
    await claimTx.signAndSend();
    return { claimed: true, amount: pending };
  }

  return { claimed: false };
}

// ─── Simulate Helper ───────────────────────────────────────────

export interface SimulateBetResult {
  simulated: boolean;
  minResourceFee?: string;
  error?: Error;
}

/**
 * Simulates placing a bet without broadcasting to inspect expected outcome,
 * resource fee estimation, and decode any contract errors.
 *
 * @example
 * const { simulated, minResourceFee, error } = await simulateBet(client, {
 *   user: "GABCDEF123...",
 *   amount: 500_000_000n,
 *   side: { tag: "Up", values: undefined },
 * })
 */
export async function simulateBet(
  client: Client,
  params: PlaceBetParams,
  options?: MethodOptions,
): Promise<SimulateBetResult> {
  try {
    const betTx = await client.place_bet(params, options);
    const simulatedTx = await betTx.simulate();
    return {
      simulated: true,
      minResourceFee: (simulatedTx as any).minResourceFee,
    };
  } catch (err) {
    return {
      simulated: false,
      error: wrapContractError(err),
    };
  }
}

// ─── Round Phase Decoding ──────────────────────────────────────

/** Variant name of a `RoundPhase`, or `"Unknown"` for a value this version does not recognise. */
export type RoundPhaseKey = keyof typeof RoundPhase | "Unknown";

export interface RoundPhaseInfo {
  /** The decoded phase, or `undefined` when the value is not a known `RoundPhase`. */
  phase: RoundPhase | undefined;
  /** Variant name, e.g. `"Betting"`. */
  key: RoundPhaseKey;
  /** Short user-facing label for wallet UIs. */
  label: string;
  /** One-line user-facing explanation of what the user can do now. */
  description: string;
  /** Whether new bets are accepted (`Betting` only). */
  canBet: boolean;
  /** Whether early cash-out may be offered (`Running` only; UpDown rounds). */
  canCashOut: boolean;
  /** Whether the round is waiting for the oracle to resolve it (`Resolvable` only). */
  awaitingResolution: boolean;
}

/**
 * User-facing strings for each `RoundPhase`, keyed by the contract's `u32`
 * discriminant. The boundaries mirror `get_round_phase` / `_derive_round_phase`:
 *   - `Betting`    while `ledger < bet_end_ledger`
 *   - `Running`    while `bet_end_ledger <= ledger < end_ledger`
 *   - `Resolvable` when  `ledger >= end_ledger`
 */
export const ROUND_PHASE_LABELS: Readonly<
  Record<RoundPhase, { key: keyof typeof RoundPhase; label: string; description: string }>
> = Object.freeze({
  [RoundPhase.Betting]: {
    key: "Betting",
    label: "Betting open",
    description: "Bets are open for this round.",
  },
  [RoundPhase.Running]: {
    key: "Running",
    label: "Round in progress",
    description: "Betting is closed. The round is running until its end ledger.",
  },
  [RoundPhase.Resolvable]: {
    key: "Resolvable",
    label: "Awaiting result",
    description: "The round has ended and is waiting for the oracle to resolve it.",
  },
});

const UNKNOWN_PHASE_LABEL = "Unknown phase";
const UNKNOWN_PHASE_DESCRIPTION =
  "This round phase is not recognised by this version of the bindings.";

/**
 * Normalises a phase as it may arrive in a wallet: the generated `RoundPhase`
 * enum, its raw `u32` discriminant (number, bigint, or numeric string), or its
 * variant name (case-insensitive). Returns `undefined` for anything else.
 */
export function parseRoundPhase(value: unknown): RoundPhase | undefined {
  let n: number | undefined;
  if (typeof value === "number") {
    n = value;
  } else if (typeof value === "bigint") {
    n = value <= 0xffffffffn && value >= 0n ? Number(value) : undefined;
  } else if (typeof value === "string") {
    const trimmed = value.trim();
    if (/^\d+$/.test(trimmed)) {
      n = Number(trimmed);
    } else {
      const match = (Object.keys(ROUND_PHASE_LABELS) as unknown as string[])
        .map(Number)
        .find(
          (k) =>
            ROUND_PHASE_LABELS[k as RoundPhase].key.toLowerCase() === trimmed.toLowerCase(),
        );
      n = match;
    }
  }
  if (n === undefined || !Number.isInteger(n)) return undefined;
  return Object.prototype.hasOwnProperty.call(ROUND_PHASE_LABELS, n)
    ? (n as RoundPhase)
    : undefined;
}

/**
 * Decodes a `RoundPhase` (from `client.get_round_phase()` or a raw `u32`) into
 * user-facing strings and capability flags for wallets. Unknown values never
 * throw: they decode to `key: "Unknown"` so a UI can still render something
 * when the contract adds a phase the bindings do not know yet.
 *
 * @example
 * const { result } = await client.get_round_phase()
 * const info = decodeRoundPhase(result.unwrap())
 * badge.textContent = info.label        // "Betting open"
 * betButton.disabled = !info.canBet
 */
export function decodeRoundPhase(value: unknown): RoundPhaseInfo {
  const phase = parseRoundPhase(value);
  if (phase === undefined) {
    return {
      phase: undefined,
      key: "Unknown",
      label: UNKNOWN_PHASE_LABEL,
      description: UNKNOWN_PHASE_DESCRIPTION,
      canBet: false,
      canCashOut: false,
      awaitingResolution: false,
    };
  }
  const { key, label, description } = ROUND_PHASE_LABELS[phase];
  return {
    phase,
    key,
    label,
    description,
    canBet: phase === RoundPhase.Betting,
    canCashOut: phase === RoundPhase.Running,
    awaitingResolution: phase === RoundPhase.Resolvable,
  };
}

/** Shorthand for `decodeRoundPhase(value).label`. */
export function roundPhaseLabel(value: unknown): string {
  return decodeRoundPhase(value).label;
}
