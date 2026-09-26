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





/**
 * Price report submitted by an oracle feeder
 */
export interface FeederReport {
  feeder: string;
  price: i128;
  timestamp: u64;
}


/**
 * Member registration record for an oracle feeder
 */
export interface CommitteeMember {
  active: boolean;
  feeder: string;
  registered_at: u64;
  stake: i128;
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
  start_timestamp: u64;
}

/**
 * Represents which side a user bet on
 */
export type BetSide = {tag: "Up", values: void} | {tag: "Down", values: void};

/**
 * Legacy monolithic storage key — retained for a few migration/read paths.
 */
export type DataKey = {tag: "Balance", values: readonly [string]} | {tag: "Admin", values: void} | {tag: "Oracle", values: void} | {tag: "SchemaVersion", values: void} | {tag: "ActiveRound", values: void} | {tag: "Positions", values: void} | {tag: "UpDownPositions", values: void} | {tag: "PrecisionPositions", values: void} | {tag: "PendingWinnings", values: readonly [string]} | {tag: "UserStats", values: readonly [string]} | {tag: "Paused", values: void} | {tag: "BetWindowLedgers", values: void} | {tag: "RunWindowLedgers", values: void} | {tag: "CloseBufferLedgers", values: void} | {tag: "LastRoundId", values: void} | {tag: "Position", values: readonly [u64, string]} | {tag: "PrecisionPosition", values: readonly [u64, string]} | {tag: "PrecisionCommitment", values: readonly [u64, string]} | {tag: "RoundParticipants", values: readonly [u64]} | {tag: "MaxStake", values: void} | {tag: "MaxUserRoundExposure", values: void} | {tag: "MaxPendingWinnings", values: void} | {tag: "CancelledRound", values: readonly [u64]} | {tag: "ConsumedOracleNonce", values: readonly [u64, u64]} | {tag: "MinParticipants", values: void} | {tag: "OracleHeartbeat", values: void} | {tag: "OracleStaleThreshold", values: void} | {tag: "MaxPrecisionParticipants", values: void} | {tag: "OracleMaxDeviationBps", values: void} | {tag: "OracleDeviationOverrideArmed", values: void} | {tag: "OracleMinConfidenceBps", values: void} | {tag: "OracleStrictMode", values: void} | {tag: "ArchivedRound", values: readonly [u64]} | {tag: "RecentArchivedRoundIds", values: void} | {tag: "UserRoundOutcome", values: readonly [u64, string]} | {tag: "MigratedToV3", values: void} | {tag: "PendingConfigChange", values: readonly [ConfigChangeKind]} | {tag: "ProtocolFeeBps", values: void} | {tag: "ProtocolFeeTreasury", values: void} | {tag: "LedgerMintCounter", values: readonly [u32]} | {tag: "MintLimitConfig", values: void} | {tag: "OracleRotationProposal", values: void} | {tag: "ArchiveRetention", values: void} | {tag: "RoundTemplate", values: void} | {tag: "Ext", values: readonly [DataKeyExt]};

/**
 * Fee incidence model (Issue #268).
 */
export enum FeeModel {
  FeeOnPot = 0,
  FeeOnWinnings = 1,
}


/**
 * Amendment proposal for parameter changes with timelock and veto window (Issue #363).
 * Represents a proposed change to a protocol parameter that must pass through a
 * governance lifecycle: optional veto window, timelock, then activation.
 */
export interface Amendment {
  activation_deadline_ledger: u32;
  created_at_ledger: u32;
  id: u64;
  new_value: any;
  parameter_name: string;
  proposer: string;
  status: AmendmentStatus;
  veto_deadline_ledger: u32;
}

/**
 * Protected administrative action types (Issue #272).
 */
export type GovAction = {tag: "PauseProtocol", values: void} | {tag: "UnpauseProtocol", values: void} | {tag: "SetProtocolFeeBps", values: readonly [Option<u32>]} | {tag: "WithdrawProtocolFee", values: readonly [string, i128]} | {tag: "SetTreasuryAddress", values: readonly [string]} | {tag: "SetAdmin", values: readonly [string]} | {tag: "SetOracle", values: readonly [string]} | {tag: "WithdrawInsuranceFund", values: readonly [string, i128]} | {tag: "SetInsuranceSplitBps", values: readonly [u32]} | {tag: "SetInsuranceCoverageBps", values: readonly [u32]};

export type HbGateKey = {tag: "Config", values: void};

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

export type DataKeyExt = {tag: "LeaderboardWins", values: void} | {tag: "LeaderboardStreak", values: void} | {tag: "SeasonId", values: void} | {tag: "SeasonUserStats", values: readonly [u32, string]} | {tag: "SeasonLeaderboardWins", values: void} | {tag: "SeasonLeaderboardStreak", values: void} | {tag: "SeasonArchive", values: readonly [u32]} | {tag: "ConstitutionMetadata", values: void} | {tag: "Amendment", values: readonly [u64]} | {tag: "NextAmendmentId", values: void};

/**
 * Lifecycle phase of an active round, derived from ledger windows.
 * 
 * Semantics (given `start_ledger`, `bet_end_ledger`, `end_ledger`):
 * - `Betting`: `ledger < bet_end_ledger` — bets and precision predictions accepted
 * - `Running`: `bet_end_ledger ≤ ledger < end_ledger` — reveal window (precision)
 * - `Resolvable`: `ledger ≥ end_ledger` — round may be settled via oracle payload
 */
export enum RoundPhase {
  Betting = 1,
  Running = 2,
  Resolvable = 3,
}

/**
 * Participant access-control state (Issue #274).
 */
export enum AccessState {
  Open = 0,
  Allowlisted = 1,
  Denylisted = 2,
}

/**
 * Parameterless system, config, and metadata storage keys.
 * 
 * Split from `DataKey` to stay under the XDR union 50-case limit
 * (`VecM<ScSpecUdtUnionCaseV0, 50>` in stellar-xdr).
 */
export type DataKeyCore = {tag: "Admin", values: void} | {tag: "Oracle", values: void} | {tag: "SchemaVersion", values: void} | {tag: "ActiveRound", values: void} | {tag: "Positions", values: void} | {tag: "UpDownPositions", values: void} | {tag: "PrecisionPositions", values: void} | {tag: "Paused", values: void} | {tag: "BetWindowLedgers", values: void} | {tag: "RunWindowLedgers", values: void} | {tag: "CloseBufferLedgers", values: void} | {tag: "LastRoundId", values: void} | {tag: "MaxStake", values: void} | {tag: "MaxUserRoundExposure", values: void} | {tag: "MaxPendingWinnings", values: void} | {tag: "MinParticipants", values: void} | {tag: "OracleHeartbeat", values: void} | {tag: "OracleStaleThreshold", values: void} | {tag: "MaxPrecisionParticipants", values: void} | {tag: "OracleMaxDeviationBps", values: void} | {tag: "OracleDeviationOverrideArmed", values: void} | {tag: "OracleMinConfidenceBps", values: void} | {tag: "OracleStrictMode", values: void} | {tag: "RecentArchivedRoundIds", values: void} | {tag: "MigratedToV3", values: void} | {tag: "ProtocolFeeBps", values: void} | {tag: "ProtocolFeeTreasury", values: void} | {tag: "MintLimitConfig", values: void} | {tag: "OracleRotationProposal", values: void} | {tag: "ArchiveRetention", values: void} | {tag: "RoundTemplate", values: void} | {tag: "OracleQuorum", values: void} | {tag: "NextSchemaVersion", values: void} | {tag: "MinBet", values: void} | {tag: "EpochMintBudget", values: void} | {tag: "EarlyCashoutBps", values: void} | {tag: "FeeModel", values: void} | {tag: "DisputeLedgers", values: void} | {tag: "PrecisionPayoutPolicy", values: void} | {tag: "AccessControlEnabled", values: void} | {tag: "GovApprover", values: void} | {tag: "GovProposalTtlLedgers", values: void} | {tag: "NextGovProposalId", values: void} | {tag: "Ext", values: readonly [DataKeyExt]};


/**
 * Stored governance proposal (Issue #272).
 */
export interface GovProposal {
  action: GovAction;
  approver: Option<string>;
  created_at_ledger: u32;
  expires_at_ledger: u32;
  id: u64;
  proposer: string;
  status: GovProposalStatus;
}


export interface PriceSample {
  price: u128;
  timestamp: u64;
}

/**
 * Status of a specific round, returned by `get_round_status(round_id)`.
 * 
 * Queries a round by its monotonic `round_id`. Covers all lifecycle
 * stages from creation through terminal settlement.
 * 
 * ## Status codes
 * 
 * | value | variant          | description                                                                      |
 * |-------|------------------|-----------------------------------------------------------------------------------|
 * | 0     | `Unknown`        | Round does not exist or has been pruned from the on-chain archive.               |
 * | 1     | `Betting`        | Round is active; bets and predictions accepted (`ledger < bet_end_ledger`).      |
 * | 2     | `Running`        | Betting closed; reveal window open (`bet_end_ledger ≤ ledger < end_ledger`).    |
 * | 3     | `AwaitingResolve`| Round ended; awaiting oracle settlement (`ledger ≥ end_ledger`).                |
 * | 4     | `Resolved`       | Oracle settled the round; pot distributed to winners.                            |
 * | 5     | `Cancelled`      | Adm
 */
export enum RoundStatus {
  Unknown = 0,
  Betting = 1,
  Running = 2,
  AwaitingResolve = 3,
  Resolved = 4,
  Cancelled = 5,
  FallbackRefund = 6,
  Voided = 7,
}

/**
 * Runtime mode for the contract lifecycle
 */
export enum RuntimeMode {
  Normal = 0,
  ClaimsOnly = 1,
  FullyPaused = 2,
}


export interface HbGateConfig {
  grace_seconds: u64;
  override_armed: boolean;
  strict_mode: boolean;
}

/**
 * Policy action class consumed by the central policy gate (Issue #261).
 */
export enum PolicyAction {
  RoundMutation = 0,
  Claim = 1,
  AdminConfig = 2,
  Settlement = 3,
}


export interface UserPosition {
  amount: i128;
  side: BetSide;
}

/**
 * Parameterised and round-scoped storage keys.
 * 
 * Split from `DataKey` to stay under the XDR union 50-case limit.
 * These variants carry per-user, per-round, or compound-key payloads.
 */
export type DataKeyScoped = {tag: "Balance", values: readonly [string]} | {tag: "PendingWinnings", values: readonly [string]} | {tag: "UserStats", values: readonly [string]} | {tag: "Position", values: readonly [u64, string]} | {tag: "PrecisionPosition", values: readonly [u64, string]} | {tag: "PrecisionCommitment", values: readonly [u64, string]} | {tag: "RoundParticipants", values: readonly [u64]} | {tag: "CancelledRound", values: readonly [u64]} | {tag: "ConsumedOracleNonce", values: readonly [u64, u64]} | {tag: "UserRoundOutcome", values: readonly [u64, string]} | {tag: "PendingConfigChange", values: readonly [ConfigChangeKind]} | {tag: "LedgerMintCounter", values: readonly [u32]} | {tag: "ArchivedRound", values: readonly [u64]} | {tag: "SeasonUserStats", values: readonly [u32, string]} | {tag: "SeasonArchive", values: readonly [u32]} | {tag: "UserArchivedRoundIds", values: readonly [string]} | {tag: "Allowlisted", values: readonly [string]} | {tag: "Denylisted", values: readonly [string]} | {tag: "GovProposal", values: readonly [u64]} | {tag: "RoundStartLedger", values: readonly [u32]};


export interface OraclePayload {
  attestation: Option<Buffer>;
  /**
 * Optional confidence score from the price feed (0–10000 bps, where 10000 = 100%).
 * When `None`, the payload is treated as a legacy submission.
 * When strict mode is enabled, `None` is rejected.
 */
confidence: Option<u32>;
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
 * `DataKeyScoped::ConsumedOracleNonce(round_id, nonce)` and rejects any reuse,
 * making resolution idempotent against accidental duplicate submissions.
 */
nonce: u64;
  price: u128;
  /**
 * Binds this payload to exactly one round.
 * 
 * Must equal the active round's **`Round.start_ledger`** — the ledger
 * sequence at which the round was created — NOT the monotonic
 * `Round.round_id`. The two identifiers are used in different places:
 * `start_ledger` binds the payload (and is covered by the attestation
 * signature), while `Round.round_id` namespaces consumed nonces under
 * `DataKeyScoped::ConsumedOracleNonce`.
 * 
 * `create_round` guarantees a ledger sequence backs at most one round
 * (`DataKeyScoped::RoundStartLedger` / `RoundStartLedgerReused`), so this
 * value identifies a single round unambiguously. See `PROTOCOL_SPEC.md`
 * invariant I10.
 */
round_id: u32;
  timestamp: u64;
}


/**
 * Admin-configured blueprint for `create_next_from_template`.
 * 
 * Mirrors the arguments accepted by `create_round` (`start_price`, `mode`)
 * so a keeper can spin up the next round after a settle/cancel without an
 * operator re-specifying parameters each time. Validated with the exact
 * same rules `create_round` applies at creation time.
 */
export interface RoundTemplate {
  mode: Option<u32>;
  start_price: u128;
}


/**
 * Frozen snapshot of a season's final bounded rankings, written by
 * `reset_leaderboard_season`. `participant_count` is the number of distinct
 * addresses that appeared in either bounded index at reset time (a lower
 * bound on total season participants beyond the tracked top
 * `LEADERBOARD_LIMIT`, mirroring the same bound the live indexes enforce).
 */
export interface SeasonArchive {
  ended_at_ledger: u32;
  participant_count: u32;
  season_id: u32;
  streak: Array<SeasonLeaderboardEntry>;
  wins: Array<SeasonLeaderboardEntry>;
}

/**
 * Eligible failure events for insurance coverage (Issue #367).
 * 
 * Each variant maps to a cancel-round reason code used by the
 * insurance coverage payout gate. Only events listed in the
 * admin-configured whitelist trigger coverage.
 */
export enum InsuranceEvent {
  OracleOutage = 0,
  OracleDeviation = 1,
  FallbackRefund = 2,
}


/**
 * One-read composite view of current market state for frontends: round
 * phase, pool composition, ledger timing buffers, and fee configuration —
 * replacing several separate calls that could otherwise observe
 * inconsistent state if the ledger advances between them (Issue #280).
 * 
 * # Empty-round semantics
 * 
 * When there is no active round, `phase` and `pool_stats` are both `None`.
 * The timing-buffer and fee fields are always populated regardless — they
 * reflect contract-wide configuration, not round state, so they have a
 * well-defined value whether or not a round is active.
 * 
 * # Consistency with individual getters
 * 
 * `phase` and `pool_stats` are the exact, unmodified results of
 * `get_round_phase`/`get_round_pool_stats` (never recomputed), and the
 * buffer/fee fields are read via the same public getters
 * (`get_bet_window_ledgers`, `get_run_window_ledgers`,
 * `get_close_buffer_ledgers`, `get_protocol_fee_bps`, `get_fee_model`) that
 * callers could otherwise call individually — so a snapshot can never
 * disagree with those getters.
 */
export interface MarketSnapshot {
  /**
 * Number of ledgers the betting window stays open after round creation.
 */
bet_window_ledgers: u32;
  /**
 * Extra ledgers appended after the betting window closes, before the
 * round transitions to `Running` (0 = disabled).
 */
close_buffer_ledgers: u32;
  /**
 * Configured fee incidence model (`FeeOnPot` or `FeeOnWinnings`).
 */
fee_model: FeeModel;
  /**
 * Current round's lifecycle phase, or empty if no round is active.
 * 
 * Modeled as a 0-or-1-element `Vec` rather than `Option<RoundPhase>`:
 * this soroban-sdk version's `#[contracttype]` derive does not generate
 * an XDR (`ScVal`) conversion for `Option<T>` wrapping a user-defined
 * type, only for `Vec<T>`.
 */
phase: Array<RoundPhase>;
  /**
 * Full pool-composition breakdown for the active round, or empty if no
 * round is active. See `phase` for why this is a `Vec` and not an
 * `Option`.
 */
pool_stats: Array<RoundPoolStats>;
  /**
 * Configured protocol fee in basis points, or `None` if fees are disabled.
 */
protocol_fee_bps: Option<u32>;
  /**
 * Number of ledgers after round creation before the round becomes
 * resolvable.
 */
run_window_ledgers: u32;
}

/**
 * One-sided (degenerate) market settlement policy (Issue #270 / #390).
 * When exactly one of pool_up/pool_down is empty, refund all stakes on the
 * populated side (default policy for one-sided UpDown pools).
 */
export enum OneSidedPolicy {
  Refund = 0,
  Void = 1,
  CarryForward = 2,
}

/**
 * Parameter classification for the on-chain constitution (Issue #363).
 * Immutable parameters cannot be changed; timelocked parameters require
 * a timelock before activation; dual-approval parameters require both
 * admin and approver sign-off.
 */
export enum ParameterClass {
  Immutable = 0,
  Timelocked = 1,
  DualApproval = 2,
  Normal = 3,
}

/**
 * Global status of the protocol, returned by `get_protocol_status`.
 * 
 * Designed for frontend state machines that need a single, stable code
 * instead of combining multiple boolean flags.
 * 
 * ## Status codes
 * 
 * | value | variant      | description                                                             |
 * |-------|--------------|-------------------------------------------------------------------------|
 * | 0     | `Active`     | `RuntimeMode::Normal` and a round is active; round mutations accepted.   |
 * | 1     | `Paused`     | `RuntimeMode::FullyPaused`; every mutation (including claims) blocked.   |
 * | 2     | `ClaimsOnly` | `RuntimeMode::ClaimsOnly`, or `Normal` with no active round; claims and settlement allowed. |
 * 
 * ## Transition rules
 * 
 * - `ClaimsOnly` → `Active` when `create_round()` succeeds in `Normal` mode.
 * - `Active` → `ClaimsOnly` when `resolve_round()` or `cancel_round()` completes,
 * or when `set_runtime_mode(1)` is called.
 * - Any state → `Paused` when `pause_contract()` / `set_runtime_mode(2)` is called.
 * - 
 */
export enum ProtocolStatus {
  Active = 0,
  Paused = 1,
  ClaimsOnly = 2,
}


/**
 * Aggregated active-round pool composition for frontend transparency.
 * 
 * Up/Down rounds populate the up/down pools, counts, and stake ratios.
 * Precision rounds populate the precision totals and participant counters while
 * leaving side-specific Up/Down fields at zero. Ratios are basis points of
 * the mode's total visible stake (10_000 = 100%).
 */
export interface RoundPoolStats {
  down_participant_count: u32;
  down_stake_ratio_bps: u32;
  mode: RoundMode;
  precision_commitment_count: u32;
  precision_participant_count: u32;
  precision_prediction_count: u32;
  precision_revealed_count: u32;
  precision_total_stake: i128;
  round_id: u64;
  total_down_stake: i128;
  total_up_stake: i128;
  up_participant_count: u32;
  up_stake_ratio_bps: u32;
}

export type TwapSamplesKey = {tag: "Samples", values: void};

/**
 * Amendment proposal lifecycle status for the constitution (Issue #363).
 */
export enum AmendmentStatus {
  Pending = 0,
  Vetoed = 1,
  ActivationReady = 2,
  Activated = 3,
  Expired = 4,
}


export interface DeviationConfig {
  reference_mode: DeviationReferenceMode;
  window_samples: u32;
}


/**
 * Settlement data stored during dispute-window resolve and consumed by
 * `finalize_round` or `void_round`.
 */
export interface RoundSettlement {
  fee_amount: i128;
  final_price: u128;
  mode: u32;
  participants: Array<ResolvedParticipant>;
  pool_down: i128;
  pool_up: i128;
  price_start: u128;
  round_id: u64;
}

/**
 * Terminal outcome persisted per user per archived round.
 * 
 * Allows `get_user_archived_participation` to answer profile/history
 * queries without replaying the full event stream.
 */
export enum UserOutcomeType {
  Win = 0,
  Loss = 1,
  Refund = 2,
  Cancel = 3,
  Void = 4,
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
  MinParticipants = 7,
  MaxPrecisionParticipants = 8,
  MintLimit = 9,
  ArchiveRetention = 10,
  CloseBufferLedgers = 11,
  OracleTimestampSkew = 12,
  EpochMintBudget = 13,
  PendingWinningsExpiry = 14,
  PrecisionPayoutPolicy = 15,
  MinBet = 16,
  DisputeLedgers = 17,
  FeeModel = 18,
  EarlyCashoutBps = 19,
}


/**
 * A single entry in the lifetime (all-time) leaderboard.
 */
export interface LeaderboardEntry {
  stats: UserStats;
  user: string;
}


/**
 * Multi-feed oracle resolution payload.
 */
export interface MultiFeedPayload {
  contract_addr: string;
  network_id: Buffer;
  nonce: u64;
  prices: Array<u128>;
  round_id: u32;
  sources: Array<u32>;
  timestamp: u64;
}


/**
 * Simulated payout result for a specific hypothetical final price.
 */
export interface SimulationResult {
  fee_amount: i128;
  fee_model: u32;
  mode: RoundMode;
  outcomes: Array<UserRoundOutcome>;
  pool_down: i128;
  pool_up: i128;
  precision_total_stake: i128;
}


export interface UserRoundOutcome {
  outcome: UserOutcomeType;
  payout: i128;
  predicted_price: u128;
  prediction_side: u32;
  round_mode: u32;
  stake: i128;
  user: string;
}


export interface AttestationConfig {
  key: Option<Buffer>;
}

/**
 * Governance proposal lifecycle status (Issue #272).
 */
export enum GovProposalStatus {
  Pending = 0,
  Approved = 1,
  Executed = 2,
  Cancelled = 3,
  Expired = 4,
}

export type DeviationConfigKey = {tag: "Config", values: void};


export interface OracleQuorumConfig {
  min_observations: u32;
  outlier_threshold_bps: u32;
  quorum_threshold: u32;
}

/**
 * Terminal outcome recorded when a round leaves the active state.
 */
export enum RoundArchiveStatus {
  Resolved = 0,
  Cancelled = 1,
  FallbackRefund = 2,
  Voided = 3,
}

/**
 * Payload for a scheduled critical config change.
 */
export type ConfigChangePayload = {tag: "Windows", values: readonly [u32, u32]} | {tag: "MaxStake", values: readonly [Option<i128>]} | {tag: "MaxUserRoundExposure", values: readonly [Option<i128>]} | {tag: "MaxPendingWinnings", values: readonly [Option<i128>]} | {tag: "OracleStaleThreshold", values: readonly [u64]} | {tag: "OracleMaxDeviationBps", values: readonly [Option<u32>]} | {tag: "ProtocolFeeBps", values: readonly [Option<u32>]} | {tag: "MinParticipants", values: readonly [Option<u32>]} | {tag: "MaxPrecisionParticipants", values: readonly [u32]} | {tag: "MintLimit", values: readonly [u32]} | {tag: "ArchiveRetention", values: readonly [u32]} | {tag: "CloseBufferLedgers", values: readonly [u32]} | {tag: "OracleTimestampSkew", values: readonly [u64]} | {tag: "EpochMintBudget", values: readonly [i128]} | {tag: "PendingWinningsExpiry", values: readonly [u32]} | {tag: "PrecisionPayoutPolicy", values: readonly [u32]} | {tag: "MinBet", values: readonly [Option<i128>]} | {tag: "DisputeLedgers", values: readonly [u32]} | {tag: "FeeModel", values: readonly [FeeModel]} | {tag: "EarlyCashoutBps", values: readonly [Option<u32>]};


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
 * Per-participant outcome stored during dispute-window settlement.
 */
export interface ResolvedParticipant {
  outcome: UserOutcomeType;
  payout: i128;
  user: string;
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

export type AttestationConfigKey = {tag: "Config", values: void};


/**
 * On-chain constitution defining parameter governance rules (Issue #363).
 * Classifies each protocol parameter and defines the amendment lifecycle
 * (veto window, timelock, dual approval requirements).
 */
export interface ConstitutionMetadata {
  /**
 * Whether dual-approval (admin + approver) is required for amendments
 */
dual_approval_required: boolean;
  /**
 * Ledger at which the constitution was established
 */
established_at_ledger: u32;
  /**
 * Timelock duration in ledgers before amendments can activate
 */
timelock_ledgers: u32;
  /**
 * Veto window duration in ledgers (0 = no veto window)
 */
veto_window_ledgers: u32;
}


/**
 * Composite protocol health status returned by `get_protocol_health`.
 * 
 * Designed for operators to poll a single endpoint instead of stitching
 * together multiple read-only calls.
 * 
 * ## Status code → alert severity mapping
 * 
 * | code | label           | severity | meaning                                   |
 * |------|-----------------|----------|-------------------------------------------|
 * | 0    | HEALTHY         | none     | All subsystems nominal                    |
 * | 1    | PAUSED          | critical | `RuntimeMode::FullyPaused`                |
 * | 2    | ORACLE_STALE    | warning  | Oracle heartbeat is stale or offline      |
 * | 3    | ROUND_STALE     | warning  | Round is past its end ledger but unresolved|
 * | 4    | NO_ACTIVE_ROUND | info     | No round currently active (idle protocol) |
 * | 5    | MULTIPLE_ISSUES | critical | Two or more issues detected simultaneously|
 * | 6    | CLAIMS_ONLY     | warning  | `RuntimeMode::ClaimsOnly`                 |
 * | 7    | ACCESS_RESTRICTED | info   | Allowlist mode on; otherwise 
 */
export interface ProtocolHealthStatus {
  /**
 * Current round phase (0=no_round, 1=betting, 2=running, 3=resolvable)
 */
active_round_phase: u32;
  /**
 * Whether a round is currently active
 */
has_active_round: boolean;
  /**
 * Ledger sequence at which this health snapshot was taken
 */
ledger_sequence: u32;
  /**
 * Ledger timestamp at which this health snapshot was taken
 */
ledger_timestamp: u64;
  /**
 * Whether the oracle heartbeat is non-stale and not offline
 */
oracle_live: boolean;
  /**
 * Raw oracle heartbeat status (0=active, 1=degraded, 2=offline, 3=unknown)
 */
oracle_status: u32;
  /**
 * `true` only in `RuntimeMode::FullyPaused` (same as `is_paused()`);
 * `ClaimsOnly` is reported via `status_code == 6`, not this flag.
 */
paused: boolean;
  /**
 * On-chain storage schema version
 */
schema_version: u32;
  /**
 * Composite status code (see mapping table above)
 */
status_code: u32;
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
 * Payout policy for Precision mode (on-chain config).
 */
export enum PrecisionPayoutPolicy {
  Equal = 0,
  StakeWeighted = 1,
}

export enum DeviationReferenceMode {
  StartPrice = 0,
  Twap = 1,
}


/**
 * Pending two-step oracle rotation proposal.
 * 
 * The admin proposes a new oracle address with a timestamp-based expiry window.
 * After `expires_at` (ledger timestamp) the proposal is stale and acceptance
 * is rejected until the admin submits a fresh proposal.
 */
export interface OracleRotationProposal {
  expires_at: u64;
  new_oracle: string;
  proposed_at: u64;
}


/**
 * A single entry in a season-scoped leaderboard, live or archived.
 */
export interface SeasonLeaderboardEntry {
  best_streak: u32;
  user: string;
  wins: u32;
}

export type PendingWinningsExpiryKey = readonly [void];

export type PendingWinningsUpdatedAtKey = readonly [string];

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
   */
  place_bet: ({user, amount, side}: {user: string, amount: i128, side: BetSide}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a claim_many transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Claims pending winnings for up to `MAX_CLAIM_BATCH_SIZE` users in one
   * call. All-or-nothing: any failure (batch too large, a duplicate
   * address, or a missing per-user auth) reverts every effect in this
   * call. See `settlement::claim_many` for full semantics.
   */
  claim_many: ({users}: {users: Array<string>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<Array<i128>>>>

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
   * Construct and simulate a void_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Anyone may call `void_round` during the dispute window to refund all
   * participants their full stakes (void-to-refund path).
   */
  void_round: ({round_id}: {round_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_min_bet transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured minimum bet, if enabled (Issue #269).
   */
  get_min_bet: (options?: MethodOptions) => Promise<AssembledTransaction<Option<i128>>>

  /**
   * Construct and simulate a set_min_bet transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked minimum-bet (dust protection) update (Issue #269).
   */
  set_min_bet: ({min_amount}: {min_amount: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_windows transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked windows update (alias for [`Self::schedule_windows`]).
   * bet_ledgers: Number of ledgers users can place bets
   * run_ledgers: Total number of ledgers before round can be resolved
   */
  set_windows: ({bet_ledgers, run_ledgers}: {bet_ledgers: u32, run_ledgers: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a cancel_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  cancel_round: ({reason}: {reason: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a create_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Creates a new prediction round (admin only)
   */
  create_round: ({start_price, mode}: {start_price: u128, mode: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a mint_initial transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Mints 1000 vXLM for new users (one-time only)
   */
  mint_initial: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_amendment transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Retrieves an amendment proposal record by ID.
   */
  get_amendment: ({amendment_id}: {amendment_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Option<Amendment>>>

  /**
   * Construct and simulate a get_fee_model transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured fee incidence model, defaulting to `FeeOnPot`.
   */
  get_fee_model: (options?: MethodOptions) => Promise<AssembledTransaction<FeeModel>>

  /**
   * Construct and simulate a get_max_stake transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_max_stake: (options?: MethodOptions) => Promise<AssembledTransaction<Option<i128>>>

  /**
   * Construct and simulate a is_denylisted transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  is_denylisted: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a predict_price transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  predict_price: ({user, guessed_price, amount}: {user: string, guessed_price: u128, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a resolve_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  resolve_round: ({payload}: {payload: OraclePayload}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_fee_model transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the fee incidence model (admin only).
   * 
   * `FeeOnPot` (0): fee is calculated on the total round pot (default).
   * `FeeOnWinnings` (1): fee is calculated only on net winnings / profit.
   */
  set_fee_model: ({model}: {model: FeeModel}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_max_stake transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_max_stake: ({max_amount}: {max_amount: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a add_denylisted transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  add_denylisted: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a cash_out_early transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Early cash-out during the Running phase for UpDown rounds.
   * 
   * Allows a bettor to exit their position early, forfeiting a percentage
   * of their stake to the protocol treasury. The forfeited amount is
   * determined by the `EarlyCashoutBps` config (set by admin).
   * 
   * # Errors
   * - `EarlyCashoutDisabled` — feature not enabled (no penalty bps configured)
   * - `EarlyCashoutPhaseInvalid` — not in Running phase
   * - `EarlyCashoutNotUpDown` — round is not UpDown mode
   * - `NoActiveRound` — no active round exists
   * - `PositionNotFound` — user has no position in the active round
   */
  cash_out_early: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a claim_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  claim_winnings: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<i128>>>

  /**
   * Construct and simulate a finalize_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Anyone may call `finalize_round` after the dispute window expires to
   * distribute winnings to winners (normal settlement outcome).
   */
  finalize_round: ({round_id}: {round_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_mint_limit transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_mint_limit: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_user_stats transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_user_stats: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<UserStats>>

  /**
   * Construct and simulate a is_allowlisted transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  is_allowlisted: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a is_oracle_live transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns `true` if the oracle has a non-stale heartbeat with status not offline (2).
   */
  is_oracle_live: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a pause_contract transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Pauses the contract for emergency recovery (admin only)
   */
  pause_contract: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_mint_limit transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_mint_limit: ({limit}: {limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a veto_amendment transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Vetoes a pending amendment before its veto window expires.
   */
  veto_amendment: ({vetoer, amendment_id}: {vetoer: string, amendment_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a add_allowlisted transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  add_allowlisted: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a arm_hb_override transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Arms a one-shot override to bypass the heartbeat health gate for the next settlement (admin only, Issue #264).
   */
  arm_hb_override: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a batch_touch_ttl transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Auth-gated batch TTL extension for allowlisted storage keys (admin only).
   * 
   * Accepts a vector of `DataKeyCore` variants. Each key is validated against the
   * TTL-touch allowlist. Keys that exist in storage have their TTL extended to
   * `TTL_BUMP_AMOUNT` (~30 days). Keys not in the allowlist cause the entire
   * call to fail with `UnsupportedDataKeyForTtlTouch`. Keys that are in the
   * allowlist but absent from storage are silently skipped.
   * 
   * Returns the number of keys whose TTL was actually extended.
   * 
   * Event: `("storage", "touch")` with `(touched, skipped)` counts.
   */
  batch_touch_ttl: ({keys}: {keys: Array<DataKeyCore>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<u32>>>

  /**
   * Construct and simulate a get_next_schema transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the announced next schema version, if any.
   */
  get_next_schema: (options?: MethodOptions) => Promise<AssembledTransaction<Option<u32>>>

  /**
   * Construct and simulate a get_round_phase transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_round_phase: (options?: MethodOptions) => Promise<AssembledTransaction<Result<RoundPhase>>>

  /**
   * Construct and simulate a simulate_payout transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Estimates payouts for the active round given a hypothetical final price.
   * Does not mutate storage. Returns SimulationResult.
   */
  simulate_payout: ({final_price}: {final_price: u128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<SimulationResult>>>

  /**
   * Construct and simulate a get_access_state transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_access_state: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<AccessState>>

  /**
   * Construct and simulate a get_active_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_active_round: (options?: MethodOptions) => Promise<AssembledTransaction<Option<Round>>>

  /**
   * Construct and simulate a get_constitution transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the on-chain constitution metadata, if established.
   */
  get_constitution: (options?: MethodOptions) => Promise<AssembledTransaction<Option<ConstitutionMetadata>>>

  /**
   * Construct and simulate a get_gov_approver transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured secondary governance approver address, if set.
   */
  get_gov_approver: (options?: MethodOptions) => Promise<AssembledTransaction<Option<string>>>

  /**
   * Construct and simulate a get_gov_proposal transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Queries details for a governance proposal.
   */
  get_gov_proposal: ({proposal_id}: {proposal_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Option<GovProposal>>>

  /**
   * Construct and simulate a get_round_status transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the status of a specific round identified by `round_id`.
   * 
   * Lookup strategy (in priority order):
   * 1. If the round is the **current active round**, derive status from
   * ledger position relative to `bet_end_ledger` / `end_ledger`.
   * 2. If the round appears in the **on-chain archive**, map its
   * [`RoundArchiveStatus`] to the corresponding terminal [`RoundStatus`].
   * 3. If a `CancelledRound` marker exists (archive may be pruned),
   * return `Cancelled`.
   * 4. Otherwise, return `Unknown`.
   * 
   * | return value          | meaning                                                       |
   * |-----------------------|---------------------------------------------------------------|
   * | `Unknown`        (0)  | Round not found; never created or pruned from archive.       |
   * | `Betting`        (1)  | Active; `ledger < bet_end_ledger`.                           |
   * | `Running`        (2)  | Active; `bet_end_ledger ≤ ledger < end_ledger`.              |
   * | `AwaitingResolve`(3)  | Active; `ledger ≥ end_ledger`, oracle not yet called.        |
   * | `R
   */
  get_round_status: ({round_id}: {round_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<RoundStatus>>

  /**
   * Construct and simulate a get_runtime_mode transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current runtime mode (0 = Normal, 1 = ClaimsOnly, 2 = FullyPaused)
   */
  get_runtime_mode: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_twap_samples transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the recorded TWAP price samples, most-recent last (Issue #266).
   */
  get_twap_samples: (options?: MethodOptions) => Promise<AssembledTransaction<Array<PriceSample>>>

  /**
   * Construct and simulate a schedule_min_bet transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  schedule_min_bet: ({min_amount}: {min_amount: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a schedule_windows transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  schedule_windows: ({bet_ledgers, run_ledgers}: {bet_ledgers: u32, run_ledgers: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_gov_approver transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Configures the secondary governance approver (admin only).
   */
  set_gov_approver: ({approver}: {approver: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_runtime_mode transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the runtime mode of the contract (admin only)
   */
  set_runtime_mode: ({mode}: {mode: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a unpause_contract transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Unpauses the contract after recovery (admin only)
   */
  unpause_contract: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a clear_next_schema transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Clears a previously announced next schema version (admin only).
   */
  clear_next_schema: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a commit_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  commit_prediction: ({user, hash, amount}: {user: string, hash: Buffer, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_access_policy transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_access_policy: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<readonly [boolean, AccessState]>>

  /**
   * Construct and simulate a get_last_round_id transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_last_round_id: (options?: MethodOptions) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a get_user_position transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_user_position: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Option<UserPosition>>>

  /**
   * Construct and simulate a is_action_allowed transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns whether `action` is currently permitted under the PolicyGate
   * for the contract's runtime mode (Issue #261). Read-only; does not
   * mutate state. See [`admin::_policy_gate`] for the full matrix.
   */
  is_action_allowed: ({action}: {action: PolicyAction}, options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a propose_amendment transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Proposes a parameter amendment with timelock and optional veto window.
   */
  propose_amendment: ({proposer, parameter_name, new_value}: {proposer: string, parameter_name: string, new_value: any}, options?: MethodOptions) => Promise<AssembledTransaction<Result<u64>>>

  /**
   * Construct and simulate a remove_denylisted transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  remove_denylisted: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a reveal_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  reveal_prediction: ({user, predicted_price, salt}: {user: string, predicted_price: u128, salt: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a activate_amendment transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Activates an amendment after timelock expires.
   */
  activate_amendment: ({activator, amendment_id}: {activator: string, amendment_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_archived_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_archived_round: ({round_id}: {round_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Option<ArchivedRoundSummary>>>

  /**
   * Construct and simulate a get_hb_strict_mode transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns whether oracle heartbeat strict mode is enabled (Issue #264).
   */
  get_hb_strict_mode: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a get_round_template transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured round template, if any.
   */
  get_round_template: (options?: MethodOptions) => Promise<AssembledTransaction<Option<RoundTemplate>>>

  /**
   * Construct and simulate a get_schema_version transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the stored schema version. If unset, returns legacy version 1.
   */
  get_schema_version: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_season_archive transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the frozen archive for a past season, if it has been reset.
   */
  get_season_archive: ({season_id}: {season_id: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Option<SeasonArchive>>>

  /**
   * Construct and simulate a is_round_cancelled transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  is_round_cancelled: ({round_id}: {round_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a propose_gov_action transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Proposes a protected administrative action (governance admin/approver only).
   */
  propose_gov_action: ({proposer, action, custom_ttl}: {proposer: string, action: GovAction, custom_ttl: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<u64>>>

  /**
   * Construct and simulate a remove_allowlisted transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  remove_allowlisted: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a schedule_max_stake transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  schedule_max_stake: ({max_amount}: {max_amount: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_hb_strict_mode transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Enables or disables strict mode for oracle heartbeat health at settlement (admin only, Issue #264).
   */
  set_hb_strict_mode: ({enabled}: {enabled: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_round_template transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Stores the admin's blueprint for `create_next_from_template` (admin only).
   */
  set_round_template: ({start_price, mode}: {start_price: u128, mode: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a cancel_gov_proposal transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Cancels an unexecuted governance proposal (governance admin/approver only).
   */
  cancel_gov_proposal: ({canceller, proposal_id}: {canceller: string, proposal_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_attestation_key transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured attestation signing key, if enabled (Issue #263).
   */
  get_attestation_key: (options?: MethodOptions) => Promise<AssembledTransaction<Option<Buffer>>>

  /**
   * Construct and simulate a get_dispute_ledgers transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_dispute_ledgers: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_market_snapshot transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns a single-read composite snapshot of current market state:
   * round phase, pool composition, timing buffers, and fee configuration.
   * See `MarketSnapshot` for empty-round semantics.
   */
  get_market_snapshot: (options?: MethodOptions) => Promise<AssembledTransaction<MarketSnapshot>>

  /**
   * Construct and simulate a get_protocol_health transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns a composite protocol health status
   */
  get_protocol_health: (options?: MethodOptions) => Promise<AssembledTransaction<ProtocolHealthStatus>>

  /**
   * Construct and simulate a get_protocol_status transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the global status of the protocol.
   * 
   * This is the canonical single-call status endpoint for frontends and
   * monitoring dashboards. It is a pure projection of [`RuntimeMode`]
   * plus "is a round active" (see `docs/STATUS_CODES.md`):
   * 
   * | `RuntimeMode`       | active round? | return value      |
   * |---------------------|---------------|-------------------|
   * | `FullyPaused` (2)   | any           | `Paused`      (1) |
   * | `ClaimsOnly`  (1)   | any           | `ClaimsOnly`  (2) |
   * | `Normal`      (0)   | no            | `ClaimsOnly`  (2) |
   * | `Normal`      (0)   | yes           | `Active`      (0) |
   * 
   * `Active` is returned only when round mutations (bets, reveals) would
   * actually pass the policy gate; `Paused` only when claims are blocked.
   */
  get_protocol_status: (options?: MethodOptions) => Promise<AssembledTransaction<ProtocolStatus>>

  /**
   * Construct and simulate a resolve_round_multi transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Resolves the active round using a multi-feed oracle payload with
   * median settlement and quorum-based outlier rejection.
   * 
   * Requires `OracleQuorumConfig` to be configured by the admin before
   * this path is available. The legacy single-oracle `resolve_round`
   * remains available independently.
   */
  resolve_round_multi: ({payload}: {payload: MultiFeedPayload}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_attestation_key transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets (or clears) the ed25519 public key used to verify oracle
   * attestation signatures (admin only, Issue #263). `None` disables
   * attestation verification, restoring account-auth-only behaviour.
   */
  set_attestation_key: ({key}: {key: Option<Buffer>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_dispute_ledgers transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_dispute_ledgers: ({ledgers}: {ledgers: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a announce_next_schema transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Announces a target schema version for the next planned migration (admin only).
   * 
   * This sets a "v-next schema template" that operators can inspect before
   * the real migration executes. It does NOT change the active schema.
   */
  announce_next_schema: ({target_version}: {target_version: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a approve_gov_proposal transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Approves a pending governance proposal (governance admin/approver only, distinct from proposer).
   */
  approve_gov_proposal: ({approver, proposal_id}: {approver: string, proposal_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a cancel_config_change transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  cancel_config_change: ({kind}: {kind: ConfigChangeKind}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a clear_round_template transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Removes the configured round template (admin only).
   */
  clear_round_template: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a execute_gov_proposal transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Executes an approved governance proposal (governance admin/approver only).
   */
  execute_gov_proposal: ({executor, proposal_id}: {executor: string, proposal_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_gov_proposal_ttl transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns default proposal TTL in ledgers.
   */
  get_gov_proposal_ttl: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_hb_grace_seconds transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured heartbeat grace period in seconds (default 0, Issue #264).
   */
  get_hb_grace_seconds: (options?: MethodOptions) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a get_min_participants transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_min_participants: (options?: MethodOptions) => Promise<AssembledTransaction<Option<u32>>>

  /**
   * Construct and simulate a get_one_sided_policy transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_one_sided_policy: (options?: MethodOptions) => Promise<AssembledTransaction<OneSidedPolicy>>

  /**
   * Construct and simulate a get_oracle_heartbeat transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the most recent oracle heartbeat record, if any.
   */
  get_oracle_heartbeat: (options?: MethodOptions) => Promise<AssembledTransaction<Option<OracleHeartbeatRecord>>>

  /**
   * Construct and simulate a get_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_pending_winnings: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_protocol_fee_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_protocol_fee_bps: (options?: MethodOptions) => Promise<AssembledTransaction<Option<u32>>>

  /**
   * Construct and simulate a get_round_pool_stats transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_round_pool_stats: (options?: MethodOptions) => Promise<AssembledTransaction<Option<RoundPoolStats>>>

  /**
   * Construct and simulate a get_updown_positions transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_updown_positions: (options?: MethodOptions) => Promise<AssembledTransaction<Map<string, UserPosition>>>

  /**
   * Construct and simulate a set_gov_proposal_ttl transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets default proposal TTL in ledgers (admin only).
   */
  set_gov_proposal_ttl: ({ttl_ledgers}: {ttl_ledgers: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_hb_grace_seconds transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the grace period in seconds between heartbeat staleness and settlement block (admin only, Issue #264).
   */
  set_hb_grace_seconds: ({seconds}: {seconds: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_min_participants transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_min_participants: ({min}: {min: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_protocol_fee_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_protocol_fee_bps: ({bps}: {bps: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_archive_retention transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_archive_retention: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_current_season_id transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the id of the currently-active leaderboard season (default 1).
   */
  get_current_season_id: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_early_cashout_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured early cash-out penalty bps, if enabled.
   */
  get_early_cashout_bps: (options?: MethodOptions) => Promise<AssembledTransaction<Option<u32>>>

  /**
   * Construct and simulate a get_epoch_mint_budget transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_epoch_mint_budget: (options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_hb_override_armed transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns whether the oracle heartbeat override is currently armed (Issue #264).
   */
  get_hb_override_armed: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a get_max_user_exposure transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_max_user_exposure: (options?: MethodOptions) => Promise<AssembledTransaction<Option<i128>>>

  /**
   * Construct and simulate a get_season_user_stats transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns a user's season-scoped stats for `season_id` (active or archived).
   */
  get_season_user_stats: ({season_id, user}: {season_id: u32, user: string}, options?: MethodOptions) => Promise<AssembledTransaction<UserStats>>

  /**
   * Construct and simulate a set_archive_retention transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_archive_retention: ({limit}: {limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_early_cashout_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the early cash-out penalty rate in basis points (admin only).
   * `None` disables early cash-out entirely (default).
   * `Some(bps)` enables it with the given penalty rate (1–1000 bps).
   */
  set_early_cashout_bps: ({bps}: {bps: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_epoch_mint_budget transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_epoch_mint_budget: ({budget}: {budget: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_max_user_exposure transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_max_user_exposure: ({max_exposure}: {max_exposure: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a top_up_insurance_fund transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Top-ups the insurance fund from the caller's vXLM balance (admin only).
   */
  top_up_insurance_fund: ({amount}: {amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a withdraw_protocol_fee transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  withdraw_protocol_fee: ({recipient, amount}: {recipient: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<i128>>>

  /**
   * Construct and simulate a accept_oracle_rotation transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Accepts a pending oracle rotation proposal before expiry (any caller).
   * 
   * **Security**: A mandatory `MIN_ROTATION_DELAY_SECONDS` (1 hour) must
   * elapse between proposal and acceptance. This prevents quiet one-block
   * takeovers — even if the admin key is compromised, the community has a
   * full hour to observe the proposal event and react before the oracle
   * actually changes.
   * 
   * If the delay has not elapsed the call returns `RotationDelayNotElapsed`.
   * If the proposal has expired it returns `NoPendingRotation` and the
   * stale proposal is removed after emitting `("oracle", "expired")`.
   * On success the stored oracle address is updated and
   * `("oracle", "accept")` is emitted with the previous and new addresses.
   */
  accept_oracle_rotation: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a cancel_oracle_rotation transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Cancels a pending oracle rotation proposal before it expires (admin only).
   * 
   * Emits `("oracle", "cancel")` on success.
   */
  cancel_oracle_rotation: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a establish_constitution transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Establishes the on-chain constitution with governance rules (admin only).
   */
  establish_constitution: ({veto_window_ledgers, timelock_ledgers, dual_approval_required}: {veto_window_ledgers: u32, timelock_ledgers: u32, dual_approval_required: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_bet_window_ledgers transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured betting-window length in ledgers.
   */
  get_bet_window_ledgers: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_deviation_ref_mode transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured deviation reference mode (default `StartPrice`, Issue #266).
   */
  get_deviation_ref_mode: (options?: MethodOptions) => Promise<AssembledTransaction<DeviationReferenceMode>>

  /**
   * Construct and simulate a get_oracle_strict_mode transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns whether oracle strict mode is enabled.
   */
  get_oracle_strict_mode: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a get_run_window_ledgers transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured run-window length in ledgers.
   */
  get_run_window_ledgers: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a set_deviation_ref_mode transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the oracle deviation reference mode — `StartPrice` (default) or
   * `Twap` — and, for `Twap`, the trailing sample window size (admin only, Issue #266).
   */
  set_deviation_ref_mode: ({mode, window_samples}: {mode: DeviationReferenceMode, window_samples: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_oracle_strict_mode transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Enables or disables strict mode for oracle confidence (admin only).
   */
  set_oracle_strict_mode: ({enabled}: {enabled: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a apply_scheduled_changes transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  apply_scheduled_changes: ({kind}: {kind: ConfigChangeKind}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_insurance_split_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured insurance split in basis points.
   */
  get_insurance_split_bps: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_leaderboard_by_wins transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Cursor-based page of the global leaderboard ordered by total wins descending.
   * Rejects if `limit` exceeds `MAX_PAGE_SIZE` (100).
   */
  get_leaderboard_by_wins: ({cursor, limit}: {cursor: Option<string>, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<readonly [Array<LeaderboardEntry>, Option<string>]>>>

  /**
   * Construct and simulate a migrate_schema_v1_to_v2 transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Migrates legacy schema version 1 → version 2 (admin only).
   * 
   * When `dry_run` is `true`, all validation checks are performed but no
   * storage writes or events are emitted.
   */
  migrate_schema_v1_to_v2: ({dry_run}: {dry_run: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a migrate_schema_v2_to_v3 transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Migrates schema version 2 → version 3 (admin only).
   * 
   * When `dry_run` is `true`, all validation checks are performed but no
   * storage writes or events are emitted.
   */
  migrate_schema_v2_to_v3: ({dry_run}: {dry_run: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a propose_oracle_rotation transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Proposes a new oracle address with an expiry window (admin only).
   * 
   * The proposal must be accepted via [`Self::accept_oracle_rotation`] before
   * `expires_in_seconds` elapses, otherwise acceptance is rejected.
   * Minimum expiry is 60 seconds.
   * 
   * Emits `("oracle", "propose")`.
   */
  propose_oracle_rotation: ({new_oracle, expires_in_seconds}: {new_oracle: string, expires_in_seconds: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_insurance_split_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the insurance accrual split: how many basis points of each
   * protocol fee are directed to the insurance fund (admin only).
   */
  set_insurance_split_bps: ({bps}: {bps: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a update_oracle_heartbeat transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Records an oracle heartbeat (oracle only).
   */
  update_oracle_heartbeat: ({status}: {status: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a withdraw_insurance_fund transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Withdraws from the insurance fund to a recipient (admin only,
   * requires governance dual-control when approver is set).
   */
  withdraw_insurance_fund: ({recipient, amount}: {recipient: string, amount: i128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<i128>>>

  /**
   * Construct and simulate a get_close_buffer_ledgers transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_close_buffer_ledgers: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_max_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_max_pending_winnings: (options?: MethodOptions) => Promise<AssembledTransaction<Option<i128>>>

  /**
   * Construct and simulate a get_oracle_quorum_config transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured multi-feed oracle quorum config, if any.
   */
  get_oracle_quorum_config: (options?: MethodOptions) => Promise<AssembledTransaction<Option<OracleQuorumConfig>>>

  /**
   * Construct and simulate a get_user_archive_history transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns paginated archived participation history for a user (newest first).
   * Rejects if `limit` exceeds `MAX_PAGE_SIZE` (100).
   */
  get_user_archive_history: ({user, offset, limit}: {user: string, offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<Array<ArchivedRoundSummary>>>>

  /**
   * Construct and simulate a reset_leaderboard_season transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Freezes the active season's rankings into a permanent archive and
   * advances to the next season (admin only). Returns the new season id.
   */
  reset_leaderboard_season: (options?: MethodOptions) => Promise<AssembledTransaction<Result<u32>>>

  /**
   * Construct and simulate a set_close_buffer_ledgers transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_close_buffer_ledgers: ({buffer_ledgers}: {buffer_ledgers: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_max_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_max_pending_winnings: ({max_pending}: {max_pending: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_oracle_quorum_config transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the multi-feed oracle quorum configuration (admin only).
   * 
   * When `Some(config)`, `resolve_round_multi` is enabled. When `None`,
   * multi-feed resolution is disabled. The legacy path is unaffected.
   */
  set_oracle_quorum_config: ({config}: {config: Option<OracleQuorumConfig>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a create_next_from_template transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Creates the next round from the configured template (admin only).
   * Fails with `RoundAlreadyActive` if a round is already active and
   * with `NoRoundTemplate` if no template has been configured.
   */
  create_next_from_template: (options?: MethodOptions) => Promise<AssembledTransaction<Result<u64>>>

  /**
   * Construct and simulate a get_leaderboard_by_streak transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Cursor-based page of the global leaderboard ordered by best streak descending.
   * Rejects if `limit` exceeds `MAX_PAGE_SIZE` (100).
   */
  get_leaderboard_by_streak: ({cursor, limit}: {cursor: Option<string>, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<readonly [Array<LeaderboardEntry>, Option<string>]>>>

  /**
   * Construct and simulate a get_oracle_timestamp_skew transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured oracle timestamp skew, or the default (300 s) if not set.
   */
  get_oracle_timestamp_skew: (options?: MethodOptions) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a get_pending_config_change transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_pending_config_change: ({kind}: {kind: ConfigChangeKind}, options?: MethodOptions) => Promise<AssembledTransaction<Option<PendingConfigChange>>>

  /**
   * Construct and simulate a get_precision_predictions transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_precision_predictions: (options?: MethodOptions) => Promise<AssembledTransaction<Array<PrecisionPrediction>>>

  /**
   * Construct and simulate a get_protocol_fee_treasury transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_protocol_fee_treasury: (options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_updown_positions_page transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_updown_positions_page: ({offset, limit}: {offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<readonly [string, UserPosition]>>>

  /**
   * Construct and simulate a is_access_control_enabled transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  is_access_control_enabled: (options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a schedule_protocol_fee_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  schedule_protocol_fee_bps: ({bps}: {bps: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_insurance_coverage_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured insurance coverage payout rate.
   */
  get_insurance_coverage_bps: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_insurance_fund_balance transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the current insurance fund balance.
   */
  get_insurance_fund_balance: (options?: MethodOptions) => Promise<AssembledTransaction<i128>>

  /**
   * Construct and simulate a get_oracle_stale_threshold transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured oracle stale threshold, or the default (3600 s) if not set.
   */
  get_oracle_stale_threshold: (options?: MethodOptions) => Promise<AssembledTransaction<u64>>

  /**
   * Construct and simulate a get_recent_archived_rounds transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_recent_archived_rounds: ({limit}: {limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<ArchivedRoundSummary>>>

  /**
   * Construct and simulate a place_precision_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  place_precision_prediction: ({user, amount, predicted_price}: {user: string, amount: i128, predicted_price: u128}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a schedule_max_user_exposure transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  schedule_max_user_exposure: ({max_exposure}: {max_exposure: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_access_control_enabled transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_access_control_enabled: ({enabled}: {enabled: boolean}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_insurance_coverage_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the insurance coverage payout rate in basis points (admin only).
   */
  set_insurance_coverage_bps: ({bps}: {bps: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_oracle_stale_threshold transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked stale threshold update
   */
  set_oracle_stale_threshold: ({seconds}: {seconds: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_pending_winnings_expiry transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_pending_winnings_expiry: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_precision_payout_policy transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_precision_payout_policy: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a set_pending_winnings_expiry transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_pending_winnings_expiry: ({ledgers}: {ledgers: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_precision_payout_policy transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_precision_payout_policy: ({policy}: {policy: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_deviation_window_samples transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured TWAP window size in samples (Issue #266).
   */
  get_deviation_window_samples: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_oracle_max_deviation_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured oracle max deviation bps, if set.
   */
  get_oracle_max_deviation_bps: (options?: MethodOptions) => Promise<AssembledTransaction<Option<u32>>>

  /**
   * Construct and simulate a get_oracle_rotation_proposal transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the pending oracle rotation proposal, if any.
   */
  get_oracle_rotation_proposal: (options?: MethodOptions) => Promise<AssembledTransaction<Option<OracleRotationProposal>>>

  /**
   * Construct and simulate a set_oracle_max_deviation_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked oracle deviation update
   */
  set_oracle_max_deviation_bps: ({bps}: {bps: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a arm_oracle_deviation_override transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Arms a one-shot override to bypass deviation checks for the next settlement (admin only).
   */
  arm_oracle_deviation_override: (options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_insurance_eligible_events transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the list of eligible insurance event type discriminants.
   */
  get_insurance_eligible_events: (options?: MethodOptions) => Promise<AssembledTransaction<Array<u32>>>

  /**
   * Construct and simulate a get_oracle_min_confidence_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Returns the configured minimum oracle confidence bps, if set.
   */
  get_oracle_min_confidence_bps: (options?: MethodOptions) => Promise<AssembledTransaction<Option<u32>>>

  /**
   * Construct and simulate a get_user_precision_prediction transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_user_precision_prediction: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Option<PrecisionPrediction>>>

  /**
   * Construct and simulate a schedule_max_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  schedule_max_pending_winnings: ({max_pending}: {max_pending: Option<i128>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a schedule_oracle_deviation_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  schedule_oracle_deviation_bps: ({bps}: {bps: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_insurance_eligible_events transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the whitelist of eligible insurance event types (admin only).
   */
  set_insurance_eligible_events: ({events}: {events: Array<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_oracle_min_confidence_bps transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Sets the minimum oracle confidence threshold in basis points (admin only).
   */
  set_oracle_min_confidence_bps: ({min_bps}: {min_bps: Option<u32>}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_max_precision_participants transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_max_precision_participants: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a get_precision_predictions_page transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_precision_predictions_page: ({offset, limit}: {offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<PrecisionPrediction>>>

  /**
   * Construct and simulate a get_season_leaderboard_by_wins transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Paginated wins leaderboard for `season_id` — live for the active
   * season, frozen archive for any past season.
   */
  get_season_leaderboard_by_wins: ({season_id, offset, limit}: {season_id: u32, offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<SeasonLeaderboardEntry>>>

  /**
   * Construct and simulate a schedule_oracle_timestamp_skew transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Schedules a timelocked update to the oracle timestamp skew (admin only).
   */
  schedule_oracle_timestamp_skew: ({seconds}: {seconds: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a set_max_precision_participants transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  set_max_precision_participants: ({max}: {max: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_user_archived_participation transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  get_user_archived_participation: ({user, round_id}: {user: string, round_id: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Option<UserRoundOutcome>>>

  /**
   * Construct and simulate a schedule_oracle_stale_threshold transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  schedule_oracle_stale_threshold: ({seconds}: {seconds: u64}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

  /**
   * Construct and simulate a get_season_leaderboard_by_streak transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Paginated best-streak leaderboard for `season_id` — live for the
   * active season, frozen archive for any past season.
   */
  get_season_leaderboard_by_streak: ({season_id, offset, limit}: {season_id: u32, offset: u32, limit: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Array<SeasonLeaderboardEntry>>>

  /**
   * Construct and simulate a reclaim_expired_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  reclaim_expired_pending_winnings: ({user}: {user: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<i128>>>

  /**
   * Construct and simulate a schedule_pending_winnings_expiry transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   */
  schedule_pending_winnings_expiry: ({ledgers}: {ledgers: u32}, options?: MethodOptions) => Promise<AssembledTransaction<Result<void>>>

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
      new ContractSpec([ "AAAAAQAAACpQcmljZSByZXBvcnQgc3VibWl0dGVkIGJ5IGFuIG9yYWNsZSBmZWVkZXIAAAAAAAAAAAAMRmVlZGVyUmVwb3J0AAAAAwAAAAAAAAAGZmVlZGVyAAAAAAATAAAAAAAAAAVwcmljZQAAAAAAAAsAAAAAAAAACXRpbWVzdGFtcAAAAAAAAAY=",
        "AAAAAQAAAC9NZW1iZXIgcmVnaXN0cmF0aW9uIHJlY29yZCBmb3IgYW4gb3JhY2xlIGZlZWRlcgAAAAAAAAAAD0NvbW1pdHRlZU1lbWJlcgAAAAAEAAAAAAAAAAZhY3RpdmUAAAAAAAEAAAAAAAAABmZlZWRlcgAAAAAAEwAAAAAAAAANcmVnaXN0ZXJlZF9hdAAAAAAAAAYAAAAAAAAABXN0YWtlAAAAAAAACw==",
        "AAAAAQAAAAAAAAAAAAAABVJvdW5kAAAAAAAACQAAAAAAAAAOYmV0X2VuZF9sZWRnZXIAAAAAAAQAAAAAAAAACmVuZF9sZWRnZXIAAAAAAAQAAAAAAAAABG1vZGUAAAfQAAAACVJvdW5kTW9kZQAAAAAAAAAAAAAJcG9vbF9kb3duAAAAAAAACwAAAAAAAAAHcG9vbF91cAAAAAALAAAAAAAAAAtwcmljZV9zdGFydAAAAAAKAAAAAAAAAAhyb3VuZF9pZAAAAAYAAAAAAAAADHN0YXJ0X2xlZGdlcgAAAAQAAAAAAAAAD3N0YXJ0X3RpbWVzdGFtcAAAAAAG",
        "AAAAAgAAACNSZXByZXNlbnRzIHdoaWNoIHNpZGUgYSB1c2VyIGJldCBvbgAAAAAAAAAAB0JldFNpZGUAAAAAAgAAAAAAAAAAAAAAAlVwAAAAAAAAAAAAAAAAAAREb3du",
        "AAAAAgAAAEpMZWdhY3kgbW9ub2xpdGhpYyBzdG9yYWdlIGtleSDigJQgcmV0YWluZWQgZm9yIGEgZmV3IG1pZ3JhdGlvbi9yZWFkIHBhdGhzLgAAAAAAAAAAAAdEYXRhS2V5AAAAAC0AAAABAAAAAAAAAAdCYWxhbmNlAAAAAAEAAAATAAAAAAAAAAAAAAAFQWRtaW4AAAAAAAAAAAAAAAAAAAZPcmFjbGUAAAAAAAAAAAAAAAAADVNjaGVtYVZlcnNpb24AAAAAAAAAAAAAAAAAAAtBY3RpdmVSb3VuZAAAAAAAAAAAAAAAAAlQb3NpdGlvbnMAAAAAAAAAAAAAAAAAAA9VcERvd25Qb3NpdGlvbnMAAAAAAAAAAAAAAAASUHJlY2lzaW9uUG9zaXRpb25zAAAAAAABAAAAAAAAAA9QZW5kaW5nV2lubmluZ3MAAAAAAQAAABMAAAABAAAAAAAAAAlVc2VyU3RhdHMAAAAAAAABAAAAEwAAAAAAAAAAAAAABlBhdXNlZAAAAAAAAAAAAAAAAAAQQmV0V2luZG93TGVkZ2VycwAAAAAAAAAAAAAAEFJ1bldpbmRvd0xlZGdlcnMAAAAAAAAAAAAAABJDbG9zZUJ1ZmZlckxlZGdlcnMAAAAAAAAAAAAAAAAAC0xhc3RSb3VuZElkAAAAAAEAAAAAAAAACFBvc2l0aW9uAAAAAgAAAAYAAAATAAAAAQAAAAAAAAARUHJlY2lzaW9uUG9zaXRpb24AAAAAAAACAAAABgAAABMAAAABAAAAAAAAABNQcmVjaXNpb25Db21taXRtZW50AAAAAAIAAAAGAAAAEwAAAAEAAAAAAAAAEVJvdW5kUGFydGljaXBhbnRzAAAAAAAAAQAAAAYAAAAAAAAAAAAAAAhNYXhTdGFrZQAAAAAAAAAAAAAAFE1heFVzZXJSb3VuZEV4cG9zdXJlAAAAAAAAAAAAAAASTWF4UGVuZGluZ1dpbm5pbmdzAAAAAAABAAAAAAAAAA5DYW5jZWxsZWRSb3VuZAAAAAAAAQAAAAYAAAABAAAAAAAAABNDb25zdW1lZE9yYWNsZU5vbmNlAAAAAAIAAAAGAAAABgAAAAAAAAAAAAAAD01pblBhcnRpY2lwYW50cwAAAAAAAAAAAAAAAA9PcmFjbGVIZWFydGJlYXQAAAAAAAAAAAAAAAAUT3JhY2xlU3RhbGVUaHJlc2hvbGQAAAAAAAAAAAAAABhNYXhQcmVjaXNpb25QYXJ0aWNpcGFudHMAAAAAAAAAAAAAABVPcmFjbGVNYXhEZXZpYXRpb25CcHMAAAAAAAAAAAAAAAAAABxPcmFjbGVEZXZpYXRpb25PdmVycmlkZUFybWVkAAAAAAAAAAAAAAAWT3JhY2xlTWluQ29uZmlkZW5jZUJwcwAAAAAAAAAAAAAAAAAQT3JhY2xlU3RyaWN0TW9kZQAAAAEAAAAAAAAADUFyY2hpdmVkUm91bmQAAAAAAAABAAAABgAAAAAAAAAAAAAAFlJlY2VudEFyY2hpdmVkUm91bmRJZHMAAAAAAAEAAAAAAAAAEFVzZXJSb3VuZE91dGNvbWUAAAACAAAABgAAABMAAAAAAAAAAAAAAAxNaWdyYXRlZFRvVjMAAAABAAAAAAAAABNQZW5kaW5nQ29uZmlnQ2hhbmdlAAAAAAEAAAfQAAAAEENvbmZpZ0NoYW5nZUtpbmQAAAAAAAAAAAAAAA5Qcm90b2NvbEZlZUJwcwAAAAAAAAAAAAAAAAATUHJvdG9jb2xGZWVUcmVhc3VyeQAAAAABAAAAAAAAABFMZWRnZXJNaW50Q291bnRlcgAAAAAAAAEAAAAEAAAAAAAAAAAAAAAPTWludExpbWl0Q29uZmlnAAAAAAAAAAAAAAAAFk9yYWNsZVJvdGF0aW9uUHJvcG9zYWwAAAAAAAAAAAAAAAAAEEFyY2hpdmVSZXRlbnRpb24AAAAAAAAAAAAAAA1Sb3VuZFRlbXBsYXRlAAAAAAAAAQAAAAAAAAADRXh0AAAAAAEAAAfQAAAACkRhdGFLZXlFeHQAAA==",
        "AAAAAwAAACFGZWUgaW5jaWRlbmNlIG1vZGVsIChJc3N1ZSAjMjY4KS4AAAAAAAAAAAAACEZlZU1vZGVsAAAAAgAAAAAAAAAIRmVlT25Qb3QAAAAAAAAAAAAAAA1GZWVPbldpbm5pbmdzAAAAAAAAAQ==",
        "AAAAAQAAAOlBbWVuZG1lbnQgcHJvcG9zYWwgZm9yIHBhcmFtZXRlciBjaGFuZ2VzIHdpdGggdGltZWxvY2sgYW5kIHZldG8gd2luZG93IChJc3N1ZSAjMzYzKS4KUmVwcmVzZW50cyBhIHByb3Bvc2VkIGNoYW5nZSB0byBhIHByb3RvY29sIHBhcmFtZXRlciB0aGF0IG11c3QgcGFzcyB0aHJvdWdoIGEKZ292ZXJuYW5jZSBsaWZlY3ljbGU6IG9wdGlvbmFsIHZldG8gd2luZG93LCB0aW1lbG9jaywgdGhlbiBhY3RpdmF0aW9uLgAAAAAAAAAAAAAJQW1lbmRtZW50AAAAAAAACAAAAAAAAAAaYWN0aXZhdGlvbl9kZWFkbGluZV9sZWRnZXIAAAAAAAQAAAAAAAAAEWNyZWF0ZWRfYXRfbGVkZ2VyAAAAAAAABAAAAAAAAAACaWQAAAAAAAYAAAAAAAAACW5ld192YWx1ZQAAAAAAAAAAAAAAAAAADnBhcmFtZXRlcl9uYW1lAAAAAAARAAAAAAAAAAhwcm9wb3NlcgAAABMAAAAAAAAABnN0YXR1cwAAAAAH0AAAAA9BbWVuZG1lbnRTdGF0dXMAAAAAAAAAABR2ZXRvX2RlYWRsaW5lX2xlZGdlcgAAAAQ=",
        "AAAAAgAAADNQcm90ZWN0ZWQgYWRtaW5pc3RyYXRpdmUgYWN0aW9uIHR5cGVzIChJc3N1ZSAjMjcyKS4AAAAAAAAAAAlHb3ZBY3Rpb24AAAAAAAAKAAAAAAAAAAAAAAANUGF1c2VQcm90b2NvbAAAAAAAAAAAAAAAAAAAD1VucGF1c2VQcm90b2NvbAAAAAABAAAAAAAAABFTZXRQcm90b2NvbEZlZUJwcwAAAAAAAAEAAAPoAAAABAAAAAEAAAAAAAAAE1dpdGhkcmF3UHJvdG9jb2xGZWUAAAAAAgAAABMAAAALAAAAAQAAAAAAAAASU2V0VHJlYXN1cnlBZGRyZXNzAAAAAAABAAAAEwAAAAEAAAAAAAAACFNldEFkbWluAAAAAQAAABMAAAABAAAAAAAAAAlTZXRPcmFjbGUAAAAAAAABAAAAEwAAAAEAAAAuV2l0aGRyYXcgZnJvbSB0aGUgaW5zdXJhbmNlIGZ1bmQgKElzc3VlICMzNjcpLgAAAAAAFVdpdGhkcmF3SW5zdXJhbmNlRnVuZAAAAAAAAAIAAAATAAAACwAAAAEAAAA5U2V0IHRoZSBpbnN1cmFuY2UgZmVlIHNwbGl0IGluIGJhc2lzIHBvaW50cyAoSXNzdWUgIzM2NykuAAAAAAAAFFNldEluc3VyYW5jZVNwbGl0QnBzAAAAAQAAAAQAAAABAAAARFNldCB0aGUgaW5zdXJhbmNlIGNvdmVyYWdlIHBheW91dCByYXRlIGluIGJhc2lzIHBvaW50cyAoSXNzdWUgIzM2NykuAAAAF1NldEluc3VyYW5jZUNvdmVyYWdlQnBzAAAAAAEAAAAE",
        "AAAAAgAAAAAAAAAAAAAACUhiR2F0ZUtleQAAAAAAAAEAAAAAAAAAAAAAAAZDb25maWcAAA==",
        "AAAAAwAAAB5Sb3VuZCBtb2RlIGZvciBwcmVkaWN0aW9uIHR5cGUAAAAAAAAAAAAJUm91bmRNb2RlAAAAAAAAAgAAAAAAAAAGVXBEb3duAAAAAAAAAAAAAAAAAAlQcmVjaXNpb24AAAAAAAAB",
        "AAAAAQAAAAAAAAAAAAAACVVzZXJTdGF0cwAAAAAAAAQAAAAAAAAAC2Jlc3Rfc3RyZWFrAAAAAAQAAAAAAAAADmN1cnJlbnRfc3RyZWFrAAAAAAAEAAAAAAAAAAx0b3RhbF9sb3NzZXMAAAAEAAAAAAAAAAp0b3RhbF93aW5zAAAAAAAE",
        "AAAAAgAAAAAAAAAAAAAACkRhdGFLZXlFeHQAAAAAAAoAAAAAAAAAAAAAAA9MZWFkZXJib2FyZFdpbnMAAAAAAAAAAAAAAAARTGVhZGVyYm9hcmRTdHJlYWsAAAAAAAAAAAAAAAAAAAhTZWFzb25JZAAAAAEAAAAAAAAAD1NlYXNvblVzZXJTdGF0cwAAAAACAAAABAAAABMAAAAAAAAAAAAAABVTZWFzb25MZWFkZXJib2FyZFdpbnMAAAAAAAAAAAAAAAAAABdTZWFzb25MZWFkZXJib2FyZFN0cmVhawAAAAABAAAAAAAAAA1TZWFzb25BcmNoaXZlAAAAAAAAAQAAAAQAAAAAAAAAUE9uLWNoYWluIGNvbnN0aXR1dGlvbiBtZXRhZGF0YSBkZWZpbmluZyBwYXJhbWV0ZXIgZ292ZXJuYW5jZSBydWxlcyAoSXNzdWUgIzM2MykuAAAAFENvbnN0aXR1dGlvbk1ldGFkYXRhAAAAAQAAAC5QZW5kaW5nIGFtZW5kbWVudCBwcm9wb3NhbCBieSBJRCAoSXNzdWUgIzM2MykuAAAAAAAJQW1lbmRtZW50AAAAAAAAAQAAAAYAAAAAAAAAMU1vbm90b25pYyBjb3VudGVyIGZvciBhbWVuZG1lbnQgSURzIChJc3N1ZSAjMzYzKS4AAAAAAAAPTmV4dEFtZW5kbWVudElkAA==",
        "AAAAAwAAAX5MaWZlY3ljbGUgcGhhc2Ugb2YgYW4gYWN0aXZlIHJvdW5kLCBkZXJpdmVkIGZyb20gbGVkZ2VyIHdpbmRvd3MuCgpTZW1hbnRpY3MgKGdpdmVuIGBzdGFydF9sZWRnZXJgLCBgYmV0X2VuZF9sZWRnZXJgLCBgZW5kX2xlZGdlcmApOgotIGBCZXR0aW5nYDogYGxlZGdlciA8IGJldF9lbmRfbGVkZ2VyYCDigJQgYmV0cyBhbmQgcHJlY2lzaW9uIHByZWRpY3Rpb25zIGFjY2VwdGVkCi0gYFJ1bm5pbmdgOiBgYmV0X2VuZF9sZWRnZXIg4omkIGxlZGdlciA8IGVuZF9sZWRnZXJgIOKAlCByZXZlYWwgd2luZG93IChwcmVjaXNpb24pCi0gYFJlc29sdmFibGVgOiBgbGVkZ2VyIOKJpSBlbmRfbGVkZ2VyYCDigJQgcm91bmQgbWF5IGJlIHNldHRsZWQgdmlhIG9yYWNsZSBwYXlsb2FkAAAAAAAAAAAAClJvdW5kUGhhc2UAAAAAAAMAAAAAAAAAB0JldHRpbmcAAAAAAQAAAAAAAAAHUnVubmluZwAAAAACAAAAAAAAAApSZXNvbHZhYmxlAAAAAAAD",
        "AAAAAwAAAC5QYXJ0aWNpcGFudCBhY2Nlc3MtY29udHJvbCBzdGF0ZSAoSXNzdWUgIzI3NCkuAAAAAAAAAAAAC0FjY2Vzc1N0YXRlAAAAAAMAAAAAAAAABE9wZW4AAAAAAAAAAAAAAAtBbGxvd2xpc3RlZAAAAAABAAAAAAAAAApEZW55bGlzdGVkAAAAAAAC",
        "AAAAAgAAAKtQYXJhbWV0ZXJsZXNzIHN5c3RlbSwgY29uZmlnLCBhbmQgbWV0YWRhdGEgc3RvcmFnZSBrZXlzLgoKU3BsaXQgZnJvbSBgRGF0YUtleWAgdG8gc3RheSB1bmRlciB0aGUgWERSIHVuaW9uIDUwLWNhc2UgbGltaXQKKGBWZWNNPFNjU3BlY1VkdFVuaW9uQ2FzZVYwLCA1MD5gIGluIHN0ZWxsYXIteGRyKS4AAAAAAAAAAAtEYXRhS2V5Q29yZQAAAAAsAAAAAAAAAAAAAAAFQWRtaW4AAAAAAAAAAAAAAAAAAAZPcmFjbGUAAAAAAAAAAAB0T24tY2hhaW4gc3RvcmFnZSBzY2hlbWEgdmVyc2lvbiBmb3IgbWlncmF0aW9uIHNhZmV0eS4KSWYgbWlzc2luZywgdGhlIGNvbnRyYWN0IHRyZWF0cyBpdCBhcyBsZWdhY3kgc2NoZW1hIHZlcnNpb24gMS4AAAANU2NoZW1hVmVyc2lvbgAAAAAAAAAAAAAAAAAAC0FjdGl2ZVJvdW5kAAAAAAAAAAAAAAAACVBvc2l0aW9ucwAAAAAAAAAAAAAAAAAAD1VwRG93blBvc2l0aW9ucwAAAAAAAAAAAAAAABJQcmVjaXNpb25Qb3NpdGlvbnMAAAAAAAAAAAAAAAAABlBhdXNlZAAAAAAAAAAAAAAAAAAQQmV0V2luZG93TGVkZ2VycwAAAAAAAAAAAAAAEFJ1bldpbmRvd0xlZGdlcnMAAAAAAAAAAAAAABJDbG9zZUJ1ZmZlckxlZGdlcnMAAAAAAAAAAAAAAAAAC0xhc3RSb3VuZElkAAAAAAAAAAA7TWF4aW11bSBzdGFrZSBhbGxvd2VkIHBlciBpbmRpdmlkdWFsIGJldCAoTm9uZSA9IHVubGltaXRlZCkAAAAACE1heFN0YWtlAAAAAAAAAEFNYXhpbXVtIGN1bXVsYXRpdmUgZXhwb3N1cmUgcGVyIHVzZXIgcGVyIHJvdW5kIChOb25lID0gdW5saW1pdGVkKQAAAAAAABRNYXhVc2VyUm91bmRFeHBvc3VyZQAAAAAAAAA/TWF4aW11bSBwZW5kaW5nIHdpbm5pbmdzIGFsbG93ZWQgcGVyIGFjY291bnQgKE5vbmUgPSB1bmxpbWl0ZWQpAAAAABJNYXhQZW5kaW5nV2lubmluZ3MAAAAAAAAAAABRTWluaW11bSBwYXJ0aWNpcGFudCBjb3VudCBmb3IgY29tcGV0aXRpdmUgc2V0dGxlbWVudDsgdW5zZXQgPSBubyBtaW5pbXVtIGVuZm9yY2VkAAAAAAAAD01pblBhcnRpY2lwYW50cwAAAAAAAAAANE9yYWNsZSBoZWFydGJlYXQ6IGxhc3QgcmVjb3JkZWQgdGltZXN0YW1wIGFuZCBzdGF0dXMAAAAPT3JhY2xlSGVhcnRiZWF0AAAAAAAAAABRU3RhbGUtaGVhcnRiZWF0IHRocmVzaG9sZCBpbiBzZWNvbmRzIChhZG1pbi1jb25maWd1cmFibGUpOyB1bnNldCA9IDM2MDAgcyBkZWZhdWx0AAAAAAAAFE9yYWNsZVN0YWxlVGhyZXNob2xkAAAAAAAAAExNYXhpbXVtIHBhcnRpY2lwYW50cyBhY2NlcHRlZCBpbiBhIFByZWNpc2lvbiByb3VuZDsgdW5zZXQgPSBwcm90b2NvbCBkZWZhdWx0AAAAGE1heFByZWNpc2lvblBhcnRpY2lwYW50cwAAAAAAAABrT3JhY2xlIG1heCBkZXZpYXRpb24gdGhyZXNob2xkIGluIGJhc2lzIHBvaW50cyAoMSBicCA9IDAuMDElKS4KSWYgdW5zZXQsIGRldmlhdGlvbiBndWFyZHJhaWxzIGFyZSBkaXNhYmxlZC4AAAAAFU9yYWNsZU1heERldmlhdGlvbkJwcwAAAAAAAAAAAABxT25lLXNob3QgYWRtaW4gb3ZlcnJpZGUgYWxsb3dpbmcgdGhlIG5leHQgc2V0dGxlbWVudCB0byBieXBhc3MgZGV2aWF0aW9uIGNoZWNrcy4KQXV0b21hdGljYWxseSBjbGVhcmVkIGFmdGVyIHVzZS4AAAAAAAAcT3JhY2xlRGV2aWF0aW9uT3ZlcnJpZGVBcm1lZAAAAAAAAABuTWluaW11bSBvcmFjbGUgY29uZmlkZW5jZSB0aHJlc2hvbGQgaW4gYmFzaXMgcG9pbnRzICgw4oCTMTAwMDApLgpJZiB1bnNldCwgY29uZmlkZW5jZSBndWFyZHJhaWxzIGFyZSBkaXNhYmxlZC4AAAAAABZPcmFjbGVNaW5Db25maWRlbmNlQnBzAAAAAAAAAAAASFdoZW4gdHJ1ZSwgcGF5bG9hZHMgd2l0aCBtaXNzaW5nIGNvbmZpZGVuY2UgYXJlIHJlamVjdGVkIGluIHN0cmljdCBtb2RlLgAAABBPcmFjbGVTdHJpY3RNb2RlAAAAAAAAADxPcmRlcmVkIHJvdW5kIGlkcyBmb3IgYXJjaGl2ZSByZXRlbnRpb24gKG9sZGVzdCBhdCBpbmRleCAwKS4AAAAWUmVjZW50QXJjaGl2ZWRSb3VuZElkcwAAAAAAAAAAAEVNYXJrZXIgd3JpdHRlbiBieSBtaWdyYXRlX3NjaGVtYV92Ml90b192MyB0byBwcm92ZSB0aGUgbWlncmF0aW9uIHJhbi4AAAAAAAAMTWlncmF0ZWRUb1YzAAAAAAAAAMlPcHRpb25hbCBwcm90b2NvbCBzZXR0bGVtZW50IGZlZSBpbiBiYXNpcyBwb2ludHMgKDEgYnAgPSAwLjAxJSkuCmBOb25lYCAoa2V5IGFic2VudCkgbWVhbnMgZmVlIGRpc2FibGVkIOKAlCBubyBiZWhhdmlvdXIgY2hhbmdlLgpIYXJkIGNhcCBvbiBmZWUgaXMgZW5mb3JjZWQgYXQgdGhlIGNvbnRyYWN0IGxheWVyLCBub3QgYnkgc3RvcmFnZSBzaGFwZS4AAAAAAAAOUHJvdG9jb2xGZWVCcHMAAAAAAAAAAACgT24tY2hhaW4gYWNjdW11bGF0ZWQgcHJvdG9jb2wgZmVlIGJhbGFuY2UgaW4gc3Ryb29wcyAoaTEyOCkuCkFkbWluIHdpdGhkcmF3cyB2aWEgdGhlIGRlZGljYXRlZCB3aXRoZHJhd2FsIG1ldGhvZDsgZG9lcyBOT1QgbWl4CmludG8gdGhlIHBlci11c2VyIGJhbGFuY2UgbGVkZ2VyLgAAABNQcm90b2NvbEZlZVRyZWFzdXJ5AAAAAAAAAABFTWludCBsaW1pdCBjb25maWd1cmF0aW9uOiBtYXhpbXVtIG51bWJlciBvZiBtaW50cyBhbGxvd2VkIHBlciBsZWRnZXIuAAAAAAAAD01pbnRMaW1pdENvbmZpZwAAAAAAAAAANlBlbmRpbmcgdHdvLXN0ZXAgb3JhY2xlIHJvdGF0aW9uIHByb3Bvc2FsIHdpdGggZXhwaXJ5LgAAAAAAFk9yYWNsZVJvdGF0aW9uUHJvcG9zYWwAAAAAAAAAAACsQ29uZmlndXJhYmxlIGFyY2hpdmUgcmV0ZW50aW9uIGxpbWl0OiBtYXhpbXVtIG51bWJlciBvZiBBcmNoaXZlZFJvdW5kIGVudHJpZXMKcmV0YWluZWQgb24tY2hhaW4gYmVmb3JlIHRoZSBvbGRlc3QgYXJlIHBydW5lZCAoRklGTykuIElmIHVuc2V0LCB0aGUgcHJvdG9jb2wKZGVmYXVsdCBpcyB1c2VkLgAAABBBcmNoaXZlUmV0ZW50aW9uAAAAAAAAALhBZG1pbi1jb25maWd1cmVkIGJsdWVwcmludCB1c2VkIGJ5IGBjcmVhdGVfbmV4dF9mcm9tX3RlbXBsYXRlYCB0byBzcGluCnVwIHRoZSBuZXh0IHJvdW5kIHdpdGhvdXQgcmUtc3BlY2lmeWluZyBgc3RhcnRfcHJpY2VgIC8gYG1vZGVgIGVhY2gKdGltZS4gQWJzZW50IG1lYW5zIG5vIHRlbXBsYXRlIGlzIGNvbmZpZ3VyZWQuAAAADVJvdW5kVGVtcGxhdGUAAAAAAAAAAAAANUFkbWluLWNvbmZpZ3VyZWQgbXVsdGktZmVlZCBvcmFjbGUgcXVvcnVtIHBhcmFtZXRlcnMuAAAAAAAADE9yYWNsZVF1b3J1bQAAAAAAAAA0QW5ub3VuY2VkIG5leHQgc2NoZW1hIHZlcnNpb24gZm9yIG1pZ3JhdGlvbiBwcmV2aWV3LgAAABFOZXh0U2NoZW1hVmVyc2lvbgAAAAAAAAAAAAA5TWluaW11bSBiZXQgYW1vdW50IChkdXN0IHByb3RlY3Rpb24pLiBVbnNldCA9IG5vIG1pbmltdW0uAAAAAAAABk1pbkJldAAAAAAAAAAAADFFcG9jaCBtaW50IGJ1ZGdldDogdG90YWwgbWludHMgYWxsb3dlZCBwZXIgZXBvY2guAAAAAAAAD0Vwb2NoTWludEJ1ZGdldAAAAAAAAAAASEVhcmx5IGNhc2gtb3V0IHBlbmFsdHkgaW4gYmFzaXMgcG9pbnRzLiBVbnNldCA9IGVhcmx5IGNhc2gtb3V0IGRpc2FibGVkLgAAAA9FYXJseUNhc2hvdXRCcHMAAAAAAAAAADlGZWUgaW5jaWRlbmNlIG1vZGVsOiBGZWVPblBvdCAoZGVmYXVsdCkgb3IgRmVlT25XaW5uaW5ncy4AAAAAAAAIRmVlTW9kZWwAAAAAAAAAOERpc3B1dGUgd2luZG93IGxlbmd0aCBpbiBsZWRnZXJzLiAwID0gbm8gZGlzcHV0ZSB3aW5kb3cuAAAADkRpc3B1dGVMZWRnZXJzAAAAAAAAAAAAKFBheW91dCBwb2xpY3kgZm9yIFByZWNpc2lvbiBtb2RlIHJvdW5kcy4AAAAVUHJlY2lzaW9uUGF5b3V0UG9saWN5AAAAAAAAAAAAAENXaGVuIHRydWUsIG9ubHkgYWxsb3dsaXN0ZWQgYWRkcmVzc2VzIG1heSBwYXJ0aWNpcGF0ZSAoSXNzdWUgIzI3NCkuAAAAABRBY2Nlc3NDb250cm9sRW5hYmxlZAAAAAAAAAArU2Vjb25kYXJ5IGdvdmVybmFuY2UgYXBwcm92ZXIgKElzc3VlICMyNzIpLgAAAAALR292QXBwcm92ZXIAAAAAAAAAACtEZWZhdWx0IGdvdmVybmFuY2UgcHJvcG9zYWwgVFRMIGluIGxlZGdlcnMuAAAAABVHb3ZQcm9wb3NhbFR0bExlZGdlcnMAAAAAAAAAAAAALk1vbm90b25pYyBjb3VudGVyIGZvciBnb3Zlcm5hbmNlIHByb3Bvc2FsIGlkcy4AAAAAABFOZXh0R292UHJvcG9zYWxJZAAAAAAAAAEAAABET3ZlcmZsb3cgYnVja2V0IGZvciBsZWFkZXJib2FyZC9zZWFzb24ga2V5cyB1bmRlciBYRFIgNTAtY2FzZSBsaW1pdC4AAAADRXh0AAAAAAEAAAfQAAAACkRhdGFLZXlFeHQAAA==",
        "AAAAAQAAAChTdG9yZWQgZ292ZXJuYW5jZSBwcm9wb3NhbCAoSXNzdWUgIzI3MikuAAAAAAAAAAtHb3ZQcm9wb3NhbAAAAAAHAAAAAAAAAAZhY3Rpb24AAAAAB9AAAAAJR292QWN0aW9uAAAAAAAAAAAAAAhhcHByb3ZlcgAAA+gAAAATAAAAAAAAABFjcmVhdGVkX2F0X2xlZGdlcgAAAAAAAAQAAAAAAAAAEWV4cGlyZXNfYXRfbGVkZ2VyAAAAAAAABAAAAAAAAAACaWQAAAAAAAYAAAAAAAAACHByb3Bvc2VyAAAAEwAAAAAAAAAGc3RhdHVzAAAAAAfQAAAAEUdvdlByb3Bvc2FsU3RhdHVzAAAA",
        "AAAAAQAAAAAAAAAAAAAAC1ByaWNlU2FtcGxlAAAAAAIAAAAAAAAABXByaWNlAAAAAAAACgAAAAAAAAAJdGltZXN0YW1wAAAAAAAABg==",
        "AAAAAwAABABTdGF0dXMgb2YgYSBzcGVjaWZpYyByb3VuZCwgcmV0dXJuZWQgYnkgYGdldF9yb3VuZF9zdGF0dXMocm91bmRfaWQpYC4KClF1ZXJpZXMgYSByb3VuZCBieSBpdHMgbW9ub3RvbmljIGByb3VuZF9pZGAuIENvdmVycyBhbGwgbGlmZWN5Y2xlCnN0YWdlcyBmcm9tIGNyZWF0aW9uIHRocm91Z2ggdGVybWluYWwgc2V0dGxlbWVudC4KCiMjIFN0YXR1cyBjb2RlcwoKfCB2YWx1ZSB8IHZhcmlhbnQgICAgICAgICAgfCBkZXNjcmlwdGlvbiAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICB8CnwtLS0tLS0tfC0tLS0tLS0tLS0tLS0tLS0tLXwtLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLXwKfCAwICAgICB8IGBVbmtub3duYCAgICAgICAgfCBSb3VuZCBkb2VzIG5vdCBleGlzdCBvciBoYXMgYmVlbiBwcnVuZWQgZnJvbSB0aGUgb24tY2hhaW4gYXJjaGl2ZS4gICAgICAgICAgICAgICB8CnwgMSAgICAgfCBgQmV0dGluZ2AgICAgICAgIHwgUm91bmQgaXMgYWN0aXZlOyBiZXRzIGFuZCBwcmVkaWN0aW9ucyBhY2NlcHRlZCAoYGxlZGdlciA8IGJldF9lbmRfbGVkZ2VyYCkuICAgICAgfAp8IDIgICAgIHwgYFJ1bm5pbmdgICAgICAgICB8IEJldHRpbmcgY2xvc2VkOyByZXZlYWwgd2luZG93IG9wZW4gKGBiZXRfZW5kX2xlZGdlciDiiaQgbGVkZ2VyIDwgZW5kX2xlZGdlcmApLiAgICB8CnwgMyAgICAgfCBgQXdhaXRpbmdSZXNvbHZlYHwgUm91bmQgZW5kZWQ7IGF3YWl0aW5nIG9yYWNsZSBzZXR0bGVtZW50IChgbGVkZ2VyIOKJpSBlbmRfbGVkZ2VyYCkuICAgICAgICAgICAgICAgIHwKfCA0ICAgICB8IGBSZXNvbHZlZGAgICAgICAgfCBPcmFjbGUgc2V0dGxlZCB0aGUgcm91bmQ7IHBvdCBkaXN0cmlidXRlZCB0byB3aW5uZXJzLiAgICAgICAgICAgICAgICAgICAgICAgICAgICB8CnwgNSAgICAgfCBgQ2FuY2VsbGVkYCAgICAgIHwgQWRtAAAAAAAAAAtSb3VuZFN0YXR1cwAAAAAIAAAAQlJvdW5kIGRvZXMgbm90IGV4aXN0IG9yIGhhcyBiZWVuIHBydW5lZCBmcm9tIHRoZSBvbi1jaGFpbiBhcmNoaXZlLgAAAAAAB1Vua25vd24AAAAAAAAAAEtSb3VuZCBpcyBhY3RpdmU7IGJldHMgYW5kIHByZWRpY3Rpb25zIGFjY2VwdGVkIChgbGVkZ2VyIDwgYmV0X2VuZF9sZWRnZXJgKS4AAAAAB0JldHRpbmcAAAAAAQAAAFRCZXR0aW5nIGlzIGNsb3NlZDsgcmV2ZWFsIHdpbmRvdyBpcyBvcGVuIChgYmV0X2VuZF9sZWRnZXIg4omkIGxlZGdlciA8IGVuZF9sZWRnZXJgKS4AAAAHUnVubmluZwAAAAACAAAAT1JvdW5kIGhhcyBlbmRlZCBhbmQgaXMgd2FpdGluZyBmb3Igb3JhY2xlIHNldHRsZW1lbnQgKGBsZWRnZXIg4omlIGVuZF9sZWRnZXJgKS4AAAAAD0F3YWl0aW5nUmVzb2x2ZQAAAAADAAAAPk9yYWNsZSBzZXR0bGVkIHRoZSByb3VuZCBub3JtYWxseTsgcG90IGRpc3RyaWJ1dGVkIHRvIHdpbm5lcnMuAAAAAAAIUmVzb2x2ZWQAAAAEAAAAL0FkbWluIGNhbmNlbGxlZCB0aGUgcm91bmQ7IGFsbCBzdGFrZXMgcmVmdW5kZWQuAAAAAAlDYW5jZWxsZWQAAAAAAAAFAAAASFNldHRsZW1lbnQgdHJpZ2dlcmVkIGJ1dCBpbnN1ZmZpY2llbnQgcGFydGljaXBhbnRzOyBhbGwgc3Rha2VzIHJlZnVuZGVkLgAAAA5GYWxsYmFja1JlZnVuZAAAAAAABgAAAEBEaXNwdXRlIHdpbmRvdyB2b2lkOyBhbGwgcGFydGljaXBhbnRzIHJlZnVuZGVkIHRoZWlyIGZ1bGwgc3Rha2UuAAAABlZvaWRlZAAAAAAABw==",
        "AAAAAwAAACdSdW50aW1lIG1vZGUgZm9yIHRoZSBjb250cmFjdCBsaWZlY3ljbGUAAAAAAAAAAAtSdW50aW1lTW9kZQAAAAADAAAAAAAAAAZOb3JtYWwAAAAAAAAAAAAAAAAACkNsYWltc09ubHkAAAAAAAEAAAAAAAAAC0Z1bGx5UGF1c2VkAAAAAAI=",
        "AAAAAQAAAAAAAAAAAAAADEhiR2F0ZUNvbmZpZwAAAAMAAAAAAAAADWdyYWNlX3NlY29uZHMAAAAAAAAGAAAAAAAAAA5vdmVycmlkZV9hcm1lZAAAAAAAAQAAAAAAAAALc3RyaWN0X21vZGUAAAAAAQ==",
        "AAAAAwAAAEVQb2xpY3kgYWN0aW9uIGNsYXNzIGNvbnN1bWVkIGJ5IHRoZSBjZW50cmFsIHBvbGljeSBnYXRlIChJc3N1ZSAjMjYxKS4AAAAAAAAAAAAADFBvbGljeUFjdGlvbgAAAAQAAAAAAAAADVJvdW5kTXV0YXRpb24AAAAAAAAAAAAAAAAAAAVDbGFpbQAAAAAAAAEAAAAAAAAAC0FkbWluQ29uZmlnAAAAAAIAAAAAAAAAClNldHRsZW1lbnQAAAAAAAM=",
        "AAAAAQAAAAAAAAAAAAAADFVzZXJQb3NpdGlvbgAAAAIAAAAAAAAABmFtb3VudAAAAAAACwAAAAAAAAAEc2lkZQAAB9AAAAAHQmV0U2lkZQA=",
        "AAAAAgAAALFQYXJhbWV0ZXJpc2VkIGFuZCByb3VuZC1zY29wZWQgc3RvcmFnZSBrZXlzLgoKU3BsaXQgZnJvbSBgRGF0YUtleWAgdG8gc3RheSB1bmRlciB0aGUgWERSIHVuaW9uIDUwLWNhc2UgbGltaXQuClRoZXNlIHZhcmlhbnRzIGNhcnJ5IHBlci11c2VyLCBwZXItcm91bmQsIG9yIGNvbXBvdW5kLWtleSBwYXlsb2Fkcy4AAAAAAAAAAAAADURhdGFLZXlTY29wZWQAAAAAAAAUAAAAAQAAABZVc2VyIGZpbmFuY2lhbCBiYWxhbmNlAAAAAAAHQmFsYW5jZQAAAAABAAAAEwAAAAEAAAAhVXNlciBwZW5kaW5nIHdpbm5pbmdzIGFjY3VtdWxhdG9yAAAAAAAAD1BlbmRpbmdXaW5uaW5ncwAAAAABAAAAEwAAAAEAAAAbVXNlciBwZXJmb3JtYW5jZSBzdGF0aXN0aWNzAAAAAAlVc2VyU3RhdHMAAAAAAAABAAAAEwAAAAEAAAA+UGVyLXVzZXIgVXBEb3duIHBvc2l0aW9uOiAocm91bmRfaWQsIGFkZHJlc3MpIOKGkiBVc2VyUG9zaXRpb24AAAAAAAhQb3NpdGlvbgAAAAIAAAAGAAAAEwAAAAEAAABKUGVyLXVzZXIgUHJlY2lzaW9uIHByZWRpY3Rpb246IChyb3VuZF9pZCwgYWRkcmVzcykg4oaSIFByZWNpc2lvblByZWRpY3Rpb24AAAAAABFQcmVjaXNpb25Qb3NpdGlvbgAAAAAAAAIAAAAGAAAAEwAAAAEAAABKUGVyLXVzZXIgUHJlY2lzaW9uIGNvbW1pdG1lbnQ6IChyb3VuZF9pZCwgYWRkcmVzcykg4oaSIFByZWNpc2lvbkNvbW1pdG1lbnQAAAAAABNQcmVjaXNpb25Db21taXRtZW50AAAAAAIAAAAGAAAAEwAAAAEAAAA/T3JkZXJlZCBwYXJ0aWNpcGFudCBsaXN0IGZvciBhIHJvdW5kOiByb3VuZF9pZCDihpIgVmVjPEFkZHJlc3M+AAAAABFSb3VuZFBhcnRpY2lwYW50cwAAAAAAAAEAAAAGAAAAAQAAAC9NYXJrZXIgZm9yIGEgY2FuY2VsbGVkIHJvdW5kOiByb3VuZF9pZCDihpIgdHJ1ZQAAAAAOQ2FuY2VsbGVkUm91bmQAAAAAAAEAAAAGAAAAAQAAAIRQZXItcm91bmQgY29uc3VtZWQgb3JhY2xlIG5vbmNlOiAocm91bmRfaWQsIG5vbmNlKSDihpIgdHJ1ZS4KVXNlZCB0byByZWplY3QgZHVwbGljYXRlIG9yYWNsZSBwYXlsb2FkIHN1Ym1pc3Npb25zIGZvciB0aGUgc2FtZSByb3VuZC4AAAATQ29uc3VtZWRPcmFjbGVOb25jZQAAAAACAAAABgAAAAYAAAABAAAAjlBlci11c2VyIG91dGNvbWUgcmVjb3JkIGZvciBhIHNwZWNpZmljIGFyY2hpdmVkIHJvdW5kIChyb3VuZF9pZCwgdXNlcikuClBlcnNpc3RlZCBhdCBzZXR0bGVtZW50IGZvciB1c2VyIGhpc3RvcnkgcXVlcmllcyB3aXRob3V0IGV2ZW50IHJlcGxheS4AAAAAABBVc2VyUm91bmRPdXRjb21lAAAAAgAAAAYAAAATAAAAAQAAAD9UaW1lbG9ja2VkIHBlbmRpbmcgY3JpdGljYWwgY29uZmlnIGNoYW5nZSBrZXllZCBieSBjaGFuZ2Uga2luZC4AAAAAE1BlbmRpbmdDb25maWdDaGFuZ2UAAAAAAQAAB9AAAAAQQ29uZmlnQ2hhbmdlS2luZAAAAAEAAABDUGVyLWxlZGdlciBtaW50IGNvdW50ZXI6IHdyYXBzIHRoZSBleHBsaWNpdCBsZWRnZXIgc2VxdWVuY2UgbnVtYmVyLgAAAAARTGVkZ2VyTWludENvdW50ZXIAAAAAAAABAAAABAAAAAEAAABJQ29tcGFjdCBwb3N0LXNldHRsZW1lbnQgc3VtbWFyeSBrZXllZCBieSByb3VuZCBpZCBmb3IgaGlzdG9yaWNhbCBxdWVyaWVzLgAAAAAAAA1BcmNoaXZlZFJvdW5kAAAAAAAAAQAAAAYAAAABAAAAuVBlci1zZWFzb24sIHBlci11c2VyIHdpbi9sb3NzL3N0cmVhayBzdGF0czogKHNlYXNvbl9pZCwgYWRkcmVzcykg4oaSClVzZXJTdGF0cywgc2NvcGVkIGluZGVwZW5kZW50bHkgb2YgdGhlIGxpZmV0aW1lIGBVc2VyU3RhdHNgIHRvdGFscyBzbwphIHNlYXNvbiByZXNldCBuZXZlciB0b3VjaGVzIGxpZmV0aW1lIGhpc3RvcnkuAAAAAAAAD1NlYXNvblVzZXJTdGF0cwAAAAACAAAABAAAABMAAAABAAAAikZyb3plbiBzbmFwc2hvdCBvZiBhIHNlYXNvbidzIGZpbmFsIHJhbmtpbmdzLCB3cml0dGVuIHdoZW4gdGhlIHNlYXNvbgppcyByZXNldC4gU2Vhc29ucyBhcmUgbmV2ZXIgZGVsZXRlZCDigJQgdGhpcyBpcyBhIHBlcm1hbmVudCBhcmNoaXZlLgAAAAAADVNlYXNvbkFyY2hpdmUAAAAAAAABAAAABAAAAAEAAAAyUGVyLXVzZXIgaW5kZXggb2YgYXJjaGl2ZWQgcm91bmQgSURzIChJc3N1ZSAjMjgxKS4AAAAAABRVc2VyQXJjaGl2ZWRSb3VuZElkcwAAAAEAAAATAAAAAQAAAD1BbGxvd2xpc3QgbWFya2VyIGZvciBwYXJ0aWNpcGFudCBhY2Nlc3MgY29udHJvbCAoSXNzdWUgIzI3NCkuAAAAAAAAC0FsbG93bGlzdGVkAAAAAAEAAAATAAAAAQAAADxEZW55bGlzdCBtYXJrZXIgZm9yIHBhcnRpY2lwYW50IGFjY2VzcyBjb250cm9sIChJc3N1ZSAjMjc0KS4AAAAKRGVueWxpc3RlZAAAAAAAAQAAABMAAAABAAAAL1N0b3JlZCBnb3Zlcm5hbmNlIHByb3Bvc2FsIHJlY29yZCAoSXNzdWUgIzI3MikuAAAAAAtHb3ZQcm9wb3NhbAAAAAABAAAABgAAAAEAAAGFUmVjb3JkcyB3aGljaCByb3VuZCBjbGFpbWVkIGEgZ2l2ZW4gbGVkZ2VyIHNlcXVlbmNlIGFzIGl0cwpgc3RhcnRfbGVkZ2VyYDogc3RhcnRfbGVkZ2VyIC0+IHJvdW5kX2lkLgoKT3JhY2xlIHBheWxvYWRzIGJpbmQgdG8gYFJvdW5kLnN0YXJ0X2xlZGdlcmAgKHNlZSBgT3JhY2xlUGF5bG9hZC5yb3VuZF9pZGApLAp3aGljaCBpcyBub3QgdW5pcXVlIG9uIGl0cyBvd246IGEgcm91bmQgY2FuIGJlIGNhbmNlbGxlZCBhbmQgcmVwbGFjZWQKd2l0aGluIGEgc2luZ2xlIGxlZGdlci4gVGhpcyBtYXJrZXIgbGV0cyBzZXR0bGVtZW50IHJlamVjdCBhIHBheWxvYWQgd2hvc2UKYHN0YXJ0X2xlZGdlcmAgcmVzb2x2ZXMgdG8gYSBkaWZmZXJlbnQgcm91bmQgdGhhbiB0aGUgYWN0aXZlIG9uZS4AAAAAAAAQUm91bmRTdGFydExlZGdlcgAAAAEAAAAE",
        "AAAAAQAAAAAAAAAAAAAADU9yYWNsZVBheWxvYWQAAAAAAAAIAAAAAAAAAAthdHRlc3RhdGlvbgAAAAPoAAAD7gAAAEAAAAC/T3B0aW9uYWwgY29uZmlkZW5jZSBzY29yZSBmcm9tIHRoZSBwcmljZSBmZWVkICgw4oCTMTAwMDAgYnBzLCB3aGVyZSAxMDAwMCA9IDEwMCUpLgpXaGVuIGBOb25lYCwgdGhlIHBheWxvYWQgaXMgdHJlYXRlZCBhcyBhIGxlZ2FjeSBzdWJtaXNzaW9uLgpXaGVuIHN0cmljdCBtb2RlIGlzIGVuYWJsZWQsIGBOb25lYCBpcyByZWplY3RlZC4AAAAACmNvbmZpZGVuY2UAAAAAA+gAAAAEAAAAg0NvbnRyYWN0IGFkZHJlc3MgdGhpcyBwYXlsb2FkIGlzIGludGVuZGVkIGZvci4KVmFsaWRhdGVkIGFnYWluc3QgYGVudi5jdXJyZW50X2NvbnRyYWN0X2FkZHJlc3MoKWAgdG8gcHJldmVudCBjcm9zcy1jb250cmFjdCByZXBsYXkuAAAAAA1jb250cmFjdF9hZGRyAAAAAAAAEwAAAItTSEEtMjU2IGhhc2ggb2YgdGhlIG5ldHdvcmsgcGFzc3BocmFzZSB0aGlzIHBheWxvYWQgdGFyZ2V0cy4KVmFsaWRhdGVkIGFnYWluc3QgYGVudi5sZWRnZXIoKS5uZXR3b3JrX2lkKClgIHRvIHByZXZlbnQgY3Jvc3MtbmV0d29yayByZXBsYXkuAAAAAApuZXR3b3JrX2lkAAAAAAPuAAAAIAAAAWpQZXItcm91bmQgcmVwbGF5LXByb3RlY3Rpb24gbm9uY2UuCgpUaGUgb3JhY2xlIHNlcnZpY2UgbXVzdCBnZW5lcmF0ZSBhIHVuaXF1ZSB2YWx1ZSBwZXIgc3VibWlzc2lvbiBmb3IgYQpnaXZlbiByb3VuZCAoZS5nLiBhIG1vbm90b25pYyBjb3VudGVyIG9yIHJhbmRvbSA2NC1iaXQgdmFsdWUpLiBUaGUKY29udHJhY3QgcmVjb3JkcyBlYWNoIGNvbnN1bWVkIG5vbmNlIHVuZGVyCmBEYXRhS2V5U2NvcGVkOjpDb25zdW1lZE9yYWNsZU5vbmNlKHJvdW5kX2lkLCBub25jZSlgIGFuZCByZWplY3RzIGFueSByZXVzZSwKbWFraW5nIHJlc29sdXRpb24gaWRlbXBvdGVudCBhZ2FpbnN0IGFjY2lkZW50YWwgZHVwbGljYXRlIHN1Ym1pc3Npb25zLgAAAAAABW5vbmNlAAAAAAAABgAAAAAAAAAFcHJpY2UAAAAAAAAKAAACgUJpbmRzIHRoaXMgcGF5bG9hZCB0byBleGFjdGx5IG9uZSByb3VuZC4KCk11c3QgZXF1YWwgdGhlIGFjdGl2ZSByb3VuZCdzICoqYFJvdW5kLnN0YXJ0X2xlZGdlcmAqKiDigJQgdGhlIGxlZGdlcgpzZXF1ZW5jZSBhdCB3aGljaCB0aGUgcm91bmQgd2FzIGNyZWF0ZWQg4oCUIE5PVCB0aGUgbW9ub3RvbmljCmBSb3VuZC5yb3VuZF9pZGAuIFRoZSB0d28gaWRlbnRpZmllcnMgYXJlIHVzZWQgaW4gZGlmZmVyZW50IHBsYWNlczoKYHN0YXJ0X2xlZGdlcmAgYmluZHMgdGhlIHBheWxvYWQgKGFuZCBpcyBjb3ZlcmVkIGJ5IHRoZSBhdHRlc3RhdGlvbgpzaWduYXR1cmUpLCB3aGlsZSBgUm91bmQucm91bmRfaWRgIG5hbWVzcGFjZXMgY29uc3VtZWQgbm9uY2VzIHVuZGVyCmBEYXRhS2V5U2NvcGVkOjpDb25zdW1lZE9yYWNsZU5vbmNlYC4KCmBjcmVhdGVfcm91bmRgIGd1YXJhbnRlZXMgYSBsZWRnZXIgc2VxdWVuY2UgYmFja3MgYXQgbW9zdCBvbmUgcm91bmQKKGBEYXRhS2V5U2NvcGVkOjpSb3VuZFN0YXJ0TGVkZ2VyYCAvIGBSb3VuZFN0YXJ0TGVkZ2VyUmV1c2VkYCksIHNvIHRoaXMKdmFsdWUgaWRlbnRpZmllcyBhIHNpbmdsZSByb3VuZCB1bmFtYmlndW91c2x5LiBTZWUgYFBST1RPQ09MX1NQRUMubWRgCmludmFyaWFudCBJMTAuAAAAAAAACHJvdW5kX2lkAAAABAAAAAAAAAAJdGltZXN0YW1wAAAAAAAABg==",
        "AAAAAQAAAUdBZG1pbi1jb25maWd1cmVkIGJsdWVwcmludCBmb3IgYGNyZWF0ZV9uZXh0X2Zyb21fdGVtcGxhdGVgLgoKTWlycm9ycyB0aGUgYXJndW1lbnRzIGFjY2VwdGVkIGJ5IGBjcmVhdGVfcm91bmRgIChgc3RhcnRfcHJpY2VgLCBgbW9kZWApCnNvIGEga2VlcGVyIGNhbiBzcGluIHVwIHRoZSBuZXh0IHJvdW5kIGFmdGVyIGEgc2V0dGxlL2NhbmNlbCB3aXRob3V0IGFuCm9wZXJhdG9yIHJlLXNwZWNpZnlpbmcgcGFyYW1ldGVycyBlYWNoIHRpbWUuIFZhbGlkYXRlZCB3aXRoIHRoZSBleGFjdApzYW1lIHJ1bGVzIGBjcmVhdGVfcm91bmRgIGFwcGxpZXMgYXQgY3JlYXRpb24gdGltZS4AAAAAAAAAAA1Sb3VuZFRlbXBsYXRlAAAAAAAAAgAAAAAAAAAEbW9kZQAAA+gAAAAEAAAAAAAAAAtzdGFydF9wcmljZQAAAAAK",
        "AAAAAQAAAVRGcm96ZW4gc25hcHNob3Qgb2YgYSBzZWFzb24ncyBmaW5hbCBib3VuZGVkIHJhbmtpbmdzLCB3cml0dGVuIGJ5CmByZXNldF9sZWFkZXJib2FyZF9zZWFzb25gLiBgcGFydGljaXBhbnRfY291bnRgIGlzIHRoZSBudW1iZXIgb2YgZGlzdGluY3QKYWRkcmVzc2VzIHRoYXQgYXBwZWFyZWQgaW4gZWl0aGVyIGJvdW5kZWQgaW5kZXggYXQgcmVzZXQgdGltZSAoYSBsb3dlcgpib3VuZCBvbiB0b3RhbCBzZWFzb24gcGFydGljaXBhbnRzIGJleW9uZCB0aGUgdHJhY2tlZCB0b3AKYExFQURFUkJPQVJEX0xJTUlUYCwgbWlycm9yaW5nIHRoZSBzYW1lIGJvdW5kIHRoZSBsaXZlIGluZGV4ZXMgZW5mb3JjZSkuAAAAAAAAAA1TZWFzb25BcmNoaXZlAAAAAAAABQAAAAAAAAAPZW5kZWRfYXRfbGVkZ2VyAAAAAAQAAAAAAAAAEXBhcnRpY2lwYW50X2NvdW50AAAAAAAABAAAAAAAAAAJc2Vhc29uX2lkAAAAAAAABAAAAAAAAAAGc3RyZWFrAAAAAAPqAAAH0AAAABZTZWFzb25MZWFkZXJib2FyZEVudHJ5AAAAAAAAAAAABHdpbnMAAAPqAAAH0AAAABZTZWFzb25MZWFkZXJib2FyZEVudHJ5AAA=",
        "AAAAAwAAAOBFbGlnaWJsZSBmYWlsdXJlIGV2ZW50cyBmb3IgaW5zdXJhbmNlIGNvdmVyYWdlIChJc3N1ZSAjMzY3KS4KCkVhY2ggdmFyaWFudCBtYXBzIHRvIGEgY2FuY2VsLXJvdW5kIHJlYXNvbiBjb2RlIHVzZWQgYnkgdGhlCmluc3VyYW5jZSBjb3ZlcmFnZSBwYXlvdXQgZ2F0ZS4gT25seSBldmVudHMgbGlzdGVkIGluIHRoZQphZG1pbi1jb25maWd1cmVkIHdoaXRlbGlzdCB0cmlnZ2VyIGNvdmVyYWdlLgAAAAAAAAAOSW5zdXJhbmNlRXZlbnQAAAAAAAMAAAAwQ2FuY2VsIGR1ZSB0byBvcmFjbGUgaGVhcnRiZWF0IGZhaWx1cmUgLyBvdXRhZ2UuAAAADE9yYWNsZU91dGFnZQAAAAAAAABCQ2FuY2VsIGR1ZSB0byBvcmFjbGUgZGV2aWF0aW9uIGV4Y2VlZGluZyB0aGUgY29uZmlndXJlZCB0aHJlc2hvbGQuAAAAAAAPT3JhY2xlRGV2aWF0aW9uAAAAAAEAAABARmFsbGJhY2sgcmVmdW5kIHdoZW4gaW5zdWZmaWNpZW50IHBhcnRpY2lwYW50cyBqb2luZWQgdGhlIHJvdW5kLgAAAA5GYWxsYmFja1JlZnVuZAAAAAAAAg==",
        "AAAAAQAAA/xPbmUtcmVhZCBjb21wb3NpdGUgdmlldyBvZiBjdXJyZW50IG1hcmtldCBzdGF0ZSBmb3IgZnJvbnRlbmRzOiByb3VuZApwaGFzZSwgcG9vbCBjb21wb3NpdGlvbiwgbGVkZ2VyIHRpbWluZyBidWZmZXJzLCBhbmQgZmVlIGNvbmZpZ3VyYXRpb24g4oCUCnJlcGxhY2luZyBzZXZlcmFsIHNlcGFyYXRlIGNhbGxzIHRoYXQgY291bGQgb3RoZXJ3aXNlIG9ic2VydmUKaW5jb25zaXN0ZW50IHN0YXRlIGlmIHRoZSBsZWRnZXIgYWR2YW5jZXMgYmV0d2VlbiB0aGVtIChJc3N1ZSAjMjgwKS4KCiMgRW1wdHktcm91bmQgc2VtYW50aWNzCgpXaGVuIHRoZXJlIGlzIG5vIGFjdGl2ZSByb3VuZCwgYHBoYXNlYCBhbmQgYHBvb2xfc3RhdHNgIGFyZSBib3RoIGBOb25lYC4KVGhlIHRpbWluZy1idWZmZXIgYW5kIGZlZSBmaWVsZHMgYXJlIGFsd2F5cyBwb3B1bGF0ZWQgcmVnYXJkbGVzcyDigJQgdGhleQpyZWZsZWN0IGNvbnRyYWN0LXdpZGUgY29uZmlndXJhdGlvbiwgbm90IHJvdW5kIHN0YXRlLCBzbyB0aGV5IGhhdmUgYQp3ZWxsLWRlZmluZWQgdmFsdWUgd2hldGhlciBvciBub3QgYSByb3VuZCBpcyBhY3RpdmUuCgojIENvbnNpc3RlbmN5IHdpdGggaW5kaXZpZHVhbCBnZXR0ZXJzCgpgcGhhc2VgIGFuZCBgcG9vbF9zdGF0c2AgYXJlIHRoZSBleGFjdCwgdW5tb2RpZmllZCByZXN1bHRzIG9mCmBnZXRfcm91bmRfcGhhc2VgL2BnZXRfcm91bmRfcG9vbF9zdGF0c2AgKG5ldmVyIHJlY29tcHV0ZWQpLCBhbmQgdGhlCmJ1ZmZlci9mZWUgZmllbGRzIGFyZSByZWFkIHZpYSB0aGUgc2FtZSBwdWJsaWMgZ2V0dGVycwooYGdldF9iZXRfd2luZG93X2xlZGdlcnNgLCBgZ2V0X3J1bl93aW5kb3dfbGVkZ2Vyc2AsCmBnZXRfY2xvc2VfYnVmZmVyX2xlZGdlcnNgLCBgZ2V0X3Byb3RvY29sX2ZlZV9icHNgLCBgZ2V0X2ZlZV9tb2RlbGApIHRoYXQKY2FsbGVycyBjb3VsZCBvdGhlcndpc2UgY2FsbCBpbmRpdmlkdWFsbHkg4oCUIHNvIGEgc25hcHNob3QgY2FuIG5ldmVyCmRpc2FncmVlIHdpdGggdGhvc2UgZ2V0dGVycy4AAAAAAAAADk1hcmtldFNuYXBzaG90AAAAAAAHAAAARU51bWJlciBvZiBsZWRnZXJzIHRoZSBiZXR0aW5nIHdpbmRvdyBzdGF5cyBvcGVuIGFmdGVyIHJvdW5kIGNyZWF0aW9uLgAAAAAAABJiZXRfd2luZG93X2xlZGdlcnMAAAAAAAQAAABxRXh0cmEgbGVkZ2VycyBhcHBlbmRlZCBhZnRlciB0aGUgYmV0dGluZyB3aW5kb3cgY2xvc2VzLCBiZWZvcmUgdGhlCnJvdW5kIHRyYW5zaXRpb25zIHRvIGBSdW5uaW5nYCAoMCA9IGRpc2FibGVkKS4AAAAAAAAUY2xvc2VfYnVmZmVyX2xlZGdlcnMAAAAEAAAAP0NvbmZpZ3VyZWQgZmVlIGluY2lkZW5jZSBtb2RlbCAoYEZlZU9uUG90YCBvciBgRmVlT25XaW5uaW5nc2ApLgAAAAAJZmVlX21vZGVsAAAAAAAH0AAAAAhGZWVNb2RlbAAAAShDdXJyZW50IHJvdW5kJ3MgbGlmZWN5Y2xlIHBoYXNlLCBvciBlbXB0eSBpZiBubyByb3VuZCBpcyBhY3RpdmUuCgpNb2RlbGVkIGFzIGEgMC1vci0xLWVsZW1lbnQgYFZlY2AgcmF0aGVyIHRoYW4gYE9wdGlvbjxSb3VuZFBoYXNlPmA6CnRoaXMgc29yb2Jhbi1zZGsgdmVyc2lvbidzIGAjW2NvbnRyYWN0dHlwZV1gIGRlcml2ZSBkb2VzIG5vdCBnZW5lcmF0ZQphbiBYRFIgKGBTY1ZhbGApIGNvbnZlcnNpb24gZm9yIGBPcHRpb248VD5gIHdyYXBwaW5nIGEgdXNlci1kZWZpbmVkCnR5cGUsIG9ubHkgZm9yIGBWZWM8VD5gLgAAAAVwaGFzZQAAAAAAA+oAAAfQAAAAClJvdW5kUGhhc2UAAAAAAI5GdWxsIHBvb2wtY29tcG9zaXRpb24gYnJlYWtkb3duIGZvciB0aGUgYWN0aXZlIHJvdW5kLCBvciBlbXB0eSBpZiBubwpyb3VuZCBpcyBhY3RpdmUuIFNlZSBgcGhhc2VgIGZvciB3aHkgdGhpcyBpcyBhIGBWZWNgIGFuZCBub3QgYW4KYE9wdGlvbmAuAAAAAAAKcG9vbF9zdGF0cwAAAAAD6gAAB9AAAAAOUm91bmRQb29sU3RhdHMAAAAAAEhDb25maWd1cmVkIHByb3RvY29sIGZlZSBpbiBiYXNpcyBwb2ludHMsIG9yIGBOb25lYCBpZiBmZWVzIGFyZSBkaXNhYmxlZC4AAAAQcHJvdG9jb2xfZmVlX2JwcwAAA+gAAAAEAAAAS051bWJlciBvZiBsZWRnZXJzIGFmdGVyIHJvdW5kIGNyZWF0aW9uIGJlZm9yZSB0aGUgcm91bmQgYmVjb21lcwpyZXNvbHZhYmxlLgAAAAAScnVuX3dpbmRvd19sZWRnZXJzAAAAAAAE",
        "AAAAAwAAAMlPbmUtc2lkZWQgKGRlZ2VuZXJhdGUpIG1hcmtldCBzZXR0bGVtZW50IHBvbGljeSAoSXNzdWUgIzI3MCAvICMzOTApLgpXaGVuIGV4YWN0bHkgb25lIG9mIHBvb2xfdXAvcG9vbF9kb3duIGlzIGVtcHR5LCByZWZ1bmQgYWxsIHN0YWtlcyBvbiB0aGUKcG9wdWxhdGVkIHNpZGUgKGRlZmF1bHQgcG9saWN5IGZvciBvbmUtc2lkZWQgVXBEb3duIHBvb2xzKS4AAAAAAAAAAAAADk9uZVNpZGVkUG9saWN5AAAAAAADAAAAAAAAAAZSZWZ1bmQAAAAAAAAAAAAAAAAABFZvaWQAAAABAAAAAAAAAAxDYXJyeUZvcndhcmQAAAAC",
        "AAAAAwAAAOtQYXJhbWV0ZXIgY2xhc3NpZmljYXRpb24gZm9yIHRoZSBvbi1jaGFpbiBjb25zdGl0dXRpb24gKElzc3VlICMzNjMpLgpJbW11dGFibGUgcGFyYW1ldGVycyBjYW5ub3QgYmUgY2hhbmdlZDsgdGltZWxvY2tlZCBwYXJhbWV0ZXJzIHJlcXVpcmUKYSB0aW1lbG9jayBiZWZvcmUgYWN0aXZhdGlvbjsgZHVhbC1hcHByb3ZhbCBwYXJhbWV0ZXJzIHJlcXVpcmUgYm90aAphZG1pbiBhbmQgYXBwcm92ZXIgc2lnbi1vZmYuAAAAAAAAAAAOUGFyYW1ldGVyQ2xhc3MAAAAAAAQAAAAnQ2Fubm90IGJlIG1vZGlmaWVkIGFmdGVyIGluaXRpYWxpemF0aW9uAAAAAAlJbW11dGFibGUAAAAAAAAAAAAAKlJlcXVpcmVzIHRpbWVsb2NrIHBlcmlvZCBiZWZvcmUgYWN0aXZhdGlvbgAAAAAAClRpbWVsb2NrZWQAAAAAAAEAAAApUmVxdWlyZXMgYm90aCBhZG1pbiBhbmQgYXBwcm92ZXIgYXBwcm92YWwAAAAAAAAMRHVhbEFwcHJvdmFsAAAAAgAAAC5NYXkgYmUgY2hhbmdlZCBpbW1lZGlhdGVseSAobGVhc3QgcmVzdHJpY3RpdmUpAAAAAAAGTm9ybWFsAAAAAAAD",
        "AAAAAwAABABHbG9iYWwgc3RhdHVzIG9mIHRoZSBwcm90b2NvbCwgcmV0dXJuZWQgYnkgYGdldF9wcm90b2NvbF9zdGF0dXNgLgoKRGVzaWduZWQgZm9yIGZyb250ZW5kIHN0YXRlIG1hY2hpbmVzIHRoYXQgbmVlZCBhIHNpbmdsZSwgc3RhYmxlIGNvZGUKaW5zdGVhZCBvZiBjb21iaW5pbmcgbXVsdGlwbGUgYm9vbGVhbiBmbGFncy4KCiMjIFN0YXR1cyBjb2RlcwoKfCB2YWx1ZSB8IHZhcmlhbnQgICAgICB8IGRlc2NyaXB0aW9uICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgIHwKfC0tLS0tLS18LS0tLS0tLS0tLS0tLS18LS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLXwKfCAwICAgICB8IGBBY3RpdmVgICAgICB8IGBSdW50aW1lTW9kZTo6Tm9ybWFsYCBhbmQgYSByb3VuZCBpcyBhY3RpdmU7IHJvdW5kIG11dGF0aW9ucyBhY2NlcHRlZC4gICB8CnwgMSAgICAgfCBgUGF1c2VkYCAgICAgfCBgUnVudGltZU1vZGU6OkZ1bGx5UGF1c2VkYDsgZXZlcnkgbXV0YXRpb24gKGluY2x1ZGluZyBjbGFpbXMpIGJsb2NrZWQuICAgfAp8IDIgICAgIHwgYENsYWltc09ubHlgIHwgYFJ1bnRpbWVNb2RlOjpDbGFpbXNPbmx5YCwgb3IgYE5vcm1hbGAgd2l0aCBubyBhY3RpdmUgcm91bmQ7IGNsYWltcyBhbmQgc2V0dGxlbWVudCBhbGxvd2VkLiB8CgojIyBUcmFuc2l0aW9uIHJ1bGVzCgotIGBDbGFpbXNPbmx5YCDihpIgYEFjdGl2ZWAgd2hlbiBgY3JlYXRlX3JvdW5kKClgIHN1Y2NlZWRzIGluIGBOb3JtYWxgIG1vZGUuCi0gYEFjdGl2ZWAg4oaSIGBDbGFpbXNPbmx5YCB3aGVuIGByZXNvbHZlX3JvdW5kKClgIG9yIGBjYW5jZWxfcm91bmQoKWAgY29tcGxldGVzLApvciB3aGVuIGBzZXRfcnVudGltZV9tb2RlKDEpYCBpcyBjYWxsZWQuCi0gQW55IHN0YXRlIOKGkiBgUGF1c2VkYCB3aGVuIGBwYXVzZV9jb250cmFjdCgpYCAvIGBzZXRfcnVudGltZV9tb2RlKDIpYCBpcyBjYWxsZWQuCi0gAAAAAAAAAA5Qcm90b2NvbFN0YXR1cwAAAAAAAwAAADZgUnVudGltZU1vZGU6Ok5vcm1hbGAgYW5kIGEgcm91bmQgaXMgY3VycmVudGx5IGFjdGl2ZS4AAAAAAAZBY3RpdmUAAAAAAAAAAABIYFJ1bnRpbWVNb2RlOjpGdWxseVBhdXNlZGA6IGV2ZXJ5IG11dGF0aW9uIGlzIGJsb2NrZWQsIGluY2x1ZGluZyBjbGFpbXMuAAAABlBhdXNlZAAAAAAAAQAAAJ5gUnVudGltZU1vZGU6OkNsYWltc09ubHlgLCBvciBgTm9ybWFsYCB3aXRoIG5vIGFjdGl2ZSByb3VuZC4KUm91bmQgbXV0YXRpb25zIGFyZSBibG9ja2VkIChvciBoYXZlIG5vIHJvdW5kIHRvIGFjdCBvbik7IGNsYWltcyBhbmQKc2V0dGxlbWVudCByZW1haW4gYXZhaWxhYmxlLgAAAAAACkNsYWltc09ubHkAAAAAAAI=",
        "AAAAAQAAAVBBZ2dyZWdhdGVkIGFjdGl2ZS1yb3VuZCBwb29sIGNvbXBvc2l0aW9uIGZvciBmcm9udGVuZCB0cmFuc3BhcmVuY3kuCgpVcC9Eb3duIHJvdW5kcyBwb3B1bGF0ZSB0aGUgdXAvZG93biBwb29scywgY291bnRzLCBhbmQgc3Rha2UgcmF0aW9zLgpQcmVjaXNpb24gcm91bmRzIHBvcHVsYXRlIHRoZSBwcmVjaXNpb24gdG90YWxzIGFuZCBwYXJ0aWNpcGFudCBjb3VudGVycyB3aGlsZQpsZWF2aW5nIHNpZGUtc3BlY2lmaWMgVXAvRG93biBmaWVsZHMgYXQgemVyby4gUmF0aW9zIGFyZSBiYXNpcyBwb2ludHMgb2YKdGhlIG1vZGUncyB0b3RhbCB2aXNpYmxlIHN0YWtlICgxMF8wMDAgPSAxMDAlKS4AAAAAAAAADlJvdW5kUG9vbFN0YXRzAAAAAAANAAAAAAAAABZkb3duX3BhcnRpY2lwYW50X2NvdW50AAAAAAAEAAAAAAAAABRkb3duX3N0YWtlX3JhdGlvX2JwcwAAAAQAAAAAAAAABG1vZGUAAAfQAAAACVJvdW5kTW9kZQAAAAAAAAAAAAAacHJlY2lzaW9uX2NvbW1pdG1lbnRfY291bnQAAAAAAAQAAAAAAAAAG3ByZWNpc2lvbl9wYXJ0aWNpcGFudF9jb3VudAAAAAAEAAAAAAAAABpwcmVjaXNpb25fcHJlZGljdGlvbl9jb3VudAAAAAAABAAAAAAAAAAYcHJlY2lzaW9uX3JldmVhbGVkX2NvdW50AAAABAAAAAAAAAAVcHJlY2lzaW9uX3RvdGFsX3N0YWtlAAAAAAAACwAAAAAAAAAIcm91bmRfaWQAAAAGAAAAAAAAABB0b3RhbF9kb3duX3N0YWtlAAAACwAAAAAAAAAOdG90YWxfdXBfc3Rha2UAAAAAAAsAAAAAAAAAFHVwX3BhcnRpY2lwYW50X2NvdW50AAAABAAAAAAAAAASdXBfc3Rha2VfcmF0aW9fYnBzAAAAAAAE",
        "AAAAAgAAAAAAAAAAAAAADlR3YXBTYW1wbGVzS2V5AAAAAAABAAAAAAAAAAAAAAAHU2FtcGxlcwA=",
        "AAAAAwAAAEZBbWVuZG1lbnQgcHJvcG9zYWwgbGlmZWN5Y2xlIHN0YXR1cyBmb3IgdGhlIGNvbnN0aXR1dGlvbiAoSXNzdWUgIzM2MykuAAAAAAAAAAAAD0FtZW5kbWVudFN0YXR1cwAAAAAFAAAAPFByb3Bvc2FsIHN1Ym1pdHRlZCwgYXdhaXRpbmcgdmV0byB3aW5kb3cgZXhwaXJ5IG9yIGFwcHJvdmFscwAAAAdQZW5kaW5nAAAAAAAAAAAuVmV0byBoYXMgYmVlbiBleGVyY2lzZWQsIHByb3Bvc2FsIGlzIGNhbmNlbGxlZAAAAAAABlZldG9lZAAAAAAAAQAAADFUaW1lbG9jayBwZXJpb2QgaGFzIGVsYXBzZWQsIHJlYWR5IGZvciBhY3RpdmF0aW9uAAAAAAAAD0FjdGl2YXRpb25SZWFkeQAAAAACAAAAMkFtZW5kbWVudCBoYXMgYmVlbiBhY3RpdmF0ZWQgYW5kIHBhcmFtZXRlciBjaGFuZ2VkAAAAAAAJQWN0aXZhdGVkAAAAAAAAAwAAACNBbWVuZG1lbnQgZXhwaXJlZCBiZWZvcmUgYWN0aXZhdGlvbgAAAAAHRXhwaXJlZAAAAAAE",
        "AAAAAQAAAAAAAAAAAAAAD0RldmlhdGlvbkNvbmZpZwAAAAACAAAAAAAAAA5yZWZlcmVuY2VfbW9kZQAAAAAH0AAAABZEZXZpYXRpb25SZWZlcmVuY2VNb2RlAAAAAAAAAAAADndpbmRvd19zYW1wbGVzAAAAAAAE",
        "AAAAAQAAAGZTZXR0bGVtZW50IGRhdGEgc3RvcmVkIGR1cmluZyBkaXNwdXRlLXdpbmRvdyByZXNvbHZlIGFuZCBjb25zdW1lZCBieQpgZmluYWxpemVfcm91bmRgIG9yIGB2b2lkX3JvdW5kYC4AAAAAAAAAAAAPUm91bmRTZXR0bGVtZW50AAAAAAgAAAAAAAAACmZlZV9hbW91bnQAAAAAAAsAAAAAAAAAC2ZpbmFsX3ByaWNlAAAAAAoAAAAAAAAABG1vZGUAAAAEAAAAAAAAAAxwYXJ0aWNpcGFudHMAAAPqAAAH0AAAABNSZXNvbHZlZFBhcnRpY2lwYW50AAAAAAAAAAAJcG9vbF9kb3duAAAAAAAACwAAAAAAAAAHcG9vbF91cAAAAAALAAAAAAAAAAtwcmljZV9zdGFydAAAAAAKAAAAAAAAAAhyb3VuZF9pZAAAAAY=",
        "AAAAAwAAAKxUZXJtaW5hbCBvdXRjb21lIHBlcnNpc3RlZCBwZXIgdXNlciBwZXIgYXJjaGl2ZWQgcm91bmQuCgpBbGxvd3MgYGdldF91c2VyX2FyY2hpdmVkX3BhcnRpY2lwYXRpb25gIHRvIGFuc3dlciBwcm9maWxlL2hpc3RvcnkKcXVlcmllcyB3aXRob3V0IHJlcGxheWluZyB0aGUgZnVsbCBldmVudCBzdHJlYW0uAAAAAAAAAA9Vc2VyT3V0Y29tZVR5cGUAAAAABQAAAAAAAAADV2luAAAAAAAAAAAAAAAABExvc3MAAAABAAAAAAAAAAZSZWZ1bmQAAAAAAAIAAAAAAAAABkNhbmNlbAAAAAAAAwAAAAAAAAAEVm9pZAAAAAQ=",
        "AAAAAwAAAEhJZGVudGlmaWVzIHdoaWNoIGNyaXRpY2FsIHJpc2sgc2V0dGluZyBpcyBwZW5kaW5nIHRpbWVsb2NrZWQgYWN0aXZhdGlvbi4AAAAAAAAAEENvbmZpZ0NoYW5nZUtpbmQAAAAUAAAAAAAAAAdXaW5kb3dzAAAAAAAAAAAAAAAACE1heFN0YWtlAAAAAQAAAAAAAAAUTWF4VXNlclJvdW5kRXhwb3N1cmUAAAACAAAAAAAAABJNYXhQZW5kaW5nV2lubmluZ3MAAAAAAAMAAAAAAAAAFE9yYWNsZVN0YWxlVGhyZXNob2xkAAAABAAAAAAAAAAVT3JhY2xlTWF4RGV2aWF0aW9uQnBzAAAAAAAABQAAAAAAAAAOUHJvdG9jb2xGZWVCcHMAAAAAAAYAAAAAAAAAD01pblBhcnRpY2lwYW50cwAAAAAHAAAAAAAAABhNYXhQcmVjaXNpb25QYXJ0aWNpcGFudHMAAAAIAAAAAAAAAAlNaW50TGltaXQAAAAAAAAJAAAAAAAAABBBcmNoaXZlUmV0ZW50aW9uAAAACgAAAAAAAAASQ2xvc2VCdWZmZXJMZWRnZXJzAAAAAAALAAAAAAAAABNPcmFjbGVUaW1lc3RhbXBTa2V3AAAAAAwAAAAAAAAAD0Vwb2NoTWludEJ1ZGdldAAAAAANAAAAAAAAABVQZW5kaW5nV2lubmluZ3NFeHBpcnkAAAAAAAAOAAAAAAAAABVQcmVjaXNpb25QYXlvdXRQb2xpY3kAAAAAAAAPAAAAAAAAAAZNaW5CZXQAAAAAABAAAAAAAAAADkRpc3B1dGVMZWRnZXJzAAAAAAARAAAAAAAAAAhGZWVNb2RlbAAAABIAAAAAAAAAD0Vhcmx5Q2FzaG91dEJwcwAAAAAT",
        "AAAAAQAAADZBIHNpbmdsZSBlbnRyeSBpbiB0aGUgbGlmZXRpbWUgKGFsbC10aW1lKSBsZWFkZXJib2FyZC4AAAAAAAAAAAAQTGVhZGVyYm9hcmRFbnRyeQAAAAIAAAAAAAAABXN0YXRzAAAAAAAH0AAAAAlVc2VyU3RhdHMAAAAAAAAAAAAABHVzZXIAAAAT",
        "AAAAAQAAACVNdWx0aS1mZWVkIG9yYWNsZSByZXNvbHV0aW9uIHBheWxvYWQuAAAAAAAAAAAAABBNdWx0aUZlZWRQYXlsb2FkAAAABwAAAAAAAAANY29udHJhY3RfYWRkcgAAAAAAABMAAAAAAAAACm5ldHdvcmtfaWQAAAAAA+4AAAAgAAAAAAAAAAVub25jZQAAAAAAAAYAAAAAAAAABnByaWNlcwAAAAAD6gAAAAoAAAAAAAAACHJvdW5kX2lkAAAABAAAAAAAAAAHc291cmNlcwAAAAPqAAAABAAAAAAAAAAJdGltZXN0YW1wAAAAAAAABg==",
        "AAAAAQAAAEBTaW11bGF0ZWQgcGF5b3V0IHJlc3VsdCBmb3IgYSBzcGVjaWZpYyBoeXBvdGhldGljYWwgZmluYWwgcHJpY2UuAAAAAAAAABBTaW11bGF0aW9uUmVzdWx0AAAABwAAAAAAAAAKZmVlX2Ftb3VudAAAAAAACwAAAAAAAAAJZmVlX21vZGVsAAAAAAAABAAAAAAAAAAEbW9kZQAAB9AAAAAJUm91bmRNb2RlAAAAAAAAAAAAAAhvdXRjb21lcwAAA+oAAAfQAAAAEFVzZXJSb3VuZE91dGNvbWUAAAAAAAAACXBvb2xfZG93bgAAAAAAAAsAAAAAAAAAB3Bvb2xfdXAAAAAACwAAAAAAAAAVcHJlY2lzaW9uX3RvdGFsX3N0YWtlAAAAAAAACw==",
        "AAAAAQAAAAAAAAAAAAAAEFVzZXJSb3VuZE91dGNvbWUAAAAHAAAAAAAAAAdvdXRjb21lAAAAB9AAAAAPVXNlck91dGNvbWVUeXBlAAAAAAAAAAAGcGF5b3V0AAAAAAALAAAAAAAAAA9wcmVkaWN0ZWRfcHJpY2UAAAAACgAAAAAAAAAPcHJlZGljdGlvbl9zaWRlAAAAAAQAAAAAAAAACnJvdW5kX21vZGUAAAAAAAQAAAAAAAAABXN0YWtlAAAAAAAACwAAAAAAAAAEdXNlcgAAABM=",
        "AAAAAQAAAAAAAAAAAAAAEUF0dGVzdGF0aW9uQ29uZmlnAAAAAAAAAQAAAAAAAAADa2V5AAAAA+gAAAPuAAAAIA==",
        "AAAAAwAAADJHb3Zlcm5hbmNlIHByb3Bvc2FsIGxpZmVjeWNsZSBzdGF0dXMgKElzc3VlICMyNzIpLgAAAAAAAAAAABFHb3ZQcm9wb3NhbFN0YXR1cwAAAAAAAAUAAAAAAAAAB1BlbmRpbmcAAAAAAAAAAAAAAAAIQXBwcm92ZWQAAAABAAAAAAAAAAhFeGVjdXRlZAAAAAIAAAAAAAAACUNhbmNlbGxlZAAAAAAAAAMAAAAAAAAAB0V4cGlyZWQAAAAABA==",
        "AAAAAgAAAAAAAAAAAAAAEkRldmlhdGlvbkNvbmZpZ0tleQAAAAAAAQAAAAAAAAAAAAAABkNvbmZpZwAA",
        "AAAAAQAAAAAAAAAAAAAAEk9yYWNsZVF1b3J1bUNvbmZpZwAAAAAAAwAAAAAAAAAQbWluX29ic2VydmF0aW9ucwAAAAQAAAAAAAAAFW91dGxpZXJfdGhyZXNob2xkX2JwcwAAAAAAAAQAAAAAAAAAEHF1b3J1bV90aHJlc2hvbGQAAAAE",
        "AAAAAwAAAD9UZXJtaW5hbCBvdXRjb21lIHJlY29yZGVkIHdoZW4gYSByb3VuZCBsZWF2ZXMgdGhlIGFjdGl2ZSBzdGF0ZS4AAAAAAAAAABJSb3VuZEFyY2hpdmVTdGF0dXMAAAAAAAQAAAA1T3JhY2xlIHNldHRsZW1lbnQgY29tcGxldGVkIChub3JtYWwgcmVzb2x1dGlvbiBwYXRoKS4AAAAAAAAIUmVzb2x2ZWQAAAAAAAAANEFkbWluIGNhbmNlbGxlZCB0aGUgcm91bmQgYW5kIHJlZnVuZGVkIHBhcnRpY2lwYW50cy4AAAAJQ2FuY2VsbGVkAAAAAAAAAQAAAEVTZXR0bGVtZW50IGFib3J0ZWQgZHVlIHRvIGluc3VmZmljaWVudCBwYXJ0aWNpcGFudHM7IHN0YWtlcyByZWZ1bmRlZC4AAAAAAAAORmFsbGJhY2tSZWZ1bmQAAAAAAAIAAABFRGlzcHV0ZSB3aW5kb3cgZW5kZWQgdmlhIHZvaWQ7IGFsbCBwYXJ0aWNpcGFudHMgcmVmdW5kZWQgdGhlaXIgc3Rha2UuAAAAAAAABlZvaWRlZAAAAAAAAw==",
        "AAAAAgAAAC9QYXlsb2FkIGZvciBhIHNjaGVkdWxlZCBjcml0aWNhbCBjb25maWcgY2hhbmdlLgAAAAAAAAAAE0NvbmZpZ0NoYW5nZVBheWxvYWQAAAAAFAAAAAEAAAAAAAAAB1dpbmRvd3MAAAAAAgAAAAQAAAAEAAAAAQAAAAAAAAAITWF4U3Rha2UAAAABAAAD6AAAAAsAAAABAAAAAAAAABRNYXhVc2VyUm91bmRFeHBvc3VyZQAAAAEAAAPoAAAACwAAAAEAAAAAAAAAEk1heFBlbmRpbmdXaW5uaW5ncwAAAAAAAQAAA+gAAAALAAAAAQAAAAAAAAAUT3JhY2xlU3RhbGVUaHJlc2hvbGQAAAABAAAABgAAAAEAAAAAAAAAFU9yYWNsZU1heERldmlhdGlvbkJwcwAAAAAAAAEAAAPoAAAABAAAAAEAAAAAAAAADlByb3RvY29sRmVlQnBzAAAAAAABAAAD6AAAAAQAAAABAAAAAAAAAA9NaW5QYXJ0aWNpcGFudHMAAAAAAQAAA+gAAAAEAAAAAQAAAAAAAAAYTWF4UHJlY2lzaW9uUGFydGljaXBhbnRzAAAAAQAAAAQAAAABAAAAAAAAAAlNaW50TGltaXQAAAAAAAABAAAABAAAAAEAAAAAAAAAEEFyY2hpdmVSZXRlbnRpb24AAAABAAAABAAAAAEAAAAAAAAAEkNsb3NlQnVmZmVyTGVkZ2VycwAAAAAAAQAAAAQAAAABAAAAAAAAABNPcmFjbGVUaW1lc3RhbXBTa2V3AAAAAAEAAAAGAAAAAQAAAAAAAAAPRXBvY2hNaW50QnVkZ2V0AAAAAAEAAAALAAAAAQAAAAAAAAAVUGVuZGluZ1dpbm5pbmdzRXhwaXJ5AAAAAAAAAQAAAAQAAAABAAAAAAAAABVQcmVjaXNpb25QYXlvdXRQb2xpY3kAAAAAAAABAAAABAAAAAEAAAAAAAAABk1pbkJldAAAAAAAAQAAA+gAAAALAAAAAQAAAAAAAAAORGlzcHV0ZUxlZGdlcnMAAAAAAAEAAAAEAAAAAQAAAAAAAAAIRmVlTW9kZWwAAAABAAAH0AAAAAhGZWVNb2RlbAAAAAEAAAAAAAAAD0Vhcmx5Q2FzaG91dEJwcwAAAAABAAAD6AAAAAQ=",
        "AAAAAQAAAFNQZW5kaW5nIHRpbWVsb2NrZWQgY29uZmlnIGNoYW5nZSB3aXRoIGFjdGl2YXRpb24gbGVkZ2VyIGZvciBvbi1jaGFpbiBvYnNlcnZhYmlsaXR5LgAAAAAAAAAAE1BlbmRpbmdDb25maWdDaGFuZ2UAAAAAAwAAAAAAAAARYWN0aXZhdGlvbl9sZWRnZXIAAAAAAAAEAAAAAAAAAAdwYXlsb2FkAAAAB9AAAAATQ29uZmlnQ2hhbmdlUGF5bG9hZAAAAAAAAAAAE3NjaGVkdWxlZF9hdF9sZWRnZXIAAAAABA==",
        "AAAAAQAAAAAAAAAAAAAAE1ByZWNpc2lvbkNvbW1pdG1lbnQAAAAAAwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAAAAAARoYXNoAAAD7gAAACAAAAAAAAAACHJldmVhbGVkAAAAAQ==",
        "AAAAAQAAADtQcmVjaXNpb24gcHJlZGljdGlvbiBlbnRyeSAodXNlciBhZGRyZXNzICsgcHJlZGljdGVkIHByaWNlKQAAAAAAAAAAE1ByZWNpc2lvblByZWRpY3Rpb24AAAAAAwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAAAAAA9wcmVkaWN0ZWRfcHJpY2UAAAAACgAAAAAAAAAEdXNlcgAAABM=",
        "AAAAAQAAAEBQZXItcGFydGljaXBhbnQgb3V0Y29tZSBzdG9yZWQgZHVyaW5nIGRpc3B1dGUtd2luZG93IHNldHRsZW1lbnQuAAAAAAAAABNSZXNvbHZlZFBhcnRpY2lwYW50AAAAAAMAAAAAAAAAB291dGNvbWUAAAAH0AAAAA9Vc2VyT3V0Y29tZVR5cGUAAAAAAAAAAAZwYXlvdXQAAAAAAAsAAAAAAAAABHVzZXIAAAAT",
        "AAAAAQAAANFDb21wYWN0IGhpc3RvcmljYWwgcm91bmQgc3VtbWFyeSBwZXJzaXN0ZWQgYWZ0ZXIgcmVzb2x2ZSBvciBjYW5jZWwuCgpEZXNpZ25lZCBmb3IgZXhwbG9yZXIvYW5hbHl0aWNzIHF1ZXJpZXMgd2l0aG91dCByZXBsYXlpbmcgZXZlbnRzLgpgcHJpY2VfZmluYWxgIGlzIGAwYCBmb3IgYWRtaW4gY2FuY2VsbGF0aW9ucyAobm8gb3JhY2xlIHNldHRsZW1lbnQgcHJpY2UpLgAAAAAAAAAAAAAUQXJjaGl2ZWRSb3VuZFN1bW1hcnkAAAAJAAAAAAAAAARtb2RlAAAH0AAAAAlSb3VuZE1vZGUAAAAAAAAAAAAAEXBhcnRpY2lwYW50X2NvdW50AAAAAAAABAAAAAAAAAAJcG9vbF9kb3duAAAAAAAACwAAAAAAAAAHcG9vbF91cAAAAAALAAAAAAAAAAtwcmljZV9maW5hbAAAAAAKAAAAAAAAAAtwcmljZV9zdGFydAAAAAAKAAAAAAAAAAhyb3VuZF9pZAAAAAYAAAAAAAAAEXNldHRsZWRfYXRfbGVkZ2VyAAAAAAAABAAAAAAAAAAGc3RhdHVzAAAAAAfQAAAAElJvdW5kQXJjaGl2ZVN0YXR1cwAA",
        "AAAAAgAAAAAAAAAAAAAAFEF0dGVzdGF0aW9uQ29uZmlnS2V5AAAAAQAAAAAAAAAAAAAABkNvbmZpZwAA",
        "AAAAAQAAAMNPbi1jaGFpbiBjb25zdGl0dXRpb24gZGVmaW5pbmcgcGFyYW1ldGVyIGdvdmVybmFuY2UgcnVsZXMgKElzc3VlICMzNjMpLgpDbGFzc2lmaWVzIGVhY2ggcHJvdG9jb2wgcGFyYW1ldGVyIGFuZCBkZWZpbmVzIHRoZSBhbWVuZG1lbnQgbGlmZWN5Y2xlCih2ZXRvIHdpbmRvdywgdGltZWxvY2ssIGR1YWwgYXBwcm92YWwgcmVxdWlyZW1lbnRzKS4AAAAAAAAAABRDb25zdGl0dXRpb25NZXRhZGF0YQAAAAQAAABDV2hldGhlciBkdWFsLWFwcHJvdmFsIChhZG1pbiArIGFwcHJvdmVyKSBpcyByZXF1aXJlZCBmb3IgYW1lbmRtZW50cwAAAAAWZHVhbF9hcHByb3ZhbF9yZXF1aXJlZAAAAAAAAQAAADBMZWRnZXIgYXQgd2hpY2ggdGhlIGNvbnN0aXR1dGlvbiB3YXMgZXN0YWJsaXNoZWQAAAAVZXN0YWJsaXNoZWRfYXRfbGVkZ2VyAAAAAAAABAAAADtUaW1lbG9jayBkdXJhdGlvbiBpbiBsZWRnZXJzIGJlZm9yZSBhbWVuZG1lbnRzIGNhbiBhY3RpdmF0ZQAAAAAQdGltZWxvY2tfbGVkZ2VycwAAAAQAAAA0VmV0byB3aW5kb3cgZHVyYXRpb24gaW4gbGVkZ2VycyAoMCA9IG5vIHZldG8gd2luZG93KQAAABN2ZXRvX3dpbmRvd19sZWRnZXJzAAAAAAQ=",
        "AAAAAQAABABDb21wb3NpdGUgcHJvdG9jb2wgaGVhbHRoIHN0YXR1cyByZXR1cm5lZCBieSBgZ2V0X3Byb3RvY29sX2hlYWx0aGAuCgpEZXNpZ25lZCBmb3Igb3BlcmF0b3JzIHRvIHBvbGwgYSBzaW5nbGUgZW5kcG9pbnQgaW5zdGVhZCBvZiBzdGl0Y2hpbmcKdG9nZXRoZXIgbXVsdGlwbGUgcmVhZC1vbmx5IGNhbGxzLgoKIyMgU3RhdHVzIGNvZGUg4oaSIGFsZXJ0IHNldmVyaXR5IG1hcHBpbmcKCnwgY29kZSB8IGxhYmVsICAgICAgICAgICB8IHNldmVyaXR5IHwgbWVhbmluZyAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgfAp8LS0tLS0tfC0tLS0tLS0tLS0tLS0tLS0tfC0tLS0tLS0tLS18LS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLXwKfCAwICAgIHwgSEVBTFRIWSAgICAgICAgIHwgbm9uZSAgICAgfCBBbGwgc3Vic3lzdGVtcyBub21pbmFsICAgICAgICAgICAgICAgICAgICB8CnwgMSAgICB8IFBBVVNFRCAgICAgICAgICB8IGNyaXRpY2FsIHwgYFJ1bnRpbWVNb2RlOjpGdWxseVBhdXNlZGAgICAgICAgICAgICAgICAgfAp8IDIgICAgfCBPUkFDTEVfU1RBTEUgICAgfCB3YXJuaW5nICB8IE9yYWNsZSBoZWFydGJlYXQgaXMgc3RhbGUgb3Igb2ZmbGluZSAgICAgIHwKfCAzICAgIHwgUk9VTkRfU1RBTEUgICAgIHwgd2FybmluZyAgfCBSb3VuZCBpcyBwYXN0IGl0cyBlbmQgbGVkZ2VyIGJ1dCB1bnJlc29sdmVkfAp8IDQgICAgfCBOT19BQ1RJVkVfUk9VTkQgfCBpbmZvICAgICB8IE5vIHJvdW5kIGN1cnJlbnRseSBhY3RpdmUgKGlkbGUgcHJvdG9jb2wpIHwKfCA1ICAgIHwgTVVMVElQTEVfSVNTVUVTIHwgY3JpdGljYWwgfCBUd28gb3IgbW9yZSBpc3N1ZXMgZGV0ZWN0ZWQgc2ltdWx0YW5lb3VzbHl8CnwgNiAgICB8IENMQUlNU19PTkxZICAgICB8IHdhcm5pbmcgIHwgYFJ1bnRpbWVNb2RlOjpDbGFpbXNPbmx5YCAgICAgICAgICAgICAgICAgfAp8IDcgICAgfCBBQ0NFU1NfUkVTVFJJQ1RFRCB8IGluZm8gICB8IEFsbG93bGlzdCBtb2RlIG9uOyBvdGhlcndpc2UgAAAAAAAAABRQcm90b2NvbEhlYWx0aFN0YXR1cwAAAAkAAABEQ3VycmVudCByb3VuZCBwaGFzZSAoMD1ub19yb3VuZCwgMT1iZXR0aW5nLCAyPXJ1bm5pbmcsIDM9cmVzb2x2YWJsZSkAAAASYWN0aXZlX3JvdW5kX3BoYXNlAAAAAAAEAAAAI1doZXRoZXIgYSByb3VuZCBpcyBjdXJyZW50bHkgYWN0aXZlAAAAABBoYXNfYWN0aXZlX3JvdW5kAAAAAQAAADdMZWRnZXIgc2VxdWVuY2UgYXQgd2hpY2ggdGhpcyBoZWFsdGggc25hcHNob3Qgd2FzIHRha2VuAAAAAA9sZWRnZXJfc2VxdWVuY2UAAAAABAAAADhMZWRnZXIgdGltZXN0YW1wIGF0IHdoaWNoIHRoaXMgaGVhbHRoIHNuYXBzaG90IHdhcyB0YWtlbgAAABBsZWRnZXJfdGltZXN0YW1wAAAABgAAADlXaGV0aGVyIHRoZSBvcmFjbGUgaGVhcnRiZWF0IGlzIG5vbi1zdGFsZSBhbmQgbm90IG9mZmxpbmUAAAAAAAALb3JhY2xlX2xpdmUAAAAAAQAAAEhSYXcgb3JhY2xlIGhlYXJ0YmVhdCBzdGF0dXMgKDA9YWN0aXZlLCAxPWRlZ3JhZGVkLCAyPW9mZmxpbmUsIDM9dW5rbm93bikAAAANb3JhY2xlX3N0YXR1cwAAAAAAAAQAAACCYHRydWVgIG9ubHkgaW4gYFJ1bnRpbWVNb2RlOjpGdWxseVBhdXNlZGAgKHNhbWUgYXMgYGlzX3BhdXNlZCgpYCk7CmBDbGFpbXNPbmx5YCBpcyByZXBvcnRlZCB2aWEgYHN0YXR1c19jb2RlID09IDZgLCBub3QgdGhpcyBmbGFnLgAAAAAABnBhdXNlZAAAAAAAAQAAAB9Pbi1jaGFpbiBzdG9yYWdlIHNjaGVtYSB2ZXJzaW9uAAAAAA5zY2hlbWFfdmVyc2lvbgAAAAAABAAAAC9Db21wb3NpdGUgc3RhdHVzIGNvZGUgKHNlZSBtYXBwaW5nIHRhYmxlIGFib3ZlKQAAAAALc3RhdHVzX2NvZGUAAAAABA==",
        "AAAAAQAAAH5PcmFjbGUgbGl2ZW5lc3MgcmVjb3JkLCB1cGRhdGVkIGJ5IHRoZSBvcmFjbGUgc2VydmljZSBvbiBlYWNoIGhlYXJ0YmVhdCBjYWxsLgpgc3RhdHVzYDogMCA9IGFjdGl2ZSwgMSA9IGRlZ3JhZGVkLCAyID0gb2ZmbGluZS4AAAAAAAAAAAAVT3JhY2xlSGVhcnRiZWF0UmVjb3JkAAAAAAAAAgAAAAAAAAAGc3RhdHVzAAAAAAAEAAAAAAAAAAl0aW1lc3RhbXAAAAAAAAAG",
        "AAAAAwAAADNQYXlvdXQgcG9saWN5IGZvciBQcmVjaXNpb24gbW9kZSAob24tY2hhaW4gY29uZmlnKS4AAAAAAAAAABVQcmVjaXNpb25QYXlvdXRQb2xpY3kAAAAAAAACAAAAAAAAAAVFcXVhbAAAAAAAAAAAAAAAAAAADVN0YWtlV2VpZ2h0ZWQAAAAAAAAB",
        "AAAAAwAAAAAAAAAAAAAAFkRldmlhdGlvblJlZmVyZW5jZU1vZGUAAAAAAAIAAAAAAAAAClN0YXJ0UHJpY2UAAAAAAAAAAAAAAAAABFR3YXAAAAAB",
        "AAAAAQAAAPpQZW5kaW5nIHR3by1zdGVwIG9yYWNsZSByb3RhdGlvbiBwcm9wb3NhbC4KClRoZSBhZG1pbiBwcm9wb3NlcyBhIG5ldyBvcmFjbGUgYWRkcmVzcyB3aXRoIGEgdGltZXN0YW1wLWJhc2VkIGV4cGlyeSB3aW5kb3cuCkFmdGVyIGBleHBpcmVzX2F0YCAobGVkZ2VyIHRpbWVzdGFtcCkgdGhlIHByb3Bvc2FsIGlzIHN0YWxlIGFuZCBhY2NlcHRhbmNlCmlzIHJlamVjdGVkIHVudGlsIHRoZSBhZG1pbiBzdWJtaXRzIGEgZnJlc2ggcHJvcG9zYWwuAAAAAAAAAAAAFk9yYWNsZVJvdGF0aW9uUHJvcG9zYWwAAAAAAAMAAAAAAAAACmV4cGlyZXNfYXQAAAAAAAYAAAAAAAAACm5ld19vcmFjbGUAAAAAABMAAAAAAAAAC3Byb3Bvc2VkX2F0AAAAAAY=",
        "AAAAAQAAAEBBIHNpbmdsZSBlbnRyeSBpbiBhIHNlYXNvbi1zY29wZWQgbGVhZGVyYm9hcmQsIGxpdmUgb3IgYXJjaGl2ZWQuAAAAAAAAABZTZWFzb25MZWFkZXJib2FyZEVudHJ5AAAAAAADAAAAAAAAAAtiZXN0X3N0cmVhawAAAAAEAAAAAAAAAAR1c2VyAAAAEwAAAAAAAAAEd2lucwAAAAQ=",
        "AAAAAQAAAAAAAAAAAAAAGFBlbmRpbmdXaW5uaW5nc0V4cGlyeUtleQAAAAEAAAAAAAAAATAAAAAAAAPtAAAAAA==",
        "AAAAAQAAAAAAAAAAAAAAG1BlbmRpbmdXaW5uaW5nc1VwZGF0ZWRBdEtleQAAAAABAAAAAAAAAAEwAAAAAAAAEw==",
        "AAAAAAAAABtSZXR1cm5zIHVzZXIncyB2WExNIGJhbGFuY2UAAAAAB2JhbGFuY2UAAAAAAQAAAAAAAAAEdXNlcgAAABMAAAABAAAACw==",
        "AAAAAAAAAAAAAAAJZ2V0X2FkbWluAAAAAAAAAAAAAAEAAAPoAAAAEw==",
        "AAAAAAAAADBSZXR1cm5zIHdoZXRoZXIgdGhlIGNvbnRyYWN0IGlzIGN1cnJlbnRseSBwYXVzZWQAAAAJaXNfcGF1c2VkAAAAAAAAAAAAAAEAAAAB",
        "AAAAAAAAAAAAAAAJcGxhY2VfYmV0AAAAAAAAAwAAAAAAAAAEdXNlcgAAABMAAAAAAAAABmFtb3VudAAAAAAACwAAAAAAAAAEc2lkZQAAB9AAAAAHQmV0U2lkZQAAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAP5DbGFpbXMgcGVuZGluZyB3aW5uaW5ncyBmb3IgdXAgdG8gYE1BWF9DTEFJTV9CQVRDSF9TSVpFYCB1c2VycyBpbiBvbmUKY2FsbC4gQWxsLW9yLW5vdGhpbmc6IGFueSBmYWlsdXJlIChiYXRjaCB0b28gbGFyZ2UsIGEgZHVwbGljYXRlCmFkZHJlc3MsIG9yIGEgbWlzc2luZyBwZXItdXNlciBhdXRoKSByZXZlcnRzIGV2ZXJ5IGVmZmVjdCBpbiB0aGlzCmNhbGwuIFNlZSBgc2V0dGxlbWVudDo6Y2xhaW1fbWFueWAgZm9yIGZ1bGwgc2VtYW50aWNzLgAAAAAACmNsYWltX21hbnkAAAAAAAEAAAAAAAAABXVzZXJzAAAAAAAD6gAAABMAAAABAAAD6QAAA+oAAAALAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAAKZ2V0X29yYWNsZQAAAAAAAAAAAAEAAAPoAAAAEw==",
        "AAAAAAAAAEhJbml0aWFsaXplcyB0aGUgY29udHJhY3Qgd2l0aCBhZG1pbiBhbmQgb3JhY2xlIGFkZHJlc3NlcyAob25lLXRpbWUgb25seSkAAAAKaW5pdGlhbGl6ZQAAAAAAAgAAAAAAAAAFYWRtaW4AAAAAAAATAAAAAAAAAAZvcmFjbGUAAAAAABMAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAHpBbnlvbmUgbWF5IGNhbGwgYHZvaWRfcm91bmRgIGR1cmluZyB0aGUgZGlzcHV0ZSB3aW5kb3cgdG8gcmVmdW5kIGFsbApwYXJ0aWNpcGFudHMgdGhlaXIgZnVsbCBzdGFrZXMgKHZvaWQtdG8tcmVmdW5kIHBhdGgpLgAAAAAACnZvaWRfcm91bmQAAAAAAAEAAAAAAAAACHJvdW5kX2lkAAAABgAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAADxSZXR1cm5zIHRoZSBjb25maWd1cmVkIG1pbmltdW0gYmV0LCBpZiBlbmFibGVkIChJc3N1ZSAjMjY5KS4AAAALZ2V0X21pbl9iZXQAAAAAAAAAAAEAAAPoAAAACw==",
        "AAAAAAAAAElTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIG1pbmltdW0tYmV0IChkdXN0IHByb3RlY3Rpb24pIHVwZGF0ZSAoSXNzdWUgIzI2OSkuAAAAAAAAC3NldF9taW5fYmV0AAAAAAEAAAAAAAAACm1pbl9hbW91bnQAAAAAA+gAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAMNTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHdpbmRvd3MgdXBkYXRlIChhbGlhcyBmb3IgW2BTZWxmOjpzY2hlZHVsZV93aW5kb3dzYF0pLgpiZXRfbGVkZ2VyczogTnVtYmVyIG9mIGxlZGdlcnMgdXNlcnMgY2FuIHBsYWNlIGJldHMKcnVuX2xlZGdlcnM6IFRvdGFsIG51bWJlciBvZiBsZWRnZXJzIGJlZm9yZSByb3VuZCBjYW4gYmUgcmVzb2x2ZWQAAAAAC3NldF93aW5kb3dzAAAAAAIAAAAAAAAAC2JldF9sZWRnZXJzAAAAAAQAAAAAAAAAC3J1bl9sZWRnZXJzAAAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAAMY2FuY2VsX3JvdW5kAAAAAQAAAAAAAAAGcmVhc29uAAAAAAAEAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAACtDcmVhdGVzIGEgbmV3IHByZWRpY3Rpb24gcm91bmQgKGFkbWluIG9ubHkpAAAAAAxjcmVhdGVfcm91bmQAAAACAAAAAAAAAAtzdGFydF9wcmljZQAAAAAKAAAAAAAAAARtb2RlAAAD6AAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAC1NaW50cyAxMDAwIHZYTE0gZm9yIG5ldyB1c2VycyAob25lLXRpbWUgb25seSkAAAAAAAAMbWludF9pbml0aWFsAAAAAQAAAAAAAAAEdXNlcgAAABMAAAABAAAACw==",
        "AAAAAAAAAC1SZXRyaWV2ZXMgYW4gYW1lbmRtZW50IHByb3Bvc2FsIHJlY29yZCBieSBJRC4AAAAAAAANZ2V0X2FtZW5kbWVudAAAAAAAAAEAAAAAAAAADGFtZW5kbWVudF9pZAAAAAYAAAABAAAD6AAAB9AAAAAJQW1lbmRtZW50AAAA",
        "AAAAAAAAAEVSZXR1cm5zIHRoZSBjb25maWd1cmVkIGZlZSBpbmNpZGVuY2UgbW9kZWwsIGRlZmF1bHRpbmcgdG8gYEZlZU9uUG90YC4AAAAAAAANZ2V0X2ZlZV9tb2RlbAAAAAAAAAAAAAABAAAH0AAAAAhGZWVNb2RlbA==",
        "AAAAAAAAAAAAAAANZ2V0X21heF9zdGFrZQAAAAAAAAAAAAABAAAD6AAAAAs=",
        "AAAAAAAAAAAAAAANaXNfZGVueWxpc3RlZAAAAAAAAAEAAAAAAAAABHVzZXIAAAATAAAAAQAAAAE=",
        "AAAAAAAAAAAAAAANcHJlZGljdF9wcmljZQAAAAAAAAMAAAAAAAAABHVzZXIAAAATAAAAAAAAAA1ndWVzc2VkX3ByaWNlAAAAAAAACgAAAAAAAAAGYW1vdW50AAAAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAANcmVzb2x2ZV9yb3VuZAAAAAAAAAEAAAAAAAAAB3BheWxvYWQAAAAH0AAAAA1PcmFjbGVQYXlsb2FkAAAAAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAALVTZXRzIHRoZSBmZWUgaW5jaWRlbmNlIG1vZGVsIChhZG1pbiBvbmx5KS4KCmBGZWVPblBvdGAgKDApOiBmZWUgaXMgY2FsY3VsYXRlZCBvbiB0aGUgdG90YWwgcm91bmQgcG90IChkZWZhdWx0KS4KYEZlZU9uV2lubmluZ3NgICgxKTogZmVlIGlzIGNhbGN1bGF0ZWQgb25seSBvbiBuZXQgd2lubmluZ3MgLyBwcm9maXQuAAAAAAAADXNldF9mZWVfbW9kZWwAAAAAAAABAAAAAAAAAAVtb2RlbAAAAAAAB9AAAAAIRmVlTW9kZWwAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAANc2V0X21heF9zdGFrZQAAAAAAAAEAAAAAAAAACm1heF9hbW91bnQAAAAAA+gAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAAOYWRkX2RlbnlsaXN0ZWQAAAAAAAEAAAAAAAAABHVzZXIAAAATAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAjBFYXJseSBjYXNoLW91dCBkdXJpbmcgdGhlIFJ1bm5pbmcgcGhhc2UgZm9yIFVwRG93biByb3VuZHMuCgpBbGxvd3MgYSBiZXR0b3IgdG8gZXhpdCB0aGVpciBwb3NpdGlvbiBlYXJseSwgZm9yZmVpdGluZyBhIHBlcmNlbnRhZ2UKb2YgdGhlaXIgc3Rha2UgdG8gdGhlIHByb3RvY29sIHRyZWFzdXJ5LiBUaGUgZm9yZmVpdGVkIGFtb3VudCBpcwpkZXRlcm1pbmVkIGJ5IHRoZSBgRWFybHlDYXNob3V0QnBzYCBjb25maWcgKHNldCBieSBhZG1pbikuCgojIEVycm9ycwotIGBFYXJseUNhc2hvdXREaXNhYmxlZGAg4oCUIGZlYXR1cmUgbm90IGVuYWJsZWQgKG5vIHBlbmFsdHkgYnBzIGNvbmZpZ3VyZWQpCi0gYEVhcmx5Q2FzaG91dFBoYXNlSW52YWxpZGAg4oCUIG5vdCBpbiBSdW5uaW5nIHBoYXNlCi0gYEVhcmx5Q2FzaG91dE5vdFVwRG93bmAg4oCUIHJvdW5kIGlzIG5vdCBVcERvd24gbW9kZQotIGBOb0FjdGl2ZVJvdW5kYCDigJQgbm8gYWN0aXZlIHJvdW5kIGV4aXN0cwotIGBQb3NpdGlvbk5vdEZvdW5kYCDigJQgdXNlciBoYXMgbm8gcG9zaXRpb24gaW4gdGhlIGFjdGl2ZSByb3VuZAAAAA5jYXNoX291dF9lYXJseQAAAAAAAQAAAAAAAAAEdXNlcgAAABMAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAAOY2xhaW1fd2lubmluZ3MAAAAAAAEAAAAAAAAABHVzZXIAAAATAAAAAQAAA+kAAAALAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAIBBbnlvbmUgbWF5IGNhbGwgYGZpbmFsaXplX3JvdW5kYCBhZnRlciB0aGUgZGlzcHV0ZSB3aW5kb3cgZXhwaXJlcyB0bwpkaXN0cmlidXRlIHdpbm5pbmdzIHRvIHdpbm5lcnMgKG5vcm1hbCBzZXR0bGVtZW50IG91dGNvbWUpLgAAAA5maW5hbGl6ZV9yb3VuZAAAAAAAAQAAAAAAAAAIcm91bmRfaWQAAAAGAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAAOZ2V0X21pbnRfbGltaXQAAAAAAAAAAAABAAAABA==",
        "AAAAAAAAAAAAAAAOZ2V0X3VzZXJfc3RhdHMAAAAAAAEAAAAAAAAABHVzZXIAAAATAAAAAQAAB9AAAAAJVXNlclN0YXRzAAAA",
        "AAAAAAAAAAAAAAAOaXNfYWxsb3dsaXN0ZWQAAAAAAAEAAAAAAAAABHVzZXIAAAATAAAAAQAAAAE=",
        "AAAAAAAAAFNSZXR1cm5zIGB0cnVlYCBpZiB0aGUgb3JhY2xlIGhhcyBhIG5vbi1zdGFsZSBoZWFydGJlYXQgd2l0aCBzdGF0dXMgbm90IG9mZmxpbmUgKDIpLgAAAAAOaXNfb3JhY2xlX2xpdmUAAAAAAAAAAAABAAAAAQ==",
        "AAAAAAAAADdQYXVzZXMgdGhlIGNvbnRyYWN0IGZvciBlbWVyZ2VuY3kgcmVjb3ZlcnkgKGFkbWluIG9ubHkpAAAAAA5wYXVzZV9jb250cmFjdAAAAAAAAAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAOc2V0X21pbnRfbGltaXQAAAAAAAEAAAAAAAAABWxpbWl0AAAAAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAADpWZXRvZXMgYSBwZW5kaW5nIGFtZW5kbWVudCBiZWZvcmUgaXRzIHZldG8gd2luZG93IGV4cGlyZXMuAAAAAAAOdmV0b19hbWVuZG1lbnQAAAAAAAIAAAAAAAAABnZldG9lcgAAAAAAEwAAAAAAAAAMYW1lbmRtZW50X2lkAAAABgAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAPYWRkX2FsbG93bGlzdGVkAAAAAAEAAAAAAAAABHVzZXIAAAATAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAG5Bcm1zIGEgb25lLXNob3Qgb3ZlcnJpZGUgdG8gYnlwYXNzIHRoZSBoZWFydGJlYXQgaGVhbHRoIGdhdGUgZm9yIHRoZSBuZXh0IHNldHRsZW1lbnQgKGFkbWluIG9ubHksIElzc3VlICMyNjQpLgAAAAAAD2FybV9oYl9vdmVycmlkZQAAAAAAAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAipBdXRoLWdhdGVkIGJhdGNoIFRUTCBleHRlbnNpb24gZm9yIGFsbG93bGlzdGVkIHN0b3JhZ2Uga2V5cyAoYWRtaW4gb25seSkuCgpBY2NlcHRzIGEgdmVjdG9yIG9mIGBEYXRhS2V5Q29yZWAgdmFyaWFudHMuIEVhY2gga2V5IGlzIHZhbGlkYXRlZCBhZ2FpbnN0IHRoZQpUVEwtdG91Y2ggYWxsb3dsaXN0LiBLZXlzIHRoYXQgZXhpc3QgaW4gc3RvcmFnZSBoYXZlIHRoZWlyIFRUTCBleHRlbmRlZCB0bwpgVFRMX0JVTVBfQU1PVU5UYCAofjMwIGRheXMpLiBLZXlzIG5vdCBpbiB0aGUgYWxsb3dsaXN0IGNhdXNlIHRoZSBlbnRpcmUKY2FsbCB0byBmYWlsIHdpdGggYFVuc3VwcG9ydGVkRGF0YUtleUZvclR0bFRvdWNoYC4gS2V5cyB0aGF0IGFyZSBpbiB0aGUKYWxsb3dsaXN0IGJ1dCBhYnNlbnQgZnJvbSBzdG9yYWdlIGFyZSBzaWxlbnRseSBza2lwcGVkLgoKUmV0dXJucyB0aGUgbnVtYmVyIG9mIGtleXMgd2hvc2UgVFRMIHdhcyBhY3R1YWxseSBleHRlbmRlZC4KCkV2ZW50OiBgKCJzdG9yYWdlIiwgInRvdWNoIilgIHdpdGggYCh0b3VjaGVkLCBza2lwcGVkKWAgY291bnRzLgAAAAAAD2JhdGNoX3RvdWNoX3R0bAAAAAABAAAAAAAAAARrZXlzAAAD6gAAB9AAAAALRGF0YUtleUNvcmUAAAAAAQAAA+kAAAAEAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAADJSZXR1cm5zIHRoZSBhbm5vdW5jZWQgbmV4dCBzY2hlbWEgdmVyc2lvbiwgaWYgYW55LgAAAAAAD2dldF9uZXh0X3NjaGVtYQAAAAAAAAAAAQAAA+gAAAAE",
        "AAAAAAAAAAAAAAAPZ2V0X3JvdW5kX3BoYXNlAAAAAAAAAAABAAAD6QAAB9AAAAAKUm91bmRQaGFzZQAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAHtFc3RpbWF0ZXMgcGF5b3V0cyBmb3IgdGhlIGFjdGl2ZSByb3VuZCBnaXZlbiBhIGh5cG90aGV0aWNhbCBmaW5hbCBwcmljZS4KRG9lcyBub3QgbXV0YXRlIHN0b3JhZ2UuIFJldHVybnMgU2ltdWxhdGlvblJlc3VsdC4AAAAAD3NpbXVsYXRlX3BheW91dAAAAAABAAAAAAAAAAtmaW5hbF9wcmljZQAAAAAKAAAAAQAAA+kAAAfQAAAAEFNpbXVsYXRpb25SZXN1bHQAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAQZ2V0X2FjY2Vzc19zdGF0ZQAAAAEAAAAAAAAABHVzZXIAAAATAAAAAQAAB9AAAAALQWNjZXNzU3RhdGUA",
        "AAAAAAAAAAAAAAAQZ2V0X2FjdGl2ZV9yb3VuZAAAAAAAAAABAAAD6AAAB9AAAAAFUm91bmQAAAA=",
        "AAAAAAAAADtSZXR1cm5zIHRoZSBvbi1jaGFpbiBjb25zdGl0dXRpb24gbWV0YWRhdGEsIGlmIGVzdGFibGlzaGVkLgAAAAAQZ2V0X2NvbnN0aXR1dGlvbgAAAAAAAAABAAAD6AAAB9AAAAAUQ29uc3RpdHV0aW9uTWV0YWRhdGE=",
        "AAAAAAAAAEVSZXR1cm5zIHRoZSBjb25maWd1cmVkIHNlY29uZGFyeSBnb3Zlcm5hbmNlIGFwcHJvdmVyIGFkZHJlc3MsIGlmIHNldC4AAAAAAAAQZ2V0X2dvdl9hcHByb3ZlcgAAAAAAAAABAAAD6AAAABM=",
        "AAAAAAAAACpRdWVyaWVzIGRldGFpbHMgZm9yIGEgZ292ZXJuYW5jZSBwcm9wb3NhbC4AAAAAABBnZXRfZ292X3Byb3Bvc2FsAAAAAQAAAAAAAAALcHJvcG9zYWxfaWQAAAAABgAAAAEAAAPoAAAH0AAAAAtHb3ZQcm9wb3NhbAA=",
        "AAAAAAAABABSZXR1cm5zIHRoZSBzdGF0dXMgb2YgYSBzcGVjaWZpYyByb3VuZCBpZGVudGlmaWVkIGJ5IGByb3VuZF9pZGAuCgpMb29rdXAgc3RyYXRlZ3kgKGluIHByaW9yaXR5IG9yZGVyKToKMS4gSWYgdGhlIHJvdW5kIGlzIHRoZSAqKmN1cnJlbnQgYWN0aXZlIHJvdW5kKiosIGRlcml2ZSBzdGF0dXMgZnJvbQpsZWRnZXIgcG9zaXRpb24gcmVsYXRpdmUgdG8gYGJldF9lbmRfbGVkZ2VyYCAvIGBlbmRfbGVkZ2VyYC4KMi4gSWYgdGhlIHJvdW5kIGFwcGVhcnMgaW4gdGhlICoqb24tY2hhaW4gYXJjaGl2ZSoqLCBtYXAgaXRzCltgUm91bmRBcmNoaXZlU3RhdHVzYF0gdG8gdGhlIGNvcnJlc3BvbmRpbmcgdGVybWluYWwgW2BSb3VuZFN0YXR1c2BdLgozLiBJZiBhIGBDYW5jZWxsZWRSb3VuZGAgbWFya2VyIGV4aXN0cyAoYXJjaGl2ZSBtYXkgYmUgcHJ1bmVkKSwKcmV0dXJuIGBDYW5jZWxsZWRgLgo0LiBPdGhlcndpc2UsIHJldHVybiBgVW5rbm93bmAuCgp8IHJldHVybiB2YWx1ZSAgICAgICAgICB8IG1lYW5pbmcgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgfAp8LS0tLS0tLS0tLS0tLS0tLS0tLS0tLS18LS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tLS0tfAp8IGBVbmtub3duYCAgICAgICAgKDApICB8IFJvdW5kIG5vdCBmb3VuZDsgbmV2ZXIgY3JlYXRlZCBvciBwcnVuZWQgZnJvbSBhcmNoaXZlLiAgICAgICB8CnwgYEJldHRpbmdgICAgICAgICAoMSkgIHwgQWN0aXZlOyBgbGVkZ2VyIDwgYmV0X2VuZF9sZWRnZXJgLiAgICAgICAgICAgICAgICAgICAgICAgICAgIHwKfCBgUnVubmluZ2AgICAgICAgICgyKSAgfCBBY3RpdmU7IGBiZXRfZW5kX2xlZGdlciDiiaQgbGVkZ2VyIDwgZW5kX2xlZGdlcmAuICAgICAgICAgICAgICB8CnwgYEF3YWl0aW5nUmVzb2x2ZWAoMykgIHwgQWN0aXZlOyBgbGVkZ2VyIOKJpSBlbmRfbGVkZ2VyYCwgb3JhY2xlIG5vdCB5ZXQgY2FsbGVkLiAgICAgICAgfAp8IGBSAAAAEGdldF9yb3VuZF9zdGF0dXMAAAABAAAAAAAAAAhyb3VuZF9pZAAAAAYAAAABAAAH0AAAAAtSb3VuZFN0YXR1cwA=",
        "AAAAAAAAAE5SZXR1cm5zIHRoZSBjdXJyZW50IHJ1bnRpbWUgbW9kZSAoMCA9IE5vcm1hbCwgMSA9IENsYWltc09ubHksIDIgPSBGdWxseVBhdXNlZCkAAAAAABBnZXRfcnVudGltZV9tb2RlAAAAAAAAAAEAAAAE",
        "AAAAAAAAAEdSZXR1cm5zIHRoZSByZWNvcmRlZCBUV0FQIHByaWNlIHNhbXBsZXMsIG1vc3QtcmVjZW50IGxhc3QgKElzc3VlICMyNjYpLgAAAAAQZ2V0X3R3YXBfc2FtcGxlcwAAAAAAAAABAAAD6gAAB9AAAAALUHJpY2VTYW1wbGUA",
        "AAAAAAAAAAAAAAAQc2NoZWR1bGVfbWluX2JldAAAAAEAAAAAAAAACm1pbl9hbW91bnQAAAAAA+gAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAAQc2NoZWR1bGVfd2luZG93cwAAAAIAAAAAAAAAC2JldF9sZWRnZXJzAAAAAAQAAAAAAAAAC3J1bl9sZWRnZXJzAAAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAADpDb25maWd1cmVzIHRoZSBzZWNvbmRhcnkgZ292ZXJuYW5jZSBhcHByb3ZlciAoYWRtaW4gb25seSkuAAAAAAAQc2V0X2dvdl9hcHByb3ZlcgAAAAEAAAAAAAAACGFwcHJvdmVyAAAAEwAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAADJTZXRzIHRoZSBydW50aW1lIG1vZGUgb2YgdGhlIGNvbnRyYWN0IChhZG1pbiBvbmx5KQAAAAAAEHNldF9ydW50aW1lX21vZGUAAAABAAAAAAAAAARtb2RlAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAADFVbnBhdXNlcyB0aGUgY29udHJhY3QgYWZ0ZXIgcmVjb3ZlcnkgKGFkbWluIG9ubHkpAAAAAAAAEHVucGF1c2VfY29udHJhY3QAAAAAAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAD9DbGVhcnMgYSBwcmV2aW91c2x5IGFubm91bmNlZCBuZXh0IHNjaGVtYSB2ZXJzaW9uIChhZG1pbiBvbmx5KS4AAAAAEWNsZWFyX25leHRfc2NoZW1hAAAAAAAAAAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAARY29tbWl0X3ByZWRpY3Rpb24AAAAAAAADAAAAAAAAAAR1c2VyAAAAEwAAAAAAAAAEaGFzaAAAA+4AAAAgAAAAAAAAAAZhbW91bnQAAAAAAAsAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAARZ2V0X2FjY2Vzc19wb2xpY3kAAAAAAAABAAAAAAAAAAR1c2VyAAAAEwAAAAEAAAPtAAAAAgAAAAEAAAfQAAAAC0FjY2Vzc1N0YXRlAA==",
        "AAAAAAAAAAAAAAARZ2V0X2xhc3Rfcm91bmRfaWQAAAAAAAAAAAAAAQAAAAY=",
        "AAAAAAAAAAAAAAARZ2V0X3VzZXJfcG9zaXRpb24AAAAAAAABAAAAAAAAAAR1c2VyAAAAEwAAAAEAAAPoAAAH0AAAAAxVc2VyUG9zaXRpb24=",
        "AAAAAAAAAMVSZXR1cm5zIHdoZXRoZXIgYGFjdGlvbmAgaXMgY3VycmVudGx5IHBlcm1pdHRlZCB1bmRlciB0aGUgUG9saWN5R2F0ZQpmb3IgdGhlIGNvbnRyYWN0J3MgcnVudGltZSBtb2RlIChJc3N1ZSAjMjYxKS4gUmVhZC1vbmx5OyBkb2VzIG5vdAptdXRhdGUgc3RhdGUuIFNlZSBbYGFkbWluOjpfcG9saWN5X2dhdGVgXSBmb3IgdGhlIGZ1bGwgbWF0cml4LgAAAAAAABFpc19hY3Rpb25fYWxsb3dlZAAAAAAAAAEAAAAAAAAABmFjdGlvbgAAAAAH0AAAAAxQb2xpY3lBY3Rpb24AAAABAAAAAQ==",
        "AAAAAAAAAEZQcm9wb3NlcyBhIHBhcmFtZXRlciBhbWVuZG1lbnQgd2l0aCB0aW1lbG9jayBhbmQgb3B0aW9uYWwgdmV0byB3aW5kb3cuAAAAAAARcHJvcG9zZV9hbWVuZG1lbnQAAAAAAAADAAAAAAAAAAhwcm9wb3NlcgAAABMAAAAAAAAADnBhcmFtZXRlcl9uYW1lAAAAAAARAAAAAAAAAAluZXdfdmFsdWUAAAAAAAAAAAAAAQAAA+kAAAAGAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAARcmVtb3ZlX2RlbnlsaXN0ZWQAAAAAAAABAAAAAAAAAAR1c2VyAAAAEwAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAARcmV2ZWFsX3ByZWRpY3Rpb24AAAAAAAADAAAAAAAAAAR1c2VyAAAAEwAAAAAAAAAPcHJlZGljdGVkX3ByaWNlAAAAAAoAAAAAAAAABHNhbHQAAAPuAAAAIAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAC5BY3RpdmF0ZXMgYW4gYW1lbmRtZW50IGFmdGVyIHRpbWVsb2NrIGV4cGlyZXMuAAAAAAASYWN0aXZhdGVfYW1lbmRtZW50AAAAAAACAAAAAAAAAAlhY3RpdmF0b3IAAAAAAAATAAAAAAAAAAxhbWVuZG1lbnRfaWQAAAAGAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAASZ2V0X2FyY2hpdmVkX3JvdW5kAAAAAAABAAAAAAAAAAhyb3VuZF9pZAAAAAYAAAABAAAD6AAAB9AAAAAUQXJjaGl2ZWRSb3VuZFN1bW1hcnk=",
        "AAAAAAAAAEVSZXR1cm5zIHdoZXRoZXIgb3JhY2xlIGhlYXJ0YmVhdCBzdHJpY3QgbW9kZSBpcyBlbmFibGVkIChJc3N1ZSAjMjY0KS4AAAAAAAASZ2V0X2hiX3N0cmljdF9tb2RlAAAAAAAAAAAAAQAAAAE=",
        "AAAAAAAAAC5SZXR1cm5zIHRoZSBjb25maWd1cmVkIHJvdW5kIHRlbXBsYXRlLCBpZiBhbnkuAAAAAAASZ2V0X3JvdW5kX3RlbXBsYXRlAAAAAAAAAAAAAQAAA+gAAAfQAAAADVJvdW5kVGVtcGxhdGUAAAA=",
        "AAAAAAAAAEZSZXR1cm5zIHRoZSBzdG9yZWQgc2NoZW1hIHZlcnNpb24uIElmIHVuc2V0LCByZXR1cm5zIGxlZ2FjeSB2ZXJzaW9uIDEuAAAAAAASZ2V0X3NjaGVtYV92ZXJzaW9uAAAAAAAAAAAAAQAAAAQ=",
        "AAAAAAAAAENSZXR1cm5zIHRoZSBmcm96ZW4gYXJjaGl2ZSBmb3IgYSBwYXN0IHNlYXNvbiwgaWYgaXQgaGFzIGJlZW4gcmVzZXQuAAAAABJnZXRfc2Vhc29uX2FyY2hpdmUAAAAAAAEAAAAAAAAACXNlYXNvbl9pZAAAAAAAAAQAAAABAAAD6AAAB9AAAAANU2Vhc29uQXJjaGl2ZQAAAA==",
        "AAAAAAAAAAAAAAASaXNfcm91bmRfY2FuY2VsbGVkAAAAAAABAAAAAAAAAAhyb3VuZF9pZAAAAAYAAAABAAAAAQ==",
        "AAAAAAAAAExQcm9wb3NlcyBhIHByb3RlY3RlZCBhZG1pbmlzdHJhdGl2ZSBhY3Rpb24gKGdvdmVybmFuY2UgYWRtaW4vYXBwcm92ZXIgb25seSkuAAAAEnByb3Bvc2VfZ292X2FjdGlvbgAAAAAAAwAAAAAAAAAIcHJvcG9zZXIAAAATAAAAAAAAAAZhY3Rpb24AAAAAB9AAAAAJR292QWN0aW9uAAAAAAAAAAAAAApjdXN0b21fdHRsAAAAAAPoAAAABAAAAAEAAAPpAAAABgAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAAScmVtb3ZlX2FsbG93bGlzdGVkAAAAAAABAAAAAAAAAAR1c2VyAAAAEwAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAASc2NoZWR1bGVfbWF4X3N0YWtlAAAAAAABAAAAAAAAAAptYXhfYW1vdW50AAAAAAPoAAAACwAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAGNFbmFibGVzIG9yIGRpc2FibGVzIHN0cmljdCBtb2RlIGZvciBvcmFjbGUgaGVhcnRiZWF0IGhlYWx0aCBhdCBzZXR0bGVtZW50IChhZG1pbiBvbmx5LCBJc3N1ZSAjMjY0KS4AAAAAEnNldF9oYl9zdHJpY3RfbW9kZQAAAAAAAQAAAAAAAAAHZW5hYmxlZAAAAAABAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAEpTdG9yZXMgdGhlIGFkbWluJ3MgYmx1ZXByaW50IGZvciBgY3JlYXRlX25leHRfZnJvbV90ZW1wbGF0ZWAgKGFkbWluIG9ubHkpLgAAAAAAEnNldF9yb3VuZF90ZW1wbGF0ZQAAAAAAAgAAAAAAAAALc3RhcnRfcHJpY2UAAAAACgAAAAAAAAAEbW9kZQAAA+gAAAAEAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAEtDYW5jZWxzIGFuIHVuZXhlY3V0ZWQgZ292ZXJuYW5jZSBwcm9wb3NhbCAoZ292ZXJuYW5jZSBhZG1pbi9hcHByb3ZlciBvbmx5KS4AAAAAE2NhbmNlbF9nb3ZfcHJvcG9zYWwAAAAAAgAAAAAAAAAJY2FuY2VsbGVyAAAAAAAAEwAAAAAAAAALcHJvcG9zYWxfaWQAAAAABgAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAEhSZXR1cm5zIHRoZSBjb25maWd1cmVkIGF0dGVzdGF0aW9uIHNpZ25pbmcga2V5LCBpZiBlbmFibGVkIChJc3N1ZSAjMjYzKS4AAAATZ2V0X2F0dGVzdGF0aW9uX2tleQAAAAAAAAAAAQAAA+gAAAPuAAAAIA==",
        "AAAAAAAAAAAAAAATZ2V0X2Rpc3B1dGVfbGVkZ2VycwAAAAAAAAAAAQAAAAQ=",
        "AAAAAAAAALdSZXR1cm5zIGEgc2luZ2xlLXJlYWQgY29tcG9zaXRlIHNuYXBzaG90IG9mIGN1cnJlbnQgbWFya2V0IHN0YXRlOgpyb3VuZCBwaGFzZSwgcG9vbCBjb21wb3NpdGlvbiwgdGltaW5nIGJ1ZmZlcnMsIGFuZCBmZWUgY29uZmlndXJhdGlvbi4KU2VlIGBNYXJrZXRTbmFwc2hvdGAgZm9yIGVtcHR5LXJvdW5kIHNlbWFudGljcy4AAAAAE2dldF9tYXJrZXRfc25hcHNob3QAAAAAAAAAAAEAAAfQAAAADk1hcmtldFNuYXBzaG90AAA=",
        "AAAAAAAAACpSZXR1cm5zIGEgY29tcG9zaXRlIHByb3RvY29sIGhlYWx0aCBzdGF0dXMAAAAAABNnZXRfcHJvdG9jb2xfaGVhbHRoAAAAAAAAAAABAAAH0AAAABRQcm90b2NvbEhlYWx0aFN0YXR1cw==",
        "AAAAAAAAAt1SZXR1cm5zIHRoZSBnbG9iYWwgc3RhdHVzIG9mIHRoZSBwcm90b2NvbC4KClRoaXMgaXMgdGhlIGNhbm9uaWNhbCBzaW5nbGUtY2FsbCBzdGF0dXMgZW5kcG9pbnQgZm9yIGZyb250ZW5kcyBhbmQKbW9uaXRvcmluZyBkYXNoYm9hcmRzLiBJdCBpcyBhIHB1cmUgcHJvamVjdGlvbiBvZiBbYFJ1bnRpbWVNb2RlYF0KcGx1cyAiaXMgYSByb3VuZCBhY3RpdmUiIChzZWUgYGRvY3MvU1RBVFVTX0NPREVTLm1kYCk6Cgp8IGBSdW50aW1lTW9kZWAgICAgICAgfCBhY3RpdmUgcm91bmQ/IHwgcmV0dXJuIHZhbHVlICAgICAgfAp8LS0tLS0tLS0tLS0tLS0tLS0tLS0tfC0tLS0tLS0tLS0tLS0tLXwtLS0tLS0tLS0tLS0tLS0tLS0tfAp8IGBGdWxseVBhdXNlZGAgKDIpICAgfCBhbnkgICAgICAgICAgIHwgYFBhdXNlZGAgICAgICAoMSkgfAp8IGBDbGFpbXNPbmx5YCAgKDEpICAgfCBhbnkgICAgICAgICAgIHwgYENsYWltc09ubHlgICAoMikgfAp8IGBOb3JtYWxgICAgICAgKDApICAgfCBubyAgICAgICAgICAgIHwgYENsYWltc09ubHlgICAoMikgfAp8IGBOb3JtYWxgICAgICAgKDApICAgfCB5ZXMgICAgICAgICAgIHwgYEFjdGl2ZWAgICAgICAoMCkgfAoKYEFjdGl2ZWAgaXMgcmV0dXJuZWQgb25seSB3aGVuIHJvdW5kIG11dGF0aW9ucyAoYmV0cywgcmV2ZWFscykgd291bGQKYWN0dWFsbHkgcGFzcyB0aGUgcG9saWN5IGdhdGU7IGBQYXVzZWRgIG9ubHkgd2hlbiBjbGFpbXMgYXJlIGJsb2NrZWQuAAAAAAAAE2dldF9wcm90b2NvbF9zdGF0dXMAAAAAAAAAAAEAAAfQAAAADlByb3RvY29sU3RhdHVzAAA=",
        "AAAAAAAAARxSZXNvbHZlcyB0aGUgYWN0aXZlIHJvdW5kIHVzaW5nIGEgbXVsdGktZmVlZCBvcmFjbGUgcGF5bG9hZCB3aXRoCm1lZGlhbiBzZXR0bGVtZW50IGFuZCBxdW9ydW0tYmFzZWQgb3V0bGllciByZWplY3Rpb24uCgpSZXF1aXJlcyBgT3JhY2xlUXVvcnVtQ29uZmlnYCB0byBiZSBjb25maWd1cmVkIGJ5IHRoZSBhZG1pbiBiZWZvcmUKdGhpcyBwYXRoIGlzIGF2YWlsYWJsZS4gVGhlIGxlZ2FjeSBzaW5nbGUtb3JhY2xlIGByZXNvbHZlX3JvdW5kYApyZW1haW5zIGF2YWlsYWJsZSBpbmRlcGVuZGVudGx5LgAAABNyZXNvbHZlX3JvdW5kX211bHRpAAAAAAEAAAAAAAAAB3BheWxvYWQAAAAH0AAAABBNdWx0aUZlZWRQYXlsb2FkAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAL9TZXRzIChvciBjbGVhcnMpIHRoZSBlZDI1NTE5IHB1YmxpYyBrZXkgdXNlZCB0byB2ZXJpZnkgb3JhY2xlCmF0dGVzdGF0aW9uIHNpZ25hdHVyZXMgKGFkbWluIG9ubHksIElzc3VlICMyNjMpLiBgTm9uZWAgZGlzYWJsZXMKYXR0ZXN0YXRpb24gdmVyaWZpY2F0aW9uLCByZXN0b3JpbmcgYWNjb3VudC1hdXRoLW9ubHkgYmVoYXZpb3VyLgAAAAATc2V0X2F0dGVzdGF0aW9uX2tleQAAAAABAAAAAAAAAANrZXkAAAAD6AAAA+4AAAAgAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAATc2V0X2Rpc3B1dGVfbGVkZ2VycwAAAAABAAAAAAAAAAdsZWRnZXJzAAAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAANlBbm5vdW5jZXMgYSB0YXJnZXQgc2NoZW1hIHZlcnNpb24gZm9yIHRoZSBuZXh0IHBsYW5uZWQgbWlncmF0aW9uIChhZG1pbiBvbmx5KS4KClRoaXMgc2V0cyBhICJ2LW5leHQgc2NoZW1hIHRlbXBsYXRlIiB0aGF0IG9wZXJhdG9ycyBjYW4gaW5zcGVjdCBiZWZvcmUKdGhlIHJlYWwgbWlncmF0aW9uIGV4ZWN1dGVzLiBJdCBkb2VzIE5PVCBjaGFuZ2UgdGhlIGFjdGl2ZSBzY2hlbWEuAAAAAAAAFGFubm91bmNlX25leHRfc2NoZW1hAAAAAQAAAAAAAAAOdGFyZ2V0X3ZlcnNpb24AAAAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAGBBcHByb3ZlcyBhIHBlbmRpbmcgZ292ZXJuYW5jZSBwcm9wb3NhbCAoZ292ZXJuYW5jZSBhZG1pbi9hcHByb3ZlciBvbmx5LCBkaXN0aW5jdCBmcm9tIHByb3Bvc2VyKS4AAAAUYXBwcm92ZV9nb3ZfcHJvcG9zYWwAAAACAAAAAAAAAAhhcHByb3ZlcgAAABMAAAAAAAAAC3Byb3Bvc2FsX2lkAAAAAAYAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAAUY2FuY2VsX2NvbmZpZ19jaGFuZ2UAAAABAAAAAAAAAARraW5kAAAH0AAAABBDb25maWdDaGFuZ2VLaW5kAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAADNSZW1vdmVzIHRoZSBjb25maWd1cmVkIHJvdW5kIHRlbXBsYXRlIChhZG1pbiBvbmx5KS4AAAAAFGNsZWFyX3JvdW5kX3RlbXBsYXRlAAAAAAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAEpFeGVjdXRlcyBhbiBhcHByb3ZlZCBnb3Zlcm5hbmNlIHByb3Bvc2FsIChnb3Zlcm5hbmNlIGFkbWluL2FwcHJvdmVyIG9ubHkpLgAAAAAAFGV4ZWN1dGVfZ292X3Byb3Bvc2FsAAAAAgAAAAAAAAAIZXhlY3V0b3IAAAATAAAAAAAAAAtwcm9wb3NhbF9pZAAAAAAGAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAChSZXR1cm5zIGRlZmF1bHQgcHJvcG9zYWwgVFRMIGluIGxlZGdlcnMuAAAAFGdldF9nb3ZfcHJvcG9zYWxfdHRsAAAAAAAAAAEAAAAE",
        "AAAAAAAAAFFSZXR1cm5zIHRoZSBjb25maWd1cmVkIGhlYXJ0YmVhdCBncmFjZSBwZXJpb2QgaW4gc2Vjb25kcyAoZGVmYXVsdCAwLCBJc3N1ZSAjMjY0KS4AAAAAAAAUZ2V0X2hiX2dyYWNlX3NlY29uZHMAAAAAAAAAAQAAAAY=",
        "AAAAAAAAAAAAAAAUZ2V0X21pbl9wYXJ0aWNpcGFudHMAAAAAAAAAAQAAA+gAAAAE",
        "AAAAAAAAAAAAAAAUZ2V0X29uZV9zaWRlZF9wb2xpY3kAAAAAAAAAAQAAB9AAAAAOT25lU2lkZWRQb2xpY3kAAA==",
        "AAAAAAAAADhSZXR1cm5zIHRoZSBtb3N0IHJlY2VudCBvcmFjbGUgaGVhcnRiZWF0IHJlY29yZCwgaWYgYW55LgAAABRnZXRfb3JhY2xlX2hlYXJ0YmVhdAAAAAAAAAABAAAD6AAAB9AAAAAVT3JhY2xlSGVhcnRiZWF0UmVjb3JkAAAA",
        "AAAAAAAAAAAAAAAUZ2V0X3BlbmRpbmdfd2lubmluZ3MAAAABAAAAAAAAAAR1c2VyAAAAEwAAAAEAAAAL",
        "AAAAAAAAAAAAAAAUZ2V0X3Byb3RvY29sX2ZlZV9icHMAAAAAAAAAAQAAA+gAAAAE",
        "AAAAAAAAAAAAAAAUZ2V0X3JvdW5kX3Bvb2xfc3RhdHMAAAAAAAAAAQAAA+gAAAfQAAAADlJvdW5kUG9vbFN0YXRzAAA=",
        "AAAAAAAAAAAAAAAUZ2V0X3VwZG93bl9wb3NpdGlvbnMAAAAAAAAAAQAAA+wAAAATAAAH0AAAAAxVc2VyUG9zaXRpb24=",
        "AAAAAAAAADJTZXRzIGRlZmF1bHQgcHJvcG9zYWwgVFRMIGluIGxlZGdlcnMgKGFkbWluIG9ubHkpLgAAAAAAFHNldF9nb3ZfcHJvcG9zYWxfdHRsAAAAAQAAAAAAAAALdHRsX2xlZGdlcnMAAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAGtTZXRzIHRoZSBncmFjZSBwZXJpb2QgaW4gc2Vjb25kcyBiZXR3ZWVuIGhlYXJ0YmVhdCBzdGFsZW5lc3MgYW5kIHNldHRsZW1lbnQgYmxvY2sgKGFkbWluIG9ubHksIElzc3VlICMyNjQpLgAAAAAUc2V0X2hiX2dyYWNlX3NlY29uZHMAAAABAAAAAAAAAAdzZWNvbmRzAAAAAAYAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAAUc2V0X21pbl9wYXJ0aWNpcGFudHMAAAABAAAAAAAAAANtaW4AAAAD6AAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAAUc2V0X3Byb3RvY29sX2ZlZV9icHMAAAABAAAAAAAAAANicHMAAAAD6AAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAAVZ2V0X2FyY2hpdmVfcmV0ZW50aW9uAAAAAAAAAAAAAAEAAAAE",
        "AAAAAAAAAEZSZXR1cm5zIHRoZSBpZCBvZiB0aGUgY3VycmVudGx5LWFjdGl2ZSBsZWFkZXJib2FyZCBzZWFzb24gKGRlZmF1bHQgMSkuAAAAAAAVZ2V0X2N1cnJlbnRfc2Vhc29uX2lkAAAAAAAAAAAAAAEAAAAE",
        "AAAAAAAAAD5SZXR1cm5zIHRoZSBjb25maWd1cmVkIGVhcmx5IGNhc2gtb3V0IHBlbmFsdHkgYnBzLCBpZiBlbmFibGVkLgAAAAAAFWdldF9lYXJseV9jYXNob3V0X2JwcwAAAAAAAAAAAAABAAAD6AAAAAQ=",
        "AAAAAAAAAAAAAAAVZ2V0X2Vwb2NoX21pbnRfYnVkZ2V0AAAAAAAAAAAAAAEAAAAL",
        "AAAAAAAAAE5SZXR1cm5zIHdoZXRoZXIgdGhlIG9yYWNsZSBoZWFydGJlYXQgb3ZlcnJpZGUgaXMgY3VycmVudGx5IGFybWVkIChJc3N1ZSAjMjY0KS4AAAAAABVnZXRfaGJfb3ZlcnJpZGVfYXJtZWQAAAAAAAAAAAAAAQAAAAE=",
        "AAAAAAAAAAAAAAAVZ2V0X21heF91c2VyX2V4cG9zdXJlAAAAAAAAAAAAAAEAAAPoAAAACw==",
        "AAAAAAAAAEpSZXR1cm5zIGEgdXNlcidzIHNlYXNvbi1zY29wZWQgc3RhdHMgZm9yIGBzZWFzb25faWRgIChhY3RpdmUgb3IgYXJjaGl2ZWQpLgAAAAAAFWdldF9zZWFzb25fdXNlcl9zdGF0cwAAAAAAAAIAAAAAAAAACXNlYXNvbl9pZAAAAAAAAAQAAAAAAAAABHVzZXIAAAATAAAAAQAAB9AAAAAJVXNlclN0YXRzAAAA",
        "AAAAAAAAAAAAAAAVc2V0X2FyY2hpdmVfcmV0ZW50aW9uAAAAAAAAAQAAAAAAAAAFbGltaXQAAAAAAAAEAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAALhTZXRzIHRoZSBlYXJseSBjYXNoLW91dCBwZW5hbHR5IHJhdGUgaW4gYmFzaXMgcG9pbnRzIChhZG1pbiBvbmx5KS4KYE5vbmVgIGRpc2FibGVzIGVhcmx5IGNhc2gtb3V0IGVudGlyZWx5IChkZWZhdWx0KS4KYFNvbWUoYnBzKWAgZW5hYmxlcyBpdCB3aXRoIHRoZSBnaXZlbiBwZW5hbHR5IHJhdGUgKDHigJMxMDAwIGJwcykuAAAAFXNldF9lYXJseV9jYXNob3V0X2JwcwAAAAAAAAEAAAAAAAAAA2JwcwAAAAPoAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAVc2V0X2Vwb2NoX21pbnRfYnVkZ2V0AAAAAAAAAQAAAAAAAAAGYnVkZ2V0AAAAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAAVc2V0X21heF91c2VyX2V4cG9zdXJlAAAAAAAAAQAAAAAAAAAMbWF4X2V4cG9zdXJlAAAD6AAAAAsAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAEdUb3AtdXBzIHRoZSBpbnN1cmFuY2UgZnVuZCBmcm9tIHRoZSBjYWxsZXIncyB2WExNIGJhbGFuY2UgKGFkbWluIG9ubHkpLgAAAAAVdG9wX3VwX2luc3VyYW5jZV9mdW5kAAAAAAAAAQAAAAAAAAAGYW1vdW50AAAAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAAVd2l0aGRyYXdfcHJvdG9jb2xfZmVlAAAAAAAAAgAAAAAAAAAJcmVjaXBpZW50AAAAAAAAEwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAQAAA+kAAAALAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAArpBY2NlcHRzIGEgcGVuZGluZyBvcmFjbGUgcm90YXRpb24gcHJvcG9zYWwgYmVmb3JlIGV4cGlyeSAoYW55IGNhbGxlcikuCgoqKlNlY3VyaXR5Kio6IEEgbWFuZGF0b3J5IGBNSU5fUk9UQVRJT05fREVMQVlfU0VDT05EU2AgKDEgaG91cikgbXVzdAplbGFwc2UgYmV0d2VlbiBwcm9wb3NhbCBhbmQgYWNjZXB0YW5jZS4gVGhpcyBwcmV2ZW50cyBxdWlldCBvbmUtYmxvY2sKdGFrZW92ZXJzIOKAlCBldmVuIGlmIHRoZSBhZG1pbiBrZXkgaXMgY29tcHJvbWlzZWQsIHRoZSBjb21tdW5pdHkgaGFzIGEKZnVsbCBob3VyIHRvIG9ic2VydmUgdGhlIHByb3Bvc2FsIGV2ZW50IGFuZCByZWFjdCBiZWZvcmUgdGhlIG9yYWNsZQphY3R1YWxseSBjaGFuZ2VzLgoKSWYgdGhlIGRlbGF5IGhhcyBub3QgZWxhcHNlZCB0aGUgY2FsbCByZXR1cm5zIGBSb3RhdGlvbkRlbGF5Tm90RWxhcHNlZGAuCklmIHRoZSBwcm9wb3NhbCBoYXMgZXhwaXJlZCBpdCByZXR1cm5zIGBOb1BlbmRpbmdSb3RhdGlvbmAgYW5kIHRoZQpzdGFsZSBwcm9wb3NhbCBpcyByZW1vdmVkIGFmdGVyIGVtaXR0aW5nIGAoIm9yYWNsZSIsICJleHBpcmVkIilgLgpPbiBzdWNjZXNzIHRoZSBzdG9yZWQgb3JhY2xlIGFkZHJlc3MgaXMgdXBkYXRlZCBhbmQKYCgib3JhY2xlIiwgImFjY2VwdCIpYCBpcyBlbWl0dGVkIHdpdGggdGhlIHByZXZpb3VzIGFuZCBuZXcgYWRkcmVzc2VzLgAAAAAAFmFjY2VwdF9vcmFjbGVfcm90YXRpb24AAAAAAAAAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAHRDYW5jZWxzIGEgcGVuZGluZyBvcmFjbGUgcm90YXRpb24gcHJvcG9zYWwgYmVmb3JlIGl0IGV4cGlyZXMgKGFkbWluIG9ubHkpLgoKRW1pdHMgYCgib3JhY2xlIiwgImNhbmNlbCIpYCBvbiBzdWNjZXNzLgAAABZjYW5jZWxfb3JhY2xlX3JvdGF0aW9uAAAAAAAAAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAElFc3RhYmxpc2hlcyB0aGUgb24tY2hhaW4gY29uc3RpdHV0aW9uIHdpdGggZ292ZXJuYW5jZSBydWxlcyAoYWRtaW4gb25seSkuAAAAAAAAFmVzdGFibGlzaF9jb25zdGl0dXRpb24AAAAAAAMAAAAAAAAAE3ZldG9fd2luZG93X2xlZGdlcnMAAAAABAAAAAAAAAAQdGltZWxvY2tfbGVkZ2VycwAAAAQAAAAAAAAAFmR1YWxfYXBwcm92YWxfcmVxdWlyZWQAAAAAAAEAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAADhSZXR1cm5zIHRoZSBjb25maWd1cmVkIGJldHRpbmctd2luZG93IGxlbmd0aCBpbiBsZWRnZXJzLgAAABZnZXRfYmV0X3dpbmRvd19sZWRnZXJzAAAAAAAAAAAAAQAAAAQ=",
        "AAAAAAAAAFNSZXR1cm5zIHRoZSBjb25maWd1cmVkIGRldmlhdGlvbiByZWZlcmVuY2UgbW9kZSAoZGVmYXVsdCBgU3RhcnRQcmljZWAsIElzc3VlICMyNjYpLgAAAAAWZ2V0X2RldmlhdGlvbl9yZWZfbW9kZQAAAAAAAAAAAAEAAAfQAAAAFkRldmlhdGlvblJlZmVyZW5jZU1vZGUAAA==",
        "AAAAAAAAAC5SZXR1cm5zIHdoZXRoZXIgb3JhY2xlIHN0cmljdCBtb2RlIGlzIGVuYWJsZWQuAAAAAAAWZ2V0X29yYWNsZV9zdHJpY3RfbW9kZQAAAAAAAAAAAAEAAAAB",
        "AAAAAAAAADRSZXR1cm5zIHRoZSBjb25maWd1cmVkIHJ1bi13aW5kb3cgbGVuZ3RoIGluIGxlZGdlcnMuAAAAFmdldF9ydW5fd2luZG93X2xlZGdlcnMAAAAAAAAAAAABAAAABA==",
        "AAAAAAAAAJxTZXRzIHRoZSBvcmFjbGUgZGV2aWF0aW9uIHJlZmVyZW5jZSBtb2RlIOKAlCBgU3RhcnRQcmljZWAgKGRlZmF1bHQpIG9yCmBUd2FwYCDigJQgYW5kLCBmb3IgYFR3YXBgLCB0aGUgdHJhaWxpbmcgc2FtcGxlIHdpbmRvdyBzaXplIChhZG1pbiBvbmx5LCBJc3N1ZSAjMjY2KS4AAAAWc2V0X2RldmlhdGlvbl9yZWZfbW9kZQAAAAAAAgAAAAAAAAAEbW9kZQAAB9AAAAAWRGV2aWF0aW9uUmVmZXJlbmNlTW9kZQAAAAAAAAAAAA53aW5kb3dfc2FtcGxlcwAAAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAENFbmFibGVzIG9yIGRpc2FibGVzIHN0cmljdCBtb2RlIGZvciBvcmFjbGUgY29uZmlkZW5jZSAoYWRtaW4gb25seSkuAAAAABZzZXRfb3JhY2xlX3N0cmljdF9tb2RlAAAAAAABAAAAAAAAAAdlbmFibGVkAAAAAAEAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAAXYXBwbHlfc2NoZWR1bGVkX2NoYW5nZXMAAAAAAQAAAAAAAAAEa2luZAAAB9AAAAAQQ29uZmlnQ2hhbmdlS2luZAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAADdSZXR1cm5zIHRoZSBjb25maWd1cmVkIGluc3VyYW5jZSBzcGxpdCBpbiBiYXNpcyBwb2ludHMuAAAAABdnZXRfaW5zdXJhbmNlX3NwbGl0X2JwcwAAAAAAAAAAAQAAAAQ=",
        "AAAAAAAAAH9DdXJzb3ItYmFzZWQgcGFnZSBvZiB0aGUgZ2xvYmFsIGxlYWRlcmJvYXJkIG9yZGVyZWQgYnkgdG90YWwgd2lucyBkZXNjZW5kaW5nLgpSZWplY3RzIGlmIGBsaW1pdGAgZXhjZWVkcyBgTUFYX1BBR0VfU0laRWAgKDEwMCkuAAAAABdnZXRfbGVhZGVyYm9hcmRfYnlfd2lucwAAAAACAAAAAAAAAAZjdXJzb3IAAAAAA+gAAAATAAAAAAAAAAVsaW1pdAAAAAAAAAQAAAABAAAD6QAAA+0AAAACAAAD6gAAB9AAAAAQTGVhZGVyYm9hcmRFbnRyeQAAA+gAAAATAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAKhNaWdyYXRlcyBsZWdhY3kgc2NoZW1hIHZlcnNpb24gMSDihpIgdmVyc2lvbiAyIChhZG1pbiBvbmx5KS4KCldoZW4gYGRyeV9ydW5gIGlzIGB0cnVlYCwgYWxsIHZhbGlkYXRpb24gY2hlY2tzIGFyZSBwZXJmb3JtZWQgYnV0IG5vCnN0b3JhZ2Ugd3JpdGVzIG9yIGV2ZW50cyBhcmUgZW1pdHRlZC4AAAAXbWlncmF0ZV9zY2hlbWFfdjFfdG9fdjIAAAAAAQAAAAAAAAAHZHJ5X3J1bgAAAAABAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAKFNaWdyYXRlcyBzY2hlbWEgdmVyc2lvbiAyIOKGkiB2ZXJzaW9uIDMgKGFkbWluIG9ubHkpLgoKV2hlbiBgZHJ5X3J1bmAgaXMgYHRydWVgLCBhbGwgdmFsaWRhdGlvbiBjaGVja3MgYXJlIHBlcmZvcm1lZCBidXQgbm8Kc3RvcmFnZSB3cml0ZXMgb3IgZXZlbnRzIGFyZSBlbWl0dGVkLgAAAAAAABdtaWdyYXRlX3NjaGVtYV92Ml90b192MwAAAAABAAAAAAAAAAdkcnlfcnVuAAAAAAEAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAQpQcm9wb3NlcyBhIG5ldyBvcmFjbGUgYWRkcmVzcyB3aXRoIGFuIGV4cGlyeSB3aW5kb3cgKGFkbWluIG9ubHkpLgoKVGhlIHByb3Bvc2FsIG11c3QgYmUgYWNjZXB0ZWQgdmlhIFtgU2VsZjo6YWNjZXB0X29yYWNsZV9yb3RhdGlvbmBdIGJlZm9yZQpgZXhwaXJlc19pbl9zZWNvbmRzYCBlbGFwc2VzLCBvdGhlcndpc2UgYWNjZXB0YW5jZSBpcyByZWplY3RlZC4KTWluaW11bSBleHBpcnkgaXMgNjAgc2Vjb25kcy4KCkVtaXRzIGAoIm9yYWNsZSIsICJwcm9wb3NlIilgLgAAAAAAF3Byb3Bvc2Vfb3JhY2xlX3JvdGF0aW9uAAAAAAIAAAAAAAAACm5ld19vcmFjbGUAAAAAABMAAAAAAAAAEmV4cGlyZXNfaW5fc2Vjb25kcwAAAAAABgAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAH1TZXRzIHRoZSBpbnN1cmFuY2UgYWNjcnVhbCBzcGxpdDogaG93IG1hbnkgYmFzaXMgcG9pbnRzIG9mIGVhY2gKcHJvdG9jb2wgZmVlIGFyZSBkaXJlY3RlZCB0byB0aGUgaW5zdXJhbmNlIGZ1bmQgKGFkbWluIG9ubHkpLgAAAAAAABdzZXRfaW5zdXJhbmNlX3NwbGl0X2JwcwAAAAABAAAAAAAAAANicHMAAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAACpSZWNvcmRzIGFuIG9yYWNsZSBoZWFydGJlYXQgKG9yYWNsZSBvbmx5KS4AAAAAABd1cGRhdGVfb3JhY2xlX2hlYXJ0YmVhdAAAAAABAAAAAAAAAAZzdGF0dXMAAAAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAHVXaXRoZHJhd3MgZnJvbSB0aGUgaW5zdXJhbmNlIGZ1bmQgdG8gYSByZWNpcGllbnQgKGFkbWluIG9ubHksCnJlcXVpcmVzIGdvdmVybmFuY2UgZHVhbC1jb250cm9sIHdoZW4gYXBwcm92ZXIgaXMgc2V0KS4AAAAAAAAXd2l0aGRyYXdfaW5zdXJhbmNlX2Z1bmQAAAAAAgAAAAAAAAAJcmVjaXBpZW50AAAAAAAAEwAAAAAAAAAGYW1vdW50AAAAAAALAAAAAQAAA+kAAAALAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAAYZ2V0X2Nsb3NlX2J1ZmZlcl9sZWRnZXJzAAAAAAAAAAEAAAAE",
        "AAAAAAAAAAAAAAAYZ2V0X21heF9wZW5kaW5nX3dpbm5pbmdzAAAAAAAAAAEAAAPoAAAACw==",
        "AAAAAAAAAD9SZXR1cm5zIHRoZSBjb25maWd1cmVkIG11bHRpLWZlZWQgb3JhY2xlIHF1b3J1bSBjb25maWcsIGlmIGFueS4AAAAAGGdldF9vcmFjbGVfcXVvcnVtX2NvbmZpZwAAAAAAAAABAAAD6AAAB9AAAAAST3JhY2xlUXVvcnVtQ29uZmlnAAA=",
        "AAAAAAAAAH1SZXR1cm5zIHBhZ2luYXRlZCBhcmNoaXZlZCBwYXJ0aWNpcGF0aW9uIGhpc3RvcnkgZm9yIGEgdXNlciAobmV3ZXN0IGZpcnN0KS4KUmVqZWN0cyBpZiBgbGltaXRgIGV4Y2VlZHMgYE1BWF9QQUdFX1NJWkVgICgxMDApLgAAAAAAABhnZXRfdXNlcl9hcmNoaXZlX2hpc3RvcnkAAAADAAAAAAAAAAR1c2VyAAAAEwAAAAAAAAAGb2Zmc2V0AAAAAAAEAAAAAAAAAAVsaW1pdAAAAAAAAAQAAAABAAAD6QAAA+oAAAfQAAAAFEFyY2hpdmVkUm91bmRTdW1tYXJ5AAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAIZGcmVlemVzIHRoZSBhY3RpdmUgc2Vhc29uJ3MgcmFua2luZ3MgaW50byBhIHBlcm1hbmVudCBhcmNoaXZlIGFuZAphZHZhbmNlcyB0byB0aGUgbmV4dCBzZWFzb24gKGFkbWluIG9ubHkpLiBSZXR1cm5zIHRoZSBuZXcgc2Vhc29uIGlkLgAAAAAAGHJlc2V0X2xlYWRlcmJvYXJkX3NlYXNvbgAAAAAAAAABAAAD6QAAAAQAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAYc2V0X2Nsb3NlX2J1ZmZlcl9sZWRnZXJzAAAAAQAAAAAAAAAOYnVmZmVyX2xlZGdlcnMAAAAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAAYc2V0X21heF9wZW5kaW5nX3dpbm5pbmdzAAAAAQAAAAAAAAALbWF4X3BlbmRpbmcAAAAD6AAAAAsAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAMRTZXRzIHRoZSBtdWx0aS1mZWVkIG9yYWNsZSBxdW9ydW0gY29uZmlndXJhdGlvbiAoYWRtaW4gb25seSkuCgpXaGVuIGBTb21lKGNvbmZpZylgLCBgcmVzb2x2ZV9yb3VuZF9tdWx0aWAgaXMgZW5hYmxlZC4gV2hlbiBgTm9uZWAsCm11bHRpLWZlZWQgcmVzb2x1dGlvbiBpcyBkaXNhYmxlZC4gVGhlIGxlZ2FjeSBwYXRoIGlzIHVuYWZmZWN0ZWQuAAAAGHNldF9vcmFjbGVfcXVvcnVtX2NvbmZpZwAAAAEAAAAAAAAABmNvbmZpZwAAAAAD6AAAB9AAAAAST3JhY2xlUXVvcnVtQ29uZmlnAAAAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAL1DcmVhdGVzIHRoZSBuZXh0IHJvdW5kIGZyb20gdGhlIGNvbmZpZ3VyZWQgdGVtcGxhdGUgKGFkbWluIG9ubHkpLgpGYWlscyB3aXRoIGBSb3VuZEFscmVhZHlBY3RpdmVgIGlmIGEgcm91bmQgaXMgYWxyZWFkeSBhY3RpdmUgYW5kCndpdGggYE5vUm91bmRUZW1wbGF0ZWAgaWYgbm8gdGVtcGxhdGUgaGFzIGJlZW4gY29uZmlndXJlZC4AAAAAAAAZY3JlYXRlX25leHRfZnJvbV90ZW1wbGF0ZQAAAAAAAAAAAAABAAAD6QAAAAYAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAIBDdXJzb3ItYmFzZWQgcGFnZSBvZiB0aGUgZ2xvYmFsIGxlYWRlcmJvYXJkIG9yZGVyZWQgYnkgYmVzdCBzdHJlYWsgZGVzY2VuZGluZy4KUmVqZWN0cyBpZiBgbGltaXRgIGV4Y2VlZHMgYE1BWF9QQUdFX1NJWkVgICgxMDApLgAAABlnZXRfbGVhZGVyYm9hcmRfYnlfc3RyZWFrAAAAAAAAAgAAAAAAAAAGY3Vyc29yAAAAAAPoAAAAEwAAAAAAAAAFbGltaXQAAAAAAAAEAAAAAQAAA+kAAAPtAAAAAgAAA+oAAAfQAAAAEExlYWRlcmJvYXJkRW50cnkAAAPoAAAAEwAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAFBSZXR1cm5zIHRoZSBjb25maWd1cmVkIG9yYWNsZSB0aW1lc3RhbXAgc2tldywgb3IgdGhlIGRlZmF1bHQgKDMwMCBzKSBpZiBub3Qgc2V0LgAAABlnZXRfb3JhY2xlX3RpbWVzdGFtcF9za2V3AAAAAAAAAAAAAAEAAAAG",
        "AAAAAAAAAAAAAAAZZ2V0X3BlbmRpbmdfY29uZmlnX2NoYW5nZQAAAAAAAAEAAAAAAAAABGtpbmQAAAfQAAAAEENvbmZpZ0NoYW5nZUtpbmQAAAABAAAD6AAAB9AAAAATUGVuZGluZ0NvbmZpZ0NoYW5nZQA=",
        "AAAAAAAAAAAAAAAZZ2V0X3ByZWNpc2lvbl9wcmVkaWN0aW9ucwAAAAAAAAAAAAABAAAD6gAAB9AAAAATUHJlY2lzaW9uUHJlZGljdGlvbgA=",
        "AAAAAAAAAAAAAAAZZ2V0X3Byb3RvY29sX2ZlZV90cmVhc3VyeQAAAAAAAAAAAAABAAAACw==",
        "AAAAAAAAAAAAAAAZZ2V0X3VwZG93bl9wb3NpdGlvbnNfcGFnZQAAAAAAAAIAAAAAAAAABm9mZnNldAAAAAAABAAAAAAAAAAFbGltaXQAAAAAAAAEAAAAAQAAA+oAAAPtAAAAAgAAABMAAAfQAAAADFVzZXJQb3NpdGlvbg==",
        "AAAAAAAAAAAAAAAZaXNfYWNjZXNzX2NvbnRyb2xfZW5hYmxlZAAAAAAAAAAAAAABAAAAAQ==",
        "AAAAAAAAAAAAAAAZc2NoZWR1bGVfcHJvdG9jb2xfZmVlX2JwcwAAAAAAAAEAAAAAAAAAA2JwcwAAAAPoAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAADZSZXR1cm5zIHRoZSBjb25maWd1cmVkIGluc3VyYW5jZSBjb3ZlcmFnZSBwYXlvdXQgcmF0ZS4AAAAAABpnZXRfaW5zdXJhbmNlX2NvdmVyYWdlX2JwcwAAAAAAAAAAAAEAAAAE",
        "AAAAAAAAACtSZXR1cm5zIHRoZSBjdXJyZW50IGluc3VyYW5jZSBmdW5kIGJhbGFuY2UuAAAAABpnZXRfaW5zdXJhbmNlX2Z1bmRfYmFsYW5jZQAAAAAAAAAAAAEAAAAL",
        "AAAAAAAAAFJSZXR1cm5zIHRoZSBjb25maWd1cmVkIG9yYWNsZSBzdGFsZSB0aHJlc2hvbGQsIG9yIHRoZSBkZWZhdWx0ICgzNjAwIHMpIGlmIG5vdCBzZXQuAAAAAAAaZ2V0X29yYWNsZV9zdGFsZV90aHJlc2hvbGQAAAAAAAAAAAABAAAABg==",
        "AAAAAAAAAAAAAAAaZ2V0X3JlY2VudF9hcmNoaXZlZF9yb3VuZHMAAAAAAAEAAAAAAAAABWxpbWl0AAAAAAAABAAAAAEAAAPqAAAH0AAAABRBcmNoaXZlZFJvdW5kU3VtbWFyeQ==",
        "AAAAAAAAAAAAAAAacGxhY2VfcHJlY2lzaW9uX3ByZWRpY3Rpb24AAAAAAAMAAAAAAAAABHVzZXIAAAATAAAAAAAAAAZhbW91bnQAAAAAAAsAAAAAAAAAD3ByZWRpY3RlZF9wcmljZQAAAAAKAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAAac2NoZWR1bGVfbWF4X3VzZXJfZXhwb3N1cmUAAAAAAAEAAAAAAAAADG1heF9leHBvc3VyZQAAA+gAAAALAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAAac2V0X2FjY2Vzc19jb250cm9sX2VuYWJsZWQAAAAAAAEAAAAAAAAAB2VuYWJsZWQAAAAAAQAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAEVTZXRzIHRoZSBpbnN1cmFuY2UgY292ZXJhZ2UgcGF5b3V0IHJhdGUgaW4gYmFzaXMgcG9pbnRzIChhZG1pbiBvbmx5KS4AAAAAAAAac2V0X2luc3VyYW5jZV9jb3ZlcmFnZV9icHMAAAAAAAEAAAAAAAAAA2JwcwAAAAAEAAAAAQAAA+kAAAPtAAAAAAAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAC1TY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHN0YWxlIHRocmVzaG9sZCB1cGRhdGUAAAAAAAAac2V0X29yYWNsZV9zdGFsZV90aHJlc2hvbGQAAAAAAAEAAAAAAAAAB3NlY29uZHMAAAAABgAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAbZ2V0X3BlbmRpbmdfd2lubmluZ3NfZXhwaXJ5AAAAAAAAAAABAAAABA==",
        "AAAAAAAAAAAAAAAbZ2V0X3ByZWNpc2lvbl9wYXlvdXRfcG9saWN5AAAAAAAAAAABAAAABA==",
        "AAAAAAAAAAAAAAAbc2V0X3BlbmRpbmdfd2lubmluZ3NfZXhwaXJ5AAAAAAEAAAAAAAAAB2xlZGdlcnMAAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAbc2V0X3ByZWNpc2lvbl9wYXlvdXRfcG9saWN5AAAAAAEAAAAAAAAABnBvbGljeQAAAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAEBSZXR1cm5zIHRoZSBjb25maWd1cmVkIFRXQVAgd2luZG93IHNpemUgaW4gc2FtcGxlcyAoSXNzdWUgIzI2NikuAAAAHGdldF9kZXZpYXRpb25fd2luZG93X3NhbXBsZXMAAAAAAAAAAQAAAAQ=",
        "AAAAAAAAADhSZXR1cm5zIHRoZSBjb25maWd1cmVkIG9yYWNsZSBtYXggZGV2aWF0aW9uIGJwcywgaWYgc2V0LgAAABxnZXRfb3JhY2xlX21heF9kZXZpYXRpb25fYnBzAAAAAAAAAAEAAAPoAAAABA==",
        "AAAAAAAAADVSZXR1cm5zIHRoZSBwZW5kaW5nIG9yYWNsZSByb3RhdGlvbiBwcm9wb3NhbCwgaWYgYW55LgAAAAAAABxnZXRfb3JhY2xlX3JvdGF0aW9uX3Byb3Bvc2FsAAAAAAAAAAEAAAPoAAAH0AAAABZPcmFjbGVSb3RhdGlvblByb3Bvc2FsAAA=",
        "AAAAAAAAAC5TY2hlZHVsZXMgYSB0aW1lbG9ja2VkIG9yYWNsZSBkZXZpYXRpb24gdXBkYXRlAAAAAAAcc2V0X29yYWNsZV9tYXhfZGV2aWF0aW9uX2JwcwAAAAEAAAAAAAAAA2JwcwAAAAPoAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAFlBcm1zIGEgb25lLXNob3Qgb3ZlcnJpZGUgdG8gYnlwYXNzIGRldmlhdGlvbiBjaGVja3MgZm9yIHRoZSBuZXh0IHNldHRsZW1lbnQgKGFkbWluIG9ubHkpLgAAAAAAAB1hcm1fb3JhY2xlX2RldmlhdGlvbl9vdmVycmlkZQAAAAAAAAAAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAEBSZXR1cm5zIHRoZSBsaXN0IG9mIGVsaWdpYmxlIGluc3VyYW5jZSBldmVudCB0eXBlIGRpc2NyaW1pbmFudHMuAAAAHWdldF9pbnN1cmFuY2VfZWxpZ2libGVfZXZlbnRzAAAAAAAAAAAAAAEAAAPqAAAABA==",
        "AAAAAAAAAD1SZXR1cm5zIHRoZSBjb25maWd1cmVkIG1pbmltdW0gb3JhY2xlIGNvbmZpZGVuY2UgYnBzLCBpZiBzZXQuAAAAAAAAHWdldF9vcmFjbGVfbWluX2NvbmZpZGVuY2VfYnBzAAAAAAAAAAAAAAEAAAPoAAAABA==",
        "AAAAAAAAAAAAAAAdZ2V0X3VzZXJfcHJlY2lzaW9uX3ByZWRpY3Rpb24AAAAAAAABAAAAAAAAAAR1c2VyAAAAEwAAAAEAAAPoAAAH0AAAABNQcmVjaXNpb25QcmVkaWN0aW9uAA==",
        "AAAAAAAAAAAAAAAdc2NoZWR1bGVfbWF4X3BlbmRpbmdfd2lubmluZ3MAAAAAAAABAAAAAAAAAAttYXhfcGVuZGluZwAAAAPoAAAACwAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAdc2NoZWR1bGVfb3JhY2xlX2RldmlhdGlvbl9icHMAAAAAAAABAAAAAAAAAANicHMAAAAD6AAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAEJTZXRzIHRoZSB3aGl0ZWxpc3Qgb2YgZWxpZ2libGUgaW5zdXJhbmNlIGV2ZW50IHR5cGVzIChhZG1pbiBvbmx5KS4AAAAAAB1zZXRfaW5zdXJhbmNlX2VsaWdpYmxlX2V2ZW50cwAAAAAAAAEAAAAAAAAABmV2ZW50cwAAAAAD6gAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAEpTZXRzIHRoZSBtaW5pbXVtIG9yYWNsZSBjb25maWRlbmNlIHRocmVzaG9sZCBpbiBiYXNpcyBwb2ludHMgKGFkbWluIG9ubHkpLgAAAAAAHXNldF9vcmFjbGVfbWluX2NvbmZpZGVuY2VfYnBzAAAAAAAAAQAAAAAAAAAHbWluX2JwcwAAAAPoAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAeZ2V0X21heF9wcmVjaXNpb25fcGFydGljaXBhbnRzAAAAAAAAAAAAAQAAAAQ=",
        "AAAAAAAAAAAAAAAeZ2V0X3ByZWNpc2lvbl9wcmVkaWN0aW9uc19wYWdlAAAAAAACAAAAAAAAAAZvZmZzZXQAAAAAAAQAAAAAAAAABWxpbWl0AAAAAAAABAAAAAEAAAPqAAAH0AAAABNQcmVjaXNpb25QcmVkaWN0aW9uAA==",
        "AAAAAAAAAG5QYWdpbmF0ZWQgd2lucyBsZWFkZXJib2FyZCBmb3IgYHNlYXNvbl9pZGAg4oCUIGxpdmUgZm9yIHRoZSBhY3RpdmUKc2Vhc29uLCBmcm96ZW4gYXJjaGl2ZSBmb3IgYW55IHBhc3Qgc2Vhc29uLgAAAAAAHmdldF9zZWFzb25fbGVhZGVyYm9hcmRfYnlfd2lucwAAAAAAAwAAAAAAAAAJc2Vhc29uX2lkAAAAAAAABAAAAAAAAAAGb2Zmc2V0AAAAAAAEAAAAAAAAAAVsaW1pdAAAAAAAAAQAAAABAAAD6gAAB9AAAAAWU2Vhc29uTGVhZGVyYm9hcmRFbnRyeQAA",
        "AAAAAAAAAEhTY2hlZHVsZXMgYSB0aW1lbG9ja2VkIHVwZGF0ZSB0byB0aGUgb3JhY2xlIHRpbWVzdGFtcCBza2V3IChhZG1pbiBvbmx5KS4AAAAec2NoZWR1bGVfb3JhY2xlX3RpbWVzdGFtcF9za2V3AAAAAAABAAAAAAAAAAdzZWNvbmRzAAAAAAYAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAAAAAAAec2V0X21heF9wcmVjaXNpb25fcGFydGljaXBhbnRzAAAAAAABAAAAAAAAAANtYXgAAAAABAAAAAEAAAPpAAAD7QAAAAAAAAfQAAAADUNvbnRyYWN0RXJyb3IAAAA=",
        "AAAAAAAAAAAAAAAfZ2V0X3VzZXJfYXJjaGl2ZWRfcGFydGljaXBhdGlvbgAAAAACAAAAAAAAAAR1c2VyAAAAEwAAAAAAAAAIcm91bmRfaWQAAAAGAAAAAQAAA+gAAAfQAAAAEFVzZXJSb3VuZE91dGNvbWU=",
        "AAAAAAAAAAAAAAAfc2NoZWR1bGVfb3JhY2xlX3N0YWxlX3RocmVzaG9sZAAAAAABAAAAAAAAAAdzZWNvbmRzAAAAAAYAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA",
        "AAAAAAAAAHVQYWdpbmF0ZWQgYmVzdC1zdHJlYWsgbGVhZGVyYm9hcmQgZm9yIGBzZWFzb25faWRgIOKAlCBsaXZlIGZvciB0aGUKYWN0aXZlIHNlYXNvbiwgZnJvemVuIGFyY2hpdmUgZm9yIGFueSBwYXN0IHNlYXNvbi4AAAAAAAAgZ2V0X3NlYXNvbl9sZWFkZXJib2FyZF9ieV9zdHJlYWsAAAADAAAAAAAAAAlzZWFzb25faWQAAAAAAAAEAAAAAAAAAAZvZmZzZXQAAAAAAAQAAAAAAAAABWxpbWl0AAAAAAAABAAAAAEAAAPqAAAH0AAAABZTZWFzb25MZWFkZXJib2FyZEVudHJ5AAA=",
        "AAAAAAAAAAAAAAAgcmVjbGFpbV9leHBpcmVkX3BlbmRpbmdfd2lubmluZ3MAAAABAAAAAAAAAAR1c2VyAAAAEwAAAAEAAAPpAAAACwAAB9AAAAANQ29udHJhY3RFcnJvcgAAAA==",
        "AAAAAAAAAAAAAAAgc2NoZWR1bGVfcGVuZGluZ193aW5uaW5nc19leHBpcnkAAAABAAAAAAAAAAdsZWRnZXJzAAAAAAQAAAABAAAD6QAAA+0AAAAAAAAH0AAAAA1Db250cmFjdEVycm9yAAAA" ]),
      options
    )
  }
  public readonly fromJSON = {
    balance: this.txFromJSON<i128>,
        get_admin: this.txFromJSON<Option<string>>,
        is_paused: this.txFromJSON<boolean>,
        place_bet: this.txFromJSON<Result<void>>,
        claim_many: this.txFromJSON<Result<Array<i128>>>,
        get_oracle: this.txFromJSON<Option<string>>,
        initialize: this.txFromJSON<Result<void>>,
        void_round: this.txFromJSON<Result<void>>,
        get_min_bet: this.txFromJSON<Option<i128>>,
        set_min_bet: this.txFromJSON<Result<void>>,
        set_windows: this.txFromJSON<Result<void>>,
        cancel_round: this.txFromJSON<Result<void>>,
        create_round: this.txFromJSON<Result<void>>,
        mint_initial: this.txFromJSON<i128>,
        get_amendment: this.txFromJSON<Option<Amendment>>,
        get_fee_model: this.txFromJSON<FeeModel>,
        get_max_stake: this.txFromJSON<Option<i128>>,
        is_denylisted: this.txFromJSON<boolean>,
        predict_price: this.txFromJSON<Result<void>>,
        resolve_round: this.txFromJSON<Result<void>>,
        set_fee_model: this.txFromJSON<Result<void>>,
        set_max_stake: this.txFromJSON<Result<void>>,
        add_denylisted: this.txFromJSON<Result<void>>,
        cash_out_early: this.txFromJSON<Result<void>>,
        claim_winnings: this.txFromJSON<Result<i128>>,
        finalize_round: this.txFromJSON<Result<void>>,
        get_mint_limit: this.txFromJSON<u32>,
        get_user_stats: this.txFromJSON<UserStats>,
        is_allowlisted: this.txFromJSON<boolean>,
        is_oracle_live: this.txFromJSON<boolean>,
        pause_contract: this.txFromJSON<Result<void>>,
        set_mint_limit: this.txFromJSON<Result<void>>,
        veto_amendment: this.txFromJSON<Result<void>>,
        add_allowlisted: this.txFromJSON<Result<void>>,
        arm_hb_override: this.txFromJSON<Result<void>>,
        batch_touch_ttl: this.txFromJSON<Result<u32>>,
        get_next_schema: this.txFromJSON<Option<u32>>,
        get_round_phase: this.txFromJSON<Result<RoundPhase>>,
        simulate_payout: this.txFromJSON<Result<SimulationResult>>,
        get_access_state: this.txFromJSON<AccessState>,
        get_active_round: this.txFromJSON<Option<Round>>,
        get_constitution: this.txFromJSON<Option<ConstitutionMetadata>>,
        get_gov_approver: this.txFromJSON<Option<string>>,
        get_gov_proposal: this.txFromJSON<Option<GovProposal>>,
        get_round_status: this.txFromJSON<RoundStatus>,
        get_runtime_mode: this.txFromJSON<u32>,
        get_twap_samples: this.txFromJSON<Array<PriceSample>>,
        schedule_min_bet: this.txFromJSON<Result<void>>,
        schedule_windows: this.txFromJSON<Result<void>>,
        set_gov_approver: this.txFromJSON<Result<void>>,
        set_runtime_mode: this.txFromJSON<Result<void>>,
        unpause_contract: this.txFromJSON<Result<void>>,
        clear_next_schema: this.txFromJSON<Result<void>>,
        commit_prediction: this.txFromJSON<Result<void>>,
        get_access_policy: this.txFromJSON<readonly [boolean, AccessState]>,
        get_last_round_id: this.txFromJSON<u64>,
        get_user_position: this.txFromJSON<Option<UserPosition>>,
        is_action_allowed: this.txFromJSON<boolean>,
        propose_amendment: this.txFromJSON<Result<u64>>,
        remove_denylisted: this.txFromJSON<Result<void>>,
        reveal_prediction: this.txFromJSON<Result<void>>,
        activate_amendment: this.txFromJSON<Result<void>>,
        get_archived_round: this.txFromJSON<Option<ArchivedRoundSummary>>,
        get_hb_strict_mode: this.txFromJSON<boolean>,
        get_round_template: this.txFromJSON<Option<RoundTemplate>>,
        get_schema_version: this.txFromJSON<u32>,
        get_season_archive: this.txFromJSON<Option<SeasonArchive>>,
        is_round_cancelled: this.txFromJSON<boolean>,
        propose_gov_action: this.txFromJSON<Result<u64>>,
        remove_allowlisted: this.txFromJSON<Result<void>>,
        schedule_max_stake: this.txFromJSON<Result<void>>,
        set_hb_strict_mode: this.txFromJSON<Result<void>>,
        set_round_template: this.txFromJSON<Result<void>>,
        cancel_gov_proposal: this.txFromJSON<Result<void>>,
        get_attestation_key: this.txFromJSON<Option<Buffer>>,
        get_dispute_ledgers: this.txFromJSON<u32>,
        get_market_snapshot: this.txFromJSON<MarketSnapshot>,
        get_protocol_health: this.txFromJSON<ProtocolHealthStatus>,
        get_protocol_status: this.txFromJSON<ProtocolStatus>,
        resolve_round_multi: this.txFromJSON<Result<void>>,
        set_attestation_key: this.txFromJSON<Result<void>>,
        set_dispute_ledgers: this.txFromJSON<Result<void>>,
        announce_next_schema: this.txFromJSON<Result<void>>,
        approve_gov_proposal: this.txFromJSON<Result<void>>,
        cancel_config_change: this.txFromJSON<Result<void>>,
        clear_round_template: this.txFromJSON<Result<void>>,
        execute_gov_proposal: this.txFromJSON<Result<void>>,
        get_gov_proposal_ttl: this.txFromJSON<u32>,
        get_hb_grace_seconds: this.txFromJSON<u64>,
        get_min_participants: this.txFromJSON<Option<u32>>,
        get_one_sided_policy: this.txFromJSON<OneSidedPolicy>,
        get_oracle_heartbeat: this.txFromJSON<Option<OracleHeartbeatRecord>>,
        get_pending_winnings: this.txFromJSON<i128>,
        get_protocol_fee_bps: this.txFromJSON<Option<u32>>,
        get_round_pool_stats: this.txFromJSON<Option<RoundPoolStats>>,
        get_updown_positions: this.txFromJSON<Map<string, UserPosition>>,
        set_gov_proposal_ttl: this.txFromJSON<Result<void>>,
        set_hb_grace_seconds: this.txFromJSON<Result<void>>,
        set_min_participants: this.txFromJSON<Result<void>>,
        set_protocol_fee_bps: this.txFromJSON<Result<void>>,
        get_archive_retention: this.txFromJSON<u32>,
        get_current_season_id: this.txFromJSON<u32>,
        get_early_cashout_bps: this.txFromJSON<Option<u32>>,
        get_epoch_mint_budget: this.txFromJSON<i128>,
        get_hb_override_armed: this.txFromJSON<boolean>,
        get_max_user_exposure: this.txFromJSON<Option<i128>>,
        get_season_user_stats: this.txFromJSON<UserStats>,
        set_archive_retention: this.txFromJSON<Result<void>>,
        set_early_cashout_bps: this.txFromJSON<Result<void>>,
        set_epoch_mint_budget: this.txFromJSON<Result<void>>,
        set_max_user_exposure: this.txFromJSON<Result<void>>,
        top_up_insurance_fund: this.txFromJSON<Result<void>>,
        withdraw_protocol_fee: this.txFromJSON<Result<i128>>,
        accept_oracle_rotation: this.txFromJSON<Result<void>>,
        cancel_oracle_rotation: this.txFromJSON<Result<void>>,
        establish_constitution: this.txFromJSON<Result<void>>,
        get_bet_window_ledgers: this.txFromJSON<u32>,
        get_deviation_ref_mode: this.txFromJSON<DeviationReferenceMode>,
        get_oracle_strict_mode: this.txFromJSON<boolean>,
        get_run_window_ledgers: this.txFromJSON<u32>,
        set_deviation_ref_mode: this.txFromJSON<Result<void>>,
        set_oracle_strict_mode: this.txFromJSON<Result<void>>,
        apply_scheduled_changes: this.txFromJSON<Result<void>>,
        get_insurance_split_bps: this.txFromJSON<u32>,
        get_leaderboard_by_wins: this.txFromJSON<Result<readonly [Array<LeaderboardEntry>, Option<string>]>>,
        migrate_schema_v1_to_v2: this.txFromJSON<Result<void>>,
        migrate_schema_v2_to_v3: this.txFromJSON<Result<void>>,
        propose_oracle_rotation: this.txFromJSON<Result<void>>,
        set_insurance_split_bps: this.txFromJSON<Result<void>>,
        update_oracle_heartbeat: this.txFromJSON<Result<void>>,
        withdraw_insurance_fund: this.txFromJSON<Result<i128>>,
        get_close_buffer_ledgers: this.txFromJSON<u32>,
        get_max_pending_winnings: this.txFromJSON<Option<i128>>,
        get_oracle_quorum_config: this.txFromJSON<Option<OracleQuorumConfig>>,
        get_user_archive_history: this.txFromJSON<Result<Array<ArchivedRoundSummary>>>,
        reset_leaderboard_season: this.txFromJSON<Result<u32>>,
        set_close_buffer_ledgers: this.txFromJSON<Result<void>>,
        set_max_pending_winnings: this.txFromJSON<Result<void>>,
        set_oracle_quorum_config: this.txFromJSON<Result<void>>,
        create_next_from_template: this.txFromJSON<Result<u64>>,
        get_leaderboard_by_streak: this.txFromJSON<Result<readonly [Array<LeaderboardEntry>, Option<string>]>>,
        get_oracle_timestamp_skew: this.txFromJSON<u64>,
        get_pending_config_change: this.txFromJSON<Option<PendingConfigChange>>,
        get_precision_predictions: this.txFromJSON<Array<PrecisionPrediction>>,
        get_protocol_fee_treasury: this.txFromJSON<i128>,
        get_updown_positions_page: this.txFromJSON<Array<readonly [string, UserPosition]>>,
        is_access_control_enabled: this.txFromJSON<boolean>,
        schedule_protocol_fee_bps: this.txFromJSON<Result<void>>,
        get_insurance_coverage_bps: this.txFromJSON<u32>,
        get_insurance_fund_balance: this.txFromJSON<i128>,
        get_oracle_stale_threshold: this.txFromJSON<u64>,
        get_recent_archived_rounds: this.txFromJSON<Array<ArchivedRoundSummary>>,
        place_precision_prediction: this.txFromJSON<Result<void>>,
        schedule_max_user_exposure: this.txFromJSON<Result<void>>,
        set_access_control_enabled: this.txFromJSON<Result<void>>,
        set_insurance_coverage_bps: this.txFromJSON<Result<void>>,
        set_oracle_stale_threshold: this.txFromJSON<Result<void>>,
        get_pending_winnings_expiry: this.txFromJSON<u32>,
        get_precision_payout_policy: this.txFromJSON<u32>,
        set_pending_winnings_expiry: this.txFromJSON<Result<void>>,
        set_precision_payout_policy: this.txFromJSON<Result<void>>,
        get_deviation_window_samples: this.txFromJSON<u32>,
        get_oracle_max_deviation_bps: this.txFromJSON<Option<u32>>,
        get_oracle_rotation_proposal: this.txFromJSON<Option<OracleRotationProposal>>,
        set_oracle_max_deviation_bps: this.txFromJSON<Result<void>>,
        arm_oracle_deviation_override: this.txFromJSON<Result<void>>,
        get_insurance_eligible_events: this.txFromJSON<Array<u32>>,
        get_oracle_min_confidence_bps: this.txFromJSON<Option<u32>>,
        get_user_precision_prediction: this.txFromJSON<Option<PrecisionPrediction>>,
        schedule_max_pending_winnings: this.txFromJSON<Result<void>>,
        schedule_oracle_deviation_bps: this.txFromJSON<Result<void>>,
        set_insurance_eligible_events: this.txFromJSON<Result<void>>,
        set_oracle_min_confidence_bps: this.txFromJSON<Result<void>>,
        get_max_precision_participants: this.txFromJSON<u32>,
        get_precision_predictions_page: this.txFromJSON<Array<PrecisionPrediction>>,
        get_season_leaderboard_by_wins: this.txFromJSON<Array<SeasonLeaderboardEntry>>,
        schedule_oracle_timestamp_skew: this.txFromJSON<Result<void>>,
        set_max_precision_participants: this.txFromJSON<Result<void>>,
        get_user_archived_participation: this.txFromJSON<Option<UserRoundOutcome>>,
        schedule_oracle_stale_threshold: this.txFromJSON<Result<void>>,
        get_season_leaderboard_by_streak: this.txFromJSON<Array<SeasonLeaderboardEntry>>,
        reclaim_expired_pending_winnings: this.txFromJSON<Result<i128>>,
        schedule_pending_winnings_expiry: this.txFromJSON<Result<void>>
  }
}

/** Stable contract error codes retained alongside the generated ABI client. */
export const ContractError = {
  1: {message:"AlreadyInitialized"},
  2: {message:"AdminNotSet"},
  3: {message:"OracleNotSet"},
  6: {message:"InvalidBetAmount"},
  7: {message:"NoActiveRound"},
  8: {message:"RoundEnded"},
  9: {message:"InsufficientBalance"},
  10: {message:"AlreadyBet"},
  11: {message:"Overflow"},
  12: {message:"InvalidPrice"},
  13: {message:"InvalidDuration"},
  14: {message:"InvalidMode"},
  15: {message:"WrongModeForPrediction"},
  16: {message:"RoundNotEnded"},
  18: {message:"StaleOracleData"},
  19: {message:"InvalidOracleRound"},
  20: {message:"RoundAlreadyActive"},
  22: {message:"ContractPaused"},
  23: {message:"WindowOutOfRange"},
  24: {message:"FutureOracleData"},
  25: {message:"PayoutOverflow"},
  27: {message:"RoundNotCancellable"},
  28: {message:"StakeExceedsMax"},
  29: {message:"ExposureCapExceeded"},
  30: {message:"PendingWinningsCapExceeded"},
  31: {message:"InvalidStartPrice"},
  33: {message:"OracleNonceReused"},
  35: {message:"InvalidMinParticipants"},
  38: {message:"InvalidPrecisionCap"},
  39: {message:"PrecisionCapExceeded"},
  41: {message:"OracleDeviationExceeded"},
  42: {message:"UnsupportedSchemaVersion"},
  44: {message:"MigrationActiveRound"},
  45: {message:"CommitmentNotFound"},
  46: {message:"AlreadyRevealed"},
  47: {message:"InvalidRevealWindow"},
  48: {message:"HashMismatch"},
  49: {message:"OracleNetworkMismatch"},
  51: {message:"InvalidProtocolFeeBps"},
  53: {message:"MintLimitExceeded"},
  54: {message:"NoPendingRotation"},
  55: {message:"RotationDelayNotElapsed"},
  62: {message:"InvalidArchiveRetention"},
  63: {message:"InvalidCommitment"},
  64: {message:"InvalidSalt"},
  65: {message:"NoRoundTemplate"},
  66: {message:"OracleTimestampOutsideWindow"},
  86: {message:"PendingWinningsNotExpired"},
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
  79: {message:"AccessDenied"},
  80: {message:"ProposalNotFound"},
  81: {message:"ProposalExpired"},
  82: {message:"GovInvalidState"},
  83: {message:"GovUnauthorized"},
  84: {message:"IllegalPhaseTransition"},
  85: {message:"OracleHeartbeatUnhealthy"},
  87: {message:"ClaimBatchTooLarge"},
  88: {message:"DuplicateClaimAddress"},
  95: {message:"EarlyCashoutDisabled"},
  96: {message:"PositionNotFound"},
  97: {message:"InvalidPhaseForCashout"},
  98: {message:"WrongModeForCashout"},
  99: {message:"InsuranceInvalidSplit"},
  100: {message:"InsuranceInsufficientFund"},
  101: {message:"InvalidAmount"},
  91: {message:"DisputeWindowExpired"},
  92: {message:"ClaimLocked"},
  93: {message:"RoundStartLedgerReused"},
  94: {message:"PageSizeExceeded"},
};

export function decodeContractError(code: number): { code: number; variant: string; message: string } | undefined {
  const entry = ContractError[code as keyof typeof ContractError];
  return entry ? { code, variant: entry.message, message: entry.message } : undefined;
}

export function formatContractError(code: number): string | undefined {
  const decoded = decodeContractError(code);
  return decoded ? `${decoded.variant} (code ${decoded.code})` : undefined;
}

export function ContractErrorDecoder(code: number): string {
  return formatContractError(code) ?? `Unknown contract error (code ${code})`;
}
