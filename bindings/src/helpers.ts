import { Client } from "./index.js";
import type {
  AssembledTransaction,
  MethodOptions,
} from "@stellar/stellar-sdk/contract";
import type { i128 } from "@stellar/stellar-sdk/contract";

// Re-export BetSide from the generated client for convenience
import type { BetSide } from "./index.js";
import type { SeasonArchive, SeasonLeaderboardEntry } from "./index.js";

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

// ─── Leaderboard Season Helpers ────────────────────────────────

export type SeasonRankingMetric = "wins" | "streak";

/**
 * Fetches the top `topN` entries of `seasonId`'s leaderboard, ranked by
 * either total wins or best streak.
 *
 * Transparently serves the live ranking if `seasonId` is the currently
 * active season, or the frozen archive snapshot once it has been rotated
 * out — callers never need to know which. Unknown/future season ids
 * resolve to an empty array rather than throwing.
 *
 * @example
 * const top10 = await getSeasonTopN(client, 3, 10, "wins")
 */
export async function getSeasonTopN(
  client: Client,
  seasonId: number,
  topN: number,
  metric: SeasonRankingMetric = "wins",
  options?: MethodOptions,
): Promise<Array<SeasonLeaderboardEntry>> {
  const call =
    metric === "wins"
      ? client.get_season_leaderboard_by_wins(
          { season_id: seasonId, offset: 0, limit: topN },
          options,
        )
      : client.get_season_leaderboard_by_streak(
          { season_id: seasonId, offset: 0, limit: topN },
          options,
        );
  const { result } = await call;
  return result;
}

/**
 * Fetches the top `topN` entries of the *currently active* season's
 * leaderboard without the caller needing a separate round-trip to look up
 * the active season id first.
 *
 * @example
 * const { seasonId, entries } = await getCurrentSeasonTopN(client, 10, "streak")
 */
export async function getCurrentSeasonTopN(
  client: Client,
  topN: number,
  metric: SeasonRankingMetric = "wins",
  options?: MethodOptions,
): Promise<{ seasonId: number; entries: Array<SeasonLeaderboardEntry> }> {
  const { result: seasonId } = await client.get_current_season_id(options);
  const entries = await getSeasonTopN(client, seasonId, topN, metric, options);
  return { seasonId, entries };
}

export interface SeasonRolloverResult {
  endedSeasonId: number;
  newSeasonId: number;
}

/**
 * Rotates the active leaderboard season: freezes the ending season's
 * rankings into a permanent archive and advances to a new, empty season.
 * Admin-only — the signing key must be the contract admin.
 *
 * @example
 * const { endedSeasonId, newSeasonId } = await rolloverSeason(client)
 */
export async function rolloverSeason(
  client: Client,
  options?: MethodOptions,
): Promise<SeasonRolloverResult> {
  const { result: endedSeasonId } = await client.get_current_season_id(options);
  const tx = await client.reset_leaderboard_season(options);
  const { result } = await tx.signAndSend();
  return { endedSeasonId, newSeasonId: result.unwrap() };
}

/**
 * Fetches a full demo-friendly snapshot of a past, archived season: the
 * frozen wins/streak top-N rankings plus participant count and the ledger
 * the season ended on. Returns `null` if the season was never archived
 * (still active, or an id that never existed).
 *
 * @example
 * const summary = await getSeasonSummary(client, 2)
 */
export async function getSeasonSummary(
  client: Client,
  seasonId: number,
  options?: MethodOptions,
): Promise<SeasonArchive | null> {
  const { result } = await client.get_season_archive({ season_id: seasonId }, options);
  return result ?? null;
}
