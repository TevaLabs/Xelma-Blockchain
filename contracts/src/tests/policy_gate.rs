// SPDX-License-Identifier: MIT
//! Exhaustive mode x action tests for the central PolicyGate (Issue #261, Issue #880).

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{
    BetSide, ConfigChangeKind, MultiFeedPayload, OraclePayload, PolicyAction, ProtocolStatus,
};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    (client, admin, oracle)
}

#[test]
fn test_policy_gate_normal_mode_allows_everything() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    assert!(client.is_action_allowed(&PolicyAction::RoundMutation));
    assert!(client.is_action_allowed(&PolicyAction::Claim));
    assert!(client.is_action_allowed(&PolicyAction::AdminConfig));
    assert!(client.is_action_allowed(&PolicyAction::Settlement));
}

#[test]
fn test_policy_gate_claims_only_blocks_only_round_mutation() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.set_runtime_mode(&1u32); // ClaimsOnly

    assert!(!client.is_action_allowed(&PolicyAction::RoundMutation));
    assert!(client.is_action_allowed(&PolicyAction::Claim));
    assert!(client.is_action_allowed(&PolicyAction::AdminConfig));
    assert!(client.is_action_allowed(&PolicyAction::Settlement));
}

#[test]
fn test_policy_gate_fully_paused_blocks_everything() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.pause_contract();

    assert!(!client.is_action_allowed(&PolicyAction::RoundMutation));
    assert!(!client.is_action_allowed(&PolicyAction::Claim));
    assert!(!client.is_action_allowed(&PolicyAction::AdminConfig));
    assert!(!client.is_action_allowed(&PolicyAction::Settlement));
}

#[test]
fn test_policy_gate_matches_actual_entrypoint_behaviour_round_mutation() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.set_runtime_mode(&1u32); // ClaimsOnly: no active round
    assert!(!client.is_action_allowed(&PolicyAction::RoundMutation));

    // place_bet is RoundMutation-gated and requires an active round anyway —
    // ClaimsOnly blocks it via the policy gate before NoActiveRound is reached.
    let result = client.try_place_bet(&user, &10_0000000, &crate::types::BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));

    // create_round itself is admin-gated (AdminConfig class, blocked only by
    // FullyPaused) since it is the entrypoint that transitions ClaimsOnly
    // back to Active — it must remain callable while in ClaimsOnly.
    assert!(client.is_action_allowed(&PolicyAction::AdminConfig));
    client.create_round(&1_0000000, &None);
    assert!(client.get_active_round().is_some());
}

#[test]
fn test_policy_gate_admin_config_still_allowed_in_claims_only() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.set_runtime_mode(&1u32); // ClaimsOnly
                                    // Admin can still reconfigure — e.g. pause_contract itself is AdminConfig-gated.
    client.pause_contract();
    assert!(client.is_paused());
}

#[test]
fn test_policy_gate_transitions_round_trip() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    assert_eq!(client.get_runtime_mode(), 0); // Normal
    client.set_runtime_mode(&1u32);
    assert_eq!(client.get_runtime_mode(), 1); // ClaimsOnly
    client.set_runtime_mode(&2u32);
    assert_eq!(client.get_runtime_mode(), 2); // FullyPaused
    client.set_runtime_mode(&0u32);
    assert_eq!(client.get_runtime_mode(), 0); // Normal
}

// ─── Suite 1: Normal mode entrypoint verification ─────────────────────────────

#[test]
fn test_policy_gate_normal_mode_allows_round_mutation_entrypoints() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);

    // mint_initial passes the policy gate in Normal mode
    assert!(client.try_mint_initial(&user).is_ok());
    assert_eq!(client.balance(&user), 1000_0000000);

    // create_round so betting entrypoints have an active round
    client.create_round(&1_0000000, &None);

    let bet_res = client.try_place_bet(&user, &10_0000000, &BetSide::Up);
    assert_ne!(bet_res, Err(Ok(ContractError::ContractPaused)));
    assert!(bet_res.is_ok());

    // All other round mutation entrypoints pass the policy gate (none fail with ContractPaused)
    assert_ne!(
        client.try_place_precision_prediction(&user, &10_0000000, &2297),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_ne!(
        client.try_predict_price(&user, &10_0000000, &2297),
        Err(Ok(ContractError::ContractPaused))
    );
    let dummy_hash = BytesN::from_array(&env, &[0u8; 32]);
    assert_ne!(
        client.try_commit_prediction(&user, &dummy_hash, &10_0000000),
        Err(Ok(ContractError::ContractPaused))
    );
    let dummy_salt = BytesN::from_array(&env, &[1u8; 32]);
    assert_ne!(
        client.try_reveal_prediction(&user, &2297, &dummy_salt),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_ne!(
        client.try_apply_scheduled_changes(&ConfigChangeKind::MinBet),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_ne!(
        client.try_cash_out_early(&user),
        Err(Ok(ContractError::ContractPaused))
    );
}

#[test]
fn test_policy_gate_normal_mode_allows_claim_entrypoints() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);

    let claim_res = client.try_claim_winnings(&user);
    assert_ne!(claim_res, Err(Ok(ContractError::ContractPaused)));
    assert!(claim_res.is_ok());

    let users = Vec::from_array(&env, [user.clone()]);
    let claim_many_res = client.try_claim_many(&users);
    assert_ne!(claim_many_res, Err(Ok(ContractError::ContractPaused)));
    assert!(claim_many_res.is_ok());
}

#[test]
fn test_policy_gate_normal_mode_allows_settlement_entrypoints() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    // Create an active round to test settlement cancellation
    client.create_round(&1_0000000, &None);
    let cancel_res = client.try_cancel_round(&0u32);
    assert_ne!(cancel_res, Err(Ok(ContractError::ContractPaused)));
    assert!(cancel_res.is_ok());

    let payload = OraclePayload {
        price: 1_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    };
    assert_ne!(
        client.try_resolve_round(&payload),
        Err(Ok(ContractError::ContractPaused))
    );

    let mut prices = Vec::new(&env);
    prices.push_back(1_0000000);
    let mut sources = Vec::new(&env);
    sources.push_back(1);
    let multi_payload = MultiFeedPayload {
        prices,
        sources,
        round_id: 0,
        nonce: 2u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        timestamp: env.ledger().timestamp(),
    };
    assert_ne!(
        client.try_resolve_round_multi(&multi_payload),
        Err(Ok(ContractError::ContractPaused))
    );

    assert_ne!(
        client.try_void_round(&0u64),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_ne!(
        client.try_finalize_round(&0u64),
        Err(Ok(ContractError::ContractPaused))
    );
}

#[test]
fn test_policy_gate_normal_mode_allows_admin_config_entrypoints() {
    let env = Env::default();
    let (client, admin, _oracle) = setup(&env);

    let win_res = client.try_set_windows(&10, &20);
    assert_ne!(win_res, Err(Ok(ContractError::ContractPaused)));
    assert!(win_res.is_ok());

    assert!(client.try_set_oracle_max_deviation_bps(&Some(500)).is_ok());
    assert!(client.try_set_max_stake(&Some(10_000_000_000)).is_ok());
    assert!(client.try_create_round(&1_0000000, &None).is_ok());

    assert_ne!(
        client.try_withdraw_protocol_fee(&admin, &1),
        Err(Ok(ContractError::ContractPaused))
    );
}

// ─── Suite 2: ClaimsOnly mode entrypoint verification ─────────────────────────

#[test]
fn test_policy_gate_claims_only_blocks_every_round_mutation_entrypoint() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);

    client.set_runtime_mode(&1u32); // ClaimsOnly

    // 1. place_bet
    assert_eq!(
        client.try_place_bet(&user, &10_0000000, &BetSide::Up),
        Err(Ok(ContractError::ContractPaused))
    );
    // 2. place_precision_prediction
    assert_eq!(
        client.try_place_precision_prediction(&user, &10_0000000, &2297),
        Err(Ok(ContractError::ContractPaused))
    );
    // 3. predict_price
    assert_eq!(
        client.try_predict_price(&user, &10_0000000, &2297),
        Err(Ok(ContractError::ContractPaused))
    );
    // 4. commit_prediction
    let dummy_hash = BytesN::from_array(&env, &[0u8; 32]);
    assert_eq!(
        client.try_commit_prediction(&user, &dummy_hash, &10_0000000),
        Err(Ok(ContractError::ContractPaused))
    );
    // 5. reveal_prediction
    let dummy_salt = BytesN::from_array(&env, &[1u8; 32]);
    assert_eq!(
        client.try_reveal_prediction(&user, &2297, &dummy_salt),
        Err(Ok(ContractError::ContractPaused))
    );
    // 6. mint_initial (panics with ContractPaused error inside Soroban host)
    assert_eq!(
        client.try_mint_initial(&user),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            ContractError::ContractPaused as u32
        )))
    );
    // 7. apply_scheduled_changes
    assert_eq!(
        client.try_apply_scheduled_changes(&ConfigChangeKind::MinBet),
        Err(Ok(ContractError::ContractPaused))
    );
    // 8. cash_out_early
    assert_eq!(
        client.try_cash_out_early(&user),
        Err(Ok(ContractError::ContractPaused))
    );
}

#[test]
fn test_policy_gate_claims_only_allows_claim_entrypoints() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);

    client.set_runtime_mode(&1u32); // ClaimsOnly

    let claim_res = client.try_claim_winnings(&user);
    assert_ne!(claim_res, Err(Ok(ContractError::ContractPaused)));
    assert!(claim_res.is_ok());

    let users = Vec::from_array(&env, [user.clone()]);
    let claim_many_res = client.try_claim_many(&users);
    assert_ne!(claim_many_res, Err(Ok(ContractError::ContractPaused)));
    assert!(claim_many_res.is_ok());
}

#[test]
fn test_policy_gate_claims_only_allows_settlement_entrypoints() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.set_runtime_mode(&1u32); // ClaimsOnly

    // create_round is AdminConfig, allowed in ClaimsOnly
    assert!(client.try_create_round(&1_0000000, &None).is_ok());

    let cancel_res = client.try_cancel_round(&0u32);
    assert_ne!(cancel_res, Err(Ok(ContractError::ContractPaused)));
    assert!(cancel_res.is_ok());

    let payload = OraclePayload {
        price: 1_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    };
    assert_ne!(
        client.try_resolve_round(&payload),
        Err(Ok(ContractError::ContractPaused))
    );

    assert_ne!(
        client.try_void_round(&0u64),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_ne!(
        client.try_finalize_round(&0u64),
        Err(Ok(ContractError::ContractPaused))
    );
}

#[test]
fn test_policy_gate_claims_only_allows_admin_config_entrypoints() {
    let env = Env::default();
    let (client, admin, _oracle) = setup(&env);

    client.set_runtime_mode(&1u32); // ClaimsOnly

    // create_round must remain callable to transition back to Active
    assert!(client.try_create_round(&1_0000000, &None).is_ok());
    let win_res = client.try_set_windows(&10, &20);
    assert_ne!(win_res, Err(Ok(ContractError::ContractPaused)));
    assert!(win_res.is_ok());
    assert!(client.try_set_max_stake(&Some(10_000_000_000)).is_ok());

    assert_ne!(
        client.try_cancel_config_change(&ConfigChangeKind::MinBet),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_ne!(
        client.try_withdraw_protocol_fee(&admin, &1),
        Err(Ok(ContractError::ContractPaused))
    );
}

// ─── Suite 3: FullyPaused mode entrypoint verification ────────────────────────

#[test]
fn test_policy_gate_fully_paused_blocks_round_mutation_entrypoints() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);

    client.pause_contract();
    assert!(client.is_paused());

    // 1. place_bet
    assert_eq!(
        client.try_place_bet(&user, &10_0000000, &BetSide::Up),
        Err(Ok(ContractError::ContractPaused))
    );
    // 2. place_precision_prediction
    assert_eq!(
        client.try_place_precision_prediction(&user, &10_0000000, &2297),
        Err(Ok(ContractError::ContractPaused))
    );
    // 3. predict_price
    assert_eq!(
        client.try_predict_price(&user, &10_0000000, &2297),
        Err(Ok(ContractError::ContractPaused))
    );
    // 4. commit_prediction
    let dummy_hash = BytesN::from_array(&env, &[0u8; 32]);
    assert_eq!(
        client.try_commit_prediction(&user, &dummy_hash, &10_0000000),
        Err(Ok(ContractError::ContractPaused))
    );
    // 5. reveal_prediction
    let dummy_salt = BytesN::from_array(&env, &[1u8; 32]);
    assert_eq!(
        client.try_reveal_prediction(&user, &2297, &dummy_salt),
        Err(Ok(ContractError::ContractPaused))
    );
    // 6. mint_initial (panics with ContractPaused error inside Soroban host)
    assert_eq!(
        client.try_mint_initial(&user),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            ContractError::ContractPaused as u32
        )))
    );
    // 7. apply_scheduled_changes
    assert_eq!(
        client.try_apply_scheduled_changes(&ConfigChangeKind::MinBet),
        Err(Ok(ContractError::ContractPaused))
    );
    // 8. cash_out_early
    assert_eq!(
        client.try_cash_out_early(&user),
        Err(Ok(ContractError::ContractPaused))
    );
}

#[test]
fn test_policy_gate_fully_paused_blocks_claim_entrypoints() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);

    client.pause_contract();
    assert!(client.is_paused());

    assert_eq!(
        client.try_claim_winnings(&user),
        Err(Ok(ContractError::ContractPaused))
    );
    let users = Vec::from_array(&env, [user.clone()]);
    assert_eq!(
        client.try_claim_many(&users),
        Err(Ok(ContractError::ContractPaused))
    );
}

#[test]
fn test_policy_gate_fully_paused_blocks_settlement_entrypoints() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    // Create a round prior to pausing so cancel_round has an active round to operate on
    client.create_round(&1_0000000, &None);

    client.pause_contract();
    assert!(client.is_paused());

    let payload = OraclePayload {
        price: 1_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
        attestation: None,
    };
    assert_eq!(
        client.try_resolve_round(&payload),
        Err(Ok(ContractError::ContractPaused))
    );

    let mut prices = Vec::new(&env);
    prices.push_back(1_0000000);
    let mut sources = Vec::new(&env);
    sources.push_back(1);
    let multi_payload = MultiFeedPayload {
        prices,
        sources,
        round_id: 0,
        nonce: 2u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        timestamp: env.ledger().timestamp(),
    };
    assert_eq!(
        client.try_resolve_round_multi(&multi_payload),
        Err(Ok(ContractError::ContractPaused))
    );

    assert_eq!(
        client.try_void_round(&0u64),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(
        client.try_finalize_round(&0u64),
        Err(Ok(ContractError::ContractPaused))
    );

    // cancel_round intentionally bypasses _ensure_not_paused — acts as emergency admin unwind.
    // See docs/PAUSE_POLICY_MATRIX.md for discrepancy note. File a separate issue to add the guard.
    let cancel_res = client.try_cancel_round(&0u32);
    assert_ne!(cancel_res, Err(Ok(ContractError::ContractPaused)));
    assert!(cancel_res.is_ok());
}

#[test]
fn test_policy_gate_fully_paused_blocks_admin_config_entrypoints() {
    let env = Env::default();
    let (client, admin, _oracle) = setup(&env);

    client.pause_contract();
    assert!(client.is_paused());

    assert_eq!(
        client.try_create_round(&1_0000000, &None),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(
        client.try_create_next_from_template(),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(
        client.try_set_windows(&10, &20),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(
        client.try_set_max_stake(&Some(10_000_000_000)),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(
        client.try_set_oracle_max_deviation_bps(&Some(500)),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(
        client.try_withdraw_protocol_fee(&admin, &1),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(
        client.try_set_round_template(&1_0000000, &None),
        Err(Ok(ContractError::ContractPaused))
    );
}

#[test]
fn test_policy_gate_fully_paused_allows_mode_controls_and_queries() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);
    let user = Address::generate(&env);

    client.pause_contract();
    assert!(client.is_paused());
    assert_eq!(client.get_runtime_mode(), 2u32);

    // Queries must never be blocked
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Paused);
    let health = client.get_protocol_health();
    assert!(health.paused);
    assert_eq!(health.status_code, 1u32);
    assert_eq!(client.balance(&user), 0);
    assert_eq!(client.get_active_round(), None);
    assert!(client.try_update_oracle_heartbeat(&0u32).is_ok());

    // Policy gate inspection queries
    assert!(!client.is_action_allowed(&PolicyAction::RoundMutation));
    assert!(!client.is_action_allowed(&PolicyAction::Claim));

    // Mode transitions must never be blocked by policy gate (incident exit path)
    assert!(client.try_set_runtime_mode(&1u32).is_ok());
    assert_eq!(client.get_runtime_mode(), 1u32);

    client.set_runtime_mode(&2u32); // Back to FullyPaused
    assert!(client.is_paused());
    assert!(client.try_unpause_contract().is_ok());
    assert!(!client.is_paused());
    assert_eq!(client.get_runtime_mode(), 0u32);
}
