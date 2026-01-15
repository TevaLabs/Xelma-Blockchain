import { Buffer } from "buffer";
import { AssembledTransaction, Client as ContractClient, ClientOptions as ContractClientOptions, MethodOptions, Result } from '@stellar/stellar-sdk/contract';
import type { u32, u128, i128, Option } from '@stellar/stellar-sdk/contract';
export * from '@stellar/stellar-sdk';
export * as contract from '@stellar/stellar-sdk/contract';
export * as rpc from '@stellar/stellar-sdk/rpc';
/**
 * Represents a prediction round
 * This stores all the information about an active betting round
 */
export interface Round {
    /**
   * The ledger number when this round ends
   * Ledgers are like blocks in blockchain - they increment every ~5 seconds
   */
    end_ledger: u32;
    /**
   * Total vXLM in the "DOWN" pool (people betting price will go down)
   */
    pool_down: i128;
    /**
   * Total vXLM in the "UP" pool (people betting price will go up)
   */
    pool_up: i128;
    /**
   * The starting price of XLM when the round begins (in stroops)
   */
    price_start: u128;
}
/**
 * Represents which side a user bet on
 */
export type BetSide = {
    tag: "Up";
    values: void;
} | {
    tag: "Down";
    values: void;
};
/**
 * Storage keys for organizing data in the contract
 * Think of these as "labels" for different storage compartments
 *
 * The #[contracttype] attribute tells Soroban this can be stored in the contract
 */
export type DataKey = {
    tag: "Balance";
    values: readonly [string];
} | {
    tag: "Admin";
    values: void;
} | {
    tag: "Oracle";
    values: void;
} | {
    tag: "ActiveRound";
    values: void;
} | {
    tag: "Positions";
    values: void;
} | {
    tag: "PendingWinnings";
    values: readonly [string];
} | {
    tag: "UserStats";
    values: readonly [string];
};
/**
 * Tracks a user's prediction performance
 */
export interface UserStats {
    /**
   * Best winning streak ever achieved
   */
    best_streak: u32;
    /**
   * Current winning streak (consecutive wins)
   */
    current_streak: u32;
    /**
   * Total number of rounds lost
   */
    total_losses: u32;
    /**
   * Total number of rounds won
   */
    total_wins: u32;
}
/**
 * Stores an individual user's bet in a round
 */
export interface UserPosition {
    /**
   * How much vXLM the user bet
   */
    amount: i128;
    /**
   * Which side they bet on
   */
    side: BetSide;
}
/**
 * Custom error types for the contract
 * Using explicit error codes helps with debugging and provides clear feedback
 */
export declare const ContractError: {
    /**
     * Contract has already been initialized
     */
    1: {
        message: string;
    };
    /**
     * Admin address not set - call initialize first
     */
    2: {
        message: string;
    };
    /**
     * Oracle address not set - call initialize first
     */
    3: {
        message: string;
    };
    /**
     * Only admin can perform this action
     */
    4: {
        message: string;
    };
    /**
     * Only oracle can perform this action
     */
    5: {
        message: string;
    };
    /**
     * Bet amount must be greater than zero
     */
    6: {
        message: string;
    };
    /**
     * No active round exists
     */
    7: {
        message: string;
    };
    /**
     * Round has already ended
     */
    8: {
        message: string;
    };
    /**
     * User has insufficient balance
     */
    9: {
        message: string;
    };
    /**
     * User has already placed a bet in this round
     */
    10: {
        message: string;
    };
    /**
     * Arithmetic overflow occurred
     */
    11: {
        message: string;
    };
    /**
     * Invalid price value
     */
    12: {
        message: string;
    };
    /**
     * Invalid duration value
     */
    13: {
        message: string;
    };
};
export interface Client {
    /**
     * Construct and simulate a balance transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Queries (reads) the current vXLM balance for a user
     *
     * # Parameters
     * * `env` - The contract environment
     * * `user` - The address of the user whose balance we want to check
     *
     * # Returns
     * The user's balance as an i128 (128-bit integer)
     * Returns 0 if the user has never received tokens
     */
    balance: ({ user }: {
        user: string;
    }, options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<i128>>;
    /**
     * Construct and simulate a get_admin transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Gets the admin address
     *
     * # Returns
     * Option<Address> - Some(admin) if set, None if not initialized
     */
    get_admin: (options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<Option<string>>>;
    /**
     * Construct and simulate a place_bet transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Places a bet on the active round
     *
     * # Parameters
     * * `env` - The contract environment
     * * `user` - The address of the user placing the bet
     * * `amount` - Amount of vXLM to bet (must be > 0)
     * * `side` - Which side to bet on (Up or Down)
     *
     * # Security
     * - Requires user authorization (prevents unauthorized betting)
     * - Validates bet amount is positive
     * - Checks round is still active (prevents late bets)
     * - Verifies sufficient balance (prevents negative balances)
     * - Prevents double betting in same round
     * - Uses checked arithmetic to prevent overflow
     * - No reentrancy risk: state updates before external calls (CEI pattern)
     *
     * # Errors
     * - `ContractError::InvalidBetAmount` if amount <= 0
     * - `ContractError::NoActiveRound` if no round exists
     * - `ContractError::RoundEnded` if round has ended
     * - `ContractError::InsufficientBalance` if user balance too low
     * - `ContractError::AlreadyBet` if user already bet in this round
     * - `ContractError::Overflow` if pool calculation overflows
     */
    place_bet: ({ user, amount, side }: {
        user: string;
        amount: i128;
        side: BetSide;
    }, options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<Result<void>>>;
    /**
     * Construct and simulate a get_oracle transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Gets the oracle address
     *
     * # Returns
     * Option<Address> - Some(oracle) if set, None if not initialized
     */
    get_oracle: (options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<Option<string>>>;
    /**
     * Construct and simulate a initialize transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Initializes the contract by setting the admin and oracle
     * This should be called once when deploying the contract
     *
     * # Parameters
     * * `env` - The contract environment
     * * `admin` - The address that will have admin privileges (creates rounds)
     * * `oracle` - The address that provides price data and resolves rounds
     *
     * # Security
     * - Prevents re-initialization attacks
     * - Requires admin authorization
     * - Admin and oracle cannot be the same (separation of concerns)
     *
     * # Errors
     * Returns `ContractError::AlreadyInitialized` if contract was already initialized
     */
    initialize: ({ admin, oracle }: {
        admin: string;
        oracle: string;
    }, options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<Result<void>>>;
    /**
     * Construct and simulate a create_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Creates a new prediction round
     * Only the admin can call this function
     *
     * # Parameters
     * * `env` - The contract environment
     * * `start_price` - The current XLM price in stroops (e.g., 1 XLM = 10,000,000 stroops)
     * * `duration_ledgers` - How many ledgers (blocks) the round should last
     * Example: 60 ledgers ≈ 5 minutes (since ledgers are ~5 seconds)
     *
     * # Security
     * - Only admin can create rounds (prevents unauthorized round creation)
     * - Validates price is non-zero
     * - Validates duration is reasonable (prevents DoS)
     * - Checks for overflow when calculating end_ledger
     *
     * # Errors
     * - `ContractError::AdminNotSet` if contract not initialized
     * - `ContractError::InvalidPrice` if start_price is 0
     * - `ContractError::InvalidDuration` if duration is 0 or too large
     * - `ContractError::Overflow` if end_ledger calculation overflows
     */
    create_round: ({ start_price, duration_ledgers }: {
        start_price: u128;
        duration_ledgers: u32;
    }, options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<Result<void>>>;
    /**
     * Construct and simulate a mint_initial transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Mints (creates) initial vXLM tokens for a user on their first interaction
     *
     * # Parameters
     * * `env` - The contract environment (provided by Soroban, gives access to storage, etc.)
     * * `user` - The address of the user who will receive tokens
     *
     * # How it works
     * 1. Checks if user already has a balance
     * 2. If not, gives them 1000 vXLM as a starting amount
     * 3. Stores this balance in the contract's persistent storage
     */
    mint_initial: ({ user }: {
        user: string;
    }, options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<i128>>;
    /**
     * Construct and simulate a resolve_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Resolves a round with the final price and calculates winnings
     * Only the oracle can call this function
     *
     * # Parameters
     * * `env` - The contract environment
     * * `final_price` - The XLM price at round end (in stroops)
     *
     * # Security
     * - Only oracle can resolve (prevents unauthorized resolution)
     * - Validates final price is non-zero
     * - Uses checked arithmetic in payout calculations
     * - No reentrancy: state cleared after all calculations
     * - Proportional distribution prevents manipulation
     *
     * # Errors
     * - `ContractError::OracleNotSet` if oracle not configured
     * - `ContractError::NoActiveRound` if no round to resolve
     * - `ContractError::InvalidPrice` if final_price is 0
     *
     * # Payout logic
     * - If price went UP: UP bettors split the DOWN pool proportionally
     * - If price went DOWN: DOWN bettors split the UP pool proportionally
     * - If price UNCHANGED: Everyone gets their bet back (no winners/losers)
     */
    resolve_round: ({ final_price }: {
        final_price: u128;
    }, options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<Result<void>>>;
    /**
     * Construct and simulate a claim_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Claims pending winnings for a user
     *
     * # Parameters
     * * `env` - The contract environment
     * * `user` - The address claiming winnings
     *
     * # How it works
     * 1. Check if user has pending winnings
     * 2. Add winnings to user's balance
     * 3. Clear pending winnings
     *
     * # Returns
     * Amount claimed (0 if no pending winnings)
     */
    claim_winnings: ({ user }: {
        user: string;
    }, options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<i128>>;
    /**
     * Construct and simulate a get_user_stats transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Gets a user's statistics (wins, losses, streaks)
     *
     * # Parameters
     * * `env` - The contract environment
     * * `user` - The address of the user
     *
     * # Returns
     * UserStats if the user has participated, or default stats (all zeros)
     */
    get_user_stats: ({ user }: {
        user: string;
    }, options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<UserStats>>;
    /**
     * Construct and simulate a get_active_round transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Gets the currently active round
     *
     * # Returns
     * Option<Round> - Some(round) if there's an active round, None if not
     *
     * # Use case
     * Frontend can call this to display current round info to users
     */
    get_active_round: (options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<Option<Round>>>;
    /**
     * Construct and simulate a get_user_position transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Gets a user's position in the current round
     *
     * # Parameters
     * * `env` - The contract environment
     * * `user` - The address of the user
     *
     * # Returns
     * Option<UserPosition> - Some(position) if user has bet, None if not
     */
    get_user_position: ({ user }: {
        user: string;
    }, options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<Option<UserPosition>>>;
    /**
     * Construct and simulate a get_pending_winnings transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
     * Gets a user's pending winnings (amount they can claim)
     *
     * # Parameters
     * * `env` - The contract environment
     * * `user` - The address of the user
     *
     * # Returns
     * Amount of vXLM the user can claim (0 if none)
     */
    get_pending_winnings: ({ user }: {
        user: string;
    }, options?: {
        /**
         * The fee to pay for the transaction. Default: BASE_FEE
         */
        fee?: number;
        /**
         * The maximum amount of time to wait for the transaction to complete. Default: DEFAULT_TIMEOUT
         */
        timeoutInSeconds?: number;
        /**
         * Whether to automatically simulate the transaction when constructing the AssembledTransaction. Default: true
         */
        simulate?: boolean;
    }) => Promise<AssembledTransaction<i128>>;
}
export declare class Client extends ContractClient {
    readonly options: ContractClientOptions;
    static deploy<T = Client>(
    /** Options for initializing a Client as well as for calling a method, with extras specific to deploying. */
    options: MethodOptions & Omit<ContractClientOptions, "contractId"> & {
        /** The hash of the Wasm blob, which must already be installed on-chain. */
        wasmHash: Buffer | string;
        /** Salt used to generate the contract's ID. Passed through to {@link Operation.createCustomContract}. Default: random. */
        salt?: Buffer | Uint8Array;
        /** The format used to decode `wasmHash`, if it's provided as a string. */
        format?: "hex" | "base64";
    }): Promise<AssembledTransaction<T>>;
    constructor(options: ContractClientOptions);
    readonly fromJSON: {
        balance: (json: string) => AssembledTransaction<bigint>;
        get_admin: (json: string) => AssembledTransaction<Option<string>>;
        place_bet: (json: string) => AssembledTransaction<Result<void, import("@stellar/stellar-sdk/contract").ErrorMessage>>;
        get_oracle: (json: string) => AssembledTransaction<Option<string>>;
        initialize: (json: string) => AssembledTransaction<Result<void, import("@stellar/stellar-sdk/contract").ErrorMessage>>;
        create_round: (json: string) => AssembledTransaction<Result<void, import("@stellar/stellar-sdk/contract").ErrorMessage>>;
        mint_initial: (json: string) => AssembledTransaction<bigint>;
        resolve_round: (json: string) => AssembledTransaction<Result<void, import("@stellar/stellar-sdk/contract").ErrorMessage>>;
        claim_winnings: (json: string) => AssembledTransaction<bigint>;
        get_user_stats: (json: string) => AssembledTransaction<UserStats>;
        get_active_round: (json: string) => AssembledTransaction<Option<Round>>;
        get_user_position: (json: string) => AssembledTransaction<Option<UserPosition>>;
        get_pending_winnings: (json: string) => AssembledTransaction<bigint>;
    };
}
