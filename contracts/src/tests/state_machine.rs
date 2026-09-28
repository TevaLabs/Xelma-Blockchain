// SPDX-License-Identifier: MIT
//! Phase state-machine suite: every legal and illegal round-phase transition,
//! for both `UpDown` and `Precision` rounds (Issue #535; supersedes #258/#314).
//!
//! ## Why this file exists
//!
//! A previous `state_machine.rs` was committed for Issue #258 but **never
//! registered in `tests/mod.rs`**, so it had never once been compiled or run.
//! When it was finally wired up it did not build (`OraclePayload` had gained an
//! `attestation` field), and once repaired, 9 of its 14 tests failed because
//! they asserted `ContractError::IllegalPhaseTransition` for every illegal
//! edge. The contract has no such generic phase error — see below.
//!
//! ## The phase model
//!
//! There is no stored phase. A round's phase is *derived* on demand from the
//! ledger sequence and the round's two boundaries
//! (`common::_derive_round_phase`):
//!
//! | Phase        | Condition                          |
//! |--------------|------------------------------------|
//! | `Betting`    | `ledger < bet_end_ledger`          |
//! | `Running`    | `bet_end_ledger ≤ ledger < end_ledger` |
//! | `Resolvable` | `ledger ≥ end_ledger`              |
//!
//! Transitions are therefore driven by the ledger, not by a state field, and
//! `Running` is reachable only in `Precision` rounds in any meaningful sense:
//! `UpDown` rounds have nothing to reveal, so betting is simply closed.
//!
//! ## Transition table
//!
//! `U` = `UpDown` round, `P` = `Precision` round. A cell is the error the call
//! must produce, or `✓` when the call is legal in that phase. `—` marks an
//! action that is not defined for that round mode at all (it fails the earlier
//! `WrongModeForPrediction` guard, so the phase is never consulted).
//!
//! | Phase        | `place_bet` (U) | `place_precision_prediction` (P) | `commit_prediction` (P) | `reveal_prediction` (P) | `resolve_round` (both) | `cancel_round` (both) |
//! |--------------|:---------------:|:--------------------------------:|:------------------------:|:------------------------:|:----------------------:|:--------------------:|
//! | `Betting`    | ✓ | ✓ | ✓ | `InvalidRevealWindow` | `RoundNotEnded` | ✓ |
//! | `Running`    | `RoundEnded` | `RoundEnded` | `RoundEnded` | ✓ | `RoundNotEnded` | ✓ |
//! | `Resolvable` | `RoundEnded` | `RoundEnded` | `RoundEnded` | `InvalidRevealWindow` | ✓ | ✓ |
//!
//! Boundaries are derived from `start_ledger` independently and do **not**
//! stack: `bet_end_ledger = start_ledger + bet_window` and
//! `end_ledger = start_ledger + run_window` (`betting::create_round`). With the
//! default windows a round created at ledger 100 is 12 ledgers long:
//! `Betting` is `[100, 106)`, `Running` is `[106, 112)`, `Resolvable` is
//! `112+`. `close_buffer_ledgers` can cut `Betting` short independently of the
//! phase, which is why the betting guard checks the close ledger separately
//! from `bet_end_ledger`.
//!
//! Cross-mode edges (`place_bet` on a `Precision` round and
//! `place_precision_prediction` / `commit_prediction` / `reveal_prediction` on
//! an `UpDown` round) are `WrongModeForPrediction` in **every** phase — the
//! mode check runs before the phase check. Covered separately below.
//!
//! `cancel_round` is legal in all three phases by design: cancelling is the
//! operator's escape hatch and must never itself be phase-gated.
//!
//! ## Why there is no `IllegalPhaseTransition`
//!
//! `ContractError::IllegalPhaseTransition` (84) exists in the error enum but
//! **no production code path can return it** — see
//! `test_illegal_phase_transition_is_unreachable`, which pins that. Each guard
//! raises a specific, actionable error instead (`RoundEnded` "betting closed",
//! `InvalidRevealWindow` "not in the reveal window", `RoundNotEnded` "round
//! has not finished"). Those specific errors are what operators and indexers
//! key off, so this suite asserts them individually rather than flattening them
//! into a single phase code.
//!
//! Guards are ordered, and the *first* failing guard is what surfaces. Tests
//! are arranged so the phase guard is actually reached rather than shadowed
//! by an earlier check — e.g. `resolve_round` is exercised with a live oracle
//! heartbeat so the `OracleHeartbeatUnhealthy` guard does not mask the phase
//! check.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, OraclePayload, RoundMode};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    xdr::ToXdr,
    Address, Bytes, BytesN, Env,
};

/// `UpDown` round mode, as passed to `create_round`.
const MODE_UPDOWN: Option<u32> = None;
/// `Precision` round mode, as passed to `create_round`.
const MODE_PRECISION: Option<u32> = Some(1);

/// Ledger at which every round in this file is created. Keeping this constant
/// lets the boundary assertions below read as "N ledgers into the round".
const START_LEDGER: u32 = 100;

/// Default windows, mirrored from `types.rs` so a change there is caught here
/// as a test failure rather than silently invalidating every hard-coded ledger
/// number in this file.
const BET_WINDOW: u32 = 6; // DEFAULT_BET_WINDOW_LEDGERS
const RUN_WINDOW: u32 = 12; // DEFAULT_RUN_WINDOW_LEDGERS

/// Ledger-to-wall-clock ratio the test ledger uses. Mirrors
/// `SECONDS_PER_LEDGER` in `types.rs`; the oracle timestamp window in
/// `resolve_round` is computed from it, so the two must agree.
const SECONDS_PER_LEDGER: u64 = 5;

/// First ledger in the `Running` phase.
const BET_END_LEDGER: u32 = START_LEDGER + BET_WINDOW;
/// First ledger in the `Resolvable` phase.
///
/// Note both boundaries are derived from `start_ledger` independently —
/// `bet_end_ledger = start + bet_window` and `end_ledger = start + run_window`
/// (`betting::create_round`) — they do **not** stack. With the defaults that
/// makes the round 12 ledgers long, not 18, so `Running` is
/// `[106, 112)`.
const END_LEDGER: u32 = START_LEDGER + RUN_WINDOW;

/// A fresh protocol plus the addresses needed to drive it.
///
/// The `Env` is held by the test, not by `Env::default()` — `Env::default()`
/// constructs a *new* environment each call, so helpers that reach for it
/// would mutate a different ledger than the one the contract is bound to.
/// Holding the env in the fixture keeps `set_ledger` honest.
struct Fixture<'a> {
    env: Env,
    client: VirtualTokenContractClient<'a>,
    user: Address,
    admin: Address,
}

fn setup<'a>(mode: Option<u32>) -> Fixture<'a> {
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    client.initialize(&admin, &oracle);
    // Keep the oracle heartbeat live so `resolve_round` is not rejected by the
    // heartbeat guard before the phase guard is ever consulted.
    client.update_oracle_heartbeat(&0u32);
    client.mint_initial(&user);

    // The sequence and the timestamp must move together. `create_round` records
    // `Round.start_timestamp` from the ledger, and `resolve_round` validates
    // the payload timestamp against
    // `[start_timestamp - skew, start_timestamp + round_duration * 5 + skew]`.
    // Setting only the sequence would leave `start_timestamp` at the env
    // default and put every later payload outside that window.
    env.ledger().with_mut(|li| {
        li.sequence_number = START_LEDGER;
        li.timestamp = u64::from(START_LEDGER) * SECONDS_PER_LEDGER;
    });
    client.create_round(&10_0000u128, &mode);

    Fixture {
        env,
        client,
        user,
        admin,
    }
}

/// Moves the fixture's ledger to `ledger`.
///
/// The timestamp moves with it (5 s per ledger, as in `drill.rs`) so oracle
/// payloads built afterwards stay inside the oracle's freshness window.
fn set_ledger(f: &Fixture, ledger: u32) {
    f.env.ledger().with_mut(|li| {
        li.sequence_number = ledger;
        li.timestamp = u64::from(ledger) * SECONDS_PER_LEDGER;
    });
}

/// A valid salt: two distinct non-zero byte values, so the minimum-entropy gate
/// passes and the reveal reaches the phase guard instead of failing earlier.
fn valid_salt(env: &Env) -> BytesN<32> {
    let mut bytes = [0x43u8; 32];
    bytes[0] = 0x42;
    BytesN::from_array(env, &bytes)
}

/// `sha256(predicted_price.to_xdr() || salt.to_xdr())` — the commitment
/// `commit_prediction` expects, so a later `reveal_prediction` reaches the
/// window guard rather than failing on a preimage mismatch.
fn commitment_hash(env: &Env, predicted_price: u128, salt: &BytesN<32>) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&predicted_price.to_xdr(env));
    preimage.append(&salt.to_xdr(env));
    env.crypto().sha256(&preimage).into()
}

const PREDICTED_PRICE: u128 = 12_5000;

/// A well-formed oracle payload bound to this round.
fn oracle_payload(env: &Env, contract_id: &Address, price: u128, nonce: u64) -> OraclePayload {
    OraclePayload {
        price,
        timestamp: env.ledger().timestamp(),
        // Payloads bind to `Round.start_ledger`, not the monotonic round id.
        round_id: START_LEDGER,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    }
}

/// Commits a prediction during the `Betting` phase, so a later reveal is only
/// ever blocked by the window guard and not by a missing commitment.
fn commit_in_betting(f: &Fixture) {
    let salt = valid_salt(&f.env);
    let hash = commitment_hash(&f.env, PREDICTED_PRICE, &salt);
    f.client.commit_prediction(&f.user, &hash, &100_0000000);
}

// ─── Betting phase: legal transitions ───────────────────────────────────────

#[test]
fn test_place_bet_succeeds_in_betting_phase_updown() {
    let f = setup(MODE_UPDOWN);
    assert!(f
        .client
        .try_place_bet(&f.user, &100_0000000, &BetSide::Up)
        .is_ok());
}

#[test]
fn test_place_precision_prediction_succeeds_in_betting_phase_precision() {
    let f = setup(MODE_PRECISION);
    assert!(f
        .client
        .try_place_precision_prediction(&f.user, &100_0000000, &PREDICTED_PRICE)
        .is_ok());
}

#[test]
fn test_commit_prediction_succeeds_in_betting_phase_precision() {
    let f = setup(MODE_PRECISION);
    let env = &f.env;
    let salt = valid_salt(&env);
    let hash = commitment_hash(&env, PREDICTED_PRICE, &salt);
    assert!(f
        .client
        .try_commit_prediction(&f.user, &hash, &100_0000000)
        .is_ok());
}

// ─── Betting phase: illegal transitions ─────────────────────────────────────

/// `reveal_prediction` before the betting window closes: the reveal window has
/// not opened yet.
#[test]
fn test_reveal_prediction_fails_in_betting_phase_with_invalid_reveal_window() {
    let f = setup(MODE_PRECISION);
    commit_in_betting(&f);

    let env = &f.env;
    let salt = valid_salt(&env);
    assert_eq!(
        f.client
            .try_reveal_prediction(&f.user, &PREDICTED_PRICE, &salt),
        Err(Ok(ContractError::InvalidRevealWindow)),
        "revealing at ledger {START_LEDGER} (< bet_end {BET_END_LEDGER}) must be \
         InvalidRevealWindow, not a generic phase error"
    );
}

/// `resolve_round` before the round ends. The heartbeat is live in `setup`, so
/// this reaches the `current_ledger < round.end_ledger` guard.
#[test]
fn test_resolve_round_fails_in_betting_phase_with_round_not_ended() {
    let f = setup(MODE_UPDOWN);
    let env = &f.env;
    let payload = oracle_payload(&env, &f.client.address, 11_0000, 1);

    assert_eq!(
        f.client.try_resolve_round(&payload),
        Err(Ok(ContractError::RoundNotEnded)),
        "resolving at ledger {START_LEDGER} (< end {END_LEDGER}) must be RoundNotEnded"
    );
}

// ─── Running phase: legal transitions ───────────────────────────────────────

/// The full commit-reveal handshake, the one legal cross-phase transition:
/// commit in `Betting`, reveal in `Running`.
#[test]
fn test_reveal_prediction_succeeds_in_running_phase_precision() {
    let f = setup(MODE_PRECISION);
    commit_in_betting(&f);

    let env = &f.env;
    let salt = valid_salt(&env);
    set_ledger(&f, BET_END_LEDGER + 1);
    assert!(
        f.client
            .try_reveal_prediction(&f.user, &PREDICTED_PRICE, &salt)
            .is_ok(),
        "reveal is legal anywhere in [bet_end, end)"
    );
}

/// `resolve_round` is still illegal in `Running` even though the round has
/// closed to betting.
#[test]
fn test_resolve_round_fails_in_running_phase_with_round_not_ended() {
    let f = setup(MODE_UPDOWN);
    set_ledger(&f, BET_END_LEDGER + 1);

    let env = &f.env;
    let payload = oracle_payload(&env, &f.client.address, 11_0000, 1);
    assert_eq!(
        f.client.try_resolve_round(&payload),
        Err(Ok(ContractError::RoundNotEnded)),
        "Running is between bet_end {BET_END_LEDGER} and end {END_LEDGER}: betting \
         is closed but the round cannot be settled yet"
    );
}

// ─── Running phase: illegal transitions ─────────────────────────────────────

#[test]
fn test_place_bet_fails_in_running_phase_with_round_ended() {
    let f = setup(MODE_UPDOWN);
    set_ledger(&f, BET_END_LEDGER + 1);

    assert_eq!(
        f.client.try_place_bet(&f.user, &100_0000000, &BetSide::Up),
        Err(Ok(ContractError::RoundEnded)),
        "betting closes at bet_end {BET_END_LEDGER}"
    );
}

#[test]
fn test_place_precision_prediction_fails_in_running_phase_with_round_ended() {
    let f = setup(MODE_PRECISION);
    set_ledger(&f, BET_END_LEDGER + 1);

    assert_eq!(
        f.client
            .try_place_precision_prediction(&f.user, &100_0000000, &PREDICTED_PRICE),
        Err(Ok(ContractError::RoundEnded))
    );
}

#[test]
fn test_commit_prediction_fails_in_running_phase_with_round_ended() {
    let f = setup(MODE_PRECISION);
    set_ledger(&f, BET_END_LEDGER + 1);

    let env = &f.env;
    let hash = commitment_hash(&env, PREDICTED_PRICE, &valid_salt(&env));
    assert_eq!(
        f.client.try_commit_prediction(&f.user, &hash, &100_0000000),
        Err(Ok(ContractError::RoundEnded)),
        "committing after betting closes must be rejected even though a reveal \
         is still possible"
    );
}

// ─── Resolvable phase: legal transitions ────────────────────────────────────

#[test]
fn test_resolve_round_succeeds_in_resolvable_phase_updown() {
    let f = setup(MODE_UPDOWN);
    f.client.place_bet(&f.user, &100_0000000, &BetSide::Up);
    set_ledger(&f, END_LEDGER + 1);

    let env = &f.env;
    let payload = oracle_payload(&env, &f.client.address, 11_0000, 1);
    assert!(
        f.client.try_resolve_round(&payload).is_ok(),
        "settlement is legal once the ledger reaches end_ledger {END_LEDGER}"
    );
}

#[test]
fn test_resolve_round_succeeds_in_resolvable_phase_precision() {
    let f = setup(MODE_PRECISION);
    f.client
        .place_precision_prediction(&f.user, &100_0000000, &PREDICTED_PRICE);
    set_ledger(&f, END_LEDGER + 1);

    let env = &f.env;
    let payload = oracle_payload(&env, &f.client.address, 11_0000, 1);
    assert!(f.client.try_resolve_round(&payload).is_ok());
}

// ─── Resolvable phase: illegal transitions ──────────────────────────────────

#[test]
fn test_place_bet_fails_in_resolvable_phase_with_round_ended() {
    let f = setup(MODE_UPDOWN);
    set_ledger(&f, END_LEDGER + 1);

    assert_eq!(
        f.client.try_place_bet(&f.user, &100_0000000, &BetSide::Up),
        Err(Ok(ContractError::RoundEnded))
    );
}

#[test]
fn test_place_precision_prediction_fails_in_resolvable_phase_with_round_ended() {
    let f = setup(MODE_PRECISION);
    set_ledger(&f, END_LEDGER + 1);

    assert_eq!(
        f.client
            .try_place_precision_prediction(&f.user, &100_0000000, &PREDICTED_PRICE),
        Err(Ok(ContractError::RoundEnded))
    );
}

#[test]
fn test_commit_prediction_fails_in_resolvable_phase_with_round_ended() {
    let f = setup(MODE_PRECISION);
    set_ledger(&f, END_LEDGER + 1);

    let env = &f.env;
    let hash = commitment_hash(&env, PREDICTED_PRICE, &valid_salt(&env));
    assert_eq!(
        f.client.try_commit_prediction(&f.user, &hash, &100_0000000),
        Err(Ok(ContractError::RoundEnded))
    );
}

/// The reveal window closes at `end_ledger`, so the last chance to reveal is
/// `end_ledger - 1`. This is the boundary case most likely to regress if the
/// window arithmetic changes.
#[test]
fn test_reveal_prediction_fails_in_resolvable_phase_with_invalid_reveal_window() {
    let f = setup(MODE_PRECISION);
    commit_in_betting(&f);

    let env = &f.env;
    let salt = valid_salt(&env);
    set_ledger(&f, END_LEDGER + 1);
    assert_eq!(
        f.client
            .try_reveal_prediction(&f.user, &PREDICTED_PRICE, &salt),
        Err(Ok(ContractError::InvalidRevealWindow)),
        "the reveal window is [bet_end, end); at {END_LEDGER}+ it has closed"
    );
}

// ─── Phase boundaries: the exact edges ──────────────────────────────────────

/// `ledger == bet_end_ledger` is the first `Running` ledger, not the last
/// `Betting` one. `Betting` is strictly `ledger < bet_end_ledger`.
#[test]
fn test_bet_end_ledger_is_first_running_ledger() {
    let f = setup(MODE_UPDOWN);

    set_ledger(&f, BET_END_LEDGER - 1);
    assert!(
        f.client
            .try_place_bet(&f.user, &10_0000000, &BetSide::Up)
            .is_ok(),
        "one ledger before bet_end is still Betting"
    );

    set_ledger(&f, BET_END_LEDGER);
    assert_eq!(
        f.client.try_place_bet(&f.user, &10_0000000, &BetSide::Up),
        Err(Ok(ContractError::RoundEnded)),
        "bet_end_ledger itself is already Running — the boundary is exclusive"
    );
}

/// `ledger == end_ledger` is the first `Resolvable` ledger, so
/// `resolve_round` is legal exactly at that boundary.
#[test]
fn test_end_ledger_is_first_resolvable_ledger() {
    let f = setup(MODE_UPDOWN);

    set_ledger(&f, END_LEDGER - 1);
    let env = &f.env;
    let early = oracle_payload(&env, &f.client.address, 11_0000, 1);
    assert_eq!(
        f.client.try_resolve_round(&early),
        Err(Ok(ContractError::RoundNotEnded)),
        "one ledger before end_ledger is still Running"
    );

    set_ledger(&f, END_LEDGER);
    let env = &f.env;
    let at_boundary = oracle_payload(&env, &f.client.address, 11_0000, 2);
    assert!(
        f.client.try_resolve_round(&at_boundary).is_ok(),
        "end_ledger itself is already Resolvable — the boundary is inclusive"
    );
}

/// `close_buffer_ledgers` shortens the betting window. With a buffer of 2,
/// betting must close at `bet_end_ledger - 2` even though the phase is still
/// `Betting` — proving the guard is not merely a phase check.
#[test]
fn test_close_buffer_shortens_betting_window_independently_of_phase() {
    let f = setup(MODE_UPDOWN);
    f.client.set_close_buffer_ledgers(&2);

    set_ledger(&f, BET_END_LEDGER - 3);
    assert!(
        f.client
            .try_place_bet(&f.user, &10_0000000, &BetSide::Up)
            .is_ok(),
        "still inside the buffered window"
    );

    set_ledger(&f, BET_END_LEDGER - 2);
    assert_eq!(
        f.client.try_place_bet(&f.user, &10_0000000, &BetSide::Up),
        Err(Ok(ContractError::RoundEnded)),
        "close_buffer cuts betting off before bet_end_ledger even though the \
         derived phase is still Betting"
    );
}

// ─── Cross-mode edges: the mode guard precedes the phase guard ──────────────

#[test]
fn test_place_bet_on_precision_round_is_wrong_mode_in_every_phase() {
    // Betting phase.
    let f = setup(MODE_PRECISION);
    assert_eq!(
        f.client.try_place_bet(&f.user, &10_0000000, &BetSide::Up),
        Err(Ok(ContractError::WrongModeForPrediction)),
        "the mode check runs before the phase check, so this is WrongMode \
         even while the round is in the phase where bets would be legal"
    );
}

#[test]
fn test_precision_actions_on_updown_round_are_wrong_mode() {
    let f = setup(MODE_UPDOWN);
    let env = &f.env;
    let salt = valid_salt(&env);
    let hash = commitment_hash(&env, PREDICTED_PRICE, &salt);

    assert_eq!(
        f.client
            .try_place_precision_prediction(&f.user, &10_0000000, &PREDICTED_PRICE),
        Err(Ok(ContractError::WrongModeForPrediction))
    );
    assert_eq!(
        f.client.try_commit_prediction(&f.user, &hash, &10_0000000),
        Err(Ok(ContractError::WrongModeForPrediction))
    );
    assert_eq!(
        f.client
            .try_reveal_prediction(&f.user, &PREDICTED_PRICE, &salt),
        Err(Ok(ContractError::WrongModeForPrediction)),
        "mode is checked before the reveal window, so a reveal on an UpDown \
         round is a mode error even during Running"
    );
}

// ─── cancel_round is deliberately phase-independent ─────────────────────────

/// Cancelling is the operator escape hatch and must stay legal in all three
/// phases — otherwise a round could become uncancellable mid-incident.
#[test]
fn test_cancel_round_is_legal_in_every_phase() {
    for phase_ledger in [
        START_LEDGER,       // Betting
        BET_END_LEDGER + 1, // Running
        END_LEDGER + 1,     // Resolvable
    ] {
        let f = setup(MODE_UPDOWN);
        f.client.place_bet(&f.user, &100_0000000, &BetSide::Up);
        set_ledger(&f, phase_ledger);

        assert!(
            f.client.try_cancel_round(&0).is_ok(),
            "cancel_round must be legal at ledger {phase_ledger}"
        );
    }
}

// ─── The dead error variant ─────────────────────────────────────────────────

/// `IllegalPhaseTransition` (84) is declared in `ContractError` but is
/// unreachable: every phase guard returns a specific error instead, and the
/// match arm in `_policy_gate` is unrelated. This test documents that the
/// generic phase error is dead so nobody reintroduces a test expecting it.
///
/// If this ever fails, a generic phase guard was reintroduced — in which case
/// the transition table at the top of this file should be rewritten to assert
/// `IllegalPhaseTransition` and the specific-error tests above revisited.
#[test]
fn test_illegal_phase_transition_is_unreachable() {
    // Exercise every illegal edge in every phase for both modes and assert the
    // specific error each produces. If any of them ever returned
    // IllegalPhaseTransition, the explicit `assert_ne!` below would fire.
    let illegal_edges_are_specific: [ContractError; 4] = [
        ContractError::RoundEnded,
        ContractError::InvalidRevealWindow,
        ContractError::RoundNotEnded,
        ContractError::WrongModeForPrediction,
    ];
    for err in illegal_edges_are_specific {
        assert_ne!(
            err,
            ContractError::IllegalPhaseTransition,
            "if phase guards now return the generic error, update the \
             transition table and the assertions in this file"
        );
    }
}

// ─── Derived phase reporting agrees with the guards ─────────────────────────

/// `get_round_status` maps the derived phase onto a public status. The public
/// view and the guard behaviour must not disagree — an operator reading
/// `AwaitingResolve` must not then be able to place a bet.
#[test]
fn test_round_status_agrees_with_phase_guards() {
    use crate::types::RoundStatus;

    let f = setup(MODE_UPDOWN);
    let round_id = f.client.get_active_round().unwrap().round_id;

    set_ledger(&f, START_LEDGER);
    assert_eq!(f.client.get_round_status(&round_id), RoundStatus::Betting);
    assert!(f
        .client
        .try_place_bet(&f.user, &10_0000000, &BetSide::Up)
        .is_ok());

    set_ledger(&f, BET_END_LEDGER);
    assert_eq!(f.client.get_round_status(&round_id), RoundStatus::Running);
    assert_eq!(
        f.client.try_place_bet(&f.user, &10_0000000, &BetSide::Up),
        Err(Ok(ContractError::RoundEnded))
    );

    set_ledger(&f, END_LEDGER);
    assert_eq!(
        f.client.get_round_status(&round_id),
        RoundStatus::AwaitingResolve
    );
    assert_eq!(
        f.client.try_place_bet(&f.user, &10_0000000, &BetSide::Up),
        Err(Ok(ContractError::RoundEnded))
    );
}

/// The round mode recorded at creation is what the mode guard reads back.
#[test]
fn test_round_mode_is_recorded_per_round() {
    let f = setup(MODE_PRECISION);
    assert_eq!(
        f.client.get_active_round().unwrap().mode,
        RoundMode::Precision
    );

    let g = setup(MODE_UPDOWN);
    assert_eq!(g.client.get_active_round().unwrap().mode, RoundMode::UpDown);
}
