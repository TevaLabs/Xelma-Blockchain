// SPDX-License-Identifier: MIT
//! Multi-feed oracle quorum gate (Issue #681).
//!
//! Quorum is hard: a payload whose surviving observations fall below the
//! configured threshold must fail with `InsufficientOracleQuorum` and leave no
//! partial settlement behind — the round stays active, balances/pools are
//! untouched, no TWAP sample is recorded, and the nonce is not burned.

use super::*;
use crate::types::{MultiFeedPayload, OracleQuorumConfig};
use soroban_sdk::Vec as SorobanVec;

fn setup_quorum(
    quorum: OracleQuorumConfig,
) -> (Env, VirtualTokenContractClient<'static>, Address, Address) {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.set_oracle_quorum_config(&Some(quorum));

    (env, client, admin, oracle)
}

fn three_feed_quorum() -> OracleQuorumConfig {
    OracleQuorumConfig {
        min_observations: 3,
        quorum_threshold: 3,
        outlier_threshold_bps: 500,
    }
}

fn build_payload(
    env: &Env,
    client: &VirtualTokenContractClient,
    prices: &[u128],
    nonce: u64,
) -> MultiFeedPayload {
    let round = client.get_active_round().expect("active round required");
    let mut price_vec = SorobanVec::new(env);
    for price in prices {
        price_vec.push_back(*price);
    }
    let mut sources = SorobanVec::new(env);
    for i in 0..prices.len() as u32 {
        sources.push_back(i + 1);
    }

    MultiFeedPayload {
        prices: price_vec,
        sources,
        round_id: round.start_ledger,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        timestamp: env.ledger().timestamp(),
    }
}

fn advance_to_round_end(env: &Env, client: &VirtualTokenContractClient) {
    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });
}

/// A rejected quorum must not settle the round, move funds, record a TWAP
/// sample, or consume the payload nonce.
#[test]
fn test_insufficient_quorum_is_hard_and_does_not_burn_nonce() {
    let (env, client, _admin, _oracle) = setup_quorum(three_feed_quorum());
    let alice = Address::generate(&env);

    client.mint_initial(&alice);
    client.create_round(&1_5000000u128, &None);
    client.place_bet(&alice, &100_0000000i128, &BetSide::Up);

    let round_before = client.get_active_round().unwrap();
    let balance_before = client.balance(&alice);
    assert_eq!(client.get_pending_winnings(&alice), 0);

    advance_to_round_end(&env, &client);

    // Median is 1.5 XLM; the 9.0 XLM feed is ~50,000 bps away and is rejected,
    // leaving only 2 survivors against a threshold of 3.
    let payload = build_payload(&env, &client, &[1_5000000, 1_5000000, 9_0000000], 7);

    let result = client.try_resolve_round_multi(&payload);
    assert_eq!(result, Err(Ok(ContractError::InsufficientOracleQuorum)));

    // No partial settle.
    assert_eq!(client.get_active_round(), Some(round_before));
    assert_eq!(client.balance(&alice), balance_before);
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_twap_samples().len(), 0);

    // The nonce was NOT consumed: replaying the same payload hits the quorum
    // gate again instead of OracleNonceReused.
    let replay = client.try_resolve_round_multi(&payload);
    assert_eq!(replay, Err(Ok(ContractError::InsufficientOracleQuorum)));

    // A corrected payload can reuse the same nonce and settles normally.
    let corrected = build_payload(&env, &client, &[1_5000000, 1_5100000, 1_4900000], 7);
    client.resolve_round_multi(&corrected);
    assert_eq!(client.get_active_round(), None);
    assert_eq!(client.get_twap_samples().len(), 1);
}

/// Exactly `quorum_threshold` survivors is sufficient: the median price is
/// settled and the multi-feed summary is emitted.
#[test]
fn test_quorum_at_threshold_settles() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.set_oracle_quorum_config(&Some(three_feed_quorum()));

    client.create_round(&1_5000000u128, &None);
    advance_to_round_end(&env, &client);

    let payload = build_payload(&env, &client, &[1_5000000, 1_5100000, 1_4900000], 1);
    client.resolve_round_multi(&payload);

    assert_eq!(client.get_active_round(), None);
}

/// Fewer observations than `min_observations` is refused before any settlement
/// work, with the dedicated `TooFewObservations` error.
#[test]
fn test_too_few_observations_does_not_settle() {
    let (env, client, _admin, _oracle) = setup_quorum(three_feed_quorum());
    let alice = Address::generate(&env);

    client.mint_initial(&alice);
    client.create_round(&1_5000000u128, &None);
    client.place_bet(&alice, &100_0000000i128, &BetSide::Up);

    let round_before = client.get_active_round().unwrap();
    advance_to_round_end(&env, &client);

    let payload = build_payload(&env, &client, &[1_5000000, 1_5100000], 1);
    let result = client.try_resolve_round_multi(&payload);
    assert_eq!(result, Err(Ok(ContractError::TooFewObservations)));

    assert_eq!(client.get_active_round(), Some(round_before));
    assert_eq!(client.balance(&alice), 900_0000000);
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_twap_samples().len(), 0);
}
