// SPDX-License-Identifier: MIT
//! Adversarial load + economic attack simulation suite (Issue #372).
//!
//! Red-team scenarios scripting economic attacks against the prediction market
//! protocol. Each scenario models a multi-step attacker sequence, asserts the
//! expected defense holds (or documents accepted residual risk), and emits a
//! structured `ADVERSARIAL_RESULT:` line for the report harness.
//!
//! ## Scenario catalog (13 scenarios)
//!
//! | Scenario | Module | Defense | CI-critical |
//! |----------|--------|---------|-------------|
//! | Sybil faucet (mint limit) | `sybil` | `MintLimitExceeded` | yes |
//! | Sybil faucet (epoch budget) | `sybil` | `EpochBudgetExceeded` | |
//! | Last-ledger sniping (UpDown) | `sniping` | `RoundEnded` + close buffer | yes |
//! | Last-ledger sniping (Precision) | `sniping` | `RoundEnded` + close buffer | |
//! | Precision spam commits | `precision` | `PrecisionCapExceeded` | yes |
//! | Oracle heartbeat griefing | `oracle` | `OracleNotLive` | |
//! | Oracle nonce replay | `oracle` | `OracleNonceReused` | |
//! | Cross-round payload replay | `oracle` | `InvalidOracleRound` | |
//! | Stale oracle timestamp griefing | `oracle` | `StaleOracleData` | |
//! | Fee gaming (mid-round schedule) | `economic` | Timelock (pending only) | |
//! | Exposure cap boundary | `economic` | `ExposureCapExceeded` | |
//! | Double-claim attack | `lifecycle` | Idempotent zero payout | |
//! | Mode confusion | `lifecycle` | `WrongModeForPrediction` | |
//!
//! Run locally: `./scripts/run_adversarial_suite.sh`
//! Deterministic seed: [`ADVERSARIAL_SEED`]

mod economic;
mod grinding;
mod lifecycle;
mod oracle;
mod precision;
mod report;
mod sniping;
mod sybil;

/// Deterministic seed referenced in all scenario reports (Issue #372).
pub const ADVERSARIAL_SEED: u64 = 372_2026;

use crate::contract::{VirtualTokenContract, VirtualTokenContractClient};
use crate::types::OraclePayload;
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Standard contract setup for adversarial scenarios.
pub(crate) fn setup_contract(
    env: &Env,
) -> (VirtualTokenContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(VirtualTokenContract, ());
    let client = VirtualTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let oracle = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &oracle);
    client.update_oracle_heartbeat(&0u32);
    (client, contract_id, admin, oracle)
}

/// Build a valid oracle payload including the required `attestation` field.
pub(crate) fn oracle_payload(
    env: &Env,
    contract_id: &Address,
    price: u128,
    round_id: u32,
    nonce: u64,
) -> OraclePayload {
    OraclePayload {
        price,
        timestamp: env.ledger().timestamp(),
        round_id,
        nonce,
        network_id: env.ledger().network_id(),
        contract_addr: contract_id.clone(),
        confidence: None,
        attestation: None,
    }
}

/// Emit a structured JSON line consumed by `scripts/adversarial_report.py`.
pub(crate) fn emit_result(
    scenario: &str,
    status: &str,
    defense: &str,
    residual_risk: &str,
    severity: &str,
    ci_critical: bool,
) {
    std::eprintln!(
        "ADVERSARIAL_RESULT:{{\"scenario\":\"{scenario}\",\"status\":\"{status}\",\"defense\":\"{defense}\",\"residual_risk\":\"{residual_risk}\",\"severity\":\"{severity}\",\"seed\":{seed},\"ci_critical\":{ci_critical}}}",
        seed = ADVERSARIAL_SEED,
        ci_critical = ci_critical,
    );
}
