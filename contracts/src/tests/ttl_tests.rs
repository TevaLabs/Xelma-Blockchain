// SPDX-License-Identifier: MIT
//! Tests for storage TTL rent policy enforcement (Issue #142).

use super::config_helpers::apply_max_stake;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{DataKeyCore, DataKeyScoped};
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
        env.storage()
            .persistent()
            .get_ttl(&DataKeyCore::SchemaVersion)
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
        let ttl = env.as_contract(&contract_id, || env.storage().persistent().get_ttl(&key));
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
            DataKeyCore::MaxStake,           // not set during init
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
    assert!(result.is_err(), "non-allowlisted key should be rejected");
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

// ─── TTL touch allowlist completeness (Issue #862) ──────────────────────────

use crate::errors::ContractError;
use crate::storage::{ttl_touch_policy, TtlTouchPolicy};
use crate::types::DataKeyExt;

/// Every long-lived, contract-wide persistent singleton, with the name the
/// docs table uses. Keep in sync with `storage::ttl_touch_policy`.
fn expected_persistent_allowlist() -> std::vec::Vec<(&'static str, DataKeyCore)> {
    std::vec![
        ("Admin", DataKeyCore::Admin),
        ("Oracle", DataKeyCore::Oracle),
        ("SchemaVersion", DataKeyCore::SchemaVersion),
        ("NextSchemaVersion", DataKeyCore::NextSchemaVersion),
        ("Paused", DataKeyCore::Paused),
        ("MigratedToV3", DataKeyCore::MigratedToV3),
        ("BetWindowLedgers", DataKeyCore::BetWindowLedgers),
        ("RunWindowLedgers", DataKeyCore::RunWindowLedgers),
        ("CloseBufferLedgers", DataKeyCore::CloseBufferLedgers),
        ("MaxStake", DataKeyCore::MaxStake),
        ("MaxUserRoundExposure", DataKeyCore::MaxUserRoundExposure),
        ("MaxPendingWinnings", DataKeyCore::MaxPendingWinnings),
        ("MinBet", DataKeyCore::MinBet),
        ("MinParticipants", DataKeyCore::MinParticipants),
        (
            "MaxPrecisionParticipants",
            DataKeyCore::MaxPrecisionParticipants
        ),
        ("OracleHeartbeat", DataKeyCore::OracleHeartbeat),
        ("OracleStaleThreshold", DataKeyCore::OracleStaleThreshold),
        ("OracleMaxDeviationBps", DataKeyCore::OracleMaxDeviationBps),
        (
            "OracleDeviationOverrideArmed",
            DataKeyCore::OracleDeviationOverrideArmed
        ),
        (
            "OracleMinConfidenceBps",
            DataKeyCore::OracleMinConfidenceBps
        ),
        ("OracleStrictMode", DataKeyCore::OracleStrictMode),
        ("OracleQuorum", DataKeyCore::OracleQuorum),
        (
            "OracleRotationProposal",
            DataKeyCore::OracleRotationProposal
        ),
        ("ProtocolFeeBps", DataKeyCore::ProtocolFeeBps),
        ("ProtocolFeeTreasury", DataKeyCore::ProtocolFeeTreasury),
        ("FeeModel", DataKeyCore::FeeModel),
        ("EarlyCashoutBps", DataKeyCore::EarlyCashoutBps),
        ("PrecisionPayoutPolicy", DataKeyCore::PrecisionPayoutPolicy),
        ("ArchiveRetention", DataKeyCore::ArchiveRetention),
        (
            "RecentArchivedRoundIds",
            DataKeyCore::RecentArchivedRoundIds
        ),
        ("RoundTemplate", DataKeyCore::RoundTemplate),
        ("LastRoundId", DataKeyCore::LastRoundId),
        ("AccessControlEnabled", DataKeyCore::AccessControlEnabled),
        ("GovApprover", DataKeyCore::GovApprover),
        ("GovProposalTtlLedgers", DataKeyCore::GovProposalTtlLedgers),
        ("NextGovProposalId", DataKeyCore::NextGovProposalId),
        (
            "LeaderboardWins",
            DataKeyCore::Ext(DataKeyExt::LeaderboardWins)
        ),
        (
            "LeaderboardStreak",
            DataKeyCore::Ext(DataKeyExt::LeaderboardStreak)
        ),
        ("SeasonId", DataKeyCore::Ext(DataKeyExt::SeasonId)),
        (
            "SeasonLeaderboardWins",
            DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardWins)
        ),
        (
            "SeasonLeaderboardStreak",
            DataKeyCore::Ext(DataKeyExt::SeasonLeaderboardStreak)
        ),
        (
            "ConstitutionMetadata",
            DataKeyCore::Ext(DataKeyExt::ConstitutionMetadata)
        ),
        (
            "NextAmendmentId",
            DataKeyCore::Ext(DataKeyExt::NextAmendmentId)
        ),
    ]
}

fn excluded_keys(env: &Env) -> std::vec::Vec<(&'static str, DataKeyCore, TtlTouchPolicy)> {
    let user = Address::generate(env);
    std::vec![
        (
            "ActiveRound",
            DataKeyCore::ActiveRound,
            TtlTouchPolicy::RoundScoped
        ),
        ("Positions", DataKeyCore::Positions, TtlTouchPolicy::Legacy),
        (
            "UpDownPositions",
            DataKeyCore::UpDownPositions,
            TtlTouchPolicy::Legacy
        ),
        (
            "PrecisionPositions",
            DataKeyCore::PrecisionPositions,
            TtlTouchPolicy::Legacy
        ),
        (
            "EpochMintBudget",
            DataKeyCore::EpochMintBudget,
            TtlTouchPolicy::NotStoredUnderVariant
        ),
        (
            "DisputeLedgers",
            DataKeyCore::DisputeLedgers,
            TtlTouchPolicy::NotStoredUnderVariant
        ),
        (
            "SeasonUserStats",
            DataKeyCore::Ext(DataKeyExt::SeasonUserStats(1, user)),
            TtlTouchPolicy::Indexed,
        ),
        (
            "SeasonArchive",
            DataKeyCore::Ext(DataKeyExt::SeasonArchive(1)),
            TtlTouchPolicy::Indexed
        ),
        (
            "Amendment",
            DataKeyCore::Ext(DataKeyExt::Amendment(1)),
            TtlTouchPolicy::Indexed
        ),
    ]
}

fn setup_touch() -> (Env, Address, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    (env, contract_id, client)
}

#[test]
fn test_ttl_touch_policy_classifies_expected_keys() {
    let env = Env::default();
    for (name, key) in expected_persistent_allowlist() {
        assert_eq!(
            ttl_touch_policy(&key),
            TtlTouchPolicy::Allowed,
            "{name} should be Allowed"
        );
    }
    assert_eq!(
        ttl_touch_policy(&DataKeyCore::MintLimitConfig),
        TtlTouchPolicy::AllowedInstanceTier
    );
    for (name, key, policy) in excluded_keys(&env) {
        assert_eq!(ttl_touch_policy(&key), policy, "{name} classification");
    }
}

#[test]
fn test_batch_touch_ttl_extends_every_allowlisted_persistent_key() {
    let (env, contract_id, client) = setup_touch();
    let expected = expected_persistent_allowlist();

    // Seed every allowlisted key that initialize() did not write, so each one
    // exists in persistent storage with a short TTL.
    env.as_contract(&contract_id, || {
        for (_, key) in expected.iter() {
            if !env.storage().persistent().has(key) {
                env.storage().persistent().set(key, &1u32);
            }
        }
    });

    let mut keys: Vec<DataKeyCore> = Vec::new(&env);
    for (_, key) in expected.iter() {
        keys.push_back(key.clone());
    }
    let touched = client.batch_touch_ttl(&keys);
    assert_eq!(
        touched as usize,
        expected.len(),
        "every allowlisted key should be touched"
    );

    for (name, key) in expected.iter() {
        let ttl = env.as_contract(&contract_id, || env.storage().persistent().get_ttl(key));
        assert!(
            ttl >= 518_400,
            "TTL for {name} should be bumped to BUMP_AMOUNT, got {ttl}"
        );
    }
}

#[test]
fn test_batch_touch_ttl_rejects_every_excluded_key() {
    let (env, _contract_id, client) = setup_touch();
    for (name, key, _) in excluded_keys(&env) {
        let keys: Vec<DataKeyCore> = Vec::from_array(&env, [key]);
        let res = client.try_batch_touch_ttl(&keys);
        assert_eq!(
            res,
            Err(Ok(ContractError::UnsupportedDataKeyForTtlTouch)),
            "{name} must not be batch-touchable"
        );
    }
}

#[test]
fn test_batch_touch_ttl_mint_limit_config_is_instance_tier_and_skipped() {
    let (env, _contract_id, client) = setup_touch();
    client.set_mint_limit(&5u32);
    let keys: Vec<DataKeyCore> =
        Vec::from_array(&env, [DataKeyCore::MintLimitConfig, DataKeyCore::Admin]);
    // Accepted (no error), but only Admin is a persistent entry.
    assert_eq!(client.batch_touch_ttl(&keys), 1);
}

#[test]
fn test_storage_lifecycle_doc_lists_exactly_the_allowlist() {
    let doc = include_str!("../../../docs/storage_lifecycle.md");
    let start = doc
        .find("### Allowlisted Key Classes")
        .expect("allowlist section present");
    let end = start
        + doc[start..]
            .find("### Excluded Keys")
            .expect("excluded keys section present");
    let section = &doc[start..end];

    // Keys are the backticked names in the second column of each table row.
    let mut documented: std::vec::Vec<&str> = std::vec::Vec::new();
    for line in section.lines().filter(|l| l.starts_with('|')) {
        if let Some(keys_cell) = line.split('|').nth(2) {
            for part in keys_cell.split('`').skip(1).step_by(2) {
                documented.push(part);
            }
        }
    }
    documented.sort_unstable();
    documented.dedup();

    let mut expected: std::vec::Vec<&str> = expected_persistent_allowlist()
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    expected.push("MintLimitConfig");
    expected.sort_unstable();

    assert_eq!(
        documented, expected,
        "docs/storage_lifecycle.md allowlist table drifted"
    );
}
