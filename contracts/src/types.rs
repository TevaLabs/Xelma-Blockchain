// SPDX-License-Identifier: MIT
// Type definitions for the XLM Price Prediction Market.

use soroban_sdk::{contracttype, Address, BytesN, Env, IntoVal, Symbol, Val, Vec};

/// Round mode for prediction type
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum RoundMode {
    UpDown = 0,    // Simple up/down predictions
    Precision = 1, // Exact price predictions (Legends mode)
}

/// Payout policy for Precision mode
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum PrecisionPayoutPolicy {
    Equal = 0,         // Split payout pool equally among winners (default)
    StakeWeighted = 1, // Split payout pool proportionally to winner stakes
}

/// Access control state for an address (Issue #274)
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum AccessState {
    Open = 0,
    Allowlisted = 1,
    Denylisted = 2,
}

/// Runtime mode for the contract lifecycle
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum RuntimeMode {
    Normal = 0,
    ClaimsOnly = 1,
    FullyPaused = 2,
}

/// Lifecycle phase of an active round, derived from ledger windows.
///
/// Semantics (given `start_ledger`, `bet_end_ledger`, `end_ledger`):
/// - `Betting`: `ledger < bet_end_ledger` — bets and precision predictions accepted
/// - `Running`: `bet_end_ledger ≤ ledger < end_ledger` — reveal window (precision)
/// - `Resolvable`: `ledger ≥ end_ledger` — round may be settled via oracle payload
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum RoundPhase {
    Betting = 1,
    Running = 2,
    Resolvable = 3,
}

/// Parameterless system, config, and metadata storage keys.
///
/// Split from `DataKey` to stay under the XDR union 50-case limit
/// (`VecM<ScSpecUdtUnionCaseV0, 50>` in stellar-xdr).
#[contracttype]
#[derive(Clone)]
pub enum DataKeyCore {
    Admin,
    Oracle,
    /// On-chain storage schema version for migration safety.
    SchemaVersion,
    ActiveRound,
    Positions,          // Legacy key — read-only migration compat
    UpDownPositions,    // Legacy key — read-only migration compat
    PrecisionPositions, // Legacy key — read-only migration compat
    Paused,
    BetWindowLedgers,
    RunWindowLedgers,
    CloseBufferLedgers,
    LastRoundId,
    /// Maximum stake allowed per individual bet (None = unlimited)
    MaxStake,
    /// Maximum cumulative exposure per user per round (None = unlimited)
    MaxUserRoundExposure,
    /// Maximum pending winnings allowed per account (None = unlimited)
    MaxPendingWinnings,
    /// Minimum participant count for competitive settlement; unset = no minimum enforced
    MinParticipants,
    /// Oracle heartbeat: last recorded timestamp and status
    OracleHeartbeat,
    /// Stale-heartbeat threshold in seconds (admin-configurable); unset = 3600 s default
    OracleStaleThreshold,
    /// Maximum participants accepted in a Precision round; unset = protocol default
    MaxPrecisionParticipants,
    /// Oracle max deviation threshold in basis points (1 bp = 0.01%).
    OracleMaxDeviationBps,
    /// One-shot admin override allowing the next settlement to bypass deviation checks.
    OracleDeviationOverrideArmed,
    /// Minimum oracle confidence threshold in basis points (0–10000).
    OracleMinConfidenceBps,
    /// When true, payloads with missing confidence are rejected in strict mode.
    OracleStrictMode,
    /// Ordered round ids for archive retention (oldest at index 0).
    RecentArchivedRoundIds,
    /// Marker written by migrate_schema_v2_to_v3 to prove the migration ran.
    MigratedToV3,
    /// Optional protocol settlement fee in basis points (1 bp = 0.01%).
    ProtocolFeeBps,
    /// On-chain accumulated protocol fee balance in stroops (i128).
    ProtocolFeeTreasury,
    /// Mint limit configuration: maximum number of mints allowed per ledger.
    MintLimitConfig,
    /// Pending two-step oracle rotation proposal with expiry.
    OracleRotationProposal,
    /// Configurable archive retention limit: maximum number of ArchivedRound entries.
    ArchiveRetention,
    /// Admin-configured blueprint used by `create_next_from_template`.
    RoundTemplate,
    /// Bounded index of user addresses sorted by lifetime total wins.
    LeaderboardWins,
    /// Bounded index of user addresses sorted by lifetime best streak.
    LeaderboardStreak,
    /// Monotonically increasing id of the currently-active leaderboard season.
    SeasonId,
    /// Bounded index of user addresses in the active season sorted by wins.
    SeasonLeaderboardWins,
    /// Bounded index of user addresses in the active season sorted by streak.
    SeasonLeaderboardStreak,
    /// Multi-feed oracle quorum configuration.
    OracleQuorum,
    /// Minimum bet amount.
    MinBet,
    /// Maximum mint budget per epoch.
    EpochMintBudget,
    /// Early cashout fee in basis points.
    EarlyCashoutBps,
    /// Precision payout distribution policy: Equal (0) or StakeWeighted (1).
    PrecisionPayoutPolicy,
    /// Fee incidence model: FeeOnPot (0) or FeeOnWinnings (1).
    FeeModel,
    /// Dispute window length in ledgers.
    DisputeLedgers,
    /// Staged schema version for migration readiness.
    NextSchemaVersion,
    /// Access control enforcement flag.
    AccessControlEnabled,
    /// Governance secondary approver address.
    GovApprover,
    /// Governance proposal time-to-live in ledgers.
    GovProposalTtlLedgers,
    /// Next monotonic governance proposal identifier.
    NextGovProposalId,
    /// List of currently open governance proposal identifiers.
    OpenGovProposalIds,
}

/// Parameterised and round-scoped storage keys.
///
/// Split from `DataKey` to stay under the XDR union 50-case limit.
#[contracttype]
#[derive(Clone)]
pub enum DataKeyScoped {
    Balance(Address),
    PendingWinnings(Address),
    UserStats(Address),
    Position(u64, Address),
    PrecisionPosition(u64, Address),
    PrecisionCommitment(u64, Address),
    RoundParticipants(u64),
    CancelledRound(u64),
    ConsumedOracleNonce(u64, u64),
    UserRoundOutcome(u64, Address),
    PendingConfigChange(ConfigChangeKind),
    LedgerMintCounter(u32),
    ArchivedRound(u64),
    SeasonUserStats(u32, Address),
    SeasonArchive(u32),
    UserArchivedRoundIds(Address),
    GovProposal(u64),
    Allowlisted(Address),
    Denylisted(Address),
}

pub type DataKey = DataKeyCore;

/// Deterministic settlement policy governing degenerate (one-sided) market rounds.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum OneSidedPolicy {
    /// Full stake refund to all participants (active protocol policy).
    Refund = 0,
    /// Void round releasing stakes without mutating stats.
    Void = 1,
    /// Carry-forward pool stakes to subsequent round (extensibility placeholder).
    CarryForward = 2,
}

pub type Policy = OneSidedPolicy;

/// Identifies which critical risk setting is pending timelocked activation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum ConfigChangeKind {
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

/// Payload for a scheduled critical config change.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigChangePayload {
    Windows(u32, u32),
    MaxStake(Option<i128>),
    MaxUserRoundExposure(Option<i128>),
    MaxPendingWinnings(Option<i128>),
    OracleStaleThreshold(u64),
    OracleMaxDeviationBps(Option<u32>),
    ProtocolFeeBps(Option<u32>),
    MinParticipants(Option<u32>),
    MaxPrecisionParticipants(u32),
    MintLimit(u32),
    ArchiveRetention(u32),
    CloseBufferLedgers(u32),
    OracleTimestampSkew(u64),
    EpochMintBudget(i128),
    PendingWinningsExpiry(u32),
    PrecisionPayoutPolicy(u32),
    MinBet(Option<i128>),
    DisputeLedgers(u32),
    FeeModel(FeeModel),
    EarlyCashoutBps(Option<u32>),
}

/// Pending timelocked config change with activation ledger for on-chain observability.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingConfigChange {
    pub payload: ConfigChangePayload,
    pub activation_ledger: u32,
    pub scheduled_at_ledger: u32,
}

/// Represents which side a user bet on
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum BetSide {
    Up,
    Down,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct UserPosition {
    pub amount: i128,
    pub side: BetSide,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct UserStats {
    pub total_wins: u32,
    pub total_losses: u32,
    pub current_streak: u32,
    pub best_streak: u32,
}

/// Precision prediction entry (user address + predicted price)
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PrecisionPrediction {
    pub user: Address,
    pub predicted_price: u128, // Price scaled to 4 decimals (e.g., 0.2297 → 2297)
    pub amount: i128,          // Bet amount
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PrecisionCommitment {
    pub hash: BytesN<32>,
    pub amount: i128,
    pub revealed: bool,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OraclePayload {
    pub price: u128,
    pub timestamp: u64,
    /// Round identifier that should match `Round.start_ledger`
    pub round_id: u32,
    /// Per-round replay-protection nonce.
    pub nonce: u64,
    /// SHA-256 hash of the network passphrase this payload targets.
    pub network_id: BytesN<32>,
    /// Contract address this payload is intended for.
    pub contract_addr: Address,
    /// Optional confidence score from the price feed (0–10000 bps, where 10000 = 100%).
    pub confidence: Option<u32>,
    /// Optional ed25519 signature over the attestation domain-separated message.
    pub attestation: Option<BytesN<64>>,
}

/// Multi-feed oracle resolution payload (N observations, quorum + median).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MultiFeedPayload {
    /// Prices from each feed, scaled to 4 decimal places (e.g. 2297 = $0.2297).
    pub prices: Vec<u128>,
    /// Feed source identifiers (0-based index, max N-1). Must be unique.
    pub sources: Vec<u32>,
    /// Round identifier that must match `Round.start_ledger`
    pub round_id: u32,
    /// Per-round replay-protection nonce.
    pub nonce: u64,
    /// SHA-256 hash of the network passphrase this payload targets.
    pub network_id: BytesN<32>,
    /// Contract address this payload is intended for.
    pub contract_addr: Address,
    /// Unix epoch seconds when the observations were collected.
    pub timestamp: u64,
}

/// Admin-configured quorum and outlier rejection parameters for multi-feed
/// oracle settlement.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleQuorumConfig {
    pub min_observations: u32,
    pub quorum_threshold: u32,
    pub outlier_threshold_bps: u32,
}

/// Oracle liveness record, updated by the oracle service on each heartbeat call.
/// `status`: 0 = active, 1 = degraded, 2 = offline.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleHeartbeatRecord {
    pub timestamp: u64,
    pub status: u32,
}

/// Heartbeat health gate configuration (Issue #264).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct HbGateConfig {
    pub strict_mode: bool,
    pub override_armed: bool,
    pub grace_seconds: u64,
}

/// Storage key for heartbeat gate config (separate from DataKey to stay within variant limits, Issue #264).
#[contracttype]
#[derive(Clone)]
pub enum HbGateKey {
    Config,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    pub round_id: u64,       // Unique monotonically increasing round identifier
    pub price_start: u128,   // Starting XLM price in stroops
    pub start_ledger: u32,   // Ledger when round was created
    pub bet_end_ledger: u32, // Ledger when betting closes
    pub end_ledger: u32,     // Ledger when round ends (~5s per ledger)
    pub pool_up: i128,       // Total vXLM bet on UP
    pub pool_down: i128,     // Total vXLM bet on DOWN
    pub mode: RoundMode,     // Round mode: UpDown (0) or Precision (1)
    pub start_timestamp: u64, // Ledger timestamp when round was created
}

/// Aggregated active-round pool composition for frontend transparency.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RoundPoolStats {
    pub round_id: u64,
    pub mode: RoundMode,
    pub total_up_stake: i128,
    pub total_down_stake: i128,
    pub up_participant_count: u32,
    pub down_participant_count: u32,
    pub up_stake_ratio_bps: u32,
    pub down_stake_ratio_bps: u32,
    pub precision_total_stake: i128,
    pub precision_participant_count: u32,
    pub precision_prediction_count: u32,
    pub precision_commitment_count: u32,
    pub precision_revealed_count: u32,
}

/// Terminal outcome recorded when a round leaves the active state.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum RoundArchiveStatus {
    Resolved = 0,
    Cancelled = 1,
    FallbackRefund = 2,
    Voided = 3,
}

/// Composite protocol health status returned by `get_protocol_health`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProtocolHealthStatus {
    pub paused: bool,
    pub oracle_live: bool,
    pub oracle_status: u32,
    pub has_active_round: bool,
    pub active_round_phase: u32,
    pub schema_version: u32,
    pub ledger_sequence: u32,
    pub ledger_timestamp: u64,
    pub status_code: u32,
}

/// Compact historical round summary persisted after resolve or cancel.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ArchivedRoundSummary {
    pub round_id: u64,
    pub price_start: u128,
    pub price_final: u128,
    pub mode: RoundMode,
    pub status: RoundArchiveStatus,
    pub pool_up: i128,
    pub pool_down: i128,
    pub participant_count: u32,
    pub settled_at_ledger: u32,
}

/// Pending two-step oracle rotation proposal.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleRotationProposal {
    pub new_oracle: Address,
    pub proposed_at: u64,
    pub expires_at: u64,
}

/// Global status of the protocol, returned by `get_protocol_status`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum ProtocolStatus {
    Active = 0,
    Paused = 1,
    ClaimsOnly = 2,
}

/// Status of a specific round, returned by `get_round_status(round_id)`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum RoundStatus {
    Unknown = 0,
    Betting = 1,
    Running = 2,
    AwaitingResolve = 3,
    Resolved = 4,
    Cancelled = 5,
    FallbackRefund = 6,
    Voided = 7,
}

/// Terminal outcome persisted per user per archived round.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum UserOutcomeType {
    Win = 0,
    Loss = 1,
    Refund = 2,
    Void = 3,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct UserRoundOutcome {
    pub user: Address,
    pub round_mode: u32,
    pub prediction_side: u32,
    pub predicted_price: u128,
    pub stake: i128,
    pub payout: i128,
    pub outcome: UserOutcomeType,
}

/// Simulated payout result for a specific hypothetical final price.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SimulationResult {
    pub mode: RoundMode,
    pub pool_up: i128,
    pub pool_down: i128,
    pub precision_total_stake: i128,
    pub fee_amount: i128,
    pub outcomes: Vec<UserRoundOutcome>,
    pub fee_model: u32,
}

/// Per-participant entry in a deferred settlement (dispute window active).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedParticipant {
    pub user: Address,
    pub amount: i128,
    pub payout: i128,
    pub predicted_price: u128,
    pub prediction_side: u32,
    pub outcome: UserOutcomeType,
}

/// Settlement data stored during dispute-window resolve and consumed by
/// `finalize_round` (window expired → winners paid) or `void_round`
/// (void → all refunded).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RoundSettlement {
    pub round_id: u64,
    pub mode: u32,
    pub final_price: u128,
    pub price_start: u128,
    pub pool_up: i128,
    pub pool_down: i128,
    pub participants: Vec<ResolvedParticipant>,
    pub fee_amount: i128,
}

/// Admin-configured blueprint for `create_next_from_template`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RoundTemplate {
    pub start_price: u128,
    pub mode: Option<u32>,
}

/// A single entry in the lifetime (all-time) leaderboard.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct LeaderboardEntry {
    pub user: Address,
    pub stats: UserStats,
}

/// A single entry in a season-scoped leaderboard, live or archived.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SeasonLeaderboardEntry {
    pub user: Address,
    pub wins: u32,
    pub best_streak: u32,
}

/// Frozen snapshot of a season's final bounded rankings.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SeasonArchive {
    pub season_id: u32,
    pub ended_at_ledger: u32,
    pub wins: Vec<SeasonLeaderboardEntry>,
    pub streak: Vec<SeasonLeaderboardEntry>,
    pub participant_count: u32,
}

/// Configurable pending-winnings expiry in ledgers.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PendingWinningsExpiryKey(pub ());

pub const PENDING_WINNINGS_EXPIRY_KEY: PendingWinningsExpiryKey = PendingWinningsExpiryKey(());

/// Ledger sequence when a user's pending winnings entry was last modified.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PendingWinningsUpdatedAtKey(pub Address);

/// Fee incidence model for protocol fees (Issue #268).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum FeeModel {
    FeeOnPot = 0,      // Fee charged on total pot (default)
    FeeOnWinnings = 1, // Fee charged only on net winnings/profit
}

/// TWAP sample ring entry (Issue #266).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceSample {
    pub price: u128,
    pub timestamp: u64,
}

/// Storage key for TWAP samples ring (separate from DataKey to stay within variant limits, Issue #266).
#[contracttype]
#[derive(Clone)]
pub enum TwapSamplesKey {
    Samples,
}

/// Dev Reference Mode (Issue #266).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum DeviationReferenceMode {
    StartPrice = 0, // Use round.price_start (default)
    Twap = 1,       // Use trailing-sample TWAP average
}

/// Deviation guardrail config (Issue #266).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DeviationConfig {
    pub reference_mode: DeviationReferenceMode,
    pub window_samples: u32,
}

/// Storage key for deviation config (separate from DataKey to stay within variant limits, Issue #266).
#[contracttype]
#[derive(Clone)]
pub enum DeviationConfigKey {
    Config,
}

/// Oracle attestation config (Issue #263).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AttestationConfig {
    pub key: Option<BytesN<32>>, // ed25519 public key; None = attestation disabled
}

/// Storage key for attestation config (separate from DataKey to stay within variant limits, Issue #263).
#[contracttype]
#[derive(Clone)]
pub enum AttestationConfigKey {
    Config,
}

// ─── Dual-Approval Governance Types (Issue #272) ──────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum GovAction {
    PauseProtocol,
    UnpauseProtocol,
    SetProtocolFeeBps(Option<u32>),
    WithdrawProtocolFee(Address, i128),
    SetTreasuryAddress(Address),
    SetAdmin(Address),
    SetOracle(Address),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum GovProposalStatus {
    Pending = 0,
    Approved = 1,
    Executed = 2,
    Cancelled = 3,
    Expired = 4,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct GovProposal {
    pub id: u64,
    pub proposer: Address,
    pub approver: Option<Address>,
    pub action: GovAction,
    pub created_at_ledger: u32,
    pub expires_at_ledger: u32,
    pub status: GovProposalStatus,
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum PolicyAction {
    RoundMutation = 0,
    Claim = 1,
    AdminConfig = 2,
    Settlement = 3,
}

// ─── Intent / Keeper Authorization Zone (Issue #370) ─────────────────────────

/// Permission scope granted to a third-party keeper via a signed intent.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum KeeperScope {
    Resolve = 0,
    Claim = 1,
    CreateNext = 2,
}

/// Lifecycle state of a keeper intent.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum KeeperIntentStatus {
    Active = 0,
    Consumed = 1,
    Expired = 2,
    Revoked = 3,
}

/// Signed authorization intent granting a keeper permission to execute a scoped action.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct KeeperIntent {
    pub user: Address,
    pub keeper: Address,
    pub scope: KeeperScope,
    pub nonce: u64,
    pub expires_at_ledger: u32,
    pub status: KeeperIntentStatus,
    pub authorized_at_ledger: u32,
}

/// Dedicated storage keys for the intent/keeper subsystem (Issue #370).
#[contracttype]
#[derive(Clone)]
pub enum IntentKey {
    Intent(Address, KeeperScope, u64),
    ConsumedIntentNonce(Address, KeeperScope, u64),
    IntentNonceCursor(Address, KeeperScope),
    RegisteredKeeper(Address),
    KeeperRegistrationRequired,
}
