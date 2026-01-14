#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, Vec};

/// Storage keys for organizing data in the contract
/// Think of these as "labels" for different storage compartments
/// 
/// The #[contracttype] attribute tells Soroban this can be stored in the contract
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Stores the balance for a specific user address
    Balance(Address),
    /// Stores the admin address (the person who can create rounds)
    Admin,
    /// Stores the oracle address (the trusted price provider)
    Oracle,
    /// Stores the currently active round
    ActiveRound,
    /// Stores user positions for the active round: Map of Address -> UserPosition
    Positions,
}

/// Represents which side a user bet on
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum BetSide {
    Up,   // User bet price will go UP
    Down, // User bet price will go DOWN
}

/// Stores an individual user's bet in a round
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct UserPosition {
    /// How much vXLM the user bet
    pub amount: i128,
    /// Which side they bet on
    pub side: BetSide,
}

/// Represents a prediction round
/// This stores all the information about an active betting round
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Round {
    /// The starting price of XLM when the round begins (in stroops)
    pub price_start: u128,
    /// The ledger number when this round ends
    /// Ledgers are like blocks in blockchain - they increment every ~5 seconds
    pub end_ledger: u32,
    /// Total vXLM in the "UP" pool (people betting price will go up)
    pub pool_up: i128,
    /// Total vXLM in the "DOWN" pool (people betting price will go down)
    pub pool_down: i128,
}

/// The main contract structure
/// This represents your vXLM (virtual XLM) token contract
#[contract]
pub struct VirtualTokenContract;

#[contractimpl]
impl VirtualTokenContract {
    /// Initializes the contract by setting the admin and oracle
    /// This should be called once when deploying the contract
    /// 
    /// # Parameters
    /// * `env` - The contract environment
    /// * `admin` - The address that will have admin privileges (creates rounds)
    /// * `oracle` - The address that provides price data and resolves rounds
    /// 
    /// # Security
    /// Choose admin and oracle addresses carefully - they control the contract!
    pub fn initialize(env: Env, admin: Address, oracle: Address) {
        // Ensure admin authorizes this initialization
        admin.require_auth();
        
        // Check if admin is already set to prevent re-initialization
        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }
        
        // Store the admin and oracle addresses
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::Oracle, &oracle);
    }
    
    /// Creates a new prediction round
    /// Only the admin can call this function
    /// 
    /// # Parameters
    /// * `env` - The contract environment
    /// * `start_price` - The current XLM price in stroops (e.g., 1 XLM = 10,000,000 stroops)
    /// * `duration_ledgers` - How many ledgers (blocks) the round should last
    ///                        Example: 60 ledgers ≈ 5 minutes (since ledgers are ~5 seconds)
    /// 
    /// # How it works
    /// 1. Verifies the caller is the admin
    /// 2. Calculates when the round will end
    /// 3. Creates a new Round with empty betting pools
    /// 4. Stores it as the active round
    pub fn create_round(env: Env, start_price: u128, duration_ledgers: u32) {
        // Get the admin address from storage
        let admin: Address = env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Admin not set - call initialize first");
        
        // Verify that the caller is the admin
        // This prevents random users from creating rounds
        admin.require_auth();
        
        // Get the current ledger number and calculate end ledger
        // Think of ledgers like block numbers - they keep incrementing
        let current_ledger = env.ledger().sequence();
        let end_ledger = current_ledger + duration_ledgers;
        
        // Create a new round with the given parameters
        let round = Round {
            price_start: start_price,
            end_ledger,
            pool_up: 0,    // No bets yet
            pool_down: 0,  // No bets yet
        };
        
        // Save the round as the active round
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    }
    
    /// Gets the currently active round
    /// 
    /// # Returns
    /// Option<Round> - Some(round) if there's an active round, None if not
    /// 
    /// # Use case
    /// Frontend can call this to display current round info to users
    pub fn get_active_round(env: Env) -> Option<Round> {
        env.storage().persistent().get(&DataKey::ActiveRound)
    }
    
    /// Gets the admin address
    /// 
    /// # Returns
    /// Option<Address> - Some(admin) if set, None if not initialized
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }
    
    /// Gets the oracle address
    /// 
    /// # Returns
    /// Option<Address> - Some(oracle) if set, None if not initialized
    pub fn get_oracle(env: Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Oracle)
    }
    
    /// Resolves a round with the final price and distributes winnings
    /// Only the oracle can call this function
    /// 
    /// # Parameters
    /// * `env` - The contract environment
    /// * `final_price` - The XLM price at round end (in stroops)
    /// 
    /// # How it works
    /// 1. Verify caller is the oracle
    /// 2. Get the active round
    /// 3. Compare final_price with price_start to determine winners
    /// 4. Calculate payouts proportionally for each winner
    /// 5. Update all user balances
    /// 6. Clear the round and positions
    /// 
    /// # Payout logic
    /// - If price went UP: UP bettors split the DOWN pool proportionally
    /// - If price went DOWN: DOWN bettors split the UP pool proportionally
    /// - If price UNCHANGED: Everyone gets their bet back (no winners/losers)
    pub fn resolve_round(env: Env, final_price: u128) {
        // Get and verify oracle
        let oracle: Address = env.storage()
            .persistent()
            .get(&DataKey::Oracle)
            .expect("Oracle not set");
        
        oracle.require_auth();
        
        // Get the active round
        let round: Round = env.storage()
            .persistent()
            .get(&DataKey::ActiveRound)
            .expect("No active round to resolve");
        
        // Get all user positions
        let positions: Map<Address, UserPosition> = env.storage()
            .persistent()
            .get(&DataKey::Positions)
            .unwrap_or(Map::new(&env));
        
        // Determine the outcome by comparing prices
        let price_went_up = final_price > round.price_start;
        let price_went_down = final_price < round.price_start;
        let price_unchanged = final_price == round.price_start;
        
        // Handle the edge case: price didn't change
        if price_unchanged {
            // Return everyone's bets - no winners or losers
            Self::_refund_all(&env, positions);
        } else if price_went_up {
            // UP side wins - they split the DOWN pool
            Self::_distribute_winnings(&env, positions, BetSide::Up, round.pool_up, round.pool_down);
        } else if price_went_down {
            // DOWN side wins - they split the UP pool
            Self::_distribute_winnings(&env, positions, BetSide::Down, round.pool_down, round.pool_up);
        }
        
        // Clear the round and positions to prepare for the next round
        env.storage().persistent().remove(&DataKey::ActiveRound);
        env.storage().persistent().remove(&DataKey::Positions);
    }
    
    /// Internal function to refund all bets (when price unchanged)
    /// 
    /// # Parameters
    /// * `env` - The contract environment
    /// * `positions` - Map of all user positions
    fn _refund_all(env: &Env, positions: Map<Address, UserPosition>) {
        // Iterate through all positions
        let keys: Vec<Address> = positions.keys();
        
        for i in 0..keys.len() {
            if let Some(user) = keys.get(i) {
                if let Some(position) = positions.get(user.clone()) {
                    // Get current balance
                    let current_balance = Self::balance(env.clone(), user.clone());
                    // Add back the bet amount
                    let new_balance = current_balance + position.amount;
                    // Update balance
                    Self::_set_balance(env, user, new_balance);
                }
            }
        }
    }
    
    /// Internal function to distribute winnings to the winning side
    /// 
    /// # Parameters
    /// * `env` - The contract environment
    /// * `positions` - Map of all user positions
    /// * `winning_side` - Which side won (Up or Down)
    /// * `winning_pool` - Total amount bet by winners
    /// * `losing_pool` - Total amount bet by losers (this gets distributed)
    /// 
    /// # Payout Formula
    /// For each winner:
    /// payout = their_bet + (their_bet / winning_pool) * losing_pool
    /// 
    /// Example:
    /// - Alice bet 100 on UP, Bob bet 200 on UP (winning_pool = 300)
    /// - Charlie bet 150 on DOWN (losing_pool = 150)
    /// - Alice gets: 100 + (100/300) * 150 = 100 + 50 = 150
    /// - Bob gets: 200 + (200/300) * 150 = 200 + 100 = 300
    fn _distribute_winnings(
        env: &Env,
        positions: Map<Address, UserPosition>,
        winning_side: BetSide,
        winning_pool: i128,
        losing_pool: i128,
    ) {
        // If no one bet on the winning side, no payouts needed
        if winning_pool == 0 {
            return;
        }
        
        let keys: Vec<Address> = positions.keys();
        
        for i in 0..keys.len() {
            if let Some(user) = keys.get(i) {
                if let Some(position) = positions.get(user.clone()) {
                    // Only pay out winners
                    if position.side == winning_side {
                        // Calculate proportional share of the losing pool
                        // share = (user_bet / winning_pool) * losing_pool
                        let share = (position.amount * losing_pool) / winning_pool;
                        
                        // Total payout = original bet + share of losing pool
                        let payout = position.amount + share;
                        
                        // Update user balance
                        let current_balance = Self::balance(env.clone(), user.clone());
                        let new_balance = current_balance + payout;
                        Self::_set_balance(env, user, new_balance);
                    }
                    // Losers get nothing - their bets are already gone
                }
            }
        }
    }
    
    /// Mints (creates) initial vXLM tokens for a user on their first interaction
    /// 
    /// # Parameters
    /// * `env` - The contract environment (provided by Soroban, gives access to storage, etc.)
    /// * `user` - The address of the user who will receive tokens
    /// 
    /// # How it works
    /// 1. Checks if user already has a balance
    /// 2. If not, gives them 1000 vXLM as a starting amount
    /// 3. Stores this balance in the contract's persistent storage
    pub fn mint_initial(env: Env, user: Address) -> i128 {
        // Verify that the user is authorized to call this function
        // This ensures only the user themselves can mint tokens for their account
        user.require_auth();
        
        // Create a storage key for this user's balance
        let key = DataKey::Balance(user.clone());
        
        // Check if the user already has a balance
        // get() returns an Option: Some(balance) if exists, None if not
        if let Some(existing_balance) = env.storage().persistent().get(&key) {
            // User already has tokens, return their existing balance
            return existing_balance;
        }
        
        // User is new! Give them 1000 vXLM as initial tokens
        // Note: We use 1000_0000000 because Stellar uses 7 decimal places (stroops)
        let initial_amount: i128 = 1000_0000000; // 1000 vXLM
        
        // Save the balance to persistent storage
        // This data will remain even after the transaction completes
        env.storage().persistent().set(&key, &initial_amount);
        
        // Return the newly minted amount
        initial_amount
    }
    
    /// Queries (reads) the current vXLM balance for a user
    /// 
    /// # Parameters
    /// * `env` - The contract environment
    /// * `user` - The address of the user whose balance we want to check
    /// 
    /// # Returns
    /// The user's balance as an i128 (128-bit integer)
    /// Returns 0 if the user has never received tokens
    pub fn balance(env: Env, user: Address) -> i128 {
        // Create the storage key for this user
        let key = DataKey::Balance(user);
        
        // Try to get the balance from storage
        // unwrap_or(0) means: if balance exists, use it; otherwise, return 0
        env.storage().persistent().get(&key).unwrap_or(0)
    }
    
    /// Internal helper function to update a user's balance
    /// The underscore prefix means this is a private/internal function
    /// 
    /// # Parameters
    /// * `env` - The contract environment
    /// * `user` - The address whose balance to update
    /// * `amount` - The new balance amount
    fn _set_balance(env: &Env, user: Address, amount: i128) {
        let key = DataKey::Balance(user);
        env.storage().persistent().set(&key, &amount);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    #[test]
    fn test_mint_initial() {
        // Create a test environment
        let env = Env::default();
        
        // Register our contract in the test environment
        // This deploys the contract to the test blockchain and returns its unique ID
        // Think of it as: installing your app on a test phone before you can use it
        // The () means we're not passing any initialization arguments
        let contract_id = env.register(VirtualTokenContract, ());
        
        // Create a client to interact with the contract
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        
        // Generate a random test user address
        let user = Address::generate(&env);
        
        // Mock the authorization (in tests, we need to simulate user approval)
        env.mock_all_auths();
        
        // Call mint_initial for the user
        let balance = client.mint_initial(&user);
        
        // Verify the user received 1000 vXLM
        assert_eq!(balance, 1000_0000000);
        
        // Verify we can query the balance
        let queried_balance = client.balance(&user);
        assert_eq!(queried_balance, 1000_0000000);
    }
    
    #[test]
    fn test_mint_initial_only_once() {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        
        env.mock_all_auths();
        
        // First mint
        let first_mint = client.mint_initial(&user);
        assert_eq!(first_mint, 1000_0000000);
        
        // Try to mint again - should return existing balance, not mint more
        let second_mint = client.mint_initial(&user);
        assert_eq!(second_mint, 1000_0000000);
        
        // Balance should still be 1000, not 2000
        let balance = client.balance(&user);
        assert_eq!(balance, 1000_0000000);
    }
    
    #[test]
    fn test_balance_for_new_user() {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        
        // Query balance for a user who never minted
        let balance = client.balance(&user);
        
        // Should return 0
        assert_eq!(balance, 0);
    }
    
    #[test]
    fn test_initialize() {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        
        // Generate an admin and oracle address
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        
        env.mock_all_auths();
        
        // Initialize the contract
        client.initialize(&admin, &oracle);
        
        // Verify admin and oracle are set
        let stored_admin = client.get_admin();
        let stored_oracle = client.get_oracle();
        assert_eq!(stored_admin, Some(admin));
        assert_eq!(stored_oracle, Some(oracle));
    }
    
    #[test]
    #[should_panic(expected = "Contract already initialized")]
    fn test_initialize_twice_fails() {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        
        env.mock_all_auths();
        
        // Initialize once
        client.initialize(&admin, &oracle);
        
        // Try to initialize again - should panic
        client.initialize(&admin, &oracle);
    }
    
    #[test]
    fn test_create_round() {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        
        // Set up admin
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin, &oracle);
        
        // Create a round
        let start_price: u128 = 1_5000000; // 1.5 XLM in stroops
        let duration: u32 = 60; // 60 ledgers
        
        client.create_round(&start_price, &duration);
        
        // Verify the round was created
        let round = client.get_active_round().expect("Round should exist");
        
        assert_eq!(round.price_start, start_price);
        assert_eq!(round.pool_up, 0);
        assert_eq!(round.pool_down, 0);
        
        // Verify end_ledger is set correctly (current ledger + duration)
        // Note: In tests, current ledger starts at 0
        assert_eq!(round.end_ledger, duration);
    }
    
    #[test]
    #[should_panic(expected = "Admin not set - call initialize first")]
    fn test_create_round_without_init_fails() {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        
        env.mock_all_auths();
        
        // Try to create round without initializing - should panic
        client.create_round(&1_0000000, &60);
    }
    
    #[test]
    fn test_get_active_round_when_none() {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        
        // No round created yet
        let round = client.get_active_round();
        
        assert_eq!(round, None);
    }
    
    #[test]
    fn test_resolve_round_price_unchanged() {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        
        client.initialize(&admin, &oracle);
        
        // Create a round with start price 1.5 XLM
        let start_price: u128 = 1_5000000;
        client.create_round(&start_price, &60);
        
        // Manually set up some test positions using env.as_contract
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        
        // Give users initial balances
        client.mint_initial(&user1);
        client.mint_initial(&user2);
        
        // Manually create positions for testing using as_contract
        env.as_contract(&contract_id, || {
            let mut positions = Map::<Address, UserPosition>::new(&env);
            positions.set(user1.clone(), UserPosition {
                amount: 100_0000000,
                side: BetSide::Up,
            });
            positions.set(user2.clone(), UserPosition {
                amount: 50_0000000,
                side: BetSide::Down,
            });
            
            // Store positions
            env.storage().persistent().set(&DataKey::Positions, &positions);
            
            // Update round pools to match positions
            let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
            round.pool_up = 100_0000000;
            round.pool_down = 50_0000000;
            env.storage().persistent().set(&DataKey::ActiveRound, &round);
        });
        
        // Get balances before resolution
        let user1_balance_before = client.balance(&user1);
        let user2_balance_before = client.balance(&user2);
        
        // Resolve with SAME price (unchanged)
        client.resolve_round(&start_price);
        
        // Both users should get their bets back
        let user1_balance_after = client.balance(&user1);
        let user2_balance_after = client.balance(&user2);
        
        assert_eq!(user1_balance_after, user1_balance_before + 100_0000000);
        assert_eq!(user2_balance_after, user2_balance_before + 50_0000000);
        
        // Round should be cleared
        assert_eq!(client.get_active_round(), None);
    }
    
    #[test]
    fn test_resolve_round_price_went_up() {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        
        client.initialize(&admin, &oracle);
        
        // Create a round with start price 1.0 XLM
        let start_price: u128 = 1_0000000;
        client.create_round(&start_price, &60);
        
        // Set up test users
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);
        
        // Give users initial balances
        client.mint_initial(&alice);
        client.mint_initial(&bob);
        client.mint_initial(&charlie);
        
        // Create positions using as_contract
        env.as_contract(&contract_id, || {
            let mut positions = Map::<Address, UserPosition>::new(&env);
            positions.set(alice.clone(), UserPosition {
                amount: 100_0000000,
                side: BetSide::Up,
            });
            positions.set(bob.clone(), UserPosition {
                amount: 200_0000000,
                side: BetSide::Up,
            });
            positions.set(charlie.clone(), UserPosition {
                amount: 150_0000000,
                side: BetSide::Down,
            });
            
            env.storage().persistent().set(&DataKey::Positions, &positions);
            
            let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
            round.pool_up = 300_0000000;
            round.pool_down = 150_0000000;
            env.storage().persistent().set(&DataKey::ActiveRound, &round);
        });
        
        let alice_before = client.balance(&alice);
        let bob_before = client.balance(&bob);
        let charlie_before = client.balance(&charlie);
        
        // Resolve with HIGHER price (1.5 XLM - price went UP)
        client.resolve_round(&1_5000000);
        
        // Calculate expected payouts:
        // Alice: 100 + (100/300) * 150 = 100 + 50 = 150
        // Bob: 200 + (200/300) * 150 = 200 + 100 = 300
        // Charlie: 0 (lost)
        
        assert_eq!(client.balance(&alice), alice_before + 150_0000000);
        assert_eq!(client.balance(&bob), bob_before + 300_0000000);
        assert_eq!(client.balance(&charlie), charlie_before); // No change (lost)
    }
    
    #[test]
    fn test_resolve_round_price_went_down() {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        
        client.initialize(&admin, &oracle);
        
        // Create a round with start price 2.0 XLM
        let start_price: u128 = 2_0000000;
        client.create_round(&start_price, &60);
        
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        
        client.mint_initial(&alice);
        client.mint_initial(&bob);
        
        // Create positions using as_contract
        env.as_contract(&contract_id, || {
            let mut positions = Map::<Address, UserPosition>::new(&env);
            positions.set(alice.clone(), UserPosition {
                amount: 200_0000000,
                side: BetSide::Down,
            });
            positions.set(bob.clone(), UserPosition {
                amount: 100_0000000,
                side: BetSide::Up,
            });
            
            env.storage().persistent().set(&DataKey::Positions, &positions);
            
            let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
            round.pool_up = 100_0000000;
            round.pool_down = 200_0000000;
            env.storage().persistent().set(&DataKey::ActiveRound, &round);
        });
        
        let alice_before = client.balance(&alice);
        let bob_before = client.balance(&bob);
        
        // Resolve with LOWER price (1.0 XLM - price went DOWN)
        client.resolve_round(&1_0000000);
        
        // Alice wins: 200 + (200/200) * 100 = 200 + 100 = 300
        // Bob loses: 0
        
        assert_eq!(client.balance(&alice), alice_before + 300_0000000);
        assert_eq!(client.balance(&bob), bob_before); // No change (lost)
    }
    
    #[test]
    #[should_panic(expected = "No active round to resolve")]
    fn test_resolve_round_without_active_round() {
        let env = Env::default();
        let contract_id = env.register(VirtualTokenContract, ());
        let client = VirtualTokenContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);
        env.mock_all_auths();
        
        client.initialize(&admin, &oracle);
        
        // Try to resolve without creating a round
        client.resolve_round(&1_0000000);
    }
}
