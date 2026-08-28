import { Buffer } from "buffer";
import { Address } from "@stellar/stellar-sdk";
import {
  AssembledTransaction,
  Client as ContractClient,
  ClientOptions as ContractClientOptions,
  MethodOptions,
  Result,
  Spec as ContractSpec,
} from "@stellar/stellar-sdk/contract";
import type {
  u32,
  i32,
  u64,
  i64,
  u128,
  i128,
  u256,
  i256,
  Option,
  Timepoint,
  Duration,
} from "@stellar/stellar-sdk/contract";
export * from "@stellar/stellar-sdk";
export * as contract from "@stellar/stellar-sdk/contract";
export * as rpc from "@stellar/stellar-sdk/rpc";

if (typeof window !== "undefined") {
  //@ts-ignore Buffer exists
  window.Buffer = window.Buffer || Buffer;
}





export interface Round {
  bet_end_ledger: u32;
  end_ledger: u32;
  mode: RoundMode;
  pool_down: i128;
  pool_up: i128;
  price_start: u128;
  round_id: u64;
  start_ledger: u32;
}

/**
 * Represents which side a user bet on
 */
export type BetSide = {tag: "Up", values: void} | {tag: "Down", values: void};

/**
 * Storage keys for contract data
 * 
 * ## Indexed position keys (variants 13–15)
 * 
 * `Position(round_id, address)` and `PrecisionPosition(round_id, address)` store
 * a single user's record under a composite key, enabling O(1) read/write per user
 * instead of deserializing the full participant map on every bet.
 * 
 * `RoundParticipants(round_id)` holds the ordered `Vec<Address>` used for
 * iteration at resolution time. Appending one address is cheaper than
 * re-serialising an N-entry `Map<Address, T>` for every bet placed.
 * 
 * Legacy single-key maps (`UpDownPositions`, `PrecisionPositions`) are kept for
 * backward-compatible reads during a migration window; they are no longer written.
 */
export type DataKey = {tag: "Balance", values: readonly [string]} | {tag: "Admin", values: void} | {tag: "Oracle", values: void} | {tag: "SchemaVersion", values: void} | {tag: "ActiveRound", values: void} | {tag: "Positions", values: void} | {tag: "UpDownPositions", values: void} | {tag: "PrecisionPositions", values: void} | {tag: "PendingWinnings", values: readonly [string]} | {tag: "UserStats", values: readonly [string]} | {tag: "Paused", values: void} | {tag: "BetWindowLedgers", values: void} | {tag: "RunWindowLedgers", values: void} | {tag: "LastRoundId", values: void} | {tag: "Position", values: readonly [u64, string]} | {tag: "PrecisionPosition", values: readonly [u64, string]} | {tag: "PrecisionCommitment", values: readonly [u64, string]} | {tag: "RoundParticipants", values: readonly [u64]} | {tag: "MaxStake", values: void} | {tag: "MaxUserRoundExposure", values: void} | {tag: "MaxPendingWinnings", values: void} | {tag: "CancelledRound", values: readonly [u64]} | {tag: "ConsumedOracleNonce", values: readonly [u64, u64]} | {tag: "MinParticipants", values: void} | {tag: "OracleHeartbeat", values: void} | {tag: "OracleStaleThreshold", values: void} | {tag: "MaxPrecisionParticipants", values: void} | {tag: "OracleMaxDeviationBps", values: void} | {tag: "OracleDeviationOverrideArmed", values: void} | {tag: "ArchivedRound", values: readonly [u64]} | {tag: "RecentArchivedRoundIds", values: void} | {tag: "PendingConfigChange", values: readonly [ConfigChangeKind]} | {tag: "ProtocolFeeBps", values: void} | {tag: "ProtocolFeeTreasury", values: void} | {tag: "FeeModel", values: void} | {tag: "CloseBufferLedgers", values: void};

/**
 * Round mode for prediction type
 */
export enum RoundMode {
  UpDown = 0,
  Precision = 1,
}


/**
 * Lifecycle phase of an active round, derived from ledger windows.
 */
export enum RoundPhase {
  Betting = 1,
  Running = 2,
  Resolvable = 3,
}


export interface UserStats {
  best_streak: u32;
  current_streak: u32;
  total_losses: u32;
  total_wins: u32;
}

/**
 * Runtime mode for the contract lifecycle
 */
export enum RuntimeMode {
  Normal = 0,
  ClaimsOnly = 1,
  FullyPaused = 2,
}

export enum ProtocolStatus {
  Active = 0,
  Paused = 1,
  ClaimsOnly = 2,
}

export enum RoundStatus {
  Unknown = 0,
  Betting = 1,
  Running = 2,
  AwaitingResolve = 3,
  Resolved = 4,
  Cancelled = 5,
  FallbackRefund = 6,
}

export enum UserOutcomeType {
  Win = 0,
  Loss = 1,
  Refund = 2,
  Cancel = 3,
}

export interface UserRoundOutcome {
  user: string;
  round_mode: u32;
  prediction_side: u32;
  predicted_price: u128;
  stake: i128;
  payout: i128;
  outcome: UserOutcomeType;
}

/**
 * Protocol fee incidence model — determines what portion of the pot is taxed.
 */
export enum FeeModel {
  /** Fee is calculated on the total round pot (all stakes pooled). */
  FeeOnPot = 0,
  /** Fee is calculated only on the net winnings (profit transferred from losers). */
  FeeOnWinnings = 1,
}

export interface SimulationResult {
  mode: RoundMode;
  pool_up: i128;
  pool_down: i128;
  precision_total_stake: i128;
  fee_amount: i128;
  /** Fee incidence model used: 0 = FeeOnPot, 1 = FeeOnWinnings (Issue #268). */
  fee_model: u32;
  outcomes: Array<UserRoundOutcome>;
}

export interface OracleRotationProposal {
  new_oracle: string;
  proposed_at: u64;
  expires_at: u64;
}

export interface ProtocolHealthStatus {
  paused: boolean;
  oracle_live: boolean;
  oracle_status: u32;
  has_active_round: boolean;
  active_round_phase: u32;
  schema_version: u32;
  ledger_sequence: u32;
  ledger_timestamp: u64;
  status_code: u32;
}

export interface RoundPoolStats {
  round_id: u64;
  mode: RoundMode;
  total_up_stake: i128;
  total_down_stake: i128;
  up_participant_count: u32;
  down_participant_count: u32;
  up_stake_ratio_bps: u32;
  down_stake_ratio_bps: u32;
  precision_total_stake: i128;
  precision_participant_count: u32;
  precision_prediction_count: u32;
  precision_commitment_count: u32;
  precision_revealed_count: u32;
}


export interface UserPosition {
  amount: i128;
  side: BetSide;
}


export interface OraclePayload {
  /**
 * Contract address this payload is intended for.
 * Validated against `env.current_contract_address()` to prevent cross-contract replay.
 */
contract_addr: string;
  /**
 * SHA-256 hash of the network passphrase this payload targets.
 * Validated against `env.ledger().network_id()` to prevent cross-network replay.
 */
network_id: Buffer;
  /**
 * Per-round replay-protection nonce.
 * 
 * The oracle service must generate a unique value per submission for a
 * given round (e.g. a monotonic counter or random 64-bit value). The
 * contract records each consumed nonce under
 * `DataKey::ConsumedOracleNonce(round_id, nonce)` and rejects any reuse,
 * making resolution idempotent against accidental duplicate submissions.
 */
nonce: u64;
  price: u128;
  /**
 * Round identifier that should match `Round.start_ledger`
 */
round_id: u32;
  timestamp: u64;
}

export interface MultiFeedPayload {
  contract_addr: string;
  network_id: Buffer;
  nonce: u64;
  prices: Array<u128>;
  round_id: u32;
  sources: Array<u32>;
  timestamp: u64;
}

export interface OracleQuorumConfig {
  min_observations: u32;
  outlier_threshold_bps: u32;
  quorum_threshold: u32;
}

/**
 * Identifies which critical risk setting is pending timelocked activation.
 */
export enum ConfigChangeKind {
  Windows = 0,
  MaxStake = 1,
  MaxUserRoundExposure = 2,
  MaxPendingWinnings = 3,
  OracleStaleThreshold = 4,
  OracleMaxDeviationBps = 5,
  ProtocolFeeBps = 6,
  /** Fee incidence model: 0 = FeeOnPot (default), 1 = FeeOnWinnings (Issue #268). */
  FeeModel = 12,
  MinParticipants = 7,
  MaxPrecisionParticipants = 8,
  MintLimit = 9,
  ArchiveRetention = 10,
  CloseBufferLedgers = 11,
  EpochMintBudget = 12,
}

/**
 * Terminal outcome recorded when a round leaves the active state.
 */
export enum RoundArchiveStatus {
  Resolved = 0,
  Cancelled = 1,
  FallbackRefund = 2,
}

/**
 * Payload for a scheduled critical config change.
 */
export type ConfigChangePayload =
  | {tag: "Windows", values: readonly [u32, u32]}
  | {tag: "MaxStake", values: readonly [Option<i128>]}
  | {tag: "MaxUserRoundExposure", values: readonly [Option<i128>]}
  | {tag: "MaxPendingWinnings", values: readonly [Option<i128>]}
  | {tag: "OracleStaleThreshold", values: readonly [u64]}
  | {tag: "OracleMaxDeviationBps", values: readonly [Option<u32>]}
  | {tag: "ProtocolFeeBps", values: readonly [Option<u32>]}
  | {tag: "MinParticipants", values: readonly [Option<u32>]}
  | {tag: "MaxPrecisionParticipants", values: readonly [u32]}
  | {tag: "MintLimit", values: readonly [u32]}
  | {tag: "ArchiveRetention", values: readonly [u32]}
  | {tag: "CloseBufferLedgers", values: readonly [u32]}
  | {tag: "EpochMintBudget", values: readonly [i128]};


/**
 * Pending timelocked config change with activation ledger for on-chain observability.
 */
export interface PendingConfigChange {
  activation_ledger: u32;
  payload: ConfigChangePayload;
  scheduled_at_ledger: u32;
}


export interface PrecisionCommitment {
  amount: i128;
  hash: Buffer;
  revealed: boolean;
}


/**
 * Precision prediction entry (user address + predicted price)
 */
export interface PrecisionPrediction {
  amount: i128;
  predicted_price: u128;
  user: string;
}

/**
 * Cursor-based page of precision predictions.
 * Pass next_cursor as the cursor argument to the next call to fetch the subsequent page.
 */
export interface PrecisionPredictionsPage {
  items: Array<PrecisionPrediction>;
  next_cursor: Option<string>;
}

/**
 * Single entry in a cursor-based Up/Down positions page.
 */
/**
 * Cursor-based page of Up/Down positions.
 */
export interface UpdownPositionsPage {
  items: Array<[string, UserPosition]>;
  next_cursor: Option<string>;
}

/**
 * Entry in the global leaderboard.
 */
export interface LeaderboardEntry {
  user: string;
  stats: UserStats;
}

/**
 * Cursor-based page of leaderboard entries.
 */
export interface LeaderboardPage {
  items: Array<LeaderboardEntry>;
  next_cursor: Option<string>;
}


/**
 * Compact historical round summary persisted after resolve or cancel.
 * 
 * Designed for explorer/analytics queries without replaying events.
 * `price_final` is `0` for admin cancellations (no oracle settlement price).
 */
export interface ArchivedRoundSummary {
  mode: RoundMode;
  participant_count: u32;
  pool_down: i128;
  pool_up: i128;
  price_final: u128;
  price_start: u128;
  round_id: u64;
  settled_at_ledger: u32;
  status: RoundArchiveStatus;
}


/**
 * Oracle liveness record, updated by the oracle service on each heartbeat call.
 * `status`: 0 = active, 1 = degraded, 2 = offline.
 */
export interface OracleHeartbeatRecord {
  status: u32;
  timestamp: u64;
}

/**
 * Contract error types
 */
export const ContractError = {
  /**
   * Contract has already been initialized
   */
  1: {message:"AlreadyInitialized"},
  /**
   * Admin address not set - call initialize first
   */
  2: {message:"AdminNotSet"},
  /**
   * Oracle address not set - call initialize first
   */
  3: {message:"OracleNotSet"},
  /**
   * Bet amount must be greater than zero
   */
  6: {message:"InvalidBetAmount"},
  /**
   * No active round exists
   */
  7: {message:"NoActiveRound"},
  /**
   * Round has already ended
   */
  8: {message:"RoundEnded"},
  /**
   * User has insufficient balance
   */
  9: {message:"InsufficientBalance"},
  /**
   * User has already placed a bet in this round
   */
  10: {message:"AlreadyBet"},
  /**
   * Arithmetic overflow occurred
   */
  11: {message:"Overflow"},
  /**
   * Invalid price value
   */
  12: {message:"InvalidPrice"},
  /**
   * Invalid duration value
   */
  13: {message:"InvalidDuration"},
  /**
   * Invalid round mode (must be 0 or 1)
   */
  14: {message:"InvalidMode"},
  /**
   * Wrong prediction type for current round mode
   */
  15: {message:"WrongModeForPrediction"},
  /**
   * Round has not reached end_ledger yet
   */
  16: {message:"RoundNotEnded"},
  /**
   * Oracle data is too old (STALE)
   */
  18: {message:"StaleOracleData"},
  /**
   * Oracle payload round_id doesn't match ActiveRound
   */
  19: {message:"InvalidOracleRound"},
  /**
   * An active round already exists and cannot be overwritten
   */
  20: {message:"RoundAlreadyActive"},
  /**
   * Contract is paused for emergency recovery
   */
  22: {message:"ContractPaused"},
  /**
   * One or more window values exceed configured maximum bounds
   */
  23: {message:"WindowOutOfRange"},
  /**
   * Oracle payload timestamp is in the future
   */
  24: {message:"FutureOracleData"},
  /**
   * Arithmetic overflow in payout accumulation — no funds moved
   */
  25: {message:"PayoutOverflow"},
  /**
   * Round cannot be cancelled (no active round or already resolved)
   */
  27: {message:"RoundNotCancellable"},
  /**
   * Bet amount exceeds the configured maximum stake
   */
  28: {message:"StakeExceedsMax"},
  /**
   * User's cumulative exposure in this round exceeds the configured cap
   */
  29: {message:"ExposureCapExceeded"},
  /**
   * Pending winnings accumulation would exceed the configured cap
   */
  30: {message:"PendingWinningsCapExceeded"},
  /**
   * Start price is outside the allowed range
   */
  31: {message:"InvalidStartPrice"},
  /**
   * Oracle payload nonce was already consumed for this round (replay)
   */
  33: {message:"OracleNonceReused"},
  /**
   * Minimum participants value is out of valid range (must be 1–10000)
   */
  35: {message:"InvalidMinParticipants"},
  /**
   * Precision participant cap is out of range (must be 1–10000)
   */
  38: {message:"InvalidPrecisionCap"},
  /**
   * Precision round has reached the configured participant cap
   */
  39: {message:"PrecisionCapExceeded"},
  /**
   * Oracle final price deviates beyond configured threshold
   */
  41: {message:"OracleDeviationExceeded"},
  /**
   * Stored schema version is unknown or unsupported by this contract build
   */
  42: {message:"UnsupportedSchemaVersion"},
  /**
   * Migration cannot run while a round is active
   */
  44: {message:"MigrationActiveRound"},
  /**
   * Commitment for precision prediction not found
   */
  45: {message:"CommitmentNotFound"},
  /**
   * Precision prediction has already been revealed
   */
  46: {message:"AlreadyRevealed"},
  /**
   * Attempted to reveal prediction outside the valid window
   */
  47: {message:"InvalidRevealWindow"},
  /**
   * Revealed prediction hash does not match committed hash
   */
  48: {message:"HashMismatch"},
  /**
   * Oracle payload network_id does not match the runtime network
   */
  49: {message:"OracleNetworkMismatch"},
  /**
   * Protocol fee bps is outside the allowed range (must be in 1..=MAX_PROTOCOL_FEE_BPS)
   */
  51: {message:"InvalidProtocolFeeBps"},
  /**
   * Rate limit for minting in the current ledger has been exceeded
   */
  53: {message:"MintLimitExceeded"},
  /**
   * No pending oracle rotation proposal to accept or cancel
   */
  54: {message:"NoPendingRotation"},
  55: {message:"RotationDelayNotElapsed"},
  /**
   * Invalid archive retention limit
   */
  62: {message:"InvalidArchiveRetention"},
  /**
   * Commitment hash is malformed (e.g. the all-zero placeholder)
   */
  63: {message:"InvalidCommitment"},
  64: {message:"InvalidSalt"},
  65: {message:"NoRoundTemplate"},
  66: {message:"OracleTimestampOutsideWindow"},
  67: {message:"EpochBudgetExceeded"},
  68: {message:"OracleNotLive"},
  69: {message:"InvalidPayoutPolicy"},
  70: {message:"BelowMinBet"},
  71: {message:"InsufficientOracleQuorum"},
  72: {message:"TooFewObservations"},
  73: {message:"OracleOutlierRejected"},
  74: {message:"DuplicateOracleSource"},
  75: {message:"InvalidObservationOrder"},
  76: {message:"UnsupportedDataKeyForTtlTouch"},
  77: {message:"PendingWinningsNotFound"},
  78: {message:"ExpiryNotConfigured"},
  79: {message:"EarlyCashoutDisabled"},
  80: {message:"PositionNotFound"},
  81: {message:"InvalidPhaseForCashout"},
  82: {message:"WrongModeForCashout"},
  83: {message:"PendingWinningsNotExpired"},
}

/**
 * Decodes a contract error code into a structured result suitable for wallet UX.
 * @param code - The on-chain u32 error code returned by the contract.
 * @returns An object with the code, variant name, and human-readable message, or null if the code is unknown.
 */
export function decodeContractError(code: number): {
  code: number;
  variant: string;
  message: string;
} | null {
  const entry = ContractError[code];
  if (!entry) {
    return null;
  }
  return {
    code,
    variant: entry.message,
    message: entry.message,
  };
}

/**
 * Formats a contract error code into a user-facing string for wallet display.
 * Returns a fallback for unknown codes.
 * @param code - The on-chain u32 error code returned by the contract.
 */
export function formatContractError(code: number): string {
  const decoded = decodeContractError(code);
  if (!decoded) {
    return `Unknown contract error (code ${code})`;
  }
  return `${decoded.message} (code ${code})`;
}

/**
 * Decode contract error code to human‑readable message.
 * @param code Numeric error code returned by the contract.
 * @returns Friendly message for UI or wallet integration.
 */
export function ContractErrorDecoder(code: number): string {
  const err = (ContractError as any)[code];
  if (err && err.message) {
    return err.message;
  }
  return `Unknown contract error code ${code}`;
}

export interface RoundTemplate {
  start_price: u128;
  mode: Option<u32>;
}

export interface LeaderboardEntry {
  user: string;
  stats: UserStats;
}

export interface SeasonLeaderboardEntry {
  user: string;
  wins: u32;
  best_streak: u32;
}

export interface SeasonArchive {
  season_id: u32;
  ended_at_ledger: u32;
  wins: Array<SeasonLeaderboardEntry>;
  streak: Array<SeasonLeaderboardEntry>;
  participant_count: u32;
}

export interface Client {
  /**
   * Construct and simulate a balance transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns user's vXLM balance
   */
  balance: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_admin transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_admin: (options?: MethodOptions) => Promise<AssembledTransaction<Option<string>>>

  /**
   * Construct and simulate a is_paused transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns whether the contract is currently paused
   */
  is_paused: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a place_bet transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Places a bet on the active round (Up/Down mode only).
   * 
   * Storage layout: each participant's position is stored under its own
   * composite key `DataKey::Position(round_id, user)` — O(1) read/write
   * regardless of how many other participants exist. An ordered participant
   * list `DataKey::RoundParticipants(round_id)` is maintained for O(n)
   * iteration at resolution time only.
   */
  place_bet: ({user, amount, side}: {user: string, amount: i128, side: BetSide}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_oracle transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_oracle: (options?: MethodOptions) => Promise<AssembledTransaction<Option<string>>>

  /**
   * Construct and simulate a initialize transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Initializes the contract with admin and oracle addresses (one-time only)
   */
  initialize: ({admin, oracle}: {admin: string, oracle: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_windows transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked windows update (alias for [`Self::schedule_windows`]).
   * bet_ledgers: Number of ledgers users can place bets
   * run_ledgers: Total number of ledgers before round can be resolved
   */
  set_windows: ({bet_ledgers, run_ledgers}: {bet_ledgers: u32, run_ledgers: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a cancel_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Cancels the active round and deterministically refunds all participant stakes.
   * 
   * Only admin may cancel. Intended for oracle-unavailable or emergency recovery
   * scenarios. After cancellation:
   * - All participant stakes are moved to their pending winnings.
   * - The active round is removed; no future settlement is possible.
   * - The round ID is marked cancelled to prevent any replay.
   */
  cancel_round: ({reason}: {reason: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a create_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Creates a new prediction round (admin only)
   * mode: 0 = Up/Down (default), 1 = Precision (Legends)
   */
  create_round: ({start_price, mode}: {start_price: u128, mode: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a mint_initial transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Mints 1000 vXLM for new users (one-time only)
   */
  mint_initial: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_max_stake transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current maximum stake cap, if set.
   */
  get_max_stake: (options?: MethodOptions) => Promise<AssembledTransaction<Option<i128>>>

  /**
   * Construct and simulate a predict_price transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Alias for place_precision_prediction - allows users to submit exact price predictions
   * guessed_price: price scaled to 4 decimals (e.g., 0.2297 → 2297)
   */
  predict_price: ({user, guessed_price, amount}: {user: string, guessed_price: u128, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a resolve_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Resolves the round with oracle payload (oracle only)
   * Mode 0 (Up/Down): Winners split losers' pool proportionally; ties get refunds
   * Mode 1 (Precision/Legends): Closest guess wins full pot; ties split evenly
   */
  resolve_round: ({payload}: {payload: OraclePayload}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a resolve_round_multi transaction.
   * Resolves the round using multi-feed oracle payload with median calculation and outlier rejection.
   */
  resolve_round_multi: ({payload}: {payload: MultiFeedPayload}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate an early cash-out transaction.
   * Allows an UpDown bettor to exit during the running phase with a penalty fee.
   */
  cash_out_early: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Sets multi-feed oracle quorum configuration (admin only).
   */
  set_oracle_quorum_config: ({cfg}: {cfg: OracleQuorumConfig}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Gets multi-feed oracle quorum configuration if configured.
   */
  get_oracle_quorum_config: (options?: MethodOptions) => Promise<AssembledTransaction<Option<OracleQuorumConfig>>>

  /**
   * Construct and simulate a set_max_stake transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked max stake update (alias for [`Self::schedule_max_stake`]).
   * Pass `None` to disable the cap.
   */
  set_max_stake: ({max_amount}: {max_amount: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a claim_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Claims pending winnings and adds to balance
   */
  claim_winnings: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<i128>>>

  /**
   * Construct and simulate a get_user_stats transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns user statistics (wins, losses, streaks)
   */
  get_user_stats: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<UserStats>>

  /**
   * Construct and simulate a is_oracle_live transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns `true` if the oracle has a non-stale heartbeat with status not offline (2).
   * Uses the configured stale threshold, defaulting to 3600 seconds.
   */
  is_oracle_live: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a pause_contract transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Pauses the contract for emergency recovery (admin only)
   */
  pause_contract: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_active_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the currently active round, if any
   */
  get_active_round: (options?: MethodOptions) => Promise<AssembledTransaction<Option<Round>>>

  /**
   /**
   * Construct and simulate a get_runtime_mode transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current runtime mode (0 = Normal, 1 = ClaimsOnly, 2 = FullyPaused)
   */
  get_runtime_mode: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a schedule_windows transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked update to betting and execution windows (admin only).
   * The change is stored pending until `apply_scheduled_changes` is called after the delay.
   */
  schedule_windows: ({bet_ledgers, run_ledgers}: {bet_ledgers: u32, run_ledgers: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_runtime_mode transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the runtime mode of the contract (admin only)
   */
  set_runtime_mode: ({mode}: {mode: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_round_phase transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current lifecycle phase of the active round.
   *
   * Phase boundaries are deterministic:
   * - `Betting` while `ledger < bet_end_ledger`
   * - `Running` while `bet_end_ledger ≤ ledger < end_ledger`
   * - `Resolvable` when `ledger ≥ end_ledger`
   *
   * Returns `NoActiveRound` when no round is active.
   */
  get_round_phase: (options?: MethodOptions) => Promise<AssembledTransaction<Result<RoundPhase>>>

  /**
   * Construct and simulate a unpause_contract transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Unpauses the contract after recovery (admin only)
   */
  unpause_contract: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a commit_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Commits a hashed prediction and stake amount (Precision mode only)
   */
  commit_prediction: ({user, hash, amount}: {user: string, hash: Buffer, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_last_round_id transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the ID of the last created round (0 if no rounds created yet)
   */
  get_last_round_id: (options?: MethodOptions) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a get_user_position transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns user's position in the current round (Up/Down mode).
   * 
   * Reads a single composite key `DataKey::Position(round_id, user)` — O(1).
   * Falls back to legacy `UpDownPositions` / `Positions` map blobs for
   * one-time migration compatibility.
   */
  get_user_position: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Option<UserPosition>>>

  /**
   * Construct and simulate a reveal_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Reveals a previously committed prediction (Precision mode only)
   */
  reveal_prediction: ({user, predicted_price, salt}: {user: string, predicted_price: u128, salt: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_archived_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns a compact archived round summary by round id, if retained.
   */
  get_archived_round: ({round_id}: {round_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Option<ArchivedRoundSummary>>>

  /**
   * Construct and simulate a get_schema_version transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the stored schema version. If unset, returns legacy version 1.
   */
  get_schema_version: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a is_round_cancelled transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns true if the given round_id was cancelled.
   */
  is_round_cancelled: ({round_id}: {round_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a schedule_max_stake transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked update to the maximum stake cap (admin only).
   */
  schedule_max_stake: ({max_amount}: {max_amount: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a cancel_config_change transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Cancels a pending timelocked config change before activation (admin only).
   */
  cancel_config_change: ({kind}: {kind: ConfigChangeKind}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_min_participants transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current minimum participant threshold, if set.
   */
  get_min_participants: (options?: MethodOptions) => Promise<AssembledTransaction<Option<u32>>>

  /**
   * Construct and simulate a get_oracle_heartbeat transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the most recent oracle heartbeat record, if any.
   */
  get_oracle_heartbeat: (options?: MethodOptions) => Promise<AssembledTransaction<Option<OracleHeartbeatRecord>>>

  /**
   * Construct and simulate a get_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns user's claimable winnings
   */
  get_pending_winnings: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_updown_positions transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns all Up/Down positions for the current round.
   * 
   * Reads the participant list once, then fetches each position individually.
   */
  get_updown_positions: (options?: MethodOptions) => Promise<AssembledTransaction<Map<string, UserPosition>>>

  /**
   * Construct and simulate a set_min_participants transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the minimum participant count required for competitive settlement (admin only).
   * Rounds that end below this threshold are refunded to all participants.
   * Pass `None` to disable the threshold.
   */
  set_min_participants: ({min}: {min: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_max_user_exposure transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current per-user round exposure cap, if set.
   */
  get_max_user_exposure: (options?: MethodOptions) => Promise<AssembledTransaction<Option<i128>>>

  /**
   * Construct and simulate a simulate_payout transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Estimates payouts for the active round given a hypothetical final price.
   */
  simulate_payout: ({final_price}: {final_price: u128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<SimulationResult>>>

  /**
   * Construct and simulate a set_max_user_exposure transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked exposure cap update (alias for [`Self::schedule_max_user_exposure`]).
   * Pass `None` to disable the cap.
   */
  set_max_user_exposure: ({max_exposure}: {max_exposure: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a apply_scheduled_changes transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Applies a scheduled critical config change after its activation ledger (any caller).
   */
  apply_scheduled_changes: ({kind}: {kind: ConfigChangeKind}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a migrate_schema_v1_to_v2 transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Migrates legacy schema version 1 → current schema version 2 (admin only).
   * 
   * Guardrails:
   * - Must not have an active round (avoids partial state interpretation changes)
   * - Only supports v1 → v2 in this release
   */
  migrate_schema_v1_to_v2: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a update_oracle_heartbeat transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Records an oracle heartbeat (oracle only).
   * `status`: 0 = active, 1 = degraded, 2 = offline.
   * Stores current ledger timestamp; emits `("oracle", "heartbeat")`.
   */
  update_oracle_heartbeat: ({status}: {status: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_max_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current maximum pending winnings cap, if set.
   */
  get_max_pending_winnings: (options?: MethodOptions) => Promise<AssembledTransaction<Option<i128>>>

  /**
   * Construct and simulate a set_max_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked pending winnings cap update (alias for [`Self::schedule_max_pending_winnings`]).
   * Pass `None` to disable the cap.
   */
  set_max_pending_winnings: ({max_pending}: {max_pending: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_pending_config_change transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns a pending timelocked config change for the given kind, if any.
   */
  get_pending_config_change: ({kind}: {kind: ConfigChangeKind}, options?: MethodOptions) => Promise<AssembledTransaction<Option<PendingConfigChange>>>

  /**
   * Construct and simulate a get_precision_predictions transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns all precision predictions for the current round.
   * 
   * Reads the participant list once, then fetches each prediction individually.
   * Total reads: 1 (participant list) + N (predictions) instead of 1 large map blob.
   */
  get_precision_predictions: (options?: MethodOptions) => Promise<AssembledTransaction<Array<PrecisionPrediction>>>

  /**
   * Construct and simulate a get_updown_positions_page transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns a deterministic slice of Up/Down positions for the active
   * round, ordered by ascending participant address, as `(Address,
   * UserPosition)` pairs.
   * 
   * A `Vec` of pairs is used instead of a `Map` because pagination over a
   * `Map` has no stable, caller-controllable slice semantics in Soroban —
   * pairs preserve the exact offset/limit window the caller requested.
   * 
   * See [`Self::get_precision_predictions_page`] for the offset/limit/empty-page
   * contract, which is identical here. This does not replace
   * [`Self::get_updown_positions`], which remains available unchanged.
   */
  get_updown_positions_page: ({offset, limit}: {offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<readonly [string, UserPosition]>>>

  /**
   * Construct and simulate a get_oracle_stale_threshold transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured oracle stale threshold, or the default (3600 s) if not set.
   */
  get_oracle_stale_threshold: (options?: MethodOptions) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a get_recent_archived_rounds transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns up to `limit` most recently archived rounds (newest first).
   * 
   * Pass `limit = 0` to receive an empty list. Values above [`MAX_ARCHIVED_ROUNDS`]
   * are capped automatically.
   */
  get_recent_archived_rounds: ({limit}: {limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<ArchivedRoundSummary>>>

  /**
   * Construct and simulate a get_user_archive_history transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns paginated archived participation history for a user (newest first).
   * 
   * Reads the user's on-chain index of archived round IDs, applies offset/limit
   * pagination, and resolves each ID to its ArchivedRoundSummary. Stale
   * entries (rounds pruned by FIFO retention) are silently skipped.
   * 
   * Standard pagination semantics:
   * - offset past the end → empty page
   * - limit == 0 → empty page
   * - limit capped at MAX_PAGE_SIZE (100)
   * - Ordering is newest-first (descending round ID)
   */
  get_user_archive_history: ({user, offset, limit}: {user: string, offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<ArchivedRoundSummary>>>

  /**
   * Construct and simulate a place_precision_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Places a precision prediction on the active round (Precision/Legends mode only)
   * predicted_price: price scaled to 4 decimals (e.g., 0.2297 → 2297)
   * 
   * Per-user key `DataKey::PrecisionPosition(round_id, user)` gives O(1)
   * write cost independent of participant count.
   */
  place_precision_prediction: ({user, amount, predicted_price}: {user: string, amount: i128, predicted_price: u128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a schedule_max_user_exposure transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked update to the per-user round exposure cap (admin only).
   */
  schedule_max_user_exposure: ({max_exposure}: {max_exposure: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_oracle_stale_threshold transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked stale threshold update (alias for [`Self::schedule_oracle_stale_threshold`]).
   * Allowed range: 60–86400 seconds (1 minute to 24 hours).
   */
  set_oracle_stale_threshold: ({seconds}: {seconds: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_oracle_max_deviation_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured oracle max deviation bps, if set.
   */
  get_oracle_max_deviation_bps: (options?: MethodOptions) => Promise<AssembledTransaction<Option<u32>>>

  /**
   * Construct and simulate a set_oracle_max_deviation_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked oracle deviation update (alias for [`Self::schedule_oracle_deviation_bps`]).
   * 
   * - `None`: disables deviation guardrails
   * - `Some(bps)`: enables guardrails with a threshold in basis points (1 bp = 0.01%)
   */
  set_oracle_max_deviation_bps: ({bps}: {bps: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a arm_oracle_deviation_override transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Arms a one-shot override to bypass deviation checks for the next settlement (admin only).
   * The flag is automatically cleared after a settlement uses it.
   */
  arm_oracle_deviation_override: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_user_precision_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns user's precision prediction in the current round (Precision mode).
   * 
   * Reads a single composite key `DataKey::PrecisionPosition(round_id, user)` — O(1).
   * Falls back to legacy `PrecisionPositions` map for migration compatibility.
   */
  get_user_precision_prediction: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Option<PrecisionPrediction>>>

  /**
   * Construct and simulate a schedule_max_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked update to the pending winnings cap (admin only).
   */
  schedule_max_pending_winnings: ({max_pending}: {max_pending: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a schedule_oracle_deviation_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked update to the oracle max deviation threshold (admin only).
   */
  schedule_oracle_deviation_bps: ({bps}: {bps: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_max_precision_participants transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured Precision participant cap, or the default if unset.
   */
  get_max_precision_participants: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_precision_predictions_page transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns a deterministic slice of Precision-mode predictions for the
   * active round, ordered by ascending participant address (the same
   * canonical order used internally for payout-remainder assignment).
   * 
   * `offset` is the zero-based index into the ordered participant list.
   * `limit` is the maximum number of entries to return and is capped at
   * `MAX_PAGE_SIZE` to bound gas/read costs regardless of caller input.
   * 
   * Returns an empty `Vec` if there is no active round, if `offset` is
   * beyond the number of available entries, or if `limit` is zero — this
   * is not an error condition, matching standard pagination semantics
   * (asking past the end of a list yields an empty page, not a fault).
   * 
   * This does not replace [`Self::get_precision_predictions`], which
   * remains available unchanged for full-set reads on small rounds.
   */
  get_precision_predictions_page: ({offset, limit}: {offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<PrecisionPrediction>>>

  /**
   * Construct and simulate a set_max_precision_participants transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the maximum participant count for Precision rounds (admin only).
   * The value must be in the range 1..=10_000. Unset contracts use the
   * protocol default of 1_000 participants.
   */
  set_max_precision_participants: ({max}: {max: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a schedule_oracle_stale_threshold transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked update to the oracle stale threshold (admin only).
   */
  schedule_oracle_stale_threshold: ({seconds}: {seconds: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  migrate_schema_v2_to_v3: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  set_oracle_min_confidence_bps: ({min_bps}: {min_bps: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  set_oracle_strict_mode: ({enabled}: {enabled: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_oracle_min_confidence_bps: (options?: MethodOptions) => Promise<AssembledTransaction<Option<u32>>>
  get_oracle_strict_mode: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>
  get_protocol_health: (options?: MethodOptions) => Promise<AssembledTransaction<ProtocolHealthStatus>>
  get_protocol_status: (options?: MethodOptions) => Promise<AssembledTransaction<ProtocolStatus>>
  get_round_status: ({round_id}: {round_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<RoundStatus>>
  propose_oracle_rotation: ({new_oracle, expires_in_seconds}: {new_oracle: string, expires_in_seconds: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  accept_oracle_rotation: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  cancel_oracle_rotation: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_oracle_rotation_proposal: (options?: MethodOptions) => Promise<AssembledTransaction<Option<OracleRotationProposal>>>
  schedule_protocol_fee_bps: ({bps}: {bps: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  set_protocol_fee_bps: ({bps}: {bps: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_protocol_fee_bps: (options?: MethodOptions) => Promise<AssembledTransaction<Option<u32>>>
  get_protocol_fee_treasury: (options?: MethodOptions) => Promise<AssembledTransaction<i128>>
  withdraw_protocol_fee: ({recipient, amount}: {recipient: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<i128>>>
  /**
   * Construct and simulate a set_fee_model transaction.
   * Sets the fee incidence model (admin only).
   * `FeeOnPot` (0): fee is calculated on the total round pot (default).
   * `FeeOnWinnings` (1): fee is calculated only on net winnings / profit.
   */
  set_fee_model: ({model}: {model: FeeModel}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  /**
   * Construct and simulate a get_fee_model transaction.
   * Returns the configured fee incidence model, defaulting to `FeeOnPot`.
   */
  get_fee_model: (options?: MethodOptions) => Promise<AssembledTransaction<FeeModel>>
  set_mint_limit: ({limit}: {limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_mint_limit: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>
  set_epoch_mint_budget: ({budget}: {budget: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_epoch_mint_budget: (options?: MethodOptions) => Promise<AssembledTransaction<i128>>
  set_archive_retention: ({limit}: {limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_archive_retention: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>
  set_close_buffer_ledgers: ({buffer_ledgers}: {buffer_ledgers: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_close_buffer_ledgers: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>
  get_round_pool_stats: (options?: MethodOptions) => Promise<AssembledTransaction<Option<RoundPoolStats>>>
  get_user_archived_participation: ({user, round_id}: {user: string, round_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Option<UserRoundOutcome>>>

  set_hb_strict_mode: ({enabled}: {enabled: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_hb_strict_mode: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>
  arm_hb_override: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_hb_override_armed: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>
  set_hb_grace_seconds: ({grace_seconds}: {grace_seconds: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_hb_grace_seconds: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>
  set_precision_payout_policy: ({policy}: {policy: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_precision_payout_policy: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>
  set_round_template: ({start_price, mode}: {start_price: u128, mode: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  clear_round_template: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_round_template: (options?: MethodOptions) => Promise<AssembledTransaction<Option<RoundTemplate>>>
  create_next_from_template: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>
  get_leaderboard_by_wins: ({offset, limit}: {offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<LeaderboardEntry>>>
  get_leaderboard_by_streak: ({offset, limit}: {offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<LeaderboardEntry>>>
  get_current_season_id: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>
  get_season_user_stats: ({season_id, user}: {season_id: u32, user: string}, options?: MethodOptions) => Promise<AssembledTransaction<UserStats>>
  reset_leaderboard_season: (options?: MethodOptions) => Promise<AssembledTransaction<Result<u32>>>
  get_season_archive: ({season_id}: {season_id: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Option<SeasonArchive>>>
  get_season_leaderboard_by_wins: ({season_id, offset, limit}: {season_id: u32, offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<SeasonLeaderboardEntry>>>
  get_season_leaderboard_by_streak: ({season_id, offset, limit}: {season_id: u32, offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<SeasonLeaderboardEntry>>>
}
export class Client extends ContractClient {
  static async deploy<T = Client>(
    /** Options for initializing a Client as well as for calling a method, with extras specific to deploying. */
    options: MethodOptions &
      Omit<ContractClientOptions, "contractId"> & {
        /** The hash of the Wasm blob, which must already be installed on-chain. */
        wasmHash: Buffer | string;
        /** Salt used to generate the contract's ID. Passed through to {@link Operation.createCustomContract}. Default: random. */
        salt?: Buffer | Uint8Array;
        /** The format used to decode `wasmHash`, if it's provided as a string. */
        format?: "hex" | "base64";
      }
  ): Promise<AssembledTransaction<T>> {
    return ContractClient.deploy(null, options)
  }
  constructor(public readonly options: ContractClientOptions) {
    super(
      new ContractSpec([ "AAAAAQAAAAAAAAAAAAAABVJvdW5kAAAAAAAACAAAAAAAAAAOYmV0X2VuZF9sZWRnZXIAAAAAAAQAAAAAAAAACmVuZF9sZWRnZXIAAAAAAAQAAAAAAAAABG1vZGUAAAfQAAAACVJvdW5kTW9kZQAAAAAAAAAAAAAJcG9vbF9kb3duAAAAAAAACwAAAAAAAAAHcG9vbF91cAAAAAALAAAAAAAAAAtwcmljZV9zdGFydAAAAAAKAAAAAAAAAAhyb3VuZF9pZAAAAAYAAAAAAAAADHN0YXJ0X2xlZGdlcgAAAAQ=",
        "AAAAAgAAACNSZXByZXNlbnRzIHdoaWNoIHNpZGUgYSB1c2VyIGJldCBvbgAAAAAAAAAAB0JldFNpZGUAAAAAAgAAAAAAAAAAAAAAAlVwAAAAAAAAAAAAAAAAAAREb3du",
        "AAAAAgAAAppTdG9yYWdlIGtleXMgZm9yIGNvbnRyYWN0IGRhdGEKCiMjIEluZGV4ZWQgcG9zaXRpb24ga2V5cyAodmFyaWFudHMgMTPigJMxNSkKCmBQb3NpdGlvbihyb3VuZF9pZCwgYWRkcmVzcylgIGFuZCBgUHJlY2lzaW9uUG9zaXRpb24ocm91bmRfaWQsIGFkZHJlc3MpYCBzdG9yZQphIHNpbmdsZSB1c2VyJ3MgcmVjb3JkIHVuZGVyIGEgY29tcG9zaXRlIGtleSwgZW5hYmxpbmcgTygxKSByZWFkL3dyaXRlIHBlciB1c2VyCmluc3RlYWQgb2YgZGVzZXJpYWxpemluZyB0aGUgZnVsbCBwYXJ0aWNpcGFudCBtYXAgb24gZXZlcnkgYmV0LgoKYFJvdW5kUGFydGljaXBhbnRzKHJvdW5kX2lkKWAgaG9sZHMgdGhlIG9yZGVyZWQgYFZlYzxBZGRyZXNzPmAgdXNlZCBmb3IKaXRlcmF0aW9uIGF0IHJlc29sdXRpb24gdGltZS4gQXBwZW5kaW5nIG9uZSBhZGRyZXNzIGlzIGNoZWFwZXIgdGhhbgpyZS1zZXJpYWxpc2luZyBhbiBOLWVudHJ5IGBNYXA8QWRkcmVzcywgVD5gIGZvciBldmVyeSBiZXQgcGxhY2VkLgoKTGVnYWN5IHNpbmdsZS1rZXkgbWFwcyAoYFVwRG93blBvc2l0aW9uc2AsIGBQcmVjaXNpb25Qb3NpdGlvbnNgKSBhcmUga2VwdCBmb3IKYmFja3dhcmQtY29tcGF0aWJsZSByZWFkcyBkdXJpbmcgYSBtaWdyYXRpb24gd2luZG93OyB0aGV5IGFyZSBubyBsb25nZXIgd3JpdHRlbi4AAAAAAAAAAAAHRGF0YUtleQAAAAAgAAAAAQAAAAAAAAAHQmFsYW5jZQAAAAABAAAAEwAAAAAAAAAAAAAABUFkbWluAAAAAAAAAAAAAAAAAAAGT3JhY2xlAAAAAAAAAAAAdE9uLWNoYWluIHN0b3JhZ2Ugc2NoZW1hIHZlcnNpb24gZm9yIG1pZ3JhdGlvbiBzYWZldHkuCklmIG1pc3NpbmcsIHRoZSBjb250cmFjdCB0cmVhdHMgaXQgYXMgbGVnYWN5IHNjaGVtYSB2ZXJzaW9uIDEuAAAADVNjaGVtYVZlcnNpb24AAAAAAAAAAAAAAAAAAAtBY3RpdmVSb3VuZAAAAAAAAAAAAAAAAAlQb3NpdGlvbnMAAAAAAAAAAAAAAAAAAA9VcERvd25Qb3NpdGlvbnMAAAAAAAAAAAAAAAASUHJlY2lzaW9uUG9zaXRpb25zAAAAAAABAAAAAAAAAA9QZW5kaW5nV2lubmluZ3MAAAAAAQAAABMAAAABAAAAAAAAAAlVc2VyU3RhdHMAAAAAAAABAAAAEwAAAAAAAAAAAAAABlBhdXNlZAAAAAAAAAAAAAAAAAAQQmV0V2luZG93TGVkZ2VycwAAAAAAAAAAAAAAEFJ1bldpbmRvd0xlZGdlcnMAAAAAAAAAAAAAAAtMYXN0Um91bmRJZAAAAAABAAAAPlBlci11c2VyIFVwRG93biBwb3NpdGlvbjogKHJvdW5kX2lkLCBhZGRyZXNzKSDihpIgVXNlclBvc2l0aW9uAAAAAAAIUG9zaXRpb24AAAACAAAABgAAABMAAAABAAAASlBlci11c2VyIFByZWNpc2lvbiBwcmVkaWN0aW9uOiAocm91bmRfaWQsIGFkZHJlc3Mg4oaSIFByZWNpc2lvblByZWRpY3Rpb24AAAAAAARQcmVjaXNpb25QcmVkaWN0aW9uAAAAAAAACAAAABgAAABMAAAABAAAASlBlci11c2VyIFByZWNpc2lvbiBjb21taXRtZW50OiAocm91bmRfaWQsIGFkZHJlc3Mg4oaSIFByZWNpc2lvbkNvbW1pdG1lbnQAAAAAAATQcmVjaXNpb25Db21taXRtZW50AAAAAACAAAABgAAABMAAAABAAAAP09yZGVyZWQgcGFydGljaXBhbnQgbGlzdCBmb3IgYSByb3VuZDogcm91bmRfaWQg4oaSIFZlYzxBZGRyZXNzPgAAAAARUm91bmRQYXJ0aWNpcGFudHMAAAAAAAABAAAABgAAAAAAAAA7TWF4aW11bSBzdGFrZSBhbGxvd2VkIHBlciBpbmRpdmlkdWFsIGJldCAoTm9uZSA9IHVubGltaXRlZCkAAAAACE1heFN0YWtlAAAAAAAAAEFNYXhpbXVtIGN1bXVsYXRpdmUgZXhwb3N1cmUgcGVyIHVzZXIgcGVyIHJvdW5kIChOb25lID0gdW5saW1pdGVkKQAAAAAAABRNYXhVc2VyUm91bmRFeHBvc3VyZQAAAAAAAAA/TWF4aW11bSBwZW5kaW5nIHdpbm5pbmdzIGFsbG93ZWQgcGVyIGFjY291bnQgKE5vbmUgPSB1bmxpbWl0ZWQpAAAAABJNYXhQZW5kaW5nV2lubmluZ3MAAAAAAAEAAAAvTWFya2VyIGZvciBhIGNhbmNlbGxlZCByb3VuZDogcm91bmRfaWQg4oaSIHRydWUAAAAADkNhbmNlbGxlZFJvdW5kAAAAAAABAAAABgAAAAEAAACEUGVyLXJvdW5kIGNvbnN1bWVkIG9yYWNsZSBub25jZTogKHJvdW5kX2lkLCBub25jZSkg4oaSIHRydWUuClVzZWQgdG8gcmVqZWN0IGR1cGxpY2F0ZSBvcmFjbGUgcGF5bG9hZCBzdWJtaXNzaW9ucyBmb3IgdGhlIHNhbWUgcm91bmQuAAAAE0NvbnN1bWVkT3JhY2xlTm9uY2UAAAAAAgAAAAYAAAAGAAAAAAAAAFFNaW5pbXVtIHBhcnRpY2lwYW50IGNvdW50IGZvciBjb21wZXRpdGl2ZSBzZXR0bGVtZW50OyB1bnNldCA9IG5vIG1pbmltdW0gZW5mb3JjZWQAAAAAAAAPTWluUGFydGljaXBhbnRzAAAAAAAAAAA0T3JhY2xlIGhlYXJ0YmVhdDogbGFzdCByZWNvcmRlZCB0aW1lc3RhbXAgYW5kIHN0YXR1cwAAAA9PcmFjbGVIZWFydGJlYXQAAAAAAAAAAFFTdGFsZS1oZWFydGJlYXQgdGhyZXNob2xkIGluIHNlY29uZHMgKGFkbWluLWNvbmZpZ3VyYWJsZSk7IHVuc2V0ID0gMzYwMCBzIGRlZmF1bHQAAAAAAAAUT3JhY2xlU3RhbGVUaHJlc2hvbGQAAAAAAAAATE1heGltdW0gcGFydGljaXBhbnRzIGFjY2VwdGVkIGluIGEgUHJlY2lzaW9uIHJvdW5kOyB1bnNldCA9IHByb3RvY29sIGRlZmF1bHQAAAAYTWF4UHJlY2lzaW9uUGFydGljaXBhbnRzAAAAAAAAAGtPcmFjbGUgbWF4IGRldmlhdGlvbiB0aHJlc2hvbGQgaW4gYmFzaXMgcG9pbnRzICgxIGJwID0gMC4wMSUpLgpJZiB1bnNldCwgZGV2aWF0aW9uIGd1YXJkcmFpbHMgYXJlIGRpc2FibGVkLgAAAAAVT3JhY2xlTWF4RGV2aWF0aW9uQnBzAAAAAAAAAAAAAHFPbmUtc2hvdCBhZG1pbiBvdmVycmlkZSBhbGxvd2luZyB0aGUgbmV4dCBzZXR0bGVtZW50IHRvIGJ5cGFzcyBkZXZpYXRpb24gY2hlY2tzLgpBdXRvbWF0aWNhbGx5IGNsZWFyZWQgYWZ0ZXIgdXNlLgAAAAAAABxPcmFjbGVEZXZpYXRpb25PdmVycmlkZUFybWVkAAAAAQAAAElDb21wYWN0IHBvc3Qtc2V0dGxlbWVudCBzdW1tYXJ5IGtleWVkIGJ5IHJvdW5kIGlkIGZvciBoaXN0b3JpY2FsIHF1ZXJpZXMuAAAAAAAADUFyY2hpdmVkUm91bmQAAAAAAAABAAAABgAAAAAAAAA8T3JkZXJlZCByb3VuZCBpZHMgZm9yIGFyY2hpdmUgcmV0ZW50aW9uIChvbGRlc3QgYXQgaW5kZXggMCkuAAAAFlJlY2VudEFyY2hpdmVkUm91bmRJZHMAAAAAAAEAAAA/VGltZWxvY2tlZCBwZW5kaW5nIGNyaXRpY2FsIGNvbmZpZyBjaGFuZ2Uga2V5ZWQgYnkgY2hhbmdlIGtpbmQuAAAAABNQZW5kaW5nQ29uZmlnQ2hhbmdlAAAAAAEAAAfQAAAAEENvbmZpZ0NoYW5nZUtpbmQ=",
        "AAAAAwAAAB5Sb3VuZCBtb2RlIGZvciBwcmVkaWN0aW9uIHR5cGUAAAAAAAAAAAAJUm91bmRNb2RlAAAAAAAAAgAAAAAAAAAGVXBEb3duAAAAAAAAAAAAAAAAAAlQcmVjaXNpb24AAAAAAAAB",
        "AAAAAQAAAAAAAAAAAAAACVVzZXJTdGF0cwAAAAAAAAQAAAAAAAAAC2Jlc3Rfc3RyZWFrAAAAAAQAAAAAAAAADmN1cnJlbnRfc3RyZWFrAAAAAAAEAAAAAAAAAAx0b3RhbF9sb3NzZXMAAAAEAAAAAAAAAAp0b3RhbF93aW5zAAAAAAAE",
        "AAAAAwAAACdSdW50aW1lIG1vZGUgZm9yIHRoZSBjb250cmFjdCBsaWZlY3ljbGUAAAAAAAAAAAtSdW50aW1lTW9kZQAAAAADAAAAAAAAAAZOb3JtYWwAAAAAAAAAAAAAAAAACkNsYWltc09ubHkAAAAAAAEAAAAAAAAAC0Z1bGx5UGF1c2VkAAAAAAI=",
        "AAAAAQAAAAAAAAAAAAAADFVzZXJQb3NpdGlvbgAAAAIAAAAAAAAABmFtb3VudAAAAAAACwAAAAAAAAAEc2lkZQAAB9AAAAAHQmV0U2lkZQA=",
        "AAAAAQAAAAAAAAAAAAAADU9yYWNsZVBheWxvYWQAAAAAAAAGAAAAg0NvbnRyYWN0IGFkZHJlc3MgdGhpcyBwYXlsb2FkIGlzIGludGVuZGVkIGZvci4KVmFsaWRhdGVkIGFnYWluc3QgYGVudi5jdXJyZW50X2NvbnRyYWN0X2FkZHJlc3MoKWAgdG8gcHJldmVudCBjcm9zcy1jb250cmFjdCByZXBsYXkuAAAAAA1jb250cmFjdF9hZGRyAAAAAAAAEwAAAItTSEEtMjU2IGhhc2ggb2YgdGhlIG5ldHdvcmsgcGFzc3BocmFzZSB0aGlzIHBheWxvYWQgdGFyZ2V0cy4KVmFsaWRhdGVkIGFnYWluc3QgYGVudi5sZWRnZXIoKS5uZXR3b3JrX2lkKClgIHRvIHByZXZlbnQgY3Jvc3MtbmV0d29yayByZXBsYXkuAAAAAApuZXR3b3JrX2lkAAAAAAPuAAAAIAAAAWRQZXItcm91bmQgcmVwbGF5LXByb3RlY3Rpb24gbm9uY2UuCgpUaGUgb3JhY2xlIHNlcnZpY2UgbXVzdCBnZW5lcmF0ZSBhIHVuaXF1ZSB2YWx1ZSBwZXIgc3VibWlzc2lvbiBmb3IgYQpnaXZlbiByb3VuZCAoZS5nLiBhIG1vbm90b25pYyBjb3VudGVyIG9yIHJhbmRvbSA2NC1iaXQgdmFsdWUpLiBUaGUKY29udHJhY3QgcmVjb3JkcyBlYWNoIGNvbnN1bWVkIG5vbmNlIHVuZGVyCmBEYXRhS2V5OjpDb25zdW1lZE9yYWNsZU5vbmNlKHJvdW5kX2lkLCBub25jZSlgIGFuZCByZWplY3RzIGFueSByZXVzZSwKbWFraW5nIHJlc29sdXRpb24gaWRlbXBvdGVudCBhZ2FpbnN0IGFjY2lkZW50YWwgZHVwbGljYXRlIHN1Ym1pc3Npb25zLgAAAAVub25jZQAAAAAAAAYAAAAAAAAABXByaWNlAAAAAAAACgAAADdSb3VuZCBpZGVudGlmaWVyIHRoYXQgc2hvdWxkIG1hdGNoIGBSb3VuZC5zdGFydF9sZWRnZXJgAAAAAAhyb3VuZF9pZAAAAAQAAAAAAAAACXRpbWVzdGFtcAAAAAAAAAY=",
        "AAAAAwAAAEhJZGVudGlmaWVzIHdoaWNoIGNyaXRpY2FsIHJpc2sgc2V0dGluZyBpcyBwZW5kaW5nIHRpbWVsb2NrZWQgYWN0aXZhdGlvbi4AAAAAAAAAEENvbmZpZ0NoYW5nZUtpbmQAAAAGAAAAAAAAAAdXaW5kb3dzAAAAAAAAAAAAAAAACE1heFN0YWtlAAAAAQAAAAAAAAAUTWF4VXNlclJvdW5kRXhwb3N1cmUAAAACAAAAAAAAABJNYXhQZW5kaW5nV2lubmluZ3MAAAAAAAMAAAAAAAAAFE9yYWNsZVN0YWxlVGhyZXNob2xkAAAABAAAAAAAAAAVT3JhY2xlTWF4RGV2aWF0aW9uQnBzAAAAAAAABQ==",
        "AAAAAwAAAD9UZXJtaW5hbCBvdXRjb21lIHJlY29yZGVkIHdoZW4gYSByb3VuZCBsZWF2ZXMgdGhlIGFjdGl2ZSBzdGF0ZS4AAAAAAAAAABJSb3VuZEFyY2hpdmVTdGF0dXMAAAAAAAMAAAA1T3JhY2xlIHNldHRsZW1lbnQgY29tcGxldGVkIChub3JtYWwgcmVzb2x1dGlvbiBwYXRoKS4AAAAAAAAIUmVzb2x2ZWQAAAAAAAAANEFkbWluIGNhbmNlbGxlZCB0aGUgcm91bmQgYW5kIHJlZnVuZGVkIHBhcnRpY2lwYW50cy4AAAAJQ2FuY2VsbGVkAAAAAAAAAQAAAEVTZXR0bGVtZW50IGFib3J0ZWQgZHVlIHRvIGluc3VmZmljaWVudCBwYXJ0aWNpcGFudHM7IHN0YWtlcyByZWZ1bmRlZC4AAAAAAAAORmFsbGJhY2tSZWZ1bmQAAAAAAAI=",
        "AAAAAgAAAC9QYXlsb2FkIGZvciBhIHNjaGVkdWxlZCBjcml0aWNhbCBjb25maWcgY2hhbmdlLgAAAAAAAAAAE0NvbmZpZ0NoYW5nZVBheWxvYWQAAAAABgAAAAEAAAAAAAAAB1dpbmRvd3MAAAAAAgAAAAQAAAAEAAAAAQAAAAAAAAAITWF4U3Rha2UAAAABAAAD6AAAAAsAAAABAAAAAAAAABRNYXhVc2VyUm91bmRFeHBvc3VyZQAAAAEAAAPoAAAACwAAAAEAAAAAAAAAEk1heFBlbmRpbmdXaW5uaW5ncwAAAAAAAQAAA+gAAAALAAAAAQAAAAAAAAAUT3JhY2xlU3RhbGVUaHJlc2hvbGQAAAABAAAABgAAAAEAAAAAAAAAFU9yYWNsZU1heERldmlhdGlvbkJwcwAAAAAAAAEAAAPoAAAABA==",
        "AAAAAQAAAFNQZW5kaW5nIHRpbWVsb2NrZWQgY29uZmlnIGNoYW5nZSB3aXRoIGFjdGl2YXRpb24gbGVkZ2VyIGZvciBvbi1jaGFpbiBvYnNlcnZhYmlsaXR5LgAAAAAAAAAAE1BlbmRpbmdDb25maWdDaGFuZ2UAAAAAAwAAAAAAAAARYWN0aXZhdGlvbl9sZWRnZXIAAAAAAAAEAAAAAAAAAAdwYXlsb2FkAAAAB9AAAAATQ29uZmlnQ2hhbmdlUGF5bG9hZAAAAAAAAAAAE3NjaGVkdWxlZF9hdF9sZWRnZXIAAAAABA==",
        "AAAAAQAAAAAAAAAAAAAAE1ByZWNpc2lvbkNvbW1pdG1lbnQAAAAAAwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAAAAAARoYXNoAAAD7gAAACAAAAAAAAAACHJldmVhbGVkAAAAAQ==",
        "AAAAAQAAADtQcmVjaXNpb24gcHJlZGljdGlvbiBlbnRyeSAodXNlciBhZGRyZXNzICsgcHJlZGljdGVkIHByaWNlKQAAAAAAAAAAE1ByZWNpc2lvblByZWRpY3Rpb24AAAAAAwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAAAAAA9wcmVkaWN0ZWRfcHJpY2UAAAAACgAAAAAAAAAEdXNlcgAAABM=",
        "AAAAAQAAANFDb21wYWN0IGhpc3RvcmljYWwgcm91bmQgc3VtbWFyeSBwZXJzaXN0ZWQgYWZ0ZXIgcmVzb2x2ZSBvciBjYW5jZWwuCgpEZXNpZ25lZCBmb3IgZXhwbG9yZXIvYW5hbHl0aWNzIHF1ZXJpZXMgd2l0aG91dCByZXBsYXlpbmcgZXZlbnRzLgpgcHJpY2VfZmluYWxgIGlzIGAwYCBmb3IgYWRtaW4gY2FuY2VsbGF0aW9ucyAobm8gb3JhY2xlIHNldHRsZW1lbnQgcHJpY2UpLgAAAAAAAAAAAAAUQXJjaGl2ZWRSb3VuZFN1bW1hcnkAAAAJAAAAAAAAAARtb2RlAAAH0AAAAAlSb3VuZE1vZGUAAAAAAAAAAAAAEXBhcnRpY2lwYW50X2NvdW50AAAAAAAABAAAAAAAAAAJcG9vbF9kb3duAAAAAAAACwAAAAAAAAAHcG9vbF91cAAAAAALAAAAAAAAAAtwcmljZV9maW5hbAAAAAAKAAAAAAAAAAtwcmljZV9zdGFydAAAAAAKAAAAAAAAAAhyb3VuZF9pZAAAAAYAAAAAAAAAEXNldHRsZWRfYXRfbGVkZ2VyAAAAAAAABAAAAAAAAAAGc3RhdHVzAAAAAAfQAAAAElJvdW5kQXJjaGl2ZVN0YXR1cwAA",
        "AAAAAQAAAH5PcmFjbGUgbGl2ZW5lc3MgcmVjb3JkLCB1cGRhdGVkIGJ5IHRoZSBvcmFjbGUgc2VydmljZSBvbiBlYWNoIGhlYXJ0YmVhdCBjYWxsLgpgc3RhdHVzYDogMCA9IGFjdGl2ZSwgMSA9IGRlZ3JhZGVkLCAyID0gb2ZmbGluZS4AAAAAAAAAAAAVT3JhY2xlSGVhcnRiZWF0UmVjb3JkAAAAAAAAAgAAAAAAAAAGc3RhdHVzAAAAAAAEAAAAAAAAAAl0aW1lc3RhbXAAAAAAAAAG",
        "AAAABAAAABRDb250cmFjdCBlcnJvciB0eXBlcwAAAAAAAAANQ29udHJhY3RFcnJvcgAAAAAAADIAAAAlQ29udHJhY3QgaGFzIGFscmVhZHkgYmVlbiBpbml0aWFsaXplZAAAAAAAABJBbHJlYWR5SW5pdGlhbGl6ZWQAAAAAAAEAAAAtQWRtaW4gYWRkcmVzcyBub3Qgc2V0IC0gY2FsbCBpbml0aWFsaXplIGZpcnN0AAAAAAAAC0FkbWluTm90U2V0AAAAAAIAAAAuT3JhY2xlIGFkZHJlc3Mgbm90IHNldCAtIGNhbGwgaW5pdGlhbGl6ZSBmaXJzdAAAAAAADE9yYWNsZU5vdFNldAAAAAMAAAAiT25seSBhZG1pbiBjYW4gcGVyZm9ybSB0aGlzIGFjdGlvbgAAAAAAEVVuYXV0aG9yaXplZEFkbWluAAAAAAAABAAAACNPbmx5IG9yYWNsZSBjYW4gcGVyZm9ybSB0aGlzIGFjdGlvbgAAAAASVW5hdXRob3JpemVkT3JhY2xlAAAAAAAFAAAAJEJldCBhbW91bnQgbXVzdCBiZSBncmVhdGVyIHRoYW4gemVybwAAABBJbnZhbGlkQmV0QW1vdW50AAAABgAAABZObyBhY3RpdmUgcm91bmQgZXhpc3RzAAAAAAANTm9BY3RpdmVSb3VuZAAAAAAAAAcAAAAXUm91bmQgaGFzIGFscmVhZHkgZW5kZWQAAAAAClJvdW5kRW5kZWQAAAAAAAgAAAAdVXNlciBoYXMgaW5zdWZmaWNpZW50IGJhbGFuY2UAAAAAAAATSW5zdWZmaWNpZW50QmFsYW5jZQAAAAAJAAAAK1VzZXIgaGFzIGFscmVhZHkgcGxhY2VkIGEgYmV0IGluIHRoaXMgcm91bmQAAAAACkFscmVhZHlCZXQAAAAAAAoAAAAcQXJpdGhtZXRpYyBvdmVyZmxvdyBvY2N1cnJlZAAAAAhPdmVyZmxvdwAAAAsAAAATSW52YWxpZCBwcmljZSB2YWx1ZQAAAAAMSW52YWxpZFByaWNlAAAADAAAABZJbnZhbGlkIGR1cmF0aW9uIHZhbHVlAAAAAAAPSW52YWxpZER1cmF0aW9uAAAAAA0AAAAjSW52YWxpZCByb3VuZCBtb2RlIChtdXN0IGJlIDAgb3IgMSkAAAAAC0ludmFsaWRNb2RlAAAAAA4AAAAsV3JvbmcgcHJlZGljdGlvbiB0eXBlIGZvciBjdXJyZW50IHJvdW5kIG1vZGUAAAAWV3JvbmdNb2RlRm9yUHJlZGljdGlvbgAAAAAADwAAACRSb3VuZCBoYXMgbm90IHJlYWNoZWQgZW5kX2xlZGdlciB5ZXQAAAANUm91bmROb3RFbmRlZAAAAAAAABAAAAA1SW52YWxpZCBwcmljZSBzY2FsZSAobXVzdCByZXByZXNlbnQgNCBkZWNpbWFsIHBsYWNlcykAAAAAAAARSW52YWxpZFByaWNlU2NhbGUAAAAAAAARAAAAHk9yYWNsZSBkYXRhIGlzIHRvbyBvbGQgKFNUQUxFKQAAAAAAD1N0YWxlT3JhY2xlRGF0YQAAAAASAAAAMU9yYWNsZSBwYXlsb2FkIHJvdW5kX2lkIGRvZXNuJ3QgbWF0Y2ggQWN0aXZlUm91bmQAAAAAAAASSW52YWxpZE9yYWNsZVJvdW5kAAAAAAATAAAAOEFuIGFjdGl2ZSByb3VuZCBhbHJlYWR5IGV4aXN0cyBhbmQgY2Fubm90IGJlIG92ZXJ3cml0dGVuAAAAElJvdW5kQWxyZWFkeUFjdGl2ZQAAAAAAFAAAAC5BZG1pbiBhbmQgT3JhY2xlIGFkZHJlc3NlcyBjYW5ub3QgYmUgaWRlbnRpY2FsAAAAAAANQWRtaW5Jc09yYWNsZQAAAAAAABUAAAApQ29udHJhY3QgaXMgcGF1c2VkIGZvciBlbWVyZ2VuY3kgcmVjb3ZlcnkAAAAAAAAOQ29udHJhY3RQYXVzZWQAAAAAABYAAAA6T25lIG9yIG1vcmUgd2luZG93IHZhbHVlcyBleGNlZWQgY29uZmlndXJlZCBtYXhpbXVtIGJvdW5kcwAAAAAAEFdpbmRvd091dE9mUmFuZ2UAAAAXAAAAKU9yYWNsZSBwYXlsb2FkIHRpbWVzdGFtcCBpcyBpbiB0aGUgZnV0dXJlAAAAAAAAEEZ1dHVyZU9yYWNsZURhdGEAAAAYAAAAPUFyaXRobWV0aWMgb3ZlcmZsb3cgaW4gcGF5b3V0IGFjY3VtdWxhdGlvbiDigJQgbm8gZnVuZHMgbW92ZWQAAAAAAAAOUGF5b3V0T3ZlcmZsb3cAAAAAABkAAAAvUm91bmQgaGFzIGJlZW4gY2FuY2VsbGVkIGFuZCBjYW5ub3QgYmUgcmVzb2x2ZWQAAAAADlJvdW5kQ2FuY2VsbGVkAAAAAAAaAAAAP1JvdW5kIGNhbm5vdCBiZSBjYW5jZWxsZWQgKG5vIGFjdGl2ZSByb3VuZCBvciBhbHJlYWR5IHJlc29sdmVkKQAAAAATUm91bmROb3RDYW5jZWxsYWJsZQAAAAAbAAAAL0JldCBhbW91bnQgZXhjZWVkcyB0aGUgY29uZmlndXJlZCBtYXhpbXVtIHN0YWtlAAAAAA9TdGFrZUV4Y2VlZHNNYXgAAAAAHAAAAENVc2VyJ3MgY3VtdWxhdGl2ZSBleHBvc3VyZSBpbiB0aGlzIHJvdW5kIGV4Y2VlZHMgdGhlIGNvbmZpZ3VyZWQgY2FwAAAAABNFeHBvc3VyZUNhcEV4Y2VlZGVkAAAAAB0AAAA9UGVuZGluZyB3aW5uaW5ncyBhY2N1bXVsYXRpb24gd291bGQgZXhjZWVkIHRoZSBjb25maWd1cmVkIGNhcAAAAAAAABpQZW5kaW5nV2lubmluZ3NDYXBFeGNlZWRlZAAAAAAAHgAAAC5TdGFydCBwcmljZSBpcyBiZWxvdyB0aGUgbWluaW11bSBhbGxvd2VkIHZhbHVlAAAAAAAQU3RhcnRQcmljZVRvb0xvdwAAAB8AAAAtU3RhcnQgcHJpY2UgZXhjZWVkcyB0aGUgbWF4aW11bSBhbGxvd2VkIHZhbHVlAAAAAAAAEVN0YXJ0UHJpY2VUb29IaWdoAAAAAAAAIAAAAEFPcmFjbGUgcGF5bG9hZCBub25jZSB3YXMgYWxyZWFkeSBjb25zdW1lZCBmb3IgdGhpcyByb3VuZCAocmVwbGF5KQAAAAAAABFPcmFjbGVOb25jZVJldXNlZAAAAAAAACEAAABTUm91bmQgaGFzIGZld2VyIHBhcnRpY2lwYW50cyB0aGFuIHRoZSBjb25maWd1cmVkIG1pbmltdW0gZm9yIGNvbXBldGl0aXZlIHNldHRsZW1lbnQAAAAAGEluc3VmZmljaWVudFBhcnRpY2lwYW50cwAAACIAAABETWluaW11bSBwYXJ0aWNpcGFudHMgdmFsdWUgaXMgb3V0IG9mIHZhbGlkIHJhbmdlIChtdXN0IGJlIDHigJMxMDAwMCkAAAAWSW52YWxpZE1pblBhcnRpY2lwYW50cwAAAAAAIwAAADxPcmFjbGUgaGVhcnRiZWF0IHN0YXR1cyBpcyBvdXQgb2YgcmFuZ2UgKG11c3QgYmUgMCwgMSwgb3IgMikAAAATSW52YWxpZE9yYWNsZVN0YXR1cwAAAAAkAAAASU9yYWNsZSBzdGFsZSB0aHJlc2hvbGQgaXMgb3V0IG9mIHZhbGlkIHJhbmdlIChtdXN0IGJlIDYw4oCTODY0MDAgc2Vjb25kcykAAAAAAAAVSW52YWxpZFN0YWxlVGhyZXNob2xkAAAAAAAAJQAAAD1QcmVjaXNpb24gcGFydGljaXBhbnQgY2FwIGlzIG91dCBvZiByYW5nZSAobXVzdCBiZSAx4oCTMTAwMDApAAAAAAAAHkludmFsaWRQcmVjaXNpb25QYXJ0aWNpcGFudENhcAAAAAAAJgAAADpQcmVjaXNpb24gcm91bmQgaGFzIHJlYWNoZWQgdGhlIGNvbmZpZ3VyZWQgcGFydGljaXBhbnQgY2FwAAAAAAAfUHJlY2lzaW9uUGFydGljaXBhbnRDYXBFeGNlZWRlZAAAAAAnAAAAMU9yYWNsZSBtYXggZGV2aWF0aW9uIGJwcyBpcyBpbnZhbGlkIChtdXN0IGJlID4gMCkAAAAAAAAZSW52YWxpZE9yYWNsZURldmlhdGlvbkJwcwAAAAAAACgAAAA3T3JhY2xlIGZpbmFsIHByaWNlIGRldmlhdGVzIGJleW9uZCBjb25maWd1cmVkIHRocmVzaG9sZAAAAAAXT3JhY2xlRGV2aWF0aW9uRXhjZWVkZWQAAAAAKQAAAEZTdG9yZWQgc2NoZW1hIHZlcnNpb24gaXMgdW5rbm93biBvciB1bnN1cHBvcnRlZCBieSB0aGlzIGNvbnRyYWN0IGJ1aWxkAAAAAAAYVW5zdXBwb3J0ZWRTY2hlbWFWZXJzaW9uAAAAKgAAADdNaWdyYXRpb24gcGF0aCBpcyBpbnZhbGlkIGZvciB0aGUgc3RvcmVkIHNjaGVtYSB2ZXJzaW9uAAAAABRJbnZhbGlkTWlncmF0aW9uUGF0aAAAACsAAAAsTWlncmF0aW9uIGNhbm5vdCBydW4gd2hpbGUgYSByb3VuZCBpcyBhY3RpdmUAAAAUTWlncmF0aW9uQWN0aXZlUm91bmQAAAAsAAAALUNvbW1pdG1lbnQgZm9yIHByZWNpc2lvbiBwcmVkaWN0aW9uIG5vdCBmb3VuZAAAAAAAABJDb21taXRtZW50Tm90Rm91bmQAAAAAAC0AAAAuUHJlY2lzaW9uIHByZWRpY3Rpb24gaGFzIGFscmVhZHkgYmVlbiByZXZlYWxlZAAAAAAAD0FscmVhZHlSZXZlYWxlZAAAAAAuAAAAN0F0dGVtcHRlZCB0byByZXZlYWwgcHJlZGljdGlvbiBvdXRzaWRlIHRoZSB2YWxpZCB3aW5kb3cAAAAAE0ludmFsaWRSZXZlYWxXaW5kb3cAAAAALwAAADZSZXZlYWxlZCBwcmVkaWN0aW9uIGhhc2ggZG9lcyBub3QgbWF0Y2ggY29tbWl0dGVkIGhhc2gAAAAAAAxIYXNoTWlzbWF0Y2gAAAAwAAAAPE9yYWNsZSBwYXlsb2FkIG5ldHdvcmtfaWQgZG9lcyBub3QgbWF0Y2ggdGhlIHJ1bnRpbWUgbmV0d29yawAAABVPcmFjbGVOZXR3b3JrTWlzbWF0Y2gAAAAAAAAxAAAAQE9yYWNsZSBwYXlsb2FkIGNvbnRyYWN0X2FkZHIgZG9lcyBub3QgbWF0Y2ggdGhlIGN1cnJlbnQgY29udHJhY3QAAAAWT3JhY2xlQ29udHJhY3RNaXNtYXRjaAAAAAAAMg==",
        "AAAAAAAAABtSZXR1cm5zIHVzZXIncyB2WExNIGJhbGFuY2UAAAAAB2JhbGFuY2UAAAAAAQAAAAAAAAAEdXNlcgAAABMAAAABAAAACw==",
        "AAAAAAAAAAAAAAAJZ2V0X2FkbWluAAAAAAAAAAAAAAEAAAPoAAAAEw==",
        "AAAAAAAAADBSZXR1cm5zIHdoZXRoZXIgdGhlIGNvbnRyYWN0IGlzIGN1cnJlbnRseSBwYXVzZWQAAAAJaXNfcGF1c2VkAAAAAAAAAAAAAAEAAAAB",
        "AAAAAAAAAW5QbGFjZXMgYSBiZXQgb24gdGhlIGFjdGl2ZSByb3VuZCAoVXAvRG93biBtb2RlIG9ubHkpLgoKU3RvcmFnZSBsYXlvdXQ6IGVhY2ggcGFydGljaXBhbnQncyBwb3NpdGlvbiBpcyBzdG9yZWQgdW5kZXIgaXRzIG93bgpjb21wb3NpdGUga2V5IGBEYXRhS2V5OjpQb3NpdGlvbihyb3VuZF9pZCwgdXNlcilgIOKAlCBPKDEpIHJlYWQvd3JpdGUKcmVnYXJkbGVzcyBvZiBob3cgbWFueSBvdGhlciBwYXJ0aWNpcGFudHMgZXhpc3QuIEFuIG9yZGVyZWQgcGFydGljaXBhbnQKbGlzdCBgRGF0YUtleTo6Um91bmRQYXJ0aWNpcGFudHMocm91bmRfaWQpYCBpcyBtYWludGFpbmVkIGZvciBPKG4pCml0ZXJhdGlvbiBhdCByZXNvbHV0aW9uIHRpbWUgb25seS4AAAAAAAlwbGFjZV9iZXQAAAAAAAADAAAAAAAAAAR1c2VyAAAAEwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAAAAAARzaWRlAAAH0AAAAAdCZXRTaWRlAAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAKZ2V0X29yYWNsZQAAAAAAAAAAAAEAAAPoAAAAEw==",
        "AAAAAAAAAEhJbml0aWFsaXplcyB0aGUgY29udHJhY3Qgd2l0aCBhZG1pbiBhbmQgb3JhY2xlIGFkZHJlc3NlcyAob25lLXRpbWUgb25seSkAAAAKaW5pdGlhbGl6ZQAAAAAAAgAAAAAAAAAFYWRtaW4AAAAAAAATAAAAAAAAAAZvcmFjbGUAAAAAABMAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAMNTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHdpbmRvd3MgdXBkYXRlIChhbGlhcyBmb3IgW2BTZWxmOjpzY2hlZHVsZV93aW5kb3dzYF0pLgpiZXRfbGVkZ2VyczogTnVtYmVyIG9mIGxlZGdlcnMgdXNlcnMgY2FuIHBsYWNlIGJldHMKcnVuX2xlZGdlcnM6IFRvdGFsIG51bWJlciBvZiBsZWRnZXJzIGJlZm9yZSByb3VuZCBjYW4gYmUgcmVzb2x2ZWQAAAAAC3NldF93aW5kb3dzAAAAAAIAAAAAAAAAC2JldF9sZWRnZXJzAAAAAAQAAAAAAAAAC3J1bl9sZWRnZXJzAAAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAXRDYW5jZWxzIHRoZSBhY3RpdmUgcm91bmQgYW5kIGRldGVybWluaXN0aWNhbGx5IHJlZnVuZHMgYWxsIHBhcnRpY2lwYW50IHN0YWtlcy4KCk9ubHkgYWRtaW4gbWF5IGNhbmNlbC4gSW50ZW5kZWQgZm9yIG9yYWNsZS11bmF2YWlsYWJsZSBvciBlbWVyZ2VuY3kgcmVjb3ZlcnkKc2NlbmFyaW9zLiBBZnRlciBjYW5jZWxsYXRpb246Ci0gQWxsIHBhcnRpY2lwYW50IHN0YWtlcyBhcmUgbW92ZWQgdG8gdGhlaXIgcGVuZGluZyB3aW5uaW5ncy4KLSBUaGUgYWN0aXZlIHJvdW5kIGlzIHJlbW92ZWQ7IG5vIGZ1dHVyZSBzZXR0bGVtZW50IGlzIHBvc3NpYmxlLgotIFRoZSByb3VuZCBJRCBpcyBtYXJrZWQgY2FuY2VsbGVkIHRvIHByZXZlbnQgYW55IHJlcGxheS4AAAAMY2FuY2VsX3JvdW5kAAAAAQAAAAAAAAAGcmVhc29uAAAAAAAEAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAGBDcmVhdGVzIGEgbmV3IHByZWRpY3Rpb24gcm91bmQgKGFkbWluIG9ubHkpCm1vZGU6IDAgPSBVcC9Eb3duIChkZWZhdWx0KSwgMSA9IFByZWNpc2lvbiAoTGVnZW5kcykAAAAMY3JlYXRlX3JvdW5kAAAAAgAAAAAAAAALc3RhcnRfcHJpY2UAAAAACgAAAAAAAAAEbW9kZQAAA+gAAAAEAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAC1NaW50cyAxMDAwIHZYTE0gZm9yIG5ldyB1c2VycyAob25lLXRpbWUgb25seSkAAAAAAAAMbWludF9pbml0aWFsAAAAAQAAAAAAAAAEdXNlcgAAABMAAAABAAAACw==",
        "AAAAAAAAAC5SZXR1cm5zIHRoZSBjdXJyZW50IG1heGltdW0gc3Rha2UgY2FwLCBpZiBzZXQuAAAAAAANZ2V0X21heF9zdGFrZQAAAAAAAAAAAAABAAAD6AAAAAs=",
        "AAAAAAAAAJdBbGlhcyBmb3IgcGxhY2VfcHJlY2lzaW9uX3ByZWRpY3Rpb24gLSBhbGxvd3MgdXNlcnMgdG8gc3VibWl0IGV4YWN0IHByaWNlIHByZWRpY3Rpb25zCmd1ZXNzZWRfcHJpY2U6IHByaWNlIHNjYWxlZCB0byA0IGRlY2ltYWxzIChlLmcuLCAwLjIyOTcg4oaSIDIyOTcpAAAAAA1wcmVkaWN0X3ByaWNlAAAAAAAAAwAAAAAAAAAEdXNlcgAAABMAAAAAAAAADWd1ZXNzZWRfcHJpY2UAAAAAAAAKAAAAAAAAAAZhbW91bnQAAAAAAAsAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAM1SZXNvbHZlcyB0aGUgcm91bmQgd2l0aCBvcmFjbGUgcGF5bG9hZCAob3JhY2xlIG9ubHkpCk1vZGUgMCAoVXAvRG93bik6IFdpbm5lcnMgc3BsaXQgbG9zZXJzJyBwb29sIHByb3BvcnRpb25hbGx5OyB0aWVzIGdldCByZWZ1bmRzCk1vZGUgMSAoUHJlY2lzaW9uL0xlZ2VuZHMpOiBDbG9zZXN0IGd1ZXNzIHdpbnMgZnVsbCBwb3Q7IHRpZXMgc3BsaXQgZXZlbmx5AAAAAAAADXJlc29sdmVfcm91bmQAAAAAAAABAAAAAAAAAAdwYXlsb2FkAAAAB9AAAAANT3JhY2xlUGF5bG9hZAAAAAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAHFTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIG1heCBzdGFrZSB1cGRhdGUgKGFsaWFzIGZvciBbYFNlbGY6OnNjaGVkdWxlX21heF9zdGFrZWBdKS4KUGFzcyBgTm9uZWAgdG8gZGlzYWJsZSB0aGUgY2FwLgAAAAAAAA1zZXRfbWF4X3N0YWtlAAAAAAAAAQAAAAAAAAAKbWF4X2Ftb3VudAAAAAAD6AAAAAsAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAACtDbGFpbXMgcGVuZGluZyB3aW5uaW5ncyBhbmQgYWRkcyB0byBiYWxhbmNlAAAAAA5jbGFpbV93aW5uaW5ncwAAAAAAAQAAAAAAAAAEdXNlcgAAABMAAAABAAAD6QAAAAsAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAC9SZXR1cm5zIHVzZXIgc3RhdGlzdGljcyAod2lucywgbG9zc2VzLCBzdHJlYWtzKQAAAAAOZ2V0X3VzZXJfc3RhdHMAAAAAAAEAAAAAAAAABHVzZXIAAAATAAAAAQAAB9AAAAAJVXNlclN0YXRzAAAA",
        "AAAAAAAAAJRSZXR1cm5zIGB0cnVlYCBpZiB0aGUgb3JhY2xlIGhhcyBhIG5vbi1zdGFsZSBoZWFydGJlYXQgd2l0aCBzdGF0dXMgbm90IG9mZmxpbmUgKDIpLgpVc2VzIHRoZSBjb25maWd1cmVkIHN0YWxlIHRocmVzaG9sZCwgZGVmYXVsdGluZyB0byAzNjAwIHNlY29uZHMuAAAADmlzX29yYWNsZV9saXZlAAAAAAAAAAAAAQAAAAE=",
        "AAAAAAAAADdQYXVzZXMgdGhlIGNvbnRyYWN0IGZvciBlbWVyZ2VuY3kgcmVjb3ZlcnkgKGFkbWluIG9ubHkpAAAAAA5wYXVzZV9jb250cmFjdAAAAAAAAAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAACpSZXR1cm5zIHRoZSBjdXJyZW50bHkgYWN0aXZlIHJvdW5kLCBpZiBhbnkAAAAAABBnZXRfYWN0aXZlX3JvdW5kAAAAAAAAAAEAAAPoAAAH0AAAAAVSb3VuZAAAAA==",
        "AAAAAAAAAE5SZXR1cm5zIHRoZSBjdXJyZW50IHJ1bnRpbWUgbW9kZSAoMCA9IE5vcm1hbCwgMSA9IENsYWltc09ubHksIDIgPSBGdWxseVBhdXNlZCkAAAAAABBnZXRfcnVudGltZV9tb2RlAAAAAAAAAAEAAAAE",
        "AAAAAAAAAKRTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHVwZGF0ZSB0byBiZXR0aW5nIGFuZCBleGVjdXRpb24gd2luZG93cyAoYWRtaW4gb25seSkuClRoZSBjaGFuZ2UgaXMgc3RvcmVkIHBlbmRpbmcgdW50aWwgYGFwcGx5X3NjaGVkdWxlZF9jaGFuZ2VzYCBpcyBjYWxsZWQgYWZ0ZXIgdGhlIGRlbGF5LgAAABBzY2hlZHVsZV93aW5kb3dzAAAAAgAAAAAAAAALYmV0X2xlZGdlcnMAAAAABAAAAAAAAAALcnVuX2xlZGdlcnMAAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAADJTZXRzIHRoZSBydW50aW1lIG1vZGUgb2YgdGhlIGNvbnRyYWN0IChhZG1pbiBvbmx5KQAAAAAAEHNldF9ydW50aW1lX21vZGUAAAABAAAAAAAAAARtb2RlAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAADFVbnBhdXNlcyB0aGUgY29udHJhY3QgYWZ0ZXIgcmVjb3ZlcnkgKGFkbWluIG9ubHkpAAAAAAAAEHVucGF1c2VfY29udHJhY3QAAAAAAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAEJDb21taXRzIGEgaGFzaGVkIHByZWRpY3Rpb24gYW5kIHN0YWtlIGFtb3VudCAoUHJlY2lzaW9uIG1vZGUgb25seSkAAAAAABFjb21taXRfcHJlZGljdGlvbgAAAAAAAAMAAAAAAAAABHVzZXIAAAATAAAAAAAAAARoYXNoAAAD7gAAACAAAAAAAAAABmFtb3VudAAAAAAACwAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAEVSZXR1cm5zIHRoZSBJRCBvZiB0aGUgbGFzdCBjcmVhdGVkIHJvdW5kICgwIGlmIG5vIHJvdW5kcyBjcmVhdGVkIHlldCkAAAAAAAARZ2V0X2xhc3Rfcm91bmRfaWQAAAAAAAAAAAAAAQAAAAY=",
        "AAAAAAAAAO1SZXR1cm5zIHVzZXIncyBwb3NpdGlvbiBpbiB0aGUgY3VycmVudCByb3VuZCAoVXAvRG93biBtb2RlKS4KClJlYWRzIGEgc2luZ2xlIGNvbXBvc2l0ZSBrZXkgYERhdGFLZXk6OlBvc2l0aW9uKHJvdW5kX2lkLCB1c2VyKWAg4oCUIE8oMSkuCkZhbGxzIGJhY2sgdG8gbGVnYWN5IGBVcERvd25Qb3NpdGlvbnNgIC8gYFBvc2l0aW9uc2AgbWFwIGJsb2JzIGZvcgpvbmUtdGltZSBtaWdyYXRpb24gY29tcGF0aWJpbGl0eS4AAAAAAAARZ2V0X3VzZXJfcG9zaXRpb24AAAAAAAABAAAAAAAAAAR1c2VyAAAAEwAAAAEAAAPoAAAH0AAAAAxVc2VyUG9zaXRpb24=",
        "AAAAAAAAAD9SZXZlYWxzIGEgcHJldmlvdXNseSBjb21taXR0ZWQgcHJlZGljdGlvbiAoUHJlY2lzaW9uIG1vZGUgb25seSkAAAAAEXJldmVhbF9wcmVkaWN0aW9uAAAAAAAAAwAAAAAAAAAEdXNlcgAAABMAAAAAAAAAD3ByZWRpY3RlZF9wcmljZQAAAAAKAAAAAAAAAARzYWx0AAAD7gAAACAAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAEJSZXR1cm5zIGEgY29tcGFjdCBhcmNoaXZlZCByb3VuZCBzdW1tYXJ5IGJ5IHJvdW5kIGlkLCBpZiByZXRhaW5lZC4AAAAAABJnZXRfYXJjaGl2ZWRfcm91bmQAAAAAAAEAAAAAAAAACHJvdW5kX2lkAAAABgAAAAEAAAPoAAAH0AAAABRBcmNoaXZlZFJvdW5kU3VtbWFyeQ==",
        "AAAAAAAAAEZSZXR1cm5zIHRoZSBzdG9yZWQgc2NoZW1hIHZlcnNpb24uIElmIHVuc2V0LCByZXR1cm5zIGxlZ2FjeSB2ZXJzaW9uIDEuAAAAAAASZ2V0X3NjaGVtYV92ZXJzaW9uAAAAAAAAAAAAAQAAAAQ=",
        "AAAAAAAAADFSZXR1cm5zIHRydWUgaWYgdGhlIGdpdmVuIHJvdW5kX2lkIHdhcyBjYW5jZWxsZWQuAAAAAAAAEmlzX3JvdW5kX2NhbmNlbGxlZAAAAAAAAQAAAAAAAAAIcm91bmRfaWQAAAAGAAAAAQAAAAE=",
        "AAAAAAAAAERTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHVwZGF0ZSB0byB0aGUgbWF4aW11bSBzdGFrZSBjYXAgKGFkbWluIG9ubHkpLgAAABJzY2hlZHVsZV9tYXhfc3Rha2UAAAAAAAEAAAAAAAAACm1heF9hbW91bnQAAAAAA+gAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAEpDYW5jZWxzIGEgcGVuZGluZyB0aW1lbG9ja2VkIGNvbmZpZyBjaGFuZ2UgYmVmb3JlIGFjdGl2YXRpb24gKGFkbWluIG9ubHkpLgAAAAAAFGNhbmNlbF9jb25maWdfY2hhbmdlAAAAAQAAAAAAAAAEa2luZAAAB9AAAAAQQ29uZmlnQ2hhbmdlS2luZAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAADpSZXR1cm5zIHRoZSBjdXJyZW50IG1pbmltdW0gcGFydGljaXBhbnQgdGhyZXNob2xkLCBpZiBzZXQuAAAAAAAUZ2V0X21pbl9wYXJ0aWNpcGFudHMAAAAAAAAAAQAAA+gAAAAE",
        "AAAAAAAAADhSZXR1cm5zIHRoZSBtb3N0IHJlY2VudCBvcmFjbGUgaGVhcnRiZWF0IHJlY29yZCwgaWYgYW55LgAAABRnZXRfb3JhY2xlX2hlYXJ0YmVhdAAAAAAAAAABAAAD6AAAB9AAAAAVT3JhY2xlSGVhcnRiZWF0UmVjb3JkAAAA",
        "AAAAAAAAACFSZXR1cm5zIHVzZXIncyBjbGFpbWFibGUgd2lubmluZ3MAAAAAAAAUZ2V0X3BlbmRpbmdfd2lubmluZ3MAAAABAAAAAAAAAAR1c2VyAAAAEwAAAAEAAAAL",
        "AAAAAAAAAH9SZXR1cm5zIGFsbCBVcC9Eb3duIHBvc2l0aW9ucyBmb3IgdGhlIGN1cnJlbnQgcm91bmQuCgpSZWFkcyB0aGUgcGFydGljaXBhbnQgbGlzdCBvbmNlLCB0aGVuIGZldGNoZXMgZWFjaCBwb3NpdGlvbiBpbmRpdmlkdWFsbHkuAAAAABRnZXRfdXBkb3duX3Bvc2l0aW9ucwAAAAAAAAABAAAD7AAAABMAAAfQAAAADFVzZXJQb3NpdGlvbg==",
        "AAAAAAAAAMFTZXRzIHRoZSBtaW5pbXVtIHBhcnRpY2lwYW50IGNvdW50IHJlcXVpcmVkIGZvciBjb21wZXRpdGl2ZSBzZXR0bGVtZW50IChhZG1pbiBvbmx5KS4KUm91bmRzIHRoYXQgZW5kIGJlbG93IHRoaXMgdGhyZXNob2xkIGFyZSByZWZ1bmRlZCB0byBhbGwgcGFydGljaXBhbnRzLgpQYXNzIGBOb25lYCB0byBkaXNhYmxlIHRoZSB0aHJlc2hvbGQuAAAAAAAAFHNldF9taW5fcGFydGljaXBhbnRzAAAAAQAAAAAAAAADbWluAAAAA+gAAAAEAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAADhSZXR1cm5zIHRoZSBjdXJyZW50IHBlci11c2VyIHJvdW5kIGV4cG9zdXJlIGNhcCwgaWYgc2V0LgAAABVnZXRfbWF4X3VzZXJfZXhwb3N1cmUAAAAAAAAAAAAAAQAAA+gAAAAL",
        "AAAAAAAAAElTdGltYXRlcyBwYXlvdXRzIGZvciB0aGUgYWN0aXZlIHJvdW5kIGdpdmVuIGEgaHlwb3RoZXRpY2FsIGZpbmFsIHByaWNlLgAAAAAPc2ltdWxhdGVfcGF5b3V0AAAAAAAAAQAAAAAAAAAMZmluYWxfcHJpY2UAAAAACgAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAHxTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIGV4cG9zdXJlIGNhcCB1cGRhdGUgKGFsaWFzIGZvciBbYFNlbGY6OnNjaGVkdWxlX21heF91c2VyX2V4cG9zdXJlYF0pLgpQYXNzIGBOb25lYCB0byBkaXNhYmxlIHRoZSBjYXAuAAAAFXNldF9tYXhfdXNlcl9leHBvc3VyZQAAAAAAAAEAAAAAAAAADG1heF9leHBvc3VyZQAAA+gAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAFRBcHBsaWVzIGEgc2NoZWR1bGVkIGNyaXRpY2FsIGNvbmZpZyBjaGFuZ2UgYWZ0ZXIgaXRzIGFjdGl2YXRpb24gbGVkZ2VyIChhbnkgY2FsbGVyKS4AAAAXYXBwbHlfc2NoZWR1bGVkX2NoYW5nZXMAAAAAAQAAAAAAAAAEa2luZAAAB9AAAAAQQ29uZmlnQ2hhbmdlS2luZAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAANBNaWdyYXRlcyBsZWdhY3kgc2NoZW1hIHZlcnNpb24gMSDihpIgY3VycmVudCBzY2hlbWEgdmVyc2lvbiAyIChhZG1pbiBvbmx5KS4KCkd1YXJkcmFpbHM6Ci0gTXVzdCBub3QgaGF2ZSBhbiBhY3RpdmUgcm91bmQgKGF2b2lkcyBwYXJ0aWFsIHN0YXRlIGludGVycHJldGF0aW9uIGNoYW5nZXMpCi0gT25seSBzdXBwb3J0cyB2MSDihpIgdjIgaW4gdGhpcyByZWxlYXNlAAAAF21pZ3JhdGVfc2NoZW1hX3YxX3RvX3YyAAAAAAAAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAJ1SZWNvcmRzIGFuIG9yYWNsZSBoZWFydGJlYXQgKG9yYWNsZSBvbmx5KS4KYHN0YXR1c2A6IDAgPSBhY3RpdmUsIDEgPSBkZWdyYWRlZCwgMiA9IG9mZmxpbmUuClN0b3JlcyBjdXJyZW50IGxlZGdlciB0aW1lc3RhbXA7IGVtaXRzIGAoIm9yYWNsZSIsICJoZWFydGJlYXQiKWAuAAAAAAAAF3VwZGF0ZV9vcmFjbGVfaGVhcnRiZWF0AAAAAAEAAAAAAAAABnN0YXR1cwAAAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAADlSZXR1cm5zIHRoZSBjdXJyZW50IG1heGltdW0gcGVuZGluZyB3aW5uaW5ncyBjYXAsIGlmIHNldC4AAAAAAAAYZ2V0X21heF9wZW5kaW5nX3dpbm5pbmdzAAAAAAAAAAEAAAPoAAAACw==",
        "AAAAAAAAAIdTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHBlbmRpbmcgd2lubmluZ3MgY2FwIHVwZGF0ZSAoYWxpYXMgZm9yIFtgU2VsZjo6c2NoZWR1bGVfbWF4X3BlbmRpbmdfd2lubmluZ3NgXSkuClBhc3MgYE5vbmVgIHRvIGRpc2FibGUgdGhlIGNhcC4AAAAAGHNldF9tYXhfcGVuZGluZ193aW5uaW5nGgAAAAEAAAAAAAAAC21heF9wZW5kaW5nAAAAA+gAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAEFSZXR1cm5zIGEgcGVuZGluZyB0aW1lbG9ja2VkIGNvbmZpZyBjaGFuZ2UgZm9yIHRoZSBnaXZlbiBraW5kLCBpZiBhbnkuAAAAAAAZZ2V0X3BlbmRpbmdfY29uZmlnX2NoYW5nZQAAAAAAAAEAAAAAAAAABGtpbmQAAAfQAAAAEENvbmZpZ0NoYW5nZUtpbmQAAAABAAAD6AAAB9AAAAATUGVuZGluZ0NvbmZpZ0NoYW5nZQA=",
        "AAAAAAAAANZSZXR1cm5zIGFsbCBwcmVjaXNpb24gcHJlZGljdGlvbnMgZm9yIHRoZSBjdXJyZW50IHJvdW5kLgoKUmVhZHMgdGhlIHBhcnRpY2lwYW50IGxpc3Qgb25jZSwgdGhlbiBmZXRjaGVzIGVhY2ggcHJlZGljdGlvbiBpbmRpdmlkdWFsbHkuClRvdGFsIHJlYWRzOiAxIChwYXJ0aWNpcGFudCBsaXN0KSArIE4gKHByZWRpY3Rpb25zKSBpbnN0ZWFkIG9mIDEgbGFyZ2UgbWFwIGJsb2IuAAAAAAAZZ2V0X3ByZWNpc2lvbl9wcmVkaWN0aW9ucwAAAAAAAAAAAAABAAAD6gAAB9AAAAATUHJlY2lzaW9uUHJlZGljdGlvbmA=",
        "AAAAAAAAAjJSZXR1cm5zIGEgZGV0ZXJtaW5pc3RpYyBzbGljZSBvZiBVcC9Eb3duIHBvc2l0aW9ucyBmb3IgdGhlIGFjdGl2ZQpyb3VuZCwgb3JkZXJlZCBieSBhc2NlbmRpbmcgcGFydGljaXBhbnQgYWRkcmVzcywgYXMgYChBZGRyZXNzLApVc2VyUG9zaXRpb24pYCBwYWlycy4KCkEgYFZlY2Agb2YgcGFpcnMgaXMgdXNlZCBpbnN0ZWFkIG9mIGEgYE1hcGAgYmVjYXVzZSBwYWdpbmF0aW9uIG92ZXIgYQpgTWFwYCBoYXMgbm8gc3RhYmxlLCBjYWxsZXItY29udHJvbGxhYmxlIHNsaWNlIHNlbWFudGljcyBpbiBTb3JvYmFuIOKAlApwYWlycyBwcmVzZXJ2ZSB0aGUgZXhhY3Qgb2Zmc2V0L2xpbWl0IHdpbmRvdyB0aGUgY2FsbGVyIHJlcXVlc3RlZC4KClNlZSBbYFNlbGY6OmdldF9wcmVjaXNpb25fcHJlZGljdGlvbnNfcGFnZWBdIGZvciB0aGUgb2Zmc2V0L2xpbWl0L2VtcHR5LXBhZ2UKY29udHJhY3QsIHdoaWNoIGlzIGlkZW50aWNhbCBoZXJlLiBUaGlzIGRvZXMgbm90IHJlcGxhY2UKW2BTZWxmOjpnZXRfdXBkb3duX3Bvc2l0aW9uc2BdLCB3aGljaCByZW1haW5zIGF2YWlsYWJsZSB1bmNoYW5nZWQuAAAAAAAZZ2V0X3VwZG93bl9wb3NpdGlvbnNfcGFnZQAAAAAAAAIAAAAAAAAABm9mZnNldAAAAAAABAAAAAAAAAAFbGltaXQAAAAAAAAEAAAAAQAAA+oAAAPtAAAAAgAAABMAAAfQAAAADFVzZXJQb3NpdGlvbg==",
        "AAAAAAAAAFJSZXR1cm5zIHRoZSBjb25maWd1cmVkIG9yYWNsZSBzdGFsZSB0aHJlc2hvbGQsIG9yIHRoZSBkZWZhdWx0ICgzNjAwIHMpIGlmIG5vdCBzZXQuAAAAAAAaZ2V0X29yYWNsZV9zdGFsZV90aHJlc2hvbGQAAAAAAAAAAAABAAAABg==",
        "AAAAAAAAAK5SZXR1cm5zIHVwIHRvIGBsaW1pdGAgbW9zdCByZWNlbnRseSBhcmNoaXZlZCByb3VuZHMgKG5ld2VzdCBmaXJzdCkuCgpQYXNzIGBsaW1pdCA9IDBgIHRvIHJlY2VpdmUgYW4gZW1wdHkgbGlzdC4gVmFsdWVzIGFib3ZlIFtgTUFYX0FSQ0hJVkVEX1JPVU5EU2BdCmFyZSBjYXBwZWQgYXV0b21hdGljYWxseS4AAAAAABpnZXRfcmVjZW50X2FyY2hpdmVkX3JvdW5kcwAAAAAAAQAAAAAAAAAFbGltaXQAAAAAAAAEAAAAAQAAA+oAAAfQAAAAFEFyY2hpdmVkUm91bmRTdW1tYXJ5",
        "AAAAAAAAAQZQbGFjZXMgYSBwcmVjaXNpb24gcHJlZGljdGlvbiBvbiB0aGUgYWN0aXZlIHJvdW5kIChQcmVjaXNpb24vTGVnZW5kcyBtb2RlIG9ubHkpCnByZWRpY3RlZF9wcmljZTogcHJpY2Ugc2NhbGVkIHRvIDQgZGVjaW1hbHMgKGUuZy4sIDAuMjI5NyDihpIgMjI5NykKClBlci11c2VyIGtleSBgRGF0YUtleTo6UHJlY2lzaW9uUG9zaXRpb24ocm91bmRfaWQsIHVzZXIpYCBnaXZlcyBPKDEpCndyaXRlIGNvc3QgaW5kZXBlbmRlbnQgb2YgcGFydGljaXBhbnQgY291bnQuAAAAAAAacGxhY2VfcHJlY2lzaW9uX3ByZWRpY3Rpb24AAAAAAAMAAAAAAAAABHVzZXIAAAATAAAAAAAAAAZhbW91bnQAAAAAAAsAAAAAAAAAD3ByZWRpY3RlZF9wcmljZQAAAAAKAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAE5TY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHVwZGF0ZSB0byB0aGUgcGVyLXVzZXIgcm91bmQgZXhwb3N1cmUgY2FwIChhZG1pbiBvbmx5KS4AAAAAABpzY2hlZHVsZV9tYXhfdXNlcl9leHBvc3VyZQAAAAAAAQAAAAAAAAAMbWF4X2V4cG9zdXJlAAAD6AAAAAsAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAJ5TY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHN0YWxlIHRocmVzaG9sZCB1cGRhdGUgKGFsaWFzIGZvciBbYFNlbGY6OnNjaGVkdWxlX29yYWNsZV9zdGFsZV90aHJlc2hvbGRgXSkuCkFsbG93ZWQgcmFuZ2U6IDYw4oCTODY0MDAgc2Vjb25kcyAoMSBtaW51dGUgdG8gMjQgaG91cnMpLgAAAAAAGnNldF9vcmFjbGVfc3RhbGVfdGhyZXNob2xkAAAAAAABAAAAAAAAAAdzZWNvbmRzAAAAAAYAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAADhSZXR1cm5zIHRoZSBjb25maWd1cmVkIG9yYWNsZSBtYXggZGV2aWF0aW9uIGJwcywgaWYgc2V0LgAAABxnZXRfb3JhY2xlX21heF9kZXZpYXRpb25fYnBzAAAAAAAAAAEAAAPoAAAABA==",
        "AAAAAAAAAN5TY2hlZHVsZXMgYSB0aW1lbG9ja2VkIG9yYWNsZSBkZXZpYXRpb24gdXBkYXRlIChhbGlhcyBmb3IgW2BTZWxmOjpzY2hlZHVsZV9vcmFjbGVfZGV2aWF0aW9uX2Jwc2BdKS4KCi0gYE5vbmVgOiBkaXNhYmxlcyBkZXZpYXRpb24gZ3VhcmRyYWlscwotIGBTb21lKGJwcylgOiBlbmFibGVzIGd1YXJkcmFpbHMgd2l0aCBhIHRocmVzaG9sZCBpbiBiYXNpcyBwb2ludHMgKDEgYnAgPSAwLjAxJSkAAAAAABxzZXRfb3JhY2xlX21heF9kZXZpYXRpb25fYnBzAAAAAQAAAAAAAAADYnBzAAAAA+gAAAAEAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAJdBcm1zIGEgb25lLXNob3Qgb3ZlcnJpZGUgdG8gYnlwYXNzIGRldmlhdGlvbiBjaGVja3MgZm9yIHRoZSBuZXh0IHNldHRsZW1lbnQgKGFkbWluIG9ubHkpLgpUaGUgZmxhZyBpcyBhdXRvbWF0aWNhbGx5IGNsZWFyZWQgYWZ0ZXIgYSBzZXR0bGVtZW50IHVzZXMgaXQuAAAAAB1hcm1fb3JhY2xlX2RldmlhdGlvbl9vdmVycmlkZQAAAAAAAAAAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAOpSZXR1cm5zIHVzZXIncyBwcmVjaXNpb24gcHJlZGljdGlvbiBpbiB0aGUgY3VycmVudCByb3VuZCAoUHJlY2lzaW9uIG1vZGUpLgoKUmVhZHMgYSBzaW5nbGUgY29tcG9zaXRlIGtleSBgRGF0YUtleTo6UHJlY2lzaW9uUG9zaXRpb24ocm91bmRfaWQsIHVzZXIpYCDigJQgTygxKS4KRmFsbHMgYmFjayB0byBsZWdhY3kgYFByZWNpc2lvblBvc2l0aW9uc2AgbWFwIGZvciBtaWdyYXRpb24gY29tcGF0aWJpbGl0eS4AAAAAAB1nZXRfdXNlcl9wcmVjaXNpb25fcHJlZGljdGlvbgAAAAAAAAEAAAAAAAAABHVzZXIAAAATAAAAAQAAA+gAAAfQAAAAE1ByZWNpc2lvblByZWRpY3Rpb24A",
        "AAAAAAAAAEdTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHVwZGF0ZSB0byB0aGUgcGVuZGluZyB3aW5uaW5ncyBjYXAgKGFkbWluIG9ubHkpLgAAAAAdc2NoZWR1bGVfbWF4X3BlbmRpbmdfd2lubmluZ3MAAAAAAAABAAAAAAAAAAttYXhfcGVuZGluZwAAAAPoAAAACwAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAFFTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHVwZGF0ZSB0byB0aGUgb3JhY2xlIG1heCBkZXZpYXRpb24gdGhyZXNob2xkIChhZG1pbiBvbmx5KS4AAAAAAAAdc2NoZWR1bGVfb3JhY2xlX2RldmlhdGlvbl9icHMAAAAAAAABAAAAAAAAAANicHMAAAAD6AAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAEpSZXR1cm5zIHRoZSBjb25maWd1cmVkIFByZWNpc2lvbiBwYXJ0aWNpcGFudCBjYXAsIG9yIHRoZSBkZWZhdWx0IGlmIHVuc2V0LgAAAAAAHmdldF9tYXhfcHJlY2lzaW9uX3BhcnRpY2lwYW50cwAAAAAAAAAAAAEAAAAE",
        "AAAAAAAAAyVSZXR1cm5zIGEgZGV0ZXJtaW5pc3RpYyBzbGljZSBvZiBQcmVjaXNpb24tbW9kZSBwcmVkaWN0aW9ucyBmb3IgdGhlCmFjdGl2ZSByb3VuZCwgb3JkZXJlZCBieSBhc2NlbmRpbmcgcGFydGljaXBhbnQgYWRkcmVzcyAodGhlIHNhbWUKY2Fub25pY2FsIG9yZGVyIHVzZWQgaW50ZXJuYWxseSBmb3IgcGF5b3V0LXJlbWFpbmRlciBhc3NpZ25tZW50KS4KCmBvZmZzZXRgIGlzIHRoZSB6ZXJvLWJhc2VkIGluZGV4IGludGVOIHRoZSBvcmRlcmVkIHBhcnRpY2lwYW50IGxpc3QuCmBsaW1pdGAgaXMgdGhlIG1heGltdW0gbnVtYmVyIG9mIGVudHJpZXMgdG8gcmV0dXJuIGFuZCBpcyBjYXBwZWQgYXQKYE1BWF9QQUdFX1NJWkVgIHRvIGJvdW5kIGdhcy9yZWFkIGNvc3RzIHJlZ2FyZGxlc3Mgb2YgY2FsbGVyIGlucHV0LgoKUmV0dXJucyBhbiBlbXB0eSBgVmVjYCBpZiB0aGVyZSBpcyBubyBhY3RpdmUgcm91bmQsIGlmIGBvZmZzZXRgIGlzCmJleW9uZCB0aGUgbnVtYmVyIG9mIGF2YWlsYWJsZSBlbnRyaWVzLCBvciBpZiBgbGltaXRgIGlzIHplcm8g4oCUIHRoaXMKaXMgbm90IGFuIGVycm9yIGNvbmRpdGlvbiwgbWF0Y2hpbmcgc3RhbmRhcmQgcGFnaW5hdGlvbiBzZW1hbnRpY3MKKGFza2luZyBwYXN0IHRoZSBlbmQgb2YgYSBsaXN0IHlpZWxkcyBhbiBlbXB0eSBwYWdlLCBub3QgYSBmYXVsdCkuCgpUaGlzIGRvZXMgbm90IHJlcGxhY2UgW2BTZWxmOjpnZXRfcHJlY2lzaW9uX3ByZWRpY3Rpb25zYF0sIHdoaWNoCnJlbWFpbnMgYXZhaWxhYmxlIHVuY2hhbmdlZCBmb3IgZnVsbC1zZXQgcmVhZHMgb24gc21hbGwgcm91bmRzLgAAAAAAHmdldF9wcmVjaXNpb25fcHJlZGljdGlvbnNfcGFnZQAAAAAAAgAAAAAAAAAGb2Zmc2V0AAAAAAAEAAAAAAAAAAVsaW1pdAAAAAAAAAQAAAABAAAD6gAAB9AAAAATUHJlY2lzaW9uUHJlZGljdGlvbmA=",
        "AAAAAAAAALBTZXRzIHRoZSBtYXhpbXVtIHBhcnRpY2lwYW50IGNvdW50IGZvciBQcmVjaXNpb24gcm91bmRzIChhZG1pbiBvbmx5KS4KVGhlIHZhbHVlIG11c3QgYmUgaW4gdGhlIHJhbmdlIDEuLj0xMF8wMDAuIFVuc2V0IGNvbnRyYWN0cyB1c2UgdGhlCnByb3RvY29sIGRlZmF1bHQgb2YgMV8wMDAgcGFydGljaXBhbnRzLgAAAB5zZXRfbWF4X3ByZWNpc2lvbl9wYXJ0aWNpcGFudHMAAAAAAAEAAAAAAAAAA21heAAAAAAEAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAElTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHVwZGF0ZSB0byB0aGUgb3JhY2xlIHN0YWxlIHRocmVzaG9sZCAoYWRtaW4gb25seSkuAAAAAAAAH3NjaGVkdWxlX29yYWNsZV9zdGFsZV90aHJlc2hvbGQAAAAAAQAAAAAAAAAHc2Vjb25kcwAAAAAGAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==" ]),
      options
    )
  }
  public readonly fromJSON = {
    balance: this.txFromJSON<i128>,
        get_admin: this.txFromJSON<Option<string>>,
        is_paused: this.txFromJSON<boolean>,
        place_bet: this.txFromJSON<Result<void>>,
        get_oracle: this.txFromJSON<Option<string>>,
        initialize: this.txFromJSON<Result<void>>,
        set_windows: this.txFromJSON<Result<void>>,
        cancel_round: this.txFromJSON<Result<void>>,
        create_round: this.txFromJSON<Result<void>>,
        mint_initial: this.txFromJSON<i128>,
        get_max_stake: this.txFromJSON<Option<i128>>,
        predict_price: this.txFromJSON<Result<void>>,
        resolve_round: this.txFromJSON<Result<void>>,
        set_max_stake: this.txFromJSON<Result<void>>,
        claim_winnings: this.txFromJSON<Result<i128>>,
        get_user_stats: this.txFromJSON<UserStats>,
        is_oracle_live: this.txFromJSON<boolean>,
        pause_contract: this.txFromJSON<Result<void>>,
        get_active_round: this.txFromJSON<Option<Round>>,
        get_runtime_mode: this.txFromJSON<u32>,
        schedule_windows: this.txFromJSON<Result<void>>,
        set_runtime_mode: this.txFromJSON<Result<void>>,
        get_round_phase: this.txFromJSON<Result<RoundPhase>>,
        unpause_contract: this.txFromJSON<Result<void>>,
        commit_prediction: this.txFromJSON<Result<void>>,
        get_last_round_id: this.txFromJSON<u64>,
        get_user_position: this.txFromJSON<Option<UserPosition>>,
        reveal_prediction: this.txFromJSON<Result<void>>,
        get_archived_round: this.txFromJSON<Option<ArchivedRoundSummary>>,
        get_schema_version: this.txFromJSON<u32>,
        is_round_cancelled: this.txFromJSON<boolean>,
        schedule_max_stake: this.txFromJSON<Result<void>>,
        cancel_config_change: this.txFromJSON<Result<void>>,
        get_min_participants: this.txFromJSON<Option<u32>>,
        get_oracle_heartbeat: this.txFromJSON<Option<OracleHeartbeatRecord>>,
        get_pending_winnings: this.txFromJSON<i128>,
        get_updown_positions: this.txFromJSON<Map<string, UserPosition>>,
        set_min_participants: this.txFromJSON<Result<void>>,
        get_max_user_exposure: this.txFromJSON<Option<i128>>,
        simulate_payout: this.txFromJSON<Result<SimulationResult>>,
        set_max_user_exposure: this.txFromJSON<Result<void>>,
        apply_scheduled_changes: this.txFromJSON<Result<void>>,
        migrate_schema_v1_to_v2: this.txFromJSON<Result<void>>,
        update_oracle_heartbeat: this.txFromJSON<Result<void>>,
        get_max_pending_winnings: this.txFromJSON<Option<i128>>,
        set_max_pending_winnings: this.txFromJSON<Result<void>>,
        get_pending_config_change: this.txFromJSON<Option<PendingConfigChange>>,
        get_precision_predictions: this.txFromJSON<Array<PrecisionPrediction>>,
        get_updown_positions_page: this.txFromJSON<Array<readonly [string, UserPosition]>>,
        get_oracle_stale_threshold: this.txFromJSON<u64>,
        get_recent_archived_rounds: this.txFromJSON<Array<ArchivedRoundSummary>>,
        get_user_archive_history: this.txFromJSON<Array<ArchivedRoundSummary>>,
        place_precision_prediction: this.txFromJSON<Result<void>>,
        schedule_max_user_exposure: this.txFromJSON<Result<void>>,
        set_oracle_stale_threshold: this.txFromJSON<Result<void>>,
        get_oracle_max_deviation_bps: this.txFromJSON<Option<u32>>,
        set_oracle_max_deviation_bps: this.txFromJSON<Result<void>>,
        arm_oracle_deviation_override: this.txFromJSON<Result<void>>,
        get_user_precision_prediction: this.txFromJSON<Option<PrecisionPrediction>>,
        schedule_max_pending_winnings: this.txFromJSON<Result<void>>,
        schedule_oracle_deviation_bps: this.txFromJSON<Result<void>>,
        get_max_precision_participants: this.txFromJSON<u32>,
        get_precision_predictions_page: this.txFromJSON<Array<PrecisionPrediction>>,
        set_max_precision_participants: this.txFromJSON<Result<void>>,
        schedule_oracle_stale_threshold: this.txFromJSON<Result<void>>,
        migrate_schema_v2_to_v3: this.txFromJSON<Result<void>>,
        set_oracle_min_confidence_bps: this.txFromJSON<Result<void>>,
        set_oracle_strict_mode: this.txFromJSON<Result<void>>,
        get_oracle_min_confidence_bps: this.txFromJSON<Option<u32>>,
        get_oracle_strict_mode: this.txFromJSON<boolean>,
        get_protocol_health: this.txFromJSON<ProtocolHealthStatus>,
        get_protocol_status: this.txFromJSON<ProtocolStatus>,
        get_round_status: this.txFromJSON<RoundStatus>,
        propose_oracle_rotation: this.txFromJSON<Result<void>>,
        accept_oracle_rotation: this.txFromJSON<Result<void>>,
        cancel_oracle_rotation: this.txFromJSON<Result<void>>,
        get_oracle_rotation_proposal: this.txFromJSON<Option<OracleRotationProposal>>,
        schedule_protocol_fee_bps: this.txFromJSON<Result<void>>,
        set_protocol_fee_bps: this.txFromJSON<Result<void>>,
        get_protocol_fee_bps: this.txFromJSON<Option<u32>>,
        get_protocol_fee_treasury: this.txFromJSON<i128>,
        withdraw_protocol_fee: this.txFromJSON<Result<i128>>,
        set_mint_limit: this.txFromJSON<Result<void>>,
        get_mint_limit: this.txFromJSON<u32>,
        set_epoch_mint_budget: this.txFromJSON<Result<void>>,
        get_epoch_mint_budget: this.txFromJSON<i128>,
        set_archive_retention: this.txFromJSON<Result<void>>,
        get_archive_retention: this.txFromJSON<u32>,
        set_close_buffer_ledgers: this.txFromJSON<Result<void>>,
        get_close_buffer_ledgers: this.txFromJSON<u32>,
        get_round_pool_stats: this.txFromJSON<Option<RoundPoolStats>>,
        get_user_archived_participation: this.txFromJSON<Option<UserRoundOutcome>>,
        set_hb_strict_mode: this.txFromJSON<Result<void>>,
        get_hb_strict_mode: this.txFromJSON<boolean>,
        arm_hb_override: this.txFromJSON<Result<void>>,
        get_hb_override_armed: this.txFromJSON<boolean>,
        set_hb_grace_seconds: this.txFromJSON<Result<void>>,
        get_hb_grace_seconds: this.txFromJSON<u32>,
        set_precision_payout_policy: this.txFromJSON<Result<void>>,
        get_precision_payout_policy: this.txFromJSON<u32>,
        set_round_template: this.txFromJSON<Result<void>>,
        clear_round_template: this.txFromJSON<Result<void>>,
        get_round_template: this.txFromJSON<Option<RoundTemplate>>,
        create_next_from_template: this.txFromJSON<Result<void>>,
        get_leaderboard_by_wins: this.txFromJSON<Array<LeaderboardEntry>>,
        get_leaderboard_by_streak: this.txFromJSON<Array<LeaderboardEntry>>,
        get_current_season_id: this.txFromJSON<u32>,
        get_season_user_stats: this.txFromJSON<UserStats>,
        reset_leaderboard_season: this.txFromJSON<Result<u32>>,
        get_season_archive: this.txFromJSON<Option<SeasonArchive>>,
        get_season_leaderboard_by_wins: this.txFromJSON<Array<SeasonLeaderboardEntry>>,
        get_season_leaderboard_by_streak: this.txFromJSON<Array<SeasonLeaderboardEntry>>
  }
}