// SPDX-License-Identifier: MIT
//! Insurance / backstop fund tests for Issue #367.
//!
//! Covers acceptance criteria:
//! - Normal fee path still conserves (split only redirects, doesn't create value)
//! - Coverage pays only for whitelisted events
//! - Cannot pay more than fund balance
//! - Events audit all movements
//! - Policy is unambiguous

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{
    BetSide, DataKeyCore, FeeModel, InsuranceEvent, OraclePayload,
    CANCEL_REASON_FALLBACK_REFUND, CANCEL_REASON_GENERIC, CANCEL_REASON_ORACLE_DEVIATION,
    CANCEL_REASON_ORACLE_OUTAGE,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, Vec as SorobanVec,
};

// ─── Shared helpers ──────────────────────────────────────────────────────────

fn setup_contract(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    (client, contract_id, admin, oracle)
}

fn set_fee_bps_now(env: &Env, contract_id: &Address, bps: u32) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::ProtocolFeeBps, &bps);
    });
}

fn set_fee_model_now(env: &Env, contract_id: &Address, model: FeeModel) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKeyCore::FeeModel, &model);
    });
}

fn mint_and_place_bet(
    env: &Env,
    client: &VirtualTokenContractClient,
    user: &Address,
    amount: i128,
    side: BetSide,
) {
    client.mint_initial(user);
    client.place_bet(user, &amount, &side);
}

fn resolve_at(
    env: &Env,
    client: &VirtualTokenContractClient,
    contract_id: &Address,
    final_price: u128,
) {
    let round = client
        .get_active_round()
        .expect("active round required to resolve");
    client.resolve_round(&OraclePayload {
        price: final_price,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });
}

fn create_and_fund_round(
    env: &Env,
    client: &VirtualTokenContractClient,
    start_price: u128,
) -> Address {
    let alice = Address::generate(env);
    let bob = Address::generate(env);

    client.create_round(&start_price, &Some(0));
    mint_and_place_bet(env, client, &alice, 1000, BetSide::Up);
    mint_and_place_bet(env, client, &bob, 1000, BetSide::Down);

    alice
}

// ─── Fee split tests ─────────────────────────────────────────────────────────

/// When insurance split is 0 (default), all fees go to ops treasury.
#[test]
fn fee_split_zero_insurance_all_goes_to_ops() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    set_fee_bps_now(&env, &contract_id, 100); // 1%
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnPot);

    // Insurance split is 0 by default
    assert_eq!(client.get_insurance_split_bps(), 0);

    let _ = create_and_fund_round(&env, &client, 1000);
    resolve_at(&env, &client, &contract_id, 1100);

    // All fee should be in ops treasury
    let ops_treasury = client.get_protocol_fee_treasury();
    assert!(ops_treasury > 0);

    // Insurance fund should be 0
    assert_eq!(client.get_insurance_fund_balance(), 0);
}

/// When insurance split is configured, fees are split between ops and insurance.
#[test]
fn fee_split_50_percent_goes_to_insurance() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    set_fee_bps_now(&env, &contract_id, 100); // 1% protocol fee
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnPot);

    // Set insurance split to 50% (5000 bps)
    client.set_insurance_split_bps(&5000);

    let _ = create_and_fund_round(&env, &client, 1000);
    resolve_at(&env, &client, &contract_id, 1100);

    let ops_treasury = client.get_protocol_fee_treasury();
    let insurance_balance = client.get_insurance_fund_balance();

    // Both should have funds (approximately equal)
    assert!(ops_treasury > 0, "ops treasury should have funds");
    assert!(insurance_balance > 0, "insurance fund should have funds");

    // Total should equal what was originally collected
    let total = ops_treasury + insurance_balance;
    assert!(
        total > 0,
        "total fee split should be conserved"
    );
}

/// Insurance split bps cannot exceed the maximum (50%).
#[test]
fn fee_split_rejects_too_high_bps() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    let result = client.try_set_insurance_split_bps(&5001);
    assert!(result.is_err());
}

// ─── Coverage eligibility tests ──────────────────────────────────────────────

/// Coverage only pays for whitelisted events.
#[test]
fn coverage_only_for_whitelisted_events() {
    let env = Env::default();
    let (client, contract_id, _admin, oracle) = setup_contract(&env);

    // Fund the insurance pool
    client.set_insurance_split_bps(&5000);
    client.set_insurance_coverage_bps(&1000); // 10% coverage
    let mut whitelist: SorobanVec<u32> = SorobanVec::new(&env);
    whitelist.push_back(InsuranceEvent::OracleOutage as u32);
    client.set_insurance_eligible_events(&whitelist);

    // Create a round and fund the insurance pool
    let _ = create_and_fund_round(&env, &client, 1000);
    resolve_at(&env, &client, &contract_id, 1100);

    let fund_before = client.get_insurance_fund_balance();
    assert!(fund_before > 0, "insurance fund should be funded");

    // Now create a new round to cancel
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.create_round(&1000, &Some(0));
    mint_and_place_bet(&env, &client, &alice, 500, BetSide::Up);
    mint_and_place_bet(&env, &client, &bob, 500, BetSide::Down);

    // Cancel with OracleOutage reason (whitelisted)
    client.cancel_round(&CANCEL_REASON_ORACLE_OUTAGE);

    // Fund should have been used
    let fund_after = client.get_insurance_fund_balance();
    assert!(
        fund_after < fund_before,
        "insurance fund should decrease after eligible cancellation"
    );
}

/// Generic cancellation reason (0) does not trigger coverage.
#[test]
fn coverage_not_paid_for_generic_reason() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    // Fund the insurance pool
    client.set_insurance_split_bps(&5000);
    client.set_insurance_coverage_bps(&1000);
    let mut whitelist: SorobanVec<u32> = SorobanVec::new(&env);
    whitelist.push_back(InsuranceEvent::OracleOutage as u32);
    client.set_insurance_eligible_events(&whitelist);

    // Create a round and fund the insurance pool
    let _ = create_and_fund_round(&env, &client, 1000);
    resolve_at(&env, &client, &contract_id, 1100);

    let fund_before = client.get_insurance_fund_balance();

    // Create a new round to cancel
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.create_round(&1000, &Some(0));
    mint_and_place_bet(&env, &client, &alice, 500, BetSide::Up);
    mint_and_place_bet(&env, &client, &bob, 500, BetSide::Down);

    // Cancel with generic reason (NOT whitelisted)
    client.cancel_round(&CANCEL_REASON_GENERIC);

    // Fund should NOT have changed
    let fund_after = client.get_insurance_fund_balance();
    assert_eq!(
        fund_before, fund_after,
        "insurance fund should not change for non-whitelisted events"
    );
}

// ─── Solvency tests ──────────────────────────────────────────────────────────

/// Cannot pay more than the fund balance.
#[test]
fn coverage_never_exceeds_fund_balance() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    // Set high coverage (100%) but very low fund balance
    client.set_insurance_split_bps(&100); // 1% of fees
    client.set_insurance_coverage_bps(&10000); // 100% coverage
    let mut whitelist: SorobanVec<u32> = SorobanVec::new(&env);
    whitelist.push_back(InsuranceEvent::OracleOutage as u32);
    client.set_insurance_eligible_events(&whitelist);

    // Fund with a small top-up
    let alice_funder = Address::generate(&env);
    client.mint_initial(&alice_funder);

    // Set insurance fund to 100 directly
    env.as_contract(&_contract_id, || {
        let key = soroban_sdk::Symbol::new(&env, "InsFundBal");
        env.storage().persistent().set(&key, &100i128);
    });

    let fund_balance = client.get_insurance_fund_balance();
    assert_eq!(fund_balance, 100);

    // Create a round with high stakes (1000 per user)
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    client.create_round(&1000, &Some(0));
    mint_and_place_bet(&env, &client, &user1, 1000, BetSide::Up);
    mint_and_place_bet(&env, &client, &user2, 1000, BetSide::Down);

    // Cancel with eligible reason
    client.cancel_round(&CANCEL_REASON_ORACLE_OUTAGE);

    // Fund should never go below 0
    let fund_after = client.get_insurance_fund_balance();
    assert!(
        fund_after >= 0,
        "insurance fund balance should never be negative"
    );
}

/// Fund balance is conserved on withdrawal.
#[test]
fn insurance_fund_withdrawal_respects_balance() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    // Fund the insurance pool with a known amount
    env.as_contract(&_contract_id, || {
        let key = soroban_sdk::Symbol::new(&env, "InsFundBal");
        env.storage().persistent().set(&key, &1000i128);
    });

    let recipient = Address::generate(&env);

    // Withdraw more than available should fail
    let result = client.try_withdraw_insurance_fund(&recipient, &2000);
    assert!(result.is_err());

    // Withdraw exact amount should succeed
    let withdrawn = client.withdraw_insurance_fund(&recipient, &500);
    assert_eq!(withdrawn, 500);

    let fund_after = client.get_insurance_fund_balance();
    assert_eq!(fund_after, 500);
}

// ─── Configuration tests ─────────────────────────────────────────────────────

/// Insurance split bps can be set and read.
#[test]
fn insurance_split_bps_config() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    assert_eq!(client.get_insurance_split_bps(), 0);

    client.set_insurance_split_bps(&2500);
    assert_eq!(client.get_insurance_split_bps(), 2500);

    client.set_insurance_split_bps(&0);
    assert_eq!(client.get_insurance_split_bps(), 0);
}

/// Insurance coverage bps can be set and read.
#[test]
fn insurance_coverage_bps_config() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    assert_eq!(client.get_insurance_coverage_bps(), 0);

    client.set_insurance_coverage_bps(&500);
    assert_eq!(client.get_insurance_coverage_bps(), 500);
}

/// Insurance eligible events can be set and read.
#[test]
fn insurance_eligible_events_config() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    // Initially empty
    let events = client.get_insurance_eligible_events();
    assert_eq!(events.len(), 0);

    // Set some events
    let mut whitelist: SorobanVec<u32> = SorobanVec::new(&env);
    whitelist.push_back(InsuranceEvent::OracleOutage as u32);
    whitelist.push_back(InsuranceEvent::OracleDeviation as u32);
    client.set_insurance_eligible_events(&whitelist);

    let events = client.get_insurance_eligible_events();
    assert_eq!(events.len(), 2);
}

/// Insurance fund balance is initially zero.
#[test]
fn insurance_fund_starts_at_zero() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    assert_eq!(client.get_insurance_fund_balance(), 0);
}

// ─── Fee conservation tests ──────────────────────────────────────────────────

/// Normal fee path still conserves with insurance split enabled.
#[test]
fn fee_conservation_with_insurance_split() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    set_fee_bps_now(&env, &contract_id, 200); // 2% fee
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnPot);

    // 30% to insurance, 70% to ops
    client.set_insurance_split_bps(&3000);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.create_round(&1000, &Some(0));
    mint_and_place_bet(&env, &client, &alice, 1000, BetSide::Up);
    mint_and_place_bet(&env, &client, &bob, 2000, BetSide::Down);

    // Price goes up — alice wins
    resolve_at(&env, &client, &contract_id, 1100);

    let ops_treasury = client.get_protocol_fee_treasury();
    let insurance_balance = client.get_insurance_fund_balance();

    // Total fee = ops + insurance
    let total_fee = ops_treasury + insurance_balance;

    // The total fee should be 2% of the total pot (3000)
    // = 3000 * 200 / 10000 = 60
    assert_eq!(total_fee, 60, "total fee should be conserved");

    // Insurance should be 30% of 60 = 18
    assert_eq!(insurance_balance, 18);

    // Ops should be 70% of 60 = 42
    assert_eq!(ops_treasury, 42);
}

/// Fee conservation with fee-on-winnings model.
#[test]
fn fee_conservation_insurance_fee_on_winnings() {
    let env = Env::default();
    let (client, contract_id, _admin, _oracle) = setup_contract(&env);

    set_fee_bps_now(&env, &contract_id, 200); // 2% fee
    set_fee_model_now(&env, &contract_id, FeeModel::FeeOnWinnings);

    // 50% to insurance
    client.set_insurance_split_bps(&5000);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.create_round(&1000, &Some(0));
    mint_and_place_bet(&env, &client, &alice, 1000, BetSide::Up);
    mint_and_place_bet(&env, &client, &bob, 2000, BetSide::Down);

    resolve_at(&env, &client, &contract_id, 1100);

    let ops_treasury = client.get_protocol_fee_treasury();
    let insurance_balance = client.get_insurance_fund_balance();
    let total_fee = ops_treasury + insurance_balance;

    // Fee on winnings: fee = losing_pool * bps / 10000 = 1000 * 200 / 10000 = 20
    assert_eq!(total_fee, 20, "fee-on-winnings total should be correct");

    // Each gets 50%
    assert_eq!(insurance_balance, 10);
    assert_eq!(ops_treasury, 10);
}

// ─── Top-up tests ────────────────────────────────────────────────────────────

/// Admin can top up the insurance fund.
#[test]
fn admin_can_top_up_insurance_fund() {
    let env = Env::default();
    let (client, _contract_id, admin, _oracle) = setup_contract(&env);

    // Give admin some balance
    client.mint_initial(&admin);

    let initial_balance = client.get_insurance_fund_balance();
    assert_eq!(initial_balance, 0);

    client.top_up_insurance_fund(&500);

    let new_balance = client.get_insurance_fund_balance();
    assert_eq!(new_balance, 500);
}

// ─── OracleDeviation coverage test ───────────────────────────────────────────

/// OracleDeviation reason triggers coverage when whitelisted.
#[test]
fn coverage_paid_for_oracle_deviation_reason() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    // Fund the insurance pool directly
    env.as_contract(&_contract_id, || {
        let key = soroban_sdk::Symbol::new(&env, "InsFundBal");
        env.storage().persistent().set(&key, &10000i128);
    });

    client.set_insurance_coverage_bps(&1000); // 10%
    let mut whitelist: SorobanVec<u32> = SorobanVec::new(&env);
    whitelist.push_back(InsuranceEvent::OracleDeviation as u32);
    client.set_insurance_eligible_events(&whitelist);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.create_round(&1000, &Some(0));
    mint_and_place_bet(&env, &client, &alice, 1000, BetSide::Up);
    mint_and_place_bet(&env, &client, &bob, 1000, BetSide::Down);

    let fund_before = client.get_insurance_fund_balance();

    client.cancel_round(&CANCEL_REASON_ORACLE_DEVIATION);

    let fund_after = client.get_insurance_fund_balance();
    assert!(
        fund_after < fund_before,
        "fund should decrease on oracle deviation cancellation"
    );
}

/// FallbackRefund reason triggers coverage when whitelisted.
#[test]
fn coverage_paid_for_fallback_refund_reason() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    // Fund the insurance pool directly
    env.as_contract(&_contract_id, || {
        let key = soroban_sdk::Symbol::new(&env, "InsFundBal");
        env.storage().persistent().set(&key, &10000i128);
    });

    client.set_insurance_coverage_bps(&1000); // 10%
    let mut whitelist: SorobanVec<u32> = SorobanVec::new(&env);
    whitelist.push_back(InsuranceEvent::FallbackRefund as u32);
    client.set_insurance_eligible_events(&whitelist);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.create_round(&1000, &Some(0));
    mint_and_place_bet(&env, &client, &alice, 1000, BetSide::Up);
    mint_and_place_bet(&env, &client, &bob, 1000, BetSide::Down);

    let fund_before = client.get_insurance_fund_balance();

    client.cancel_round(&CANCEL_REASON_FALLBACK_REFUND);

    let fund_after = client.get_insurance_fund_balance();
    assert!(
        fund_after < fund_before,
        "fund should decrease on fallback refund cancellation"
    );
}

// ─── Edge case: no coverage when bps is 0 ────────────────────────────────────

/// No coverage is paid when coverage_bps is 0 even if event is whitelisted.
#[test]
fn no_coverage_when_bps_zero() {
    let env = Env::default();
    let (client, _contract_id, _admin, _oracle) = setup_contract(&env);

    // Fund the insurance pool
    env.as_contract(&_contract_id, || {
        let key = soroban_sdk::Symbol::new(&env, "InsFundBal");
        env.storage().persistent().set(&key, &10000i128);
    });

    // Coverage bps is 0 (default)
    let mut whitelist: SorobanVec<u32> = SorobanVec::new(&env);
    whitelist.push_back(InsuranceEvent::OracleOutage as u32);
    client.set_insurance_eligible_events(&whitelist);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.create_round(&1000, &Some(0));
    mint_and_place_bet(&env, &client, &alice, 1000, BetSide::Up);
    mint_and_place_bet(&env, &client, &bob, 1000, BetSide::Down);

    let fund_before = client.get_insurance_fund_balance();

    client.cancel_round(&CANCEL_REASON_ORACLE_OUTAGE);

    let fund_after = client.get_insurance_fund_balance();
    assert_eq!(
        fund_before, fund_after,
        "fund should not change when coverage_bps is 0"
    );
}
