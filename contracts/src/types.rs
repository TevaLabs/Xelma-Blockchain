// SPDX-License-Identifier: MIT
//! Type definitions for the XLM Price Prediction Market.

use soroban_sdk::{contracttype, Address, BytesN};

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

/// Lifecycle phase of an active round, derived from ledger windows.
///
/// Semantics (given `start_ledger`, `bet_end_ledger`, `end_ledger`):
/// - `Betting`: `ledger < bet_end_ledger` â€” bets and precision predictions accepted
/// - `Running`: `bet_end_ledger â‰¤ ledger < end_ledger` â€” reveal window (precision)
/// - `Resolvable`: `ledger â‰¥ end_ledger` â€” round may be settled via oracle payload
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum RoundPhase {
    Betting = 1,
    Running = 2,
    Resolvable = 3,
}

/// Storage keys for contract data
///
/// ## Indexed position keys (variants 13â€“15)
///
/// `Position(round_id, address)` and `PrecisionPosition(round_id, address)` store
/// a single user's record under a composite key, enabling O(1) read/write per user
/// instead of deserializing the full participant map on every bet.
///
/// `RoundParticipants(round_id)` holds the ordered `Vec<Address>` used for
/// iteration at resolution time. Appending one address is cheaper than
/// re-serialising an N-entry `Map<Address, T>` for every bet placed.
///
/// Legacy single-key maps (`UpDownPositions`, `PrecisionPositions`) are kept for
/// backward-compatible reads during a migration window; they are no longer written.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Balance(Address),
    Admin,
    Oracle,
    /// On-chain storage schema version for migration safety.
    /// If missing, the contract treats it as legacy schema version 1.
    SchemaVersion,
    ActiveRound,
    Positions,          // Legacy key â€” read-only migration compat
    UpDownPositions,    // Legacy key â€” read-only migration compat
    PrecisionPositions, // Legacy key â€” read-only migration compat
    PendingWinnings(Address),
    UserStats(Address),
    Paused,
    BetWindowLedgers,
    RunWindowLedgers,
    LastRoundId,
    /// Per-user UpDown position: (round_id, address) â†’ UserPosition
    Position(u64, Address),
    /// Per-user Precision prediction: (round_id, address) â†’ PrecisionPrediction
    PrecisionPosition(u64, Address),
    /// Per-user Precision commitment: (round_id, address) â†’ PrecisionCommitment
    PrecisionCommitment(u64, Address),
    /// Ordered participant list for a round: round_id â†’ Vec<Address>
    RoundParticipants(u64),
    /// Maximum stake allowed per individual bet (None = unlimited)
    MaxStake,
    /// Maximum cumulative exposure per user per round (None = unlimited)
    MaxUserRoundExposure,
    /// Maximum pending winnings allowed per account (None = unlimited)
    MaxPendingWinnings,
    /// Marker for a cancelled round: round_id â†’ true
    CancelledRound(u64),
    /// Per-round consumed oracle nonce: (round_id, nonce) â†’ true.
    /// Used to reject duplicate oracle payload submissions for the same round.
    ConsumedOracleNonce(u64, u64),
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
    /// Minimum oracle confidence threshold in basis points (0â€“10000).
    /// If unset, confidence guardrails are disabled.
    OracleMinConfidenceBps,
    /// When true, payloads with missing confidence are rejected in strict mode.
    OracleStrictMode,
    /// Compact post-settlement summary keyed by round id for historical queries.
    ArchivedRound(u64),
    /// Ordered round ids for archive retention (oldest at index 0).
    RecentArchivedRoundIds,
    /// Per-user outcome record for a specific archived round (round_id, user).
    /// Persisted at settlement for user history queries without event replay.
    UserRoundOutcome(u64, Address),
    /// Marker written by migrate_schema_v2_to_v3 to prove the migration ran.
    MigratedToV3,
    /// Timelocked pending critical config change keyed by change kind.
    PendingConfigChange(ConfigChangeKind),
    /// Optional protocol settlement fee in basis points (1 bp = 0.01%).
    /// `None` (key absent) means fee disabled â€” no behaviour change.
    /// Hard cap on fee is enforced at the contract layer, not by storage shape.
    ProtocolFeeBps,
    /// On-chain accumulated protocol fee balance in stroops (i128).
    /// Admin withdraws via the dedicated withdrawal method; does NOT mix
    /// into the per-user balance ledger.
    ProtocolFeeTreasury,
    /// Per-ledger mint counter: wraps the explicit ledger sequence number.
    LedgerMintCounter(u32),
    /// Mint limit configuration: maximum number of mints allowed per ledger.
    MintLimitConfig,
    /// Configured archived round retention limit.
    ArchiveRetention,
    /// Pending two-step oracle rotation proposal with expiry.
    OracleRotationProposal,
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
    /// Optional protocol settlement fee in bps (Issue #162).
    /// `None` disables the fee entirely, restoring pre-fee behaviour.
    ProtocolFeeBps = 6,
    MinParticipants = 7,
    MaxPrecisionParticipants = 8,
    MintLimit = 9,
    ArchiveRetention = 10,
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
    pub predicted_price: u128, // Price scaled to 4 decimals (e.g., 0.2297 â†’ 2297)
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
    ///
    /// The oracle service must generate a unique value per submission for a
    /// given round (e.g. a monotonic counter or random 64-bit value). The
    /// contract records each consumed nonce under
    /// `DataKey::ConsumedOracleNonce(round_id, nonce)` and rejects any reuse,
    /// making resolution idempotent against accidental duplicate submissions.
    pub nonce: u64,
    /// SHA-256 hash of the network passphrase this payload targets.
    /// Validated against `env.ledger().network_id()` to prevent cross-network replay.
    pub network_id: BytesN<32>,
    /// Contract address this payload is intended for.
    /// Validated against `env.current_contract_address()` to prevent cross-contract replay.
    pub contract_addr: Address,
    /// Optional confidence score from the price feed (0â€“10000 bps, where 10000 = 100%).
    /// When `None`, the payload is treated as a legacy submission.
    /// When strict mode is enabled, `None` is rejected.
    pub confidence: Option<u32>,
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
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum UserOutcomeType {
    Win = 0,
    Loss = 1,
    Refund = 2,
    Cancel = 3,
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

/// Composite protocol health status returned by `get_protocol_health`.
///
/// Designed for operators to poll a single endpoint instead of stitching
/// together multiple read-only calls.
///
/// ## Status code â†’ alert severity mapping
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
/// | 2     | Running / reveal window (`bet_end_ledger â‰¤ ledger < end_ledger`) |
/// | 3     | Resolvable (`ledger â‰¥ end_ledger`)                |
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
/// - `ClaimsOnly` â†’ `Active` when `create_round()` succeeds.
/// - `Active` â†’ `ClaimsOnly` when `resolve_round()` or `cancel_round()` completes.
/// - Any state â†’ `Paused` when `pause_contract()` is called.
/// - `Paused` â†’ `Active` when `unpause_contract()` is called *and* an active round still exists.
/// - `Paused` â†’ `ClaimsOnly` when `unpause_contract()` is called *and* no active round exists.
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
/// | 2     | `Running`        | Betting closed; reveal window open (`bet_end_ledger â‰¤ ledger < end_ledger`).    |
/// | 3     | `AwaitingResolve`| Round ended; awaiting oracle settlement (`ledger â‰¥ end_ledger`).                |
/// | 4     | `Resolved`       | Oracle settled the round; pot distributed to winners.                            |
/// | 5     | `Cancelled`      | Admin cancelled the round; all stakes refunded.                                  |
/// | 6     | `FallbackRefund` | Insufficient participants at settlement; all stakes refunded.                    |
///
/// ## Transition rules
///
/// - `Unknown` â†’ `Betting` when `create_round()` succeeds.
/// - `Betting` â†’ `Running` when `ledger â‰¥ bet_end_ledger` (derived; no on-chain write).
/// - `Running` â†’ `AwaitingResolve` when `ledger â‰¥ end_ledger` (derived; no on-chain write).
/// - `{Betting | Running | AwaitingResolve}` â†’ `Cancelled` when `cancel_round()` is called.
/// - `AwaitingResolve` â†’ `Resolved` when `resolve_round()` settles with enough participants.
/// - `AwaitingResolve` â†’ `FallbackRefund` when `resolve_round()` finds fewer than `min_participants`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum RoundStatus {
    /// Round does not exist or has been pruned from the on-chain archive.
    Unknown = 0,
    /// Round is active; bets and predictions accepted (`ledger < bet_end_ledger`).
    Betting = 1,
    /// Betting is closed; reveal window is open (`bet_end_ledger â‰¤ ledger < end_ledger`).
    Running = 2,
    /// Round has ended and is waiting for oracle settlement (`ledger â‰¥ end_ledger`).
    AwaitingResolve = 3,
    /// Oracle settled the round normally; pot distributed to winners.
    Resolved = 4,
    /// Admin cancelled the round; all stakes refunded.
    Cancelled = 5,
    /// Settlement triggered but insufficient participants; all stakes refunded.
    FallbackRefund = 6,
}
