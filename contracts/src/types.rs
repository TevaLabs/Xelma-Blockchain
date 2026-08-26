// SPDX-License-Identifier: MIT
//! Type definitions for the XLM Price Prediction Market.

use soroban_sdk::{contracttype, Address, BytesN, Vec};

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
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum RoundPhase {
    Betting = 1,
    Running = 2,
    Resolvable = 3,
}

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

/// Resolved participant access state for allowlist/denylist gating.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum AccessState {
    Open = 0,
    Allowlisted = 1,
    Denylisted = 2,
}

/// Parameterless system, config, and metadata storage keys.
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
    MaxStake,
    MaxUserRoundExposure,
    MaxPendingWinnings,
    MinParticipants,
    OracleHeartbeat,
    OracleStaleThreshold,
    MaxPrecisionParticipants,
    OracleMaxDeviationBps,
    OracleDeviationOverrideArmed,
    OracleMinConfidenceBps,
    OracleStrictMode,
    RecentArchivedRoundIds,
    MigratedToV3,
    ProtocolFeeBps,
    ProtocolFeeTreasury,
    MintLimitConfig,
    OracleRotationProposal,
    ArchiveRetention,
    RoundTemplate,
    LeaderboardWins,
    LeaderboardStreak,
    SeasonId,
    SeasonLeaderboardWins,
    SeasonLeaderboardStreak,
    OracleQuorum,
    NextSchemaVersion,
    MinBet,
    EpochMintBudget,
    EarlyCashoutBps,
    FeeModel,
    DisputeLedgers,
    PrecisionPayoutPolicy,
    AccessControlEnabled,
    NextGovProposalId,
    GovApprover,
    GovProposalTtlLedgers,
}

/// Parameterised and round-scoped storage keys.
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
    UserArchivedRoundIds(Address),
    PendingConfigChange(ConfigChangeKind),
    LedgerMintCounter(u32),
    ArchivedRound(u64),
    SeasonUserStats(u32, Address),
    SeasonArchive(u32),
    Allowlisted(Address),
    Denylisted(Address),
    GovProposal(u64),
}

pub type DataKey = DataKeyCore;

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
    PrecisionPayoutPolicy = 12,
    MinBet = 13,
    EpochMintBudget = 14,
    EarlyCashoutBps = 15,
    FeeModel = 16,
    DisputeLedgers = 17,
    OracleTimestampSkew = 18,
    PendingWinningsExpiry = 19,
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
    PrecisionPayoutPolicy(u32),
    MinBet(Option<i128>),
    EpochMintBudget(i128),
    EarlyCashoutBps(Option<u32>),
    FeeModel(FeeModel),
    DisputeLedgers(u32),
    OracleTimestampSkew(u64),
    PendingWinningsExpiry(u32),
}

/// Pending timelocked config change with activation ledger for on-chain observability.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingConfigChange {
    pub payload: ConfigChangePayload,
    pub activation_ledger: u32,
    pub scheduled_at_ledger: u32,
}

/// Actions protected by dual-approval governance (Issue #272)
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

/// Lifecycle status of a dual-approval governance proposal
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum GovProposalStatus {
    Pending = 0,
    Approved = 1,
    Executed = 2,
    Cancelled = 3,
    Expired = 4,
}

/// Governance proposal record requiring dual approval before execution
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

/// Policy action kind for governance audit logs
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum PolicyAction {
    RoundMutation = 0,
    Claim = 1,
    AdminConfig = 2,
    Settlement = 3,
    AllowlistAdd = 4,
    AllowlistRemove = 5,
    DenylistAdd = 6,
    DenylistRemove = 7,
    ToggleAccessControl = 8,
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
    pub predicted_price: u128,
    pub amount: i128,
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
    pub round_id: u32,
    pub nonce: u64,
    pub network_id: BytesN<32>,
    pub contract_addr: Address,
    pub confidence: Option<u32>,
    pub attestation: Option<BytesN<64>>,
}

/// Multi-feed oracle payload containing aggregated reports.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MultiFeedPayload {
    pub price: u128,
    pub timestamp: u64,
    pub round_id: u32,
    pub nonce: u64,
    pub network_id: BytesN<32>,
    pub contract_addr: Address,
    pub confidence: Option<u32>,
    pub reports: Vec<PriceSample>,
    pub prices: Vec<u128>,
    pub sources: Vec<u32>,
}

/// Oracle quorum configuration.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleQuorumConfig {
    pub min_observations: u32,
    pub quorum_threshold: u32,
    pub outlier_threshold_bps: u32,
    pub min_reports: u32,
    pub max_skew_seconds: u64,
}

/// Oracle liveness record, updated by the oracle service on each heartbeat call.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleHeartbeatRecord {
    pub timestamp: u64,
    pub status: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    pub round_id: u64,
    pub price_start: u128,
    pub start_ledger: u32,
    pub bet_end_ledger: u32,
    pub end_ledger: u32,
    pub pool_up: i128,
    pub pool_down: i128,
    pub mode: RoundMode,
    pub start_timestamp: u64,
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

/// Health-check gate config for oracle heartbeat strictness and override state.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct HbGateConfig {
    pub strict_mode: bool,
    pub override_armed: bool,
    pub grace_seconds: u64,
}

/// Storage key for heartbeat gate config.
#[contracttype]
#[derive(Clone)]
pub enum HbGateKey {
    Config,
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
    Cancel = 3,
    Void = 4,
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
