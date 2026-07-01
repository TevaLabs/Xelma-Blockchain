// SPDX-License-Identifier: MIT
//! Contract entrypoint for the XLM Price Prediction Market.

use soroban_sdk::{contract, contractimpl, symbol_short, Address, BytesN, Env, Map, Vec};

use crate::errors::ContractError;
use crate::types::{
    ArchivedRoundSummary, BetSide, ConfigChangeKind, DataKey, OracleHeartbeatRecord, OraclePayload,
    OracleRotationProposal, PendingConfigChange, PrecisionPrediction, ProtocolHealthStatus,
    ProtocolStatus, Round, RoundArchiveStatus, RoundPhase, RoundPoolStats, RoundStatus,
    UserPosition, UserRoundOutcome, UserStats,
};
use crate::{admin, betting, common, config, queries, settlement};

const MIN_ROTATION_EXPIRY_SECONDS: u64 = 60;

#[contract]
pub struct VirtualTokenContract;

#[contractimpl]
impl VirtualTokenContract {
    pub fn initialize(env: Env, admin: Address, oracle: Address) -> Result<(), ContractError> {
        admin::initialize(env, admin, oracle)
    }
    pub fn get_schema_version(env: Env) -> u32 {
        admin::get_schema_version(env)
    }
    pub fn migrate_schema_v1_to_v2(env: Env) -> Result<(), ContractError> {
        admin::migrate_schema_v1_to_v2(env)
    }
    pub fn migrate_schema_v2_to_v3(env: Env) -> Result<(), ContractError> {
        admin::migrate_schema_v2_to_v3(env)
    }
    pub fn is_paused(env: Env) -> bool {
        admin::is_paused(env)
    }
    pub fn pause_contract(env: Env) -> Result<(), ContractError> {
        admin::pause_contract(env)
    }
    pub fn unpause_contract(env: Env) -> Result<(), ContractError> {
        admin::unpause_contract(env)
    }
    pub fn get_runtime_mode(env: Env) -> u32 {
        admin::get_runtime_mode(env)
    }
    pub fn set_runtime_mode(env: Env, mode: u32) -> Result<(), ContractError> {
        admin::set_runtime_mode(env, mode)
    }
    pub fn get_admin(env: Env) -> Option<Address> {
        admin::get_admin(env)
    }
    pub fn get_oracle(env: Env) -> Option<Address> {
        admin::get_oracle(env)
    }
    pub fn set_oracle_max_deviation_bps(env: Env, bps: Option<u32>) -> Result<(), ContractError> {
        admin::set_oracle_max_deviation_bps(env, bps)
    }
    pub fn get_oracle_max_deviation_bps(env: Env) -> Option<u32> {
        admin::get_oracle_max_deviation_bps(env)
    }
    pub fn arm_oracle_deviation_override(env: Env) -> Result<(), ContractError> {
        admin::arm_oracle_deviation_override(env)
    }
    pub fn set_oracle_min_confidence_bps(
        env: Env,
        min_bps: Option<u32>,
    ) -> Result<(), ContractError> {
        admin::set_oracle_min_confidence_bps(env, min_bps)
    }
    pub fn set_oracle_strict_mode(env: Env, enabled: bool) -> Result<(), ContractError> {
        admin::set_oracle_strict_mode(env, enabled)
    }
    pub fn get_oracle_min_confidence_bps(env: Env) -> Option<u32> {
        admin::get_oracle_min_confidence_bps(env)
    }
    pub fn get_oracle_strict_mode(env: Env) -> bool {
        admin::get_oracle_strict_mode(env)
    }
    pub fn update_oracle_heartbeat(env: Env, status: u32) -> Result<(), ContractError> {
        admin::update_oracle_heartbeat(env, status)
    }
    pub fn get_oracle_heartbeat(env: Env) -> Option<OracleHeartbeatRecord> {
        admin::get_oracle_heartbeat(env)
    }
    pub fn is_oracle_live(env: Env) -> bool {
        admin::is_oracle_live(env)
    }
    pub fn set_oracle_stale_threshold(env: Env, seconds: u64) -> Result<(), ContractError> {
        admin::set_oracle_stale_threshold(env, seconds)
    }
    pub fn get_protocol_health(env: Env) -> ProtocolHealthStatus {
        admin::get_protocol_health(env)
    }
    pub fn get_oracle_stale_threshold(env: Env) -> u64 {
        admin::get_oracle_stale_threshold(env)
    }

    pub fn get_protocol_status(env: Env) -> ProtocolStatus {
        if Self::is_paused(env.clone()) {
            ProtocolStatus::Paused
        } else if env.storage().persistent().has(&DataKey::ActiveRound) {
            ProtocolStatus::Active
        } else {
            ProtocolStatus::ClaimsOnly
        }
    }

    pub fn get_round_status(env: Env, round_id: u64) -> RoundStatus {
        if let Some(active_round) = env
            .storage()
            .persistent()
            .get::<_, Round>(&DataKey::ActiveRound)
        {
            if active_round.round_id == round_id {
                return match common::_derive_round_phase(env.ledger().sequence(), &active_round) {
                    RoundPhase::Betting => RoundStatus::Betting,
                    RoundPhase::Running => RoundStatus::Running,
                    RoundPhase::Resolvable => RoundStatus::AwaitingResolve,
                };
            }
        }

        if let Some(archive) = env
            .storage()
            .persistent()
            .get::<_, ArchivedRoundSummary>(&DataKey::ArchivedRound(round_id))
        {
            return match archive.status {
                RoundArchiveStatus::Resolved => RoundStatus::Resolved,
                RoundArchiveStatus::Cancelled => RoundStatus::Cancelled,
                RoundArchiveStatus::FallbackRefund => RoundStatus::FallbackRefund,
            };
        }

        if Self::is_round_cancelled(env, round_id) {
            RoundStatus::Cancelled
        } else {
            RoundStatus::Unknown
        }
    }

    pub fn propose_oracle_rotation(
        env: Env,
        new_oracle: Address,
        expires_in_seconds: u64,
    ) -> Result<(), ContractError> {
        admin::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        admin::_ensure_not_paused(&env)?;

        if expires_in_seconds < MIN_ROTATION_EXPIRY_SECONDS {
            return Err(ContractError::InvalidStaleThreshold);
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
        let key = DataKey::OracleRotationProposal;
        env.storage().persistent().set(&key, &proposal);
        common::_extend_persistent_ttl(&env, &key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("oracle"), symbol_short!("propose")),
            (new_oracle, expires_at),
        );
        Ok(())
    }

    pub fn accept_oracle_rotation(env: Env) -> Result<(), ContractError> {
        admin::_require_supported_schema(&env)?;
        admin::_ensure_not_paused(&env)?;

        let key = DataKey::OracleRotationProposal;
        let proposal: OracleRotationProposal = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::NoPendingRotation)?;
        if env.ledger().timestamp() > proposal.expires_at {
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
            return Err(ContractError::RotationExpired);
        }

        let oracle_key = DataKey::Oracle;
        let previous: Address = env
            .storage()
            .persistent()
            .get(&oracle_key)
            .ok_or(ContractError::OracleNotSet)?;
        env.storage()
            .persistent()
            .set(&oracle_key, &proposal.new_oracle);
        common::_extend_persistent_ttl(&env, &oracle_key);
        env.storage().persistent().remove(&key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("oracle"), symbol_short!("accept")),
            (previous, proposal.new_oracle),
        );
        Ok(())
    }

    pub fn cancel_oracle_rotation(env: Env) -> Result<(), ContractError> {
        admin::_require_supported_schema(&env)?;
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        admin.require_auth();
        admin::_ensure_not_paused(&env)?;

        let key = DataKey::OracleRotationProposal;
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

    pub fn get_oracle_rotation_proposal(env: Env) -> Option<OracleRotationProposal> {
        let key = DataKey::OracleRotationProposal;
        common::_extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    }

    pub fn create_round(
        env: Env,
        start_price: u128,
        mode: Option<u32>,
    ) -> Result<(), ContractError> {
        betting::create_round(env, start_price, mode)
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
    pub fn mint_initial(env: Env, user: Address) -> i128 {
        betting::mint_initial(env, user)
    }
    pub fn balance(env: Env, user: Address) -> i128 {
        common::balance(env, user)
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
    pub fn resolve_round(env: Env, payload: OraclePayload) -> Result<(), ContractError> {
        settlement::resolve_round(env, payload)
    }

    pub fn get_active_round(env: Env) -> Option<Round> {
        queries::get_active_round(env)
    }
    pub fn get_round_pool_stats(env: Env) -> Option<RoundPoolStats> {
        queries::get_round_pool_stats(env)
    }
    pub fn get_round_phase(env: Env) -> Result<RoundPhase, ContractError> {
        queries::get_round_phase(env)
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

    pub fn set_windows(env: Env, bet_ledgers: u32, run_ledgers: u32) -> Result<(), ContractError> {
        config::set_windows(env, bet_ledgers, run_ledgers)
    }
    pub fn set_max_stake(env: Env, max_amount: Option<i128>) -> Result<(), ContractError> {
        config::set_max_stake(env, max_amount)
    }
    pub fn get_max_stake(env: Env) -> Option<i128> {
        config::get_max_stake(env)
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
    pub fn set_mint_limit(env: Env, limit: u32) -> Result<(), ContractError> {
        config::set_mint_limit(env, limit)
    }
    pub fn get_mint_limit(env: Env) -> u32 {
        config::get_mint_limit(env)
    }
    pub fn set_archive_retention(env: Env, limit: u32) -> Result<(), ContractError> {
        config::set_archive_retention(env, limit)
    }
    pub fn get_archive_retention(env: Env) -> u32 {
        config::get_archive_retention(env)
    }
}
