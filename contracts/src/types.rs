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
/// Split from a single unified `DataKey` to stay under the XDR union 50-case
/// limit (`VecM<ScSpecUdtUnionCaseV0, 50>` in stellar-xdr). `DataKeyCore`
/// holds singleton/config keys with no payload (plus `Ext`, itself a further
/// split for the leaderboard/season singleton keys); `DataKeyScoped` (below)
/// holds every key parameterised by round id, address, or season id.
#[contracttype]
#[derive(Clone)]
pub enum DataKeyCore {
    Admin,
    Oracle,
    /// On-chain storage schema version for migration safety.
    /// If missing, the contract treats it as legacy schema version 1.
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
    /// If unset, deviation guardrails are disabled.
    OracleMaxDeviationBps,
    /// One-shot admin override allowing the next settlement to bypass deviation checks.
    /// Automatically cleared after use.
    OracleDeviationOverrideArmed,
    /// Minimum oracle confidence threshold in basis points (0–10000).
    /// If unset, confidence guardrails are disabled.
    OracleMinConfidenceBps,
    /// When true, payloads with missing confidence are rejected in strict mode.
    OracleStrictMode,
    /// Ordered round ids for archive retention (oldest at index 0).
    RecentArchivedRoundIds,
    /// Marker written by migrate_schema_v2_to_v3 to prove the migration ran.
    MigratedToV3,
    /// Optional protocol settlement fee in basis points (1 bp = 0.01%).
    /// `None` (key absent) means fee disabled — no behaviour change.
    /// Hard cap on fee is enforced at the contract layer, not by storage shape.
    ProtocolFeeBps,
    /// On-chain accumulated protocol fee balance in stroops (i128).
    /// Admin withdraws via the dedicated withdrawal method; does NOT mix
    /// into the per-user balance ledger.
    ProtocolFeeTreasury,
    /// Mint limit configuration: maximum number of mints allowed per ledger.
    MintLimitConfig,
    /// Pending two-step oracle rotation proposal with expiry.
    OracleRotationProposal,
    /// Configurable archive retention limit: maximum number of ArchivedRound entries
    /// retained on-chain before the oldest are pruned (FIFO). If unset, the protocol
    /// default is used.
    ArchiveRetention,
    /// Admin-configured blueprint used by `create_next_from_template` to spin
    /// up the next round without re-specifying `start_price` / `mode` each
    /// time. Absent means no template is configured.
    RoundTemplate,
    /// Admin-configured multi-feed oracle quorum parameters.
    /// When set, `resolve_round_multi` is enabled.
    OracleQuorum,
    /// Announced next schema version for migration preview (v-next template).
    /// When set, operators can inspect this value before executing a real migration.
    /// Absent means no next migration has been announced.
    NextSchemaVersion,
    /// Minimum bet amount (dust protection). Unset = no minimum.
    MinBet,
    /// Early cash-out penalty in basis points. Unset = early cash-out disabled.
    EarlyCashoutBps,
    /// Fee incidence model: FeeOnPot (default) or FeeOnWinnings.
    FeeModel,
    /// Precision payout policy: Equal (default) or StakeWeighted.
    PrecisionPayoutPolicy,
    /// Whether allowlist/denylist access control gating is active.
    AccessControlEnabled,
    /// Configured secondary governance approver for dual-approval actions.
    GovApprover,
    /// Default TTL (in ledgers) applied to new governance proposals.
    GovProposalTtlLedgers,
    /// Monotonic counter for the next governance proposal id.
    NextGovProposalId,
    /// Further split for parameterless leaderboard/season keys — see `DataKeyExt`.
    Ext(DataKeyExt),
}

/// Leaderboard/season singleton keys, nested under `DataKeyCore::Ext` to stay
/// within the same 50-case XDR union limit that motivated the Core/Scoped split.
#[contracttype]
#[derive(Clone)]
pub enum DataKeyExt {
    /// Bounded index of user addresses sorted by lifetime total wins
    /// descending (all-time leaderboard, independent of seasons).
    LeaderboardWins,
    /// Bounded index of user addresses sorted by lifetime best streak
    /// descending (all-time leaderboard, independent of seasons).
    LeaderboardStreak,
    /// Monotonically increasing id of the currently-active leaderboard
    /// season. Absent is treated as season 1.
    SeasonId,
    /// Bounded index of user addresses in the *active* season sorted by
    /// season-scoped total wins descending.
    SeasonLeaderboardWins,
    /// Bounded index of user addresses in the *active* season sorted by
    /// season-scoped best streak descending.
    SeasonLeaderboardStreak,
}

/// Parameterised and round-scoped storage keys.
///
/// Split from a single unified `DataKey` to stay under the XDR union 50-case
/// limit (see `DataKeyCore` above). These variants carry per-user, per-round,
/// per-season, or compound-key payloads.
#[contracttype]
#[derive(Clone)]
pub enum DataKeyScoped {
    /// User financial balance
    Balance(Address),
    /// User pending winnings accumulator
    PendingWinnings(Address),
    /// User performance statistics
    UserStats(Address),
    /// Per-user UpDown position: (round_id, address) → UserPosition
    Position(u64, Address),
    /// Per-user Precision prediction: (round_id, address) → PrecisionPrediction
    PrecisionPosition(u64, Address),
    /// Per-user Precision commitment: (round_id, address) → PrecisionCommitment
    PrecisionCommitment(u64, Address),
    /// Ordered participant list for a round: round_id → Vec<Address>
    RoundParticipants(u64),
    /// Marker for a cancelled round: round_id → true
    CancelledRound(u64),
    /// Per-round consumed oracle nonce: (round_id, nonce) → true.
    /// Used to reject duplicate oracle payload submissions for the same round.
    ConsumedOracleNonce(u64, u64),
    /// Compact post-settlement summary keyed by round id for historical queries.
    ArchivedRound(u64),
    /// Per-user outcome record for a specific archived round (round_id, user).
    /// Persisted at settlement for user history queries without event replay.
    UserRoundOutcome(u64, Address),
    /// Per-user index of archived round IDs the user participated in.
    UserArchivedRoundIds(Address),
    /// Timelocked pending critical config change keyed by change kind.
    PendingConfigChange(ConfigChangeKind),
    /// Per-ledger mint counter: wraps the explicit ledger sequence number.
    LedgerMintCounter(u32),
    /// Per-season, per-user win/loss/streak stats: (season_id, address) →
    /// UserStats, scoped independently of the lifetime `UserStats` totals so
    /// a season reset never touches lifetime history.
    SeasonUserStats(u32, Address),
    /// Frozen snapshot of a season's final rankings, written when the season
    /// is reset. Seasons are never deleted — this is a permanent archive.
    SeasonArchive(u32),
    /// Address explicitly allowlisted for access-control-gated actions.
    Allowlisted(Address),
    /// Address explicitly denylisted for access-control-gated actions.
    Denylisted(Address),
    /// Pending dual-approval governance proposal, keyed by proposal id.
    GovProposal(u64),
    /// Latest published pool-aggregate commitment for a round.
    PoolCommitment(u64),
    /// Secret salt bound into a round's pool commitment chain; never
    /// returned by any query, only consumed internally and copied into
    /// `PoolOpening` once the round is opened.
    PoolCommitmentSalt(u64),
    /// Revealed pool-aggregate opening for a round, written once at or after
    /// `bet_end_ledger`. Absence means "not yet opened" (fail-closed).
    PoolOpening(u64),
    /// Running total of Precision-mode stake (direct placements + commit-reveal
    /// commitments), maintained incrementally so the pool commitment can fold
    /// it in at O(1) cost instead of re-scanning all participants.
    PrecisionCommitStake(u64),
    /// Staged settlement outcome awaiting the dispute window (Issue #276),
    /// keyed by round id. Only written when `DisputeLedgers > 0`; absence
    /// means the round either has no dispute window configured or has
    /// already been finalised/voided.
    PendingDispute(u64),
}

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
    PendingWinningsExpiry = 13,
    PrecisionPayoutPolicy = 14,
    MinBet = 15,
    DisputeLedgers = 16,
    FeeModel = 17,
    EpochMintBudget = 18,
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
///
/// Unlike the legacy single-oracle `OraclePayload`, this carries N independent
/// feed observations as parallel arrays. The contract computes the median,
/// rejects outliers, and requires a configurable quorum of feeds to agree
/// within the outlier threshold before settlement proceeds.
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

/// Admin-configurable quorum and outlier rejection parameters for multi-feed
/// oracle settlement. Stored under `DataKeyCore::OracleQuorum`.
///
/// When set, `resolve_round_multi` becomes the preferred settlement path.
/// The legacy single-oracle `resolve_round` path remains available
/// independently of this configuration.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleQuorumConfig {
    /// Minimum number of unique feed observations required in a multi-feed payload.
    pub min_observations: u32,
    /// Minimum number of observations that must survive outlier rejection to
    /// form a valid quorum and proceed to settlement.
    pub quorum_threshold: u32,
    /// Maximum deviation from the median (in basis points, 1 bp = 0.01%)
    /// before an observation is rejected as an outlier.
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
///
/// The variant name, not the enclosing Rust enum name, is what Soroban
/// encodes into the storage key for a unit variant — so this must stay
/// distinct from every other single-variant "config key" enum's variant
/// name in this file (`DeviationConfigKey`, `AttestationConfigKey`, ...) or
/// their writes silently collide onto the same storage slot.
#[contracttype]
#[derive(Clone)]
pub enum HbGateKey {
    HbConfig,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    pub round_id: u64,        // Unique monotonically increasing round identifier
    pub price_start: u128,    // Starting XLM price in stroops
    pub start_ledger: u32,    // Ledger when round was created
    pub bet_end_ledger: u32,  // Ledger when betting closes
    pub end_ledger: u32,      // Ledger when round ends (~5s per ledger)
    pub pool_up: i128,        // Total vXLM bet on UP
    pub pool_down: i128,      // Total vXLM bet on DOWN
    pub mode: RoundMode,      // Round mode: UpDown (0) or Precision (1)
    pub start_timestamp: u64, // Ledger timestamp when round was created
}

/// Aggregated active-round pool composition for frontend transparency.
///
/// Up/Down rounds populate the up/down pools, counts, and stake ratios.
/// Precision rounds populate the precision totals and participant counters while
/// leaving side-specific Up/Down fields at zero. Ratios are basis points of
/// the mode's total visible stake (10_000 = 100%).
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
    /// Oracle settlement completed (normal resolution path).
    Resolved = 0,
    /// Admin cancelled the round and refunded participants.
    Cancelled = 1,
    /// Settlement aborted due to insufficient participants; stakes refunded.
    FallbackRefund = 2,
    /// Dispute window ended via void; all participants refunded their stake.
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
/// See the note on [`HbGateKey`] — the variant name alone forms the storage
/// key, so it must not collide with other config-key enums' variant names.
#[contracttype]
#[derive(Clone)]
pub enum DeviationConfigKey {
    DeviationConfig,
}

/// Oracle attestation config (Issue #263).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AttestationConfig {
    pub key: Option<BytesN<32>>, // ed25519 public key; None = attestation disabled
}

/// Storage key for attestation config (separate from DataKey to stay within variant limits, Issue #263).
/// See the note on [`HbGateKey`] — the variant name alone forms the storage
/// key, so it must not collide with other config-key enums' variant names.
#[contracttype]
#[derive(Clone)]
pub enum AttestationConfigKey {
    AttestationConfig,
}

/// Protected administrative action gated behind dual-approval governance
/// (Issue #272).
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

/// Lifecycle state of a governance proposal.
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

/// A dual-approval governance proposal: one party proposes, a distinct party
/// (admin or the configured secondary approver) must approve, and either can
/// then execute.
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

/// Access-control classification for a user under allowlist/denylist gating.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum AccessState {
    Open = 0,
    Allowlisted = 1,
    Denylisted = 2,
}

/// Deterministic policy applied when a round ends with liquidity on only one
/// side of the market (Issue #270). See `("pool", "onesided")` in
/// `docs/EVENT_SCHEMA.md` for the corresponding `policy_code` values.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum OneSidedPolicy {
    Refund = 0,
    Void = 1,
    CarryForward = 2,
}

/// Alias retained for call sites written against the pre-#270 name.
pub type Policy = OneSidedPolicy;

/// Staged settlement outcome awaiting the dispute window (Issue #276).
/// `void_round` may refund in full while `ledger < expires_at_ledger`;
/// `finalize_round` distributes `final_price`'s computed settlement once the
/// window has elapsed.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingDispute {
    pub final_price: u128,
    pub confidence: Option<u32>,
    pub expires_at_ledger: u32,
}

/// Classifies a mutating entrypoint for the runtime-mode policy gate
/// (`_policy_gate`), so each call site references a semantic action rather
/// than re-deriving the mode rules inline.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum PolicyAction {
    RoundMutation = 0,
    Claim = 1,
    AdminConfig = 2,
    Settlement = 3,
}

/// Published commitment to a round's pool aggregate, advanced on every
/// mutating betting action. Carries only a hash + sequence + ledger — never
/// the raw aggregate values (see `PoolOpening` for the post-close reveal).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PoolCommitment {
    pub round_id: u64,
    pub seq: u32,
    pub commitment: BytesN<32>,
    pub ledger: u32,
}

/// Revealed opening for a round's pool-aggregate commitment chain, written
/// once at or after `bet_end_ledger` by `open_pool_commitment` or
/// automatically at settlement. Recomputing `sha256(round_id || seq ||
/// pool_up || pool_down || precision_total_stake || salt)` must equal the
/// last `("pool", "commit")` event's commitment hash.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PoolOpening {
    pub round_id: u64,
    pub seq: u32,
    pub pool_up: i128,
    pub pool_down: i128,
    pub precision_total_stake: i128,
    pub salt: BytesN<32>,
    pub opened_ledger: u32,
}
