// SPDX-License-Identifier: MIT
//! Sybil faucet abuse — attacker spins up many identities to drain `mint_initial`.

use super::{emit_result, setup_contract};
use crate::errors::ContractError;
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Attacker creates sybil addresses faster than the per-ledger mint rate limit.
/// Defense: third mint in the same ledger is rejected; no funds minted.
#[test]
fn test_critical_sybil_faucet_abuse_mint_limit() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    client.set_mint_limit(&2);

    let sybil_1 = Address::generate(&env);
    let sybil_2 = Address::generate(&env);
    let sybil_3 = Address::generate(&env);

    assert_eq!(client.mint_initial(&sybil_1), 1000_0000000);
    assert_eq!(client.mint_initial(&sybil_2), 1000_0000000);

    let blocked = client.try_mint_initial(&sybil_3);
    assert!(blocked.is_err());
    assert_eq!(client.balance(&sybil_3), 0);

    emit_result(
        "sybil_faucet_mint_limit",
        "pass",
        "MintLimitExceeded",
        "none",
        "info",
        true,
    );
}

/// Attacker drains the epoch mint budget across sybil identities.
/// Defense: epoch budget caps total faucet outflow per epoch.
#[test]
fn test_sybil_faucet_abuse_epoch_budget() {
    let env = Env::default();
    let (client, _cid, _admin, _oracle) = setup_contract(&env);

    client.set_epoch_mint_budget(&2000_0000000);

    let sybil_1 = Address::generate(&env);
    let sybil_2 = Address::generate(&env);
    let sybil_3 = Address::generate(&env);

    client.mint_initial(&sybil_1);
    client.mint_initial(&sybil_2);

    let blocked = client.try_mint_initial(&sybil_3);
    assert!(blocked.is_err());

    emit_result(
        "sybil_faucet_epoch_budget",
        "pass",
        "EpochBudgetExceeded",
        "none when budget configured",
        "info",
        false,
    );
}
