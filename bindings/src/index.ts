import { Buffer } from "buffer";
import { Address } from '@stellar/stellar-sdk';
import {
  AssembledTransaction,
  Client as ContractClient,
  ClientOptions as ContractClientOptions,
  MethodOptions,
  Result,
  Spec as ContractSpec,
} from '@stellar/stellar-sdk/contract';
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
  Typepoint,
  Duration,
} from '@stellar/stellar-sdk/contract';
export * from '@stellar/stellar-sdk'
export * as contract from '@stellar/stellar-sdk/contract'
export * as rpc from '@stellar/stellar-sdk/rpc'

if (typeof window !== 'undefined') {
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
export type DataKey = {tag: "Balance", values: readonly [string]} | {tag: "Admin", values: void} | {tag: "Oracle", values: void} | {tag: "SchemaVersion", values: void} | {tag: "ActiveRound", values: void} | {tag: "Positions", values: void} | {tag: "UpDownPositions", values: void} | {tag: "PrecisionPositions", values: void} | {tag: "PendingWinnings", values: readonly [string]} | {tag: "UserStats", values: readonly [string]} | {tag: "Paused", values: void} | {tag: "BetWindowLedgers", values: void} | {tag: "RunWindowLedgers", values: void} | {tag: "LastRoundId", values: void} | {tag: "Position", values: readonly [u64, string]} | {tag: "PrecisionPosition", values: readonly [u64, string]} | {tag: "PrecisionCommitment", values: readonly [u64, string]} | {tag: "RoundParticipants", values: readonly [u64]} | {tag: "MaxStake", values: void} | {tag: "MaxUserRoundExposure", values: void} | {tag: "MaxPendingWinnings", values: void} | {tag: "CancelledRound", values: readonly [u64]} | {tag: "ConsumedOracleNonce", values: readonly [u64, u64]} | {tag: "MinParticipants", values: void} | {tag: "OracleHeartbeat", values: void} | {tag: "OracleStaleThreshold", values: void} | {tag: "OracleMaxDeviationBps", values: void} | {tag: "OracleDeviationOverrideArmed", values: void};

/**
 * Round mode for prediction type
 */
export enum RoundMode {
  UpDown = 0,
  Precision = 1,
}


export interface UserStats {
  best_streak: u32;
  current_streak: u32;
  total_losses: u32;
  total_wins: u32;
}


export interface UserPosition {
  amount: i128;
  side: BetSide;
}


export interface OraclePayload {
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

/**
 * Oracle liveness record, updated by the oracle service on each heartbeat call.
 * `status`: 0 = active, 1 = degraded, 2 = offline.
 */
export interface OracleHeartbeatRecord {
  timestamp: u64;
  status: u32;
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
   * Only admin can perform this action
   */
  4: {message:"UnauthorizedAdmin"},
  /**
   * Only oracle can perform this action
   */
  5: {message:"UnauthorizedOracle"},
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
   * Invalid price scale (must represent 4 decimal places)
   */
  17: {message:"InvalidPriceScale"},
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
   * Admin and Oracle addresses cannot be identical
   */
  21: {message:"AdminIsOracle"},
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
   * Round has been cancelled and cannot be resolved
   */
  26: {message:"RoundCancelled"},
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
   * Start price is below the minimum allowed value
   */
  31: {message:"StartPriceTooLow"},
  /**
   * Start price exceeds the maximum allowed value
   */
  32: {message:"StartPriceTooHigh"},
  /**
   * Oracle payload nonce was already consumed for this round (replay)
   */
  33: {message:"OracleNonceReused"},
  /**
   * Round has fewer participants than the configured minimum for competitive settlement
   */
  34: {message:"InsufficientParticipants"},
  /**
   * Minimum participants value is out of valid range (must be 1–10000)
   */
  35: {message:"InvalidMinParticipants"},
  /**
   * Oracle heartbeat status is out of range (must be 0, 1, or 2)
   */
  36: {message:"InvalidOracleStatus"},
  /**
   * Oracle stale threshold is out of valid range (must be 60–86400 seconds)
   */
  37: {message:"InvalidStaleThreshold"},
  /**
   * Oracle max deviation bps is invalid (must be > 0)
   */
  38: {message:"InvalidOracleDeviationBps"},
  /**
   * Oracle final price deviates beyond configured threshold
   */
  39: {message:"OracleDeviationExceeded"},
  /**
   * Stored schema version is unknown or unsupported by this contract build
   */
  40: {message:"UnsupportedSchemaVersion"},
  /**
   * Migration path is invalid for the stored schema version
   */
  41: {message:"InvalidMigrationPath"},
  /**
   * Migration cannot run while a round is active
   */
  42: {message:"MigrationActiveRound"},
  /**
   * Commitment for precision prediction not found
   */
  43: {message:"CommitmentNotFound"},
  /**
   * Precision prediction has already been revealed
   */
  44: {message:"AlreadyRevealed"},
  /**
   * Attempted to reveal prediction outside the valid window
   */
  45: {message:"InvalidRevealWindow"},
  /**
   * Revealed prediction hash does not match committed hash
   */
  46: {message:"HashMismatch"}
}

export interface Client {
  /**
   * Construct and simulate a balance transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns user's vXLM balance
   */
  balance: ({user}: {user: string}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_admin transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_admin: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Option<string>>>

  /**
   * Construct and simulate a is_paused transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns whether the contract is currently paused
   */
  is_paused: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<boolean>>

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
  place_bet: ({user, amount, side}: {user: string, amount: i128, side: BetSide}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_oracle transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_oracle: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Option<string>>>

  /**
   * Construct and simulate a initialize transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Initializes the contract with admin and oracle addresses (one-time only)
   */
  initialize: ({admin, oracle}: {admin: string, oracle: string}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_windows transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the betting and execution windows (admin only)
   * bet_ledgers: Number of ledgers users can place bets
   * run_ledgers: Total number of ledgers before round can be resolved
   */
  set_windows: ({bet_ledgers, run_ledgers}: {bet_ledgers: u32, run_ledgers: u32}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

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
  cancel_round: ({reason}: {reason: u32}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a create_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Creates a new prediction round (admin only)
   * mode: 0 = Up/Down (default), 1 = Precision (Legends)
   */
  create_round: ({start_price, mode}: {start_price: u128, mode: Option<u32>}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a mint_initial transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Mints 1000 vXLM for new users (one-time only)
   */
  mint_initial: ({user}: {user: string}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_max_stake transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current maximum stake cap, if set.
   */
  get_max_stake: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Option<i128>>>

  /**
   * Construct and simulate a predict_price transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Alias for place_precision_prediction - allows users to submit exact price predictions
   * guessed_price: price scaled to 4 decimals (e.g., 0.2297 → 2297)
   */
  predict_price: ({user, guessed_price, amount}: {user: string, guessed_price: u128, amount: i128}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a resolve_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Resolves the round with oracle payload (oracle only)
   * Mode 0 (Up/Down): Winners split losers' pool proportionally; ties get refunds
   * Mode 1 (Precision/Legends): Closest guess wins full pot; ties split evenly
   */
  resolve_round: ({payload}: {payload: OraclePayload}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_max_stake transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the maximum stake allowed per individual bet (admin only).
   * Pass `None` to disable the cap.
   */
  set_max_stake: ({max_amount}: {max_amount: Option<i128>}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a claim_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Claims pending winnings and adds to balance
   */
  claim_winnings: ({user}: {user: string}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<i128>>>

  /**
   * Construct and simulate a get_user_stats transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns user statistics (wins, losses, streaks)
   */
  get_user_stats: ({user}: {user: string}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<UserStats>>

  /**
   * Construct and simulate a pause_contract transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Pauses the contract for emergency recovery (admin only)
   */
  pause_contract: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_active_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the currently active round, if any
   */
  get_active_round: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Option<Round>>>

  /**
   * Construct and simulate a unpause_contract transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Unpauses the contract after recovery (admin only)
   */
  unpause_contract: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a commit_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Commits a hashed prediction and stake amount (Precision mode only)
   */
  commit_prediction: ({user, hash, amount}: {user: string, hash: Buffer, amount: i128}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_last_round_id transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the ID of the last created round (0 if no rounds created yet)
   */
  get_last_round_id: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a get_user_position transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns user's position in the current round (Up/Down mode).
   * 
   * Reads a single composite key `DataKey::Position(round_id, user)` — O(1).
   * Falls back to legacy `UpDownPositions` / `Positions` map blobs for
   * one-time migration compatibility.
   */
  get_user_position: ({user}: {user: string}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Option<UserPosition>>>

  /**
   * Construct and simulate a reveal_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Reveals a previously committed prediction (Precision mode only)
   */
  reveal_prediction: ({user, predicted_price, salt}: {user: string, predicted_price: u128, salt: Buffer}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a is_round_cancelled transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns true if the given round_id was cancelled.
   */
  is_round_cancelled: ({round_id}: {round_id: u64}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a get_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns user's claimable winnings
   */
  get_pending_winnings: ({user}: {user: string}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_updown_positions transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns all Up/Down positions for the current round.
   * 
   * Reads the participant list once, then fetches each position individually.
   */
  get_updown_positions: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Map<string, UserPosition>>>

  /**
   * Construct and simulate a get_max_user_exposure transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current per-user round exposure cap, if set.
   */
  get_max_user_exposure: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Option<i128>>>

  /**
   * Construct and simulate a set_max_user_exposure transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the maximum cumulative exposure a user may have per round (admin only).
   * Pass `None` to disable the cap.
   */
  set_max_user_exposure: ({max_exposure}: {max_exposure: Option<i128>}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_max_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current maximum pending winnings cap, if set.
   */
  get_max_pending_winnings: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Option<i128>>>

  /**
   * Construct and simulate a set_max_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the maximum pending winnings allowed per account (admin only).
   * Pass `None` to disable the cap.
   */
  set_max_pending_winnings: ({max_pending}: {max_pending: Option<i128>}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_precision_predictions transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns all precision predictions for the current round.
   * 
   * Reads the participant list once, then fetches each prediction individually.
   * Total reads: 1 (participant list) + N (predictions) instead of 1 large map blob.
   */
  get_precision_predictions: (options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Array<PrecisionPrediction>>>

  /**
   * Construct and simulate a place_precision_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Places a precision prediction on the active round (Precision/Legends mode only)
   * predicted_price: price scaled to 4 decimals (e.g., 0.2297 → 2297)
   * 
   * Per-user key `DataKey::PrecisionPosition(round_id, user)` gives O(1)
   * write cost independent of participant count.
   */
  place_precision_prediction: ({user, amount, predicted_price}: {user: string, amount: i128, predicted_price: u128}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_user_precision_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns user's precision prediction in the current round (Precision mode).
   * 
   * Reads a single composite key `DataKey::PrecisionPosition(round_id, user)` — O(1).
   * Falls back to legacy `PrecisionPositions` map for migration compatibility.
   */
  get_user_precision_prediction: ({user}: {user: string}, options?: {
    /**
     * The fee to pay for the transaction. Default: BASE_FEE
     */
    fee?: number;

    /**
     * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
     */
    timeoutInSeconds?: number;

    /**
     * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
     */
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Option<PrecisionPrediction>>>

  /**
   * Construct and simulate a get_schema_version transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the stored schema version. If unset, returns legacy version 1.
   */
  get_schema_version: (options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a migrate_schema_v1_to_v2 transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Migrates legacy schema version 1 → current schema version 2 (admin only).
   */
  migrate_schema_v1_to_v2: (options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_oracle_max_deviation_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the maximum oracle price deviation allowed at settlement (admin only).
   */
  set_oracle_max_deviation_bps: ({bps}: {bps: Option<u32>}, options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_oracle_max_deviation_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured oracle max deviation bps, if set.
   */
  get_oracle_max_deviation_bps: (options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Option<u32>>>

  /**
   * Construct and simulate an arm_oracle_deviation_override transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Arms a one-shot override to bypass deviation checks for the next settlement (admin only).
   */
  arm_oracle_deviation_override: (options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate an update_oracle_heartbeat transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Records an oracle heartbeat (oracle only).
   * `status`: 0 = active, 1 = degraded, 2 = offline.
   */
  update_oracle_heartbeat: ({status}: {status: u32}, options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_oracle_heartbeat transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the most recent oracle heartbeat record, if any.
   */
  get_oracle_heartbeat: (options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Option<OracleHeartbeatRecord>>>

  /**
   * Construct and simulate an is_oracle_live transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns `true` if the oracle has a non-stale heartbeat with status not offline (2).
   */
  is_oracle_live: (options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a set_oracle_stale_threshold transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the stale heartbeat threshold in seconds (admin only).
   * Allowed range: 60–86400 seconds (1 minute to 24 hours).
   */
  set_oracle_stale_threshold: ({seconds}: {seconds: u64}, options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_oracle_stale_threshold transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured oracle stale threshold, or the default (3600 s) if not set.
   */
  get_oracle_stale_threshold: (options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a set_min_participants transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the minimum participant count required for competitive settlement (admin only).
   */
  set_min_participants: ({min}: {min: Option<u32>}, options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_min_participants transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current minimum participant threshold, if set.
   */
  get_min_participants: (options?: {
    fee?: number;
    timeoutInSeconds?: number;
    simulate?: boolean;
  }) => Promise<AssembledTransaction<Option<u32>>>

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
        "AAAAAgAAAppTdG9yYWdlIGtleXMgZm9yIGNvbnRyYWN0IGRhdGEKCiMjIEluZGV4ZWQgcG9zaXRpb24ga2V5cyAodmFyaWFudHMgMTPigJMxNSkKCmBQb3NpdGlvbihyb3VuZF9pZCwgYWRkcmVzcylgIGFuZCBgUHJlY2lzaW9uUG9zaXRpb24ocm91bmRfaWQsIGFkZHJlc3MpYCBzdG9yZQphIHNpbmdsZSB1c2VyJ3MgcmVjb3JkIHVuZGVyIGEgY29tcG9zaXRlIGtleSwgZW5hYmxpbmcgTygxKSByZWFkL3dyaXRlIHBlciB1c2VyCmluc3RlYWQgb2YgZGVzZXJpYWxpemluZyB0aGUgZnVsbCBwYXJ0aWNpcGFudCBtYXAgb24gZXZlcnkgYmV0LgoKYFJvdW5kUGFydGljaXBhbnRzKHJvdW5kX2lkKWAgaG9sZHMgdGhlIG9yZGVyZWQgYFZlYzxBZGRyZXNzPmAgdXNlZCBmb3IKaXRlcmF0aW9uIGF0IHJlc29sdXRpb24gdGltZS4gQXBwZW5kaW5nIG9uZSBhZGRyZXNzIGlzIGNoZWFwZXIgdGhhbgpyZS1zZXJpYWxpc2luZyBhbiBOLWVudHJ5IGBNYXA8QWRkcmVzcywgVD5gIGZvciBldmVyeSBiZXQgcGxhY2VkLgoKTGVnYWN5IHNpbmdsZS1rZXkgbWFwcyAoYFVwRG93blBvc2l0aW9uc2AsIGBQcmVjaXNpb25Qb3NpdGlvbnNgKSBhcmUga2VwdCBmb3IKYmFja3dhcmQtY29tcGF0aWJsZSByZWFkcyBkdXJpbmcgYSBtaWdyYXRpb24gd2luZG93OyB0aGV5IGFyZSBubyBsb25nZXIgd3JpdHRlbi4AAAAAAAAAAAAHRGF0YUtleQAAAAAWAAAAAQAAAAAAAAAHQmFsYW5jZQAAAAABAAAAEwAAAAAAAAAAAAAABUFkbWluAAAAAAAAAAAAAAAAAAAGT3JhY2xlAAAAAAAAAAAAAAAAAAtBY3RpdmVSb3VuZAAAAAAAAAAAAAAAAAlQb3NpdGlvbnMAAAAAAAAAAAAAAAAAAA9VcERvd25Qb3NpdGlvbnMAAAAAAAAAAAAAAAASUHJlY2lzaW9uUG9zaXRpb25zAAAAAAABAAAAAAAAAA9QZW5kaW5nV2lubmluZ3MAAAAAAQAAABMAAAABAAAAAAAAAAlVc2VyU3RhdHMAAAAAAAABAAAAEwAAAAAAAAAAAAAABlBhdXNlZAAAAAAAAAAAAAAAAAAQQmV0V2luZG93TGVkZ2VycwAAAAAAAAAAAAAAEFJ1bldpbmRvd0xlZGdlcnMAAAAAAAAAAAAAAAtMYXN0Um91bmRJZAAAAAABAAAAPlBlci11c2VyIFVwRG93biBwb3NpdGlvbjogKHJvdW5kX2lkLCBhZGRyZXNzKSDihpIgVXNlclBvc2l0aW9uAAAAAAAIUG9zaXRpb24AAAACAAAABgAAABMAAAABAAAASlBlci11c2VyIFByZWNpc2lvbiBwcmVkaWN0aW9uOiAocm91bmRfaWQsIGFkZHJlc3MpIOKGkiBQcmVjaXNpb25QcmVkaWN0aW9uAAAAAAARUHJlY2lzaW9uUG9zaXRpb24AAAAAAAACAAAABgAAABMAAAABAAAASlBlci11c2VyIFByZWNpc2lvbiBjb21taXRtZW50OiAocm91bmRfaWQsIGFkZHJlc3MpIOKGkiBQcmVjaXNpb25Db21taXRtZW50AAAAAAATUHJlY2lzaW9uQ29tbWl0bWVudAAAAAACAAAABgAAABMAAAABAAAAP09yZGVyZWQgcGFydGljaXBhbnQgbGlzdCBmb3IgYSByb3VuZDogcm91bmRfaWQg4oaSIFZlYzxBZGRyZXNzPgAAAAARUm91bmRQYXJ0aWNpcGFudHMAAAAAAAABAAAABgAAAAAAAAA7TWF4aW11bSBzdGFrZSBhbGxvd2VkIHBlciBpbmRpdmlkdWFsIGJldCAoTm9uZSA9IHVubGltaXRlZCkAAAAACE1heFN0YWtlAAAAAAAAAEFNYXhpbXVtIGN1bXVsYXRpdmUgZXhwb3N1cmUgcGVyIHVzZXIgcGVyIHJvdW5kIChOb25lID0gdW5saW1pdGVkKQAAAAAAABRNYXhVc2VyUm91bmRFeHBvc3VyZQAAAAAAAAA/TWF4aW11bSBwZW5kaW5nIHdpbm5pbmdzIGFsbG93ZWQgcGVyIGFjY291bnQgKE5vbmUgPSB1bmxpbWl0ZWQpAAAAABJNYXhQZW5kaW5nV2lubmluZ3MAAAAAAAEAAAAvTWFya2VyIGZvciBhIGNhbmNlbGxlZCByb3VuZDogcm91bmRfaWQg4oaSIHRydWUAAAAADkNhbmNlbGxlZFJvdW5kAAAAAAABAAAABgAAAAEAAACEUGVyLXJvdW5kIGNvbnN1bWVkIG9yYWNsZSBub25jZTogKHJvdW5kX2lkLCBub25jZSkg4oaSIHRydWUuClVzZWQgdG8gcmVqZWN0IGR1cGxpY2F0ZSBvcmFjbGUgcGF5bG9hZCBzdWJtaXNzaW9ucyBmb3IgdGhlIHNhbWUgcm91bmQuAAAAE0NvbnN1bWVkT3JhY2xlTm9uY2UAAAAAAgAAAAYAAAAG",
        "AAAAAwAAAB5Sb3VuZCBtb2RlIGZvciBwcmVkaWN0aW9uIHR5cGUAAAAAAAAAAAAJUm91bmRNb2RlAAAAAAAAAgAAAAAAAAAGVXBEb3duAAAAAAAAAAAAAAAAAAlQcmVjaXNpb24AAAAAAAAB",
        "AAAAAQAAAAAAAAAAAAAACVVzZXJTdGF0cwAAAAAAAAQAAAAAAAAAC2Jlc3Rfc3RyZWFrAAAAAAQAAAAAAAAADmN1cnJlbnRfc3RyZWFrAAAAAAAEAAAAAAAAAAx0b3RhbF9sb3NzZXMAAAAEAAAAAAAAAAp0b3RhbF93aW5zAAAAAAAE",
        "AAAAAQAAAAAAAAAAAAAADFVzZXJQb3NpdGlvbgAAAAIAAAAAAAAABmFtb3VudAAAAAAACwAAAAAAAAAEc2lkZQAAB9AAAAAHQmV0U2lkZQA=",
        "AAAAAQAAAAAAAAAAAAAADU9yYWNsZVBheWxvYWQAAAAAAAAEAAABZFBlci1yb3VuZCByZXBsYXktcHJvdGVjdGlvbiBub25jZS4KClRoZSBvcmFjbGUgc2VydmljZSBtdXN0IGdlbmVyYXRlIGEgdW5pcXVlIHZhbHVlIHBlciBzdWJtaXNzaW9uIGZvciBhCmdpdmVuIHJvdW5kIChlLmcuIGEgbW9ub3RvbmljIGNvdW50ZXIgb3IgcmFuZG9tIDY0LWJpdCB2YWx1ZSkuIFRoZQpjb250cmFjdCByZWNvcmRzIGVhY2ggY29uc3VtZWQgbm9uY2UgdW5kZXIKYERhdGFLZXk6OkNvbnN1bWVkT3JhY2xlTm9uY2Uocm91bmRfaWQsIG5vbmNlKWAgYW5kIHJlamVjdHMgYW55IHJldXNlLAptYWtpbmcgcmVzb2x1dGlvbiBpZGVtcG90ZW50IGFnYWluc3QgYWNjaWRlbnRhbCBkdXBsaWNhdGUgc3VibWlzc2lvbnMuAAAABW5vbmNlAAAAAAAABgAAAAAAAAAFcHJpY2UAAAAAAAAKAAAAN1JvdW5kIGlkZW50aWZpZXIgdGhhdCBzaG91bGQgbWF0Y2ggYFJvdW5kLnN0YXJ0X2xlZGdlcmAAAAAACHJvdW5kX2lkAAAABAAAAAAAAAAJdGltZXN0YW1wAAAAAAAABg==",
        "AAAAAQAAAAAAAAAAAAAAE1ByZWNpc2lvbkNvbW1pdG1lbnQAAAAAAwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAAAAAARoYXNoAAAD7gAAACAAAAAAAAAACHJldmVhbGVkAAAAAQ==",
        "AAAAAQAAADtQcmVjaXNpb24gcHJlZGljdGlvbiBlbnRyeSAodXNlciBhZGRyZXNzICsgcHJlZGljdGVkIHByaWNlKQAAAAAAAAAAE1ByZWNpc2lvblByZWRpY3Rpb24AAAAAAwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAAAAAA9wcmVkaWN0ZWRfcHJpY2UAAAAACgAAAAAAAAAEdXNlcgAAABM=",
        "AAAABAAAABRDb250cmFjdCBlcnJvciB0eXBlcwAAAAAAAAANQ29udHJhY3RFcnJvcgAAAAAAACMAAAAlQ29udHJhY3QgaGFzIGFscmVhZHkgYmVlbiBpbml0aWFsaXplZAAAAAAAABJBbHJlYWR5SW5pdGlhbGl6ZWQAAAAAAAEAAAAtQWRtaW4gYWRkcmVzcyBub3Qgc2V0IC0gY2FsbCBpbml0aWFsaXplIGZpcnN0AAAAAAAAC0FkbWluTm90U2V0AAAAAAIAAAAuT3JhY2xlIGFkZHJlc3Mgbm90IHNldCAtIGNhbGwgaW5pdGlhbGl6ZSBmaXJzdAAAAAAADE9yYWNsZU5vdFNldAAAAAMAAAAiT25seSBhZG1pbiBjYW4gcGVyZm9ybSB0aGlzIGFjdGlvbgAAAAAAEVVuYXV0aG9yaXplZEFkbWluAAAAAAAABAAAACNPbmx5IG9yYWNsZSBjYW4gcGVyZm9ybSB0aGlzIGFjdGlvbgAAAAASVW5hdXRob3JpemVkT3JhY2xlAAAAAAAFAAAAJEJldCBhbW91bnQgbXVzdCBiZSBncmVhdGVyIHRoYW4gemVybwAAABBJbnZhbGlkQmV0QW1vdW50AAAABgAAABZObyBhY3RpdmUgcm91bmQgZXhpc3RzAAAAAAANTm9BY3RpdmVSb3VuZAAAAAAAAAcAAAAXUm91bmQgaGFzIGFscmVhZHkgZW5kZWQAAAAAClJvdW5kRW5kZWQAAAAAAAgAAAAdVXNlciBoYXMgaW5zdWZmaWNpZW50IGJhbGFuY2UAAAAAAAATSW5zdWZmaWNpZW50QmFsYW5jZQAAAAAJAAAAK1VzZXIgaGFzIGFscmVhZHkgcGxhY2VkIGEgYmV0IGluIHRoaXMgcm91bmQAAAAACkFscmVhZHlCZXQAAAAAAAoAAAAcQXJpdGhtZXRpYyBvdmVyZmxvdyBvY2N1cnJlZAAAAAhPdmVyZmxvdwAAAAsAAAATSW52YWxpZCBwcmljZSB2YWx1ZQAAAAAMSW52YWxpZFByaWNlAAAADAAAABZJbnZhbGlkIGR1cmF0aW9uIHZhbHVlAAAAAAAPSW52YWxpZER1cmF0aW9uAAAAAA0AAAAjSW52YWxpZCByb3VuZCBtb2RlIChtdXN0IGJlIDAgb3IgMSkAAAAAC0ludmFsaWRNb2RlAAAAAA4AAAAsV3JvbmcgcHJlZGljdGlvbiB0eXBlIGZvciBjdXJyZW50IHJvdW5kIG1vZGUAAAAWV3JvbmdNb2RlRm9yUHJlZGljdGlvbgAAAAAADwAAACRSb3VuZCBoYXMgbm90IHJlYWNoZWQgZW5kX2xlZGdlciB5ZXQAAAANUm91bmROb3RFbmRlZAAAAAAAABAAAAA1SW52YWxpZCBwcmljZSBzY2FsZSAobXVzdCByZXByZXNlbnQgNCBkZWNpbWFsIHBsYWNlcykAAAAAAAARSW52YWxpZFByaWNlU2NhbGUAAAAAAAARAAAAHk9yYWNsZSBkYXRhIGlzIHRvbyBvbGQgKFNUQUxFKQAAAAAAD1N0YWxlT3JhY2xlRGF0YQAAAAASAAAAMU9yYWNsZSBwYXlsb2FkIHJvdW5kX2lkIGRvZXNuJ3QgbWF0Y2ggQWN0aXZlUm91bmQAAAAAAAASSW52YWxpZE9yYWNsZVJvdW5kAAAAAAATAAAAOEFuIGFjdGl2ZSByb3VuZCBhbHJlYWR5IGV4aXN0cyBhbmQgY2Fubm90IGJlIG92ZXJ3cml0dGVuAAAAElJvdW5kQWxyZWFkeUFjdGl2ZQAAAAAAFAAAAC5BZG1pbiBhbmQgT3JhY2xlIGFkZHJlc3NlcyBjYW5ub3QgYmUgaWRlbnRpY2FsAAAAAAANQWRtaW5Jc09yYWNsZQAAAAAAABUAAAApQ29udHJhY3QgaXMgcGF1c2VkIGZvciBlbWVyZ2VuY3kgcmVjb3ZlcnkAAAAAAAAOQ29udHJhY3RQYXVzZWQAAAAAABYAAAA6T25lIG9yIG1vcmUgd2luZG93IHZhbHVlcyBleGNlZWQgY29uZmlndXJlZCBtYXhpbXVtIGJvdW5kcwAAAAAAEFdpbmRvd091dE9mUmFuZ2UAAAAXAAAAKU9yYWNsZSBwYXlsb2FkIHRpbWVzdGFtcCBpcyBpbiB0aGUgZnV0dXJlAAAAAAAAEEZ1dHVyZU9yYWNsZURhdGEAAAAYAAAAPUFyaXRobWV0aWMgb3ZlcmZsb3cgaW4gcGF5b3V0IGFjY3VtdWxhdGlvbiDigJQgbm8gZnVuZHMgbW92ZWQAAAAAAAAOUGF5b3V0T3ZlcmZsb3cAAAAAABkAAAAvUm91bmQgaGFzIGJlZW4gY2FuY2VsbGVkIGFuZCBjYW5ub3QgYmUgcmVzb2x2ZWQAAAAADlJvdW5kQ2FuY2VsbGVkAAAAAAAaAAAAP1JvdW5kIGNhbm5vdCBiZSBjYW5jZWxsZWQgKG5vIGFjdGl2ZSByb3VuZCBvciBhbHJlYWR5IHJlc29sdmVkKQAAAAATUm91bmROb3RDYW5jZWxsYWJsZQAAAAAbAAAAL0JldCBhbW91bnQgZXhjZWVkcyB0aGUgY29uZmlndXJlZCBtYXhpbXVtIHN0YWtlAAAAAA9TdGFrZUV4Y2VlZHNNYXgAAAAAHAAAAENVc2VyJ3MgY3VtdWxhdGl2ZSBleHBvc3VyZSBpbiB0aGlzIHJvdW5kIGV4Y2VlZHMgdGhlIGNvbmZpZ3VyZWQgY2FwAAAAABNFeHBvc3VyZUNhcEV4Y2VlZGVkAAAAAB0AAAA9UGVuZGluZyB3aW5uaW5ncyBhY2N1bXVsYXRpb24gd291bGQgZXhjZWVkIHRoZSBjb25maWd1cmVkIGNhcAAAAAAAABpQZW5kaW5nV2lubmluZ3NDYXBFeGNlZWRlZAAAAAAAHgAAAEFPcmFjbGUgcGF5bG9hZCBub25jZSB3YXMgYWxyZWFkeSBjb25zdW1lZCBmb3IgdGhpcyByb3VuZCAocmVwbGF5KQAAAAAAABFPcmFjbGVOb25jZVJldXNlZAAAAAAAAB8AAAAtQ29tbWl0bWVudCBmb3IgcHJlY2lzaW9uIHByZWRpY3Rpb24gbm90IGZvdW5kAAAAAAAAEkNvbW1pdG1lbnROb3RGb3VuZAAAAAAAIAAAAC5QcmVjaXNpb24gcHJlZGljdGlvbiBoYXMgYWxyZWFkeSBiZWVuIHJldmVhbGVkAAAAAAAPQWxyZWFkeVJldmVhbGVkAAAAACEAAAA3QXR0ZW1wdGVkIHRvIHJldmVhbCBwcmVkaWN0aW9uIG91dHNpZGUgdGhlIHZhbGlkIHdpbmRvdwAAAAATSW52YWxpZFJldmVhbFdpbmRvdwAAAAAiAAAANlJldmVhbGVkIHByZWRpY3Rpb24gaGFzaCBkb2VzIG5vdCBtYXRjaCBjb21taXR0ZWQgaGFzaAAAAAAADEhhc2hNaXNtYXRjaAAAACM=",
        "AAAAAAAAABtSZXR1cm5zIHVzZXIncyB2WExNIGJhbGFuY2UAAAAAB2JhbGFuY2UAAAAAAQAAAAAAAAAEdXNlcgAAABMAAAABAAAACw==",
        "AAAAAAAAAAAAAAAJZ2V0X2FkbWluAAAAAAAAAAAAAAEAAAPoAAAAEw==",
        "AAAAAAAAADBSZXR1cm5zIHdoZXRoZXIgdGhlIGNvbnRyYWN0IGlzIGN1cnJlbnRseSBwYXVzZWQAAAAJaXNfcGF1c2VkAAAAAAAAAAAAAAEAAAAB",
        "AAAAAAAAAW5QbGFjZXMgYSBiZXQgb24gdGhlIGFjdGl2ZSByb3VuZCAoVXAvRG93biBtb2RlIG9ubHkpLgoKU3RvcmFnZSBsYXlvdXQ6IGVhY2ggcGFydGljaXBhbnQncyBwb3NpdGlvbiBpcyBzdG9yZWQgdW5kZXIgaXRzIG93bgpjb21wb3NpdGUga2V5IGBEYXRhS2V5OjpQb3NpdGlvbihyb3VuZF9pZCwgdXNlcilgIOKAlCBPKDEpIHJlYWQvd3JpdGUKcmVnYXJkbGVzcyBvZiBob3cgbWFueSBvdGhlciBwYXJ0aWNpcGFudHMgZXhpc3QuIEFuIG9yZGVyZWQgcGFydGljaXBhbnQKbGlzdCBgRGF0YUtleTo6Um91bmRQYXJ0aWNpcGFudHMocm91bmRfaWQpYCBpcyBtYWludGFpbmVkIGZvciBPKG4pCml0ZXJhdGlvbiBhdCByZXNvbHV0aW9uIHRpbWUgb25seS4AAAAAAAlwbGFjZV9iZXQAAAAAAAADAAAAAAAAAAR1c2VyAAAAEwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAAAAAARzaWRlAAAH0AAAAAdCZXRTaWRlAAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAKZ2V0X29yYWNsZQAAAAAAAAAAAAEAAAPoAAAAEw==",
        "AAAAAAAAAEhJbml0aWFsaXplcyB0aGUgY29udHJhY3Qgd2l0aCBhZG1pbiBhbmQgb3JhY2xlIGFkZHJlc3NlcyAob25lLXRpbWUgb25seSkAAAAKaW5pdGlhbGl6ZQAAAAAAAgAAAAAAAAAFYWRtaW4AAAAAAAATAAAAAAAAAAZvcmFjbGUAAAAAABMAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAKlTZXRzIHRoZSBiZXR0aW5nIGFuZCBleGVjdXRpb24gd2luZG93cyAoYWRtaW4gb25seSkKYmV0X2xlZGdlcnM6IE51bWJlciBvZiBsZWRnZXJzIHVzZXJzIGNhbiBwbGFjZSBiZXRzCnJ1bl9sZWRnZXJzOiBUb3RhbCBudW1iZXIgb2YgbGVkZ2VycyBiZWZvcmUgcm91bmQgY2FuIGJlIHJlc29sdmVkAAAAAAAAC3NldF93aW5kb3dzAAAAAAIAAAAAAAAAC2JldF9sZWRnZXJzAAAAAAQAAAAAAAAAC3J1bl9sZWRnZXJzAAAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAXRDYW5jZWxzIHRoZSBhY3RpdmUgcm91bmQgYW5kIGRldGVybWluaXN0aWNhbGx5IHJlZnVuZHMgYWxsIHBhcnRpY2lwYW50IHN0YWtlcy4KCk9ubHkgYWRtaW4gbWF5IGNhbmNlbC4gSW50ZW5kZWQgZm9yIG9yYWNsZS11bmF2YWlsYWJsZSBvciBlbWVyZ2VuY3kgcmVjb3ZlcnkKc2NlbmFyaW9zLiBBZnRlciBjYW5jZWxsYXRpb246Ci0gQWxsIHBhcnRpY2lwYW50IHN0YWtlcyBhcmUgbW92ZWQgdG8gdGhlaXIgcGVuZGluZyB3aW5uaW5ncy4KLSBUaGUgYWN0aXZlIHJvdW5kIGlzIHJlbW92ZWQ7IG5vIGZ1dHVyZSBzZXR0bGVtZW50IGlzIHBvc3NpYmxlLgotIFRoZSByb3VuZCBJRCBpcyBtYXJrZWQgY2FuY2VsbGVkIHRvIHByZXZlbnQgYW55IHJlcGxheS4AAAAMY2FuY2VsX3JvdW5kAAAAAQAAAAAAAAAGcmVhc29uAAAAAAAEAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAGBDcmVhdGVzIGEgbmV3IHByZWRpY3Rpb24gcm91bmQgKGFkbWluIG9ubHkpCm1vZGU6IDAgPSBVcC9Eb3duIChkZWZhdWx0KSwgMSA9IFByZWNpc2lvbiAoTGVnZW5kcykAAAAMY3JlYXRlX3JvdW5kAAAAAgAAAAAAAAALc3RhcnRfcHJpY2UAAAAACgAAAAAAAAAEbW9kZQAAA+gAAAAEAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAC1NaW50cyAxMDAwIHZYTE0gZm9yIG5ldyB1c2VycyAob25lLXRpbWUgb25seSkAAAAAAAAMbWludF9pbml0aWFsAAAAAQAAAAAAAAAEdXNlcgAAABMAAAABAAAACw==",
        "AAAAAAAAAC5SZXR1cm5zIHRoZSBjdXJyZW50IG1heGltdW0gc3Rha2UgY2FwLCBpZiBzZXQuAAAAAAANZ2V0X21heF9zdGFrZQAAAAAAAAAAAAABAAAD6AAAAAs=",
        "AAAAAAAAAJdBbGlhcyBmb3IgcGxhY2VfcHJlY2lzaW9uX3ByZWRpY3Rpb24gLSBhbGxvd3MgdXNlcnMgdG8gc3VibWl0IGV4YWN0IHByaWNlIHByZWRpY3Rpb25zCmd1ZXNzZWRfcHJpY2U6IHByaWNlIHNjYWxlZCB0byA0IGRlY2ltYWxzIChlLmcuLCAwLjIyOTcg4oaSIDIyOTcpAAAAAA1wcmVkaWN0X3ByaWNlAAAAAAAAAwAAAAAAAAAEdXNlcgAAABMAAAAAAAAADWd1ZXNzZWRfcHJpY2UAAAAAAAAKAAAAAAAAAAZhbW91bnQAAAAAAAsAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAM1SZXNvbHZlcyB0aGUgcm91bmQgd2l0aCBvcmFjbGUgcGF5bG9hZCAob3JhY2xlIG9ubHkpCk1vZGUgMCAoVXAvRG93bik6IFdpbm5lcnMgc3BsaXQgbG9zZXJzJyBwb29sIHByb3BvcnRpb25hbGx5OyB0aWVzIGdldCByZWZ1bmRzCk1vZGUgMSAoUHJlY2lzaW9uL0xlZ2VuZHMpOiBDbG9zZXN0IGd1ZXNzIHdpbnMgZnVsbCBwb3Q7IHRpZXMgc3BsaXQgZXZlbmx5AAAAAAAADXJlc29sdmVfcm91bmQAAAAAAAABAAAAAAAAAAdwYXlsb2FkAAAAB9AAAAANT3JhY2xlUGF5bG9hZAAAAAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAF9TZXRzIHRoZSBtYXhpbXVtIHN0YWtlIGFsbG93ZWQgcGVyIGluZGl2aWR1YWwgYmV0IChhZG1pbiBvbmx5KS4KUGFzcyBgTm9uZWAgdG8gZGlzYWJsZSB0aGUgY2FwLgAAAAANc2V0X21heF9zdGFrZQAAAAAAAAEAAAAAAAAACm1heF9hbW91bnQAAAAAA+gAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAACtDbGFpbXMgcGVuZGluZyB3aW5uaW5ncyBhbmQgYWRkcyB0byBiYWxhbmNlAAAAAA5jbGFpbV93aW5uaW5ncwAAAAAAAQAAAAAAAAAEdXNlcgAAABMAAAABAAAD6QAAAAsAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAC9SZXR1cm5zIHVzZXIgc3RhdGlzdGljcyAod2lucywgbG9zc2VzLCBzdHJlYWtzKQAAAAAOZ2V0X3VzZXJfc3RhdHMAAAAAAAEAAAAAAAAABHVzZXIAAAATAAAAAQAAB9AAAAAJVXNlclN0YXRzAAAA",
        "AAAAAAAAADdQYXVzZXMgdGhlIGNvbnRyYWN0IGZvciBlbWVyZ2VuY3kgcmVjb3ZlcnkgKGFkbWluIG9ubHkpAAAAAA5wYXVzZV9jb250cmFjdAAAAAAAAAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAACpSZXR1cm5zIHRoZSBjdXJyZW50bHkgYWN0aXZlIHJvdW5kLCBpZiBhbnkAAAAAABBnZXRfYWN0aXZlX3JvdW5kAAAAAAAAAAEAAAPoAAAH0AAAAAVSb3VuZAAAAA==",
        "AAAAAAAAADFVbnBhdXNlcyB0aGUgY29udHJhY3QgYWZ0ZXIgcmVjb3ZlcnkgKGFkbWluIG9ubHkpAAAAAAAAEHVucGF1c2VfY29udHJhY3QAAAAAAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAEJDb21taXRzIGEgaGFzaGVkIHByZWRpY3Rpb24gYW5kIHN0YWtlIGFtb3VudCAoUHJlY2lzaW9uIG1vZGUgb25seSkAAAAAABFjb21taXRfcHJlZGljdGlvbgAAAAAAAAMAAAAAAAAABHVzZXIAAAATAAAAAAAAAARoYXNoAAAD7gAAACAAAAAAAAAABmFtb3VudAAAAAAACwAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAEVSZXR1cm5zIHRoZSBJRCBvZiB0aGUgbGFzdCBjcmVhdGVkIHJvdW5kICgwIGlmIG5vIHJvdW5kcyBjcmVhdGVkIHlldCkAAAAAAAARZ2V0X2xhc3Rfcm91bmRfaWQAAAAAAAAAAAAAAQAAAAY=",
        "AAAAAAAAAO1SZXR1cm5zIHVzZXIncyBwb3NpdGlvbiBpbiB0aGUgY3VycmVudCByb3VuZCAoVXAvRG93biBtb2RlKS4KClJlYWRzIGEgc2luZ2xlIGNvbXBvc2l0ZSBrZXkgYERhdGFLZXk6OlBvc2l0aW9uKHJvdW5kX2lkLCB1c2VyKWAg4oCUIE8oMSkuCkZhbGxzIGJhY2sgdG8gbGVnYWN5IGBVcERvd25Qb3NpdGlvbnNgIC8gYFBvc2l0aW9uc2AgbWFwIGJsb2JzIGZvcgpvbmUtdGltZSBtaWdyYXRpb24gY29tcGF0aWJpbGl0eS4AAAAAAAARZ2V0X3VzZXJfcG9zaXRpb24AAAAAAAABAAAAAAAAAAR1c2VyAAAAEwAAAAEAAAPoAAAH0AAAAAxVc2VyUG9zaXRpb24=",
        "AAAAAAAAAD9SZXZlYWxzIGEgcHJldmlvdXNseSBjb21taXR0ZWQgcHJlZGljdGlvbiAoUHJlY2lzaW9uIG1vZGUgb25seSkAAAAAEXJldmVhbF9wcmVkaWN0aW9uAAAAAAAAAwAAAAAAAAAEdXNlcgAAABMAAAAAAAAAD3ByZWRpY3RlZF9wcmljZQAAAAAKAAAAAAAAAARzYWx0AAAD7gAAACAAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAADFSZXR1cm5zIHRydWUgaWYgdGhlIGdpdmVuIHJvdW5kX2lkIHdhcyBjYW5jZWxsZWQuAAAAAAAAEmlzX3JvdW5kX2NhbmNlbGxlZAAAAAAAAQAAAAAAAAAIcm91bmRfaWQAAAAGAAAAAQAAAAE=",
        "AAAAAAAAACFSZXR1cm5zIHVzZXIncyBjbGFpbWFibGUgd2lubmluZ3MAAAAAAAAUZ2V0X3BlbmRpbmdfd2lubmluZ3MAAAABAAAAAAAAAAR1c2VyAAAAEwAAAAEAAAAL",
        "AAAAAAAAAH9SZXR1cm5zIGFsbCBVcC9Eb3duIHBvc2l0aW9ucyBmb3IgdGhlIGN1cnJlbnQgcm91bmQuCgpSZWFkcyB0aGUgcGFydGljaXBhbnQgbGlzdCBvbmNlLCB0aGVuIGZldGNoZXMgZWFjaCBwb3NpdGlvbiBpbmRpdmlkdWFsbHkuAAAAABRnZXRfdXBkb3duX3Bvc2l0aW9ucwAAAAAAAAABAAAD7AAAABMAAAfQAAAADFVzZXJQb3NpdGlvbg==",
        "AAAAAAAAADhSZXR1cm5zIHRoZSBjdXJyZW50IHBlci11c2VyIHJvdW5kIGV4cG9zdXJlIGNhcCwgaWYgc2V0LgAAABVnZXRfbWF4X3VzZXJfZXhwb3N1cmUAAAAAAAAAAAAAAQAAA+gAAAAL",
        "AAAAAAAAAGxTZXRzIHRoZSBtYXhpbXVtIGN1bXVsYXRpdmUgZXhwb3N1cmUgYSB1c2VyIG1heSBoYXZlIHBlciByb3VuZCAoYWRtaW4gb25seSkuClBhc3MgYE5vbmVgIHRvIGRpc2FibGUgdGhlIGNhcC4AAAAVc2V0X21heF91c2VyX2V4cG9zdXJlAAAAAAAAAQAAAAAAAAAMbWF4X2V4cG9zdXJlAAAD6AAAAAsAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAADlSZXR1cm5zIHRoZSBjdXJyZW50IG1heGltdW0gcGVuZGluZyB3aW5uaW5ncyBjYXAsIGlmIHNldC4AAAAAAAAYZ2V0X21heF9wZW5kaW5nX3dpbm5pbmdzAAAAAAAAAAEAAAPoAAAACw==",
        "AAAAAAAAAGNTZXRzIHRoZSBtYXhpbXVtIHBlbmRpbmcgd2lubmluZ3MgYWxsb3dlZCBwZXIgYWNjb3VudCAoYWRtaW4gb25seSkuClBhc3MgYE5vbmVgIHRvIGRpc2FibGUgdGhlIGNhcC4AAAAAGHNldF9tYXhfcGVuZGluZ193aW5uaW5ncwAAAAEAAAAAAAAAC21heF9wZW5kaW5nAAAAA+gAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAANZSZXR1cm5zIGFsbCBwcmVjaXNpb24gcHJlZGljdGlvbnMgZm9yIHRoZSBjdXJyZW50IHJvdW5kLgoKUmVhZHMgdGhlIHBhcnRpY2lwYW50IGxpc3Qgb25jZSwgdGhlbiBmZXRjaGVzIGVhY2ggcHJlZGljdGlvbiBpbmRpdmlkdWFsbHkuClRvdGFsIHJlYWRzOiAxIChwYXJ0aWNpcGFudCBsaXN0KSArIE4gKHByZWRpY3Rpb25zKSBpbnN0ZWFkIG9mIDEgbGFyZ2UgbWFwIGJsb2IuAAAAAAAZZ2V0X3ByZWNpc2lvbl9wcmVkaWN0aW9ucwAAAAAAAAAAAAABAAAD6gAAB9AAAAATUHJlY2lzaW9uUHJlZGljdGlvbgA=",
        "AAAAAAAAAQZQbGFjZXMgYSBwcmVjaXNpb24gcHJlZGljdGlvbiBvbiB0aGUgYWN0aXZlIHJvdW5kIChQcmVjaXNpb24vTGVnZW5kcyBtb2RlIG9ubHkpCnByZWRpY3RlZF9wcmljZTogcHJpY2Ugc2NhbGVkIHRvIDQgZGVjaW1hbHMgKGUuZy4sIDAuMjI5NyDihpIgMjI5NykKClBlci11c2VyIGtleSBgRGF0YUtleTo6UHJlY2lzaW9uUG9zaXRpb24ocm91bmRfaWQsIHVzZXIpYCBnaXZlcyBPKDEpCndyaXRlIGNvc3QgaW5kZXBlbmRlbnQgb2YgcGFydGljaXBhbnQgY291bnQuAAAAAAAacGxhY2VfcHJlY2lzaW9uX3ByZWRpY3Rpb24AAAAAAAMAAAAAAAAABHVzZXIAAAATAAAAAAAAAAZhbW91bnQAAAAAAAsAAAAAAAAAD3ByZWRpY3RlZF9wcmljZQAAAAAKAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAOpSZXR1cm5zIHVzZXIncyBwcmVjaXNpb24gcHJlZGljdGlvbiBpbiB0aGUgY3VycmVudCByb3VuZCAoUHJlY2lzaW9uIG1vZGUpLgoKUmVhZHMgYSBzaW5nbGUgY29tcG9zaXRlIGtleSBgRGF0YUtleTo6UHJlY2lzaW9uUG9zaXRpb24ocm91bmRfaWQsIHVzZXIpYCDigJQgTygxKS4KRmFsbHMgYmFjayB0byBsZWdhY3kgYFByZWNpc2lvblBvc2l0aW9uc2AgbWFwIGZvciBtaWdyYXRpb24gY29tcGF0aWJpbGl0eS4AAAAAAB1nZXRfdXNlcl9wcmVjaXNpb25fcHJlZGljdGlvbgAAAAAAAAEAAAAAAAAABHVzZXIAAAATAAAAAQAAA+gAAAfQAAAAE1ByZWNpc2lvblByZWRpY3Rpb24A" ]),
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
        pause_contract: this.txFromJSON<Result<void>>,
        get_active_round: this.txFromJSON<Option<Round>>,
        unpause_contract: this.txFromJSON<Result<void>>,
        commit_prediction: this.txFromJSON<Result<void>>,
        get_last_round_id: this.txFromJSON<u64>,
        get_user_position: this.txFromJSON<Option<UserPosition>>,
        reveal_prediction: this.txFromJSON<Result<void>>,
        is_round_cancelled: this.txFromJSON<boolean>,
        get_pending_winnings: this.txFromJSON<i128>,
        get_updown_positions: this.txFromJSON<Map<string, UserPosition>>,
        get_max_user_exposure: this.txFromJSON<Option<i128>>,
        set_max_user_exposure: this.txFromJSON<Result<void>>,
        get_max_pending_winnings: this.txFromJSON<Option<i128>>,
        set_max_pending_winnings: this.txFromJSON<Result<void>>,
        get_precision_predictions: this.txFromJSON<Array<PrecisionPrediction>>,
        place_precision_prediction: this.txFromJSON<Result<void>>,
        get_user_precision_prediction: this.txFromJSON<Option<PrecisionPrediction>>,
        get_schema_version: this.txFromJSON<u32>,
        migrate_schema_v1_to_v2: this.txFromJSON<Result<void>>,
        set_oracle_max_deviation_bps: this.txFromJSON<Result<void>>,
        get_oracle_max_deviation_bps: this.txFromJSON<Option<u32>>,
        arm_oracle_deviation_override: this.txFromJSON<Result<void>>,
        update_oracle_heartbeat: this.txFromJSON<Result<void>>,
        get_oracle_heartbeat: this.txFromJSON<Option<OracleHeartbeatRecord>>,
        is_oracle_live: this.txFromJSON<boolean>,
        set_oracle_stale_threshold: this.txFromJSON<Result<void>>,
        get_oracle_stale_threshold: this.txFromJSON<u64>,
        set_min_participants: this.txFromJSON<Result<void>>,
        get_min_participants: this.txFromJSON<Option<u32>>
  }
}