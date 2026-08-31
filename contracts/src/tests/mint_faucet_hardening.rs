// SPDX-License-Identifier: MIT
//! Faucet hardening: `mint_initial` rate limiting via `MintLimitConfig`
//! (per-ledger cap) and `EpochMintBudget` (per-epoch total cap).
//!
//! Both configs are stored in persistent storage (not instance storage) so
//! they participate in the standard on-access TTL extension path and the
//! operator `batch_touch_ttl` allowlist like every other admin-configured
//! limit — see `docs/storage_lifecycle.md`.

use crate::common::EPOCH_LEDGERS;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::DataKeyCore;
use soroban_sdk::testutils::storage::Persistent as _;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

const TTL_BUMP_AMOUNT: u32 = 518_400;

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    (client, contract_id)
}

// ─── Mint limit (per-ledger cap) ────────────────────────────────────────────

#[test]
fn test_mint_limit_zero_is_unlimited() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);
    assert_eq!(client.get_mint_limit(), 0);

    for _ in 0..5 {
        let user = Address::generate(&env);
        client.mint_initial(&user);
    }
}

#[test]
fn test_mint_limit_exceeded_within_same_ledger() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);
    client.set_mint_limit(&2);
    assert_eq!(client.get_mint_limit(), 2);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    client.mint_initial(&user1);
    client.mint_initial(&user2);

    let result = client.try_mint_initial(&user3);
    assert_eq!(
        result,
        Err(Ok(ContractError::MintLimitExceeded)),
        "third mint in the same ledger must be rejected once the limit is hit"
    );

    // The two successful mints are unaffected by the rejected third attempt.
    assert_eq!(client.balance(&user1), 1000_0000000);
    assert_eq!(client.balance(&user2), 1000_0000000);
    assert_eq!(client.balance(&user3), 0);
}

#[test]
fn test_mint_limit_resets_on_next_ledger() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);
    client.set_mint_limit(&1);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    client.mint_initial(&user1);
    assert_eq!(
        client.try_mint_initial(&user2),
        Err(Ok(ContractError::MintLimitExceeded))
    );

    // A new ledger gets a fresh `LedgerMintCounter` — the cap does not carry over.
    env.ledger().with_mut(|l| l.sequence_number += 1);
    client.mint_initial(&user2);
    assert_eq!(client.balance(&user2), 1000_0000000);
}

#[test]
fn test_mint_limit_config_uses_persistent_storage_with_ttl() {
    let env = Env::default();
    let (client, contract_id) = setup(&env);
    client.set_mint_limit(&3);

    // Must be readable via persistent storage directly (not instance storage)
    // so it is covered by the standard TTL-extension and batch-touch paths.
    env.as_contract(&contract_id, || {
        let ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKeyCore::MintLimitConfig);
        assert!(ttl >= TTL_BUMP_AMOUNT);
    });

    // The admin batch-touch allowlist covers it too.
    let keys = soroban_sdk::Vec::from_array(&env, [DataKeyCore::MintLimitConfig]);
    let touched = client.batch_touch_ttl(&keys);
    assert_eq!(touched, 1);
}

// ─── Epoch mint budget (per-epoch total cap) ────────────────────────────────

#[test]
fn test_epoch_mint_budget_zero_is_unlimited() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);
    assert_eq!(client.get_epoch_mint_budget(), 0);

    for _ in 0..5 {
        let user = Address::generate(&env);
        client.mint_initial(&user);
    }
}

#[test]
fn test_epoch_mint_budget_exceeded() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);
    // Budget covers exactly 2 mints (1000 vXLM each).
    client.set_epoch_mint_budget(&2000_0000000);
    assert_eq!(client.get_epoch_mint_budget(), 2000_0000000);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    client.mint_initial(&user1);
    client.mint_initial(&user2);

    let result = client.try_mint_initial(&user3);
    assert_eq!(
        result,
        Err(Ok(ContractError::EpochBudgetExceeded)),
        "a third mint must be rejected once the epoch budget is exhausted"
    );
    assert_eq!(client.balance(&user3), 0);
}

#[test]
fn test_epoch_mint_budget_resets_on_next_epoch() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);
    client.set_epoch_mint_budget(&1000_0000000);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    client.mint_initial(&user1);
    assert_eq!(
        client.try_mint_initial(&user2),
        Err(Ok(ContractError::EpochBudgetExceeded))
    );

    // Advance past the epoch boundary — the consumed counter must reset.
    env.ledger()
        .with_mut(|l| l.sequence_number += EPOCH_LEDGERS);
    client.mint_initial(&user2);
    assert_eq!(client.balance(&user2), 1000_0000000);
}

#[test]
fn test_epoch_mint_budget_setter_rejects_negative() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);
    let result = client.try_set_epoch_mint_budget(&(-1));
    assert_eq!(result, Err(Ok(ContractError::InvalidBetAmount)));
}

#[test]
fn test_epoch_mint_budget_config_uses_persistent_storage_with_ttl() {
    let env = Env::default();
    let (client, contract_id) = setup(&env);
    client.set_epoch_mint_budget(&5000_0000000);

    env.as_contract(&contract_id, || {
        let ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKeyCore::EpochMintBudget);
        assert!(ttl >= TTL_BUMP_AMOUNT);
    });

    let keys = soroban_sdk::Vec::from_array(&env, [DataKeyCore::EpochMintBudget]);
    let touched = client.batch_touch_ttl(&keys);
    assert_eq!(touched, 1);
}

// ─── Combined: both limits active at once ───────────────────────────────────

#[test]
fn test_mint_limit_and_epoch_budget_combine() {
    let env = Env::default();
    let (client, _contract_id) = setup(&env);
    // Per-ledger cap of 5 (generous), but epoch budget only covers 2 mints —
    // the tighter constraint (epoch budget) should be the one that trips.
    client.set_mint_limit(&5);
    client.set_epoch_mint_budget(&2000_0000000);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    client.mint_initial(&user1);
    client.mint_initial(&user2);

    assert_eq!(
        client.try_mint_initial(&user3),
        Err(Ok(ContractError::EpochBudgetExceeded))
    );
}
