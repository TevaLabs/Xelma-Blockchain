
use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::RoundMode;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _, Events},
    Address, BytesN, Env, IntoVal, symbol_short
};

#[test]
fn test_commit_fee() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&user); // Gives 10_000_0000 by default

    // set commit fee
    client.set_commit_fee(&2_0000);
    // advance ledger to bypass timelock
    let ledger = env.ledger().sequence();
    env.ledger().with_mut(|l| l.sequence = ledger + 100);

    // create precision round
    client.create_round_with_mode(&1_0000000, &None, &RoundMode::Precision);

    let bet_amount = 1_0000;
    let initial_balance = client.balance(&user);
    
    // Create dummy hash
    let hash = BytesN::from_array(&env, &[1; 32]);
    client.commit_prediction(&user, &hash, &bet_amount);

    // Balance should have been deducted by amount + commit_fee
    let expected_deduction = bet_amount + 2_0000;
    assert_eq!(client.balance(&user), initial_balance - expected_deduction);

    // Fee treasury should receive fee
    assert_eq!(client.get_protocol_fee_treasury(), 2_0000);

    // Check event emission
    let events = env.events().all();
    let mut found_fee_event = false;
    for (contract_id, topic, data) in events.iter() {
        if contract_id == client.address {
            let t: soroban_sdk::Vec<soroban_sdk::Val> = topic.into_val(&env);
            if t.len() == 2 && t.get(0).unwrap().try_into_val(&env) == Ok(symbol_short!("commit")) {
                if t.get(1).unwrap().try_into_val(&env) == Ok(symbol_short!("fee")) {
                    found_fee_event = true;
                    // data should be (user, round_id, fee)
                }
            }
        }
    }
    assert!(found_fee_event, "commit fee event not emitted");
}

