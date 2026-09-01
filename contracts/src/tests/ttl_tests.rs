// SPDX-License-Identifier: MIT
//! Tests for storage TTL rent policy enforcement (Issue #142).

use super::config_helpers::apply_max_stake;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{DataKeyCore, DataKeyExt, DataKeyScoped};
use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

#[test]
fn test_schema_version_and_admin_ttl_extended_on_interaction() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();

    // Initialize the contract
    client.initialize(&admin, &oracle);

    // SchemaVersion and Admin are long-lived keys.
    // Verify they are extended to BUMP_AMOUNT (518_400 ledgers)
    let schema_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKeyCore::SchemaVersion)
    });
    assert!(schema_ttl >= 518_400);

    let admin_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKeyCore::Admin)
    });
    assert!(admin_ttl >= 518_400);
}

#[test]
fn test_balance_ttl_extended_on_mint_and_query() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    env.mock_all_auths();

    // Call mint_initial, which writes to user's Balance
    client.mint_initial(&user);

    // Verify balance key is extended to BUMP_AMOUNT
    let balance_ttl = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKeyScoped::Balance(user.clone()))
    });
    assert!(balance_ttl >= 518_400);

    // Query balance, which also reads and extends TTL
    client.balance(&user);
    let balance_ttl_after = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKeyScoped::Balance(user.clone()))
    });
    assert!(balance_ttl_after >= 518_400);
}

#[test]
fn test_paused_and_configs_ttl_extended_on_calls() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Set and get max stake to trigger TTL extensions
    apply_max_stake(&env, &client, Some(100_000_000_000));
    client.get_max_stake();

    let max_stake_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKeyCore::MaxStake)
    });
    assert!(max_stake_ttl >= 518_400);

    // Paused config key checked in ensure_not_paused
    let paused_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKeyCore::Paused)
    });
    assert!(paused_ttl >= 518_400);
}

#[test]
fn test_oracle_and_heartbeat_ttl_extended() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Trigger heartbeat which updates OracleHeartbeat
    client.update_oracle_heartbeat(&0u32); // status 0 (online)

    let heartbeat_ttl = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKeyCore::OracleHeartbeat)
    });
    assert!(heartbeat_ttl >= 518_400);

    let oracle_ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKeyCore::Oracle)
    });
    assert!(oracle_ttl >= 518_400);
}

#[test]
fn test_batch_touch_ttl_touches_allowlisted_keys() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Build a list of allowlisted keys that should exist after init.
    let keys: Vec<DataKeyCore> = Vec::from_array(
        &env,
        [
            DataKeyCore::Admin,
            DataKeyCore::Oracle,
            DataKeyCore::SchemaVersion,
            DataKeyCore::Paused,
            DataKeyCore::BetWindowLedgers,
            DataKeyCore::RunWindowLedgers,
        ],
    );

    let touched = client.batch_touch_ttl(&keys);
    assert_eq!(touched, 6, "all 6 keys exist and should be touched");

    // Verify each key now has a fresh TTL
    for key in keys.iter() {
        let ttl = env.as_contract(&contract_id, || {
            env.storage().persistent().get_ttl(&key)
        });
        assert!(
            ttl >= 518_400,
            "TTL for key should be bumped to at least BUMP_AMOUNT"
        );
    }
}

#[test]
fn test_batch_touch_ttl_skips_absent_keys() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Include keys that are allowlisted but not yet set (e.g., CloseBufferLedgers,
    // MaxStake — not set by initialize).
    let keys: Vec<DataKeyCore> = Vec::from_array(
        &env,
        [
            DataKeyCore::Admin,
            DataKeyCore::CloseBufferLedgers, // not set during init
            DataKeyCore::MaxStake,            // not set during init
            DataKeyCore::OracleQuorum,
            DataKeyCore::DisputeLedgers,
            DataKeyCore::Ext(DataKeyExt::LeaderboardWins),
            DataKeyCore::Ext(DataKeyExt::LeaderboardStreak),
            DataKeyCore::Ext(DataKeyExt::SeasonId),
            DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardWins),
            DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardStreak),
        ],
    );

    let touched = client.batch_touch_ttl(&keys);
    // Only Admin exists; CloseBufferLedgers and MaxStake are absent from storage.
    assert_eq!(touched, 1, "only Admin exists among the supplied keys");
}

#[test]
fn test_batch_touch_ttl_rejects_non_allowlisted_key() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // ActiveRound is NOT in the allowlist (it is ephemeral round state).
    let keys: Vec<DataKeyCore> = Vec::from_array(&env, [DataKeyCore::ActiveRound]);

    let result = client.try_batch_touch_ttl(&keys);
    assert!(
        result.is_err(),
        "non-allowlisted key should be rejected"
    );
}

#[test]
fn test_batch_touch_ttl_rejects_paused_contract() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.pause_contract();

    let keys: Vec<DataKeyCore> = Vec::from_array(&env, [DataKeyCore::Admin]);
    let result = client.try_batch_touch_ttl(&keys);
    assert!(
        result.is_err(),
        "batch_touch_ttl should fail when contract is paused"
    );
}

/// Ensures every key in the Ext/leaderboard/quorum/dispute family is accepted
/// by the batch_touch_ttl allowlist (issue #423). The keys are absent from
/// storage after a plain initialize so they are expected to be *skipped* (not
/// touched), but the call must not be rejected.
#[test]
fn test_batch_touch_ttl_accepts_ext_leaderboard_quorum_dispute_keys() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // All keys are allowlisted but none exist yet after bare init.
    let keys: Vec<DataKeyCore> = Vec::from_array(
        &env,
        [
            DataKeyCore::OracleQuorum,
            DataKeyCore::DisputeLedgers,
            DataKeyCore::Ext(DataKeyExt::LeaderboardWins),
            DataKeyCore::Ext(DataKeyExt::LeaderboardStreak),
            DataKeyCore::Ext(DataKeyExt::SeasonId),
            DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardWins),
            DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardStreak),
        ],
    );

    // None exist yet → all skipped, none touched; the call itself must succeed.
    let touched = client.batch_touch_ttl(&keys);
    assert_eq!(touched, 0, "newly allowlisted keys absent from storage should be skipped");
}

/// Ensures the new governance/config keys added in issue #423 are accepted
/// by the batch_touch_ttl allowlist. After setting each key the call must
/// touch it and return a count ≥ 1.
#[test]
fn test_batch_touch_ttl_accepts_new_config_and_gov_keys() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // Write each new key so it exists in storage, then assert batch_touch_ttl
    // can bump all of them without an allowlist rejection.
    client.set_gov_approver(&Address::generate(&env));
    client.set_gov_proposal_ttl(&200u32);
    client.set_access_control_enabled(&true);

    let keys: Vec<DataKeyCore> = Vec::from_array(
        &env,
        [
            DataKeyCore::GovApprover,
            DataKeyCore::GovProposalTtlLedgers,
            DataKeyCore::AccessControlEnabled,
            // Absent-but-allowlisted: touching skips them gracefully.
            DataKeyCore::EpochMintBudget,
            DataKeyCore::EarlyCashoutBps,
            DataKeyCore::FeeModel,
            DataKeyCore::PrecisionPayoutPolicy,
            DataKeyCore::MinBet,
            DataKeyCore::NextGovProposalId,
        ],
    );

    let touched = client.batch_touch_ttl(&keys);
    // GovApprover, GovProposalTtlLedgers, AccessControlEnabled exist → 3 touched.
    assert_eq!(touched, 3, "only keys written to storage should be counted as touched");

    // Verify the touched keys have fresh TTLs.
    let gov_approver_ttl = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKeyCore::GovApprover)
    });
    assert!(gov_approver_ttl >= 518_400, "GovApprover TTL should be bumped");

    let gov_ttl_ttl = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKeyCore::GovProposalTtlLedgers)
    });
    assert!(gov_ttl_ttl >= 518_400, "GovProposalTtlLedgers TTL should be bumped");

    let ac_ttl = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKeyCore::AccessControlEnabled)
    });
    assert!(ac_ttl >= 518_400, "AccessControlEnabled TTL should be bumped");
}
