//! Tests for two-step oracle rotation with expiry.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, Env, TryIntoVal,
};

fn init(env: &Env, client: &VirtualTokenContractClient) -> (Address, Address, Address) {
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    let new_oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    (admin, oracle, new_oracle)
}

fn has_event_with_topic(
    events: &soroban_sdk::Vec<(Address, soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val)>,
    env: &Env,
    topic: soroban_sdk::Symbol,
) -> bool {
    (0..events.len()).any(|i| {
        let event = events.get(i).unwrap();
        let topics = event.1;
        topics.len() == 2
            && topics.get(0).try_into_val(env) == Ok(symbol_short!("oracle"))
            && topics.get(1).try_into_val(env) == Ok(topic)
    })
}

#[test]
fn test_propose_and_accept_before_expiry_succeeds() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    client.propose_oracle_rotation(&new_oracle, &3600);

    let proposal = client
        .get_oracle_rotation_proposal()
        .expect("proposal should exist");
    assert_eq!(proposal.new_oracle, new_oracle);
    assert_eq!(proposal.proposed_at, 1000);
    assert_eq!(proposal.expires_at, 4600);

    env.ledger().with_mut(|li| {
        li.timestamp = 2000;
    });

    client.accept_oracle_rotation();

    let stored: Address = client.get_oracle().expect("oracle should be set");
    assert_eq!(stored, new_oracle);

    assert!(
        client.get_oracle_rotation_proposal().is_none(),
        "proposal should be removed after acceptance"
    );
}

#[test]
fn test_accept_after_expiry_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| {
        li.timestamp = 500;
    });

    client.propose_oracle_rotation(&new_oracle, &300);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let result = client.try_accept_oracle_rotation();
    assert_eq!(result, Err(Ok(ContractError::RotationExpired)));

    let stored: Address = client.get_oracle().expect("oracle should be set");
    assert_ne!(stored, new_oracle, "oracle should NOT have been rotated");

    assert!(
        client.get_oracle_rotation_proposal().is_none(),
        "expired proposal should be removed"
    );
}

#[test]
fn test_admin_cancel_removes_proposal() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    client.propose_oracle_rotation(&new_oracle, &3600);

    assert!(
        client.get_oracle_rotation_proposal().is_some(),
        "proposal should exist after propose"
    );

    client.cancel_oracle_rotation();

    assert!(
        client.get_oracle_rotation_proposal().is_none(),
        "proposal should be removed after cancel"
    );

    let stored: Address = client.get_oracle().expect("oracle should be set");
    assert_ne!(stored, new_oracle, "oracle should NOT have changed");
}

#[test]
fn test_cancel_when_no_proposal_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, _new_oracle) = init(&env, &client);

    let result = client.try_cancel_oracle_rotation();
    assert_eq!(result, Err(Ok(ContractError::NoPendingRotation)));
}

#[test]
fn test_accept_when_no_proposal_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, _new_oracle) = init(&env, &client);

    let result = client.try_accept_oracle_rotation();
    assert_eq!(result, Err(Ok(ContractError::NoPendingRotation)));
}

#[test]
fn test_propose_replaces_existing_proposal() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);
    let another_oracle = Address::generate(&env);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    client.propose_oracle_rotation(&new_oracle, &3600);
    client.propose_oracle_rotation(&another_oracle, &7200);

    let proposal = client
        .get_oracle_rotation_proposal()
        .expect("proposal should exist");
    assert_eq!(proposal.new_oracle, another_oracle);
    assert_eq!(proposal.proposed_at, 1000);
    assert_eq!(proposal.expires_at, 8200);
}

#[test]
fn test_propose_expiry_too_short_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    let result = client.try_propose_oracle_rotation(&new_oracle, &59);
    assert_eq!(result, Err(Ok(ContractError::InvalidStaleThreshold)));
}

#[test]
fn test_propose_and_accept_emits_events() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    client.propose_oracle_rotation(&new_oracle, &3600);

    let events = env.events().all();
    assert!(
        has_event_with_topic(&events, &env, symbol_short!("propose")),
        "propose event should be emitted"
    );

    env.ledger().with_mut(|li| {
        li.timestamp = 2000;
    });

    client.accept_oracle_rotation();

    let events = env.events().all();
    assert!(
        has_event_with_topic(&events, &env, symbol_short!("accept")),
        "accept event should be emitted"
    );
}

#[test]
fn test_accept_after_expiry_emits_expired_event() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| {
        li.timestamp = 500;
    });

    client.propose_oracle_rotation(&new_oracle, &300);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let _ = client.try_accept_oracle_rotation();

    let events = env.events().all();
    assert!(
        has_event_with_topic(&events, &env, symbol_short!("expired")),
        "expired event should be emitted"
    );
}

#[test]
fn test_cancel_emits_cancel_event() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, _new_oracle) = init(&env, &client);

    client.propose_oracle_rotation(&Address::generate(&env), &3600);
    client.cancel_oracle_rotation();

    let events = env.events().all();
    assert!(
        has_event_with_topic(&events, &env, symbol_short!("cancel")),
        "cancel event should be emitted"
    );
}

#[test]
fn test_propose_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.mock_all_auths_allowing_non_root_auth();
    let result = client.try_propose_oracle_rotation(&new_oracle, &3600);
    assert!(
        result.is_err(),
        "non-admin should not be able to propose"
    );
}
