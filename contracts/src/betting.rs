// SPDX-License-Identifier: MIT
use crate::access_control::_enforce_access_control;
use crate::admin::{_ensure_normal_mode, _ensure_not_paused, _require_supported_schema};
use crate::common::{
    _accumulate_pending, _current_epoch_id, _emit_action_rejected, _enforce_min_bet,
    _extend_persistent_ttl, _set_balance, assert_no_active_round, balance, BPS_DENOMINATOR,
    DEFAULT_BET_WINDOW_LEDGERS, DEFAULT_RUN_WINDOW_LEDGERS, MAX_START_PRICE, MIN_START_PRICE,
};
use crate::config::{
    _collect_protocol_fee, _read_fee_model, get_early_cashout_bps, get_max_precision_participants,
};
use crate::errors::ContractError;
use crate::settlement::_persist_user_outcome;
use crate::types::{
    BetSide, DataKeyCore, DataKeyScoped, PrecisionCommitment, PrecisionPrediction, Round,
    RoundMode, RoundTemplate, UserOutcomeType, UserPosition,
};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, Symbol, Vec};

/// Commitment preimage format (Precision commit-reveal):
///
/// ```text
/// commitment = sha256( predicted_price.to_xdr() || salt.to_xdr() )
/// ```
///
/// - `predicted_price` is a `u128` (4-decimal price scale).
/// - `salt` is a 32-byte value with minimum on-chain entropy (see
///   [`salt_has_minimum_entropy`]); wallets MUST still sample it from a CSPRNG.
/// - The commitment hash stored at commit time MUST be exactly 32 bytes and
///   MUST NOT be the all-zero placeholder (`InvalidCommitment`).
///
/// Unrevealed commitments are settled by the precision resolver: forfeited to
/// the pot when at least one prediction is revealed, or refunded when nobody
/// reveals (see `_resolve_precision_mode`).
fn is_zero_bytes32(env: &Env, value: &BytesN<32>) -> bool {
    *value == BytesN::from_array(env, &[0u8; 32])
}

/// Rejects clearly weak salts: all-zero or every byte identical.
/// Stronger entropy (CSPRNG) is a client responsibility; this only blocks
/// trivial placeholders that invite grinding / griefing.
fn salt_has_minimum_entropy(salt: &BytesN<32>) -> bool {
    let bytes = salt.to_array();
    let first = bytes[0];
    let mut saw_nonzero = false;
    let mut saw_different = false;
    for b in bytes.iter() {
        if *b != 0 {
            saw_nonzero = true;
        }
        if *b != first {
            saw_different = true;
        }
    }
    saw_nonzero && saw_different
}

/// Creates a new prediction round (admin only)
pub fn create_round(env: Env, start_price: u128, mode: Option<u32>) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    if start_price < MIN_START_PRICE {
        return Err(ContractError::InvalidStartPrice);
    }
    if start_price > MAX_START_PRICE {
        return Err(ContractError::InvalidStartPrice);
    }

    // Default to Up/Down mode (0) if not specified
    let mode_value = mode.unwrap_or(0);

    // Validate mode is either 0 or 1
    if mode_value > 1 {
        return Err(ContractError::InvalidMode);
    }

    let round_mode = if mode_value == 0 {
        RoundMode::UpDown
    } else {
        RoundMode::Precision
    };

    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;

    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("create"), e);
    })?;
    assert_no_active_round(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("create"), e);
    })?;

    // Get configured windows (with defaults)
    _extend_persistent_ttl(&env, &DataKeyCore::BetWindowLedgers);
    let bet_ledgers: u32 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::BetWindowLedgers)
        .unwrap_or(DEFAULT_BET_WINDOW_LEDGERS);
    _extend_persistent_ttl(&env, &DataKeyCore::RunWindowLedgers);
    let run_ledgers: u32 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::RunWindowLedgers)
        .unwrap_or(DEFAULT_RUN_WINDOW_LEDGERS);

    // Generate unique round ID
    _extend_persistent_ttl(&env, &DataKeyCore::LastRoundId);
    let last_round_id: u64 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::LastRoundId)
        .unwrap_or(0);
    let round_id = last_round_id
        .checked_add(1)
        .ok_or(ContractError::Overflow)?;
    env.storage()
        .persistent()
        .set(&DataKeyCore::LastRoundId, &round_id);
    _extend_persistent_ttl(&env, &DataKeyCore::LastRoundId);

    let start_ledger = env.ledger().sequence();
    let bet_end_ledger = start_ledger
        .checked_add(bet_ledgers)
        .ok_or(ContractError::Overflow)?;
    let end_ledger = start_ledger
        .checked_add(run_ledgers)
        .ok_or(ContractError::Overflow)?;

    let start_timestamp = env.ledger().timestamp();

    let round = Round {
        round_id,
        price_start: start_price,
        start_ledger,
        bet_end_ledger,
        end_ledger,
        pool_up: 0,
        pool_down: 0,
        mode: round_mode.clone(),
        start_timestamp,
    };

    env.storage()
        .persistent()
        .set(&DataKeyCore::ActiveRound, &round);
    _extend_persistent_ttl(&env, &DataKeyCore::ActiveRound);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("round"), symbol_short!("created")),
        (
            round_id,
            start_price,
            start_ledger,
            bet_end_ledger,
            end_ledger,
            mode_value,
        ),
    );

    Ok(())
}

/// Creates the next round from the admin-configured template (admin only).
///
/// This is the "keeper" entry point for always-on demos: rather than an
/// operator re-supplying `start_price` / `mode` after every settle or
/// cancel, a keeper (holding the admin key) calls this once a round has
/// left the active state. Round creation is delegated to [`create_round`]
/// itself, so the single-active-round guard (`RoundAlreadyActive`) is
/// inherited verbatim — overlap is structurally impossible, not just
/// checked twice in two places that could drift apart.
pub fn create_next_from_template(env: Env) -> Result<u64, ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("nxttmpl"), e);
    })?;

    let template: RoundTemplate = env
        .storage()
        .persistent()
        .get(&DataKeyCore::RoundTemplate)
        .ok_or(ContractError::NoRoundTemplate)
        .inspect_err(|&e| {
            _emit_action_rejected(&env, &admin, symbol_short!("nxttmpl"), e);
        })?;

    create_round(env.clone(), template.start_price, template.mode)?;

    let round_id: u64 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::LastRoundId)
        .unwrap_or(0);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("template"), symbol_short!("applied")),
        (round_id, template.start_price, template.mode.unwrap_or(0)),
    );

    Ok(round_id)
}

pub fn place_bet(
    env: Env,
    user: Address,
    amount: i128,
    side: BetSide,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_normal_mode(&env)?;
    _enforce_access_control(&env, &user)?;

    if amount <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }
    _enforce_min_bet(&env, amount)?;

    // Enforce max stake cap
    if let Some(max_stake) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKeyCore::MaxStake)
    {
        if amount > max_stake {
            return Err(ContractError::StakeExceedsMax);
        }
    }

    // Single read of the active round
    let mut round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    // Enforce per-user round exposure cap
    if let Some(max_exposure) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKeyCore::MaxUserRoundExposure)
    {
        if amount > max_exposure {
            return Err(ContractError::ExposureCapExceeded);
        }
    }

    // Verify round is in Up/Down mode
    if round.mode != RoundMode::UpDown {
        return Err(ContractError::WrongModeForPrediction);
    }

    let current_ledger = env.ledger().sequence();
    let close_buffer_ledgers = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKeyCore::CloseBufferLedgers)
        .unwrap_or(0);
    let close_ledger = round.bet_end_ledger.saturating_sub(close_buffer_ledgers);
    if current_ledger >= round.bet_end_ledger {
        return Err(ContractError::RoundEnded);
    }
    if close_buffer_ledgers > 0 && current_ledger >= close_ledger {
        return Err(ContractError::RoundEnded);
    }

    let user_balance = balance(env.clone(), user.clone());
    if user_balance < amount {
        return Err(ContractError::InsufficientBalance);
    }

    // O(1) duplicate-bet check
    let pos_key = DataKeyScoped::Position(round.round_id, user.clone());
    if env.storage().persistent().has(&pos_key) {
        return Err(ContractError::AlreadyBet);
    }

    // Deduct balance
    let new_balance = user_balance
        .checked_sub(amount)
        .ok_or(ContractError::Overflow)?;
    _set_balance(&env, user.clone(), new_balance);

    // Write single-user position key
    let position = UserPosition {
        amount,
        side: side.clone(),
    };
    env.storage().persistent().set(&pos_key, &position);

    // Append to participant list
    let participants_key = DataKeyScoped::RoundParticipants(round.round_id);
    let mut participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&participants_key)
        .unwrap_or(Vec::new(&env));
    participants.push_back(user.clone());
    env.storage()
        .persistent()
        .set(&participants_key, &participants);

    // Update cached round pools and write once
    match side {
        BetSide::Up => {
            round.pool_up = round
                .pool_up
                .checked_add(amount)
                .ok_or(ContractError::Overflow)?;
        }
        BetSide::Down => {
            round.pool_down = round
                .pool_down
                .checked_add(amount)
                .ok_or(ContractError::Overflow)?;
        }
    }
    env.storage()
        .persistent()
        .set(&DataKeyCore::ActiveRound, &round);

    let side_value: u32 = match side {
        BetSide::Up => 0,
        BetSide::Down => 1,
    };
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("bet"), symbol_short!("placed")),
        (user, round.round_id, amount, side_value),
    );

    Ok(())
}

pub fn place_precision_prediction(
    env: Env,
    user: Address,
    amount: i128,
    predicted_price: u128,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_normal_mode(&env)?;
    _enforce_access_control(&env, &user)?;

    if amount <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }
    _enforce_min_bet(&env, amount)?;

    // Enforce max stake cap
    if let Some(max_stake) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKeyCore::MaxStake)
    {
        if amount > max_stake {
            return Err(ContractError::StakeExceedsMax);
        }
    }

    if predicted_price > 99_999_999 {
        return Err(ContractError::InvalidPrice);
    }

    // Single read of the active round
    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    // Enforce per-user round exposure cap
    if let Some(max_exposure) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKeyCore::MaxUserRoundExposure)
    {
        if amount > max_exposure {
            return Err(ContractError::ExposureCapExceeded);
        }
    }

    // Verify round is in Precision mode
    if round.mode != RoundMode::Precision {
        return Err(ContractError::WrongModeForPrediction);
    }

    let current_ledger = env.ledger().sequence();
    let close_buffer_ledgers = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKeyCore::CloseBufferLedgers)
        .unwrap_or(0);
    let close_ledger = round.bet_end_ledger.saturating_sub(close_buffer_ledgers);
    if current_ledger >= round.bet_end_ledger {
        return Err(ContractError::RoundEnded);
    }
    if close_buffer_ledgers > 0 && current_ledger >= close_ledger {
        return Err(ContractError::RoundEnded);
    }

    let pred_key = DataKeyScoped::PrecisionPosition(round.round_id, user.clone());
    let commit_key = DataKeyScoped::PrecisionCommitment(round.round_id, user.clone());
    if env.storage().persistent().has(&pred_key) || env.storage().persistent().has(&commit_key) {
        return Err(ContractError::AlreadyBet);
    }

    let participants_key = DataKeyScoped::RoundParticipants(round.round_id);
    let mut participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&participants_key)
        .unwrap_or(Vec::new(&env));
    let max_precision_participants = get_max_precision_participants(env.clone());
    if participants.len() >= max_precision_participants {
        return Err(ContractError::PrecisionCapExceeded);
    }

    let user_balance = balance(env.clone(), user.clone());
    if user_balance < amount {
        return Err(ContractError::InsufficientBalance);
    }

    // Deduct balance
    let new_balance = user_balance
        .checked_sub(amount)
        .ok_or(ContractError::Overflow)?;
    _set_balance(&env, user.clone(), new_balance);

    // Write single-user prediction key
    let prediction = PrecisionPrediction {
        user: user.clone(),
        predicted_price,
        amount,
    };
    env.storage().persistent().set(&pred_key, &prediction);

    // Append to shared participant list
    participants.push_back(user.clone());
    env.storage()
        .persistent()
        .set(&participants_key, &participants);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("predict"), symbol_short!("price")),
        (user, round.round_id, predicted_price, amount),
    );

    Ok(())
}

pub fn predict_price(
    env: Env,
    user: Address,
    guessed_price: u128,
    amount: i128,
) -> Result<(), ContractError> {
    place_precision_prediction(env, user, amount, guessed_price)
}

pub fn commit_prediction(
    env: Env,
    user: Address,
    hash: BytesN<32>,
    amount: i128,
) -> Result<(), ContractError> {
    user.require_auth();
    _ensure_normal_mode(&env)?;
    _enforce_access_control(&env, &user)?;

    // Reject clearly invalid commitment placeholders early (before balance
    // reads / deductions) so griefing commits cannot lock liquidity.
    if is_zero_bytes32(&env, &hash) {
        return Err(ContractError::InvalidPrice);
    }

    if amount <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }
    _enforce_min_bet(&env, amount)?;

    // Enforce max stake cap
    if let Some(max_stake) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKeyCore::MaxStake)
    {
        if amount > max_stake {
            return Err(ContractError::StakeExceedsMax);
        }
    }

    // Single read of the active round
    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    // Enforce per-user round exposure cap
    if let Some(max_exposure) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKeyCore::MaxUserRoundExposure)
    {
        if amount > max_exposure {
            return Err(ContractError::ExposureCapExceeded);
        }
    }

    // Verify round is in Precision mode
    if round.mode != RoundMode::Precision {
        return Err(ContractError::WrongModeForPrediction);
    }

    let current_ledger = env.ledger().sequence();
    let close_buffer_ledgers = env
        .storage()
        .persistent()
        .get::<_, u32>(&DataKeyCore::CloseBufferLedgers)
        .unwrap_or(0);
    let close_ledger = round.bet_end_ledger.saturating_sub(close_buffer_ledgers);
    if current_ledger >= round.bet_end_ledger {
        return Err(ContractError::RoundEnded);
    }
    if close_buffer_ledgers > 0 && current_ledger >= close_ledger {
        return Err(ContractError::RoundEnded);
    }

    let user_balance = balance(env.clone(), user.clone());
    if user_balance < amount {
        return Err(ContractError::InsufficientBalance);
    }

    // Check duplicate bet or commitment
    let pred_key = DataKeyScoped::PrecisionPosition(round.round_id, user.clone());
    let commit_key = DataKeyScoped::PrecisionCommitment(round.round_id, user.clone());
    if env.storage().persistent().has(&pred_key) || env.storage().persistent().has(&commit_key) {
        return Err(ContractError::AlreadyBet);
    }

    // Deduct balance
    let new_balance = user_balance
        .checked_sub(amount)
        .ok_or(ContractError::Overflow)?;
    _set_balance(&env, user.clone(), new_balance);

    // Store commitment
    let commitment = PrecisionCommitment {
        hash: hash.clone(),
        amount,
        revealed: false,
    };
    env.storage().persistent().set(&commit_key, &commitment);

    // Append to shared participant list
    let participants_key = DataKeyScoped::RoundParticipants(round.round_id);
    let mut participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&participants_key)
        .unwrap_or(Vec::new(&env));
    participants.push_back(user.clone());
    env.storage()
        .persistent()
        .set(&participants_key, &participants);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("commit"), symbol_short!("predict")),
        (user, round.round_id, hash, amount),
    );

    Ok(())
}

pub fn reveal_prediction(
    env: Env,
    user: Address,
    predicted_price: u128,
    salt: BytesN<32>,
) -> Result<(), ContractError> {
    user.require_auth();
    _ensure_normal_mode(&env)?;
    _enforce_access_control(&env, &user)?;

    // Enforce salt entropy before any storage reads so malformed reveals fail
    // fast with an explicit error (not HashMismatch after a wasted lookup).
    if !salt_has_minimum_entropy(&salt) {
        return Err(ContractError::InvalidPrice);
    }

    // Single read of the active round
    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    // Verify round is in Precision mode
    if round.mode != RoundMode::Precision {
        return Err(ContractError::WrongModeForPrediction);
    }

    // Enforce reveal window
    let current_ledger = env.ledger().sequence();
    if current_ledger < round.bet_end_ledger || current_ledger >= round.end_ledger {
        return Err(ContractError::InvalidRevealWindow);
    }

    // Retrieve commitment
    let commit_key = DataKeyScoped::PrecisionCommitment(round.round_id, user.clone());
    let mut commitment: PrecisionCommitment = env
        .storage()
        .persistent()
        .get(&commit_key)
        .ok_or(ContractError::CommitmentNotFound)?;

    if commitment.revealed {
        return Err(ContractError::AlreadyRevealed);
    }

    // Verify hash: sha256(predicted_price.to_xdr() || salt.to_xdr())
    let mut preimage = Bytes::new(&env);
    preimage.append(&predicted_price.to_xdr(&env));
    preimage.append(&salt.to_xdr(&env));
    let computed_hash = env.crypto().sha256(&preimage);
    let computed_hash_bytes: BytesN<32> = computed_hash.into();

    if computed_hash_bytes != commitment.hash {
        return Err(ContractError::HashMismatch);
    }

    // Mark revealed and write
    commitment.revealed = true;
    env.storage().persistent().set(&commit_key, &commitment);

    // Store prediction for resolution
    let pred_key = DataKeyScoped::PrecisionPosition(round.round_id, user.clone());
    let prediction = PrecisionPrediction {
        user: user.clone(),
        predicted_price,
        amount: commitment.amount,
    };
    env.storage().persistent().set(&pred_key, &prediction);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("reveal"), symbol_short!("predict")),
        (user, round.round_id, predicted_price, commitment.amount),
    );

    Ok(())
}

/// Early cash-out during the Running phase for UpDown rounds.
///
/// **Design**: Default-off. The admin must configure `EarlyCashoutBps` to a
/// non-`None` penalty rate before this entrypoint is usable. During the
/// Running phase (after betting closes but before the round ends), a bettor
/// can exit their position early, forfeiting a percentage of their stake to
/// the protocol treasury. The full original stake is deducted from the pool
/// so conservation holds exactly at resolution.
///
/// **Formula**:
/// ```text
/// forfeit = stake * penalty_bps / 10_000
/// cashout = stake - forfeit
/// ```
///
/// **Conservation invariants**:
/// - The full stake is removed from `pool_up` or `pool_down` so the pool
///   strictly matches the sum of remaining active positions.
/// - The forfeited amount is credited to the protocol fee treasury.
/// - The user receives `cashout` as pending winnings.
///
/// **Restrictions**:
/// - Only during the Running phase (`bet_end_ledger ≤ ledger < end_ledger`).
/// - Only for UpDown rounds (not Precision).
/// - User must have an active position in the current round.
/// - Feature must be enabled via `EarlyCashoutBps`.
pub fn cash_out_early(env: Env, user: Address) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_normal_mode(&env)?;
    _enforce_access_control(&env, &user)?;

    // Check early cash-out is enabled
    let penalty_bps = get_early_cashout_bps(env.clone())
        .ok_or(ContractError::RoundEnded)?;

    // Single read of the active round
    let mut round: Round = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    // Only UpDown rounds supported
    if round.mode != RoundMode::UpDown {
        return Err(ContractError::RoundEnded);
    }

    // Must be in Running phase (betting closed, round not yet ended)
    let current_ledger = env.ledger().sequence();
    if current_ledger < round.bet_end_ledger || current_ledger >= round.end_ledger {
        return Err(ContractError::RoundEnded);
    }

    // Retrieve user's position
    let pos_key = DataKeyScoped::Position(round.round_id, user.clone());
    let position: UserPosition = env
        .storage()
        .persistent()
        .get(&pos_key)
        .ok_or(ContractError::CommitmentNotFound)?;

    let stake = position.amount;
    if stake <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }

    // Calculate forfeited amount and cashout
    let penalty_bps_i128 = penalty_bps as i128;
    let forfeit = stake
        .checked_mul(penalty_bps_i128)
        .ok_or(ContractError::Overflow)?
        / BPS_DENOMINATOR;

    // If forfeit rounds down to zero (very small stake relative to penalty),
    // user gets full refund — still remove position from pool.
    let cashout = stake
        .checked_sub(forfeit)
        .ok_or(ContractError::Overflow)?;

    // Deduct full stake from the appropriate pool
    match position.side {
        BetSide::Up => {
            round.pool_up = round
                .pool_up
                .checked_sub(stake)
                .ok_or(ContractError::Overflow)?;
        }
        BetSide::Down => {
            round.pool_down = round
                .pool_down
                .checked_sub(stake)
                .ok_or(ContractError::Overflow)?;
        }
    }
    env.storage()
        .persistent()
        .set(&DataKeyCore::ActiveRound, &round);

    // Credit cashout to user's pending winnings
    if cashout > 0 {
        _accumulate_pending(&env, user.clone(), cashout)?;
    }

    // Collect forfeited amount as protocol fee
    if forfeit > 0 {
        let fee_model = _read_fee_model(&env);
        _collect_protocol_fee(&env, round.round_id, forfeit, Some(penalty_bps), fee_model)?;
    }

    // Persist user outcome record for the early exit
    let prediction_side = match position.side {
        BetSide::Up => 0,
        BetSide::Down => 1,
    };
    _persist_user_outcome(
        &env,
        round.round_id,
        0,
        &user,
        prediction_side,
        0,
        stake,
        cashout,
        UserOutcomeType::Refund,
    );

    // Remove user's position
    env.storage().persistent().remove(&pos_key);

    // Remove user from participant list
    let participants_key = DataKeyScoped::RoundParticipants(round.round_id);
    let participants: Vec<Address> = env
        .storage()
        .persistent()
        .get(&participants_key)
        .unwrap_or(Vec::new(&env));
    let mut new_participants: Vec<Address> = Vec::new(&env);
    for i in 0..participants.len() {
        if let Some(addr) = participants.get(i) {
            if addr != user {
                new_participants.push_back(addr);
            }
        }
    }
    env.storage()
        .persistent()
        .set(&participants_key, &new_participants);

    // Emit event
    let side_value: u32 = match position.side {
        BetSide::Up => 0,
        BetSide::Down => 1,
    };
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("cashout"), symbol_short!("early")),
        (user, round.round_id, side_value, stake, cashout, forfeit),
    );

    Ok(())
}

/// Mints 1000 vXLM for new users (one-time only)
pub fn mint_initial(env: Env, user: Address) -> i128 {
    user.require_auth();
    if let Err(e) = _require_supported_schema(&env) {
        soroban_sdk::panic_with_error!(&env, e);
    }
    if let Err(e) = _ensure_normal_mode(&env) {
        soroban_sdk::panic_with_error!(&env, e);
    }
    if let Err(e) = _enforce_access_control(&env, &user) {
        soroban_sdk::panic_with_error!(&env, e);
    }

    let key = DataKeyScoped::Balance(user.clone());

    if let Some(existing_balance) = env.storage().persistent().get(&key) {
        _extend_persistent_ttl(&env, &key);
        return existing_balance;
    }

    let sequence = env.ledger().sequence();
    if let Some(limit) = env
        .storage()
        .instance()
        .get::<_, u32>(&DataKeyCore::MintLimitConfig)
    {
        if limit > 0 {
            let counter_key = DataKeyScoped::LedgerMintCounter(sequence);
            let current_count = env
                .storage()
                .temporary()
                .get::<_, u32>(&counter_key)
                .unwrap_or(0);
            if current_count >= limit {
                soroban_sdk::panic_with_error!(&env, ContractError::MintLimitExceeded);
            }
            env.storage()
                .temporary()
                .set(&counter_key, &(current_count + 1));
        }
    }

    let initial_amount: i128 = 1000_0000000;

    // ─── Epoch budget check ──────────────────────────────────────────────
    const EP_BUDGET_KEY: Symbol = symbol_short!("EpMintBgt");
    let epoch_budget: i128 = env
        .storage()
        .instance()
        .get(&EP_BUDGET_KEY)
        .unwrap_or(0);
    if epoch_budget > 0 {
        let current_epoch = _current_epoch_id(&env);
        const EP_CONSUMED_KEY: Symbol = symbol_short!("EpMintCsm");
        const EP_EPOCH_KEY: Symbol = symbol_short!("EpMintEpc");
        let stored_epoch: u32 = env
            .storage()
            .temporary()
            .get(&EP_EPOCH_KEY)
            .unwrap_or(0);
        let consumed: i128 = if stored_epoch == current_epoch {
            env.storage()
                .temporary()
                .get(&EP_CONSUMED_KEY)
                .unwrap_or(0)
        } else {
            0
        };
        let new_consumed = consumed.checked_add(initial_amount);
        match new_consumed {
            Some(val) if val <= epoch_budget => {
                env.storage()
                    .temporary()
                    .set(&EP_CONSUMED_KEY, &val);
                if stored_epoch != current_epoch {
                    env.storage()
                        .temporary()
                        .set(&EP_EPOCH_KEY, &current_epoch);
                }
            }
            _ => {
                soroban_sdk::panic_with_error!(&env, ContractError::MintLimitExceeded);
            }
        }
    }

    env.storage().persistent().set(&key, &initial_amount);
    _extend_persistent_ttl(&env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("mint"), symbol_short!("initial")),
        (user, initial_amount),
    );

    initial_amount
}