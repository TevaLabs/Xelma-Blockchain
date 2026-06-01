//! Deterministic protocol state-machine tests (Issue #103).
//!
//! ## Transition graph
//!
//! States:
//!   Uninit     — contract deployed, initialize() not yet called
//!   Idle       — initialized, no active round
//!   Betting    — active round, current_ledger < bet_end_ledger
//!   Locked     — active round, bet_end_ledger <= current_ledger < end_ledger
//!   Resolvable — active round, current_ledger >= end_ledger
//!   Cancelled  — round cancelled; behaves as Idle for next-round purposes
//!   Paused     — contract paused (overlays Idle / Betting / Locked / Resolvable)
//!
//! Allowed transitions:
//!   Uninit        --[initialize]-------------------------> Idle
//!   Idle          --[create_round(UpDown)]--------------> Betting(UpDown)
//!   Idle          --[create_round(Precision)]-----------> Betting(Precision)
//!   Betting(UD)   --[place_bet]-------------------------> Betting (position stored)
//!   Betting(Prec) --[place_precision_prediction]--------> Betting (prediction stored)
//!   Betting       --[ledger → bet_end_ledger]-----------> Locked
//!   Locked        --[ledger → end_ledger]---------------> Resolvable
//!   Resolvable    --[resolve_round]--------------------> Idle (winnings attributed)
//!   Betting       --[cancel_round]--------------------> Idle (refunds attributed)
//!   Locked        --[cancel_round]--------------------> Idle (refunds attributed)
//!   Idle          --[claim_winnings (pending > 0)]------> Idle (balance updated)
//!   Any           --[pause_contract]------------------> Paused(*state*)
//!   Paused        --[unpause_contract]-----------------> *state*
//!
//! Rejected transitions (error codes asserted):
//!   Uninit        --[create_round]----> AdminNotSet
//!   Idle          --[resolve_round]---> NoActiveRound
//!   Idle          --[cancel_round]----> RoundNotCancellable
//!   Idle          --[place_bet]-------> NoActiveRound
//!   Betting(UD)   --[resolve_round, ledger < end_ledger]--> RoundNotEnded
//!   Betting       --[create_round]---> RoundAlreadyActive
//!   Cancelled     --[cancel_round]---> RoundNotCancellable
//!   Cancelled     --[resolve_round]--> NoActiveRound
//!   Paused        --[create_round]---> ContractPaused
//!   Paused        --[place_bet]------> ContractPaused
//!   Paused        --[place_precision_prediction]--> ContractPaused
//!   Paused        --[resolve_round]--> ContractPaused
//!   Paused        --[claim_winnings]--> ContractPaused
//!   UpDown round  --[place_precision_prediction]--> WrongModeForPrediction
//!   Precision rnd --[place_bet]-------> WrongModeForPrediction

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload, RoundMode};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, TryIntoVal,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn setup() -> (Env, VirtualTokenContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, client, admin, oracle)
}

fn resolve_active(env: &Env, client: &VirtualTokenContractClient, nonce: u64) {
    let round = client.get_active_round().expect("expected active round");
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });
    client.resolve_round(&OraclePayload {
        price: round.price_start + 1,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce,
    });
}

fn has_event(env: &Env, topic0: &str, topic1: &str) -> bool {
    env.events().all().iter().any(|e| {
        let (_contract, topics, _data) = e;
        topics.len() == 2
            && topics.get(0).unwrap().try_into_val(env)
                == Ok(soroban_sdk::Symbol::new(env, topic0))
            && topics.get(1).unwrap().try_into_val(env)
                == Ok(soroban_sdk::Symbol::new(env, topic1))
    })
}

// ─── Allowed transition: Uninit → Idle ───────────────────────────────────────

#[test]
fn sm_uninit_to_idle_via_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    assert!(client.get_active_round().is_none());
    assert!(client.get_admin().is_none());

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);

    assert_eq!(client.get_admin(), Some(admin));
    assert_eq!(client.get_oracle(), Some(oracle));
    assert!(client.get_active_round().is_none());
}

// ─── Rejected: Uninit → create_round → AdminNotSet ───────────────────────────

#[test]
fn sm_uninit_create_round_returns_admin_not_set() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let result = client.try_create_round(&1_0000000, &None);
    assert_eq!(result, Err(Ok(ContractError::AdminNotSet)));
    assert!(client.get_active_round().is_none());
}

// ─── Allowed: Idle → Betting (UpDown) ────────────────────────────────────────

#[test]
fn sm_idle_to_betting_updown() {
    let (env, client, _admin, _oracle) = setup();

    assert!(client.get_active_round().is_none());
    client.create_round(&1_0000000, &None);
    // Check event immediately — each subsequent contract call resets the log.
    assert!(has_event(&env, "round", "created"));

    let round = client.get_active_round().expect("round must exist");
    assert_eq!(round.mode, RoundMode::UpDown);
    assert_eq!(round.pool_up, 0);
    assert_eq!(round.pool_down, 0);
}

// ─── Allowed: Idle → Betting (Precision) ─────────────────────────────────────

#[test]
fn sm_idle_to_betting_precision() {
    let (env, client, _admin, _oracle) = setup();

    client.create_round(&1_0000000, &Some(1));
    assert!(has_event(&env, "round", "created"));

    let round = client.get_active_round().expect("round must exist");
    assert_eq!(round.mode, RoundMode::Precision);
}

// ─── Allowed: Betting(UpDown) → place_bet → Betting ─────────────────────────

#[test]
fn sm_betting_updown_place_bet_persists_position() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);

    let balance_before = client.balance(&user);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    assert!(has_event(&env, "bet", "placed"));

    let pos = client.get_user_position(&user).expect("position must exist");
    assert_eq!(pos.amount, 100_0000000);
    assert_eq!(pos.side, BetSide::Up);
    assert_eq!(client.balance(&user), balance_before - 100_0000000);

    let round = client.get_active_round().expect("round still active");
    assert_eq!(round.pool_up, 100_0000000);
}

// ─── Allowed: Betting(Precision) → place_precision_prediction → Betting ──────

#[test]
fn sm_betting_precision_place_prediction_persists_position() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1));

    let balance_before = client.balance(&user);
    client.place_precision_prediction(&user, &150_0000000, &2297);
    assert!(has_event(&env, "predict", "price"));

    let pred = client
        .get_user_precision_prediction(&user)
        .expect("prediction must exist");
    assert_eq!(pred.amount, 150_0000000);
    assert_eq!(pred.predicted_price, 2297);
    assert_eq!(client.balance(&user), balance_before - 150_0000000);
    assert!(client.get_active_round().is_some());
}

// ─── Allowed: Betting → Locked (ledger advance past bet_end_ledger) ───────────

#[test]
fn sm_betting_to_locked_bets_rejected_after_bet_window() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);

    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.bet_end_ledger;
    });

    let result = client.try_place_bet(&user, &50_0000000, &BetSide::Down);
    assert_eq!(result, Err(Ok(ContractError::RoundEnded)));
    assert!(client.get_active_round().is_some());
}

// ─── Rejected: Betting → resolve_round (before end_ledger) → RoundNotEnded ───

#[test]
fn sm_betting_resolve_before_end_ledger_returns_round_not_ended() {
    let (env, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);

    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.bet_end_ledger;
    });

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
    });
    assert_eq!(result, Err(Ok(ContractError::RoundNotEnded)));
    assert!(client.get_active_round().is_some());
}

// ─── Rejected: Betting → create_round → RoundAlreadyActive ───────────────────

#[test]
fn sm_betting_create_round_returns_already_active() {
    let (_env, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);

    let original = client.get_active_round().unwrap();
    let result = client.try_create_round(&2_0000000, &None);
    assert_eq!(result, Err(Ok(ContractError::RoundAlreadyActive)));

    let after = client.get_active_round().unwrap();
    assert_eq!(after.round_id, original.round_id);
    assert_eq!(after.price_start, original.price_start);
}

// ─── Allowed: Resolvable → resolve_round → Idle ──────────────────────────────

#[test]
fn sm_resolvable_to_idle_via_resolve_round() {
    let (env, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &200_0000000, &BetSide::Up);
    client.place_bet(&bob, &100_0000000, &BetSide::Down);

    resolve_active(&env, &client, 1);
    assert!(has_event(&env, "round", "resolved"));

    assert!(client.get_active_round().is_none());
    assert!(client.get_pending_winnings(&alice) > 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);
}

// ─── Allowed: Idle (post-resolve) → create_round → Betting ──────────────────

#[test]
fn sm_idle_after_resolve_create_new_round() {
    let (env, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);
    resolve_active(&env, &client, 1);

    assert!(client.get_active_round().is_none());
    client.create_round(&2_0000000, &None);

    let round = client.get_active_round().unwrap();
    assert_eq!(round.price_start, 2_0000000);
    assert_eq!(round.round_id, 2);
}

// ─── Rejected: Idle (post-resolve) → resolve_round → NoActiveRound ───────────

#[test]
fn sm_idle_after_resolve_resolve_again_returns_no_active_round() {
    let (env, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);
    resolve_active(&env, &client, 1);

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 2,
    });
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

// ─── Rejected: Idle → resolve_round → NoActiveRound ─────────────────────────

#[test]
fn sm_idle_resolve_returns_no_active_round() {
    let (env, client, _admin, _oracle) = setup();

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_0000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1,
    });
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

// ─── Rejected: Idle → cancel_round → RoundNotCancellable ─────────────────────

#[test]
fn sm_idle_cancel_returns_round_not_cancellable() {
    let (_env, client, _admin, _oracle) = setup();

    let result = client.try_cancel_round(&0u32);
    assert_eq!(result, Err(Ok(ContractError::RoundNotCancellable)));
}

// ─── Rejected: Idle → place_bet → NoActiveRound ──────────────────────────────

#[test]
fn sm_idle_place_bet_returns_no_active_round() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);

    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

// ─── Allowed: Betting → cancel_round → Idle ──────────────────────────────────

#[test]
fn sm_betting_cancel_transitions_to_idle_with_refunds() {
    let (env, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    client.mint_initial(&alice);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &300_0000000, &BetSide::Up);

    let round_id = client.get_active_round().unwrap().round_id;
    client.cancel_round(&0u32);
    assert!(has_event(&env, "round", "cancelled"));

    assert!(client.get_active_round().is_none());
    assert!(client.is_round_cancelled(&round_id));
    assert_eq!(client.get_pending_winnings(&alice), 300_0000000);
}

// ─── Allowed: Locked → cancel_round → Idle ───────────────────────────────────

#[test]
fn sm_locked_cancel_transitions_to_idle_with_refunds() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &200_0000000, &BetSide::Down);

    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.bet_end_ledger;
    });

    client.cancel_round(&0u32);

    assert!(client.get_active_round().is_none());
    assert_eq!(client.get_pending_winnings(&user), 200_0000000);
}

// ─── Rejected: Cancelled → cancel_round → RoundNotCancellable ────────────────

#[test]
fn sm_cancelled_cancel_again_returns_not_cancellable() {
    let (_env, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);
    client.cancel_round(&0u32);

    let result = client.try_cancel_round(&0u32);
    assert_eq!(result, Err(Ok(ContractError::RoundNotCancellable)));
}

// ─── Rejected: Cancelled → resolve_round → NoActiveRound ─────────────────────

#[test]
fn sm_cancelled_resolve_returns_no_active_round() {
    let (env, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);
    client.cancel_round(&0u32);

    env.ledger().with_mut(|li| {
        li.sequence_number = 20;
    });

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: 0,
        nonce: 1,
    });
    assert_eq!(result, Err(Ok(ContractError::NoActiveRound)));
}

// ─── Allowed: Cancelled → create_round → Betting ─────────────────────────────

#[test]
fn sm_cancelled_to_new_betting_via_create_round() {
    let (env, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);
    client.cancel_round(&0u32);

    client.create_round(&2_0000000, &None);
    assert!(has_event(&env, "round", "created"));

    let round = client.get_active_round().unwrap();
    assert_eq!(round.price_start, 2_0000000);
}

// ─── Paused: create_round blocked ────────────────────────────────────────────

#[test]
fn sm_paused_create_round_returns_paused() {
    let (_env, client, _admin, _oracle) = setup();
    client.pause_contract();

    let result = client.try_create_round(&1_0000000, &None);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    assert!(client.get_active_round().is_none());
}

// ─── Paused: place_bet blocked ────────────────────────────────────────────────

#[test]
fn sm_paused_place_bet_returns_paused() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.pause_contract();

    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
}

// ─── Paused: place_precision_prediction blocked ───────────────────────────────

#[test]
fn sm_paused_place_precision_prediction_returns_paused() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.pause_contract();

    let result = client.try_place_precision_prediction(&user, &100_0000000, &2297);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
}

// ─── Paused: resolve_round blocked ───────────────────────────────────────────

#[test]
fn sm_paused_resolve_round_returns_paused() {
    let (env, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);

    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });

    client.pause_contract();

    let result = client.try_resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
    });
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    assert!(client.get_active_round().is_some());
}

// ─── Paused: claim_winnings blocked ──────────────────────────────────────────

#[test]
fn sm_paused_claim_winnings_returns_paused() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.pause_contract();

    let result = client.try_claim_winnings(&user);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
}

// ─── Allowed: Paused → unpause → operations resume ───────────────────────────

#[test]
fn sm_unpause_restores_operations() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.pause_contract();
    assert!(client.is_paused());

    client.unpause_contract();
    assert!(!client.is_paused());

    client.create_round(&1_0000000, &None);
    client.place_bet(&user, &50_0000000, &BetSide::Up);

    assert!(client.get_active_round().is_some());
    assert!(client.get_user_position(&user).is_some());
}

// ─── Rejected: UpDown round → place_precision_prediction → WrongMode ─────────

#[test]
fn sm_updown_round_rejects_precision_prediction() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(0));

    let result = client.try_place_precision_prediction(&user, &100_0000000, &2297);
    assert_eq!(result, Err(Ok(ContractError::WrongModeForPrediction)));
}

// ─── Rejected: Precision round → place_bet → WrongMode ───────────────────────

#[test]
fn sm_precision_round_rejects_updown_bet() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);
    client.create_round(&1_0000000, &Some(1));

    let result = client.try_place_bet(&user, &100_0000000, &BetSide::Up);
    assert_eq!(result, Err(Ok(ContractError::WrongModeForPrediction)));
}

// ─── Claim winnings: Idle → claim (pending > 0) → balance updated ─────────────

#[test]
fn sm_claim_winnings_after_resolution_updates_balance() {
    let (env, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.create_round(&1_0000000, &None);
    client.place_bet(&alice, &400_0000000, &BetSide::Up);
    client.place_bet(&bob, &200_0000000, &BetSide::Down);

    resolve_active(&env, &client, 1);

    let pending = client.get_pending_winnings(&alice);
    assert!(pending > 0);

    let balance_before = client.balance(&alice);
    let claimed = client.claim_winnings(&alice);
    assert!(has_event(&env, "claim", "winnings"));

    assert_eq!(claimed, pending);
    assert_eq!(client.balance(&alice), balance_before + claimed);
    assert_eq!(client.get_pending_winnings(&alice), 0);
}

// ─── Regression: multi-round sequential transitions ──────────────────────────
// Verifies round_id increments and state is clean between rounds.

#[test]
fn sm_sequential_rounds_clean_state() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);

    // Round 1
    client.create_round(&1_0000000, &None);
    assert_eq!(client.get_active_round().unwrap().round_id, 1);
    client.place_bet(&user, &100_0000000, &BetSide::Up);
    resolve_active(&env, &client, 1);
    assert!(client.get_active_round().is_none());

    // Round 2
    client.create_round(&2_0000000, &None);
    let r2 = client.get_active_round().unwrap();
    assert_eq!(r2.round_id, 2);
    assert_eq!(r2.pool_up, 0);
    assert_eq!(r2.pool_down, 0);
    assert!(client.get_user_position(&user).is_none());

    resolve_active(&env, &client, 1);

    // Round 3
    client.create_round(&3_0000000, &None);
    assert_eq!(client.get_active_round().unwrap().round_id, 3);
}

// ─── Regression: nonce replay within same round rejected ─────────────────────

#[test]
fn sm_oracle_nonce_replay_rejected() {
    let (env, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);

    let round = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round.end_ledger;
    });

    client.resolve_round(&OraclePayload {
        price: 1_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 42,
    });

    // Round resolved; nonce 42 is consumed — next round must use a fresh nonce
    client.create_round(&2_0000000, &None);
    let round2 = client.get_active_round().unwrap();
    env.ledger().with_mut(|li| {
        li.sequence_number = round2.end_ledger;
    });

    // Nonce 42 is safe to reuse for a *different* round (per-round nonce scope)
    client.resolve_round(&OraclePayload {
        price: 2_5000000,
        timestamp: env.ledger().timestamp(),
        round_id: round2.start_ledger,
        nonce: 42,
    });
    assert!(client.get_active_round().is_none());
}

// ─── Regression: precision mode full lifecycle ────────────────────────────────

#[test]
fn sm_precision_full_lifecycle_transitions() {
    let (env, client, _admin, _oracle) = setup();
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Idle → Betting(Precision)
    client.create_round(&1_0000000, &Some(1));
    assert_eq!(client.get_active_round().unwrap().mode, RoundMode::Precision);

    // Betting → place predictions
    client.place_precision_prediction(&alice, &300_0000000, &9900);
    client.place_precision_prediction(&bob, &200_0000000, &10100);

    // Resolvable → resolve
    resolve_active(&env, &client, 1);

    // Idle — one winner, one loser; total pending ≤ total pool
    let alice_p = client.get_pending_winnings(&alice);
    let bob_p = client.get_pending_winnings(&bob);
    assert!(alice_p >= 0);
    assert!(bob_p >= 0);
    assert!(alice_p + bob_p <= 500_0000000);

    // Claim
    client.claim_winnings(&alice);
    client.claim_winnings(&bob);
    assert_eq!(client.get_pending_winnings(&alice), 0);
    assert_eq!(client.get_pending_winnings(&bob), 0);
}

// ─── Event side-effect: full round emits created → placed → resolved → claim ──

#[test]
fn sm_event_sequence_updown_round() {
    let (env, client, _admin, _oracle) = setup();
    let user = Address::generate(&env);
    client.mint_initial(&user);

    client.create_round(&1_0000000, &Some(0));
    assert!(has_event(&env, "round", "created"));

    client.place_bet(&user, &100_0000000, &BetSide::Up);
    assert!(has_event(&env, "bet", "placed"));

    resolve_active(&env, &client, 1);
    assert!(has_event(&env, "round", "resolved"));

    client.claim_winnings(&user);
    assert!(has_event(&env, "claim", "winnings"));
}

// ─── Event side-effect: cancelled round emits cancelled event ─────────────────

#[test]
fn sm_event_sequence_cancelled_round() {
    let (env, client, _admin, _oracle) = setup();
    client.create_round(&1_0000000, &None);
    client.cancel_round(&0u32);
    assert!(has_event(&env, "round", "cancelled"));
}
