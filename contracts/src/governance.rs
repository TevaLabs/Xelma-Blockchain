// SPDX-License-Identifier: MIT
//! Dual-Approval Governance Mechanism for Critical Administrative Actions (Issue #272).

use crate::admin::{_ensure_not_paused, _require_supported_schema, _set_mode};
use crate::common::{
    _emit_action_rejected, _extend_persistent_ttl, DEFAULT_GOV_PROPOSAL_TTL_LEDGERS,
};
use crate::errors::ContractError;
use crate::types::{
    DataKeyCore, DataKeyScoped, GovAction, GovProposal, GovProposalStatus, RuntimeMode,
};
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
        GovAction::WithdrawInsuranceFund(_, _) => 7,
        GovAction::SetInsuranceSplitBps(_) => 8,
        GovAction::SetInsuranceCoverageBps(_) => 9,
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
        (
            proposal_id,
            proposer,
            _action_code(&action),
            expires_at_ledger,
        ),
    );

    Ok(proposal_id)
}

/// Approves a pending governance proposal (governance admin/approver only, distinct from proposer).
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
        .ok_or(ContractError::ProposalNotFound)?;

    let current_ledger = env.ledger().sequence();
    if current_ledger > proposal.expires_at_ledger {
        proposal.status = GovProposalStatus::Expired;
        env.storage().persistent().set(&p_key, &proposal);
        _extend_persistent_ttl(&env, &p_key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("gov"), symbol_short!("expired")),
            (proposal_id, current_ledger),
        );
        return Err(ContractError::ProposalExpired);
    }

    match proposal.status {
        GovProposalStatus::Cancelled => return Err(ContractError::GovInvalidState),
        GovProposalStatus::Executed => return Err(ContractError::GovInvalidState),
        GovProposalStatus::Approved => return Err(ContractError::GovInvalidState),
        GovProposalStatus::Expired => return Err(ContractError::ProposalExpired),
        GovProposalStatus::Pending => {}
    }

    if proposal.proposer == approver {
        _emit_action_rejected(
            &env,
            &approver,
            symbol_short!("approve"),
            ContractError::GovInvalidState,
        );
        return Err(ContractError::GovInvalidState);
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

/// Executes an approved governance proposal (governance admin/approver only).
pub fn execute(env: Env, executor: Address, proposal_id: u64) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    executor.require_auth();

    if !_is_authorized_gov_user(&env, &executor) {
        _emit_action_rejected(
            &env,
            &executor,
            symbol_short!("execute"),
            ContractError::GovUnauthorized,
        );
        return Err(ContractError::GovUnauthorized);
    }

    let p_key = DataKeyScoped::GovProposal(proposal_id);
    let mut proposal: GovProposal = env
        .storage()
        .persistent()
        .get(&p_key)
        .ok_or(ContractError::ProposalNotFound)?;

    let current_ledger = env.ledger().sequence();
    if current_ledger > proposal.expires_at_ledger {
        proposal.status = GovProposalStatus::Expired;
        env.storage().persistent().set(&p_key, &proposal);
        _extend_persistent_ttl(&env, &p_key);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("gov"), symbol_short!("expired")),
            (proposal_id, current_ledger),
        );
        return Err(ContractError::ProposalExpired);
    }

    match proposal.status {
        GovProposalStatus::Cancelled => return Err(ContractError::GovInvalidState),
        GovProposalStatus::Executed => return Err(ContractError::GovInvalidState),
        GovProposalStatus::Expired => return Err(ContractError::ProposalExpired),
        GovProposalStatus::Pending => return Err(ContractError::GovInvalidState),
        GovProposalStatus::Approved => {}
    }

    // Pause rules must be respected on execute. Emergency pause/unpause must
    // remain callable while the protocol is fully paused, but every other
    // protected action is gated by the same policy that applies to the direct
    // admin entrypoints (AdminConfig is blocked by `FullyPaused`).
    if !matches!(
        proposal.action,
        GovAction::PauseProtocol | GovAction::UnpauseProtocol
    ) {
        _ensure_not_paused(&env).inspect_err(|&e| {
            _emit_action_rejected(&env, &executor, symbol_short!("execute"), e);
        })?;
    }

    // CEI: mark the proposal Executed before performing any external
    // interaction so a racing/reentrant caller can never double-execute (or
    // cancel) a proposal whose effects have begun. On a failed action Soroban
    // reverts this write atomically, so it cannot burn spent approvals.
    proposal.status = GovProposalStatus::Executed;
    env.storage().persistent().set(&p_key, &proposal);
    _extend_persistent_ttl(&env, &p_key);

    // Execute the action payload
    match &proposal.action {
        GovAction::PauseProtocol => {
            _set_mode(&env, RuntimeMode::FullyPaused)?;
        }
        GovAction::UnpauseProtocol => {
            _set_mode(&env, RuntimeMode::Normal)?;
        }
        GovAction::SetProtocolFeeBps(bps) => {
            crate::config::_validate_protocol_fee_bps(bps.clone())?;
            let key = DataKeyCore::ProtocolFeeBps;
            if let Some(ref v) = bps {
                env.storage().persistent().set(&key, v);
                _extend_persistent_ttl(&env, &key);
            } else {
                env.storage().persistent().remove(&key);
            }
        }
        GovAction::WithdrawProtocolFee(recipient, amount) => {
            _execute_withdraw_fee(&env, &recipient, *amount)?;
        }
        GovAction::SetTreasuryAddress(treasury) => {
            env.storage()
                .persistent()
                .set(&DataKeyCore::ProtocolFeeTreasury, &treasury);
            _extend_persistent_ttl(&env, &DataKeyCore::ProtocolFeeTreasury);
        }
        GovAction::SetAdmin(new_admin) => {
            env.storage()
                .persistent()
                .set(&DataKeyCore::Admin, &new_admin);
            _extend_persistent_ttl(&env, &DataKeyCore::Admin);
        }
        GovAction::SetOracle(new_oracle) => {
            env.storage()
                .persistent()
                .set(&DataKeyCore::Oracle, &new_oracle);
            _extend_persistent_ttl(&env, &DataKeyCore::Oracle);
        }
        GovAction::WithdrawInsuranceFund(recipient, amount) => {
            crate::insurance::execute_withdraw_insurance_fund(env, &recipient, *amount)?;
        }
        GovAction::SetInsuranceSplitBps(bps) => {
            crate::insurance::set_insurance_split_bps(env.clone(), *bps)?;
        }
        GovAction::SetInsuranceCoverageBps(bps) => {
            crate::insurance::set_insurance_coverage_bps(env.clone(), *bps)?;
        }
    }

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("gov"), symbol_short!("executed")),
        (proposal_id, executor, _action_code(&proposal.action)),
    );

    Ok(())
}

/// Helper executing fee withdrawals for Governance Action
fn _execute_withdraw_fee(
    env: &Env,
    recipient: &Address,
    amount: i128,
) -> Result<i128, ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidBetAmount);
    }
    let treasury_key = DataKeyCore::ProtocolFeeTreasury;
    let current: i128 = env.storage().persistent().get(&treasury_key).unwrap_or(0);
    if amount > current {
        return Err(ContractError::InsufficientBalance);
    }
    let new_treasury = current
        .checked_sub(amount)
        .ok_or(ContractError::InsufficientBalance)?;
    env.storage().persistent().set(&treasury_key, &new_treasury);
    _extend_persistent_ttl(env, &treasury_key);

    let recipient_bal: i128 = crate::common::balance(env.clone(), recipient.clone());
    let new_bal = crate::common::payout_add(recipient_bal, amount)?;
    crate::common::_set_balance(env, recipient.clone(), new_bal);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("protocol"), symbol_short!("fee_with")),
        (recipient.clone(), amount, new_treasury),
    );

    Ok(amount)
}

/// Cancels an unexecuted governance proposal (governance admin/approver only).
pub fn cancel(env: Env, canceller: Address, proposal_id: u64) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    canceller.require_auth();

    if !_is_authorized_gov_user(&env, &canceller) {
        _emit_action_rejected(
            &env,
            &canceller,
            symbol_short!("cancel"),
            ContractError::GovUnauthorized,
        );
        return Err(ContractError::GovUnauthorized);
    }

    let p_key = DataKeyScoped::GovProposal(proposal_id);
    let mut proposal: GovProposal = env
        .storage()
        .persistent()
        .get(&p_key)
        .ok_or(ContractError::ProposalNotFound)?;

    match proposal.status {
        GovProposalStatus::Executed => return Err(ContractError::GovInvalidState),
        GovProposalStatus::Cancelled => return Err(ContractError::GovInvalidState),
        _ => {}
    }

    proposal.status = GovProposalStatus::Cancelled;
    env.storage().persistent().set(&p_key, &proposal);
    _extend_persistent_ttl(&env, &p_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("gov"), symbol_short!("cancel")),
        (proposal_id, canceller),
    );

    Ok(())
}

/// Queries details for a governance proposal.
pub fn get_gov_proposal(env: Env, proposal_id: u64) -> Option<GovProposal> {
    let p_key = DataKeyScoped::GovProposal(proposal_id);
    _extend_persistent_ttl(&env, &p_key);
    let mut proposal: GovProposal = env.storage().persistent().get(&p_key)?;

    let current_ledger = env.ledger().sequence();
    if (proposal.status == GovProposalStatus::Pending
        || proposal.status == GovProposalStatus::Approved)
        && current_ledger > proposal.expires_at_ledger
    {
        proposal.status = GovProposalStatus::Expired;
    }

    Some(proposal)
}
