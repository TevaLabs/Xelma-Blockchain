// SPDX-License-Identifier: MIT
//! Whole-ledger "no storage mutation" assertion for negative tests.
//!
//! Checking a handful of getters after a rejected call only proves that
//! *those* keys are untouched. [`assert_ledger_unchanged`] instead compares
//! every ledger entry (instance, persistent and temporary storage, including
//! TTLs) before and after the call, so a guard that wrote to any key before
//! returning its error would be caught.
//!
//! Take the "before" snapshot immediately before the rejected call: contract
//! getters can legitimately bump TTLs, which would show up as a difference.

use soroban_sdk::Env;

/// Runs `f` (expected to be a rejected `try_*` call) and asserts that no
/// ledger entry changed. Returns `f`'s result so the caller can check the
/// error code.
pub fn assert_ledger_unchanged<R>(env: &Env, label: &str, f: impl FnOnce() -> R) -> R {
    let before = env.to_ledger_snapshot().ledger_entries;
    let result = f();
    let after = env.to_ledger_snapshot().ledger_entries;

    if before != after {
        let changed = before.iter().filter(|entry| !after.contains(entry)).count()
            + after.iter().filter(|entry| !before.contains(entry)).count();
        panic!(
            "{label}: rejected call mutated ledger storage \
             ({} entries before, {} after, {changed} differing)",
            before.len(),
            after.len()
        );
    }
    result
}
