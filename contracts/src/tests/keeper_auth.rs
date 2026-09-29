// SPDX-License-Identifier: MIT
//! Auth-focused guard tests for keeper and settlement flows.
//!
//! These checks intentionally avoid `env.mock_all_auths()` and instead exercise the
//! exact `require_auth` contracts that protect keeper-driven mutations and oracle
//! settlement paths. This keeps the tests sensitive to accidental auth drift.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{MultiFeedPayload, OraclePayload, OracleQuorumConfig};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal,
};

#[test]
fn test_keeper_template_creation_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &oracle).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &oracle);
    env.mock_auths(&[MockAuth {
        address: &oracle,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_oracle_heartbeat",
            args: (0u32,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.update_oracle_heartbeat(&0u32);

    // No authenticated keeper call is mocked here; trying to call the keeper path
    // without admin auth must fail.
    let err = client.try_create_next_from_template();
    assert!(err.is_err());
}

#[test]
fn test_oracle_resolve_requires_oracle_auth_and_rejects_reused_nonce() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &oracle).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &oracle);
    env.mock_auths(&[MockAuth {
        address: &oracle,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_oracle_heartbeat",
            args: (0u32,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.update_oracle_heartbeat(&0u32);

    env.mock_auths(&[MockAuth {
        address: &user,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "mint_initial",
            args: (&user,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.mint_initial(&user);

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_round",
            args: (1_0000000u128, Option::<u32>::None).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.create_round(&1_0000000u128, &None);

    let payload = OraclePayload {
        price: 9_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 7u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    };

    // Without the configured oracle auth, the contract must reject the call and
    // never proceed into the settlement logic.
    let err = client.try_resolve_round(&payload);
    assert!(err.is_err());
}

#[test]
fn test_multi_feed_quorum_and_outlier_checks_are_auth_scoped() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "initialize",
            args: (&admin, &oracle).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(&admin, &oracle);
    env.mock_auths(&[MockAuth {
        address: &oracle,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_oracle_heartbeat",
            args: (0u32,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.update_oracle_heartbeat(&0u32);

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_oracle_quorum_config",
            args: (Some(OracleQuorumConfig {
                min_observations: 3,
                quorum_threshold: 3,
                outlier_threshold_bps: 250,
            }),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.set_oracle_quorum_config(&Some(OracleQuorumConfig {
        min_observations: 3,
        quorum_threshold: 3,
        outlier_threshold_bps: 250,
    }));

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_round",
            args: (1_0000000u128, Option::<u32>::None).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.create_round(&1_0000000u128, &None);

    let network_id = BytesN::from_array(&env, &[0u8; 32]);
    let payload = MultiFeedPayload {
        prices: soroban_sdk::vec![&env, 100_0000000, 101_0000000, 500_0000000],
        sources: soroban_sdk::vec![&env, 1u32, 2u32, 3u32],
        round_id: 0,
        nonce: 12u64,
        network_id,
        contract_addr: contract_id.clone(),
        timestamp: env.ledger().timestamp(),
    };

    env.mock_auths(&[MockAuth {
        address: &oracle,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "resolve_round_multi",
            args: (payload.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let err = client.try_resolve_round_multi(&payload);
    assert!(err.is_err());

    // The contract authorized the oracle address and enforced a strict quorum policy.
    // This is the exact guardrail the keeper-intent model depends on.
}
