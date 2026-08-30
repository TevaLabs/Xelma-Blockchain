// SPDX-License-Identifier: MIT
//! Core contract implementation for the XLM Price Prediction Market.

#![allow(dead_code)]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, BytesN, Env, Map, Symbol, Vec};

use crate::access_control;
use crate::errors::ContractError;
use crate::governance;
use crate::types::{
    ArchivedRoundSummary, AccessState, BetSide, ConfigChangeKind, ConfigChangePayload, DataKeyCore,
    DataKeyScoped, DeviationReferenceMode, LeaderboardEntry, MultiFeedPayload, OneSidedPolicy,
    MarketSnapshot, OracleHeartbeatRecord,
    OraclePayload, OracleQuorumConfig, OracleRotationProposal, PendingConfigChange,
    PolicyAction, PrecisionPrediction, PriceSample, ProtocolHealthStatus, ProtocolStatus, Round,
    RoundArchiveStatus, RoundPhase, RoundPoolStats, RoundStatus, RoundTemplate, RuntimeMode,
    SeasonArchive, SeasonLeaderboardEntry, SimulationResult, UserPosition,
    UserRoundOutcome, UserStats, FeeModel, GovAction, GovProposal,
};

// ─── Economic control limits ─────────────────────────────────────────────────
/// Minimum allowed value when setting an economic cap to prevent zero-value lockouts.
const MIN_CAP_VALUE: i128 = 1;
/// Upper bound on the minimum-participants config to prevent unbounded gas in resolution.
const MAX_MIN_PARTICIPANTS: u32 = 10_000;
const DEFAULT_MAX_PRECISION_PARTICIPANTS: u32 = 1_000;
const MAX_PRECISION_PARTICIPANTS_LIMIT: u32 = 10_000;
/// Maximum number of entries returned per page by paginated query methods,
/// regardless of the caller-requested `limit` (Issue #139).
const MAX_PAGE_SIZE: u32 = 100;

// ─── Oracle heartbeat limits ──────────────────────────────────────────────────
const DEFAULT_ORACLE_STALE_THRESHOLD: u64 = 3_600; // 1 hour
const MIN_ORACLE_STALE_THRESHOLD: u64 = 60; // 1 minute
const MAX_ORACLE_STALE_THRESHOLD: u64 = 86_400; // 24 hours

// ─── Oracle rotation expiry ───────────────────────────────────────────────────
const MIN_ROTATION_EXPIRY_SECONDS: u64 = 60; // 1 minute minimum
/// Minimum delay between proposing and accepting an oracle rotation.
/// Prevents quiet takeovers: even with admin key compromise, a 1-hour window
/// gives operators and monitoring dashboards time to react.
const MIN_ROTATION_DELAY_SECONDS: u64 = 3_600; // 1 hour

const DEFAULT_BET_WINDOW_LEDGERS: u32 = 6;
const DEFAULT_RUN_WINDOW_LEDGERS: u32 = 12;
const MAX_BET_WINDOW_LEDGERS: u32 = 1_440;
const MAX_RUN_WINDOW_LEDGERS: u32 = 2_880;

const ROUND_MODE_UPDOWN: u32 = 0;
const ROUND_MODE_PRECISION: u32 = 1;
const PAYOUT_OUTCOME_LOSS: u32 = 0;
const PAYOUT_OUTCOME_WIN: u32 = 1;
const PAYOUT_OUTCOME_REFUND: u32 = 2;
// ─── Oracle deviation guardrails ─────────────────────────────────────────────
/// Maximum allowed basis points for oracle deviation is bounded to avoid absurd configs.
/// 100_000 bp = 1000% deviation (effectively "off", but still explicit).
const MAX_ORACLE_DEVIATION_BPS: u32 = 100_000;

// ─── Protocol fee (Issue #162) ────────────────────────────────────────────────
/// Hard cap on the optional protocol settlement fee, in basis points
/// (1 bp = 0.01%). 1_000 bp = 10% of the round's total pot — the maximum an
/// admin may ever schedule via timelock. Larger values would risk turning
/// the protocol into a de-facto extraction mechanism and are explicitly
/// disallowed to preserve user trust and the conservation invariant.
const MAX_PROTOCOL_FEE_BPS: u32 = 1_000;
/// Denominator for bps math: `fee = total_pot * bps / BPS_DENOMINATOR`.
/// Pinned to 10_000 to match the universal "1 bp = 0.01%" convention.
const BPS_DENOMINATOR: i128 = 10_000;

// ─── Storage schema versioning ───────────────────────────────────────────────
const CURRENT_SCHEMA_VERSION: u32 = 3;
// ─── Start-price bounds (Issue #119) ─────────────────────────────────────────
/// Minimum start price in protocol units — prevents zero-value and dust rounds.
const MIN_START_PRICE: u128 = 1;
/// Maximum start price in protocol units — guards against overflow in payout math.
const MAX_START_PRICE: u128 = 1_000_000_000_000_000_000;
// ─── Storage TTL Lifecycle Limits (Issue #142) ──────────────────────────────
/// Minimum remaining ledgers before a persistent entry is extended.
const TTL_BUMP_THRESHOLD: u32 = 17_280; // ~1 day at 5-second ledgers
/// Amount of ledgers to extend a persistent entry to when below threshold.
const TTL_BUMP_AMOUNT: u32 = 518_400; // ~30 days at 5-second ledgers

/// Default archived round summaries retained on-chain (FIFO pruning).
const DEFAULT_ARCHIVE_RETENTION: u32 = 128;
/// Minimum archive retention limit — prevents accidental pruning of all history.
const MIN_ARCHIVE_RETENTION: u32 = 1;
/// Maximum archive retention limit — prevents unbounded storage growth.
const MAX_ARCHIVE_RETENTION: u32 = 10_000;
/// Ledgers to wait before a scheduled critical config change may be applied (~2 hours).
const CONFIG_TIMELOCK_LEDGERS: u32 = 1440;

use crate::admin;
use crate::betting;
use crate::common;
use crate::config;
use crate::leaderboard;
use crate::queries;
use crate::settlement;

#[contract]
pub struct VirtualTokenContract;

#[contractimpl]
impl VirtualTokenContract {
    /// Initializes the contract with admin and oracle addresses (one-time only)
    pub fn initialize(env: Env, admin: Address, oracle: Address) -> Result<(), ContractError> {
        admin::initialize(env, admin, oracle)
    }

    /// Returns the stored schema version. If unset, returns legacy version 1.
    pub fn get_schema_version(env: Env) -> u32 {
        admin::get_schema_version(env)
    }

    /// Migrates legacy schema version 1 → version 2 (admin only).
    ///
    /// When `dry_run` is `true`, all validation checks are performed but no
    /// storage writes or events are emitted.
    pub fn migrate_schema_v1_to_v2(env: Env, dry_run: bool) -> Result<(), ContractError> {
        admin::migrate_schema_v1_to_v2(env, dry_run)
    }

    /// Migrates schema version 2 → version 3 (admin only).
    ///
    /// When `dry_run` is `true`, all validation checks are performed but no
    /// storage writes or events are emitted.
    pub fn migrate_schema_v2_to_v3(env: Env, dry_run: bool) -> Result<(), ContractError> {
        admin::migrate_schema_v2_to_v3(env, dry_run)
    }

    /// Announces a target schema version for the next planned migration (admin only).
    ///
    /// This sets a "v-next schema template" that operators can inspect before
    /// the real migration executes. It does NOT change the active schema.
    pub fn announce_next_schema(env: Env, target_version: u32) -> Result<(), ContractError> {
        admin::announce_next_schema(env, target_version)
    }

    /// Returns the announced next schema version, if any.
    pub fn get_next_schema(env: Env) -> Option<u32> {
        admin::get_next_schema(env)
    }

    /// Clears a previously announced next schema version (admin only).
    pub fn clear_next_schema(env: Env) -> Result<(), ContractError> {
        admin::clear_next_schema(env)
    }

    /// Returns whether the contract is currently paused
    pub fn is_paused(env: Env) -> bool {
        admin::is_paused(env)
    }

    /// Pauses the contract for emergency recovery (admin only)
    pub fn pause_contract(env: Env) -> Result<(), ContractError> {
        admin::pause_contract(env)
    }

    /// Unpauses the contract after recovery (admin only)
    pub fn unpause_contract(env: Env) -> Result<(), ContractError> {
        admin::unpause_contract(env)
    }

    /// Returns the current runtime mode (0 = Normal, 1 = ClaimsOnly, 2 = FullyPaused)
    pub fn get_runtime_mode(env: Env) -> u32 {
        admin::get_runtime_mode(env)
    }

    /// Sets the runtime mode of the contract (admin only)
    pub fn set_runtime_mode(env: Env, mode: u32) -> Result<(), ContractError> {
        admin::set_runtime_mode(env, mode)
    }

    /// Returns paginated archived participation history for a user (newest first).
    pub fn get_user_archive_history(
        env: Env,
        user: Address,
        offset: u32,
        limit: u32,
    ) -> Vec<ArchivedRoundSummary> {
        queries::get_user_archive_history(env, user, offset, limit)
    }

    /// Returns whether `action` is currently permitted under the PolicyGate
    /// for the contract's runtime mode (Issue #261). Read-only; does not
    /// mutate state. See [`admin::_policy_gate`] for the full matrix.
    pub fn is_action_allowed(env: Env, action: PolicyAction) -> bool {
        admin::_policy_gate(&env, action).is_ok()
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        admin::get_admin(env)
    }

    pub fn get_oracle(env: Env) -> Option<Address> {
        admin::get_oracle(env)
    }

    /// Schedules a timelocked oracle deviation update
    pub fn set_oracle_max_deviation_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
        admin::set_oracle_max_deviation_bps(env, bps)
    }

    /// Returns the configured oracle max deviation bps, if set.
    pub fn get_oracle_max_deviation_bps(env: Env) -> Option<u32> {
        admin::get_oracle_max_deviation_bps(env)
    }

    /// Sets the oracle deviation reference mode — `StartPrice` (default) or
    /// `Twap` — and, for `Twap`, the trailing sample window size (admin only, Issue #266).
    pub fn set_deviation_ref_mode(
        env: Env,
        mode: DeviationReferenceMode,
        window_samples: u32,
    ) -> Result<(), ContractError> {
        admin::set_deviation_ref_mode(env, mode, window_samples)
    }

    /// Returns the configured deviation reference mode (default `StartPrice`, Issue #266).
    pub fn get_deviation_ref_mode(env: Env) -> DeviationReferenceMode {
        admin::get_deviation_ref_mode(env)
    }

    /// Returns the configured TWAP window size in samples (Issue #266).
    pub fn get_deviation_window_samples(env: Env) -> u32 {
        admin::get_deviation_window_samples(env)
    }

    /// Returns the recorded TWAP price samples, most-recent last (Issue #266).
    pub fn get_twap_samples(env: Env) -> Vec<PriceSample> {
        settlement::_load_twap_samples(&env)
    }

    /// Sets (or clears) the ed25519 public key used to verify oracle
    /// attestation signatures (admin only, Issue #263). `None` disables
    /// attestation verification, restoring account-auth-only behaviour.
    pub fn set_attestation_key(env: Env, key: Option<BytesN<32>>) -> Result<(), ContractError> {
        admin::set_attestation_key(env, key)
    }

    /// Returns the configured attestation signing key, if enabled (Issue #263).
    pub fn get_attestation_key(env: Env) -> Option<BytesN<32>> {
        admin::get_attestation_key(env)
    }

    /// Arms a one-shot override to bypass deviation checks for the next settlement (admin only).
    pub fn arm_oracle_deviation_override(env: Env) -> Result<(), ContractError> {
        admin::arm_oracle_deviation_override(env)
    }

    /// Sets the minimum oracle confidence threshold in basis points (admin only).
    pub fn set_oracle_min_confidence_bps(
        env: Env,
        min_bps: Option<u32>,
    ) -> Result<(), ContractError> {
        admin::set_oracle_min_confidence_bps(env, min_bps)
    }

    /// Enables or disables strict mode for oracle confidence (admin only).
    pub fn set_oracle_strict_mode(env: Env, enabled: bool) -> Result<(), ContractError> {
        admin::set_oracle_strict_mode(env, enabled)
    }

    /// Returns the configured minimum oracle confidence bps, if set.
    pub fn get_oracle_min_confidence_bps(env: Env) -> Option<u32> {
        admin::get_oracle_min_confidence_bps(env)
    }

    /// Returns whether oracle strict mode is enabled.
    pub fn get_oracle_strict_mode(env: Env) -> bool {
        admin::get_oracle_strict_mode(env)
    }

    /// Enables or disables strict mode for oracle heartbeat health at settlement (admin only, Issue #264).
    pub fn set_hb_strict_mode(env: Env, enabled: bool) -> Result<(), ContractError> {
        admin::set_hb_strict_mode(env, enabled)
    }

    /// Returns whether oracle heartbeat strict mode is enabled (Issue #264).
    pub fn get_hb_strict_mode(env: Env) -> bool {
        admin::get_hb_strict_mode(env)
    }

    /// Arms a one-shot override to bypass the heartbeat health gate for the next settlement (admin only, Issue #264).
    pub fn arm_hb_override(env: Env) -> Result<(), ContractError> {
        admin::arm_hb_override(env)
    }

    /// Returns whether the oracle heartbeat override is currently armed (Issue #264).
    pub fn get_hb_override_armed(env: Env) -> bool {
        admin::get_hb_override_armed(env)
    }

    /// Sets the grace period in seconds between heartbeat staleness and settlement block (admin only, Issue #264).
    pub fn set_hb_grace_seconds(env: Env, seconds: u64) -> Result<(), ContractError> {
        admin::set_hb_grace_seconds(env, seconds)
    }

    /// Returns the configured heartbeat grace period in seconds (default 0, Issue #264).
    pub fn get_hb_grace_seconds(env: Env) -> u64 {
        admin::get_hb_grace_seconds(env)
    }

    /// Records an oracle heartbeat (oracle only).
    pub fn update_oracle_heartbeat(env: Env, status: u32) -> Result<(), ContractError> {
        admin::update_oracle_heartbeat(env, status)
    }

    /// Returns the most recent oracle heartbeat record, if any.
    pub fn get_oracle_heartbeat(env: Env) -> Option<OracleHeartbeatRecord> {
        admin::get_oracle_heartbeat(env)
    }

    /// Returns `true` if the oracle has a non-stale heartbeat with status not offline (2).
    pub fn is_oracle_live(env: Env) -> bool {
        admin::is_oracle_live(env)
    }

    /// Schedules a timelocked stale threshold update
    pub fn set_oracle_stale_threshold(env: Env, seconds: u64) -> Result<(), ContractError> {
        admin::set_oracle_stale_threshold(env, seconds)
    }

    /// Returns a composite protocol health status
    pub fn get_protocol_health(env: Env) -> ProtocolHealthStatus {
        admin::get_protocol_health(env)
    }

    /// Returns the configured oracle stale threshold, or the default if not set.
    /// Returns the global status of the protocol.
    ///
    /// This is the canonical single-call status endpoint for frontends and
    /// monitoring dashboards. The returned [`ProtocolStatus`] maps directly to
    /// the three mutually-exclusive states visible to end users:
    ///
    /// | return value      | meaning                                             |
    /// |-------------------|-----------------------------------------------------|
    /// | `Active`      (0) | A round is live; bets or reveals are accepted.      |
    /// | `Paused`      (1) | Emergency pause active; mutations rejected.          |
    /// | `ClaimsOnly`  (2) | No active round; only `claim_winnings` is useful.   |
    ///
    /// **Priority**: `Paused` is always returned first when the contract is
    /// paused, regardless of whether an active round exists.
    pub fn get_protocol_status(env: Env) -> ProtocolStatus {
        if Self::is_paused(env.clone()) {
            ProtocolStatus::Paused
        } else if env.storage().persistent().has(&DataKeyCore::ActiveRound) {
            ProtocolStatus::Active
        } else {
            ProtocolStatus::ClaimsOnly
        }
    }

    /// Returns the status of a specific round identified by `round_id`.
    ///
    /// Lookup strategy (in priority order):
    /// 1. If the round is the **current active round**, derive status from
    ///    ledger position relative to `bet_end_ledger` / `end_ledger`.
    /// 2. If the round appears in the **on-chain archive**, map its
    ///    [`RoundArchiveStatus`] to the corresponding terminal [`RoundStatus`].
    /// 3. If a `CancelledRound` marker exists (archive may be pruned),
    ///    return `Cancelled`.
    /// 4. Otherwise, return `Unknown`.
    ///
    /// | return value          | meaning                                                       |
    /// |-----------------------|---------------------------------------------------------------|
    /// | `Unknown`        (0)  | Round not found; never created or pruned from archive.       |
    /// | `Betting`        (1)  | Active; `ledger < bet_end_ledger`.                           |
    /// | `Running`        (2)  | Active; `bet_end_ledger ≤ ledger < end_ledger`.              |
    /// | `AwaitingResolve`(3)  | Active; `ledger ≥ end_ledger`, oracle not yet called.        |
    /// | `Resolved`       (4)  | Settled normally; pot distributed.                           |
    /// | `Cancelled`      (5)  | Admin-cancelled; stakes refunded.                            |
    /// | `FallbackRefund` (6)  | Settled with insufficient participants; stakes refunded.     |
    ///
    /// Note: `Betting`, `Running`, and `AwaitingResolve` are **derived** from
    /// ledger sequence — they do not involve additional storage writes.
    pub fn get_round_status(env: Env, round_id: u64) -> RoundStatus {
        // First check if it is the active round
        if let Some(active_round) = env
            .storage()
            .persistent()
            .get::<_, Round>(&DataKeyCore::ActiveRound)
        {
            if active_round.round_id == round_id {
                let phase = Self::_derive_round_phase(env.ledger().sequence(), &active_round);
                return match phase {
                    RoundPhase::Betting => RoundStatus::Betting,
                    RoundPhase::Running => RoundStatus::Running,
                    RoundPhase::Resolvable => RoundStatus::AwaitingResolve,
                };
            }
        }

        // Second, check the archived rounds summary
        let archive_key = DataKeyScoped::ArchivedRound(round_id);
        if let Some(archive) = env
            .storage()
            .persistent()
            .get::<_, ArchivedRoundSummary>(&archive_key)
        {
            return match archive.status {
                RoundArchiveStatus::Resolved => RoundStatus::Resolved,
                RoundArchiveStatus::Cancelled => RoundStatus::Cancelled,
                RoundArchiveStatus::FallbackRefund => RoundStatus::FallbackRefund,
                RoundArchiveStatus::Voided => RoundStatus::Voided,
            };
        }

        // Third, fallback check for cancelled rounds (in case it was pruned but CancelledRound flag remains)
        if Self::is_round_cancelled(env.clone(), round_id) {
            return RoundStatus::Cancelled;
        }

        // Otherwise, it's not active, not in archive, not cancelled.
        RoundStatus::Unknown
    }

    /// Returns the configured oracle stale threshold, or the default (3600 s) if not set.
    pub fn get_oracle_stale_threshold(env: Env) -> u64 {
        admin::get_oracle_stale_threshold(env)
    }

    /// Auth-gated batch TTL extension for allowlisted storage keys (admin only).
    ///
    /// Accepts a vector of `DataKeyCore` variants. Each key is validated against the
    /// TTL-touch allowlist. Keys that exist in storage have their TTL extended to
    /// `TTL_BUMP_AMOUNT` (~30 days). Keys not in the allowlist cause the entire
    /// call to fail with `UnsupportedDataKeyForTtlTouch`. Keys that are in the
    /// allowlist but absent from storage are silently skipped.
    ///
    /// Returns the number of keys whose TTL was actually extended.
    ///
    /// Event: `("storage", "touch")` with `(touched, skipped)` counts.
    pub fn batch_touch_ttl(env: Env, keys: Vec<DataKeyCore>) -> Result<u32, ContractError> {
        admin::batch_touch_ttl(env, keys)
    }

    // ─── Oracle rotation (two-step with expiry) ─────────────────────────────

    /// Proposes a new oracle address with an expiry window (admin only).
    ///
    /// The proposal must be accepted via [`Self::accept_oracle_rotation`] before
    /// `expires_in_seconds` elapses, otherwise acceptance is rejected.
    /// Minimum expiry is 60 seconds.
    ///
    /// Emits `("oracle", "propose")`.
    pub fn propose_oracle_rotation(
        env: Env,
        new_oracle: Address,
        expires_in_seconds: u64,
    ) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKeyCore::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        if expires_in_seconds < MIN_ROTATION_DELAY_SECONDS {
            return Err(ContractError::InvalidDuration);
        }

        let proposed_at = env.ledger().timestamp();
        let expires_at = proposed_at
            .checked_add(expires_in_seconds)
            .ok_or(ContractError::Overflow)?;

        let proposal = OracleRotationProposal {
            new_oracle: new_oracle.clone(),
            proposed_at,
            expires_at,
        };

        let key = DataKeyCore::OracleRotationProposal;
        env.storage().persistent().set(&key, &proposal);
        Self::_extend_persistent_ttl(&env, &key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("oracle"), symbol_short!("propose")),
            (new_oracle, expires_at),
        );

        Ok(())
    }

    /// Accepts a pending oracle rotation proposal before expiry (any caller).
    ///
    /// **Security**: A mandatory `MIN_ROTATION_DELAY_SECONDS` (1 hour) must
    /// elapse between proposal and acceptance. This prevents quiet one-block
    /// takeovers — even if the admin key is compromised, the community has a
    /// full hour to observe the proposal event and react before the oracle
    /// actually changes.
    ///
    /// If the delay has not elapsed the call returns `RotationDelayNotElapsed`.
    /// If the proposal has expired it returns `NoPendingRotation` and the
    /// stale proposal is removed after emitting `("oracle", "expired")`.
    /// On success the stored oracle address is updated and
    /// `("oracle", "accept")` is emitted with the previous and new addresses.
    pub fn accept_oracle_rotation(env: Env) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        Self::_ensure_not_paused(&env)?;

        let key = DataKeyCore::OracleRotationProposal;
        let proposal: OracleRotationProposal = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::NoPendingRotation)?;

        let current_ts = env.ledger().timestamp();

        // Mandatory delay before acceptance (prevents quiet takeovers)
        let earliest_accept = proposal
            .proposed_at
            .checked_add(MIN_ROTATION_DELAY_SECONDS)
            .ok_or(ContractError::Overflow)?;
        if current_ts < earliest_accept {
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("oracle"), symbol_short!("early")),
                (
                    proposal.new_oracle.clone(),
                    current_ts,
                    earliest_accept,
                ),
            );
            return Err(ContractError::RotationDelayNotElapsed);
        }

        if current_ts > proposal.expires_at {
            env.storage().persistent().remove(&key);
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("oracle"), symbol_short!("expired")),
                (
                    proposal.new_oracle,
                    proposal.proposed_at,
                    proposal.expires_at,
                ),
            );
            return Err(ContractError::NoPendingRotation);
        }

        let oracle_key = DataKeyCore::Oracle;
        let previous: Address = env
            .storage()
            .persistent()
            .get(&oracle_key)
            .ok_or(ContractError::OracleNotSet)?;

        env.storage()
            .persistent()
            .set(&oracle_key, &proposal.new_oracle);
        Self::_extend_persistent_ttl(&env, &oracle_key);
        env.storage().persistent().remove(&key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("oracle"), symbol_short!("accept")),
            (previous, proposal.new_oracle),
        );

        Ok(())
    }

    /// Cancels a pending oracle rotation proposal before it expires (admin only).
    ///
    /// Emits `("oracle", "cancel")` on success.
    pub fn cancel_oracle_rotation(env: Env) -> Result<(), ContractError> {
        Self::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKeyCore::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        Self::_ensure_not_paused(&env)?;

        let key = DataKeyCore::OracleRotationProposal;
        let proposal: OracleRotationProposal = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::NoPendingRotation)?;

        env.storage().persistent().remove(&key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("oracle"), symbol_short!("cancel")),
            (proposal.new_oracle,),
        );

        Ok(())
    }

    /// Returns the pending oracle rotation proposal, if any.
    pub fn get_oracle_rotation_proposal(env: Env) -> Option<OracleRotationProposal> {
        let key = DataKeyCore::OracleRotationProposal;
        Self::_extend_persistent_ttl(&env, &key);
        let proposal: Option<OracleRotationProposal> = env.storage().persistent().get(&key);
        if let Some(ref prop) = proposal {
            if env.ledger().timestamp() > prop.expires_at {
                env.storage().persistent().remove(&key);
                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("oracle"), symbol_short!("expired")),
                    (prop.new_oracle.clone(), prop.proposed_at, prop.expires_at),
                );
                return None;
            }
        }
        proposal
    }

    // ─── Participant access control (Issue #274) ────────────────────────────

    pub fn set_access_control_enabled(env: Env, enabled: bool) -> Result<(), ContractError> {
        access_control::set_access_control_enabled(env, enabled)
    }

    pub fn is_access_control_enabled(env: Env) -> bool {
        access_control::is_access_control_enabled(env)
    }

    pub fn add_allowlisted(env: Env, user: Address) -> Result<(), ContractError> {
        access_control::add_allowlisted(env, user)
    }

    pub fn remove_allowlisted(env: Env, user: Address) -> Result<(), ContractError> {
        access_control::remove_allowlisted(env, user)
    }

    pub fn add_denylisted(env: Env, user: Address) -> Result<(), ContractError> {
        access_control::add_denylisted(env, user)
    }

    pub fn remove_denylisted(env: Env, user: Address) -> Result<(), ContractError> {
        access_control::remove_denylisted(env, user)
    }

    pub fn is_allowlisted(env: Env, user: Address) -> bool {
        access_control::is_allowlisted(env, user)
    }

    pub fn is_denylisted(env: Env, user: Address) -> bool {
        access_control::is_denylisted(env, user)
    }

    pub fn get_access_state(env: Env, user: Address) -> AccessState {
        access_control::get_access_state(env, user)
    }

    pub fn get_access_policy(env: Env, user: Address) -> (bool, AccessState) {
        access_control::get_access_policy(env, user)
    }

    // ─── Dual-Approval Governance (Issue #272) ──────────────────────────────

    /// Configures the secondary governance approver (admin only).
    pub fn set_gov_approver(env: Env, approver: Address) -> Result<(), ContractError> {
        governance::set_gov_approver(env, approver)
    }

    /// Returns the configured secondary governance approver address, if set.
    pub fn get_gov_approver(env: Env) -> Option<Address> {
        governance::get_gov_approver(env)
    }

    /// Sets default proposal TTL in ledgers (admin only).
    pub fn set_gov_proposal_ttl(env: Env, ttl_ledgers: u32) -> Result<(), ContractError> {
        governance::set_gov_proposal_ttl(env, ttl_ledgers)
    }

    /// Returns default proposal TTL in ledgers.
    pub fn get_gov_proposal_ttl(env: Env) -> u32 {
        governance::get_gov_proposal_ttl(env)
    }

    /// Proposes a protected administrative action (governance admin/approver only).
    pub fn propose_gov_action(
        env: Env,
        proposer: Address,
        action: GovAction,
        custom_ttl: Option<u32>,
    ) -> Result<u64, ContractError> {
        governance::propose(env, proposer, action, custom_ttl)
    }

    /// Approves a pending governance proposal (governance admin/approver only, distinct from proposer).
    pub fn approve_gov_proposal(
        env: Env,
        approver: Address,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        governance::approve(env, approver, proposal_id)
    }

    /// Executes an approved governance proposal (governance admin/approver only).
    pub fn execute_gov_proposal(
        env: Env,
        executor: Address,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        governance::execute(env, executor, proposal_id)
    }

    /// Cancels an unexecuted governance proposal (governance admin/approver only).
    pub fn cancel_gov_proposal(
        env: Env,
        canceller: Address,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        governance::cancel(env, canceller, proposal_id)
    }

    /// Queries details for a governance proposal.
    pub fn get_gov_proposal(env: Env, proposal_id: u64) -> Option<GovProposal> {
        governance::get_gov_proposal(env, proposal_id)
    }

    /// Schedules a timelocked windows update (alias for [`Self::schedule_windows`]).
    /// bet_ledgers: Number of ledgers users can place bets
    /// run_ledgers: Total number of ledgers before round can be resolved
    pub fn set_windows(env: Env, bet_ledgers: u32, run_ledgers: u32) -> Result<(), ContractError> {
        config::set_windows(env, bet_ledgers, run_ledgers)
    }

    pub fn set_max_stake(env: Env, max_amount: Option<i128>) -> Result<(), ContractError> {
        config::set_max_stake(env, max_amount)
    }

    pub fn get_max_stake(env: Env) -> Option<i128> {
        config::get_max_stake(env)
    }

    /// Schedules a timelocked minimum-bet (dust protection) update (Issue #269).
    pub fn set_min_bet(env: Env, min_amount: Option<i128>) -> Result<(), ContractError> {
        config::set_min_bet(env, min_amount)
    }

    pub fn schedule_min_bet(env: Env, min_amount: Option<i128>) -> Result<(), ContractError> {
        config::schedule_min_bet(env, min_amount)
    }

    /// Returns the configured minimum bet, if enabled (Issue #269).
    pub fn get_min_bet(env: Env) -> Option<i128> {
        config::get_min_bet(env)
    }

    pub fn set_max_user_exposure(
        env: Env,
        max_exposure: Option<i128>,
    ) -> Result<(), ContractError> {
        config::set_max_user_exposure(env, max_exposure)
    }

    pub fn get_max_user_exposure(env: Env) -> Option<i128> {
        config::get_max_user_exposure(env)
    }

    pub fn set_max_pending_winnings(
        env: Env,
        max_pending: Option<i128>,
    ) -> Result<(), ContractError> {
        config::set_max_pending_winnings(env, max_pending)
    }

    pub fn schedule_windows(
        env: Env,
        bet_ledgers: u32,
        run_ledgers: u32,
    ) -> Result<(), ContractError> {
        config::schedule_windows(env, bet_ledgers, run_ledgers)
    }

    pub fn schedule_max_stake(env: Env, max_amount: Option<i128>) -> Result<(), ContractError> {
        config::schedule_max_stake(env, max_amount)
    }

    pub fn schedule_max_user_exposure(
        env: Env,
        max_exposure: Option<i128>,
    ) -> Result<(), ContractError> {
        config::schedule_max_user_exposure(env, max_exposure)
    }

    pub fn schedule_max_pending_winnings(
        env: Env,
        max_pending: Option<i128>,
    ) -> Result<(), ContractError> {
        config::schedule_max_pending_winnings(env, max_pending)
    }

    pub fn schedule_oracle_stale_threshold(env: Env, seconds: u64) -> Result<(), ContractError> {
        config::schedule_oracle_stale_threshold(env, seconds)
    }

    pub fn schedule_oracle_deviation_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
        config::schedule_oracle_deviation_bps(env, bps)
    }

    /// Schedules a timelocked update to the oracle timestamp skew (admin only).
    pub fn schedule_oracle_timestamp_skew(
        env: Env,
        seconds: u64,
    ) -> Result<(), ContractError> {
        config::schedule_oracle_timestamp_skew(env, seconds)
    }

    /// Returns the configured oracle timestamp skew, or the default (300 s) if not set.
    pub fn get_oracle_timestamp_skew(env: Env) -> u64 {
        config::get_oracle_timestamp_skew(env)
    }

    pub fn schedule_protocol_fee_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
        config::schedule_protocol_fee_bps(env, bps)
    }

    pub fn set_protocol_fee_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
        config::set_protocol_fee_bps(env, bps)
    }

    pub fn get_protocol_fee_bps(env: Env) -> Option<u32> {
        config::get_protocol_fee_bps(env)
    }

    pub fn get_protocol_fee_treasury(env: Env) -> i128 {
        config::get_protocol_fee_treasury(env)
    }

    pub fn withdraw_protocol_fee(
        env: Env,
        recipient: Address,
        amount: i128,
    ) -> Result<i128, ContractError> {
        config::withdraw_protocol_fee(env, recipient, amount)
    }

    pub fn get_pending_config_change(
        env: Env,
        kind: ConfigChangeKind,
    ) -> Option<PendingConfigChange> {
        config::get_pending_config_change(env, kind)
    }

    pub fn apply_scheduled_changes(env: Env, kind: ConfigChangeKind) -> Result<(), ContractError> {
        config::apply_scheduled_changes(env, kind)
    }

    pub fn cancel_config_change(env: Env, kind: ConfigChangeKind) -> Result<(), ContractError> {
        config::cancel_config_change(env, kind)
    }

    pub fn get_max_pending_winnings(env: Env) -> Option<i128> {
        config::get_max_pending_winnings(env)
    }

    pub fn set_min_participants(env: Env, min: Option<u32>) -> Result<(), ContractError> {
        config::set_min_participants(env, min)
    }

    pub fn get_min_participants(env: Env) -> Option<u32> {
        config::get_min_participants(env)
    }

    pub fn set_max_precision_participants(env: Env, max: u32) -> Result<(), ContractError> {
        config::set_max_precision_participants(env, max)
    }

    pub fn get_max_precision_participants(env: Env) -> u32 {
        config::get_max_precision_participants(env)
    }

    pub fn set_precision_payout_policy(env: Env, policy: u32) -> Result<(), ContractError> {
        config::set_precision_payout_policy(env, policy)
    }

    pub fn get_precision_payout_policy(env: Env) -> u32 {
        config::get_precision_payout_policy(env)
    }

    pub fn set_mint_limit(env: Env, limit: u32) -> Result<(), ContractError> {
        config::set_mint_limit(env, limit)
    }

    pub fn get_mint_limit(env: Env) -> u32 {
        config::get_mint_limit(env)
    }

    pub fn set_epoch_mint_budget(env: Env, budget: i128) -> Result<(), ContractError> {
        config::set_epoch_mint_budget(env, budget)
    }

    pub fn get_epoch_mint_budget(env: Env) -> i128 {
        config::get_epoch_mint_budget(env)
    }

    pub fn set_archive_retention(env: Env, limit: u32) -> Result<(), ContractError> {
        config::set_archive_retention(env, limit)
    }

    pub fn get_archive_retention(env: Env) -> u32 {
        config::get_archive_retention(env)
    }

    pub fn set_pending_winnings_expiry(env: Env, ledgers: u32) -> Result<(), ContractError> {
        config::set_pending_winnings_expiry(env, ledgers)
    }

    pub fn schedule_pending_winnings_expiry(env: Env, ledgers: u32) -> Result<(), ContractError> {
        config::schedule_pending_winnings_expiry(env, ledgers)
    }

    pub fn get_pending_winnings_expiry(env: Env) -> u32 {
        config::get_pending_winnings_expiry(env)
    }

    pub fn reclaim_expired_pending_winnings(
        env: Env,
        user: Address,
    ) -> Result<i128, ContractError> {
        admin::reclaim_expired_pending_winnings(env, user)
    }

    pub fn set_close_buffer_ledgers(env: Env, buffer_ledgers: u32) -> Result<(), ContractError> {
        config::set_close_buffer_ledgers(env, buffer_ledgers)
    }

    /// Sets the multi-feed oracle quorum configuration (admin only).
    ///
    /// When `Some(config)`, `resolve_round_multi` is enabled. When `None`,
    /// multi-feed resolution is disabled. The legacy path is unaffected.
    pub fn set_oracle_quorum_config(
        env: Env,
        config: Option<OracleQuorumConfig>,
    ) -> Result<(), ContractError> {
        admin::set_oracle_quorum_config(env, config)
    }

    /// Returns the configured multi-feed oracle quorum config, if any.
    pub fn get_oracle_quorum_config(env: Env) -> Option<OracleQuorumConfig> {
        admin::get_oracle_quorum_config(env)
    }

    pub fn get_close_buffer_ledgers(env: Env) -> u32 {
        config::get_close_buffer_ledgers(env)
    }

    /// Returns the configured betting-window length in ledgers.
    pub fn get_bet_window_ledgers(env: Env) -> u32 {
        config::get_bet_window_ledgers(env)
    }

    /// Returns the configured run-window length in ledgers.
    pub fn get_run_window_ledgers(env: Env) -> u32 {
        config::get_run_window_ledgers(env)
    }

    /// Sets the early cash-out penalty rate in basis points (admin only).
    /// `None` disables early cash-out entirely (default).
    /// `Some(bps)` enables it with the given penalty rate (1–1000 bps).
    pub fn set_early_cashout_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
        config::set_early_cashout_bps(env, bps)
    }

    /// Returns the configured early cash-out penalty bps, if enabled.
    pub fn get_early_cashout_bps(env: Env) -> Option<u32> {
        config::get_early_cashout_bps(env)
    }

    /// Creates a new prediction round (admin only)
    pub fn create_round(
        env: Env,
        start_price: u128,
        mode: Option<u32>,
    ) -> Result<(), ContractError> {
        betting::create_round(env, start_price, mode)
    }

    /// Stores the admin's blueprint for `create_next_from_template` (admin only).
    pub fn set_round_template(
        env: Env,
        start_price: u128,
        mode: Option<u32>,
    ) -> Result<(), ContractError> {
        config::set_round_template(env, start_price, mode)
    }

    /// Removes the configured round template (admin only).
    pub fn clear_round_template(env: Env) -> Result<(), ContractError> {
        config::clear_round_template(env)
    }

    /// Returns the configured round template, if any.
    pub fn get_round_template(env: Env) -> Option<RoundTemplate> {
        config::get_round_template(env)
    }

    /// Creates the next round from the configured template (admin only).
    /// Fails with `RoundAlreadyActive` if a round is already active and
    /// with `NoRoundTemplate` if no template has been configured.
    pub fn create_next_from_template(env: Env) -> Result<u64, ContractError> {
        betting::create_next_from_template(env)
    }

    pub fn place_bet(
        env: Env,
        user: Address,
        amount: i128,
        side: BetSide,
    ) -> Result<(), ContractError> {
        betting::place_bet(env, user, amount, side)
    }

    pub fn place_precision_prediction(
        env: Env,
        user: Address,
        amount: i128,
        predicted_price: u128,
    ) -> Result<(), ContractError> {
        betting::place_precision_prediction(env, user, amount, predicted_price)
    }

    pub fn predict_price(
        env: Env,
        user: Address,
        guessed_price: u128,
        amount: i128,
    ) -> Result<(), ContractError> {
        betting::predict_price(env, user, guessed_price, amount)
    }

    pub fn commit_prediction(
        env: Env,
        user: Address,
        hash: BytesN<32>,
        amount: i128,
    ) -> Result<(), ContractError> {
        betting::commit_prediction(env, user, hash, amount)
    }

    pub fn reveal_prediction(
        env: Env,
        user: Address,
        predicted_price: u128,
        salt: BytesN<32>,
    ) -> Result<(), ContractError> {
        betting::reveal_prediction(env, user, predicted_price, salt)
    }

    /// Mints 1000 vXLM for new users (one-time only)
    pub fn mint_initial(env: Env, user: Address) -> i128 {
        betting::mint_initial(env, user)
    }

    pub fn resolve_round(env: Env, payload: OraclePayload) -> Result<(), ContractError> {
        settlement::resolve_round(env, payload)
    }

    /// Resolves the active round using a multi-feed oracle payload with
    /// median settlement and quorum-based outlier rejection.
    ///
    /// Requires `OracleQuorumConfig` to be configured by the admin before
    /// this path is available. The legacy single-oracle `resolve_round`
    /// remains available independently.
    pub fn resolve_round_multi(
        env: Env,
        payload: MultiFeedPayload,
    ) -> Result<(), ContractError> {
        settlement::resolve_round_multi(env, payload)
    }

    pub fn cancel_round(env: Env, reason: u32) -> Result<(), ContractError> {
        settlement::cancel_round(env, reason)
    }

    pub fn is_round_cancelled(env: Env, round_id: u64) -> bool {
        settlement::is_round_cancelled(env, round_id)
    }

    pub fn claim_winnings(env: Env, user: Address) -> Result<i128, ContractError> {
        settlement::claim_winnings(env, user)
    }

    /// Claims pending winnings for up to `MAX_CLAIM_BATCH_SIZE` users in one
    /// call. All-or-nothing: any failure (batch too large, a duplicate
    /// address, or a missing per-user auth) reverts every effect in this
    /// call. See `settlement::claim_many` for full semantics.
    pub fn claim_many(env: Env, users: Vec<Address>) -> Result<Vec<i128>, ContractError> {
        settlement::claim_many(env, users)
    }

    /// Early cash-out during the Running phase for UpDown rounds.
    ///
    /// Allows a bettor to exit their position early, forfeiting a percentage
    /// of their stake to the protocol treasury. The forfeited amount is
    /// determined by the `EarlyCashoutBps` config (set by admin).
    ///
    /// # Errors
    /// - `EarlyCashoutDisabled` — feature not enabled (no penalty bps configured)
    /// - `EarlyCashoutPhaseInvalid` — not in Running phase
    /// - `EarlyCashoutNotUpDown` — round is not UpDown mode
    /// - `NoActiveRound` — no active round exists
    /// - `PositionNotFound` — user has no position in the active round
    pub fn cash_out_early(env: Env, user: Address) -> Result<(), ContractError> {
        betting::cash_out_early(env, user)
    }

    // ─── Dispute window / void-to-refund (Issue #276) ──────────────────────

    pub fn set_dispute_ledgers(env: Env, ledgers: u32) -> Result<(), ContractError> {
        config::set_dispute_ledgers(env, ledgers)
    }

    pub fn get_dispute_ledgers(env: Env) -> u32 {
        config::get_dispute_ledgers(&env)
    }

    /// Anyone may call `void_round` during the dispute window to refund all
    /// participants their full stakes (void-to-refund path).
    pub fn void_round(env: Env, round_id: u64) -> Result<(), ContractError> {
        settlement::void_round(env, round_id)
    }

    /// Anyone may call `finalize_round` after the dispute window expires to
    /// distribute winnings to winners (normal settlement outcome).
    pub fn finalize_round(env: Env, round_id: u64) -> Result<(), ContractError> {
        settlement::_finalize_round(env, round_id)
    }

    pub fn get_active_round(env: Env) -> Option<Round> {
        queries::get_active_round(env)
    }

    pub fn get_one_sided_policy(env: Env) -> OneSidedPolicy {
        let active_round: Option<Round> = env.storage().persistent().get(&DataKeyCore::ActiveRound);
        if let Some(round) = active_round {
            settlement::_select_one_sided_policy(&round)
        } else {
            OneSidedPolicy::Refund
        }
    }

    pub fn get_round_pool_stats(env: Env) -> Option<RoundPoolStats> {
        queries::get_round_pool_stats(env)
    }

    pub fn get_round_phase(env: Env) -> Result<RoundPhase, ContractError> {
        queries::get_round_phase(env)
    }

    /// Returns a single-read composite snapshot of current market state:
    /// round phase, pool composition, timing buffers, and fee configuration.
    /// See `MarketSnapshot` for empty-round semantics.
    pub fn get_market_snapshot(env: Env) -> MarketSnapshot {
        queries::get_market_snapshot(env)
    }

    pub fn get_last_round_id(env: Env) -> u64 {
        queries::get_last_round_id(env)
    }

    pub fn get_archived_round(env: Env, round_id: u64) -> Option<ArchivedRoundSummary> {
        queries::get_archived_round(env, round_id)
    }

    pub fn get_recent_archived_rounds(env: Env, limit: u32) -> Vec<ArchivedRoundSummary> {
        queries::get_recent_archived_rounds(env, limit)
    }

    pub fn get_user_archived_participation(
        env: Env,
        user: Address,
        round_id: u64,
    ) -> Option<UserRoundOutcome> {
        queries::get_user_archived_participation(env, user, round_id)
    }

    pub fn get_user_stats(env: Env, user: Address) -> UserStats {
        queries::get_user_stats(env, user)
    }

    pub fn get_pending_winnings(env: Env, user: Address) -> i128 {
        queries::get_pending_winnings(env, user)
    }

    pub fn get_user_position(env: Env, user: Address) -> Option<UserPosition> {
        queries::get_user_position(env, user)
    }

    pub fn get_user_precision_prediction(env: Env, user: Address) -> Option<PrecisionPrediction> {
        queries::get_user_precision_prediction(env, user)
    }

    pub fn get_precision_predictions(env: Env) -> Vec<PrecisionPrediction> {
        queries::get_precision_predictions(env)
    }

    pub fn get_updown_positions(env: Env) -> Map<Address, UserPosition> {
        queries::get_updown_positions(env)
    }

    pub fn get_precision_predictions_page(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Vec<PrecisionPrediction> {
        queries::get_precision_predictions_page(env, offset, limit)
    }

    pub fn get_updown_positions_page(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Vec<(Address, UserPosition)> {
        queries::get_updown_positions_page(env, offset, limit)

    }

    /// Returns user's vXLM balance
    pub fn balance(env: Env, user: Address) -> i128 {
        common::balance(env, user)
    }

    /// Estimates payouts for the active round given a hypothetical final price.
    /// Does not mutate storage. Returns SimulationResult.
    pub fn simulate_payout(env: Env, final_price: u128) -> Result<SimulationResult, ContractError> {
        queries::simulate_payout(env, final_price)
    }

    // ─── Fee incidence model (Issue #268) ──────────────────────────────────

    /// Sets the fee incidence model (admin only).
    ///
    /// `FeeOnPot` (0): fee is calculated on the total round pot (default).
    /// `FeeOnWinnings` (1): fee is calculated only on net winnings / profit.
    pub fn set_fee_model(env: Env, model: FeeModel) -> Result<(), ContractError> {
        config::set_fee_model(env, model)
    }

    /// Returns the configured fee incidence model, defaulting to `FeeOnPot`.
    pub fn get_fee_model(env: Env) -> FeeModel {
        config::get_fee_model(env)
    }

    // ─── Leaderboards (lifetime + seasons) ──────────────────────────────────

    /// Cursor-based page of the global leaderboard ordered by total wins descending.
    pub fn get_leaderboard_by_wins(
        env: Env,
        cursor: Option<Address>,
        limit: u32,
    ) -> (Vec<LeaderboardEntry>, Option<Address>) {
        queries::get_leaderboard_by_wins(env, cursor, limit)
    }

    /// Cursor-based page of the global leaderboard ordered by best streak descending.
    pub fn get_leaderboard_by_streak(
        env: Env,
        cursor: Option<Address>,
        limit: u32,
    ) -> (Vec<LeaderboardEntry>, Option<Address>) {
        queries::get_leaderboard_by_streak(env, cursor, limit)
    }
    // ─── Leaderboards (lifetime + seasons) ──────────────────────────────────

    /// Returns the id of the currently-active leaderboard season (default 1).
    pub fn get_current_season_id(env: Env) -> u32 {
        leaderboard::get_current_season_id(env)
    }

    /// Returns a user's season-scoped stats for `season_id` (active or archived).
    pub fn get_season_user_stats(env: Env, season_id: u32, user: Address) -> UserStats {
        leaderboard::get_season_user_stats(env, season_id, user)
    }

    /// Freezes the active season's rankings into a permanent archive and
    /// advances to the next season (admin only). Returns the new season id.
    pub fn reset_leaderboard_season(env: Env) -> Result<u32, ContractError> {
        leaderboard::reset_leaderboard_season(env)
    }

    /// Returns the frozen archive for a past season, if it has been reset.
    pub fn get_season_archive(env: Env, season_id: u32) -> Option<SeasonArchive> {
        leaderboard::get_season_archive(env, season_id)
    }

    /// Paginated wins leaderboard for `season_id` — live for the active
    /// season, frozen archive for any past season.
    pub fn get_season_leaderboard_by_wins(
        env: Env,
        season_id: u32,
        offset: u32,
        limit: u32,
    ) -> Vec<SeasonLeaderboardEntry> {
        leaderboard::get_season_leaderboard_by_wins(env, season_id, offset, limit)
    }

    /// Paginated best-streak leaderboard for `season_id` — live for the
    /// active season, frozen archive for any past season.
    pub fn get_season_leaderboard_by_streak(
        env: Env,
        season_id: u32,
        offset: u32,
        limit: u32,
    ) -> Vec<SeasonLeaderboardEntry> {
        leaderboard::get_season_leaderboard_by_streak(env, season_id, offset, limit)
    }
}

impl VirtualTokenContract {
    pub(crate) fn _set_balance(env: &Env, user: Address, amount: i128) {
        let key = DataKeyScoped::Balance(user);
        env.storage().persistent().set(&key, &amount);
        Self::_extend_persistent_ttl(env, &key);
    }

    /// Delegates to the central [`PolicyGate`](crate::admin::_policy_gate) — see its
    /// doc comment for the full mode × action matrix and entrypoint inventory (Issue #261).
    fn _ensure_not_paused(env: &Env) -> Result<(), ContractError> {
        crate::admin::_policy_gate(env, PolicyAction::AdminConfig)
    }

    /// Delegates to the central [`PolicyGate`](crate::admin::_policy_gate) — see its
    /// doc comment for the full mode × action matrix and entrypoint inventory (Issue #261).
    fn _ensure_normal_mode(env: &Env) -> Result<(), ContractError> {
        crate::admin::_policy_gate(env, PolicyAction::RoundMutation)
    }

    fn _set_mode(env: &Env, new_mode: RuntimeMode) -> Result<(), ContractError> {
        let key = DataKeyCore::Paused;
        let old_mode = env
            .storage()
            .persistent()
            .get::<_, RuntimeMode>(&key)
            .unwrap_or(RuntimeMode::Normal);
        if old_mode != new_mode {
            env.storage().persistent().set(&key, &new_mode);
            Self::_extend_persistent_ttl(env, &key);
            #[allow(deprecated)]
            env.events().publish(
                (symbol_short!("mode"), Symbol::new(env, "transition")),
                (old_mode as u32, new_mode as u32),
            );
        }
        Ok(())
    }

    /// Derives the round lifecycle phase for `round` at `ledger_sequence`.
    fn _derive_round_phase(ledger_sequence: u32, round: &Round) -> RoundPhase {
        if ledger_sequence < round.bet_end_ledger {
            RoundPhase::Betting
        } else if ledger_sequence < round.end_ledger {
            RoundPhase::Running
        } else {
            RoundPhase::Resolvable
        }
    }

    fn _schema_version(env: &Env) -> Option<u32> {
        env.storage().persistent().get(&DataKeyCore::SchemaVersion)
    }

    fn _require_supported_schema(env: &Env) -> Result<u32, ContractError> {
        Self::_extend_persistent_ttl(env, &DataKeyCore::SchemaVersion);
        if env.storage().persistent().has(&DataKeyCore::Admin) {
            Self::_extend_persistent_ttl(env, &DataKeyCore::Admin);
        }
        let v = Self::_schema_version(env).unwrap_or(1);
        if v == 0 || v > CURRENT_SCHEMA_VERSION {
            return Err(ContractError::UnsupportedSchemaVersion);
        }
        Ok(v)
    }

    fn assert_no_active_round(env: &Env) -> Result<(), ContractError> {
        if env.storage().persistent().has(&DataKeyCore::ActiveRound) {
            return Err(ContractError::RoundAlreadyActive);
        }

        Ok(())
    }

    /// Checked addition for payout accumulation.
    ///
    /// All payout aggregation (refunds, winnings, precision payouts) routes
    /// through this helper so overflow always maps to the stable
    /// `PayoutOverflow` variant rather than a generic `Overflow`. This makes
    /// the failure mode auditable and distinguishable from non-financial
    /// overflow (e.g. round-ID counter, ledger arithmetic).
    ///
    /// All-or-nothing guarantee: callers must not mutate storage before all
    /// payout math is complete and checked. The functions below enforce this
    /// by computing the new value first and only writing it afterward.
    #[inline(always)]
    fn payout_add(a: i128, b: i128) -> Result<i128, ContractError> {
        a.checked_add(b).ok_or(ContractError::PayoutOverflow)
    }

    #[inline(always)]
    fn payout_mul(a: i128, b: i128) -> Result<i128, ContractError> {
        a.checked_mul(b).ok_or(ContractError::PayoutOverflow)
    }

    fn _emit_payout_outcome(
        env: &Env,
        round_id: u64,
        mode: u32,
        user: Address,
        gross_payout: i128,
        outcome_type: u32,
    ) {
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("payout"), symbol_short!("outcome")),
            (round_id, mode, user, gross_payout, outcome_type),
        );
    }

    /// Accumulates `amount` into a user's pending winnings, enforcing the cap if set (Issue #120).
    ///
    /// Reads and writes `DataKeyScoped::PendingWinnings(user)` in one place, ensuring the cap
    /// check and overflow protection are applied consistently across all payout paths.
    fn _accumulate_pending(env: &Env, user: Address, amount: i128) -> Result<(), ContractError> {
        let key = DataKeyScoped::PendingWinnings(user);
        let existing: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_pending = Self::payout_add(existing, amount)?;

        // Enforce pending winnings cap if configured
        if let Some(cap) = env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKeyCore::MaxPendingWinnings)
        {
            if new_pending > cap {
                return Err(ContractError::PendingWinningsCapExceeded);
            }
        }

        env.storage().persistent().set(&key, &new_pending);
        Self::_extend_persistent_ttl(env, &key);
        Ok(())
    }

    fn _validate_windows(bet_ledgers: u32, run_ledgers: u32) -> Result<(), ContractError> {
        if bet_ledgers == 0 || run_ledgers == 0 {
            return Err(ContractError::InvalidDuration);
        }
        if bet_ledgers > MAX_BET_WINDOW_LEDGERS || run_ledgers > MAX_RUN_WINDOW_LEDGERS {
            return Err(ContractError::WindowOutOfRange);
        }
        if bet_ledgers >= run_ledgers {
            return Err(ContractError::InvalidDuration);
        }
        Ok(())
    }

    fn _validate_max_stake(max_amount: Option<i128>) -> Result<(), ContractError> {
        if let Some(v) = max_amount {
            if v < MIN_CAP_VALUE {
                return Err(ContractError::InvalidBetAmount);
            }
        }
        Ok(())
    }

    fn _validate_oracle_stale_threshold(seconds: u64) -> Result<(), ContractError> {
        if !(MIN_ORACLE_STALE_THRESHOLD..=MAX_ORACLE_STALE_THRESHOLD).contains(&seconds) {
            return Err(ContractError::InvalidDuration);
        }
        Ok(())
    }

    fn _validate_oracle_max_deviation_bps(bps: Option<u32>) -> Result<(), ContractError> {
        if let Some(v) = bps {
            if v == 0 || v > MAX_ORACLE_DEVIATION_BPS {
                return Err(ContractError::WindowOutOfRange);
            }
        }
        Ok(())
    }

    /// Validates a requested protocol-fee bps (Issue #162).
    /// `None` always allowed (disables fee entirely, restoring pre-#162
    /// byte-for-byte behaviour). `Some(0)` is rejected — only explicit `None`
    /// is the legitimate way to express "fee disabled". `Some(bps)` must
    /// satisfy `1 <= bps <= MAX_PROTOCOL_FEE_BPS`.
    fn _validate_protocol_fee_bps(bps: Option<u32>) -> Result<(), ContractError> {
        if let Some(v) = bps {
            if v == 0 || v > MAX_PROTOCOL_FEE_BPS {
                return Err(ContractError::InvalidProtocolFeeBps);
            }
        }
        Ok(())
    }

    /// Reads the currently-configured protocol fee in bps (Issue #162).
    /// Bumps TTL only when the key is present (avoids extra storage writes
    /// on the hot "fee disabled" path through every competitive settlement).
    fn _read_protocol_fee_bps(env: &Env) -> Option<u32> {
        let key = DataKeyCore::ProtocolFeeBps;
        let v: Option<u32> = env.storage().persistent().get(&key);
        if v.is_some() {
            Self::_extend_persistent_ttl(env, &key);
        }
        v
    }

    /// Credits `fee_amount` stroops to the protocol fee treasury and emits
    /// `("protocol", "fee_collected")` (Issue #162). TTL on the treasury
    /// key is extended on every write so the cumulative balance never
    /// falls into archival. Payload mirrors the active bps so indexers
    /// do not need an extra storage read.
    fn _collect_protocol_fee(
        env: &Env,
        round_id: u64,
        fee_amount: i128,
        bps_active: Option<u32>,
    ) -> Result<(), ContractError> {
        if fee_amount <= 0 {
            return Ok(());
        }
        let treasury_key = DataKeyCore::ProtocolFeeTreasury;
        let current: i128 = env.storage().persistent().get(&treasury_key).unwrap_or(0);
        let new_treasury = current
            .checked_add(fee_amount)
            .ok_or(ContractError::Overflow)?;
        env.storage().persistent().set(&treasury_key, &new_treasury);
        Self::_extend_persistent_ttl(env, &treasury_key);

        let bps_value: u32 = bps_active.unwrap_or(0);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("protocol"), symbol_short!("collected")),
            (round_id, fee_amount, new_treasury, bps_value),
        );

        Ok(())
    }

    /// Splits a `(winning_pool, losing_pool)` pair into the post-fee pools
    /// and the treasury's cut, used by both UpDown settlement paths
    /// (Issue #162). Conservation invariant
    ///   dist_winning + dist_losing + fee == winning + losing
    /// holds ALWAYS, even in the pathological case `fee > losing_pool`
    /// (very thin losing-side liquidity near the bps cap): the spillover
    /// is then deducted from `winning_pool`, so winners lose a portion
    /// of their principal rather than the fee being silently dropped.
    /// Behaviour is documented in `docs/EVENT_SCHEMA.md` and exercised
    /// by `test_protocol_fee_thin_losing_pool`.
    fn _apply_protocol_fee_updown(
        env: &Env,
        round_id: u64,
        winning_pool: i128,
        losing_pool: i128,
    ) -> Result<(i128, i128, i128), ContractError> {
        let bps = Self::_read_protocol_fee_bps(env);
        if bps.is_none() {
            return Ok((winning_pool, losing_pool, 0));
        }
        let bps_value = bps.unwrap();
        let total_pot = Self::payout_add(winning_pool, losing_pool)?;
        let fee_amount = total_pot
            .checked_mul(bps_value as i128)
            .ok_or(ContractError::Overflow)?
            / BPS_DENOMINATOR;
        if fee_amount == 0 {
            return Ok((winning_pool, losing_pool, 0));
        }
        let fee_from_losing = fee_amount.min(losing_pool);
        let fee_from_winning = fee_amount
            .checked_sub(fee_from_losing)
            .ok_or(ContractError::Overflow)?;
        let dist_winning = winning_pool
            .checked_sub(fee_from_winning)
            .ok_or(ContractError::Overflow)?;
        let dist_losing = losing_pool
            .checked_sub(fee_from_losing)
            .ok_or(ContractError::Overflow)?;
        Self::_collect_protocol_fee(env, round_id, fee_amount, Some(bps_value))?;
        Ok((dist_winning, dist_losing, fee_amount))
    }

    /// Splits a precision-mode `total_pot` into the distributable amount
    /// (split among winners per the existing remainder policy) and the
    /// treasury's cut (Issue #162). Returns `(distributable, fee_amount)`.
    fn _apply_protocol_fee_precision(
        env: &Env,
        round_id: u64,
        total_pot: i128,
    ) -> Result<(i128, i128), ContractError> {
        let bps = Self::_read_protocol_fee_bps(env);
        if bps.is_none() || total_pot <= 0 {
            return Ok((total_pot, 0));
        }
        let bps_value = bps.unwrap();
        let fee_amount = total_pot
            .checked_mul(bps_value as i128)
            .ok_or(ContractError::Overflow)?
            / BPS_DENOMINATOR;
        let distributable = total_pot
            .checked_sub(fee_amount)
            .ok_or(ContractError::Overflow)?;
        if fee_amount > 0 {
            Self::_collect_protocol_fee(env, round_id, fee_amount, Some(bps_value))?;
        }
        Ok((distributable, fee_amount))
    }

    fn _emit_action_rejected(env: &Env, actor: &Address, action: Symbol, reason: ContractError) {
        // Privacy: event payload contains only the actor Address, an action
        // symbol, and a numeric reason code. No personally identifiable
        // information, financial amounts, or internal state is exposed.
        // Operators can match reason codes against ContractError variants.
        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("action"), symbol_short!("rejct")),
            (actor.clone(), action, reason as u32),
        );
    }

    fn _current_config_payload(env: &Env, kind: &ConfigChangeKind) -> ConfigChangePayload {
        config::_current_config_payload(env, kind)
    }

    fn _schedule_config_change(
        env: &Env,
        kind: ConfigChangeKind,
        payload: ConfigChangePayload,
    ) -> Result<(), ContractError> {
        config::_schedule_config_change(env, kind, payload)
    }

    fn _apply_config_payload(
        env: &Env,
        kind: &ConfigChangeKind,
        payload: &ConfigChangePayload,
    ) -> Result<(), ContractError> {
        config::_apply_config_payload(env, kind, payload)
    }

    fn _extend_persistent_ttl<T: soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>>(env: &Env, key: &T) {
        if env.storage().persistent().has(key) {
            env.storage()
                .persistent()
                .extend_ttl(key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
        }
    }
}

impl VirtualTokenContract {
    pub fn _update_stats_win(env: &Env, user: Address) -> Result<(), ContractError> {
        settlement::_update_stats_win(env, user)
    }

    pub fn _update_stats_loss(env: &Env, user: Address) -> Result<(), ContractError> {
        settlement::_update_stats_loss(env, user)
    }
}
