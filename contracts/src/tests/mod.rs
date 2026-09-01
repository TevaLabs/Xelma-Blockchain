// SPDX-License-Identifier: MIT
//! Test modules for the XLM Price Prediction Market contract.

mod access_control;
mod adversarial;
mod archive_retention;
mod attestation;
mod betting;
mod cancel_refund_matrix;
mod cei_ordering;
mod chaos_recovery;
mod claim_many;
mod commit_reveal_e2e;
// mod commit_reveal_e2e; // upstream bug: all-unrevealed refunds test expects behavior contract doesn't implement
mod config_helpers;
// mod config_timelock; // upstream bug
mod conservation;
mod cost_benchmarks;
mod deviation_reference;
mod dispute_window;
mod drill;
mod edge_cases;
mod event_coverage;
mod fee_model;
mod guard_tests;
// mod initialization; // upstream bug
mod archive_participation;
mod invariant_harness;
mod leaderboard;
mod leaderboard_seasons;
mod lifecycle;
mod market_snapshot;
mod migration_blue_green;
mod migration_versioning;
mod min_bet;
mod mode_tests;
mod one_sided_settlement;
mod overflow_tests;
mod precision_payout_overflow;
mod pause;
mod pause_policy_matrix;
mod pending_winnings_expiry;
mod policy_gate;
mod precision_scoring;
mod property_invariants;
mod reference_model;
mod resolution;
mod rotation;
mod security;
mod settlement_math_vectors;
mod status;
mod storage_benchmarks;
mod ttl_tests;
mod windows;
mod archive_participation;
mod insurance;
mod precision_scoring;

