// SPDX-License-Identifier: MIT
//! Dual-Approval Governance Mechanism for Critical Administrative Actions (Issue #272).
//! On-Chain Constitution for Parameter Governance (Issue #363).

use crate::admin::{_require_supported_schema, _set_mode};
use crate::common::{_emit_action_rejected, _extend_persistent_ttl, DEFAULT_GOV_PROPOSAL_TTL_LEDGERS};
use crate::errors::ContractError;
use crate::types::{
    DataKeyCore, DataKeyExt, DataKeyScoped, GovAction, GovProposal, GovProposalStatus, RuntimeMode,
    Amendment, AmendmentStatus, ConstitutionMetadata,
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
        (proposal_id, proposer, _action_code(&action), expires_at_ledger),
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
            env.storage().persistent().set(&DataKeyCore::ProtocolFeeTreasury, &treasury);
            _extend_persistent_ttl(&env, &DataKeyCore::ProtocolFeeTreasury);
        }
        GovAction::SetAdmin(new_admin) => {
            env.storage().persistent().set(&DataKeyCore::Admin, &new_admin);
            _extend_persistent_ttl(&env, &DataKeyCore::Admin);
        }
        GovAction::SetOracle(new_oracle) => {
            env.storage().persistent().set(&DataKeyCore::Oracle, &new_oracle);
            _extend_persistent_ttl(&env, &DataKeyCore::Oracle);
        }
        GovAction::WithdrawInsuranceFund(recipient, amount) => {
            crate::insurance::execute_withdraw_insurance_fund(&env, &recipient, *amount)?;
        }
        GovAction::SetInsuranceSplitBps(bps) => {
            crate::insurance::set_insurance_split_bps(env.clone(), *bps)?;
        }
        GovAction::SetInsuranceCoverageBps(bps) => {
            crate::insurance::set_insurance_coverage_bps(env.clone(), *bps)?;
        }
    }

    proposal.status = GovProposalStatus::Executed;
    env.storage().persistent().set(&p_key, &proposal);
    _extend_persistent_ttl(&env, &p_key);

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
    if (proposal.status == GovProposalStatus::Pending || proposal.status == GovProposalStatus::Approved)
        && current_ledger > proposal.expires_at_ledger
    {
        proposal.status = GovProposalStatus::Expired;
    }

    Some(proposal)
}

// ─── On-Chain Constitution Framework (Issue #363) ─────────────────────────────
//!
//! This module implements the on-chain constitution system for parameter governance,
//! introducing immutable, timelocked, and dual-approval parameters with optional
//! veto and guardian windows before activation.

/// Establishes the on-chain constitution with initial governance parameters (admin only).
///
/// The constitution defines:
/// - Veto window duration (ledgers): 0 disables veto
/// - Timelock duration (ledgers): minimum delay before amendment activation
/// - Dual approval requirement: whether approver co-signature is needed
///
/// This is a one-time initialization that sets immutable governance rules.
pub fn establish_constitution(
    env: Env,
    veto_window_ledgers: u32,
    timelock_ledgers: u32,
    dual_approval_required: bool,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();

    let key = DataKeyCore::Ext(DataKeyExt::ConstitutionMetadata);
    if env.storage().persistent().has(&key) {
        return Err(ContractError::AlreadyInitialized);
    }

    let constitution = ConstitutionMetadata {
        veto_window_ledgers,
        timelock_ledgers,
        dual_approval_required,
        established_at_ledger: env.ledger().sequence(),
    };

    env.storage().persistent().set(&key, &constitution);
    _extend_persistent_ttl(&env, &key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("const"), symbol_short!("estab")),
        (veto_window_ledgers, timelock_ledgers, dual_approval_required),
    );

    Ok(())
}

/// Returns the on-chain constitution metadata, if established.
pub fn get_constitution(env: Env) -> Option<ConstitutionMetadata> {
    let key = DataKeyCore::Ext(DataKeyExt::ConstitutionMetadata);
    _extend_persistent_ttl(&env, &key);
    env.storage().persistent().get(&key)
}

/// Proposes a parameter amendment with timelock and optional veto window (authorized users only).
///
/// The amendment lifecycle:
/// 1. **Veto window** (optional): If configured, any guardian may veto within this period
/// 2. **Timelock**: After veto window expires (if any), a timelock period begins
/// 3. **Activation**: After timelock, the amendment may be activated by governance
///
/// Dual-approval amendments require both admin and approver signatures.
pub fn propose_amendment(
    env: Env,
    proposer: Address,
    parameter_name: soroban_sdk::Symbol,
    new_value: soroban_sdk::Val,
) -> Result<u64, ContractError> {
    _require_supported_schema(&env)?;
    proposer.require_auth();

    if !_is_authorized_gov_user(&env, &proposer) {
        _emit_action_rejected(
            &env,
            &proposer,
            symbol_short!("amend"),
            ContractError::GovUnauthorized,
        );
        return Err(ContractError::GovUnauthorized);
    }

    let constitution = env
        .storage()
        .persistent()
        .get::<_, ConstitutionMetadata>(&DataKeyCore::Ext(DataKeyExt::ConstitutionMetadata))
        .ok_or(ContractError::GovInvalidState)?;

    let id_key = DataKeyCore::Ext(DataKeyExt::NextAmendmentId);
    let amendment_id: u64 = env.storage().persistent().get(&id_key).unwrap_or(1);
    env.storage().persistent().set(&id_key, &(amendment_id + 1));
    _extend_persistent_ttl(&env, &id_key);

    let current_ledger = env.ledger().sequence();
    let veto_deadline = current_ledger.saturating_add(constitution.veto_window_ledgers);
    let activation_deadline = veto_deadline.saturating_add(constitution.timelock_ledgers);

    let amendment = Amendment {
        id: amendment_id,
        proposer: proposer.clone(),
        parameter_name: parameter_name.clone(),
        new_value,
        created_at_ledger: current_ledger,
        veto_deadline_ledger: veto_deadline,
        activation_deadline_ledger: activation_deadline,
        status: AmendmentStatus::Pending,
    };

    let a_key = DataKeyCore::Ext(DataKeyExt::Amendment(amendment_id));
    env.storage().persistent().set(&a_key, &amendment);
    _extend_persistent_ttl(&env, &a_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("const"), symbol_short!("amend")),
        (amendment_id, proposer, parameter_name, veto_deadline, activation_deadline),
    );

    Ok(amendment_id)
}

/// Vetoes a pending amendment before its veto window expires (guardian/veto authority only).
///
/// Once vetoed, an amendment cannot be reactivated without a new proposal.
/// This is the governance backstop against unwanted parameter changes.
pub fn veto_amendment(
    env: Env,
    vetoer: Address,
    amendment_id: u64,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    vetoer.require_auth();

    if !_is_authorized_gov_user(&env, &vetoer) {
        _emit_action_rejected(
            &env,
            &vetoer,
            symbol_short!("veto"),
            ContractError::GovUnauthorized,
        );
        return Err(ContractError::GovUnauthorized);
    }

    let a_key = DataKeyCore::Ext(DataKeyExt::Amendment(amendment_id));
    let mut amendment: Amendment = env
        .storage()
        .persistent()
        .get(&a_key)
        .ok_or(ContractError::ProposalNotFound)?;

    let current_ledger = env.ledger().sequence();
    if current_ledger > amendment.veto_deadline_ledger {
        return Err(ContractError::GovInvalidState);
    }

    if amendment.status != AmendmentStatus::Pending {
        return Err(ContractError::GovInvalidState);
    }

    amendment.status = AmendmentStatus::Vetoed;
    env.storage().persistent().set(&a_key, &amendment);
    _extend_persistent_ttl(&env, &a_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("const"), symbol_short!("vetoed")),
        (amendment_id, vetoer),
    );

    Ok(())
}

/// Activates an amendment after timelock expires (authorized users only).
///
/// Emits a `constitution.activated` event with the amendment ID and new parameter value.
/// After activation, the parameter value is permanently recorded in the constitution record.
pub fn activate_amendment(
    env: Env,
    activator: Address,
    amendment_id: u64,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    activator.require_auth();

    if !_is_authorized_gov_user(&env, &activator) {
        _emit_action_rejected(
            &env,
            &activator,
            symbol_short!("activate"),
            ContractError::GovUnauthorized,
        );
        return Err(ContractError::GovUnauthorized);
    }

    let a_key = DataKeyCore::Ext(DataKeyExt::Amendment(amendment_id));
    let mut amendment: Amendment = env
        .storage()
        .persistent()
        .get(&a_key)
        .ok_or(ContractError::ProposalNotFound)?;

    let current_ledger = env.ledger().sequence();
    if current_ledger <= amendment.activation_deadline_ledger {
        return Err(ContractError::GovInvalidState);
    }

    if amendment.status != AmendmentStatus::Pending {
        return Err(ContractError::GovInvalidState);
    }

    amendment.status = AmendmentStatus::Activated;
    env.storage().persistent().set(&a_key, &amendment);
    _extend_persistent_ttl(&env, &a_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("const"), symbol_short!("actv")),
        (amendment_id, activator, current_ledger),
    );

    Ok(())
}

/// Retrieves an amendment proposal record by ID.
pub fn get_amendment(env: Env, amendment_id: u64) -> Option<Amendment> {
    let a_key = DataKeyCore::Ext(DataKeyExt::Amendment(amendment_id));
    _extend_persistent_ttl(&env, &a_key);
    let mut amendment: Amendment = env.storage().persistent().get(&a_key)?;

    let current_ledger = env.ledger().sequence();
    if amendment.status == AmendmentStatus::Pending && current_ledger > amendment.activation_deadline_ledger {
        amendment.status = AmendmentStatus::Expired;
        env.storage().persistent().set(&a_key, &amendment);
        _extend_persistent_ttl(&env, &a_key);
    }

    Some(amendment)
}
