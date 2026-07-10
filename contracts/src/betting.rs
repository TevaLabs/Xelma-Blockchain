// SPDX-License-Identifier: MIT
use crate::admin::{_ensure_normal_mode, _ensure_not_paused, _require_supported_schema};
use crate::common::{
    _emit_action_rejected, _extend_persistent_ttl, _set_balance, assert_no_active_round, balance,
    DEFAULT_BET_WINDOW_LEDGERS, DEFAULT_RUN_WINDOW_LEDGERS, MAX_START_PRICE, MIN_START_PRICE,
};
use crate::config::get_max_precision_participants;
use crate::errors::ContractError;
use crate::types::{
    BetSide, DataKey, PrecisionCommitment, PrecisionPrediction, Round, RoundMode, UserPosition,
};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, Vec};

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
        .get(&DataKey::Admin)
        .ok_or(ContractError::AdminNotSet)?;

    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("create"), e);
    })?;
    assert_no_active_round(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("create"), e);
    })?;

    // Get configured windows (with defaults)
    _extend_persistent_ttl(&env, &DataKey::BetWindowLedgers);
    let bet_ledgers: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::BetWindowLedgers)
        .unwrap_or(DEFAULT_BET_WINDOW_LEDGERS);
    _extend_persistent_ttl(&env, &DataKey::RunWindowLedgers);
    let run_ledgers: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::RunWindowLedgers)
        .unwrap_or(DEFAULT_RUN_WINDOW_LEDGERS);

    // Generate unique round ID
    _extend_persistent_ttl(&env, &DataKey::LastRoundId);
    let last_round_id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::LastRoundId)
        .unwrap_or(0);
    let round_id = last_round_id
        .checked_add(1)
        .ok_or(ContractError::Overflow)?;
    env.storage()
        .persistent()
        .set(&DataKey::LastRoundId, &round_id);
    _extend_persistent_ttl(&env, &DataKey::LastRoundId);

    let start_ledger = env.ledger().sequence();
    let bet_end_ledger = start_ledger
        .checked_add(bet_ledgers)
        .ok_or(ContractError::Overflow)?;
    let end_ledger = start_ledger
        .checked_add(run_ledgers)
        .ok_or(ContractError::Overflow)?;

    let round = Round {
        round_id,
        price_start: start_price,
        start_ledger,
        bet_end_ledger,
        end_ledger,
        pool_up: 0,
        pool_down: 0,
        mode: round_mode.clone(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::ActiveRound, &round);
    _extend_persistent_ttl(&env, &DataKey::ActiveRound);

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

pub fn place_bet(
    env: Env,
    user: Address,
    amount: i128,
    side: BetSide,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_normal_mode(&env)?;

    if amount <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }

    // Enforce max stake cap
    if let Some(max_stake) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKey::MaxStake)
    {
        if amount > max_stake {
            return Err(ContractError::StakeExceedsMax);
        }
    }

    // Single read of the active round
    let mut round: Round = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    // Enforce per-user round exposure cap
    if let Some(max_exposure) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKey::MaxUserRoundExposure)
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
        .get::<_, u32>(&DataKey::CloseBufferLedgers)
        .unwrap_or(0);
    let close_ledger = round.bet_end_ledger.saturating_sub(close_buffer_ledgers);
    if current_ledger >= round.bet_end_ledger || current_ledger >= close_ledger {
        return Err(ContractError::RoundEnded);
    }

    let user_balance = balance(env.clone(), user.clone());
    if user_balance < amount {
        return Err(ContractError::InsufficientBalance);
    }

    // O(1) duplicate-bet check
    let pos_key = DataKey::Position(round.round_id, user.clone());
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
    let participants_key = DataKey::RoundParticipants(round.round_id);
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
        .set(&DataKey::ActiveRound, &round);

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

    if amount <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }

    // Enforce min bet floor (Issue #161)
    if let Some(min_bet) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKey::MinBet)
    {
        if amount < min_bet {
            return Err(ContractError::InvalidBetAmount);
        }
    }

    // Enforce max stake cap
    if let Some(max_stake) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKey::MaxStake)
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
        .get(&DataKey::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    // Enforce per-user round exposure cap
    if let Some(max_exposure) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKey::MaxUserRoundExposure)
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
        .get::<_, u32>(&DataKey::CloseBufferLedgers)
        .unwrap_or(0);
    let close_ledger = round.bet_end_ledger.saturating_sub(close_buffer_ledgers);
    if current_ledger >= round.bet_end_ledger || current_ledger >= close_ledger {
        return Err(ContractError::RoundEnded);
    }

    let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
    let commit_key = DataKey::PrecisionCommitment(round.round_id, user.clone());
    if env.storage().persistent().has(&pred_key) || env.storage().persistent().has(&commit_key) {
        return Err(ContractError::AlreadyBet);
    }

    let participants_key = DataKey::RoundParticipants(round.round_id);
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

    if amount <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }

    // Enforce min bet floor (Issue #161)
    if let Some(min_bet) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKey::MinBet)
    {
        if amount < min_bet {
            return Err(ContractError::InvalidBetAmount);
        }
    }

    // Enforce max stake cap
    if let Some(max_stake) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKey::MaxStake)
    {
        if amount > max_stake {
            return Err(ContractError::StakeExceedsMax);
        }
    }

    // Single read of the active round
    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveRound)
        .ok_or(ContractError::NoActiveRound)?;

    // Enforce per-user round exposure cap
    if let Some(max_exposure) = env
        .storage()
        .persistent()
        .get::<_, i128>(&DataKey::MaxUserRoundExposure)
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
        .get::<_, u32>(&DataKey::CloseBufferLedgers)
        .unwrap_or(0);
    let close_ledger = round.bet_end_ledger.saturating_sub(close_buffer_ledgers);
    if current_ledger >= round.bet_end_ledger || current_ledger >= close_ledger {
        return Err(ContractError::RoundEnded);
    }

    let user_balance = balance(env.clone(), user.clone());
    if user_balance < amount {
        return Err(ContractError::InsufficientBalance);
    }

    // Check duplicate bet or commitment
    let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
    let commit_key = DataKey::PrecisionCommitment(round.round_id, user.clone());
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
    let participants_key = DataKey::RoundParticipants(round.round_id);
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

    // Single read of the active round
    let round: Round = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveRound)
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
    let commit_key = DataKey::PrecisionCommitment(round.round_id, user.clone());
    let mut commitment: PrecisionCommitment = env
        .storage()
        .persistent()
        .get(&commit_key)
        .ok_or(ContractError::CommitmentNotFound)?;

    if commitment.revealed {
        return Err(ContractError::AlreadyRevealed);
    }

    // Verify hash
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
    let pred_key = DataKey::PrecisionPosition(round.round_id, user.clone());
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

/// Mints 1000 vXLM for new users (one-time only)
pub fn mint_initial(env: Env, user: Address) -> i128 {
    user.require_auth();
    if let Err(e) = _require_supported_schema(&env) {
        soroban_sdk::panic_with_error!(&env, e);
    }
    if let Err(e) = _ensure_normal_mode(&env) {
        soroban_sdk::panic_with_error!(&env, e);
    }

    let key = DataKey::Balance(user.clone());

    if let Some(existing_balance) = env.storage().persistent().get(&key) {
        _extend_persistent_ttl(&env, &key);
        return existing_balance;
    }

    let sequence = env.ledger().sequence();
    if let Some(limit) = env
        .storage()
        .instance()
        .get::<_, u32>(&DataKey::MintLimitConfig)
    {
        if limit > 0 {
            let counter_key = DataKey::LedgerMintCounter(sequence);
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
    env.storage().persistent().set(&key, &initial_amount);
    _extend_persistent_ttl(&env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("mint"), symbol_short!("initial")),
        (user, initial_amount),
    );

    initial_amount
}
