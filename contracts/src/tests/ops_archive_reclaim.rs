// SPDX-License-Identifier: MIT
//! Executable checks for the archive-retention and expired-pending-winnings
//! operator playbook (Issue #571, `docs/OPS_ARCHIVE_RECLAIM_PLAYBOOK.md`).
//!
//! Every command and failure mode the playbook documents is exercised here
//! so the runbook cannot silently drift from contract behaviour.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, ConfigChangeKind, DataKeyScoped, RoundStatus};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, Symbol, TryFromVal,
};

fn setup() -> (Env, VirtualTokenContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.sequence_number = 1_000);
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, client, contract_id, admin)
}

fn next_ledger(env: &Env, by: u32) {
    env.ledger().with_mut(|li| li.sequence_number += by);
}

/// Opens and cancels a round (reason 0), archiving it. Returns its round id.
fn archive_one(env: &Env, client: &VirtualTokenContractClient<'_>) -> u64 {
    next_ledger(env, 1);
    client.create_round(&1_0000000, &None);
    let id = client.get_active_round().unwrap().round_id;
    client.cancel_round(&0u32);
    id
}

fn pruned_ids_in_last_call(env: &Env, contract_id: &Address) -> std::vec::Vec<(u64, u32)> {
    let mut out = std::vec::Vec::new();
    for (c, topics, data) in env.events().all().iter() {
        if &c != contract_id || topics.len() != 2 {
            continue;
        }
        let t0 = Symbol::try_from_val(env, &topics.get(0).unwrap()).unwrap();
        let t1 = Symbol::try_from_val(env, &topics.get(1).unwrap()).unwrap();
        if t0 == symbol_short!("archive") && t1 == symbol_short!("pruned") {
            out.push(<(u64, u32)>::try_from_val(env, &data).unwrap());
        }
    }
    out
}

fn has_archive_entry(env: &Env, contract_id: &Address, round_id: u64) -> bool {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .has(&DataKeyScoped::ArchivedRound(round_id))
    })
}

/// Schedules the expiry, waits out the timelock and applies it — exactly the
/// three playbook commands.
fn enable_expiry(env: &Env, client: &VirtualTokenContractClient<'_>, ledgers: u32) {
    client.schedule_pending_winnings_expiry(&ledgers);
    let pending = client
        .get_pending_config_change(&ConfigChangeKind::PendingWinningsExpiry)
        .expect("scheduled");
    // Applying before activation fails with RoundNotEnded (#16).
    assert_eq!(
        client.try_apply_scheduled_changes(&ConfigChangeKind::PendingWinningsExpiry),
        Err(Ok(ContractError::RoundNotEnded))
    );
    env.ledger()
        .with_mut(|li| li.sequence_number = pending.activation_ledger);
    client.apply_scheduled_changes(&ConfigChangeKind::PendingWinningsExpiry);
    assert_eq!(client.get_pending_winnings_expiry(), ledgers);
}

// ─── Archive retention ───────────────────────────────────────────────────────

/// Retention N keeps exactly N archived rounds and deletes every pruned
/// entry (no orphaned `ArchivedRound` keys).
#[test]
fn archive_prune_keeps_exactly_retention_and_leaves_no_orphans() {
    let (env, client, contract_id, _admin) = setup();
    client.set_archive_retention(&2u32);

    let ids: std::vec::Vec<u64> = (0..5).map(|_| archive_one(&env, &client)).collect();

    let recent = client.get_recent_archived_rounds(&10u32);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent.get(0).unwrap().round_id, ids[4]); // newest first
    assert_eq!(recent.get(1).unwrap().round_id, ids[3]);

    for (i, id) in ids.iter().enumerate() {
        let kept = i >= 3;
        assert_eq!(
            has_archive_entry(&env, &contract_id, *id),
            kept,
            "round {}",
            id
        );
        assert_eq!(client.get_archived_round(id).is_some(), kept);
    }
    // A pruned cancelled round loses its cancel marker too → Unknown.
    assert_eq!(client.get_round_status(&ids[0]), RoundStatus::Unknown);
    assert_eq!(client.get_round_status(&ids[4]), RoundStatus::Cancelled);
}

/// Lowering retention does nothing until the next archive write, which then
/// prunes the whole backlog and emits one `("archive","pruned")` per round.
#[test]
fn lowering_retention_prunes_backlog_on_next_archive_write() {
    let (env, client, contract_id, _admin) = setup();
    client.set_archive_retention(&5u32);
    let ids: std::vec::Vec<u64> = (0..5).map(|_| archive_one(&env, &client)).collect();

    client.set_archive_retention(&2u32);
    assert_eq!(client.get_archive_retention(), 2);
    // Nothing is deleted by the setter itself.
    for id in &ids {
        assert!(has_archive_entry(&env, &contract_id, *id));
    }

    let newest = archive_one(&env, &client);
    let pruned = pruned_ids_in_last_call(&env, &contract_id);
    assert_eq!(
        pruned,
        std::vec![(ids[0], 2u32), (ids[1], 2), (ids[2], 2), (ids[3], 2)]
    );
    let recent = client.get_recent_archived_rounds(&10u32);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent.get(0).unwrap().round_id, newest);
    assert_eq!(recent.get(1).unwrap().round_id, ids[4]);
}

#[test]
fn set_archive_retention_failure_modes() {
    let (_env, client, _cid, _admin) = setup();
    assert_eq!(
        client.try_set_archive_retention(&0u32),
        Err(Ok(ContractError::WindowOutOfRange))
    );
    assert_eq!(
        client.try_set_archive_retention(&10_001u32),
        Err(Ok(ContractError::WindowOutOfRange))
    );
    // Allowed in ClaimsOnly, blocked in FullyPaused.
    client.set_runtime_mode(&1u32);
    client.set_archive_retention(&64u32);
    client.set_runtime_mode(&2u32);
    assert_eq!(
        client.try_set_archive_retention(&32u32),
        Err(Ok(ContractError::ContractPaused))
    );
    assert_eq!(client.get_archive_retention(), 64);
}

// ─── Expired pending winnings ────────────────────────────────────────────────

/// Happy path plus the documented error for each precondition.
#[test]
fn reclaim_expired_pending_winnings_playbook() {
    let (env, client, _cid, admin) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);

    // Credit a refund to `user` via a cancelled round.
    next_ledger(&env, 1);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    client.cancel_round(&0u32);
    assert_eq!(client.get_pending_winnings(&user), 100_0000000);

    // Expiry disabled (default 0).
    assert_eq!(client.get_pending_winnings_expiry(), 0);
    assert_eq!(
        client.try_reclaim_expired_pending_winnings(&user),
        Err(Ok(ContractError::ExpiryNotConfigured))
    );

    enable_expiry(&env, &client, 500);
    // The timelock (1440 ledgers) already aged the entry past 500 ledgers.
    let admin_before = client.balance(&admin);
    let reclaimed = client.reclaim_expired_pending_winnings(&user);
    assert_eq!(reclaimed, 100_0000000);
    assert_eq!(client.get_pending_winnings(&user), 0);
    // Funds move to the admin's vXLM balance, not the fee treasury.
    assert_eq!(client.balance(&admin), admin_before + reclaimed);

    // Nothing left to reclaim.
    assert_eq!(
        client.try_reclaim_expired_pending_winnings(&user),
        Err(Ok(ContractError::PendingWinningsNotFound))
    );
}

/// Any new credit refreshes the timer for the user's *whole* pending balance.
#[test]
fn new_credit_resets_expiry_timer() {
    let (env, client, _cid, _admin) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    enable_expiry(&env, &client, 500);

    next_ledger(&env, 1);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    client.cancel_round(&0u32);

    next_ledger(&env, 400);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &50_0000000, &BetSide::Up);
    client.cancel_round(&0u32); // refresh at +401

    // 500 ledgers after the first credit, only 100 after the second.
    next_ledger(&env, 100);
    assert_eq!(
        client.try_reclaim_expired_pending_winnings(&user),
        Err(Ok(ContractError::PendingWinningsNotExpired))
    );
    next_ledger(&env, 400);
    assert_eq!(client.reclaim_expired_pending_winnings(&user), 150_0000000);
}

/// Reclaim works in ClaimsOnly, is blocked when FullyPaused; applying a
/// scheduled expiry change needs Normal mode.
#[test]
fn reclaim_and_apply_respect_runtime_mode() {
    let (env, client, _cid, _admin) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    next_ledger(&env, 1);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    client.cancel_round(&0u32);

    client.schedule_pending_winnings_expiry(&500u32);
    let pending = client
        .get_pending_config_change(&ConfigChangeKind::PendingWinningsExpiry)
        .unwrap();
    env.ledger()
        .with_mut(|li| li.sequence_number = pending.activation_ledger);

    client.set_runtime_mode(&1u32);
    assert_eq!(
        client.try_apply_scheduled_changes(&ConfigChangeKind::PendingWinningsExpiry),
        Err(Ok(ContractError::ContractPaused))
    );
    client.set_runtime_mode(&0u32);
    client.apply_scheduled_changes(&ConfigChangeKind::PendingWinningsExpiry);

    client.set_runtime_mode(&2u32);
    assert_eq!(
        client.try_reclaim_expired_pending_winnings(&user),
        Err(Ok(ContractError::ContractPaused))
    );
    client.set_runtime_mode(&1u32);
    assert_eq!(client.reclaim_expired_pending_winnings(&user), 100_0000000);
}

#[test]
fn schedule_expiry_validation() {
    let (_env, client, _cid, _admin) = setup();
    // Outside [128, 1_000_000] (0 = disable is allowed).
    assert_eq!(
        client.try_schedule_pending_winnings_expiry(&127u32),
        Err(Ok(ContractError::InvalidDuration))
    );
    assert_eq!(
        client.try_schedule_pending_winnings_expiry(&1_000_001u32),
        Err(Ok(ContractError::InvalidDuration))
    );
    client.schedule_pending_winnings_expiry(&128u32);
    // Only one pending change per kind.
    assert_eq!(
        client.try_schedule_pending_winnings_expiry(&256u32),
        Err(Ok(ContractError::RoundAlreadyActive))
    );
    client.cancel_config_change(&ConfigChangeKind::PendingWinningsExpiry);
    client.schedule_pending_winnings_expiry(&0u32);
}

/// Pruning hides a user's outcome from `get_user_archived_participation`,
/// but the per-user `UserRoundOutcome` record itself stays in storage.
#[test]
fn prune_hides_user_outcome_but_keeps_record() {
    let (env, client, contract_id, _admin) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.set_archive_retention(&1u32);

    next_ledger(&env, 1);
    client.create_round(&1_0000000, &None);
    let first = client.get_active_round().unwrap().round_id;
    client.place_bet(&user, &10_0000000, &BetSide::Up);
    client.cancel_round(&0u32);
    assert!(client
        .get_user_archived_participation(&user, &first)
        .is_some());

    archive_one(&env, &client); // prunes `first`
    assert!(client.get_archived_round(&first).is_none());
    assert!(client
        .get_user_archived_participation(&user, &first)
        .is_none());
    let record_kept = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .has(&DataKeyScoped::UserRoundOutcome(first, user.clone()))
    });
    assert!(
        record_kept,
        "prune does not delete per-user outcome records"
    );
}
