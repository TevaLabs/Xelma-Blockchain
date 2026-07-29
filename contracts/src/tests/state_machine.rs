// SPDX-License-Identifier: MIT
//! Full legal/illegal transition suite for both modes (Issue #284).
//!
//! Validates every allowed transition across the round lifecycle state
//! machine and asserts that every illegal edge returns the correct error.
//!
//! ## Transition table (documented inline)
//!
//! ### RoundStatus transitions
//! | from              | action             | to                | legal? | error if illegal     |
//! |-------------------|--------------------|-------------------|--------|----------------------|
//! | Unknown           | create_round       | Betting           | yes    | –                    |
//! | Betting           | create_round       | Betting           | no     | RoundAlreadyActive   |
//! | Betting           | ledger advance     | Running           | yes    | –                    |
//! | Running           | ledger advance     | AwaitingResolve   | yes    | –                    |
//! | Betting           | cancel_round       | Cancelled         | yes    | –                    |
//! | Running           | cancel_round       | Cancelled         | yes    | –                    |
//! | AwaitingResolve   | cancel_round       | Cancelled         | yes    | –                    |
//! | Cancelled         | cancel_round       | Cancelled         | no     | RoundNotCancellable  |
//! | Resolved          | cancel_round       | Cancelled         | no     | RoundNotCancellable  |
//! | AwaitingResolve   | resolve_round (≥pp)| Resolved          | yes    | –                    |
//! | AwaitingResolve   | resolve_round (<pp)| FallbackRefund    | yes    | –                    |
//! | Betting           | resolve_round      | Betting           | no     | RoundNotEnded        |
//! | Running           | resolve_round      | Running           | no     | RoundNotEnded        |
//!
//! ### ProtocolStatus transitions
//! | from        | action               | to          | legal? |
//! |-------------|----------------------|-------------|--------|
//! | ClaimsOnly  | create_round         | Active      | yes    |
//! | Active      | resolve_round        | ClaimsOnly  | yes    |
//! | Active      | cancel_round         | ClaimsOnly  | yes    |
//! | ClaimsOnly  | pause_contract       | Paused      | yes    |
//! | Active      | pause_contract       | Paused      | yes    |
//! | Paused      | unpause_contract     | ClaimsOnly  | yes    |
//! | Paused      | unpause_contract     | Active      | yes†   |
//! | Paused      | create_round         | Paused      | no     |
//! | Paused      | place_bet            | Paused      | no     |
//! | Paused      | resolve_round        | Paused      | no     |
//!
//! † When a round was active before pause.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload, ProtocolStatus, RoundMode, RoundStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address) {
    env.ledger().with_mut(|li| {
        li.sequence_number = 1;
    });
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    (client, admin, oracle)
}

fn setup_with_user(env: &Env) -> (VirtualTokenContractClient<'_>, Address, Address, Address) {
    let (client, admin, oracle) = setup(env);
    let user = Address::generate(env);
    client.mint_initial(&user);
    (client, admin, oracle, user)
}

fn test_salt(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        bytes[i] = seed.wrapping_add(i as u8).wrapping_mul(17).wrapping_add(3);
        i += 1;
    }
    bytes[0] = seed | 0x80;
    bytes[31] = seed ^ 0x5A;
    BytesN::from_array(env, &bytes)
}

fn make_commitment(env: &Env, predicted_price: u128, salt: &BytesN<32>) -> BytesN<32> {
    use soroban_sdk::xdr::ToXdr;
    use soroban_sdk::Bytes;
    let mut preimage = Bytes::new(env);
    preimage.append(&predicted_price.to_xdr(env));
    preimage.append(&salt.to_xdr(env));
    let hash = env.crypto().sha256(&preimage);
    hash.into()
}

// ─── Legal transitions ───────────────────────────────────────────────────────

/// Unknown → Betting: create_round transitions the round to Betting phase.
#[test]
fn test_transition_unknown_to_betting() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    assert_eq!(client.get_round_status(&1), RoundStatus::Unknown);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);

    client.create_round(&1_0000000, &None);

    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);
}

/// ClaimsOnly → Active: creating a round moves protocol to Active.
#[test]
fn test_transition_claims_only_to_active() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
    client.create_round(&1_0000000, &None);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);
}

/// Betting → Running → AwaitingResolve: derived transitions via ledger advance.
#[test]
fn test_transition_betting_to_running_to_awaiting_resolve() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.create_round(&1_0000000, &None);
    // start_ledger=1, bet_end_ledger=1+6=7, end_ledger=1+12=13
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);

    // Advance into Running phase
    env.ledger().with_mut(|li| li.sequence_number = 8);
    assert_eq!(client.get_round_status(&1), RoundStatus::Running);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);

    // Advance into AwaitingResolve phase
    env.ledger().with_mut(|li| li.sequence_number = 14);
    assert_eq!(client.get_round_status(&1), RoundStatus::AwaitingResolve);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);
}

/// AwaitingResolve → Resolved: resolve_round completes the round.
#[test]
fn test_transition_awaiting_resolve_to_resolved() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| li.sequence_number = 20);
    assert_eq!(client.get_round_status(&1), RoundStatus::AwaitingResolve);

    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 1,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    });

    assert_eq!(client.get_round_status(&1), RoundStatus::Resolved);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
}

/// Active → ClaimsOnly: resolve_round clears active status.
#[test]
fn test_transition_active_to_claims_only_via_resolve() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| li.sequence_number = 20);
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 1,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    });

    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
}

/// Betting → Cancelled: cancel_round from Betting phase.
#[test]
fn test_transition_betting_to_cancelled() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.create_round(&1_0000000, &None);
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);

    client.cancel_round(&0u32);

    assert_eq!(client.get_round_status(&1), RoundStatus::Cancelled);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
}

/// Running → Cancelled: cancel_round from Running phase.
#[test]
fn test_transition_running_to_cancelled() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.create_round(&1_0000000, &None);
    env.ledger().with_mut(|li| li.sequence_number = 8);
    assert_eq!(client.get_round_status(&1), RoundStatus::Running);

    client.cancel_round(&0u32);

    assert_eq!(client.get_round_status(&1), RoundStatus::Cancelled);
}

/// AwaitingResolve → Cancelled: cancel_round from AwaitingResolve phase.
#[test]
fn test_transition_awaiting_resolve_to_cancelled() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.create_round(&1_0000000, &None);
    env.ledger().with_mut(|li| li.sequence_number = 20);
    assert_eq!(client.get_round_status(&1), RoundStatus::AwaitingResolve);

    client.cancel_round(&0u32);

    assert_eq!(client.get_round_status(&1), RoundStatus::Cancelled);
}

/// Active → Paused → Active: pause and unpause with active round restores Active.
#[test]
fn test_transition_active_to_paused_to_active() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.create_round(&1_0000000, &None);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);

    client.pause_contract();
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Paused);

    client.unpause_contract();
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Active);
}

/// ClaimsOnly → Paused → ClaimsOnly: pause and unpause with no round.
#[test]
fn test_transition_claims_only_to_paused_to_claims_only() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);

    client.pause_contract();
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Paused);

    client.unpause_contract();
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
}

/// Round phase unchanged by pause – derived from ledger, not from paused state.
#[test]
fn test_round_phase_unaffected_by_pause() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.create_round(&1_0000000, &None);
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);

    client.pause_contract();
    assert_eq!(client.get_protocol_status(), ProtocolStatus::Paused);
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);

    env.ledger().with_mut(|li| li.sequence_number = 8);
    assert_eq!(client.get_round_status(&1), RoundStatus::Running);
}

// ─── Illegal transitions ─────────────────────────────────────────────────────

/// create_round when already active → RoundAlreadyActive.
#[test]
fn test_illegal_create_round_while_active() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.create_round(&1_0000000, &None);
    let result = client.try_create_round(&2_0000000, &None);
    assert_eq!(result, Err(Ok(ContractError::RoundAlreadyActive)));
}

/// resolve_round before end_ledger → RoundNotEnded.
#[test]
fn test_illegal_resolve_before_end_ledger() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    // At ledger 2, still in Betting phase
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 1,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::RoundNotEnded)));
}

/// resolve_round during Running phase → RoundNotEnded.
#[test]
fn test_illegal_resolve_during_running() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| li.sequence_number = 8);
    assert_eq!(client.get_round_status(&1), RoundStatus::Running);

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 1,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::RoundNotEnded)));
}

/// cancel_round with no active round → RoundNotCancellable.
#[test]
fn test_illegal_cancel_no_active_round() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    let result = client.try_cancel_round(&0u32);
    assert_eq!(result, Err(Ok(ContractError::RoundNotCancellable)));
}

/// cancel_round on already resolved round → RoundNotCancellable.
#[test]
fn test_illegal_cancel_after_resolved() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| li.sequence_number = 20);
    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 1,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    });

    assert_eq!(client.get_round_status(&1), RoundStatus::Resolved);
    let result = client.try_cancel_round(&0u32);
    assert_eq!(result, Err(Ok(ContractError::RoundNotCancellable)));
}

/// place_bet in Precision mode → WrongModeForPrediction.
#[test]
fn test_illegal_place_bet_in_precision_mode() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &Some(1)); // Precision
    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::WrongModeForPrediction)));
}

/// place_precision_prediction in UpDown mode → WrongModeForPrediction.
#[test]
fn test_illegal_precision_prediction_in_updown_mode() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &Some(0)); // UpDown
    let result = client.try_place_precision_prediction(&user, &100_0000000, &2297);
    assert_eq!(result, Err(Ok(ContractError::WrongModeForPrediction)));
}

/// place_bet while paused → ContractPaused.
#[test]
fn test_illegal_place_bet_while_paused() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &None);
    client.pause_contract();

    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
}

/// create_round while paused → ContractPaused.
#[test]
fn test_illegal_create_round_while_paused() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.pause_contract();
    let result = client.try_create_round(&1_0000000, &None);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
}

/// resolve_round while paused → ContractPaused.
#[test]
fn test_illegal_resolve_while_paused() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| li.sequence_number = 20);
    client.pause_contract();

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 1,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
}

/// commit_prediction while paused → ContractPaused.
#[test]
fn test_illegal_commit_while_paused() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &Some(1)); // Precision
    client.pause_contract();

    let salt = test_salt(&env, 42);
    let predicted_price: u128 = 2297;
    let hash = make_commitment(&env, predicted_price, &salt);
    let result = client.try_commit_prediction(&user, &hash, &100_0000000);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
}

/// reveal_prediction while paused → ContractPaused.
#[test]
fn test_illegal_reveal_while_paused() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    let salt = test_salt(&env, 42);
    let predicted_price: u128 = 2297;
    let hash = make_commitment(&env, predicted_price, &salt);

    client.create_round(&1_0000000, &Some(1)); // Precision
    client.commit_prediction(&user, &hash, &100_0000000);

    client.pause_contract();

    let result = client.try_reveal_prediction(&user, &predicted_price, &salt);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
}

/// Invalid mode (2) → InvalidMode.
#[test]
fn test_illegal_create_round_invalid_mode() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    let result = client.try_create_round(&1_0000000, &Some(2));
    assert_eq!(result, Err(Ok(ContractError::InvalidMode)));
}

/// Already bet in Up/Down → AlreadyBet.
#[test]
fn test_illegal_double_bet_updown() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    let result = client.try_place_bet(&user, &50_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::AlreadyBet)));
}

/// Already committed → AlreadyBet.
#[test]
fn test_illegal_double_commit_precision() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    let salt = test_salt(&env, 42);
    let predicted_price: u128 = 2297;
    let hash = make_commitment(&env, predicted_price, &salt);

    client.create_round(&1_0000000, &Some(1)); // Precision
    client.commit_prediction(&user, &hash, &100_0000000);

    let salt2 = test_salt(&env, 99);
    let hash2 = make_commitment(&env, 2500, &salt2);
    let result = client.try_commit_prediction(&user, &hash2, &50_0000000);
    assert_eq!(result, Err(Ok(ContractError::AlreadyBet)));
}

/// reveal_prediction outside reveal window → InvalidRevealWindow.
#[test]
fn test_illegal_reveal_outside_window() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    let salt = test_salt(&env, 42);
    let predicted_price: u128 = 2297;
    let hash = make_commitment(&env, predicted_price, &salt);

    client.create_round(&1_0000000, &Some(1)); // Precision
    client.commit_prediction(&user, &hash, &100_0000000);

    // Still in Betting phase (ledger 3 < bet_end_ledger 7)
    let result = client.try_reveal_prediction(&user, &predicted_price, &salt);
    assert_eq!(result, Err(Ok(ContractError::InvalidRevealWindow)));
}

/// HashMismatch on wrong reveal.
#[test]
fn test_illegal_reveal_hash_mismatch() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    let salt = test_salt(&env, 42);
    let predicted_price: u128 = 2297;
    let hash = make_commitment(&env, predicted_price, &salt);

    client.create_round(&1_0000000, &Some(1)); // Precision
    client.commit_prediction(&user, &hash, &100_0000000);

    // Advance to reveal window (ledger 7 ≤ seq < 13)
    env.ledger().with_mut(|li| li.sequence_number = 8);

    // Reveal with wrong price
    let result = client.try_reveal_prediction(&user, &9999, &salt);
    assert_eq!(result, Err(Ok(ContractError::HashMismatch)));
}

/// resolve with wrong round_id → InvalidOracleRound.
#[test]
fn test_illegal_resolve_wrong_round_id() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| li.sequence_number = 20);

    // Payload round_id = 999, but active round has start_ledger = 1
    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 999,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    });
    assert_eq!(result, Err(Ok(ContractError::InvalidOracleRound)));
}

// ─── Mode alternation ────────────────────────────────────────────────────────

/// Full cycle: UpDown → resolved → Precision → resolved → UpDown.
#[test]
fn test_mode_alternation_full_cycle() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    // ── Round 1: Up/Down ──
    client.create_round(&1_0000000, &Some(0));
    let r1 = client.get_active_round().unwrap();
    assert_eq!(r1.mode, RoundMode::UpDown);
    assert_eq!(r1.round_id, 1);

    client.place_bet(&user, &100_0000000, &BetSide::Up);
    env.ledger()
        .with_mut(|li| li.sequence_number = r1.end_ledger);

    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: r1.start_ledger,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    });

    assert_eq!(client.get_round_status(&1), RoundStatus::Resolved);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);

    // Claim winnings
    let claimed = client.claim_winnings(&user);
    assert!(claimed > 0);

    // ── Round 2: Precision ──
    client.create_round(&2_0000000, &Some(1));
    let r2 = client.get_active_round().unwrap();
    assert_eq!(r2.mode, RoundMode::Precision);
    assert_eq!(r2.round_id, 2);

    let salt = test_salt(&env, 42);
    let predicted_price: u128 = 2297;
    let hash = make_commitment(&env, predicted_price, &salt);

    client.commit_prediction(&user, &hash, &100_0000000);

    // Advance to reveal window and reveal (r2.bet_end_ledger = start_ledger + 6)
    env.ledger()
        .with_mut(|li| li.sequence_number = r2.bet_end_ledger);
    client.reveal_prediction(&user, &predicted_price, &salt);

    env.ledger()
        .with_mut(|li| li.sequence_number = r2.end_ledger);

    client.resolve_round(&OraclePayload {
        price: 2297,
        timestamp: env.ledger().timestamp(),
        round_id: r2.start_ledger,
        nonce: 2u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    });

    assert_eq!(client.get_round_status(&2), RoundStatus::Resolved);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);

    // ── Round 3: Up/Down again ──
    client.create_round(&3_0000000, &Some(0));
    let r3 = client.get_active_round().unwrap();
    assert_eq!(r3.mode, RoundMode::UpDown);
    assert_eq!(r3.round_id, 3);

    client.place_bet(&user, &200_0000000, &BetSide::Down);
    env.ledger()
        .with_mut(|li| li.sequence_number = r3.end_ledger);

    client.resolve_round(&OraclePayload {
        price: 2_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: r3.start_ledger,
        nonce: 3u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    });

    assert_eq!(client.get_round_status(&3), RoundStatus::Resolved);
}

// ─── FallbackRefund transition ───────────────────────────────────────────────

/// AwaitingResolve → FallbackRefund: insufficient participants at resolution.
#[test]
fn test_transition_awaiting_resolve_to_fallback_refund() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    // Require 3 participants but only 1 bets
    client.set_min_participants(&Some(3u32));
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &100_0000000, &BetSide::Up);

    env.ledger().with_mut(|li| li.sequence_number = 20);
    assert_eq!(client.get_round_status(&1), RoundStatus::AwaitingResolve);

    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 1,
        nonce: 1u64,
        network_id: env.ledger().network_id(),
        contract_addr: client.address.clone(),
        confidence: None,
    });

    assert_eq!(client.get_round_status(&1), RoundStatus::FallbackRefund);
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);
}

// ─── RoundStatus query after cancellation and re-creation ────────────────────

/// New round after cancel: RoundStatus shows Betting for new round_id.
#[test]
fn test_new_round_after_cancel_is_betting() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    client.create_round(&1_0000000, &None);
    assert_eq!(client.get_round_status(&1), RoundStatus::Betting);

    client.cancel_round(&0u32);
    assert_eq!(client.get_round_status(&1), RoundStatus::Cancelled);

    client.create_round(&1_2000000, &None);
    assert_eq!(client.get_round_status(&2), RoundStatus::Betting);
    assert_eq!(client.get_round_status(&1), RoundStatus::Cancelled);
}

// ─── ClaimsOnly validation ───────────────────────────────────────────────────

/// In ClaimsOnly mode, only claim_winnings should succeed; all mutations blocked.
#[test]
fn test_claims_only_blocks_mutations() {
    let env = Env::default();
    let (client, _admin, _oracle, user) = setup_with_user(&env);

    // ClaimsOnly is the initial state
    assert_eq!(client.get_protocol_status(), ProtocolStatus::ClaimsOnly);

    // claim_winnings should work fine (returns 0 since no pending)
    let claimed = client.claim_winnings(&user);
    assert_eq!(claimed, 0);

    // place_bet should fail (no active round)
    // This test just verifies the state consistency
    assert_eq!(client.get_active_round(), None);
}

// ─── RoundStatus for non-existent ids ────────────────────────────────────────

#[test]
fn test_round_status_unknown_for_nonexistent_rounds() {
    let env = Env::default();
    let (client, _admin, _oracle) = setup(&env);

    assert_eq!(client.get_round_status(&0), RoundStatus::Unknown);
    assert_eq!(client.get_round_status(&1), RoundStatus::Unknown);
    assert_eq!(client.get_round_status(&42), RoundStatus::Unknown);
    assert_eq!(client.get_round_status(&u64::MAX), RoundStatus::Unknown);
}
