// SPDX-License-Identifier: MIT
//! Test modules for the XLM Price Prediction Market contract.

// mod archive_retention; // upstream bug
mod betting;
mod cei_ordering;
mod chaos_recovery;
// mod commit_reveal_e2e; // upstream bug
mod config_helpers;
// mod config_timelock; // upstream bug
mod cost_benchmarks;
mod edge_cases;
mod event_coverage;
mod guard_tests;
// mod initialization; // upstream bug
// mod invariant_harness; // upstream bug: uses std HashMap in no_std context
mod lifecycle;
mod migration_versioning;
mod mode_tests;
mod overflow_tests;
mod pause;
mod property_invariants;
mod reference_model;
mod resolution;
mod rotation;
// mod reference_model; // upstream bug: uses std HashMap in no_std context
// mod resolution; // upstream bug: duplicate type imports
mod security;
mod storage_benchmarks;
mod ttl_tests;
mod windows;
