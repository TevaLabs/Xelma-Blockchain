// SPDX-License-Identifier: MIT
//! Protocol-fee bps upper bound (Issue #807).
//!
//! Valid protocol-fee rates are `None` (fee disabled) or
//! `Some(1..=MAX_PROTOCOL_FEE_BPS)`, where `MAX_PROTOCOL_FEE_BPS = 1_000`
//! (10%). Every entry point that accepts a fee rate must reject
//! `MAX_PROTOCOL_FEE_BPS + 1` and anything above it, including `10_001`
//! (one past 100% in basis points) and `u32::MAX`, with exactly
//! `ContractError::InvalidProtocolFeeBps`, without writing to storage.

use super::ledger_snapshot::assert_ledger_unchanged;
use crate::common::MAX_PROTOCOL_FEE_BPS;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{ConfigChangeKind, GovAction, GovProposalStatus};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Out-of-range values: one past the cap, one past 100%, and the type max.
const OVER_CAP: [u32; 4] = [MAX_PROTOCOL_FEE_BPS + 1, 10_000, 10_001, u32::MAX];

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    (client, admin)
}

#[test]
fn test_schedule_protocol_fee_bps_rejects_max_plus_one_and_above() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    for bps in OVER_CAP {
        let r = assert_ledger_unchanged(&env, "schedule_protocol_fee_bps", || {
            client.try_schedule_protocol_fee_bps(&Some(bps))
        });
        assert_eq!(
            r,
            Err(Ok(ContractError::InvalidProtocolFeeBps)),
            "bps={bps}"
        );

        // `set_protocol_fee_bps` is an alias that schedules the same change.
        let r = assert_ledger_unchanged(&env, "set_protocol_fee_bps", || {
            client.try_set_protocol_fee_bps(&Some(bps))
        });
        assert_eq!(
            r,
            Err(Ok(ContractError::InvalidProtocolFeeBps)),
            "bps={bps}"
        );
    }

    assert!(
        client
            .get_pending_config_change(&ConfigChangeKind::ProtocolFeeBps)
            .is_none(),
        "a rejected value must not leave a pending change"
    );
    assert_eq!(client.get_protocol_fee_bps(), None);

    // The cap itself is still accepted.
    super::config_helpers::apply_protocol_fee_bps(&env, &client, Some(MAX_PROTOCOL_FEE_BPS));
    assert_eq!(client.get_protocol_fee_bps(), Some(MAX_PROTOCOL_FEE_BPS));
}

#[test]
fn test_rejected_fee_bps_does_not_replace_pending_or_active_value() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    super::config_helpers::apply_protocol_fee_bps(&env, &client, Some(250));
    client.schedule_protocol_fee_bps(&Some(MAX_PROTOCOL_FEE_BPS));
    let pending = client
        .get_pending_config_change(&ConfigChangeKind::ProtocolFeeBps)
        .expect("valid schedule is pending");

    for bps in OVER_CAP {
        let r = assert_ledger_unchanged(&env, "schedule_protocol_fee_bps", || {
            client.try_schedule_protocol_fee_bps(&Some(bps))
        });
        assert_eq!(
            r,
            Err(Ok(ContractError::InvalidProtocolFeeBps)),
            "bps={bps}"
        );
    }

    assert_eq!(client.get_protocol_fee_bps(), Some(250));
    assert_eq!(
        client.get_pending_config_change(&ConfigChangeKind::ProtocolFeeBps),
        Some(pending),
        "the earlier valid schedule is untouched"
    );
}

#[test]
fn test_early_cashout_bps_shares_protocol_fee_bound() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    for bps in OVER_CAP {
        let r = assert_ledger_unchanged(&env, "set_early_cashout_bps", || {
            client.try_set_early_cashout_bps(&Some(bps))
        });
        assert_eq!(
            r,
            Err(Ok(ContractError::InvalidProtocolFeeBps)),
            "bps={bps}"
        );
    }
    assert_eq!(client.get_early_cashout_bps(), None);

    client.set_early_cashout_bps(&Some(MAX_PROTOCOL_FEE_BPS));
    assert_eq!(client.get_early_cashout_bps(), Some(MAX_PROTOCOL_FEE_BPS));
}

/// Governance validates the payload when the proposal executes. An approved
/// `SetProtocolFeeBps(Some(10_001))` fails with `InvalidProtocolFeeBps`, and
/// both the active fee and the proposal are left as they were.
#[test]
fn test_gov_set_protocol_fee_bps_over_cap_rejected_on_execute() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let approver = Address::generate(&env);
    client.set_gov_approver(&approver);

    let ok = client.propose_gov_action(&admin, &GovAction::SetProtocolFeeBps(Some(300)), &None);
    client.approve_gov_proposal(&approver, &ok);
    client.execute_gov_proposal(&admin, &ok);
    assert_eq!(client.get_protocol_fee_bps(), Some(300));

    for bps in OVER_CAP {
        let pid =
            client.propose_gov_action(&admin, &GovAction::SetProtocolFeeBps(Some(bps)), &None);
        client.approve_gov_proposal(&approver, &pid);

        let r = assert_ledger_unchanged(&env, "execute_gov_proposal", || {
            client.try_execute_gov_proposal(&admin, &pid)
        });
        assert_eq!(
            r,
            Err(Ok(ContractError::InvalidProtocolFeeBps)),
            "bps={bps}"
        );
        assert_eq!(
            client.get_gov_proposal(&pid).map(|p| p.status),
            Some(GovProposalStatus::Approved),
            "bps={bps}: a failed execution must not mark the proposal executed"
        );
    }
    assert_eq!(client.get_protocol_fee_bps(), Some(300));
}
