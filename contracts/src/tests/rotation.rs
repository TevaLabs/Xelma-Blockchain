// SPDX-License-Identifier: MIT
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
    events: &soroban_sdk::Vec<(
        Address,
        soroban_sdk::Vec<soroban_sdk::Val>,
        soroban_sdk::Val,
    )>,
    env: &Env,
    topic: soroban_sdk::Symbol,
) -> bool {
    (0..events.len()).any(|i| {
        let event = events.get(i).unwrap();
        let topics = event.1;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("oracle"))
            && topics.get(1).unwrap().try_into_val(env) == Ok(topic.clone())
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
        // Earliest allowed moment: the 1h delay has elapsed and the
        // proposal (expires_at = 4600) has not yet expired.
        li.timestamp = 4600;
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

    client.propose_oracle_rotation(&new_oracle, &3600);

    env.ledger().with_mut(|li| {
        li.timestamp = 5000;
    });

    let result = client.try_accept_oracle_rotation();
    assert_eq!(result, Err(Ok(ContractError::NoPendingRotation)));

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
    assert_eq!(result, Err(Ok(ContractError::InvalidDuration)));
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
        // Earliest allowed moment: the 1h delay has elapsed and the
        // proposal (expires_at = 4600) has not yet expired.
        li.timestamp = 4600;
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

    client.propose_oracle_rotation(&new_oracle, &3600);

    env.ledger().with_mut(|li| {
        li.timestamp = 5000;
    });

    let _ = client.try_accept_oracle_rotation();
    let _ = client.get_oracle_rotation_proposal();

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

    env.mock_auths(&[]);
    let result = client.try_propose_oracle_rotation(&new_oracle, &3600);
    assert!(result.is_err(), "non-admin should not be able to propose");
}

// ────────────────────────────────────────────────────────────────────────────
// Early-accept / delay tests (Issue #273)
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn test_accept_before_min_delay_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    // Propose with long expiry
    client.propose_oracle_rotation(&new_oracle, &7200);

    // Try to accept immediately — should fail (delay not elapsed)
    let result = client.try_accept_oracle_rotation();
    assert_eq!(result, Err(Ok(ContractError::RotationDelayNotElapsed)));

    // Oracle should NOT have changed
    let stored: Address = client.get_oracle().expect("oracle should still be set");
    assert_ne!(
        stored, new_oracle,
        "oracle should not have been rotated early"
    );

    // Proposal should still exist
    assert!(
        client.get_oracle_rotation_proposal().is_some(),
        "proposal should still exist after failed early accept"
    );
}

#[test]
fn test_accept_after_min_delay_succeeds() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    client.propose_oracle_rotation(&new_oracle, &7200);

    // Advance past the min delay (1 hour = 3600 seconds)
    env.ledger().with_mut(|li| {
        li.timestamp = 1000 + 3600 + 1; // 1 hour + 1 second
    });

    // Should succeed now
    client.accept_oracle_rotation();

    let stored: Address = client.get_oracle().expect("oracle should be set");
    assert_eq!(stored, new_oracle, "oracle should have been rotated");

    assert!(
        client.get_oracle_rotation_proposal().is_none(),
        "proposal should be removed after acceptance"
    );
}

#[test]
fn test_accept_exactly_at_min_delay_succeeds() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    client.propose_oracle_rotation(&new_oracle, &7200);

    // Advance exactly to the boundary (1000 + 3600 = 4600)
    env.ledger().with_mut(|li| {
        li.timestamp = 1000 + 3600; // exactly at min delay boundary
    });

    // Should succeed at exactly the boundary
    client.accept_oracle_rotation();

    let stored: Address = client.get_oracle().expect("oracle should be set");
    assert_eq!(
        stored, new_oracle,
        "oracle should have been rotated at exact boundary"
    );
}

#[test]
fn test_early_accept_emits_oracle_early_event() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    client.propose_oracle_rotation(&new_oracle, &7200);

    let _ = client.try_accept_oracle_rotation();

    let events = env.events().all();
    assert!(
        has_event_with_topic(&events, &env, symbol_short!("early")),
        "early-accept attempt should emit (oracle, early) event"
    );
}

#[test]
fn test_accept_before_delay_fails_then_succeeds_after_delay() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| {
        li.timestamp = 5000;
    });

    client.propose_oracle_rotation(&new_oracle, &7200);

    // Try immediately — should fail
    let result = client.try_accept_oracle_rotation();
    assert_eq!(result, Err(Ok(ContractError::RotationDelayNotElapsed)));
    assert_eq!(client.get_oracle().unwrap(), oracle);

    // Try halfway through delay — should still fail
    env.ledger().with_mut(|li| {
        li.timestamp = 5000 + 1800; // 30 min
    });
    let result = client.try_accept_oracle_rotation();
    assert_eq!(result, Err(Ok(ContractError::RotationDelayNotElapsed)));
    assert_eq!(client.get_oracle().unwrap(), oracle);

    // Advance past full delay — should succeed
    env.ledger().with_mut(|li| {
        li.timestamp = 5000 + 3601; // 1 hour + 1 second
    });
    client.accept_oracle_rotation();
    assert_eq!(client.get_oracle().unwrap(), new_oracle);
}

#[test]
fn test_expired_proposal_returns_expired_when_delay_passed() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    // Expiry (3600) is exactly at the min delay boundary (also 3600).
    // After 4000 seconds, the proposal is expired AND delay elapsed.
    client.propose_oracle_rotation(&new_oracle, &3600);

    env.ledger().with_mut(|li| {
        li.timestamp = 1000 + 4000; // expired
    });

    // Should fail with NoPendingRotation (expiry check wins)
    let result = client.try_accept_oracle_rotation();
    assert_eq!(result, Err(Ok(ContractError::NoPendingRotation)));
}

// ────────────────────────────────────────────────────────────────────────────
// RotationDelayNotElapsed boundary and state tests (Issue #712)
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn test_accept_one_second_before_min_delay_fails() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let (_admin, oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| li.timestamp = 1000);
    client.propose_oracle_rotation(&new_oracle, &7200);

    // 1 second short of the 3600s delay: still rejected.
    env.ledger().with_mut(|li| li.timestamp = 1000 + 3599);
    let result = client.try_accept_oracle_rotation();
    assert_eq!(result, Err(Ok(ContractError::RotationDelayNotElapsed)));
    assert_eq!(client.get_oracle().unwrap(), oracle);

    // Exactly at the delay: accepted.
    env.ledger().with_mut(|li| li.timestamp = 1000 + 3600);
    client.accept_oracle_rotation();
    assert_eq!(client.get_oracle().unwrap(), new_oracle);
}

#[test]
fn test_early_accept_leaves_proposal_intact() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let (_admin, oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| li.timestamp = 2000);
    client.propose_oracle_rotation(&new_oracle, &7200);
    let before = client.get_oracle_rotation_proposal().expect("proposal set");

    for offset in [0u64, 1, 1800, 3599] {
        env.ledger().with_mut(|li| li.timestamp = 2000 + offset);
        let result = client.try_accept_oracle_rotation();
        assert_eq!(result, Err(Ok(ContractError::RotationDelayNotElapsed)));
    }

    // Rejected attempts neither consume nor alter the proposal, and the
    // active oracle never changes.
    assert_eq!(client.get_oracle().unwrap(), oracle);
    let after = client
        .get_oracle_rotation_proposal()
        .expect("still pending");
    assert_eq!(after.new_oracle, before.new_oracle);
    assert_eq!(after.proposed_at, before.proposed_at);
    assert_eq!(after.expires_at, before.expires_at);
}

#[test]
fn test_early_event_payload_reports_earliest_accept() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let (_admin, _oracle, new_oracle) = init(&env, &client);

    env.ledger().with_mut(|li| li.timestamp = 1000);
    client.propose_oracle_rotation(&new_oracle, &7200);

    env.ledger().with_mut(|li| li.timestamp = 1500);
    let _ = client.try_accept_oracle_rotation();

    let events = env.events().all();
    let payloads: std::vec::Vec<(Address, u64, u64)> = (0..events.len())
        .filter_map(|i| {
            let (_, topics, data) = events.get(i).unwrap();
            let is_early = topics.len() == 2
                && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("oracle"))
                && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("early"));
            is_early.then(|| data.try_into_val(&env).unwrap())
        })
        .collect();
    assert_eq!(payloads, std::vec![(new_oracle, 1500u64, 1000u64 + 3600)]);
}

#[test]
fn test_reproposal_restarts_the_delay() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let (_admin, oracle, first) = init(&env, &client);
    let second = Address::generate(&env);

    env.ledger().with_mut(|li| li.timestamp = 1000);
    client.propose_oracle_rotation(&first, &7200);

    // Replace the proposal 3000s in. The delay now counts from this proposal.
    env.ledger().with_mut(|li| li.timestamp = 4000);
    client.propose_oracle_rotation(&second, &7200);

    // 1000 + 3601 would have satisfied the first proposal's delay, but not
    // the replacement's.
    env.ledger().with_mut(|li| li.timestamp = 4000 + 3599);
    let result = client.try_accept_oracle_rotation();
    assert_eq!(result, Err(Ok(ContractError::RotationDelayNotElapsed)));
    assert_eq!(client.get_oracle().unwrap(), oracle);

    env.ledger().with_mut(|li| li.timestamp = 4000 + 3600);
    client.accept_oracle_rotation();
    assert_eq!(client.get_oracle().unwrap(), second);
}
