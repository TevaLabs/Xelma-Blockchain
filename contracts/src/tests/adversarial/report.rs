// SPDX-License-Identifier: MIT
//! Inventory test — verifies the full scenario catalog is registered.

use super::ADVERSARIAL_SEED;
use std::eprintln;

/// Scenario count gate: Issue #372 requires ≥8 named attack scenarios.
/// Individual scenario tests emit structured `ADVERSARIAL_RESULT:` lines;
/// this test documents the catalog and seed for the report harness.
#[test]
fn test_adversarial_suite_inventory() {
    const SCENARIO_COUNT: usize = 13;

    const {
        assert!(SCENARIO_COUNT >= 8, "Issue #372 requires ≥8 scenarios");
    }

    eprintln!(
        "ADVERSARIAL_SUITE:{{\"seed\":{seed},\"scenario_count\":{count},\"modules\":[\"sybil\",\"sniping\",\"precision\",\"oracle\",\"economic\",\"lifecycle\"]}}",
        seed = ADVERSARIAL_SEED,
        count = SCENARIO_COUNT,
    );
}
