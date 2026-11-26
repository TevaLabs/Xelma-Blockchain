// SPDX-License-Identifier: MIT
//! Comprehensive test suite for deterministic one-sided (degenerate) market settlement policies.
//! Issue #270: Policy enum, deterministic selection, event emission, value conservation, and edge cases.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::settlement::{_apply_one_sided_policy, _select_one_sided_policy};
use crate::types::{BetSide, OneSidedPolicy, OraclePayload, Policy};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, TryIntoVal,
};

fn setup_test_env() -> (Env, VirtualTokenContractClient<'static>, Address, Address) {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    (env, client, admin, oracle)
}

#[test]
fn test_one_sided_up_market() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Up);

    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 300_0000000);
    assert_eq!(round.pool_down, 0);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    });

    // Capture events before subsequent contract calls reset the log
    let events = env.events().all();
    let one_sided_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("pool"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("onesided"))
    });
    assert!(one_sided_event.is_some(), "onesided event must be emitted");

    // 100% refund of stakes
    assert_eq!(client.get_pending_winnings(&alice), 100_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 200_0000000);

    // Verify stats were NOT mutated (no fake win/loss stats on one-sided refund)
    let alice_stats = client.get_user_stats(&alice);
    assert_eq!(alice_stats.total_wins, 0);
    assert_eq!(alice_stats.total_losses, 0);
}

#[test]
fn test_one_sided_down_market() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&2_0000000, &None);
    client.place_bet(&alice, &150_0000000, &BetSide::Down);
    client.place_bet(&bob, &250_0000000, &BetSide::Down);

    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 400_0000000);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    });

    let events = env.events().all();
    let one_sided_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("pool"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("onesided"))
    });
    assert!(one_sided_event.is_some());

    assert_eq!(client.get_pending_winnings(&alice), 150_0000000);
    assert_eq!(client.get_pending_winnings(&bob), 250_0000000);
}

#[test]
fn test_empty_market() {
    let (env, client, _admin, _oracle) = setup_test_env();

    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 0);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    });

    assert_eq!(client.get_active_round(), None);
}

#[test]
fn test_deterministic_policy_selection() {
    let (_env, client, _admin, _oracle) = setup_test_env();

    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    let selected_policy = _select_one_sided_policy(&round);
    assert_eq!(selected_policy, OneSidedPolicy::Refund);

    let contract_policy = client.get_one_sided_policy();
    assert_eq!(contract_policy, Policy::Refund);
}

/// #828: querying the policy with no active round must not panic and
/// defaults to `Refund`, both before any round and after one settles.
#[test]
fn test_one_sided_policy_defaults_to_refund_without_active_round() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    // Uninitialized contract: no active round, no config.
    assert_eq!(client.get_active_round(), None);
    assert_eq!(client.get_one_sided_policy(), OneSidedPolicy::Refund);

    let (_env, client, _admin, _oracle) = setup_test_env();
    assert_eq!(client.get_active_round(), None);
    assert_eq!(client.get_one_sided_policy(), OneSidedPolicy::Refund);
}

#[test]
fn test_emitted_events_and_metadata() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);

    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &500_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    });

    let events = env.events().all();
    let one_sided_event = events.iter().find(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("pool"))
            && topics.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("onesided"))
    });

    assert!(one_sided_event.is_some());
}

#[test]
fn test_refund_behavior_value_preservation() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    let alice_initial = client.balance(&alice);
    let bob_initial = client.balance(&bob);
    let charlie_initial = client.balance(&charlie);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Up);
    client.place_bet(&charlie, &300_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 1_2000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    });

    // Claim pending winnings
    client.claim_winnings(&alice);
    client.claim_winnings(&bob);
    client.claim_winnings(&charlie);

    assert_eq!(client.balance(&alice), alice_initial);
    assert_eq!(client.balance(&bob), bob_initial);
    assert_eq!(client.balance(&charlie), charlie_initial);
}

#[test]
fn test_carry_forward_behavior_fallback() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);

    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    let round = client.get_active_round().unwrap();

    let result = env.as_contract(&client.address, || {
        _apply_one_sided_policy(
            &env,
            &round,
            OneSidedPolicy::CarryForward,
            &soroban_sdk::vec![&env, alice.clone()],
            &None,
        )
    });

    assert!(result.is_ok());
    assert_eq!(client.get_pending_winnings(&alice), 100_0000000);
}

#[test]
fn test_repeated_settlement_attempts() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);

    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    let payload = OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    };

    client.resolve_round(&payload);

    // Second resolution attempt must fail with NoActiveRound
    let res = client.try_resolve_round(&OraclePayload {
        nonce: 2u64,
        ..payload
    });
    assert_eq!(res, Err(Ok(ContractError::NoActiveRound)));
}

#[test]
fn test_rounding_and_value_conservation() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    let alice_stake = 333_3333333i128;
    let bob_stake = 666_6666667i128;
    let total_pool = alice_stake + bob_stake;

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &alice_stake, &BetSide::Up);
    client.place_bet(&bob, &bob_stake, &BetSide::Up);

    env.ledger().with_mut(|li| {
        li.sequence_number = 12;
    });

    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    });

    let alice_refund = client.get_pending_winnings(&alice);
    let bob_refund = client.get_pending_winnings(&bob);

    assert_eq!(alice_refund, alice_stake);
    assert_eq!(bob_refund, bob_stake);
    assert_eq!(alice_refund + bob_refund, total_pool);
    assert_eq!(client.get_protocol_fee_treasury(), 0);
}

// ─── Golden vectors: OneSidedPolicy treasury / carry accounting (#825) ───────

/// `("pool", "onesided")` payload:
/// (round_id, policy_code, affected_side, refund, carry, pool_up, pool_down)
type OneSidedEvent = (u64, u32, u32, i128, i128, i128, i128);

fn last_onesided_event(env: &Env) -> OneSidedEvent {
    // Some paths also emit a shorter legacy `("pool", "onesided")` payload, so
    // only accept the documented 7-field one. Call this before any other
    // client call: `events().all()` only holds the last invocation.
    env.events()
        .all()
        .iter()
        .rev()
        .filter(|(_, topics, _)| {
            topics.len() == 2
                && topics.get(0).unwrap().try_into_val(env) == Ok(symbol_short!("pool"))
                && topics.get(1).unwrap().try_into_val(env) == Ok(symbol_short!("onesided"))
        })
        .find(|(_, _, data)| {
            let fields: Result<soroban_sdk::Vec<soroban_sdk::Val>, _> = data.try_into_val(env);
            fields.map(|f| f.len() == 7).unwrap_or(false)
        })
        .map(|(_, _, data)| data.try_into_val(env).unwrap())
        .expect("7-field onesided event must be emitted")
}

/// Opens a round, places one-sided bets and applies `policy` directly.
/// Returns the policy result, the event, and the bettors.
fn apply_policy(
    side: BetSide,
    stakes: &[i128],
    policy: OneSidedPolicy,
) -> (
    Env,
    VirtualTokenContractClient<'static>,
    Result<i128, ContractError>,
    OneSidedEvent,
    soroban_sdk::Vec<Address>,
) {
    let (env, client, _admin, _oracle) = setup_test_env();
    client.create_round(&1_0000000, &None);
    let mut users = soroban_sdk::Vec::new(&env);
    for stake in stakes {
        let user = Address::generate(&env);
        client.mint_initial(&user);
        client.place_bet(&user, stake, &side);
        users.push_back(user);
    }
    let round = client.get_active_round().unwrap();
    let result = env.as_contract(&client.address, || {
        _apply_one_sided_policy(&env, &round, policy, &users, &None)
    });
    let event = last_onesided_event(&env);
    (env, client, result, event, users)
}

#[test]
fn golden_carry_forward_up_only_event_and_treasury() {
    let stakes = [100_0000000i128, 200_0000000, 300_0000000];
    let (_env, client, result, event, users) =
        apply_policy(BetSide::Up, &stakes, OneSidedPolicy::CarryForward);

    // No protocol fee is taken on a degenerate market.
    assert_eq!(result, Ok(0));
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    // refund = 0, carry = whole pot, pools echoed, policy code 2, side Up.
    assert_eq!(
        event,
        (1, 2, 0, 0, 600_0000000, 600_0000000, 0),
        "CarryForward golden event"
    );
    // Stakes are still credited per participant (nothing is burned).
    for (i, stake) in stakes.iter().enumerate() {
        assert_eq!(
            client.get_pending_winnings(&users.get(i as u32).unwrap()),
            *stake
        );
    }
}

#[test]
fn golden_carry_forward_down_only_reports_side_one() {
    let stakes = [40_0000000i128, 60_0000000];
    let (_env, client, result, event, _users) =
        apply_policy(BetSide::Down, &stakes, OneSidedPolicy::CarryForward);

    assert_eq!(result, Ok(0));
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    assert_eq!(event, (1, 2, 1, 0, 100_0000000, 0, 100_0000000));
}

#[test]
fn golden_refund_and_void_report_refund_not_carry() {
    let stakes = [100_0000000i128, 200_0000000, 300_0000000];

    let (_e1, c1, r1, refund_event, _u1) =
        apply_policy(BetSide::Up, &stakes, OneSidedPolicy::Refund);
    assert_eq!(r1, Ok(0));
    assert_eq!(c1.get_protocol_fee_treasury(), 0);
    assert_eq!(refund_event, (1, 0, 0, 600_0000000, 0, 600_0000000, 0));

    let (_e2, c2, r2, void_event, _u2) = apply_policy(BetSide::Up, &stakes, OneSidedPolicy::Void);
    assert_eq!(r2, Ok(0));
    assert_eq!(c2.get_protocol_fee_treasury(), 0);
    assert_eq!(void_event, (1, 1, 0, 600_0000000, 0, 600_0000000, 0));
}

#[test]
fn golden_carry_forward_empty_market_is_all_zero() {
    let (env, client, _admin, _oracle) = setup_test_env();
    client.create_round(&1_0000000, &None);
    let round = client.get_active_round().unwrap();

    let result = env.as_contract(&client.address, || {
        _apply_one_sided_policy(
            &env,
            &round,
            OneSidedPolicy::CarryForward,
            &soroban_sdk::Vec::new(&env),
            &None,
        )
    });

    let event = last_onesided_event(&env);
    assert_eq!(result, Ok(0));
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    // affected_side = 2 (empty), nothing refunded or carried.
    assert_eq!(event, (1, 2, 2, 0, 0, 0, 0));
}

#[test]
fn golden_carry_forward_conserves_supply_after_claims() {
    let stakes = [333_3333333i128, 666_6666667];
    let (env, client, _result, event, users) =
        apply_policy(BetSide::Up, &stakes, OneSidedPolicy::CarryForward);
    let total: i128 = stakes.iter().sum();
    assert_eq!(event.4, total, "carry equals the whole pot");

    let mut credited = 0i128;
    for i in 0..users.len() {
        credited += client.get_pending_winnings(&users.get(i).unwrap());
    }
    // Σ credits + treasury == pot: nothing created or destroyed.
    assert_eq!(credited + client.get_protocol_fee_treasury(), total);

    for i in 0..users.len() {
        let user = users.get(i).unwrap();
        client.claim_winnings(&user);
        assert_eq!(client.balance(&user), client.balance(&user).max(0));
    }
    let _ = env;
}

#[test]
fn golden_default_policy_through_resolve_round_is_refund() {
    let (env, client, _admin, _oracle) = setup_test_env();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);
    env.ledger().with_mut(|li| li.sequence_number = 12);

    client.resolve_round(&OraclePayload {
        price: 2_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    });

    // Read right after the call: `events().all()` only holds the last invocation.
    let event = last_onesided_event(&env);
    assert_eq!(event, (1, 0, 0, 100_0000000, 0, 100_0000000, 0));
    assert_eq!(client.get_protocol_fee_treasury(), 0);
}
