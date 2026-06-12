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

/// Storage keys for contract data
///
/// ## Indexed position keys (variants 13–15)
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
    Positions,          // Legacy key — read-only migration compat
    UpDownPositions,    // Legacy key — read-only migration compat
    PrecisionPositions, // Legacy key — read-only migration compat
    PendingWinnings(Address),
    UserStats(Address),
    Paused,
    BetWindowLedgers,
    RunWindowLedgers,
    LastRoundId,
    /// Per-user UpDown position: (round_id, address) → UserPosition
    Position(u64, Address),
    /// Per-user Precision prediction: (round_id, address) → PrecisionPrediction
    PrecisionPosition(u64, Address),
    /// Per-user Precision commitment: (round_id, address) → PrecisionCommitment
    PrecisionCommitment(u64, Address),
    /// Ordered participant list for a round: round_id → Vec<Address>
    RoundParticipants(u64),
    /// Maximum stake allowed per individual bet (None = unlimited)
    MaxStake,
    /// Maximum cumulative exposure per user per round (None = unlimited)
    MaxUserRoundExposure,
    /// Maximum pending winnings allowed per account (None = unlimited)
    MaxPendingWinnings,
    /// Marker for a cancelled round: round_id → true
    CancelledRound(u64),
    /// Per-round consumed oracle nonce: (round_id, nonce) → true.
    /// Used to reject duplicate oracle payload submissions for the same round.
    ConsumedOracleNonce(u64, u64),
    /// Minimum participant count for competitive settlement; unset = no minimum enforced
    MinParticipants,
    /// Oracle heartbeat: last recorded timestamp and status
    OracleHeartbeat,
    /// Stale-heartbeat threshold in seconds (admin-configurable); unset = 3600 s default
    OracleStaleThreshold,
    /// Oracle max deviation threshold in basis points (1 bp = 0.01%).
    /// If unset, deviation guardrails are disabled.
    OracleMaxDeviationBps,
    /// One-shot admin override allowing the next settlement to bypass deviation checks.
    /// Automatically cleared after use.
    OracleDeviationOverrideArmed,
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
    ///
    /// The oracle service must generate a unique value per submission for a
    /// given round (e.g. a monotonic counter or random 64-bit value). The
    /// contract records each consumed nonce under
    /// `DataKey::ConsumedOracleNonce(round_id, nonce)` and rejects any reuse,
    /// making resolution idempotent against accidental duplicate submissions.
    pub nonce: u64,
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
