//! Core contract implementation for the XLM Price Prediction Market.

use soroban_sdk::{contract, contractimpl, Address, Env, Map, Vec};

use crate::errors::ContractError;
use crate::types::{BetSide, DataKey, Round, UserPosition, UserStats};

#[contract]
pub struct VirtualTokenContract;

#[contractimpl]
impl VirtualTokenContract {
    /// Initializes the contract with admin and oracle addresses (one-time only)
    pub fn initialize(env: Env, admin: Address, oracle: Address) -> Result<(), ContractError> {
        admin.require_auth();
        
        if env.storage().persistent().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }
        
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::Oracle, &oracle);
        
        Ok(())
    }
    
    /// Creates a new prediction round (admin only)
    pub fn create_round(env: Env, start_price: u128, duration_ledgers: u32) -> Result<(), ContractError> {
        if start_price == 0 {
            return Err(ContractError::InvalidPrice);
        }
        
        if duration_ledgers == 0 || duration_ledgers > 100_000 {
            return Err(ContractError::InvalidDuration);
        }
        
        let admin: Address = env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::AdminNotSet)?;
        
        admin.require_auth();
        
        let current_ledger = env.ledger().sequence();
        let end_ledger = current_ledger
            .checked_add(duration_ledgers)
            .ok_or(ContractError::Overflow)?;
        
        let round = Round {
            price_start: start_price,
            end_ledger,
            pool_up: 0,
            pool_down: 0,
        };
        
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
        
        Ok(())
    }
    
    /// Returns the currently active round, if any
    pub fn get_active_round(env: Env) -> Option<Round> {
        env.storage().persistent().get(&DataKey::ActiveRound)
    }
    
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }
    
    pub fn get_oracle(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Oracle)
    }
    
    /// Returns user statistics (wins, losses, streaks)
    pub fn get_user_stats(env: Env, user: Address) -> UserStats {
        let key = DataKey::UserStats(user);
        env.storage().persistent().get(&key).unwrap_or(UserStats {
            total_wins: 0,
            total_losses: 0,
            current_streak: 0,
            best_streak: 0,
        })
    }
    
    /// Returns user's claimable winnings
    pub fn get_pending_winnings(env: Env, user: Address) -> i128 {
        let key = DataKey::PendingWinnings(user);
        env.storage().persistent().get(&key).unwrap_or(0)
    }
    
    /// Places a bet on the active round
    pub fn place_bet(env: Env, user: Address, amount: i128, side: BetSide) -> Result<(), ContractError> {
        user.require_auth();
        
        if amount <= 0 {
            return Err(ContractError::InvalidBetAmount);
        }
        
        let mut round: Round = env.storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .ok_or(ContractError::NoActiveRound)?;
        
        let current_ledger = env.ledger().sequence();
        if current_ledger >= round.end_ledger {
            return Err(ContractError::RoundEnded);
        }
        
        let user_balance = Self::balance(env.clone(), user.clone());
        if user_balance < amount {
            return Err(ContractError::InsufficientBalance);
        }
        
        let mut positions: Map<Address, UserPosition> = env.storage()
            .persistent()
            .get(&DataKey::Positions)
            .unwrap_or(Map::new(&env));
        
        if positions.contains_key(user.clone()) {
            return Err(ContractError::AlreadyBet);
        }
        
        let new_balance = user_balance
            .checked_sub(amount)
            .ok_or(ContractError::Overflow)?;
        Self::_set_balance(&env, user.clone(), new_balance);
        
        let position = UserPosition {
            amount,
            side: side.clone(),
        };
        positions.set(user, position);
        
        match side {
            BetSide::Up => {
                round.pool_up = round.pool_up
                    .checked_add(amount)
                    .ok_or(ContractError::Overflow)?;
            },
            BetSide::Down => {
                round.pool_down = round.pool_down
                    .checked_add(amount)
                    .ok_or(ContractError::Overflow)?;
            },
        }
        
        env.storage().persistent().set(&DataKey::Positions, &positions);
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
        
        Ok(())
    }
    
    /// Returns user's position in the current round
    pub fn get_user_position(env: Env, user: Address) -> Option<UserPosition> {
        let positions: Map<Address, UserPosition> = env.storage()
            .persistent()
            .get(&DataKey::Positions)
            .unwrap_or(Map::new(&env));
        
        positions.get(user)
    }
    
    /// Resolves the round with final price (oracle only)
    /// Winners split losers' pool proportionally; ties get refunds
    pub fn resolve_round(env: Env, final_price: u128) -> Result<(), ContractError> {
        if final_price == 0 {
            return Err(ContractError::InvalidPrice);
        }
        
        let oracle: Address = env.storage()
            .persistent()
            .get(&DataKey::Oracle)
            .ok_or(ContractError::OracleNotSet)?;
        
        oracle.require_auth();
        
        let round: Round = env.storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .ok_or(ContractError::NoActiveRound)?;
        
        let positions: Map<Address, UserPosition> = env.storage()
            .persistent()
            .get(&DataKey::Positions)
            .unwrap_or(Map::new(&env));
        
        let price_went_up = final_price > round.price_start;
        let price_went_down = final_price < round.price_start;
        let price_unchanged = final_price == round.price_start;
        
        if price_unchanged {
            Self::_record_refunds(&env, positions)?;
        } else if price_went_up {
            Self::_record_winnings(&env, positions, BetSide::Up, round.pool_up, round.pool_down)?;
        } else if price_went_down {
            Self::_record_winnings(&env, positions, BetSide::Down, round.pool_down, round.pool_up)?;
        }
        
        env.storage().persistent().remove(&DataKey::ActiveRound);
        env.storage().persistent().remove(&DataKey::Positions);
        
        Ok(())
    }
    
    /// Claims pending winnings and adds to balance
    pub fn claim_winnings(env: Env, user: Address) -> i128 {
        user.require_auth();
        
        let key = DataKey::PendingWinnings(user.clone());
        let pending: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        
        if pending == 0 {
            return 0;
        }
        
        let current_balance = Self::balance(env.clone(), user.clone());
        let new_balance = current_balance + pending;
        Self::_set_balance(&env, user.clone(), new_balance);
        
        env.storage().persistent().remove(&key);
        
        pending
    }
    
    /// Records refunds when price unchanged
    fn _record_refunds(env: &Env, positions: Map<Address, UserPosition>) -> Result<(), ContractError> {
        let keys: Vec<Address> = positions.keys();
        
        for i in 0..keys.len() {
            if let Some(user) = keys.get(i) {
                if let Some(position) = positions.get(user.clone()) {
                    let key = DataKey::PendingWinnings(user.clone());
                    let existing_pending: i128 = env.storage().persistent().get(&key).unwrap_or(0);
                    let new_pending = existing_pending
                        .checked_add(position.amount)
                        .ok_or(ContractError::Overflow)?;
                    env.storage().persistent().set(&key, &new_pending);
                }
            }
        }
        
        Ok(())
    }
    
    /// Records winnings for winning side
    /// Formula: payout = bet + (bet / winning_pool) * losing_pool
    fn _record_winnings(
        env: &Env,
        positions: Map<Address, UserPosition>,
        winning_side: BetSide,
        winning_pool: i128,
        losing_pool: i128,
    ) -> Result<(), ContractError> {
        if winning_pool == 0 {
            return Ok(());
        }
        
        let keys: Vec<Address> = positions.keys();
        
        for i in 0..keys.len() {
            if let Some(user) = keys.get(i) {
                if let Some(position) = positions.get(user.clone()) {
                    if position.side == winning_side {
                        let share_numerator = position.amount
                            .checked_mul(losing_pool)
                            .ok_or(ContractError::Overflow)?;
                        let share = share_numerator / winning_pool;
                        let payout = position.amount
                            .checked_add(share)
                            .ok_or(ContractError::Overflow)?;
                        
                        let key = DataKey::PendingWinnings(user.clone());
                        let existing_pending: i128 = env.storage().persistent().get(&key).unwrap_or(0);
                        let new_pending = existing_pending
                            .checked_add(payout)
                            .ok_or(ContractError::Overflow)?;
                        env.storage().persistent().set(&key, &new_pending);
                        
                        Self::_update_stats_win(env, user);
                    } else {
                        Self::_update_stats_loss(env, user);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    pub(crate) fn _update_stats_win(env: &Env, user: Address) {
        let key = DataKey::UserStats(user);
        let mut stats: UserStats = env.storage().persistent().get(&key).unwrap_or(UserStats {
            total_wins: 0,
            total_losses: 0,
            current_streak: 0,
            best_streak: 0,
        });
        
        stats.total_wins += 1;
        stats.current_streak += 1;
        
        if stats.current_streak > stats.best_streak {
            stats.best_streak = stats.current_streak;
        }
        
        env.storage().persistent().set(&key, &stats);
    }
    
    pub(crate) fn _update_stats_loss(env: &Env, user: Address) {
        let key = DataKey::UserStats(user);
        let mut stats: UserStats = env.storage().persistent().get(&key).unwrap_or(UserStats {
            total_wins: 0,
            total_losses: 0,
            current_streak: 0,
            best_streak: 0,
        });
        
        stats.total_losses += 1;
        stats.current_streak = 0;
        
        env.storage().persistent().set(&key, &stats);
    }
    
    /// Mints 1000 vXLM for new users (one-time only)
    pub fn mint_initial(env: Env, user: Address) -> i128 {
        user.require_auth();
        
        let key = DataKey::Balance(user.clone());
        
        if let Some(existing_balance) = env.storage().persistent().get(&key) {
            return existing_balance;
        }
        
        let initial_amount: i128 = 1000_0000000;
        env.storage().persistent().set(&key, &initial_amount);
        
        initial_amount
    }
    
    /// Returns user's vXLM balance
    pub fn balance(env: Env, user: Address) -> i128 {
        let key = DataKey::Balance(user);
        env.storage().persistent().get(&key).unwrap_or(0)
    }
    
    pub(crate) fn _set_balance(env: &Env, user: Address, amount: i128) {
        let key = DataKey::Balance(user);
        env.storage().persistent().set(&key, &amount);
    }
}

