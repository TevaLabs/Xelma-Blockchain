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

/// Runtime mode for the contract lifecycle
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum RuntimeMode {
    Normal = 0,
    ClaimsOnly = 1,
    FullyPaused = 2,
}

/// Policy action class consumed by the central policy gate (Issue #261).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum PolicyAction {
    RoundMutation = 0,
    Claim = 1,
    AdminConfig = 2,
    Settlement = 3,
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
    OracleQuorum,
    /// Announced next schema version for migration preview.
    NextSchemaVersion,
    /// Minimum bet amount (dust protection). Unset = no minimum.
    MinBet,
    /// Epoch mint budget: total mints allowed per epoch.
    EpochMintBudget,
    /// Early cash-out penalty in basis points. Unset = early cash-out disabled.
    EarlyCashoutBps,
    /// Fee incidence model: FeeOnPot (default) or FeeOnWinnings.
    FeeModel,
    /// Dispute window length in ledgers. 0 = no dispute window.
    DisputeLedgers,
    /// Payout policy for Precision mode rounds.
    PrecisionPayoutPolicy,
    /// When true, only allowlisted addresses may participate (Issue #274).
    AccessControlEnabled,
    /// Secondary governance approver (Issue #272).
    GovApprover,
    /// Default governance proposal TTL in ledgers.
    GovProposalTtlLedgers,
    /// Monotonic counter for governance proposal ids.
    NextGovProposalId,
    /// Overflow bucket for leaderboard/season keys under XDR 50-case limit.
    Ext(DataKeyExt),
}

#[contracttype]
#[derive(Clone)]
pub enum DataKeyExt {
    LeaderboardWins,
    LeaderboardStreak,
    SeasonId,
    SeasonUserStats(u32, Address),
    SeasonLeaderboardWins,
    SeasonLeaderboardStreak,
    SeasonArchive(u32),
    /// On-chain constitution metadata defining parameter governance rules (Issue #363).
    ConstitutionMetadata,
    /// Pending amendment proposal by ID (Issue #363).
    Amendment(u64),
    /// Monotonic counter for amendment IDs (Issue #363).
    NextAmendmentId,
}

/// Parameterised and round-scoped storage keys.
///
/// Split from `DataKey` to stay under the XDR union 50-case limit.
/// These variants carry per-user, per-round, or compound-key payloads.
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
    /// Per-user outcome record for a specific archived round (round_id, user).
    /// Persisted at settlement for user history queries without event replay.
    UserRoundOutcome(u64, Address),
    /// Timelocked pending critical config change keyed by change kind.
    PendingConfigChange(ConfigChangeKind),
    /// Per-ledger mint counter: wraps the explicit ledger sequence number.
    LedgerMintCounter(u32),
    /// Compact post-settlement summary keyed by round id for historical queries.
    ArchivedRound(u64),
    /// Per-season, per-user win/loss/streak stats: (season_id, address) →
    /// UserStats, scoped independently of the lifetime `UserStats` totals so
    /// a season reset never touches lifetime history.
    SeasonUserStats(u32, Address),
    /// Frozen snapshot of a season's final rankings, written when the season
    /// is reset. Seasons are never deleted — this is a permanent archive.
    SeasonArchive(u32),
    /// Per-user index of archived round IDs (Issue #281).
    UserArchivedRoundIds(Address),
    /// Allowlist marker for participant access control (Issue #274).
    Allowlisted(Address),
    /// Denylist marker for participant access control (Issue #274).
    Denylisted(Address),
    /// Stored governance proposal record (Issue #272).
    GovProposal(u64),
    /// Records which round claimed a given ledger sequence as its
    /// `start_ledger`: start_ledger -> round_id.
    ///
    /// Oracle payloads bind to `Round.start_ledger` (see `OraclePayload.round_id`),
    /// which is not unique on its own: a round can be cancelled and replaced
    /// within a single ledger. This marker lets settlement reject a payload whose
    /// `start_ledger` resolves to a different round than the active one.
    RoundStartLedger(u32),
}

/// Fee incidence model (Issue #268).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum FeeModel {
    FeeOnPot = 0,
    FeeOnWinnings = 1,
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

/// One-sided (degenerate) market settlement policy (Issue #270 / #390).
/// When exactly one of pool_up/pool_down is empty, refund all stakes on the
/// populated side (default policy for one-sided UpDown pools).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum OneSidedPolicy {
    Refund = 0,
    Void = 1,
    CarryForward = 2,
}

pub type Policy = OneSidedPolicy;

/// Payout policy for Precision mode (on-chain config).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum PrecisionPayoutPolicy {
    Equal = 0,
    StakeWeighted = 1,
}

/// Participant access-control state (Issue #274).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum AccessState {
    Open = 0,
    Allowlisted = 1,
    Denylisted = 2,
}

/// Governance proposal lifecycle status (Issue #272).
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

/// Protected administrative action types (Issue #272).
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
    /// Withdraw from the insurance fund (Issue #367).
    WithdrawInsuranceFund(Address, i128),
    /// Set the insurance fee split in basis points (Issue #367).
    SetInsuranceSplitBps(u32),
    /// Set the insurance coverage payout rate in basis points (Issue #367).
    SetInsuranceCoverageBps(u32),
}

/// Stored governance proposal (Issue #272).
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
    /// Binds this payload to exactly one round.
    ///
    /// Must equal the active round's **`Round.start_ledger`** — the ledger
    /// sequence at which the round was created — NOT the monotonic
    /// `Round.round_id`. The two identifiers are used in different places:
    /// `start_ledger` binds the payload (and is covered by the attestation
    /// signature), while `Round.round_id` namespaces consumed nonces under
    /// `DataKeyScoped::ConsumedOracleNonce`.
    ///
    /// `create_round` guarantees a ledger sequence backs at most one round
    /// (`DataKeyScoped::RoundStartLedger` / `RoundStartLedgerReused`), so this
    /// value identifies a single round unambiguously. See `PROTOCOL_SPEC.md`
    /// invariant I10.
    pub round_id: u32,
    /// Per-round replay-protection nonce.
    ///
    /// The oracle service must generate a unique value per submission for a
    /// given round (e.g. a monotonic counter or random 64-bit value). The
    /// contract records each consumed nonce under
    /// `DataKeyScoped::ConsumedOracleNonce(round_id, nonce)` and rejects any reuse,
    /// making resolution idempotent against accidental duplicate submissions.
    pub nonce: u64,
    /// SHA-256 hash of the network passphrase this payload targets.
    /// Validated against `env.ledger().network_id()` to prevent cross-network replay.
    pub network_id: BytesN<32>,
    /// Contract address this payload is intended for.
    /// Validated against `env.current_contract_address()` to prevent cross-contract replay.
    pub contract_addr: Address,
    /// Optional confidence score from the price feed (0–10000 bps, where 10000 = 100%).
    /// When `None`, the payload is treated as a legacy submission.
    /// When strict mode is enabled, `None` is rejected.
    pub confidence: Option<u32>,
    pub attestation: Option<BytesN<64>>,
}

/// Oracle liveness record, updated by the oracle service on each heartbeat call.
/// `status`: 0 = active, 1 = degraded, 2 = offline.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleHeartbeatRecord {
    pub timestamp: u64,
    pub status: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    pub round_id: u64,       // Unique monotonically increasing round identifier
    pub price_start: u128,   // Starting XLM price in stroops
    pub start_ledger: u32,   // Ledger when round was created
    pub start_timestamp: u64,  // Ledger timestamp when round was created
    pub bet_end_ledger: u32, // Ledger when betting closes
    pub end_ledger: u32,     // Ledger when round ends (~5s per ledger)
    pub pool_up: i128,       // Total vXLM bet on UP
    pub pool_down: i128,     // Total vXLM bet on DOWN
    pub mode: RoundMode,     // Round mode: UpDown (0) or Precision (1)
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

/// One-read composite view of current market state for frontends: round
/// phase, pool composition, ledger timing buffers, and fee configuration —
/// replacing several separate calls that could otherwise observe
/// inconsistent state if the ledger advances between them (Issue #280).
///
/// # Empty-round semantics
///
/// When there is no active round, `phase` and `pool_stats` are both `None`.
/// The timing-buffer and fee fields are always populated regardless — they
/// reflect contract-wide configuration, not round state, so they have a
/// well-defined value whether or not a round is active.
///
/// # Consistency with individual getters
///
/// `phase` and `pool_stats` are the exact, unmodified results of
/// `get_round_phase`/`get_round_pool_stats` (never recomputed), and the
/// buffer/fee fields are read via the same public getters
/// (`get_bet_window_ledgers`, `get_run_window_ledgers`,
/// `get_close_buffer_ledgers`, `get_protocol_fee_bps`, `get_fee_model`) that
/// callers could otherwise call individually — so a snapshot can never
/// disagree with those getters.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MarketSnapshot {
    /// Current round's lifecycle phase, or empty if no round is active.
    ///
    /// Modeled as a 0-or-1-element `Vec` rather than `Option<RoundPhase>`:
    /// this soroban-sdk version's `#[contracttype]` derive does not generate
    /// an XDR (`ScVal`) conversion for `Option<T>` wrapping a user-defined
    /// type, only for `Vec<T>`.
    pub phase: Vec<RoundPhase>,
    /// Full pool-composition breakdown for the active round, or empty if no
    /// round is active. See `phase` for why this is a `Vec` and not an
    /// `Option`.
    pub pool_stats: Vec<RoundPoolStats>,
    /// Number of ledgers the betting window stays open after round creation.
    pub bet_window_ledgers: u32,
    /// Number of ledgers after round creation before the round becomes
    /// resolvable.
    pub run_window_ledgers: u32,
    /// Extra ledgers appended after the betting window closes, before the
    /// round transitions to `Running` (0 = disabled).
    pub close_buffer_ledgers: u32,
    /// Configured protocol fee in basis points, or `None` if fees are disabled.
    pub protocol_fee_bps: Option<u32>,
    /// Configured fee incidence model (`FeeOnPot` or `FeeOnWinnings`).
    pub fee_model: FeeModel,
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
///
/// Designed for operators to poll a single endpoint instead of stitching
/// together multiple read-only calls.
///
/// ## Status code → alert severity mapping
///
/// | code | label           | severity | meaning                                   |
/// |------|-----------------|----------|-------------------------------------------|
/// | 0    | HEALTHY         | none     | All subsystems nominal                    |
/// | 1    | PAUSED          | critical | Contract is emergency-paused               |
/// | 2    | ORACLE_STALE    | warning  | Oracle heartbeat is stale or offline      |
/// | 3    | ROUND_STALE     | warning  | Round is past its end ledger but unresolved|
/// | 4    | NO_ACTIVE_ROUND | info     | No round currently active (idle protocol) |
/// | 5    | MULTIPLE_ISSUES | critical | Two or more issues detected simultaneously|
///
/// ## Phase codes (`active_round_phase`)
///
/// | phase | meaning                                           |
/// |-------|---------------------------------------------------|
/// | 0     | No active round                                   |
/// | 1     | Betting open (`ledger < bet_end_ledger`)           |
/// | 2     | Running / reveal window (`bet_end_ledger ≤ ledger < end_ledger`) |
/// | 3     | Resolvable (`ledger ≥ end_ledger`)                |
///
/// ## Oracle status codes (`oracle_status`)
///
/// | code | meaning                                |
/// |------|----------------------------------------|
/// | 0    | Active (healthy heartbeat)             |
/// | 1    | Degraded (heartbeat marked degraded)   |
/// | 2    | Offline (heartbeat marked offline)     |
/// | 3    | Unknown (no heartbeat record stored)   |
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProtocolHealthStatus {
    /// Whether the contract is emergency-paused (`Paused == true`)
    pub paused: bool,
    /// Whether the oracle heartbeat is non-stale and not offline
    pub oracle_live: bool,
    /// Raw oracle heartbeat status (0=active, 1=degraded, 2=offline, 3=unknown)
    pub oracle_status: u32,
    /// Whether a round is currently active
    pub has_active_round: bool,
    /// Current round phase (0=no_round, 1=betting, 2=running, 3=resolvable)
    pub active_round_phase: u32,
    /// On-chain storage schema version
    pub schema_version: u32,
    /// Ledger sequence at which this health snapshot was taken
    pub ledger_sequence: u32,
    /// Ledger timestamp at which this health snapshot was taken
    pub ledger_timestamp: u64,
    /// Composite status code (see mapping table above)
    pub status_code: u32,
}

/// Compact historical round summary persisted after resolve or cancel.
///
/// Designed for explorer/analytics queries without replaying events.
/// `price_final` is `0` for admin cancellations (no oracle settlement price).
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
///
/// The admin proposes a new oracle address with a timestamp-based expiry window.
/// After `expires_at` (ledger timestamp) the proposal is stale and acceptance
/// is rejected until the admin submits a fresh proposal.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleRotationProposal {
    pub new_oracle: Address,
    pub proposed_at: u64,
    pub expires_at: u64,
}

/// Global status of the protocol, returned by `get_protocol_status`.
///
/// Designed for frontend state machines that need a single, stable code
/// instead of combining multiple boolean flags.
///
/// ## Status codes
///
/// | value | variant      | description                                                             |
/// |-------|--------------|-------------------------------------------------------------------------|
/// | 0     | `Active`     | Not paused; a round is currently active (bets open or running).          |
/// | 1     | `Paused`     | Emergency-paused by the admin; no mutations accepted except unpause.     |
/// | 2     | `ClaimsOnly` | Not paused; no active round. Only `claim_winnings` is meaningful.        |
///
/// ## Transition rules
///
/// - `ClaimsOnly` → `Active` when `create_round()` succeeds.
/// - `Active` → `ClaimsOnly` when `resolve_round()` or `cancel_round()` completes.
/// - Any state → `Paused` when `pause_contract()` is called.
/// - `Paused` → `Active` when `unpause_contract()` is called *and* an active round still exists.
/// - `Paused` → `ClaimsOnly` when `unpause_contract()` is called *and* no active round exists.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum ProtocolStatus {
    /// The contract is not paused and has a currently active round.
    Active = 0,
    /// The contract is emergency-paused by the admin.
    Paused = 1,
    /// The contract is not paused, but no round is active.
    /// Mutating actions are limited to claiming pending winnings.
    ClaimsOnly = 2,
}

/// Status of a specific round, returned by `get_round_status(round_id)`.
///
/// Queries a round by its monotonic `round_id`. Covers all lifecycle
/// stages from creation through terminal settlement.
///
/// ## Status codes
///
/// | value | variant          | description                                                                      |
/// |-------|------------------|-----------------------------------------------------------------------------------|
/// | 0     | `Unknown`        | Round does not exist or has been pruned from the on-chain archive.               |
/// | 1     | `Betting`        | Round is active; bets and predictions accepted (`ledger < bet_end_ledger`).      |
/// | 2     | `Running`        | Betting closed; reveal window open (`bet_end_ledger ≤ ledger < end_ledger`).    |
/// | 3     | `AwaitingResolve`| Round ended; awaiting oracle settlement (`ledger ≥ end_ledger`).                |
/// | 4     | `Resolved`       | Oracle settled the round; pot distributed to winners.                            |
/// | 5     | `Cancelled`      | Admin cancelled the round; all stakes refunded.                                  |
/// | 6     | `FallbackRefund` | Insufficient participants at settlement; all stakes refunded.                    |
///
/// ## Transition rules
///
/// - `Unknown` → `Betting` when `create_round()` succeeds.
/// - `Betting` → `Running` when `ledger ≥ bet_end_ledger` (derived; no on-chain write).
/// - `Running` → `AwaitingResolve` when `ledger ≥ end_ledger` (derived; no on-chain write).
/// - `{Betting | Running | AwaitingResolve}` → `Cancelled` when `cancel_round()` is called.
/// - `AwaitingResolve` → `Resolved` when `resolve_round()` settles with enough participants.
/// - `AwaitingResolve` → `FallbackRefund` when `resolve_round()` finds fewer than `min_participants`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum RoundStatus {
    /// Round does not exist or has been pruned from the on-chain archive.
    Unknown = 0,
    /// Round is active; bets and predictions accepted (`ledger < bet_end_ledger`).
    Betting = 1,
    /// Betting is closed; reveal window is open (`bet_end_ledger ≤ ledger < end_ledger`).
    Running = 2,
    /// Round has ended and is waiting for oracle settlement (`ledger ≥ end_ledger`).
    AwaitingResolve = 3,
    /// Oracle settled the round normally; pot distributed to winners.
    Resolved = 4,
    /// Admin cancelled the round; all stakes refunded.
    Cancelled = 5,
    /// Settlement triggered but insufficient participants; all stakes refunded.
    FallbackRefund = 6,
    /// Dispute window void; all participants refunded their full stake.
    Voided = 7,
}

/// Terminal outcome persisted per user per archived round.
///
/// Allows `get_user_archived_participation` to answer profile/history
/// queries without replaying the full event stream.
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
    pub fee_model: u32,
    pub outcomes: Vec<UserRoundOutcome>,
}

/// Per-participant outcome stored during dispute-window settlement.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedParticipant {
    pub user: Address,
    pub outcome: UserOutcomeType,
    pub payout: i128,
}

/// Settlement data stored during dispute-window resolve and consumed by
/// `finalize_round` or `void_round`.
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
///
/// Mirrors the arguments accepted by `create_round` (`start_price`, `mode`)
/// so a keeper can spin up the next round after a settle/cancel without an
/// operator re-specifying parameters each time. Validated with the exact
/// same rules `create_round` applies at creation time.
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

/// Frozen snapshot of a season's final bounded rankings, written by
/// `reset_leaderboard_season`. `participant_count` is the number of distinct
/// addresses that appeared in either bounded index at reset time (a lower
/// bound on total season participants beyond the tracked top
/// `LEADERBOARD_LIMIT`, mirroring the same bound the live indexes enforce).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SeasonArchive {
    pub season_id: u32,
    pub ended_at_ledger: u32,
    pub wins: Vec<SeasonLeaderboardEntry>,
    pub streak: Vec<SeasonLeaderboardEntry>,
    pub participant_count: u32,
}

/// Multi-feed oracle resolution payload.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MultiFeedPayload {
    pub prices: Vec<u128>,
    pub sources: Vec<u32>,
    pub round_id: u32,
    pub nonce: u64,
    pub network_id: BytesN<32>,
    pub contract_addr: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct OracleQuorumConfig {
    pub min_observations: u32,
    pub quorum_threshold: u32,
    pub outlier_threshold_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PriceSample {
    pub price: u128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum TwapSamplesKey {
    Samples,
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum DeviationReferenceMode {
    StartPrice = 0,
    Twap = 1,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DeviationConfig {
    pub reference_mode: DeviationReferenceMode,
    pub window_samples: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum DeviationConfigKey {
    Config,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AttestationConfig {
    pub key: Option<BytesN<32>>,
}

#[contracttype]
#[derive(Clone)]
pub enum AttestationConfigKey {
    Config,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct HbGateConfig {
    pub strict_mode: bool,
    pub override_armed: bool,
    pub grace_seconds: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum HbGateKey {
    Config,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct PendingWinningsExpiryKey(pub ());

pub const PENDING_WINNINGS_EXPIRY_KEY: PendingWinningsExpiryKey = PendingWinningsExpiryKey(());

/// Eligible failure events for insurance coverage (Issue #367).
///
/// Each variant maps to a cancel-round reason code used by the
/// insurance coverage payout gate. Only events listed in the
/// admin-configured whitelist trigger coverage.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum InsuranceEvent {
    /// Cancel due to oracle heartbeat failure / outage.
    OracleOutage = 0,
    /// Cancel due to oracle deviation exceeding the configured threshold.
    OracleDeviation = 1,
    /// Fallback refund when insufficient participants joined the round.
    FallbackRefund = 2,
}

/// Cancel-round reason codes that map to InsuranceEvent variants.
///
/// Passed as the `reason` argument to `cancel_round`. The mapping is:
/// - 0 → not eligible (generic / admin discretion)
/// - 1 → OracleOutage
/// - 2 → OracleDeviation
/// - 3 → FallbackRefund
pub const CANCEL_REASON_GENERIC: u32 = 0;
pub const CANCEL_REASON_ORACLE_OUTAGE: u32 = 1;
pub const CANCEL_REASON_ORACLE_DEVIATION: u32 = 2;
pub const CANCEL_REASON_FALLBACK_REFUND: u32 = 3;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingWinningsUpdatedAtKey(pub Address);

/// Parameter classification for the on-chain constitution (Issue #363).
/// Immutable parameters cannot be changed; timelocked parameters require
/// a timelock before activation; dual-approval parameters require both
/// admin and approver sign-off.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum ParameterClass {
    /// Cannot be modified after initialization
    Immutable = 0,
    /// Requires timelock period before activation
    Timelocked = 1,
    /// Requires both admin and approver approval
    DualApproval = 2,
    /// May be changed immediately (least restrictive)
    Normal = 3,
}

/// Amendment proposal lifecycle status for the constitution (Issue #363).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum AmendmentStatus {
    /// Proposal submitted, awaiting veto window expiry or approvals
    Pending = 0,
    /// Veto has been exercised, proposal is cancelled
    Vetoed = 1,
    /// Timelock period has elapsed, ready for activation
    ActivationReady = 2,
    /// Amendment has been activated and parameter changed
    Activated = 3,
    /// Amendment expired before activation
    Expired = 4,
}

/// Amendment proposal for parameter changes with timelock and veto window (Issue #363).
/// Represents a proposed change to a protocol parameter that must pass through a
/// governance lifecycle: optional veto window, timelock, then activation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Amendment {
    pub id: u64,
    pub proposer: Address,
    pub parameter_name: Symbol,
    pub new_value: Val,
    pub created_at_ledger: u32,
    pub veto_deadline_ledger: u32,
    pub activation_deadline_ledger: u32,
    pub status: AmendmentStatus,
}

/// On-chain constitution defining parameter governance rules (Issue #363).
/// Classifies each protocol parameter and defines the amendment lifecycle
/// (veto window, timelock, dual approval requirements).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ConstitutionMetadata {
    /// Veto window duration in ledgers (0 = no veto window)
    pub veto_window_ledgers: u32,
    /// Timelock duration in ledgers before amendments can activate
    pub timelock_ledgers: u32,
    /// Whether dual-approval (admin + approver) is required for amendments
    pub dual_approval_required: bool,
    /// Ledger at which the constitution was established
    pub established_at_ledger: u32,
}

/// Legacy monolithic storage key — retained for a few migration/read paths.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Balance(Address),
    Admin,
    Oracle,
    SchemaVersion,
    ActiveRound,
    Positions,
    UpDownPositions,
    PrecisionPositions,
    PendingWinnings(Address),
    UserStats(Address),
    Paused,
    BetWindowLedgers,
    RunWindowLedgers,
    CloseBufferLedgers,
    LastRoundId,
    Position(u64, Address),
    PrecisionPosition(u64, Address),
    PrecisionCommitment(u64, Address),
    RoundParticipants(u64),
    MaxStake,
    MaxUserRoundExposure,
    MaxPendingWinnings,
    CancelledRound(u64),
    ConsumedOracleNonce(u64, u64),
    MinParticipants,
    OracleHeartbeat,
    OracleStaleThreshold,
    MaxPrecisionParticipants,
    OracleMaxDeviationBps,
    OracleDeviationOverrideArmed,
    OracleMinConfidenceBps,
    OracleStrictMode,
    ArchivedRound(u64),
    RecentArchivedRoundIds,
    UserRoundOutcome(u64, Address),
    MigratedToV3,
    PendingConfigChange(ConfigChangeKind),
    ProtocolFeeBps,
    ProtocolFeeTreasury,
    LedgerMintCounter(u32),
    MintLimitConfig,
    OracleRotationProposal,
    ArchiveRetention,
    RoundTemplate,
    Ext(DataKeyExt),
}
