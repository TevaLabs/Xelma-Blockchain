// SPDX-License-Identifier: MIT
//! Helpers to apply timelocked config immediately in tests.

use crate::contract::VirtualTokenContractClient;
use crate::types::ConfigChangeKind;
use soroban_sdk::{testutils::Ledger, Env};

fn activate_pending(env: &Env, client: &VirtualTokenContractClient, kind: ConfigChangeKind) {
    let ledger_before = env.ledger().sequence();
    let pending = client
        .get_pending_config_change(&kind)
        .expect("expected pending config change after schedule");
    env.ledger().with_mut(|li| {
        li.sequence_number = pending.activation_ledger;
    });
    client.apply_scheduled_changes(&kind);
    env.ledger().with_mut(|li| {
        li.sequence_number = ledger_before;
    });
}

pub fn apply_windows(env: &Env, client: &VirtualTokenContractClient, bet: u32, run: u32) {
    client.schedule_windows(&bet, &run);
    activate_pending(env, client, ConfigChangeKind::Windows);
}

pub fn apply_max_stake(env: &Env, client: &VirtualTokenContractClient, max: Option<i128>) {
    client.schedule_max_stake(&max);
    activate_pending(env, client, ConfigChangeKind::MaxStake);
}

pub fn apply_max_user_exposure(env: &Env, client: &VirtualTokenContractClient, max: Option<i128>) {
    client.schedule_max_user_exposure(&max);
    activate_pending(env, client, ConfigChangeKind::MaxUserRoundExposure);
}

pub fn apply_max_pending_winnings(
    env: &Env,
    client: &VirtualTokenContractClient,
    max: Option<i128>,
) {
    client.schedule_max_pending_winnings(&max);
    activate_pending(env, client, ConfigChangeKind::MaxPendingWinnings);
}

pub fn apply_oracle_stale_threshold(env: &Env, client: &VirtualTokenContractClient, seconds: u64) {
    client.schedule_oracle_stale_threshold(&seconds);
    activate_pending(env, client, ConfigChangeKind::OracleStaleThreshold);
}

pub fn apply_oracle_max_deviation_bps(
    env: &Env,
    client: &VirtualTokenContractClient,
    bps: Option<u32>,
) {
    client.schedule_oracle_deviation_bps(&bps);
    activate_pending(env, client, ConfigChangeKind::OracleMaxDeviationBps);
}
