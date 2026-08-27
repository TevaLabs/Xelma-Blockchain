// ============================================================================
// These tests exercise the optional protocol fee: default (ProtocolFeeBps
// storage key absent) is byte-for-byte the pre-#162 behaviour; activating
// the fee routes `fee = total_pot * bps / 10_000` to the on-chain treasury
// while preserving the conservation invariant
//     Σ payouts + treasury_growth == total_pot
// for every competitive settlement path (UpDown indexed/legacy, Precision
// indexed/legacy). Refund paths (price-unchanged, one-sided, min-participants,
// admin cancel) MUST NOT emit a fee event — and the treasury MUST stay flat.
//
// The 10% hard cap is enforced at schedule time; timelock semantics tested
// in `config_timelock.rs::test_protocol_fee_timelock_*`.

use super::*;

#[test]
fn test_protocol_fee_disabled_default_is_no_behaviour_change() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.create_round(&1_000_0000, &None);
    client.place_bet(&alice, &100_000_0000, &BetSide::Up);
    client.place_bet(&bob, &50_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number = 12);
    client.resolve_round(&OraclePayload {
        price: 1_500_0000,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    assert_eq!(
        sum_pending_payouts(&env, &client.address, &[alice.clone(), bob.clone()]),
        150_000_0000i128,
    );
    assert_eq!(client.get_protocol_fee_bps(), None);
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    assert_eq!(count_protocol_fee_events(&env), 0);
}

#[test]
fn test_protocol_fee_updown_indexed_conservation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.schedule_protocol_fee_bps(&Some(200u32));
    env.ledger().with_mut(|li| {
        li.sequence_number = 2000;
    });
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);
    assert_eq!(client.get_protocol_fee_bps(), Some(200u32));

    client.create_round(&1_000_0000, &None);
    client.place_bet(&alice, &100_000_0000, &BetSide::Up);
    client.place_bet(&bob, &50_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
        price: 1_500_0000,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 2u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    assert_eq!(count_protocol_fee_events(&env), 1);
    let events = collect_protocol_fee_events(&env);
    let (ev_round_id, fee, _treasury_after, bps) = events[0];
    assert_eq!(ev_round_id, 1u64);
    assert_eq!(fee, 3_000_0000i128);
    assert_eq!(bps, 200u32);

    let payouts = sum_pending_payouts(&env, &client.address, &[alice.clone(), bob.clone()]);
    assert_eq!(
        payouts, 147_000_0000i128,
        "winner payout must reflect fee deducted from losing pool"
    );
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(
        treasury, 3_000_0000i128,
        "treasury must accumulate exactly the bps-computed fee"
    );

    let total_pot: i128 = 150_000_0000i128;
    assert_eq!(
        payouts + treasury,
        total_pot,
        "conservation: payouts + treasury must equal total_pot"
    );
}

#[test]
fn test_protocol_fee_updown_legacy_conservation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.schedule_protocol_fee_bps(&Some(500u32));
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    let start_price: u128 = 1_000_0000;
    client.create_round(&start_price, &None);

    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(
            alice.clone(),
            UserPosition {
                amount: 100_000_0000,
                side: BetSide::Up,
            },
        );
        positions.set(
            bob.clone(),
            UserPosition {
                amount: 50_000_0000,
                side: BetSide::Down,
            },
        );
        env.storage()
            .persistent()
            .set(&DataKeyCore::UpDownPositions, &positions);

        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKeyCore::ActiveRound)
            .unwrap();
        round.pool_up = 100_000_0000;
        round.pool_down = 50_000_0000;
        env.storage()
            .persistent()
            .set(&DataKeyCore::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
        price: 1_500_0000,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 3u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    let payouts = sum_pending_payouts(&env, &client.address, &[alice.clone(), bob.clone()]);
    assert_eq!(payouts, 142_500_0000i128);
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 7_500_0000i128);
    assert_eq!(payouts + treasury, 150_000_0000i128);
}

#[test]
fn test_protocol_fee_precision_indexed_conservation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.schedule_protocol_fee_bps(&Some(1000u32));
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    client.create_round(&2000, &Some(1));
    client.place_precision_prediction(&alice, &100_000_0000, &2297u128);
    client.place_precision_prediction(&bob, &150_000_0000, &2500u128);
    client.place_precision_prediction(&charlie, &50_000_0000, &5000u128);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
        price: 2298,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 4u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    let payouts = sum_pending_payouts(
        &env,
        &client.address,
        &[alice.clone(), bob.clone(), charlie.clone()],
    );
    assert_eq!(payouts, 270_000_0000i128);
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 30_000_0000i128);
    assert_eq!(
        payouts + treasury,
        300_000_0000i128,
        "conservation invariant must hold for Precision indexed path"
    );
}

#[test]
fn test_protocol_fee_precision_legacy_conservation() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.schedule_protocol_fee_bps(&Some(100u32));
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    let start_price: u128 = 2000;
    client.create_round(&start_price, &Some(1));

    env.as_contract(&contract_id, || {
        let mut predictions = Map::<Address, PrecisionPrediction>::new(&env);
        predictions.set(
            alice.clone(),
            PrecisionPrediction {
                user: alice.clone(),
                predicted_price: 2297,
                amount: 100_000_0000,
            },
        );
        predictions.set(
            bob.clone(),
            PrecisionPrediction {
                user: bob.clone(),
                predicted_price: 2500,
                amount: 150_000_0000,
            },
        );
        predictions.set(
            charlie.clone(),
            PrecisionPrediction {
                user: charlie.clone(),
                predicted_price: 5000,
                amount: 50_000_0000,
            },
        );
        env.storage()
            .persistent()
            .set(&DataKeyCore::PrecisionPositions, &predictions);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
        price: 2298,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 5u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    let payouts = sum_pending_payouts(
        &env,
        &client.address,
        &[alice.clone(), bob.clone(), charlie.clone()],
    );
    assert_eq!(payouts, 297_000_0000i128);
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(treasury, 3_000_0000i128);
    assert_eq!(
        payouts + treasury,
        300_000_0000i128,
        "conservation invariant must hold for Precision legacy path"
    );
}

#[test]
fn test_protocol_fee_thin_losing_pool_updown() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.schedule_protocol_fee_bps(&Some(1000u32));
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    client.create_round(&1_000_0000, &None);
    client.place_bet(&alice, &1000_000_0000, &BetSide::Up);
    client.place_bet(&bob, &1_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
        price: 1_500_0000,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 6u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    let payouts = sum_pending_payouts(&env, &client.address, &[alice.clone(), bob.clone()]);
    assert_eq!(
        payouts, 9_009_000_000i128,
        "winner payout reduced by winning-pool spillover"
    );
    let treasury = client.get_protocol_fee_treasury();
    assert_eq!(
        treasury, 1_001_000_000i128,
        "full fee still collected: 10_000_000 (from losing) + 991_000_000 (from winning spillover)"
    );
    assert_eq!(
        payouts + treasury,
        10_010_000_000i128,
        "conservation invariant holds even when losing_pool is thin"
    );
}

#[test]
fn test_protocol_fee_not_collected_on_refund_paths() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.schedule_protocol_fee_bps(&Some(1000u32));
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    let start_price: u128 = 1_500_0000;
    client.create_round(&start_price, &None);
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(
            alice.clone(),
            UserPosition {
                amount: 100_000_0000,
                side: BetSide::Up,
            },
        );
        positions.set(
            bob.clone(),
            UserPosition {
                amount: 50_000_0000,
                side: BetSide::Down,
            },
        );
        env.storage()
            .persistent()
            .set(&DataKeyCore::UpDownPositions, &positions);
        let mut round: Round = env
            .storage()
            .persistent()
            .get(&DataKeyCore::ActiveRound)
            .unwrap();
        round.pool_up = 100_000_0000;
        round.pool_down = 50_000_0000;
        env.storage()
            .persistent()
            .set(&DataKeyCore::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
        price: start_price,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 7u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    assert_eq!(
        count_protocol_fee_events(&env),
        0,
        "price-unchanged refunds MUST NOT emit a fee event"
    );
    assert_eq!(client.get_protocol_fee_treasury(), 0);
    let payouts = sum_pending_payouts(&env, &client.address, &[alice.clone(), bob.clone()]);
    assert_eq!(
        payouts, 150_000_0000i128,
        "all participants refunded their full stake"
    );
}

#[test]
fn test_protocol_fee_not_collected_on_one_sided_pool_refund() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.schedule_protocol_fee_bps(&Some(500u32));
    env.ledger().with_mut(|li| li.sequence_number = 2_000);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    let start_price: u128 = 1_500_0000;
    client.create_round(&start_price, &None);

    client.place_bet(&alice, &100_000_0000, &BetSide::Down);
    client.place_bet(&bob, &50_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
        price: 1_700_0000,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 9u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });

    assert_eq!(
        count_protocol_fee_events(&env),
        0,
        "one-sided refund MUST NOT emit a fee event"
    );
    assert_eq!(
        client.get_protocol_fee_treasury(),
        0,
        "one-sided refund MUST NOT credit the treasury"
    );
    let payouts = sum_pending_payouts(&env, &client.address, &[alice.clone(), bob.clone()]);
    assert_eq!(
        payouts, 150_000_0000i128,
        "all participants refunded their full stake on one-sided pool"
    );
}

#[test]
fn test_protocol_fee_withdrawal_to_recipient() {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let treasury_account = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&treasury_account);

    client.schedule_protocol_fee_bps(&Some(1000u32));
    env.ledger().with_mut(|li| li.sequence_number = 2000);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    client.create_round(&1_000_0000, &None);
    client.place_bet(&alice, &100_000_0000, &BetSide::Up);
    client.place_bet(&bob, &50_000_0000, &BetSide::Down);

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
        price: 1_500_0000,
        timestamp: env.ledger().timestamp(),
        round_id: client
            .get_active_round()
            .map(|r| r.start_ledger)
            .unwrap_or(0),
        nonce: 8u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,    });
    assert_eq!(client.get_protocol_fee_treasury(), 15_000_0000i128);

    let starting_bal = client.balance(&treasury_account);
    let withdrawn = client.withdraw_protocol_fee(&treasury_account.clone(), &10_000_0000i128);
    assert_eq!(withdrawn, 10_000_0000i128);
    assert_eq!(
        client.balance(&treasury_account),
        starting_bal + 10_000_0000i128,
    );
    assert_eq!(client.get_protocol_fee_treasury(), 5_000_0000i128);

    let result = client.try_withdraw_protocol_fee(&treasury_account.clone(), &1_000_000_0000i128);
    assert!(result.is_err(), "over-withdrawal must be rejected");
    assert_eq!(client.get_protocol_fee_treasury(), 5_000_0000i128);
}

#[test]
fn test_protocol_fee_schedule_validation_rejects_zero_and_over_cap() {
    fn run_to_activation(env: &Env) {
        env.ledger().with_mut(|li| li.sequence_number += 1_500);
    }
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    client.schedule_protocol_fee_bps(&None);
    run_to_activation(&env);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);
    assert_eq!(client.get_protocol_fee_bps(), None);

    let r0 = client.try_schedule_protocol_fee_bps(&Some(0u32));
    assert!(r0.is_err(), "Some(0) is not a valid bps value");
    run_to_activation(&env);
    let _ = client.try_cancel_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    let r_max = client.try_schedule_protocol_fee_bps(&Some(1_001u32));
    assert!(
        r_max.is_err(),
        "1_001 bps exceeds MAX_PROTOCOL_FEE_BPS=1000"
    );
    run_to_activation(&env);
    let _ = client.try_cancel_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    let r_top = client.try_schedule_protocol_fee_bps(&Some(1_000u32));
    assert!(r_top.is_ok(), "1_000 bps (MAX) must be accepted");
    run_to_activation(&env);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);
    assert_eq!(client.get_protocol_fee_bps(), Some(1_000u32));
}
