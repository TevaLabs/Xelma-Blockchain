// SPDX-License-Identifier: MIT
//! Comprehensive test suite for Dual-Approval Governance Mechanism (Issue #272).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{GovAction, GovProposalStatus};
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
    use soroban_sdk::TryIntoVal;

    let (env, client, admin, approver, _oracle, _user) = setup_governance_env();

    // Verify propose emits a gov.proposed event.
    let pid = client.propose_gov_action(&admin, &GovAction::PauseProtocol, &None);
    let propose_events = env.events().all();
    let has_proposed = propose_events.iter().any(|(_contract, topics, _data)| {
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env)
                == Ok(soroban_sdk::symbol_short!("gov"))
            && topics.get(1).unwrap().try_into_val(&env)
                == Ok(soroban_sdk::symbol_short!("proposed"))
    });
    assert!(has_proposed, "propose_gov_action must emit gov.proposed event");

    // Verify approve emits a gov.approved event.
    client.approve_gov_proposal(&approver, &pid);
    let approve_events = env.events().all();
    let has_approved = approve_events.iter().any(|(_contract, topics, _data)| {
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env)
                == Ok(soroban_sdk::symbol_short!("gov"))
            && topics.get(1).unwrap().try_into_val(&env)
                == Ok(soroban_sdk::symbol_short!("approved"))
    });
    assert!(has_approved, "approve_gov_proposal must emit gov.approved event");

    // Verify execute emits a gov.executed event (and optionally a mode.transition event).
    client.execute_gov_proposal(&admin, &pid);
    let execute_events = env.events().all();
    let has_executed = execute_events.iter().any(|(_contract, topics, _data)| {
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env)
                == Ok(soroban_sdk::symbol_short!("gov"))
            && topics.get(1).unwrap().try_into_val(&env)
                == Ok(soroban_sdk::symbol_short!("executed"))
    });
    assert!(has_executed, "execute_gov_proposal must emit gov.executed event");
    // At least the executed event is present; the mode-transition event may also be emitted.
    let gov_events_count = execute_events
        .iter()
        .filter(|(_contract, topics, _data)| {
            topics.len() == 2
                && topics
                    .get(0)
                    .unwrap()
                    .try_into_val(&env)
                    .ok()
                    == Some(soroban_sdk::symbol_short!("gov"))
        })
        .count();
    assert!(gov_events_count >= 1, "execute step must emit at least one gov event");
}
