// SPDX-License-Identifier: MIT
//! Tests for optional participant access control (Issue #274).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{AccessState, BetSide};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _, Events},
    Address, Env, IntoVal, TryIntoVal,
};

fn setup() -> (Env, VirtualTokenContractClient<'static>, Address, Address) {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    (env, client, admin, oracle)
}

/// `test_default_open` — without any access-control configuration the
/// contract behaves exactly as before: any user may mint and bet.
#[test]
fn test_default_open_unconfigured() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    assert!(!client.is_access_control_enabled());
    assert_eq!(client.get_access_state(&user), AccessState::Open);

    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(client.balance(&user), 900_0000000);
}

/// `test_denylist_overrides` — a denylisted address is blocked regardless of
/// allowlist mode (it is an emergency block that works on open deployments).
#[test]
fn test_denylist_blocks_bet_in_open_mode() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    // Mint and fund while the deployment is still open.
    client.mint_initial(&user);
    assert_eq!(client.get_access_state(&user), AccessState::Open);

    client.add_denylisted(&user);
    assert!(client.is_user_denylisted(&user));
    assert_eq!(client.get_access_state(&user), AccessState::Denylisted);

    client.create_round(&1_0000000, &None);
    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::AccessDenied)));
}

/// `test_allowlist_enforced` — once allowlist mode is enabled, only allowlisted
/// addresses may mint / bet; everyone else is refused with `AccessDenied`.
#[test]
fn test_allowlist_mode_restricts_participation() {
    let (env, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.set_access_control_enabled(&true);
    assert!(client.is_access_control_enabled());

    client.add_allowlisted(&alice);
    assert!(client.is_user_allowlisted(&alice));
    assert_eq!(client.get_access_state(&alice), AccessState::Allowlisted);
    assert_eq!(client.get_access_state(&bob), AccessState::Open);

    // Alice can mint and fund; Bob is not allowlisted, so minting is refused.
    client.mint_initial(&alice);
    let mint_result = client.try_mint_initial(&bob);
    assert!(mint_result.is_err(), "non-allowlisted mint must fail");

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    // Bob is not allowlisted → denied even though not on any list.
    let bet_result = client.try_place_bet(&bob, &100_0000000, &BetSide::Up);
    assert_eq!(bet_result, Err(Ok(ContractError::AccessDenied)));
}

/// `test_allowlist_gates_precision_flows` — precision predictions and
/// commit-reveal commitments are gated the same way as Up/Down bets.
#[test]
fn test_allowlist_gates_precision_and_commit() {
    let (env, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.set_access_control_enabled(&true);
    client.add_allowlisted(&alice);
    client.mint_initial(&alice);

    client.create_round(&1_0000000, &Some(1));

    // Allowed user succeeds.
    client.place_precision_prediction(&alice, &125_0000000, &1_1000000);

    // Non-allowlisted user is refused on both precision entrypoints.
    let predict = client.try_place_precision_prediction(&bob, &75_0000000, &1_0000000);
    assert_eq!(predict, Err(Ok(ContractError::AccessDenied)));
    let commit =
        client.try_commit_prediction(&bob, &soroban_sdk::BytesN::from_array(&env, &[7; 32]), &75_0000000);
    assert_eq!(commit, Err(Ok(ContractError::AccessDenied)));
}

/// `test_denylist_wins_over_allowlist` — adding an allowlisted address to the
/// denylist flips its policy state and blocks it, because the denylist always
/// takes precedence.
#[test]
fn test_denylist_wins_over_allowlist() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    client.set_access_control_enabled(&true);
    client.add_allowlisted(&user);
    assert_eq!(client.get_access_state(&user), AccessState::Allowlisted);

    // Fund while allowlisted, then the admin bans the user.
    client.mint_initial(&user);
    client.add_denylisted(&user);
    assert_eq!(client.get_access_state(&user), AccessState::Denylisted);
    assert!(!client.is_user_allowlisted(&user), "conflicting allowlist marker cleared");

    client.create_round(&1_5000000, &None);
    let result = client.try_place_bet(&user, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::AccessDenied)));
}

/// `test_admin_only_mutations` — non-admins cannot toggle the mode or mutate
/// either list; every require_auth is enforced.
#[test]
fn test_admin_only_mutations_enforced() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let attacker = Address::generate(&env);

    // Only mock admin auth for `initialize`.
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &oracle).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &oracle);

    // No auth is mocked for these mutations → admin.require_auth() fails.
    assert!(client.try_set_access_control_enabled(&true).is_err());
    assert!(client.try_add_allowlisted(&attacker).is_err());
    assert!(client.try_remove_allowlisted(&attacker).is_err());
    assert!(client.try_add_denylisted(&attacker).is_err());
    assert!(client.try_remove_denylisted(&attacker).is_err());
}

/// `test_list_update_events` — every mutation publishes an `("access", …)`
/// event carrying the affected address and reconciliation flag.
#[test]
fn test_list_update_events_emitted() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    client.set_access_control_enabled(&true);
    assert!(has_access_event(&env, &symbol_short!("mode")));

    client.add_allowlisted(&user);
    assert!(has_access_event(&env, &symbol_short!("allow_add")));

    client.add_denylisted(&user); // reconciles (clears allowlist marker)
    assert!(has_access_event(&env, &symbol_short!("deny_add")));

    client.remove_denylisted(&user);
    assert!(has_access_event(&env, &symbol_short!("deny_rm")));

    // allowlist marker was reconciled away by the deny; removal is a no-op.
    client.remove_allowlisted(&user);
    assert!(!has_access_event(&env, &symbol_short!("allow_rm")));
}

fn has_access_event(env: &Env, detail: &soroban_sdk::Symbol) -> bool {
    for (_, topics, _data) in env.events().all().iter() {
        if topics.get(0).is_some() {
            let first: soroban_sdk::Symbol = topics.get(0).unwrap().try_into_val(env).unwrap();
            if first == symbol_short!("access") {
                let d: soroban_sdk::Symbol = topics.get(1).unwrap().try_into_val(env).unwrap();
                if &d == detail {
                    return true;
                }
            }
        }
    }
    false
}

/// `test_removal_is_idempotent` — removing an address that is not present
/// succeeds as a no-op, so operator reconciliation scripts stay simple.
#[test]
fn test_removal_is_idempotent_and_does_not_emit() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    client.remove_allowlisted(&user);
    client.remove_denylisted(&user);

    let events = env.events().all();
    for (_, topics, _data) in events.iter() {
        if topics.get(0).is_some() {
            let first: soroban_sdk::Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
            if first == symbol_short!("access") {
                panic!("no-access-mutation should emit no access event");
            }
        }
    }
}

/// `test_allowlist_gates_cashout` — the early cash-out entrypoint
/// is also gated by access control, so a non-allowlisted user cannot
/// cash out even in an active UpDown round.
#[test]
fn test_allowlist_gates_cashout() {
    let (env, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.set_access_control_enabled(&true);
    client.add_allowlisted(&alice);
    client.mint_initial(&alice);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    // Advance into the Running phase.
    env.ledger().with_mut(|li| { li.sequence_number = 8; });

    // Non-allowlisted user is refused on early cash-out.
    let cashout = client.try_cash_out_early(&bob);
    assert_eq!(cashout, Err(Ok(ContractError::AccessDenied)));
}

/// `test_denylist_blocks_cashout` — a denylisted address is blocked
/// from early cash-out regardless of mode.
#[test]
fn test_denylist_blocks_cashout() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    client.set_access_control_enabled(&true);
    client.add_allowlisted(&user);
    client.mint_initial(&user);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| { li.sequence_number = 8; });

    client.add_denylisted(&user);
    assert_eq!(client.get_access_state(&user), AccessState::Denylisted);

    let cashout = client.try_cash_out_early(&user);
    assert_eq!(cashout, Err(Ok(ContractError::AccessDenied)));
}

/// `test_protocol_health_reports_access_mode` — enabling allowlist mode is
/// surfaced as the informational `ACCESS_RESTRICTED` (6) status code when the
/// rest of the protocol is otherwise healthy.
#[test]
fn test_protocol_health_reports_access_mode() {
    let (_env, client, _admin, _oracle) = setup();

    // Heartbeat so the oracle is live (otherwise status stays ORACLE_STALE).
    client.update_oracle_heartbeat(&0);
    client.create_round(&1_0000000, &None);

    let before = client.get_protocol_health();
    assert_ne!(before.status_code, 6, "should not be restricted before enabling");

    client.set_access_control_enabled(&true);
    let after = client.get_protocol_health();
    assert_eq!(after.status_code, 6, "allowlist mode should surface ACCESS_RESTRICTED");
}

/// `test_allowlist_gates_claim_winnings` — a non-allowlisted user cannot
/// claim pending winnings when allowlist mode is enabled.
#[test]
fn test_allowlist_gates_claim_winnings() {
    let (env, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);

    client.set_access_control_enabled(&true);
    client.add_allowlisted(&alice);
    client.mint_initial(&alice);

    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &100_0000000, &BetSide::Up);

    // Cancel the round so Alice has pending winnings refunded.
    client.cancel_round(&0);

    // Remove Alice from allowlist → claim is denied.
    client.remove_allowlisted(&alice);
    let claim_denied = client.try_claim_winnings(&alice);
    assert_eq!(claim_denied, Err(Ok(ContractError::AccessDenied)));

    // Re-allowlist Alice → claim succeeds.
    client.add_allowlisted(&alice);
    let claimed = client.claim_winnings(&alice);
    assert_eq!(claimed, 100_0000000);
}

/// `test_denylist_blocks_claim_winnings` — a denylisted user cannot
/// claim pending winnings regardless of allowlist mode.
#[test]
fn test_denylist_blocks_claim_winnings() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // Cancel the round so user has pending winnings.
    client.cancel_round(&0);

    // Denylist the user.
    client.add_denylisted(&user);
    let claim_denied = client.try_claim_winnings(&user);
    assert_eq!(claim_denied, Err(Ok(ContractError::AccessDenied)));

    // Un-denylist the user → claim succeeds.
    client.remove_denylisted(&user);
    let claimed = client.claim_winnings(&user);
    assert_eq!(claimed, 100_0000000);
}

/// `test_allowlist_gates_reveal` — a non-allowlisted user cannot reveal commitments.
#[test]
fn test_allowlist_gates_reveal() {
    let (env, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);

    client.set_access_control_enabled(&true);
    client.add_allowlisted(&alice);
    client.mint_initial(&alice);

    client.create_round(&1_0000000, &Some(1));

    let salt = [42u8; 32];
    let salt_bytes = soroban_sdk::BytesN::from_array(&env, &salt);
    let mut preimage = soroban_sdk::Bytes::new(&env);
    preimage.append(&1_1000000u128.to_xdr(&env));
    preimage.append(&salt_bytes.to_xdr(&env));
    let hash: soroban_sdk::BytesN<32> = env.crypto().sha256(&preimage).into();

    client.commit_prediction(&alice, &hash, &100_0000000);

    // Advance to reveal phase (bet_end_ledger = 6).
    env.ledger().with_mut(|li| { li.sequence_number = 7; });

    // Admin removes Alice from allowlist before reveal.
    client.remove_allowlisted(&alice);
    let reveal_err = client.try_reveal_prediction(&alice, &1_1000000, &salt_bytes);
    assert_eq!(reveal_err, Err(Ok(ContractError::AccessDenied)));

    // Admin re-allowlists Alice.
    client.add_allowlisted(&alice);
    let reveal_ok = client.try_reveal_prediction(&alice, &1_1000000, &salt_bytes);
    assert!(reveal_ok.is_ok());
}

/// `test_denylist_blocks_reveal` — a denylisted user cannot reveal commitments.
#[test]
fn test_denylist_blocks_reveal() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1));

    let salt = [42u8; 32];
    let salt_bytes = soroban_sdk::BytesN::from_array(&env, &salt);
    let mut preimage = soroban_sdk::Bytes::new(&env);
    preimage.append(&1_1000000u128.to_xdr(&env));
    preimage.append(&salt_bytes.to_xdr(&env));
    let hash: soroban_sdk::BytesN<32> = env.crypto().sha256(&preimage).into();

    client.commit_prediction(&user, &hash, &100_0000000);

    // Advance to reveal phase.
    env.ledger().with_mut(|li| { li.sequence_number = 7; });

    client.add_denylisted(&user);
    let reveal_err = client.try_reveal_prediction(&user, &1_1000000, &salt_bytes);
    assert_eq!(reveal_err, Err(Ok(ContractError::AccessDenied)));
}

/// `test_get_access_policy_query` — tests querying composite policy tuple.
#[test]
fn test_get_access_policy_query() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);

    let (enabled, state) = client.get_access_policy(&user);
    assert!(!enabled);
    assert_eq!(state, AccessState::Open);

    client.set_access_control_enabled(&true);
    let (enabled, state) = client.get_access_policy(&user);
    assert!(enabled);
    assert_eq!(state, AccessState::Open);

    client.add_allowlisted(&user);
    let (enabled, state) = client.get_access_policy(&user);
    assert!(enabled);
    assert_eq!(state, AccessState::Allowlisted);

    client.add_denylisted(&user);
    let (enabled, state) = client.get_access_policy(&user);
    assert!(enabled);
    assert_eq!(state, AccessState::Denylisted);
}