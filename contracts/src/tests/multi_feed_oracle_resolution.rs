// SPDX-License-Identifier: MIT
//! Multi-feed oracle quorum and replay validation for production parity.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{DataKeyScoped, MultiFeedPayload, OracleQuorumConfig};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal,
};

fn setup_round(
    env: &Env,
    contract_id: &Address,
    client: &VirtualTokenContractClient<'_>,
    admin: &Address,
    oracle: &Address,
) {
    env.mock_auths(&[MockAuth {
        address: admin,
        invoke: &MockAuthInvoke {
            contract: contract_id,
            fn_name: "initialize",
            args: (admin, oracle).into_val(env),
            sub_invokes: &[],
        },
    }]);
    client.initialize(admin, oracle);

    env.mock_auths(&[MockAuth {
        address: oracle,
        invoke: &MockAuthInvoke {
            contract: contract_id,
            fn_name: "update_oracle_heartbeat",
            args: (0u32,).into_val(env),
            sub_invokes: &[],
        },
    }]);
    client.update_oracle_heartbeat(&0u32);

    env.mock_auths(&[MockAuth {
        address: admin,
        invoke: &MockAuthInvoke {
            contract: contract_id,
            fn_name: "create_round",
            args: (1_0000000u128, Option::<u32>::None).into_val(env),
            sub_invokes: &[],
        },
    }]);
    client.create_round(&1_0000000u128, &None);
}

#[test]
fn test_multi_feed_rejects_insufficient_quorum() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    setup_round(&env, &contract_id, &client, &admin, &oracle);

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

    let round = client.get_active_round().expect("round should exist");
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });

    let payload = MultiFeedPayload {
        prices: soroban_sdk::vec![&env, 100_0000000, 101_0000000, 500_0000000],
        sources: soroban_sdk::vec![&env, 1u32, 2u32, 3u32],
        round_id: 0,
        nonce: 42u64,
        network_id: env.ledger().network_id(),
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
    let result = client.try_resolve_round_multi(&payload);
    assert_eq!(result, Err(Ok(ContractError::InsufficientOracleQuorum)));
}

#[test]
fn test_multi_feed_outlier_is_excluded_and_quorum_can_still_pass() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    setup_round(&env, &contract_id, &client, &admin, &oracle);

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_oracle_quorum_config",
            args: (Some(OracleQuorumConfig {
                min_observations: 4,
                quorum_threshold: 3,
                outlier_threshold_bps: 250,
            }),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.set_oracle_quorum_config(&Some(OracleQuorumConfig {
        min_observations: 4,
        quorum_threshold: 3,
        outlier_threshold_bps: 250,
    }));
    let round = client.get_active_round().expect("round should exist");
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });

    let payload = MultiFeedPayload {
        prices: soroban_sdk::vec![&env, 100_0000000, 101_0000000, 102_0000000, 500_0000000],
        sources: soroban_sdk::vec![&env, 11u32, 12u32, 13u32, 14u32],
        round_id: 0,
        nonce: 99u64,
        network_id: env.ledger().network_id(),
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
    let result = client.try_resolve_round_multi(&payload);
    assert_eq!(result, Ok(Ok(())));
}

#[test]
fn test_multi_feed_reused_nonce_is_rejected() {
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
            fn_name: "create_round",
            args: (1_0000000u128, Option::<u32>::None).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.create_round(&1_0000000u128, &None);

    let round = client.get_active_round().expect("round should exist");
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
        li.timestamp = 1_000;
    });

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

    let payload = MultiFeedPayload {
        prices: soroban_sdk::vec![&env, 100_0000000, 101_0000000, 500_0000000],
        sources: soroban_sdk::vec![&env, 21u32, 22u32, 23u32],
        round_id: round.start_ledger,
        nonce: 77u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        timestamp: round.start_timestamp + 1,
    };

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(
            &DataKeyScoped::ConsumedOracleNonce(round.round_id, payload.nonce),
            &true,
        );
    });

    env.mock_auths(&[MockAuth {
        address: &oracle,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "resolve_round_multi",
            args: (payload.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_resolve_round_multi(&payload);
    assert_eq!(result, Err(Ok(ContractError::OracleNonceReused)));
}
