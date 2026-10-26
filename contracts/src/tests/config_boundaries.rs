// SPDX-License-Identifier: MIT
//! Boundary tests for the protocol-fee bps validation guard (Issue #671).
//!
//! `_validate_protocol_fee_bps` (contracts/src/config.rs) enforces:
//!   - `None` is always allowed (disables the fee entirely);
//!   - `Some(0)` is rejected — only `None` means "fee disabled";
//!   - `Some(bps)` must satisfy `1 <= bps <= MAX_PROTOCOL_FEE_BPS`.
//!
//! Before these tests, no active test module exercised the boundary: the only
//! protocol-fee scheduling tests live in the disabled `config_timelock`
//! module, and they used only the mid-range value 500. These tests pin the
//! exact acceptance boundary — `MAX_PROTOCOL_FEE_BPS` itself is accepted,
//! `MAX_PROTOCOL_FEE_BPS + 1` (and the out-of-range 10_001 from the issue
//! title) is rejected with `InvalidProtocolFeeBps` — and verify that a
//! rejected schedule leaves no pending timelock entry behind.

use crate::common::MAX_PROTOCOL_FEE_BPS;
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::ConfigChangeKind;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

/// The concrete out-of-range boundary named in issue #671 (10_000 + 1 bps).
const ISSUE_BOUNDARY_BPS: u32 = 10_001;

fn setup() -> (Env, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, client)
}

/// `Some(MAX_PROTOCOL_FEE_BPS)` is the last accepted value: it schedules
/// normally and, once the timelock elapses, applies and reads back exactly.
#[test]
fn test_protocol_fee_bps_max_boundary_accepted() {
    let (env, client) = setup();

    assert_eq!(client.get_protocol_fee_bps(), None);

    client.schedule_protocol_fee_bps(&Some(MAX_PROTOCOL_FEE_BPS));

    let pending = client
        .get_pending_config_change(&ConfigChangeKind::ProtocolFeeBps)
        .expect("max-boundary schedule must create a pending timelock entry");
    assert_eq!(
        pending.payload,
        crate::types::ConfigChangePayload::ProtocolFeeBps(Some(MAX_PROTOCOL_FEE_BPS))
    );

    // Advance to the activation ledger and apply.
    env.ledger().with_mut(|li| {
        li.sequence_number = pending.activation_ledger;
    });
    client.apply_scheduled_changes(&ConfigChangeKind::ProtocolFeeBps);

    assert_eq!(
        client.get_protocol_fee_bps(),
        Some(MAX_PROTOCOL_FEE_BPS),
        "fee at exactly MAX_PROTOCOL_FEE_BPS must be accepted and applied"
    );
}

/// `Some(MAX_PROTOCOL_FEE_BPS + 1)` is the first rejected value: the call
/// fails with `InvalidProtocolFeeBps` and leaves no pending timelock entry.
#[test]
fn test_protocol_fee_bps_max_plus_one_rejected() {
    let (_env, client) = setup();

    let result = client.try_schedule_protocol_fee_bps(&(Some(MAX_PROTOCOL_FEE_BPS + 1)));
    assert_eq!(
        result,
        Err(Ok(ContractError::InvalidProtocolFeeBps)),
        "max+1 bps must be rejected with InvalidProtocolFeeBps"
    );

    // Rejection must be side-effect free: no pending entry, no active fee.
    assert!(
        client
            .get_pending_config_change(&ConfigChangeKind::ProtocolFeeBps)
            .is_none(),
        "rejected schedule must not leave a pending timelock entry"
    );
    assert_eq!(client.get_protocol_fee_bps(), None);
}

/// The explicit out-of-range value from the issue title (10_001 bps) is
/// rejected with the same error.
#[test]
fn test_protocol_fee_bps_above_max_10001_rejected() {
    let (_env, client) = setup();

    let result = client.try_schedule_protocol_fee_bps(&Some(ISSUE_BOUNDARY_BPS));
    assert_eq!(
        result,
        Err(Ok(ContractError::InvalidProtocolFeeBps)),
        "10_001 bps is far above the cap and must be rejected"
    );
    assert!(client
        .get_pending_config_change(&ConfigChangeKind::ProtocolFeeBps)
        .is_none());
    assert_eq!(client.get_protocol_fee_bps(), None);
}

/// `Some(0)` is rejected: zero is not a legitimate fee value, only `None`
/// disables the fee.
#[test]
fn test_protocol_fee_bps_zero_rejected() {
    let (_env, client) = setup();

    let result = client.try_schedule_protocol_fee_bps(&Some(0u32));
    assert_eq!(
        result,
        Err(Ok(ContractError::InvalidProtocolFeeBps)),
        "Some(0) must be rejected — only None disables the fee"
    );
    assert!(client
        .get_pending_config_change(&ConfigChangeKind::ProtocolFeeBps)
        .is_none());
    assert_eq!(client.get_protocol_fee_bps(), None);
}

/// A rejected schedule must not clobber an already-pending or already-active
/// fee: the guard runs before any storage mutation.
#[test]
fn test_protocol_fee_bps_rejection_preserves_existing_state() {
    let (env, client) = setup();

    // Put an accepted fee in place through the full timelock cycle.
    client.schedule_protocol_fee_bps(&Some(500u32));
    let pending = client
        .get_pending_config_change(&ConfigChangeKind::ProtocolFeeBps)
        .expect("accepted schedule must be pending");
    env.ledger().with_mut(|li| {
        li.sequence_number = pending.activation_ledger;
    });
    client.apply_scheduled_changes(&ConfigChangeKind::ProtocolFeeBps);
    assert_eq!(client.get_protocol_fee_bps(), Some(500u32));

    // Now attempt an out-of-range reschedule: it must fail atomically.
    let result = client.try_schedule_protocol_fee_bps(&(Some(MAX_PROTOCOL_FEE_BPS + 1)));
    assert_eq!(result, Err(Ok(ContractError::InvalidProtocolFeeBps)));

    assert_eq!(
        client.get_protocol_fee_bps(),
        Some(500u32),
        "active fee must be untouched by a rejected reschedule"
    );
    assert!(
        client
            .get_pending_config_change(&ConfigChangeKind::ProtocolFeeBps)
            .is_none(),
        "rejected reschedule must not create a new pending entry"
    );
}

/// `None` (fee disabled) is always accepted, including as a reschedule of an
/// active fee.
#[test]
fn test_protocol_fee_bps_none_always_accepted() {
    let (_env, client) = setup();

    client.schedule_protocol_fee_bps(&None);

    let pending = client
        .get_pending_config_change(&ConfigChangeKind::ProtocolFeeBps)
        .expect("None must schedule like any other value");
    assert_eq!(
        pending.payload,
        crate::types::ConfigChangePayload::ProtocolFeeBps(None)
    );
    assert_eq!(
        client.get_protocol_fee_bps(),
        None,
        "scheduled-but-not-applied None must not leak into the active fee"
    );
}
