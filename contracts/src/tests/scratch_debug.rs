// SPDX-License-Identifier: MIT
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::ConfigChangeKind;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger as _},
    Address, Env,
};

fn dump(env: &Env, label: &str) {
    let all = env.events().all();
    std::println!("--- {label}: {} events", all.len());
    for (i, (_contract, topics, data)) in all.iter().enumerate() {
        std::println!("  event {i}: topics={topics:?} data={data:?}");
    }
}

#[test]
fn scratch_debug_scheduled_events() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    client.schedule_windows(&10, &20);
    dump(&env, "after schedule_windows");

    let pending = client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .unwrap();
    dump(&env, "after get_pending_config_change");

    env.ledger().with_mut(|li| li.sequence_number = 1440);
    client.apply_scheduled_changes(&ConfigChangeKind::Windows);
    dump(&env, "after apply_scheduled_changes");

    let pending2 = client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .unwrap();
    std::println!("pending2={pending2:?}");
    dump(&env, "after get_pending_config_change 2");
}
