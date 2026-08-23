// SPDX-License-Identifier: MIT
//! Dual-Approval Governance Mechanism for Critical Administrative Actions (Issue #272).

use crate::admin::{_require_supported_schema, _set_mode};
use crate::common::{_emit_action_rejected, _extend_persistent_ttl, DEFAULT_GOV_PROPOSAL_TTL_LEDGERS};
use crate::errors::ContractError;
use crate::types::{DataKeyCore, DataKeyScoped, GovAction, GovProposal, GovProposalStatus, RuntimeMode};
use soroban_sdk::{symbol_short, Address, Env};

/// Returns whether `user` is an authorized governance administrator or approver.
pub fn _is_authorized_gov_user(env: &Env, user: &Address) -> bool {
    let admin: Option<Address> = env.storage().persistent().get(&DataKeyCore::Admin);
    if let Some(ref a) = admin {
        if a == user {
            return true;
        }
    }
    let approver: Option<Address> = env.storage().persistent().get(&DataKeyCore::GovApprover);
    if let Some(ref ap) = approver {
        if ap == user {
            return true;
        }
    }
    false
}

/// Returns whether dual governance approval is currently active (a secondary approver is set).
pub fn _is_gov_approver_set(env: &Env) -> bool {
    let key = DataKeyCore::GovApprover;
    if env.storage().persistent().has(&key) {
        _extend_persistent_ttl(env, &key);
        true
    } else {
        false
    }
}

/// Configures the secondary governance approver (admin only).
pub fn set_gov_approver(env: Env, approver: Address) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();

    let key = DataKeyCore::GovApprover;
    env.storage().persistent().set(&key, &approver);
    _extend_persistent_ttl(&env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("gov"), symbol_short!("appr_set")),
        (admin, approver),
    );

    Ok(())
}

/// Returns the configured secondary governance approver address, if set.
pub fn get_gov_approver(env: Env) -> Option<Address> {
    let key = DataKeyCore::GovApprover;
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Sets the default proposal TTL in ledgers (admin only).
pub fn set_gov_proposal_ttl(env: Env, ttl_ledgers: u32) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();

    if ttl_ledgers == 0 {
        return Err(ContractError::WindowOutOfRange);
    }

    let key = DataKeyCore::GovProposalTtlLedgers;
    env.storage().persistent().set(&key, &ttl_ledgers);
    _extend_persistent_ttl(&env, &key);
    Ok(())
}

/// Returns the configured default proposal TTL in ledgers.
pub fn get_gov_proposal_ttl(env: Env) -> u32 {
    let key = DataKeyCore::GovProposalTtlLedgers;
    _extend_persistent_ttl(&env, &key);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(DEFAULT_GOV_PROPOSAL_TTL_LEDGERS)
}

/// Helper mapping GovAction to a numeric code for event payloads
fn _action_code(action: &GovAction) -> u32 {
    match action {
        GovAction::PauseProtocol => 0,
        GovAction::UnpauseProtocol => 1,
        GovAction::SetProtocolFeeBps(_) => 2,
        GovAction::WithdrawProtocolFee(_, _) => 3,
        GovAction::SetTreasuryAddress(_) => 4,
        GovAction::SetAdmin(_) => 5,
        GovAction::SetOracle(_) => 6,
    }
}

/// Proposes a protected administrative action (governance admin/approver only).
pub fn propose(
    env: Env,
    proposer: Address,
    action: GovAction,
    custom_ttl: Option<u32>,
) -> Result<u64, ContractError> {
    _require_supported_schema(&env)?;
    proposer.require_auth();

    if !_is_authorized_gov_user(&env, &proposer) {
        _emit_action_rejected(
            &env,
            &proposer,
            symbol_short!("propose"),
            ContractError::GovUnauthorized,
        );
        return Err(ContractError::GovUnauthorized);
    }

    let id_key = DataKeyCore::NextGovProposalId;
    let proposal_id: u64 = env.storage().persistent().get(&id_key).unwrap_or(1);
    env.storage().persistent().set(&id_key, &(proposal_id + 1));
    _extend_persistent_ttl(&env, &id_key);

    let ttl = custom_ttl.unwrap_or_else(|| get_gov_proposal_ttl(env.clone()));
    let created_at_ledger = env.ledger().sequence();
    let expires_at_ledger = created_at_ledger.saturating_add(ttl);

    let proposal = GovProposal {
        id: proposal_id,
        proposer: proposer.clone(),
        approver: None,
        action: action.clone(),
        created_at_ledger,
        expires_at_ledger,
        status: GovProposalStatus::Pending,
    };

    let p_key = DataKeyScoped::GovProposal(proposal_id);
    env.storage().persistent().set(&p_key, &proposal);
    _extend_persistent_ttl(&env, &p_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("gov"), symbol_short!("proposed")),
        (proposal_id, proposer, _action_code(&action), expires_at_ledger),
    );

    Ok(proposal_id)
}

/// Approves a pending governance proposal (must be the other authorized party).
pub fn approve(env: Env, approver: Address, proposal_id: u64) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    approver.require_auth();

    if !_is_authorized_gov_user(&env, &approver) {
        _emit_action_rejected(
            &env,
            &approver,
            symbol_short!("approve"),
            ContractError::GovUnauthorized,
        );
        return Err(ContractError::GovUnauthorized);
    }

    let p_key = DataKeyScoped::GovProposal(proposal_id);
    let mut proposal: GovProposal = env
        .storage()
        .persistent()
        .get(&p_key)
        .ok_or(ContractError::GovProposalNotFound)?;

    let current_ledger = env.ledger().sequence();
    if current_ledger > proposal.expires_at_ledger {
        proposal.status = GovProposalStatus::Expired;
        env.storage().persistent().set(&p_key, &proposal);
        _extend_persistent_ttl(&env, &p_key);
        _emit_action_rejected(
            &env,
            &approver,
            symbol_short!("approve"),
            ContractError::GovProposalExpired,
        );
        return Err(ContractError::GovProposalExpired);
    }

    if proposal.status != GovProposalStatus::Pending {
        _emit_action_rejected(
            &env,
            &approver,
            symbol_short!("approve"),
            ContractError::GovInvalidState,
        );
        return Err(ContractError::GovInvalidState);
    }

    // Dual approval: Proposer cannot approve their own proposal if dual approval is active.
    let dual_active = _is_gov_approver_set(&env);
    if dual_active && approver == proposal.proposer {
        _emit_action_rejected(
            &env,
            &approver,
            symbol_short!("approve"),
            ContractError::GovSelfApprovalDenied,
        );
        return Err(ContractError::GovSelfApprovalDenied);
    }

    proposal.approver = Some(approver.clone());
    proposal.status = GovProposalStatus::Approved;
    env.storage().persistent().set(&p_key, &proposal);
    _extend_persistent_ttl(&env, &p_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("gov"), symbol_short!("approved")),
        (proposal_id, approver),
    );

    Ok(())
}

/// Executes an approved governance proposal (admin/approver only).
pub fn execute_proposal(env: Env, caller: Address, proposal_id: u64) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    caller.require_auth();

    if !_is_authorized_gov_user(&env, &caller) {
        _emit_action_rejected(
            &env,
            &caller,
            symbol_short!("exec_prop"),
            ContractError::GovUnauthorized,
        );
        return Err(ContractError::GovUnauthorized);
    }

    let p_key = DataKeyScoped::GovProposal(proposal_id);
    let mut proposal: GovProposal = env
        .storage()
        .persistent()
        .get(&p_key)
        .ok_or(ContractError::GovProposalNotFound)?;

    let current_ledger = env.ledger().sequence();
    if current_ledger > proposal.expires_at_ledger {
        proposal.status = GovProposalStatus::Expired;
        env.storage().persistent().set(&p_key, &proposal);
        _extend_persistent_ttl(&env, &p_key);
        _emit_action_rejected(
            &env,
            &caller,
            symbol_short!("exec_prop"),
            ContractError::GovProposalExpired,
        );
        return Err(ContractError::GovProposalExpired);
    }

    // Require Approved state (or Pending if dual governance is NOT active)
    let dual_active = _is_gov_approver_set(&env);
    if dual_active && proposal.status != GovProposalStatus::Approved {
        _emit_action_rejected(
            &env,
            &caller,
            symbol_short!("exec_prop"),
            ContractError::GovInvalidState,
        );
        return Err(ContractError::GovInvalidState);
    }
    if !dual_active && proposal.status != GovProposalStatus::Pending && proposal.status != GovProposalStatus::Approved {
        _emit_action_rejected(
            &env,
            &caller,
            symbol_short!("exec_prop"),
            ContractError::GovInvalidState,
        );
        return Err(ContractError::GovInvalidState);
    }

    // Apply the action
    match proposal.action {
        GovAction::PauseProtocol => {
            _set_mode(&env, RuntimeMode::FullyPaused)?;
        }
        GovAction::UnpauseProtocol => {
            _set_mode(&env, RuntimeMode::Normal)?;
        }
        GovAction::SetProtocolFeeBps(bps) => {
            let key = DataKeyCore::ProtocolFeeBps;
            if let Some(ref v) = bps {
                env.storage().persistent().set(&key, v);
                _extend_persistent_ttl(&env, &key);
            } else {
                env.storage().persistent().remove(&key);
            }
        }
        GovAction::WithdrawProtocolFee(ref recipient, amount) => {
            _execute_withdraw_fee(&env, recipient, amount)?;
        }
        GovAction::SetTreasuryAddress(ref treasury) => {
            env.storage().persistent().set(&DataKeyCore::ProtocolFeeTreasury, treasury);
            _extend_persistent_ttl(&env, &DataKeyCore::ProtocolFeeTreasury);
        }
        GovAction::SetAdmin(ref new_admin) => {
            env.storage().persistent().set(&DataKeyCore::Admin, new_admin);
            _extend_persistent_ttl(&env, &DataKeyCore::Admin);
        }
        GovAction::SetOracle(ref new_oracle) => {
            env.storage().persistent().set(&DataKeyCore::Oracle, new_oracle);
            _extend_persistent_ttl(&env, &DataKeyCore::Oracle);
        }
    }

    proposal.status = GovProposalStatus::Executed;
    env.storage().persistent().set(&p_key, &proposal);
    _extend_persistent_ttl(&env, &p_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("gov"), symbol_short!("executed")),
        (proposal_id, caller),
    );

    Ok(())
}

fn _execute_withdraw_fee(env: &Env, recipient: &Address, amount: i128) -> Result<(), ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }
    let treasury_key = DataKeyCore::ProtocolFeeTreasury;
    let balance: i128 = env.storage().persistent().get(&treasury_key).unwrap_or(0);
    if amount > balance {
        return Err(ContractError::InsufficientBalance);
    }
    let new_balance = balance.checked_sub(amount).ok_or(ContractError::Overflow)?;
    env.storage().persistent().set(&treasury_key, &new_balance);
    _extend_persistent_ttl(env, &treasury_key);

    // Credit user's in-contract balance
    let current_user_bal: i128 = env
        .storage()
        .persistent()
        .get(&DataKeyScoped::Balance(recipient.clone()))
        .unwrap_or(0);
    let new_user_bal = current_user_bal.checked_add(amount).ok_or(ContractError::Overflow)?;
    env.storage()
        .persistent()
        .set(&DataKeyScoped::Balance(recipient.clone()), &new_user_bal);
    _extend_persistent_ttl(env, &DataKeyScoped::Balance(recipient.clone()));

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("fee"), symbol_short!("withdrawn")),
        (recipient.clone(), amount, new_balance),
    );

    Ok(())
}

/// Cancels a pending or approved governance proposal (proposer or admin only).
pub fn cancel_proposal(env: Env, caller: Address, proposal_id: u64) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    caller.require_auth();

    if !_is_authorized_gov_user(&env, &caller) {
        _emit_action_rejected(
            &env,
            &caller,
            symbol_short!("cancel_p"),
            ContractError::GovUnauthorized,
        );
        return Err(ContractError::GovUnauthorized);
    }

    let p_key = DataKeyScoped::GovProposal(proposal_id);
    let mut proposal: GovProposal = env
        .storage()
        .persistent()
        .get(&p_key)
        .ok_or(ContractError::GovProposalNotFound)?;

    if proposal.status != GovProposalStatus::Pending && proposal.status != GovProposalStatus::Approved {
        _emit_action_rejected(
            &env,
            &caller,
            symbol_short!("cancel_p"),
            ContractError::GovInvalidState,
        );
        return Err(ContractError::GovInvalidState);
    }

    proposal.status = GovProposalStatus::Cancelled;
    env.storage().persistent().set(&p_key, &proposal);
    _extend_persistent_ttl(&env, &p_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("gov"), symbol_short!("cancelled")),
        (proposal_id, caller),
    );

    Ok(())
}

/// Queries the state of a governance proposal.
pub fn get_proposal(env: Env, proposal_id: u64) -> Option<GovProposal> {
    let p_key = DataKeyScoped::GovProposal(proposal_id);
    _extend_persistent_ttl(&env, &p_key);
    let mut proposal: GovProposal = env.storage().persistent().get(&p_key)?;

    let current_ledger = env.ledger().sequence();
    if (proposal.status == GovProposalStatus::Pending || proposal.status == GovProposalStatus::Approved)
        && current_ledger > proposal.expires_at_ledger
    {
        proposal.status = GovProposalStatus::Expired;
    }

    Some(proposal)
}

pub use cancel_proposal as cancel;
pub use execute_proposal as execute;
pub use get_proposal as get_gov_proposal;
