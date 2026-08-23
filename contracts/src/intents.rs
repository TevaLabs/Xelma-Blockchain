// SPDX-License-Identifier: MIT
//! Authorization-zone intent/keeper framework (Issue #370).
//!
//! ## Overview
//!
//! This module allows users to issue signed, scoped, expiring **intents** that
//! authorize a nominated third-party **keeper** to perform one specific
//! permissioned action on their behalf:
//!
//! | Scope        | Keeper operation                                             |
//! |--------------|--------------------------------------------------------------|
//! | `Resolve`    | Submit an oracle payload to settle the active round.         |
//! | `Claim`      | Withdraw the user's pending winnings to the user's account.  |
//! | `CreateNext` | Spin up the next round from the admin-configured template.   |
//!
//! ## Security model
//!
//! - **User custody preserved**: the `Claim` path moves funds to `user`, not
//!   to the keeper. The keeper cannot redirect winnings.
//! - **Scope isolation**: a keeper granted `Resolve` cannot invoke `Claim` or
//!   `CreateNext`. The contract rejects scope mismatches.
//! - **Replay protection**: every intent carries a per-`(user, scope)` nonce.
//!   Consumed nonces are permanently tombstoned and never reused.
//! - **Expiry**: intents have an inclusive `expires_at_ledger`. The contract
//!   rejects execution at any ledger strictly greater than that value.
//! - **Revocation**: a user may revoke any active intent before execution.
//! - **Keeper registration**: an admin may require that only explicitly
//!   registered keepers are permitted to execute intents. When the flag is off
//!   (the default), any keeper named in a valid intent may execute.
//! - **Pause/rate-limit respect**: all execution paths check the contract's
//!   runtime mode before proceeding.
//!
//! ## Threat model
//!
//! See `docs/INTENT_THREAT_MODEL.md` for the full threat-model write-up.

use crate::admin::{_ensure_not_paused, _require_supported_schema};
use crate::betting;
use crate::common::{TTL_BUMP_AMOUNT, TTL_BUMP_THRESHOLD};
use crate::errors::ContractError;
use crate::types::{
    DataKey, IntentKey, KeeperIntent, KeeperIntentStatus, KeeperScope, OraclePayload, RoundTemplate,
};
use soroban_sdk::{symbol_short, Address, Env};

/// Maximum number of ledgers a keeper intent may be valid for.
///
/// Caps at ~60 days (5 s/ledger). Prevents intents that stay valid so long
/// they become operational liabilities.
pub const MAX_INTENT_EXPIRY_LEDGERS: u32 = 1_036_800; // ~60 days

/// Minimum number of ledgers a keeper intent must be valid for.
///
/// Prevents dust intents that expire before a keeper can realistically act.
pub const MIN_INTENT_EXPIRY_LEDGERS: u32 = 6; // ~30 s

// ─── Internal helpers ──────────────────────────────────────────────────────────

/// Bumps the TTL of an `IntentKey` persistent entry.
fn _extend_intent_ttl<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, key: &T) {
    if env.storage().persistent().has(key) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_BUMP_THRESHOLD, TTL_BUMP_AMOUNT);
    }
}

/// Loads the admin address or returns `AdminNotSet`.
fn _load_admin(env: &Env) -> Result<Address, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::Admin)
        .ok_or(ContractError::AdminNotSet)
}

/// Reads (and advances) the nonce cursor for `(user, scope)`.
///
/// The cursor starts at 0 and is incremented each time a new intent is created.
/// Returned value is the nonce assigned to the new intent.
fn _next_nonce(env: &Env, user: &Address, scope: &KeeperScope) -> u64 {
    let cursor_key = IntentKey::IntentNonceCursor(user.clone(), scope.clone());
    let current: u64 = env
        .storage()
        .persistent()
        .get(&cursor_key)
        .unwrap_or(0u64);
    let next = current.saturating_add(1);
    env.storage().persistent().set(&cursor_key, &next);
    _extend_intent_ttl(env, &cursor_key);
    current
}

/// Validates that the given keeper is allowed to execute intents.
///
/// When keeper registration is required (flag present and `true`), only
/// addresses registered by the admin via `register_keeper` may proceed.
fn _check_keeper_allowed(env: &Env, keeper: &Address) -> Result<(), ContractError> {
    let required: bool = env
        .storage()
        .persistent()
        .get(&IntentKey::KeeperRegistrationRequired)
        .unwrap_or(false);
    if required {
        let registered: bool = env
            .storage()
            .persistent()
            .get(&IntentKey::RegisteredKeeper(keeper.clone()))
            .unwrap_or(false);
        if !registered {
            return Err(ContractError::KeeperNotRegistered);
        }
    }
    Ok(())
}

/// Guards that an intent is in `Active` status and has not expired.
///
/// Returns a mutable clone of the intent on success so callers can update its
/// status before writing back.
fn _check_intent_active(env: &Env, intent: &KeeperIntent) -> Result<(), ContractError> {
    match intent.status {
        KeeperIntentStatus::Active => {}
        KeeperIntentStatus::Consumed => return Err(ContractError::IntentAlreadyConsumed),
        KeeperIntentStatus::Expired => return Err(ContractError::IntentExpired),
        KeeperIntentStatus::Revoked => return Err(ContractError::IntentRevoked),
    }
    // Re-check expiry at execution time (status may be stale if intent was
    // not explicitly marked expired on-chain).
    if env.ledger().sequence() > intent.expires_at_ledger {
        return Err(ContractError::IntentExpired);
    }
    Ok(())
}

/// Consumes an intent: writes a tombstone, updates status to `Consumed`,
/// and emits a `keeper_exec` event.
fn _consume_intent(
    env: &Env,
    intent: &mut KeeperIntent,
    intent_key: &IntentKey,
    scope_label: soroban_sdk::Symbol,
) {
    intent.status = KeeperIntentStatus::Consumed;
    env.storage().persistent().set(intent_key, intent);

    let tombstone_key =
        IntentKey::ConsumedIntentNonce(intent.user.clone(), intent.scope.clone(), intent.nonce);
    env.storage().persistent().set(&tombstone_key, &true);
    // Tombstone TTL is bumped to the maximum — replay protection must outlive
    // the original intent expiry window.
    env.storage().persistent().extend_ttl(
        &tombstone_key,
        TTL_BUMP_THRESHOLD,
        TTL_BUMP_AMOUNT,
    );

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("intent"), symbol_short!("exec")),
        (
            intent.keeper.clone(),
            intent.user.clone(),
            scope_label,
            intent.nonce,
        ),
    );
}

// ─── Public entry points ───────────────────────────────────────────────────────

/// Creates a new keeper intent authorizing `keeper` to execute `scope` on
/// behalf of the caller (`user`) within `expiry_ledgers` from now.
///
/// # Authorization
/// The transaction must carry `user`'s auth (`user.require_auth()`).
///
/// # Returns
/// The nonce assigned to the new intent. The keeper needs this nonce to look
/// up and execute the intent.
///
/// # Errors
/// - `InvalidIntentExpiry` — `expiry_ledgers` is 0 or exceeds
///   `MAX_INTENT_EXPIRY_LEDGERS`.
/// - `ContractPaused` — the contract is in `FullyPaused` mode.
/// - `KeeperNotRegistered` — registration is required and `keeper` is not
///   registered.
pub fn authorize_keeper_intent(
    env: Env,
    user: Address,
    keeper: Address,
    scope: KeeperScope,
    expiry_ledgers: u32,
) -> Result<u64, ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();
    _ensure_not_paused(&env)?;

    if expiry_ledgers < MIN_INTENT_EXPIRY_LEDGERS || expiry_ledgers > MAX_INTENT_EXPIRY_LEDGERS {
        return Err(ContractError::InvalidIntentExpiry);
    }

    _check_keeper_allowed(&env, &keeper)?;

    let current_ledger = env.ledger().sequence();
    let expires_at = current_ledger
        .checked_add(expiry_ledgers)
        .unwrap_or(u32::MAX);

    let nonce = _next_nonce(&env, &user, &scope);

    let intent = KeeperIntent {
        user: user.clone(),
        keeper: keeper.clone(),
        scope: scope.clone(),
        nonce,
        expires_at_ledger: expires_at,
        status: KeeperIntentStatus::Active,
        authorized_at_ledger: current_ledger,
    };

    let intent_key = IntentKey::Intent(user.clone(), scope.clone(), nonce);
    env.storage().persistent().set(&intent_key, &intent);
    _extend_intent_ttl(&env, &intent_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("intent"), symbol_short!("auth")),
        (user, keeper, nonce, expires_at),
    );

    Ok(nonce)
}

/// Revokes an active intent before it is executed.
///
/// # Authorization
/// Only the `user` who originally created the intent may revoke it.
///
/// # Errors
/// - `IntentNotFound` — no intent exists for the given `(user, scope, nonce)`.
/// - `IntentAlreadyConsumed` / `IntentExpired` / `IntentRevoked` — intent is
///   not in `Active` status.
pub fn revoke_keeper_intent(
    env: Env,
    user: Address,
    scope: KeeperScope,
    nonce: u64,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    user.require_auth();

    let intent_key = IntentKey::Intent(user.clone(), scope.clone(), nonce);
    let mut intent: KeeperIntent = env
        .storage()
        .persistent()
        .get(&intent_key)
        .ok_or(ContractError::IntentNotFound)?;

    _check_intent_active(&env, &intent)?;

    intent.status = KeeperIntentStatus::Revoked;
    env.storage().persistent().set(&intent_key, &intent);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("intent"), symbol_short!("revoke")),
        (user, nonce),
    );

    Ok(())
}

/// Returns the intent record for `(user, scope, nonce)`, if present.
pub fn get_keeper_intent(
    env: Env,
    user: Address,
    scope: KeeperScope,
    nonce: u64,
) -> Option<KeeperIntent> {
    let intent_key = IntentKey::Intent(user, scope, nonce);
    _extend_intent_ttl(&env, &intent_key);
    env.storage().persistent().get(&intent_key)
}

/// Keeper executes a `Resolve` intent: settles the active round using the
/// supplied oracle `payload`.
///
/// ## Authorization model
/// The keeper's account must sign (`keeper.require_auth()`).  The user's
/// custody is not weakened: the intent only authorises a specific oracle
/// payload delivery; fund movement follows the normal settlement path.
///
/// ## Errors
/// - `IntentNotFound` — no intent for `(user, scope, nonce)`.
/// - `IntentScopeMismatch` — intent scope is not `Resolve`.
/// - `IntentKeeperMismatch` — caller is not the keeper named in the intent.
/// - `IntentAlreadyConsumed`, `IntentExpired`, `IntentRevoked` — invalid state.
/// - Any error from the underlying `resolve_round` logic.
pub fn execute_keeper_resolve(
    env: Env,
    keeper: Address,
    user: Address,
    nonce: u64,
    payload: OraclePayload,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    keeper.require_auth();
    _ensure_not_paused(&env)?;
    _check_keeper_allowed(&env, &keeper)?;

    let intent_key = IntentKey::Intent(user.clone(), KeeperScope::Resolve, nonce);
    let mut intent: KeeperIntent = env
        .storage()
        .persistent()
        .get(&intent_key)
        .ok_or(ContractError::IntentNotFound)?;

    // Scope guard
    if intent.scope != KeeperScope::Resolve {
        return Err(ContractError::IntentScopeMismatch);
    }

    // Keeper identity guard
    if intent.keeper != keeper {
        return Err(ContractError::IntentKeeperMismatch);
    }

    _check_intent_active(&env, &intent)?;

    // Execute the settlement — passes through all existing oracle validation,
    // deviation checks, nonce deduplication, and quorum logic.
    crate::settlement::resolve_round(env.clone(), payload)?;

    _consume_intent(
        &env,
        &mut intent,
        &intent_key,
        symbol_short!("resolve"),
    );

    Ok(())
}

/// Keeper executes a `Claim` intent: withdraws the user's pending winnings.
///
/// Funds are transferred to the **user's** account, not the keeper's.
/// The keeper is acting purely as an automation operator.
///
/// ## Errors
/// - `IntentNotFound` — no intent for `(user, scope, nonce)`.
/// - `IntentScopeMismatch` — intent scope is not `Claim`.
/// - `IntentKeeperMismatch` — caller is not the keeper named in the intent.
/// - `IntentAlreadyConsumed`, `IntentExpired`, `IntentRevoked` — invalid state.
/// - Any error from the underlying `claim_winnings` logic.
pub fn execute_keeper_claim(
    env: Env,
    keeper: Address,
    user: Address,
    nonce: u64,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    keeper.require_auth();
    _ensure_not_paused(&env)?;
    _check_keeper_allowed(&env, &keeper)?;

    let intent_key = IntentKey::Intent(user.clone(), KeeperScope::Claim, nonce);
    let mut intent: KeeperIntent = env
        .storage()
        .persistent()
        .get(&intent_key)
        .ok_or(ContractError::IntentNotFound)?;

    if intent.scope != KeeperScope::Claim {
        return Err(ContractError::IntentScopeMismatch);
    }

    if intent.keeper != keeper {
        return Err(ContractError::IntentKeeperMismatch);
    }

    _check_intent_active(&env, &intent)?;

    // Claim winnings on behalf of the user. The claim_winnings path:
    //   1. Reads the user's pending winnings balance.
    //   2. Transfers it to the user's XLM account.
    //   3. Clears the pending winnings entry.
    // No funds can ever reach the keeper — the destination is hard-coded in
    // the settlement module as `user`.
    crate::settlement::claim_winnings(env.clone(), user.clone())?;

    _consume_intent(
        &env,
        &mut intent,
        &intent_key,
        symbol_short!("claim"),
    );

    Ok(())
}

/// Keeper executes a `CreateNext` intent: spins up the next round from the
/// admin-configured template.
///
/// ## Errors
/// - `IntentNotFound` — no intent for `(user, scope, nonce)`.
/// - `IntentScopeMismatch` — intent scope is not `CreateNext`.
/// - `IntentKeeperMismatch` — caller is not the keeper named in the intent.
/// - `IntentAlreadyConsumed`, `IntentExpired`, `IntentRevoked` — invalid state.
/// - Any error from `create_next_from_template` (e.g., `NoRoundTemplate`,
///   `RoundAlreadyActive`, `ContractPaused`).
pub fn execute_keeper_create_next(
    env: Env,
    keeper: Address,
    user: Address,
    nonce: u64,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    keeper.require_auth();
    _ensure_not_paused(&env)?;
    _check_keeper_allowed(&env, &keeper)?;

    let intent_key = IntentKey::Intent(user.clone(), KeeperScope::CreateNext, nonce);
    let mut intent: KeeperIntent = env
        .storage()
        .persistent()
        .get(&intent_key)
        .ok_or(ContractError::IntentNotFound)?;

    if intent.scope != KeeperScope::CreateNext {
        return Err(ContractError::IntentScopeMismatch);
    }

    if intent.keeper != keeper {
        return Err(ContractError::IntentKeeperMismatch);
    }

    _check_intent_active(&env, &intent)?;

    // Loads the admin-configured RoundTemplate and creates the round.
    let template: RoundTemplate = env
        .storage()
        .persistent()
        .get(&DataKey::RoundTemplate)
        .ok_or(ContractError::NoRoundTemplate)?;

    betting::create_round(env.clone(), template.start_price, template.mode)?;

    _consume_intent(
        &env,
        &mut intent,
        &intent_key,
        symbol_short!("crt_next"),
    );

    Ok(())
}

// ─── Admin: keeper registration ────────────────────────────────────────────────

/// Registers `keeper` as an authorised keeper operator (admin only).
///
/// When keeper registration is required (see `set_keeper_registration_required`),
/// only registered keepers may execute intents.
pub fn register_keeper(env: Env, keeper: Address) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin = _load_admin(&env)?;
    admin.require_auth();

    let key = IntentKey::RegisteredKeeper(keeper.clone());
    env.storage().persistent().set(&key, &true);
    _extend_intent_ttl(&env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("keeper"), symbol_short!("reg")),
        (keeper,),
    );

    Ok(())
}

/// Removes `keeper` from the registered-keeper allowlist (admin only).
///
/// Idempotent: removing an unregistered keeper is a no-op.
pub fn deregister_keeper(env: Env, keeper: Address) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin = _load_admin(&env)?;
    admin.require_auth();

    let key = IntentKey::RegisteredKeeper(keeper.clone());
    env.storage().persistent().remove(&key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("keeper"), symbol_short!("dereg")),
        (keeper,),
    );

    Ok(())
}

/// Toggles the global keeper-registration requirement (admin only).
///
/// When `true`, all intent executions require the keeper to be on the
/// registered-keeper allowlist. When `false` (default), any keeper named in a
/// valid intent may execute.
pub fn set_keeper_registration_required(env: Env, required: bool) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin = _load_admin(&env)?;
    admin.require_auth();

    let key = IntentKey::KeeperRegistrationRequired;
    if required {
        env.storage().persistent().set(&key, &true);
        _extend_intent_ttl(&env, &key);
    } else {
        env.storage().persistent().remove(&key);
    }

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("keeper"), symbol_short!("req_set")),
        (required,),
    );

    Ok(())
}

/// Returns whether `keeper` is currently registered.
pub fn is_keeper_registered(env: Env, keeper: Address) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&IntentKey::RegisteredKeeper(keeper))
        .unwrap_or(false)
}

/// Returns whether keeper registration is currently required.
pub fn is_keeper_registration_required(env: Env) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&IntentKey::KeeperRegistrationRequired)
        .unwrap_or(false)
}
