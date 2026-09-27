// SPDX-License-Identifier: MIT
//! Blue/green contract migration: state export and import (Issue #518).
//!
//! ## Overview
//!
//! The blue/green migration lets an operator move the canonical economic state
//! (user balances, pending winnings, required config) from a **source** contract
//! (vN) to a **destination** contract (vN+1) in a safe, verifiable, and
//! reversible way.
//!
//! ### Protocol
//!
//! 1. **Source contract** — admin calls `export_state(users, pending_users)`.
//!    - Must be called while the contract is paused and has no active round.
//!    - Produces a [`MigrationManifest`] persisted at [`MigrationKey::ExportManifest`].
//!    - The manifest contains sorted balance entries, sorted pending-claim entries,
//!      a config snapshot, and a SHA-256 checksum of the sorted entry lists.
//!    - Returns the manifest so it can be logged off-chain.
//!
//! 2. **Off-chain** — operator verifies the checksum, serialises the manifest
//!    as JSON / XDR, and prepares the import transaction for the destination.
//!
//! 3. **Destination contract** — admin calls `import_state(manifest, dry_run)`.
//!    - `dry_run = true` validates the manifest (checksum, schema, no active round)
//!      without writing anything. Safe to call repeatedly.
//!    - `dry_run = false` writes all balances, pending winnings, and config,
//!      then writes [`MigrationKey::ImportComplete`] to prevent double-import.
//!    - Emits `("migration", "imported")` with the export sequence number.
//!
//! ### Invariants
//!
//! * No active round is allowed on either source or destination during the
//!   respective call (`MigrationActiveRound`).
//! * The source contract must be paused before exporting.
//! * The destination contract must be paused before importing.
//! * Import is idempotent under `dry_run` but exactly-once when committed
//!   (`MigrationAlreadyImported` on repeat).
//! * The checksum is recomputed inside `import_state` and compared to the
//!   manifest's claimed checksum (`HashMismatch`).

#![allow(dead_code)]

extern crate alloc;
use alloc::vec::Vec as StdVec;

use crate::common::{
    _extend_persistent_ttl, DEFAULT_ARCHIVE_RETENTION, DEFAULT_BET_WINDOW_LEDGERS,
    DEFAULT_CLOSE_BUFFER_LEDGERS, DEFAULT_DISPUTE_LEDGERS, DEFAULT_ORACLE_STALE_THRESHOLD,
    DEFAULT_RUN_WINDOW_LEDGERS,
};
use crate::errors::ContractError;
use crate::types::{
    BalanceEntry, DataKeyCore, DataKeyScoped, MigrationConfig, MigrationKey, MigrationManifest,
    PendingClaimEntry,
};
use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, Vec};

// ─── Hard limits ─────────────────────────────────────────────────────────────

/// Maximum number of balance entries allowed in a single export.
/// Bounding this keeps the export transaction's compute cost predictable.
pub const MAX_EXPORT_BALANCE_ENTRIES: u32 = 5_000;

/// Maximum number of pending-claim entries allowed in a single export.
pub const MAX_EXPORT_PENDING_ENTRIES: u32 = 5_000;

// ─── Checksum ────────────────────────────────────────────────────────────────

/// Compute the canonical SHA-256 checksum of the sorted entry lists.
///
/// Encoding:
///   For each `BalanceEntry`      → `strkey_bytes(user) || i128::to_be_bytes(balance)`
///   For each `PendingClaimEntry` → `strkey_bytes(user) || i128::to_be_bytes(amount)`
///
/// The two lists are concatenated in order (balances first, then claims) and
/// SHA-256'd.  Both lists **must be sorted** by `user` (ascending) before this
/// function is called.
pub fn compute_checksum(
    env: &Env,
    balances: &Vec<BalanceEntry>,
    claims: &Vec<PendingClaimEntry>,
) -> BytesN<32> {
    let mut raw: StdVec<u8> = StdVec::new();

    for entry in balances.iter() {
        let addr_bytes = entry.user.to_string();
        // Address::to_string returns a Symbol-like value; convert via Bytes
        let addr_b: Bytes = addr_bytes.into_val(env);
        for b in addr_b.iter() {
            raw.push(b);
        }
        let val_bytes = entry.balance.to_be_bytes();
        for b in val_bytes {
            raw.push(b);
        }
    }

    for entry in claims.iter() {
        let addr_bytes = entry.user.to_string();
        let addr_b: Bytes = addr_bytes.into_val(env);
        for b in addr_b.iter() {
            raw.push(b);
        }
        let val_bytes = entry.amount.to_be_bytes();
        for b in val_bytes {
            raw.push(b);
        }
    }

    let buf = Bytes::from_slice(env, &raw);
    env.crypto().sha256(&buf)
}

// ─── Config snapshot helpers ──────────────────────────────────────────────────

/// Read the current contract's configuration into a [`MigrationConfig`].
fn _snapshot_config(env: &Env) -> MigrationConfig {
    let schema_version: u32 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::SchemaVersion)
        .unwrap_or(1);

    let bet_window_ledgers: u32 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::BetWindowLedgers)
        .unwrap_or(DEFAULT_BET_WINDOW_LEDGERS);

    let run_window_ledgers: u32 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::RunWindowLedgers)
        .unwrap_or(DEFAULT_RUN_WINDOW_LEDGERS);

    let close_buffer_ledgers: u32 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::CloseBufferLedgers)
        .unwrap_or(DEFAULT_CLOSE_BUFFER_LEDGERS);

    let protocol_fee_bps: Option<u32> =
        env.storage().persistent().get(&DataKeyCore::ProtocolFeeBps);

    let max_stake: Option<i128> = env.storage().persistent().get(&DataKeyCore::MaxStake);

    let max_user_round_exposure: Option<i128> = env
        .storage()
        .persistent()
        .get(&DataKeyCore::MaxUserRoundExposure);

    let max_pending_winnings: Option<i128> = env
        .storage()
        .persistent()
        .get(&DataKeyCore::MaxPendingWinnings);

    let min_participants: Option<u32> =
        env.storage().persistent().get(&DataKeyCore::MinParticipants);

    let min_bet: Option<i128> = env.storage().persistent().get(&DataKeyCore::MinBet);

    let oracle_stale_threshold: u64 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::OracleStaleThreshold)
        .unwrap_or(DEFAULT_ORACLE_STALE_THRESHOLD);

    let archive_retention: u32 = env
        .storage()
        .persistent()
        .get(&DataKeyCore::ArchiveRetention)
        .unwrap_or(DEFAULT_ARCHIVE_RETENTION);

    // DisputeLedgers uses a Symbol key; read via the helper in config module.
    // We duplicate the read here to avoid a circular dep with config::get_dispute_ledgers.
    let dispute_key = soroban_sdk::Symbol::new(env, "DisputeLedgers");
    let dispute_ledgers: u32 = env
        .storage()
        .persistent()
        .get(&dispute_key)
        .unwrap_or(DEFAULT_DISPUTE_LEDGERS);

    MigrationConfig {
        schema_version,
        bet_window_ledgers,
        run_window_ledgers,
        close_buffer_ledgers,
        protocol_fee_bps,
        max_stake,
        max_user_round_exposure,
        max_pending_winnings,
        min_participants,
        min_bet,
        oracle_stale_threshold,
        archive_retention,
        dispute_ledgers,
    }
}

// ─── Sort helpers ─────────────────────────────────────────────────────────────

/// Sort `Vec<BalanceEntry>` lexicographically by user address (ascending).
fn _sort_balances(env: &Env, entries: Vec<BalanceEntry>) -> Vec<BalanceEntry> {
    if entries.len() <= 1 {
        return entries;
    }
    let mut v: StdVec<BalanceEntry> = StdVec::with_capacity(entries.len() as usize);
    for e in entries.iter() {
        v.push(e);
    }
    // Sort by the string representation of the address.
    v.sort_unstable_by(|a, b| {
        let sa = a.user.to_string();
        let sb = b.user.to_string();
        let ba: Bytes = sa.into_val(env);
        let bb: Bytes = sb.into_val(env);
        // Compare byte-by-byte
        let la = ba.len();
        let lb = bb.len();
        let min_len = if la < lb { la } else { lb };
        for i in 0..min_len {
            let ca = ba.get(i).unwrap_or(0);
            let cb = bb.get(i).unwrap_or(0);
            if ca != cb {
                return ca.cmp(&cb);
            }
        }
        la.cmp(&lb)
    });
    let mut sorted = Vec::new(env);
    for e in v {
        sorted.push_back(e);
    }
    sorted
}

/// Sort `Vec<PendingClaimEntry>` lexicographically by user address (ascending).
fn _sort_claims(env: &Env, entries: Vec<PendingClaimEntry>) -> Vec<PendingClaimEntry> {
    if entries.len() <= 1 {
        return entries;
    }
    let mut v: StdVec<PendingClaimEntry> = StdVec::with_capacity(entries.len() as usize);
    for e in entries.iter() {
        v.push(e);
    }
    v.sort_unstable_by(|a, b| {
        let sa = a.user.to_string();
        let sb = b.user.to_string();
        let ba: Bytes = sa.into_val(env);
        let bb: Bytes = sb.into_val(env);
        let la = ba.len();
        let lb = bb.len();
        let min_len = if la < lb { la } else { lb };
        for i in 0..min_len {
            let ca = ba.get(i).unwrap_or(0);
            let cb = bb.get(i).unwrap_or(0);
            if ca != cb {
                return ca.cmp(&cb);
            }
        }
        la.cmp(&lb)
    });
    let mut sorted = Vec::new(env);
    for e in v {
        sorted.push_back(e);
    }
    sorted
}

// ─── Export ────────────────────────────────────────────────────────────────────

/// Export canonical state from the source (vN) contract (admin only).
///
/// # Arguments
///
/// * `users`          – addresses whose `Balance` entries should be exported.
///   Entries with a zero balance are skipped automatically.
/// * `pending_users`  – addresses whose `PendingWinnings` entries should be
///   exported.  Zero entries are skipped.
///
/// # Guards
///
/// * Admin authentication required.
/// * Contract must be **paused** (use `pause_contract` first).
/// * No active round (`MigrationActiveRound`).
/// * Total entries must not exceed [`MAX_EXPORT_BALANCE_ENTRIES`] /
///   [`MAX_EXPORT_PENDING_ENTRIES`] (`MigrationExportTooLarge`).
///
/// # Returns
///
/// The constructed [`MigrationManifest`] persisted to
/// [`MigrationKey::ExportManifest`].
pub fn export_state(
    env: Env,
    users: Vec<Address>,
    pending_users: Vec<Address>,
) -> Result<MigrationManifest, ContractError> {
    // ── Auth ────────────────────────────────────────────────────────────────
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();

    // ── Pre-conditions ──────────────────────────────────────────────────────
    // Must be paused — we check the stored RuntimeMode value.
    _ensure_paused_for_migration(&env)?;

    // No active round.
    if env.storage().persistent().has(&DataKeyCore::ActiveRound) {
        return Err(ContractError::MigrationActiveRound);
    }

    // Size guards.
    if users.len() > MAX_EXPORT_BALANCE_ENTRIES {
        return Err(ContractError::MigrationExportTooLarge);
    }
    if pending_users.len() > MAX_EXPORT_PENDING_ENTRIES {
        return Err(ContractError::MigrationExportTooLarge);
    }

    // ── Collect balances ────────────────────────────────────────────────────
    let mut balance_entries: Vec<BalanceEntry> = Vec::new(&env);
    for user in users.iter() {
        let key = DataKeyScoped::Balance(user.clone());
        let bal: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if bal != 0 {
            balance_entries.push_back(BalanceEntry {
                user,
                balance: bal,
            });
        }
    }

    // ── Collect pending claims ──────────────────────────────────────────────
    let mut claim_entries: Vec<PendingClaimEntry> = Vec::new(&env);
    for user in pending_users.iter() {
        let key = DataKeyScoped::PendingWinnings(user.clone());
        let amt: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if amt != 0 {
            claim_entries.push_back(PendingClaimEntry {
                user,
                amount: amt,
            });
        }
    }

    // ── Sort (determinism) ──────────────────────────────────────────────────
    let sorted_balances = _sort_balances(&env, balance_entries);
    let sorted_claims = _sort_claims(&env, claim_entries);

    // ── Checksum ────────────────────────────────────────────────────────────
    let checksum = compute_checksum(&env, &sorted_balances, &sorted_claims);

    // ── Config snapshot ─────────────────────────────────────────────────────
    let config = _snapshot_config(&env);

    // ── Export sequence number ──────────────────────────────────────────────
    let seq_key = MigrationKey::ExportManifest;
    let prev_seq: u32 = if env.storage().persistent().has(&seq_key) {
        let prev: MigrationManifest = env.storage().persistent().get(&seq_key).unwrap();
        prev.export_seq
    } else {
        0
    };
    let export_seq = prev_seq.checked_add(1).ok_or(ContractError::Overflow)?;

    let manifest = MigrationManifest {
        export_seq,
        captured_at_ledger: env.ledger().sequence(),
        captured_at_timestamp: env.ledger().timestamp(),
        source_schema_version: config.schema_version,
        balances: sorted_balances,
        pending_claims: sorted_claims,
        config,
        checksum,
    };

    // ── Persist ─────────────────────────────────────────────────────────────
    env.storage().persistent().set(&seq_key, &manifest);
    _extend_persistent_ttl(&env, &seq_key);

    // ── Event ───────────────────────────────────────────────────────────────
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("migr"), symbol_short!("export")),
        (export_seq, manifest.captured_at_ledger, checksum),
    );

    Ok(manifest)
}

/// Return the most recently produced export manifest, if any.
pub fn get_export_manifest(env: Env) -> Option<MigrationManifest> {
    let key = MigrationKey::ExportManifest;
    if env.storage().persistent().has(&key) {
        _extend_persistent_ttl(&env, &key);
        env.storage().persistent().get(&key)
    } else {
        None
    }
}

// ─── Import ────────────────────────────────────────────────────────────────────

/// Import state into the destination (vN+1) contract (admin only).
///
/// # Arguments
///
/// * `manifest`  – the [`MigrationManifest`] produced by `export_state` on the
///   source contract.
/// * `dry_run`   – when `true` all validation is performed but no storage
///   writes are issued.  Safe to call repeatedly before committing.
///
/// # Guards
///
/// * Admin authentication required.
/// * Contract must be **paused**.
/// * No active round.
/// * Import has not already been committed (`MigrationAlreadyImported`).
/// * Checksum over `manifest.balances ++ manifest.pending_claims` must match
///   `manifest.checksum` (`HashMismatch`).
///
/// # Behaviour on commit (`dry_run = false`)
///
/// 1. Writes every `BalanceEntry` → `DataKeyScoped::Balance(user)`.
/// 2. Writes every `PendingClaimEntry` → `DataKeyScoped::PendingWinnings(user)`.
/// 3. Writes selected config keys from `manifest.config` (does **not**
///    overwrite Admin / Oracle / SchemaVersion on the destination).
/// 4. Writes [`MigrationKey::ImportComplete`] to seal the import.
/// 5. Emits `("migration", "imported")`.
pub fn import_state(
    env: Env,
    manifest: MigrationManifest,
    dry_run: bool,
) -> Result<(), ContractError> {
    // ── Auth ────────────────────────────────────────────────────────────────
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Admin)
        .ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();

    // ── Pre-conditions ──────────────────────────────────────────────────────
    _ensure_paused_for_migration(&env)?;

    if env.storage().persistent().has(&DataKeyCore::ActiveRound) {
        return Err(ContractError::MigrationActiveRound);
    }

    // Guard double-import (not checked in dry-run so operators can validate freely).
    if !dry_run && env.storage().persistent().has(&MigrationKey::ImportComplete) {
        return Err(ContractError::MigrationAlreadyImported);
    }

    // ── Checksum verification ───────────────────────────────────────────────
    let recomputed = compute_checksum(&env, &manifest.balances, &manifest.pending_claims);
    if recomputed != manifest.checksum {
        return Err(ContractError::HashMismatch);
    }

    // ── Dry-run exits here ───────────────────────────────────────────────────
    if dry_run {
        return Ok(());
    }

    // ── Write balances ───────────────────────────────────────────────────────
    for entry in manifest.balances.iter() {
        let key = DataKeyScoped::Balance(entry.user.clone());
        env.storage().persistent().set(&key, &entry.balance);
        _extend_persistent_ttl(&env, &key);
    }

    // ── Write pending winnings ───────────────────────────────────────────────
    for entry in manifest.pending_claims.iter() {
        let key = DataKeyScoped::PendingWinnings(entry.user.clone());
        env.storage().persistent().set(&key, &entry.amount);
        _extend_persistent_ttl(&env, &key);
    }

    // ── Write config (selective — never touch Admin / Oracle / SchemaVersion) ─
    _apply_config(&env, &manifest.config);

    // ── Seal import ──────────────────────────────────────────────────────────
    let seal_key = MigrationKey::ImportComplete;
    env.storage().persistent().set(&seal_key, &manifest.export_seq);
    _extend_persistent_ttl(&env, &seal_key);

    // ── Event ────────────────────────────────────────────────────────────────
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("migr"), symbol_short!("import")),
        (
            manifest.export_seq,
            manifest.captured_at_ledger,
            manifest.checksum,
        ),
    );

    Ok(())
}

/// Returns `true` if an import has been committed on this contract.
pub fn is_import_complete(env: Env) -> bool {
    env.storage()
        .persistent()
        .has(&MigrationKey::ImportComplete)
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Apply the config snapshot to the destination contract's storage.
///
/// Deliberately does **not** touch: Admin, Oracle, SchemaVersion,
/// OracleRotationProposal, or any governance / leaderboard keys.
fn _apply_config(env: &Env, cfg: &MigrationConfig) {
    macro_rules! set_opt {
        ($key:expr, $val:expr) => {
            if let Some(v) = $val {
                env.storage().persistent().set(&$key, v);
                _extend_persistent_ttl(env, &$key);
            }
        };
    }
    macro_rules! set_val {
        ($key:expr, $val:expr) => {{
            env.storage().persistent().set(&$key, $val);
            _extend_persistent_ttl(env, &$key);
        }};
    }

    set_val!(DataKeyCore::BetWindowLedgers, &cfg.bet_window_ledgers);
    set_val!(DataKeyCore::RunWindowLedgers, &cfg.run_window_ledgers);
    set_val!(DataKeyCore::CloseBufferLedgers, &cfg.close_buffer_ledgers);
    set_val!(DataKeyCore::OracleStaleThreshold, &cfg.oracle_stale_threshold);
    set_val!(DataKeyCore::ArchiveRetention, &cfg.archive_retention);
    set_opt!(DataKeyCore::ProtocolFeeBps, &cfg.protocol_fee_bps);
    set_opt!(DataKeyCore::MaxStake, &cfg.max_stake);
    set_opt!(DataKeyCore::MaxUserRoundExposure, &cfg.max_user_round_exposure);
    set_opt!(DataKeyCore::MaxPendingWinnings, &cfg.max_pending_winnings);
    set_opt!(DataKeyCore::MinParticipants, &cfg.min_participants);
    set_opt!(DataKeyCore::MinBet, &cfg.min_bet);

    // DisputeLedgers uses a Symbol key (same as config.rs does).
    if cfg.dispute_ledgers > 0 {
        let dispute_key = soroban_sdk::Symbol::new(env, "DisputeLedgers");
        env.storage()
            .persistent()
            .set(&dispute_key, &cfg.dispute_ledgers);
        crate::common::_extend_ttl_symbol(env, &dispute_key);
    }
}

/// Ensure the contract is in `FullyPaused` mode; rejects with `ContractPaused`
/// inverted — actually returns `InvalidMode` if not paused so the caller knows
/// what is wrong.
///
/// We re-read `RuntimeMode` from the `Paused` storage key (same slot that
/// `admin::pause_contract` writes) rather than importing from `admin` to
/// avoid circular dependencies.
fn _ensure_paused_for_migration(env: &Env) -> Result<(), ContractError> {
    use crate::types::RuntimeMode;
    let mode: RuntimeMode = env
        .storage()
        .persistent()
        .get(&DataKeyCore::Paused)
        .unwrap_or(RuntimeMode::Normal);
    if mode != RuntimeMode::FullyPaused {
        return Err(ContractError::InvalidMode);
    }
    Ok(())
}
