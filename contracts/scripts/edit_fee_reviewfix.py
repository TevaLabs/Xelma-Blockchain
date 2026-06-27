#!/usr/bin/env python3
"""Address material reviewer findings for Issue #162:

  1. Reorder `test_protocol_fee_schedule_validation_rejects_zero_and_over_cap`
     so the assertion fires for the right reason (each schedule precedes an
     apply/cancel, so `_schedule_config_change` does not bounce them on
     `RoundAlreadyActive` before reaching the bps validator).

  2. Expand `test_protocol_fee_not_collected_on_refund_paths` with explicit
     one-sided-pool coverage (price-unchanged was already tested). The
     acceptance criteria require guarantees for every refund path.

  3. Replace `(*bps,)` with `bps.clone()` in the new
     `_apply_config_payload` ProtocolFeeBps arm: same compile semantics since
     `Option<u32>: Copy`, but the intent is far clearer for future contributors
     if Option<u32> ever becomes e.g. Option<SomeStruct>.
"""
import sys

RES = '/workspaces/Xelma-Blockchain/contracts/src/tests/resolution.rs'
CON = '/workspaces/Xelma-Blockchain/contracts/src/contract.rs'

# ---------------------------------------------------------------------------
# Fix 1: rewrite validation test with the correct sequencing.
# ---------------------------------------------------------------------------
res = open(RES).read()

OLD_TEST = """#[test]
fn test_protocol_fee_schedule_validation_rejects_zero_and_over_cap() {
    // `Some(0)` rejected (use `None` to disable); `Some(1_001)` rejected
    // (over MAX_PROTOCOL_FEE_BPS = 1_000). Empty (None) is allowed.
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);

    // None always OK.
    client.schedule_protocol_fee_bps(&None);
    // Some(0) rejected.
    let r0 = client.try_schedule_protocol_fee_bps(&Some(0u32));
    assert!(r0.is_err(), "Some(0) is not a valid bps value");
    // Over cap rejected.
    let r_max = client.try_schedule_protocol_fee_bps(&Some(1_001u32));
    assert!(r_max.is_err(), "1_001 bps exceeds MAX_PROTOCOL_FEE_BPS=1000");
    // 1_000 (cap) accepted.
    // First clear the existing None-schedule.
    env.ledger().with_mut(|li| li.sequence_number = 1_000_000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );
    let r_top = client.try_schedule_protocol_fee_bps(&Some(1_000u32));
    assert!(r_top.is_ok(), "1_000 bps (MAX) must be accepted");
}"""

NEW_TEST = """#[test]
fn test_protocol_fee_schedule_validation_rejects_zero_and_over_cap() {
    // Each test cases schedules, fast-forwards past the timelock, and
    // applies/cancels before attempting the next one -- otherwise
    // `_schedule_config_change` would bounce subsequent calls on
    // `RoundAlreadyActive` and never reach the bps validator.
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

    // None always OK.
    client.schedule_protocol_fee_bps(&None);
    run_to_activation(&env);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);
    assert_eq!(client.get_protocol_fee_bps(), None);

    // Some(0) rejected -- explicit disable is the only legitimate way.
    let r0 = client.try_schedule_protocol_fee_bps(&Some(0u32));
    assert!(r0.is_err(), "Some(0) is not a valid bps value");
    run_to_activation(&env);
    client.cancel_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    // Over cap rejected.
    let r_max = client.try_schedule_protocol_fee_bps(&Some(1_001u32));
    assert!(r_max.is_err(), "1_001 bps exceeds MAX_PROTOCOL_FEE_BPS=1000");
    run_to_activation(&env);
    client.cancel_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps);

    // Cap (1_000) accepted.
    let r_top = client.try_schedule_protocol_fee_bps(&Some(1_000u32));
    assert!(r_top.is_ok(), "1_000 bps (MAX) must be accepted");
    run_to_activation(&env);
    client.apply_scheduled_changes(&crate::types::ConfigChangeKind::ProtocolFeeBps);
    assert_eq!(client.get_protocol_fee_bps(), Some(1_000u32));
}"""

if OLD_TEST not in res:
    print('resolution.rs: validation test anchor not found -- aborting', file=sys.stderr)
    sys.exit(1)
res = res.replace(OLD_TEST, NEW_TEST)
print('[resolution.rs] rewrote validation test (correct sequencing)')

# ---------------------------------------------------------------------------
# Fix 2: add explicit one-sided-pool coverage to refund test.
# ---------------------------------------------------------------------------
# We add a new test below the existing one (preserves test isolation).
ONE_SIDED_TEST = r"""
#[test]
fn test_protocol_fee_not_collected_on_one_sided_pool_refund() {
    // One-sided pool (only the losing side has bets) refunds all participants
    // without entering the winner-distribution path -- the fee MUST NOT be
    // collected even though `get_protocol_fee_bps` returns Some(active).
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.mint_initial(&alice);
    client.mint_initial(&bob);

    client.schedule_protocol_fee_bps(&Some(500u32)); // 5%
    env.ledger().with_mut(|li| li.sequence_number = 2_000);
    client.apply_scheduled_changes(
        &crate::types::ConfigChangeKind::ProtocolFeeBps,
    );

    let start_price: u128 = 1_500_0000;
    client.create_round(&start_price, &None);

    // ONLY down bets -- pool_up=0. Price goes UP -> one-sided refund of all.
    env.as_contract(&contract_id, || {
        let mut positions = Map::<Address, UserPosition>::new(&env);
        positions.set(alice.clone(), UserPosition { amount: 100_000_0000, side: BetSide::Down });
        positions.set(bob.clone(), UserPosition { amount: 50_000_0000, side: BetSide::Down });
        env.storage().persistent().set(&DataKey::UpDownPositions, &positions);
        let mut round: Round = env.storage().persistent().get(&DataKey::ActiveRound).unwrap();
        round.pool_up = 0;
        round.pool_down = 150_000_0000;
        env.storage().persistent().set(&DataKey::ActiveRound, &round);
    });

    env.ledger().with_mut(|li| li.sequence_number += 12);
    client.resolve_round(&OraclePayload {
        price: 1_700_0000, // up
        timestamp: env.ledger().timestamp(),
        round_id: 0u32,
        nonce: 9u64,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
    });

    assert_eq!(
        count_protocol_fee_events(&env),
        0,
        "one-sided refund MUST NOT emit a fee event"
    );
    assert_eq!(client.get_protocol_fee_treasury(), 0,
        "one-sided refund MUST NOT credit the treasury");
    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone()]);
    assert_eq!(
        payouts, 150_000_0000i128,
        "all participants refunded their full stake on one-sided pool"
    );
}
"""

# Find the closing `}` of the validation test (the last `#[test]` example we
# just rewrote) and insert the one-sided test AFTER the existing refund test.
# We anchor on the end of the price-unchanged refund test body.
INSERT_AFTER = (
    "    // Refund: no fee event, treasury still 0.\n"
    "    assert_eq!(count_protocol_fee_events(&env), 0,\n"
    "        \"price-unchanged refunds MUST NOT emit a fee event\");\n"
    "    assert_eq!(client.get_protocol_fee_treasury(), 0);\n"
    "    let payouts = sum_pending_payouts(&env, &[alice.clone(), bob.clone()]);\n"
    "    assert_eq!(payouts, 150_000_0000i128,\n"
    "        \"all participants refunded their full stake\");\n"
    "}\n"
)
if INSERT_AFTER not in res:
    print('resolution.rs: refund-test anchor not found', file=sys.stderr)
    sys.exit(1)
res = res.replace(INSERT_AFTER, INSERT_AFTER + ONE_SIDED_TEST)
print('[resolution.rs] appended one-sided refund coverage test')

open(RES, 'w').write(res)

# ---------------------------------------------------------------------------
# Fix 3: rewrite `(*bps,)` to `bps.clone()` in _apply_config_payload arm
# ---------------------------------------------------------------------------
con = open(CON).read()
OLD_ARM_EMIT = (
    "        #[allow(deprecated)]\n"
    "        env.events().publish(\n"
    "            (symbol_short!(\"protocol\"), symbol_short!(\"fee_bps_set\")),\n"
    "            (*bps,),\n"
    "        );\n"
)
NEW_ARM_EMIT = (
    "        // Payload ([`Option<u32>`]) is moved into the event so indexers see\n"
    "        // `None` for \"fee disabled\". `Option<u32>: Copy`, so `bps.clone()`\n"
    "        // is equivalent to and more transparent than `*bps`.\n"
    "        #[allow(deprecated)]\n"
    "        env.events().publish(\n"
    "            (symbol_short!(\"protocol\"), symbol_short!(\"fee_bps_set\")),\n"
    "            (bps.clone(),),\n"
    "        );\n"
)
if OLD_ARM_EMIT not in con:
    print('contract.rs: emit-arm anchor not found', file=sys.stderr)
    sys.exit(1)
con = con.replace(OLD_ARM_EMIT, NEW_ARM_EMIT)
open(CON, 'w').write(con)
print('[contract.rs] _apply_config_payload arm clarified')

print(f'\nresolution.rs: now {len(open(RES).read().splitlines())} lines')
print(f'contract.rs:   now {len(open(CON).read().splitlines())} lines')
