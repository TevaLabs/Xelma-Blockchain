// SPDX-License-Identifier: MIT
//! Comprehensive test suite for Dual-Approval Governance Mechanism (Issue #272).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{DataKeyCore, GovAction, GovProposalStatus};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env,
};

fn setup_governance_env() -> (
    Env,
    VirtualTokenContractClient<'static>,
    Address, // Admin / Proposer
    Address, // Secondary Approver
    Address, // Oracle
    Address, // Third Party User
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let approver = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &oracle);
    client.set_gov_approver(&approver);

    (env, client, admin, approver, oracle, user)
}

#[test]
fn test_successful_proposal_lifecycle_pause_unpause() {
    let (env, client, admin, approver, _oracle, _user) = setup_governance_env();

    assert!(!client.is_paused());

    // 1. Propose pause
    let proposal_id = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &Some(50));
    assert_eq!(proposal_id, 1);

    let prop1 = client.get_gov_proposal(&1).expect("proposal must exist");
    assert_eq!(prop1.proposer, admin);
    assert_eq!(prop1.approver, None);
    assert_eq!(prop1.action, GovAction::PauseProtocol);
    assert_eq!(prop1.status, GovProposalStatus::Pending);
    assert_eq!(prop1.expires_at_ledger, env.ledger().sequence() + 50);

    // 2. Approve pause (by secondary approver)
    client.approve_gov_proposal(&approver, &1);

    let prop2 = client.get_gov_proposal(&1).expect("proposal must exist");
    assert_eq!(prop2.approver, Some(approver.clone()));
    assert_eq!(prop2.status, GovProposalStatus::Approved);

    // 3. Execute pause
    client.execute_gov_proposal(&admin, &1);

    let prop3 = client.get_gov_proposal(&1).expect("proposal must exist");
    assert_eq!(prop3.status, GovProposalStatus::Executed);
    assert!(client.is_paused());

    // 4. Propose unpause
    let proposal2_id = client.propose_gov_action(&approver, &GovAction::UnpauseProtocol, &None);
    assert_eq!(proposal2_id, 2);

    client.approve_gov_proposal(&admin, &2);
    client.execute_gov_proposal(&approver, &2);

    assert!(!client.is_paused());
}

#[test]
fn test_proposer_cannot_approve_own_proposal() {
    let (_env, client, admin, _approver, _oracle, _user) = setup_governance_env();

    let pid = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &None);

    let res = client.try_approve_gov_proposal(&admin, &pid);
    assert_eq!(res, Err(Ok(ContractError::GovInvalidState)));
}

#[test]
fn test_execution_before_approval_fails() {
    let (_env, client, admin, _approver, _oracle, _user) = setup_governance_env();

    let pid = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &None);

    let res = client.try_execute_gov_proposal(&admin, &pid);
    assert_eq!(res, Err(Ok(ContractError::GovInvalidState)));
}

#[test]
fn test_duplicate_approval_fails() {
    let (_env, client, admin, approver, _oracle, _user) = setup_governance_env();

    let pid = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &None);

    client.approve_gov_proposal(&approver, &pid);

    let res = client.try_approve_gov_proposal(&approver, &pid);
    assert_eq!(res, Err(Ok(ContractError::GovInvalidState)));
}

#[test]
fn test_proposal_expiration_handling() {
    let (env, client, admin, approver, _oracle, _user) = setup_governance_env();

    let pid = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &Some(10));
    let initial_ledger = env.ledger().sequence();

    // Advance past expiration ledger
    env.ledger().with_mut(|li| {
        li.sequence_number = initial_ledger + 15;
    });

    // Approval after expiry should fail
    let app_res = client.try_approve_gov_proposal(&approver, &pid);
    assert_eq!(app_res, Err(Ok(ContractError::ProposalExpired)));

    // Query proposal shows Expired status
    let prop = client.get_gov_proposal(&pid).expect("proposal must exist");
    assert_eq!(prop.status, GovProposalStatus::Expired);

    // Execution after expiry should fail
    let exec_res = client.try_execute_gov_proposal(&approver, &pid);
    assert_eq!(exec_res, Err(Ok(ContractError::ProposalExpired)));
}

#[test]
fn test_proposal_cancellation_lifecycle() {
    let (_env, client, admin, approver, _oracle, _user) = setup_governance_env();

    let pid = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &None);

    // Cancel proposal
    client.cancel_gov_proposal(&admin, &pid);

    let prop = client.get_gov_proposal(&pid).expect("proposal must exist");
    assert_eq!(prop.status, GovProposalStatus::Cancelled);

    // Attempt approve cancelled
    let app_res = client.try_approve_gov_proposal(&approver, &pid);
    assert_eq!(app_res, Err(Ok(ContractError::GovInvalidState)));

    // Attempt execute cancelled
    let exec_res = client.try_execute_gov_proposal(&admin, &pid);
    assert_eq!(exec_res, Err(Ok(ContractError::GovInvalidState)));
}

#[test]
fn test_unauthorized_proposer_and_approver() {
    let (_env, client, admin, approver, _oracle, attacker) = setup_governance_env();

    // Attacker proposes -> fails
    let prop_res = client.try_propose_gov_action(&attacker, &GovAction::PauseProtocol, &None);
    assert_eq!(prop_res, Err(Ok(ContractError::GovUnauthorized)));

    // Authorized proposal
    let pid = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &None);

    // Attacker approves -> fails
    let app_res = client.try_approve_gov_proposal(&attacker, &pid);
    assert_eq!(app_res, Err(Ok(ContractError::GovUnauthorized)));

    // Attacker executes -> fails
    client.approve_gov_proposal(&approver, &pid);
    let exec_res = client.try_execute_gov_proposal(&attacker, &pid);
    assert_eq!(exec_res, Err(Ok(ContractError::GovUnauthorized)));
}

#[test]
fn test_protected_action_fee_update_and_withdrawal() {
    let (_env, client, admin, approver, _oracle, recipient) = setup_governance_env();

    // 1. Propose & execute fee update
    let pid1 = client.propose_gov_action(&admin, &GovAction::SetProtocolFeeBps(Some(500)), &None);
    client.approve_gov_proposal(&approver, &pid1);
    client.execute_gov_proposal(&admin, &pid1);

    assert_eq!(client.get_protocol_fee_bps(), Some(500));

    // Direct single-key withdraw should fail when approver set
    let direct_withdraw = client.try_withdraw_protocol_fee(&recipient, &100);
    assert_eq!(direct_withdraw, Err(Ok(ContractError::GovUnauthorized)));

    // 2. Propose & execute fee withdrawal via governance
    let pid2 = client.propose_gov_action(
        &admin,
        &GovAction::WithdrawProtocolFee(recipient.clone(), 0), // 0 amount fails validation
        &None,
    );
    client.approve_gov_proposal(&approver, &pid2);
    let exec_fail = client.try_execute_gov_proposal(&admin, &pid2);
    assert_eq!(exec_fail, Err(Ok(ContractError::InvalidBetAmount)));
    assert_eq!(
        client.get_gov_proposal(&pid2).unwrap().status,
        GovProposalStatus::Approved,
        "a reverted action must not consume the approval"
    );
}

#[test]
fn test_protected_action_role_transfers() {
    let (_env, client, admin, approver, _oracle, _user) = setup_governance_env();

    let new_admin = Address::generate(&client.env);

    // Propose admin transfer
    let pid = client.propose_gov_action(&admin, &GovAction::SetAdmin(new_admin.clone()), &None);
    client.approve_gov_proposal(&approver, &pid);
    client.execute_gov_proposal(&admin, &pid);

    assert_eq!(client.get_admin(), Some(new_admin));
}

#[test]
fn test_audit_event_emission() {
    let (env, client, admin, approver, _oracle, _user) = setup_governance_env();

    let pid = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &None);
    client.approve_gov_proposal(&approver, &pid);
    client.execute_gov_proposal(&admin, &pid);

    let events = env.events().all();
    let gov_events: std::vec::Vec<_> = events
        .into_iter()
        .filter(|e| e.0 == client.address)
        .collect();

    assert!(gov_events.len() >= 3);
}

fn seed_protocol_fee_treasury(
    env: &Env,
    client: &VirtualTokenContractClient<'static>,
    amount: i128,
) {
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::ProtocolFeeTreasury, &amount);
    });
}

#[test]
fn test_governance_fee_withdraw_double_execute_impossible() {
    let (env, client, admin, approver, _oracle, recipient) = setup_governance_env();
    let amount = 1_000_0000000i128;
    seed_protocol_fee_treasury(&env, &client, 5_000_0000000);

    let proposal_id = client.propose_gov_action(
        &admin,
        &GovAction::WithdrawProtocolFee(recipient.clone(), amount),
        &None,
    );
    client.approve_gov_proposal(&approver, &proposal_id);
    client.execute_gov_proposal(&admin, &proposal_id);

    assert_eq!(client.get_protocol_fee_treasury(), 4_000_0000000);
    assert_eq!(client.balance(&recipient), amount);

    let second_execute = client.try_execute_gov_proposal(&approver, &proposal_id);
    assert_eq!(second_execute, Err(Ok(ContractError::GovInvalidState)));
    assert_eq!(client.get_protocol_fee_treasury(), 4_000_0000000);
    assert_eq!(client.balance(&recipient), amount);
    assert_eq!(
        client.get_gov_proposal(&proposal_id).unwrap().status,
        GovProposalStatus::Executed
    );
}

#[test]
fn test_cancel_after_approved_fee_withdraw_blocks_execution() {
    let (env, client, admin, approver, _oracle, recipient) = setup_governance_env();
    let amount = 750_0000000i128;
    seed_protocol_fee_treasury(&env, &client, 5_000_0000000);

    let proposal_id = client.propose_gov_action(
        &admin,
        &GovAction::WithdrawProtocolFee(recipient.clone(), amount),
        &None,
    );
    client.approve_gov_proposal(&approver, &proposal_id);
    client.cancel_gov_proposal(&admin, &proposal_id);

    let execute = client.try_execute_gov_proposal(&admin, &proposal_id);
    assert_eq!(execute, Err(Ok(ContractError::GovInvalidState)));
    let reapprove = client.try_approve_gov_proposal(&approver, &proposal_id);
    assert_eq!(reapprove, Err(Ok(ContractError::GovInvalidState)));
    assert_eq!(client.get_protocol_fee_treasury(), 5_000_0000000);
    assert_eq!(client.balance(&recipient), 0);
    assert_eq!(
        client.get_gov_proposal(&proposal_id).unwrap().status,
        GovProposalStatus::Cancelled
    );
}

#[test]
fn test_cancel_execute_race_execution_winner_is_terminal() {
    let (env, client, admin, approver, _oracle, recipient) = setup_governance_env();
    let amount = 250_0000000i128;
    seed_protocol_fee_treasury(&env, &client, 5_000_0000000);

    let proposal_id = client.propose_gov_action(
        &admin,
        &GovAction::WithdrawProtocolFee(recipient.clone(), amount),
        &None,
    );
    client.approve_gov_proposal(&approver, &proposal_id);
    client.execute_gov_proposal(&admin, &proposal_id);

    let cancel = client.try_cancel_gov_proposal(&approver, &proposal_id);
    assert_eq!(cancel, Err(Ok(ContractError::GovInvalidState)));
    assert_eq!(client.get_protocol_fee_treasury(), 4_750_0000000);
    assert_eq!(client.balance(&recipient), amount);
}

#[test]
fn test_expired_approved_fee_withdraw_is_rejected() {
    let (env, client, admin, approver, _oracle, recipient) = setup_governance_env();
    let amount = 300_0000000i128;
    seed_protocol_fee_treasury(&env, &client, 5_000_0000000);

    let proposal_id = client.propose_gov_action(
        &admin,
        &GovAction::WithdrawProtocolFee(recipient.clone(), amount),
        &Some(10),
    );
    client.approve_gov_proposal(&approver, &proposal_id);

    env.ledger().with_mut(|ledger| {
        ledger.sequence_number += 11;
    });

    let execute = client.try_execute_gov_proposal(&admin, &proposal_id);
    assert_eq!(execute, Err(Ok(ContractError::ProposalExpired)));
    assert_eq!(client.get_protocol_fee_treasury(), 5_000_0000000);
    assert_eq!(client.balance(&recipient), 0);
    assert_eq!(
        client.get_gov_proposal(&proposal_id).unwrap().status,
        GovProposalStatus::Expired
    );
}

#[test]
fn test_pause_rules_are_respected_by_governance_execute() {
    let (env, client, admin, approver, _oracle, recipient) = setup_governance_env();
    let amount = 100_0000000i128;
    seed_protocol_fee_treasury(&env, &client, 5_000_0000000);

    let pause_id = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &None);
    client.approve_gov_proposal(&approver, &pause_id);
    client.execute_gov_proposal(&admin, &pause_id);
    assert!(client.is_paused());

    let withdraw_id = client.propose_gov_action(
        &admin,
        &GovAction::WithdrawProtocolFee(recipient.clone(), amount),
        &None,
    );
    client.approve_gov_proposal(&approver, &withdraw_id);
    let execute_withdraw = client.try_execute_gov_proposal(&admin, &withdraw_id);
    assert_eq!(execute_withdraw, Err(Ok(ContractError::ContractPaused)));
    assert_eq!(client.get_protocol_fee_treasury(), 5_000_0000000);
    assert_eq!(client.balance(&recipient), 0);
    assert_eq!(
        client.get_gov_proposal(&withdraw_id).unwrap().status,
        GovProposalStatus::Approved,
        "a pause rejection must not consume the proposal"
    );

    let fee_update_id =
        client.propose_gov_action(&admin, &GovAction::SetProtocolFeeBps(Some(500)), &None);
    client.approve_gov_proposal(&approver, &fee_update_id);
    let execute_fee_update = client.try_execute_gov_proposal(&admin, &fee_update_id);
    assert_eq!(execute_fee_update, Err(Ok(ContractError::ContractPaused)));
    assert_eq!(client.get_protocol_fee_bps(), None);

    let unpause_id = client.propose_gov_action(&admin, &GovAction::UnpauseProtocol, &None);
    client.approve_gov_proposal(&approver, &unpause_id);
    client.execute_gov_proposal(&admin, &unpause_id);
    assert!(!client.is_paused());
}

#[test]
fn test_pause_action_remains_executable_while_paused() {
    let (_env, client, admin, approver, _oracle, _user) = setup_governance_env();

    let pause_id = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &None);
    client.approve_gov_proposal(&approver, &pause_id);
    client.execute_gov_proposal(&admin, &pause_id);

    let second_pause_id = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &None);
    client.approve_gov_proposal(&approver, &second_pause_id);
    client.execute_gov_proposal(&admin, &second_pause_id);

    assert!(client.is_paused());
}
