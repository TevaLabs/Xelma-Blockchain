// SPDX-License-Identifier: MIT
//! Test modules for the XLM Price Prediction Market contract.

mod archive_retention;
mod betting;
mod cei_ordering;
mod chaos_recovery;
mod commit_reveal_e2e;
mod config_helpers;
// mod config_timelock; // upstream bug
mod conservation;
mod cost_benchmarks;
mod edge_cases;
mod event_coverage;
mod guard_tests;
// mod initialization; // upstream bug
// mod invariant_harness; // upstream bug: uses std HashMap in no_std context
mod leaderboard;
mod leaderboard_seasons;
mod lifecycle;
mod migration_versioning;
mod mode_tests;
mod overflow_tests;
mod pause;
mod property_invariants;
mod reference_model;
mod resolution;
mod rotation;
mod security;
mod status;
mod storage_benchmarks;
mod ttl_tests;
mod windows;
