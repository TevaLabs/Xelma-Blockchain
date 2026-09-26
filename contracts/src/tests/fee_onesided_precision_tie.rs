// SPDX-License-Identifier: MIT
//! Cross-product property suite for Issue #563.
//!
//! Every combination of fee incidence (`FeeOnPot`, `FeeOnWinnings`), fee rate
//! (disabled and 500 bps), and market shape (one-sided up, one-sided down,
//! two-sided, precision unique winner, precision 2-way tie, precision 3-way
//! tie) is settled and checked for conservation. Failure output names the
//! combination, the accounted value, and the expected fee.

use std::format;

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::{BetSide, DataKeyCore, FeeModel, OraclePayload};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

const MINT: i128 = 1000_0000000;
const ALICE: i128 = 100_0000000;
const BOB: i128 = 200_0000000;
const CHARLIE: i128 = 150_0000000;
const BPS: u32 = 500;

#[derive(Clone, Copy, Debug)]
enum Shape {
    OneSidedUp,
    OneSidedDown,
    TwoSided,
    PrecisionUnique,
    PrecisionTieTwo,
    PrecisionTieThree,
}

impl Shape {
    fn all() -> [Shape; 6] {
        [
            Shape::OneSidedUp,
            Shape::OneSidedDown,
            Shape::TwoSided,
            Shape::PrecisionUnique,
            Shape::PrecisionTieTwo,
            Shape::PrecisionTieThree,
        ]
    }

    fn one_sided(self) -> bool {
        matches!(self, Shape::OneSidedUp | Shape::OneSidedDown)
    }

    fn pot(self) -> i128 {
        match self {
            Shape::OneSidedUp => ALICE + BOB,
            Shape::OneSidedDown => ALICE + BOB,
            Shape::TwoSided => ALICE + BOB + CHARLIE,
            Shape::PrecisionUnique | Shape::PrecisionTieTwo | Shape::PrecisionTieThree => {
                ALICE + BOB + CHARLIE
            }
        }
    }

    /// Stakes that belong to the winning set. One-sided markets refund, so the
    /// taxable profit base is unused (fee is forced to 0).
    fn winner_stakes(self) -> i128 {
        match self {
            Shape::OneSidedUp | Shape::OneSidedDown => 0,
            Shape::TwoSided => ALICE + BOB,
            Shape::PrecisionUnique => ALICE,
            Shape::PrecisionTieTwo => ALICE + BOB,
            Shape::PrecisionTieThree => ALICE + BOB + CHARLIE,
        }
    }
}

fn expected_fee(model: FeeModel, bps: Option<u32>, shape: Shape) -> i128 {
    if shape.one_sided() {
        return 0;
    }
    let Some(bps) = bps else {
        return 0;
    };
    let pot = shape.pot();
    let base = match model {
        FeeModel::FeeOnPot => pot,
        FeeModel::FeeOnWinnings => match shape {
            Shape::TwoSided => CHARLIE,
            _ => pot - shape.winner_stakes(),
        },
    };
    if base <= 0 {
        return 0;
    }
    base * i128::from(bps) / 10_000
}

fn settle_combination(model: FeeModel, bps: Option<u32>, shape: Shape) {
    let label = format!("model={model:?} bps={bps:?} shape={shape:?}");
    let env = Env::default();
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);
    client.mint_initial(&alice);
    client.mint_initial(&bob);
    client.mint_initial(&charlie);

    client.set_fee_model(&model);
    if let Some(rate) = bps {
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&DataKeyCore::ProtocolFeeBps, &rate);
        });
    }

    let precision = matches!(
        shape,
        Shape::PrecisionUnique | Shape::PrecisionTieTwo | Shape::PrecisionTieThree
    );
    if precision {
        client.create_round(&1_0000000u128, &Some(1u32));
    } else {
        client.create_round(&1_0000000u128, &None);
    }

    let final_price = match shape {
        Shape::OneSidedUp => {
            client.place_bet(&alice, &ALICE, &BetSide::Up);
            client.place_bet(&bob, &BOB, &BetSide::Up);
            2_0000000u128
        }
        Shape::OneSidedDown => {
            client.place_bet(&alice, &ALICE, &BetSide::Down);
            client.place_bet(&bob, &BOB, &BetSide::Down);
            5000000u128
        }
        Shape::TwoSided => {
            client.place_bet(&alice, &ALICE, &BetSide::Up);
            client.place_bet(&bob, &BOB, &BetSide::Up);
            client.place_bet(&charlie, &CHARLIE, &BetSide::Down);
            2_0000000u128
        }
        Shape::PrecisionUnique => {
            client.place_precision_prediction(&alice, &ALICE, &1_1000000u128);
            client.place_precision_prediction(&bob, &BOB, &1_4000000u128);
            client.place_precision_prediction(&charlie, &CHARLIE, &1_8000000u128);
            1_1000000u128
        }
        Shape::PrecisionTieTwo => {
            client.place_precision_prediction(&alice, &ALICE, &1_1000000u128);
            client.place_precision_prediction(&bob, &BOB, &1_1000000u128);
            client.place_precision_prediction(&charlie, &CHARLIE, &1_8000000u128);
            1_1000000u128
        }
        Shape::PrecisionTieThree => {
            client.place_precision_prediction(&alice, &ALICE, &1_1000000u128);
            client.place_precision_prediction(&bob, &BOB, &1_1000000u128);
            client.place_precision_prediction(&charlie, &CHARLIE, &1_1000000u128);
            1_1000000u128
        }
    };

    env.ledger().with_mut(|li| li.sequence_number = 12);
    let round = client.get_active_round().expect("round still active");
    client.resolve_round(&OraclePayload {
        price: final_price,
        timestamp: env.ledger().timestamp(),
        round_id: round.start_ledger,
        nonce: 1,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    });

    let alice_pending = client.get_pending_winnings(&alice);
    let bob_pending = client.get_pending_winnings(&bob);
    let charlie_pending = client.get_pending_winnings(&charlie);
    let treasury = client.get_protocol_fee_treasury();
    let balances = client.balance(&alice) + client.balance(&bob) + client.balance(&charlie);
    let pending = alice_pending + bob_pending + charlie_pending;
    let accounted = balances + pending + treasury;
    let minted = MINT * 3;
    let fee = expected_fee(model, bps, shape);

    assert!(
        client.get_active_round().is_none(),
        "{label}: round was not cleared"
    );
    assert!(
        alice_pending >= 0 && bob_pending >= 0 && charlie_pending >= 0 && treasury >= 0,
        "{label}: negative pending or treasury alice={alice_pending} bob={bob_pending} charlie={charlie_pending} treasury={treasury}"
    );
    assert_eq!(
        treasury, fee,
        "{label}: treasury {treasury} != expected fee {fee} (pot={} winner_stakes={})",
        shape.pot(),
        shape.winner_stakes()
    );
    assert_eq!(
        pending + treasury,
        shape.pot(),
        "{label}: pending {pending} + treasury {treasury} != pot {}",
        shape.pot()
    );
    assert_eq!(
        accounted, minted,
        "{label}: conservation break accounted={accounted} minted={minted} pending={pending} treasury={treasury} balances={balances}"
    );

    if shape.one_sided() {
        assert_eq!(alice_pending, ALICE, "{label}: one-sided alice refund");
        assert_eq!(bob_pending, BOB, "{label}: one-sided bob refund");
        assert_eq!(charlie_pending, 0, "{label}: one-sided charlie was not in the round");
    }
    if matches!(shape, Shape::PrecisionTieTwo | Shape::PrecisionTieThree) {
        assert!(alice_pending > 0, "{label}: tied alice received nothing");
        assert!(bob_pending > 0, "{label}: tied bob received nothing");
    }
    if matches!(shape, Shape::PrecisionTieThree) {
        assert!(charlie_pending > 0, "{label}: tied charlie received nothing");
    }
}

#[test]
fn fee_model_onesided_precision_tie_all_combinations() {
    let models = [FeeModel::FeeOnPot, FeeModel::FeeOnWinnings];
    let rates = [None, Some(BPS)];
    for model in models {
        for bps in rates {
            for shape in Shape::all() {
                settle_combination(model, bps, shape);
            }
        }
    }
}
