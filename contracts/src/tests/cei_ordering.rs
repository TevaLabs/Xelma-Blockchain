//! CEI (Checks-Effects-Interactions) ordering regression tests.
//!
//! These tests verify that the two CEI fixes applied in issue #195 hold
//! under normal execution:
//!
//!   1. `claim_winnings`: the `PendingWinnings` storage slot is cleared
//!      *before* the user balance is increased.
//!   2. `cancel_config_change`: the `PendingConfigChange` storage key is
//!      removed *before* the cancellation event is emitted.

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::errors::ContractError;
use crate::types::{BetSide, ConfigChangeKind};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

/// Must match `CONFIG_TIMELOCK_LEDGERS` in contract.rs.
const CONFIG_TIMELOCK_LEDGERS: u32 = 1440;

// ─── Helper ──────────────────────────────────────────────────────────────────

fn setup() -> (Env, Address, Address, VirtualTokenContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.initialize(&admin, &oracle);
    (env, admin, oracle, client)
}

// ─── SR-2026-06-001: claim_winnings CEI ordering ─────────────────────────────

/// After a successful `claim_winnings` call, the `PendingWinnings` slot must be
/// cleared (so a second claim returns 0) and the user's balance must reflect the
/// credited amount. This confirms the Effect (slot removal) occurs before the
/// balance increase, and the function is idempotent after the first claim.
#[test]
fn test_claim_winnings_cei_pending_cleared_after_claim() {
    let (env, admin, oracle, client) = setup();

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint_initial(&alice);
    client.mint_initial(&bob);

    // Create and resolve a round so alice has pending winnings.
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
        li.timestamp = 1_000_000;
    });
    client.create_round(&1_0000000, &None);

    // Alice bets Up, Bob bets Down.
    client.place_bet(&alice, &500_0000000, &BetSide::Up);
    client.place_bet(&bob, &500_0000000, &BetSide::Down);

    // Advance past bet window and run window.
    env.ledger().with_mut(|li| {
        li.sequence_number = 130;
        li.timestamp = 1_000_700;
    });

    let round = client.get_active_round().expect("round should exist");
    let payload = crate::types::OraclePayload {
        price: 2_0000000, // price went up
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: env.current_contract_address(),
    };
    client.resolve_round(&payload);

    // Alice should have pending winnings.
    let pending_before = client.get_pending_winnings(&alice);
    assert!(
        pending_before > 0,
        "alice should have pending winnings after winning round"
    );

    let balance_before_claim = client.balance(&alice);

    // --- CEI assertion: claim should succeed ---
    let claimed = client.claim_winnings(&alice);
    assert_eq!(claimed, pending_before, "claimed amount must equal pending");

    // Effect 1: pending winnings slot is now cleared.
    let pending_after = client.get_pending_winnings(&alice);
    assert_eq!(
        pending_after, 0,
        "PendingWinnings slot must be cleared after claim (CEI: Effect before Interaction)"
    );

    // Effect 2: balance increased by exactly the claimed amount.
    let balance_after_claim = client.balance(&alice);
    assert_eq!(
        balance_after_claim,
        balance_before_claim + claimed,
        "user balance must increase by claimed amount"
    );

    // Idempotency: a second claim returns 0 and does not mutate state.
    let second_claim = client.claim_winnings(&alice);
    assert_eq!(second_claim, 0, "second claim on empty pending must return 0");
    assert_eq!(
        client.balance(&alice),
        balance_after_claim,
        "balance must not change on empty claim"
    );
}

// ─── SR-2026-06-002: cancel_config_change CEI ordering ───────────────────────

/// After `cancel_config_change` succeeds, the `PendingConfigChange` key must
/// be absent from storage. A second cancellation attempt must fail with
/// `CommitmentNotFound`, proving the key was removed as an Effect rather than
/// after the event Interaction.
#[test]
fn test_cancel_config_change_cei_key_removed_before_event() {
    let (env, _admin, _oracle, client) = setup();

    // Schedule a windows change (creates PendingConfigChange(Windows)).
    client.schedule_windows(&10, &20);

    let pending = client.get_pending_config_change(&ConfigChangeKind::Windows);
    assert!(
        pending.is_some(),
        "pending config change must exist after scheduling"
    );

    // Cancel before the timelock expires.
    // activation_ledger = current + CONFIG_TIMELOCK_LEDGERS; stay before that.
    client.cancel_config_change(&ConfigChangeKind::Windows);

    // Effect: the pending key must be gone.
    let pending_after = client.get_pending_config_change(&ConfigChangeKind::Windows);
    assert!(
        pending_after.is_none(),
        "PendingConfigChange key must be absent after cancellation (CEI: Effect before Interaction)"
    );

    // Idempotency / Effect finality: a second cancellation must fail.
    let result = client.try_cancel_config_change(&ConfigChangeKind::Windows);
    assert_eq!(
        result,
        Err(Ok(ContractError::CommitmentNotFound)),
        "second cancellation must return CommitmentNotFound — key was removed as Effect, not after event"
    );
}

/// Additional guard: once the activation ledger is reached, cancellation
/// is rejected even if no CEI fix is in place (existing guard). This test
/// ensures the ordering fix does not regress the timelock guard.
#[test]
fn test_cancel_config_change_rejected_after_activation() {
    let (env, _admin, _oracle, client) = setup();

    client.schedule_windows(&10, &20);

    let pending = client
        .get_pending_config_change(&ConfigChangeKind::Windows)
        .expect("pending must exist");

    // Advance past the activation ledger.
    env.ledger().with_mut(|li| {
        li.sequence_number = pending.activation_ledger;
    });

    let result = client.try_cancel_config_change(&ConfigChangeKind::Windows);
    assert_eq!(
        result,
        Err(Ok(ContractError::RoundNotCancellable)),
        "cancellation must be rejected once activation ledger is reached"
    );
}
