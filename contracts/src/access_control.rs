// SPDX-License-Identifier: MIT
//! Optional participant access control (Issue #274).
//!
//! This gates who may bet / predict on private or hackathon deployments
//! without forking the protocol. The model is intentionally simple and safe:
//!
//! - **Default open**: until an admin explicitly enables allowlist mode via
//!   [`set_access_control_enabled`], every address may participate (subject to
//!   the existing min-bet / cap / mode / window rules). Deploying the feature
//!   changes nothing on its own.
//! - **Denylist overrides**: a denylisted address is refused **regardless of
//!   mode**. This is an emergency block / ban that works on open deployments
//!   too, and always takes precedence over the allowlist.
//! - **Allowlist mode**: when enabled via
//!   [`set_access_control_enabled(true)`], only addresses that are explicitly
//!   allowlisted are admitted. The admin signal (`DataKeyCore::AccessControlEnabled`)
//!   is the single on/off switch; the per-address markers
//!   (`DataKeyScoped::Allowlisted` / `DataKeyScoped::Denylisted`) are operative
//!   state.
//! - **Admin-only mutations**: every entrypoint below calls `admin.require_auth()`
//!   and is gated by the policy gate, so a non-admin can neither add nor remove
//!   addresses nor toggle the mode.
//! - **List update events**: each mutation emits a structured event so indexers
//!   and dashboards can track membership changes without replaying storage.

use crate::admin::{_ensure_not_paused, _require_supported_schema};
use crate::common::_emit_action_rejected;
use crate::common::_extend_persistent_ttl;
use crate::errors::ContractError;
use crate::types::{AccessState, DataKeyCore, DataKeyScoped};
use soroban_sdk::{symbol_short, Address, Env};

/// Turns participant access control on (`true`) or off (`false`) (admin only).
///
/// When disabled, admission is open except for denylisted addresses. This is
/// the default, so the flag is stored only when it differs from `false`.
pub fn set_access_control_enabled(env: Env, enabled: bool) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("acc_mode"), e);
    })?;

    let key = DataKeyCore::AccessControlEnabled;
    if enabled {
        env.storage().persistent().set(&key, &true);
        _extend_persistent_ttl(&env, &key);
    } else {
        env.storage().persistent().remove(&key);
    }

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("access"), symbol_short!("mode")),
        (enabled,),
    );

    Ok(())
}

/// Returns whether allowlist mode is currently enabled (`false` = open).
pub fn is_access_control_enabled(env: Env) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&DataKeyCore::AccessControlEnabled)
        .unwrap_or(false)
}

/// Adds `user` to the allowlist and clears any stale denylist entry (admin only).
///
/// Removing an address from the denylist when it is allowlisted guarantees the
/// two markers never conflict (denylist would otherwise silently win).
pub fn add_allowlisted(env: Env, user: Address) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("allow_add"), e);
    })?;

    let allow_key = DataKeyScoped::Allowlisted(user.clone());
    env.storage().persistent().set(&allow_key, &true);
    _extend_persistent_ttl(&env, &allow_key);

    // Clear a conflicting denylist marker so the allowlist actually takes effect.
    let deny_key = DataKeyScoped::Denylisted(user.clone());
    let was_denied = env.storage().persistent().has(&deny_key);
    if was_denied {
        env.storage().persistent().remove(&deny_key);
    }

    _emit_list_changed(&env, "allow", "add", &user, was_denied);
    Ok(())
}

/// Removes `user` from the allowlist (admin only). Idempotent — removing an
/// address that is not allowlisted is a no-op that still succeeds, so operator
/// scripts can reconcile lists without error handling.
pub fn remove_allowlisted(env: Env, user: Address) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("allow_rm"), e);
    })?;

    let allow_key = DataKeyScoped::Allowlisted(user.clone());
    if !env.storage().persistent().has(&allow_key) {
        return Ok(());
    }
    env.storage().persistent().remove(&allow_key);

    _emit_list_changed(&env, "allow", "rm", &user, false);
    Ok(())
}

/// Adds `user` to the denylist and clears any stale allowlist entry (admin only).
///
/// The denylist wins over the allowlist, so an allowlisted user who is later
/// denied is blocked. The conflicting allowlist marker is removed for clarity.
pub fn add_denylisted(env: Env, user: Address) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("deny_add"), e);
    })?;

    let deny_key = DataKeyScoped::Denylisted(user.clone());
    env.storage().persistent().set(&deny_key, &true);
    _extend_persistent_ttl(&env, &deny_key);

    // Clear a conflicting allowlist marker for a consistent, singular policy state.
    let allow_key = DataKeyScoped::Allowlisted(user.clone());
    let was_allowed = env.storage().persistent().has(&allow_key);
    if was_allowed {
        env.storage().persistent().remove(&allow_key);
    }

    _emit_list_changed(&env, "deny", "add", &user, was_allowed);
    Ok(())
}

/// Removes `user` from the denylist (admin only). Idempotent — removing an
/// address that is not denylisted is a no-op that still succeeds.
pub fn remove_denylisted(env: Env, user: Address) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_action_rejected(&env, &admin, symbol_short!("deny_rm"), e);
    })?;

    let deny_key = DataKeyScoped::Denylisted(user.clone());
    if !env.storage().persistent().has(&deny_key) {
        return Ok(());
    }
    env.storage().persistent().remove(&deny_key);

    _emit_list_changed(&env, "deny", "rm", &user, false);
    Ok(())
}

/// Returns whether `user` is allowlisted (read-only).
pub fn is_allowlisted(env: Env, user: Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKeyScoped::Allowlisted(user))
}

/// Returns whether `user` is denylisted (read-only).
pub fn is_denylisted(env: Env, user: Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKeyScoped::Denylisted(user))
}

/// Returns the resolved access state for `user` (read-only).
///
/// Denylist takes precedence over allowlist. An address that is neither marked
/// resolves to `Open`, regardless of whether allowlist mode is enabled.
pub fn get_access_state(env: Env, user: Address) -> AccessState {
    if env.storage().persistent().has(&DataKeyScoped::Denylisted(user.clone())) {
        AccessState::Denylisted
    } else if env.storage().persistent().has(&DataKeyScoped::Allowlisted(user)) {
        AccessState::Allowlisted
    } else {
        AccessState::Open
    }
}

/// Returns a human-facing policy summary: (allowlist enabled, user state).
pub fn get_access_policy(env: Env, user: Address) -> (bool, AccessState) {
    let enabled = is_access_control_enabled(env.clone());
    (enabled, get_access_state(env, user))
}

/// The single admission gate called by betting entrypoints (Issue #274).
///
/// Rules (applied in this order):
/// 1. A denylisted `user` is always rejected — this is an emergency block and
///    must win even on open deployments.
/// 2. If allowlist mode is enabled, an `user` that is not allowlisted is rejected.
/// 3. Otherwise, the call proceeds (default open).
///
/// Both rejection paths return the stable, dedicated
/// [`ContractError::AccessDenied`] error (code `79`).
pub fn _enforce_access_control(env: &Env, user: &Address) -> Result<(), ContractError> {
    if env
        .storage()
        .persistent()
        .has(&DataKeyScoped::Denylisted(user.clone()))
    {
        return Err(ContractError::AccessDenied);
    }
    let enabled: bool = env
        .storage()
        .persistent()
        .get(&DataKeyCore::AccessControlEnabled)
        .unwrap_or(false);
    if enabled
        && !env
            .storage()
            .persistent()
            .has(&DataKeyScoped::Allowlisted(user.clone()))
    {
        return Err(ContractError::AccessDenied);
    }
    Ok(())
}

/// Emits a list-mutation event under the `("access", <detail>)` topic.
/// `reconciled` reports whether the write additionally cleared a conflicting
/// marker on the other list (so indexers can detect a policy flip).
fn _emit_list_changed(env: &Env, list: &str, action: &str, user: &Address, reconciled: bool) {
    let detail: soroban_sdk::Symbol = match (list, action) {
        ("allow", "add") => symbol_short!("allow_add"),
        ("allow", "rm") => symbol_short!("allow_rm"),
        ("deny", "add") => symbol_short!("deny_add"),
        ("deny", "rm") => symbol_short!("deny_rm"),
        _ => symbol_short!("list_chg"),
    };
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("access"), detail),
        (user.clone(), reconciled),
    );
}
