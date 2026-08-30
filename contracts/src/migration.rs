// SPDX-License-Identifier: MIT
//! Blue/green contract migration with state export/import proofs (Issue #366).
//!
//! This module implements the secure upgrade path:
//!
//! 1. **Export** — the source contract (vN) canonicalizes its live economic
//!    state (user balances, pending claims, and the economic configuration
//!    subset) into an ordered, deterministic set of leaves. An admin operator
//!    feeds users in batches; the contract re-reads the authoritative amounts
//!    from storage so no value can be fabricated. The export is then finalized
//!    into a single [`MigrationCommitment`] (a Merkle root).
//! 2. **Drain mode** — finalizing freezes the source contract into a
//!    claims-only drain state: new rounds, betting, minting and settlement
//!    (which would create state outside the snapshot) are blocked, while
//!    claims and read-only queries remain available.
//! 3. **Import** — the destination contract (vN+1) is initialized for the
//!    expected migration, then each canonical record is imported individually
//!    along with a Merkle proof verified against the source commitment.
//!    Replay protection prevents double-importing any record, and the session
//!    can be finalized only after the full expected subset is applied.
//!
//! # Canonical encoding (shared with `scripts/generate_migration_witness.py`)
//!
//! Every leaf preimage is:
//!
//! ```text
//! preimage = DOMAIN_MARKER ++ source_version_u32le ++ record
//! ```
//!
//! with `DOMAIN_MARKER = "XELMA-CPAY-V1"` (13 ASCII bytes) providing domain
//! separation, and `record` one of:
//!
//! * Config leaf (`tag = 0x00`): `0x00 ++ config_bytes`
//! * Balance leaf (`tag = 0x01`): `0x01 ++ addr ++ amount`
//! * Pending leaf (`tag = 0x02`): `0x02 ++ addr ++ amount`
//!
//! with `addr = u32le(addr_string.len()) ++ addr_string_bytes` (the StrKey
//! ASCII) and `amount = i128le` (16 bytes, little-endian, two's complement).
//! Optional fields are a presence flag byte (`0x00`/`0x01`) plus the value.
//!
//! Leaf order is deterministic and independent of storage iteration: config
//! leaf first, then balance leaves sorted by `addr`, then pending leaves
//! sorted by `addr`. A standard binary Merkle tree over these leaves is the
//! commitment. See `docs/UPGRADE_BLUE_GREEN.md` for the normative spec.

use crate::admin::{_ensure_not_paused, _require_supported_schema, _set_mode, get_admin};
use crate::common::{_extend_persistent_ttl, CURRENT_SCHEMA_VERSION};
use crate::config::{
    _read_fee_model, _read_precision_payout_policy, _read_protocol_fee_bps, get_bet_window_ledgers,
    get_close_buffer_ledgers, get_dispute_ledgers, get_early_cashout_bps, get_max_pending_winnings,
    get_max_precision_participants, get_max_stake, get_max_user_exposure, get_min_bet,
    get_min_participants, get_protocol_fee_treasury, get_run_window_ledgers,
};
use crate::errors::ContractError;
use crate::types::{
    DataKeyCore, DataKeyScoped, MerkleProof, MigrationBalance, MigrationCommitment,
    MigrationConfig, MigrationKey, MigrationPending, RuntimeMode,
};
use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, Vec};

/// The destination schema version this migration tooling imports into.
pub const MIGRATION_DESTINATION_VERSION: u32 = 4;

pub fn _null_leaf(env: &Env) -> BytesN<32> {
    let mut pre = Bytes::new(env);
    pre.append(&Bytes::from_slice(env, crate::types::MIGRATION_DOMAIN));
    pre.append(&Bytes::from_slice(env, &[0xFFu8]));
    env.crypto().sha256(&pre).into()
}

fn _append_u32le(env: &Env, out: &mut Bytes, v: u32) {
    out.append(&Bytes::from_slice(env, &v.to_le_bytes()));
}

fn _append_i128le(env: &Env, out: &mut Bytes, v: i128) {
    out.append(&Bytes::from_slice(env, &v.to_le_bytes()));
}

fn _append_opt_flag(env: &Env, out: &mut Bytes, is_some: bool) {
    out.append(&Bytes::from_slice(env, &[if is_some { 1u8 } else { 0u8 }]));
}

fn _append_addr(env: &Env, out: &mut Bytes, addr: &Address) {
    let b = addr.to_string().to_bytes();
    _append_u32le(env, out, b.len());
    out.append(&b);
}

/// Deterministic byte encoding of the canonical [`MigrationConfig`]. Field
/// order is fixed (struct order) so identical logical config → identical bytes.
pub fn _config_bytes(env: &Env, cfg: &MigrationConfig) -> Bytes {
    let mut out = Bytes::new(env);
    match cfg.protocol_fee_bps {
        Some(bps) => {
            _append_opt_flag(env, &mut out, true);
            _append_u32le(env, &mut out, bps);
        }
        None => _append_opt_flag(env, &mut out, false),
    }
    _append_u32le(env, &mut out, cfg.fee_model as u32);
    _append_i128le(env, &mut out, cfg.protocol_fee_treasury);
    _append_u32le(env, &mut out, cfg.bet_window_ledgers);
    _append_u32le(env, &mut out, cfg.run_window_ledgers);
    _append_u32le(env, &mut out, cfg.close_buffer_ledgers);
    match cfg.max_stake {
        Some(v) => {
            _append_opt_flag(env, &mut out, true);
            _append_i128le(env, &mut out, v);
        }
        None => _append_opt_flag(env, &mut out, false),
    }
    match cfg.max_user_round_exposure {
        Some(v) => {
            _append_opt_flag(env, &mut out, true);
            _append_i128le(env, &mut out, v);
        }
        None => _append_opt_flag(env, &mut out, false),
    }
    match cfg.max_pending_winnings {
        Some(v) => {
            _append_opt_flag(env, &mut out, true);
            _append_i128le(env, &mut out, v);
        }
        None => _append_opt_flag(env, &mut out, false),
    }
    match cfg.min_bet {
        Some(v) => {
            _append_opt_flag(env, &mut out, true);
            _append_i128le(env, &mut out, v);
        }
        None => _append_opt_flag(env, &mut out, false),
    }
    match cfg.min_participants {
        Some(v) => {
            _append_opt_flag(env, &mut out, true);
            _append_u32le(env, &mut out, v);
        }
        None => _append_opt_flag(env, &mut out, false),
    }
    _append_u32le(env, &mut out, cfg.max_precision_participants);
    _append_u32le(env, &mut out, cfg.precision_payout_policy);
    _append_u32le(env, &mut out, cfg.dispute_ledgers);
    match cfg.early_cashout_bps {
        Some(v) => {
            _append_opt_flag(env, &mut out, true);
            _append_u32le(env, &mut out, v);
        }
        None => _append_opt_flag(env, &mut out, false),
    }
    out
}

fn _record_preimage(env: &Env, source_version: u32, tag: u8, payload: &Bytes) -> Bytes {
    let mut pre = Bytes::new(env);
    pre.append(&Bytes::from_slice(env, crate::types::MIGRATION_DOMAIN));
    _append_u32le(env, &mut pre, source_version);
    pre.append(&Bytes::from_slice(env, &[tag]));
    pre.append(payload);
    pre
}

pub fn _balance_leaf(env: &Env, source_version: u32, rec: &MigrationBalance) -> BytesN<32> {
    let mut payload = Bytes::new(env);
    _append_addr(env, &mut payload, &rec.user);
    _append_i128le(env, &mut payload, rec.amount);
    let pre = _record_preimage(env, source_version, 0x01, &payload);
    env.crypto().sha256(&pre).into()
}

pub fn _pending_leaf(env: &Env, source_version: u32, rec: &MigrationPending) -> BytesN<32> {
    let mut payload = Bytes::new(env);
    _append_addr(env, &mut payload, &rec.user);
    _append_i128le(env, &mut payload, rec.amount);
    let pre = _record_preimage(env, source_version, 0x02, &payload);
    env.crypto().sha256(&pre).into()
}

pub fn _config_leaf(env: &Env, source_version: u32, cfg: &MigrationConfig) -> BytesN<32> {
    let payload = _config_bytes(env, cfg);
    let pre = _record_preimage(env, source_version, 0x00, &payload);
    env.crypto().sha256(&pre).into()
}

fn _hash_pair(env: &Env, left: &BytesN<32>, right: &BytesN<32>) -> BytesN<32> {
    let mut buf = Bytes::new(env);
    buf.extend_from_array(&left.to_array());
    buf.extend_from_array(&right.to_array());
    env.crypto().sha256(&buf).into()
}

/// Deterministic Merkle root over the leaves (left-to-right, padded to the
/// next power of two with the null leaf, then hashing adjacent pairs).
pub fn _merkle_root(env: &Env, leaves: &Vec<BytesN<32>>) -> BytesN<32> {
    let mut level: Vec<BytesN<32>> = Vec::new(env);
    for l in leaves.iter() {
        level.push_back(l);
    }
    if level.is_empty() {
        return _null_leaf(env);
    }
    let mut target = 1u32;
    while target < level.len() {
        target = target.saturating_mul(2);
    }
    while level.len() < target {
        level.push_back(_null_leaf(env));
    }
    let mut cur = level;
    while cur.len() > 1 {
        let mut next: Vec<BytesN<32>> = Vec::new(env);
        let mut i = 0u32;
        while i + 1 < cur.len() {
            let l = cur.get(i).unwrap();
            let r = cur.get(i + 1).unwrap();
            next.push_back(_hash_pair(env, &l, &r));
            i += 2;
        }
        if i < cur.len() {
            next.push_back(cur.get(i).unwrap());
        }
        cur = next;
    }
    cur.get(0).unwrap()
}

/// Number of sibling steps required to reach the root for `leaf_count`.
pub fn _tree_height(leaf_count: u32) -> u32 {
    if leaf_count <= 1 {
        return 0;
    }
    let padded = leaf_count.saturating_sub(1).next_power_of_two();
    let mut h = 0u32;
    let mut n = padded;
    while n > 1 {
        n /= 2;
        h += 1;
    }
    h
}

/// Verifies a Merkle proof: walks from the leaf up `tree_height` levels
/// following `leaf_index` (even index = left child). Requires exactly
/// `tree_height` siblings and returns whether the recomputed root matches.
pub fn _merkle_verify(
    env: &Env,
    leaf_hash: &BytesN<32>,
    proof: &MerkleProof,
    root: &BytesN<32>,
) -> bool {
    if proof.siblings.len() != proof.tree_height {
        return false;
    }
    let mut cur = leaf_hash.clone();
    let mut idx = proof.leaf_index;
    for sib in proof.siblings.iter() {
        if idx.is_multiple_of(2) {
            cur = _hash_pair(env, &cur, &sib);
        } else {
            cur = _hash_pair(env, &sib, &cur);
        }
        idx /= 2;
    }
    cur == root.clone()
}

/// Reads the live canonical config subset from the source contract.
pub fn _read_canonical_config(env: &Env) -> MigrationConfig {
    MigrationConfig {
        protocol_fee_bps: _read_protocol_fee_bps(env),
        fee_model: _read_fee_model(env),
        protocol_fee_treasury: get_protocol_fee_treasury(env.clone()),
        bet_window_ledgers: get_bet_window_ledgers(env.clone()),
        run_window_ledgers: get_run_window_ledgers(env.clone()),
        close_buffer_ledgers: get_close_buffer_ledgers(env.clone()),
        max_stake: get_max_stake(env.clone()),
        max_user_round_exposure: get_max_user_exposure(env.clone()),
        max_pending_winnings: get_max_pending_winnings(env.clone()),
        min_bet: get_min_bet(env.clone()),
        min_participants: get_min_participants(env.clone()),
        max_precision_participants: get_max_precision_participants(env.clone()),
        precision_payout_policy: _read_precision_payout_policy(env) as u32,
        dispute_ledgers: get_dispute_ledgers(env),
        early_cashout_bps: get_early_cashout_bps(env.clone()),
    }
}

pub fn _is_frozen(env: &Env) -> bool {
    let key = MigrationKey::Frozen;
    _extend_persistent_ttl(env, &key);
    env.storage()
        .persistent()
        .get::<_, bool>(&key)
        .unwrap_or(false)
}

/// Blocks state-creating actions on a frozen source contract; claims and
/// read-only queries remain available.
pub fn _ensure_not_migration_frozen(env: &Env) -> Result<(), ContractError> {
    if _is_frozen(env) {
        return Err(ContractError::MigrationFrozen);
    }
    Ok(())
}

fn _require_admin(env: &Env) -> Result<Address, ContractError> {
    let admin = get_admin(env.clone()).ok_or(ContractError::AdminNotSet)?;
    admin.require_auth();
    Ok(admin)
}

fn _has_commitment(env: &Env) -> bool {
    env.storage().persistent().has(&MigrationKey::Commitment)
}

pub fn _get_commitment(env: &Env) -> Option<MigrationCommitment> {
    let key = MigrationKey::Commitment;
    _extend_persistent_ttl(env, &key);
    env.storage().persistent().get(&key)
}

fn _emit_rejected(env: &Env, actor: &Address, action: soroban_sdk::Symbol, reason: ContractError) {
    crate::common::_emit_action_rejected(env, actor, action, reason);
}

// ═══════════════════════════════════════════════════════════════════════════
// SOURCE: export + commitment + drain
// ═══════════════════════════════════════════════════════════════════════════

/// Opens a migration export session (admin only). Rejects if a commitment is
/// already finalized (an established commitment cannot be changed) or if a
/// round is active (the snapshot must be stable). Idempotent.
pub fn export_start(env: Env, dry_run: bool) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin = _require_admin(&env)?;
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_rejected(&env, &admin, symbol_short!("exp_st"), e);
    })?;

    if _has_commitment(&env) {
        return Err(ContractError::MigrationAlreadyFinalized);
    }
    if env.storage().persistent().has(&DataKeyCore::ActiveRound) {
        return Err(ContractError::MigrationActiveRound);
    }
    if dry_run {
        return Ok(());
    }

    let in_progress = MigrationKey::ExportInProgress;
    if !env
        .storage()
        .persistent()
        .get::<_, bool>(&in_progress)
        .unwrap_or(false)
    {
        env.storage().persistent().set(&in_progress, &true);
        _extend_persistent_ttl(&env, &in_progress);
    }
    if !env
        .storage()
        .persistent()
        .has(&MigrationKey::ExportBalances)
    {
        env.storage().persistent().set(
            &MigrationKey::ExportBalances,
            &Vec::<MigrationBalance>::new(&env),
        );
    }
    if !env
        .storage()
        .persistent()
        .has(&MigrationKey::ExportPendings)
    {
        env.storage().persistent().set(
            &MigrationKey::ExportPendings,
            &Vec::<MigrationPending>::new(&env),
        );
    }
    if !env
        .storage()
        .persistent()
        .has(&MigrationKey::ExportedBalanceUsers)
    {
        env.storage().persistent().set(
            &MigrationKey::ExportedBalanceUsers,
            &Vec::<Address>::new(&env),
        );
    }
    if !env
        .storage()
        .persistent()
        .has(&MigrationKey::ExportedPendingUsers)
    {
        env.storage().persistent().set(
            &MigrationKey::ExportedPendingUsers,
            &Vec::<Address>::new(&env),
        );
    }
    Ok(())
}

fn _user_already_exported(env: &Env, key: MigrationKey, user: &Address) -> bool {
    let list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    for u in list.iter() {
        if u == user.clone() {
            return true;
        }
    }
    false
}

/// Exports balances for a batch of users (admin only). Authoritative amounts
/// are re-read from `Balance(user)`; an absent balance exports as `0`. Each
/// user may be exported at most once.
pub fn export_balances(env: Env, users: Vec<Address>, dry_run: bool) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin = _require_admin(&env)?;
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_rejected(&env, &admin, symbol_short!("exp_bal"), e);
    })?;
    if _has_commitment(&env) {
        return Err(ContractError::MigrationAlreadyFinalized);
    }
    if !env
        .storage()
        .persistent()
        .get::<_, bool>(&MigrationKey::ExportInProgress)
        .unwrap_or(false)
    {
        return Err(ContractError::MigrationExportIncomplete);
    }
    if dry_run {
        return Ok(());
    }

    let mut list: Vec<MigrationBalance> = env
        .storage()
        .persistent()
        .get(&MigrationKey::ExportBalances)
        .unwrap_or_else(|| Vec::new(&env));

    for user in users.iter() {
        if _user_already_exported(&env, MigrationKey::ExportedBalanceUsers, &user) {
            return Err(ContractError::MigrationRecordAlreadyImported);
        }
        let amount = crate::common::balance(env.clone(), user.clone());
        list.push_back(MigrationBalance {
            user: user.clone(),
            amount,
        });
        let mut seen: Vec<Address> = env
            .storage()
            .persistent()
            .get(&MigrationKey::ExportedBalanceUsers)
            .unwrap_or_else(|| Vec::new(&env));
        seen.push_back(user.clone());
        env.storage()
            .persistent()
            .set(&MigrationKey::ExportedBalanceUsers, &seen);
        _extend_persistent_ttl(&env, &MigrationKey::ExportedBalanceUsers);
    }
    env.storage()
        .persistent()
        .set(&MigrationKey::ExportBalances, &list);
    _extend_persistent_ttl(&env, &MigrationKey::ExportBalances);
    Ok(())
}

/// Exports pending claims for a batch of users (admin only). Authoritative
/// amounts are re-read from `PendingWinnings(user)`.
pub fn export_pendings(env: Env, users: Vec<Address>, dry_run: bool) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin = _require_admin(&env)?;
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_rejected(&env, &admin, symbol_short!("exp_pend"), e);
    })?;
    if _has_commitment(&env) {
        return Err(ContractError::MigrationAlreadyFinalized);
    }
    if !env
        .storage()
        .persistent()
        .get::<_, bool>(&MigrationKey::ExportInProgress)
        .unwrap_or(false)
    {
        return Err(ContractError::MigrationExportIncomplete);
    }
    if dry_run {
        return Ok(());
    }

    let mut list: Vec<MigrationPending> = env
        .storage()
        .persistent()
        .get(&MigrationKey::ExportPendings)
        .unwrap_or_else(|| Vec::new(&env));

    for user in users.iter() {
        if _user_already_exported(&env, MigrationKey::ExportedPendingUsers, &user) {
            return Err(ContractError::MigrationRecordAlreadyImported);
        }
        let key = DataKeyScoped::PendingWinnings(user.clone());
        let amount: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        list.push_back(MigrationPending {
            user: user.clone(),
            amount,
        });
        let mut seen: Vec<Address> = env
            .storage()
            .persistent()
            .get(&MigrationKey::ExportedPendingUsers)
            .unwrap_or_else(|| Vec::new(&env));
        seen.push_back(user.clone());
        env.storage()
            .persistent()
            .set(&MigrationKey::ExportedPendingUsers, &seen);
        _extend_persistent_ttl(&env, &MigrationKey::ExportedPendingUsers);
    }
    env.storage()
        .persistent()
        .set(&MigrationKey::ExportPendings, &list);
    _extend_persistent_ttl(&env, &MigrationKey::ExportPendings);
    Ok(())
}

/// Builds the canonical, deterministically-ordered leaf set from the recorded
/// accumulators and the live config.
pub fn _build_leaves(env: &Env, source_version: u32) -> Vec<BytesN<32>> {
    let config = _read_canonical_config(env);
    let mut leaves: Vec<BytesN<32>> = Vec::new(env);
    leaves.push_back(_config_leaf(env, source_version, &config));

    let balances: Vec<MigrationBalance> = env
        .storage()
        .persistent()
        .get(&MigrationKey::ExportBalances)
        .unwrap_or_else(|| Vec::new(env));
    let mut sorted_b = alloc::vec::Vec::with_capacity(balances.len() as usize);
    for b in balances.iter() {
        sorted_b.push(b);
    }
    sorted_b.sort_by(|a, b2| {
        addr_key(&a.user)
            .cmp(&addr_key(&b2.user))
            .then(a.amount.cmp(&b2.amount))
    });
    for b in sorted_b {
        leaves.push_back(_balance_leaf(env, source_version, &b));
    }

    let pendings: Vec<MigrationPending> = env
        .storage()
        .persistent()
        .get(&MigrationKey::ExportPendings)
        .unwrap_or_else(|| Vec::new(env));
    let mut sorted_p = alloc::vec::Vec::with_capacity(pendings.len() as usize);
    for p in pendings.iter() {
        sorted_p.push(p);
    }
    sorted_p.sort_by(|a, b2| {
        addr_key(&a.user)
            .cmp(&addr_key(&b2.user))
            .then(a.amount.cmp(&b2.amount))
    });
    for p in sorted_p {
        leaves.push_back(_pending_leaf(env, source_version, &p));
    }
    leaves
}

fn addr_key(addr: &Address) -> soroban_sdk::String {
    addr.to_string()
}

/// Finalizes the source export: computes the commitment, freezes the source
/// into claims-only drain mode, and becomes a no-op once finalized (rejects
/// duplicates).
pub fn export_finalize(env: Env, dry_run: bool) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let admin = _require_admin(&env)?;
    _ensure_not_paused(&env).inspect_err(|&e| {
        _emit_rejected(&env, &admin, symbol_short!("exp_fin"), e);
    })?;

    if _has_commitment(&env) {
        return Err(ContractError::MigrationAlreadyFinalized);
    }
    if env.storage().persistent().has(&DataKeyCore::ActiveRound) {
        return Err(ContractError::MigrationActiveRound);
    }
    let source_version = _require_supported_schema(&env)?;
    if dry_run {
        return Ok(());
    }

    let leaves = _build_leaves(&env, source_version);
    let root = _merkle_root(&env, &leaves);
    let leaf_count = leaves.len();

    let commitment = MigrationCommitment {
        source_version,
        destination_version: MIGRATION_DESTINATION_VERSION,
        root,
        leaf_count,
        finalized_at_ledger: env.ledger().sequence(),
    };

    env.storage()
        .persistent()
        .set(&MigrationKey::Commitment, &commitment);
    _extend_persistent_ttl(&env, &MigrationKey::Commitment);
    env.storage()
        .persistent()
        .set(&MigrationKey::ExportInProgress, &false);

    _set_mode(&env, RuntimeMode::ClaimsOnly)?;
    env.storage().persistent().set(&MigrationKey::Frozen, &true);
    _extend_persistent_ttl(&env, &MigrationKey::Frozen);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("migrate"), symbol_short!("committed")),
        (
            commitment.source_version,
            commitment.destination_version,
            commitment.leaf_count,
        ),
    );
    Ok(())
}

/// Read-only source migration status.
pub fn get_migration_status(env: Env) -> MigrationStatus {
    let frozen = _is_frozen(&env);
    match _get_commitment(&env) {
        Some(c) => MigrationStatus {
            frozen,
            finalized: true,
            source_version: c.source_version,
            destination_version: c.destination_version,
            leaf_count: c.leaf_count,
            root: c.root,
        },
        None => MigrationStatus {
            frozen,
            finalized: false,
            source_version: _require_supported_schema(&env).unwrap_or(1),
            destination_version: 0,
            leaf_count: 0,
            root: BytesN::from_array(&env, &[0u8; 32]),
        },
    }
}

/// Source-contract migration lifecycle status (read-only).
#[soroban_sdk::contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MigrationStatus {
    pub frozen: bool,
    pub finalized: bool,
    pub source_version: u32,
    pub destination_version: u32,
    pub leaf_count: u32,
    pub root: BytesN<32>,
}

// ═══════════════════════════════════════════════════════════════════════════
// DESTINATION: import + verification + finalize
// ═══════════════════════════════════════════════════════════════════════════

/// Initializes the destination session bound to an expected source commitment
/// (admin only). Version checks reject a mismatched source/destination pair.
pub fn import_init(
    env: Env,
    expected_root: BytesN<32>,
    source_version: u32,
    destination_version: u32,
    leaf_count: u32,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    _require_admin(&env)?;

    if env
        .storage()
        .persistent()
        .get::<_, bool>(&MigrationKey::ImportInitialized)
        .unwrap_or(false)
    {
        return Err(ContractError::MigrationAlreadyInitialized);
    }
    if destination_version != MIGRATION_DESTINATION_VERSION {
        return Err(ContractError::MigrationVersionMismatch);
    }
    if source_version == 0 || source_version >= destination_version {
        return Err(ContractError::MigrationVersionMismatch);
    }

    let expected = MigrationCommitment {
        source_version,
        destination_version,
        root: expected_root,
        leaf_count,
        finalized_at_ledger: env.ledger().sequence(),
    };
    env.storage()
        .persistent()
        .set(&MigrationKey::ExpectedCommitment, &expected);
    env.storage()
        .persistent()
        .set(&MigrationKey::ImportInitialized, &true);
    _extend_persistent_ttl(&env, &MigrationKey::ExpectedCommitment);
    _extend_persistent_ttl(&env, &MigrationKey::ImportInitialized);
    Ok(())
}

fn _expected_commitment(env: &Env) -> Result<MigrationCommitment, ContractError> {
    env.storage()
        .persistent()
        .get::<_, MigrationCommitment>(&MigrationKey::ExpectedCommitment)
        .ok_or(ContractError::MigrationNotInitialized)
}

fn _bump_imported_count(env: &Env) {
    let new_count = env
        .storage()
        .persistent()
        .get::<_, u32>(&MigrationKey::ImportedRecords)
        .unwrap_or(0)
        + 1;
    env.storage()
        .persistent()
        .set(&MigrationKey::ImportedRecords, &new_count);
}

fn _check_import_ready(env: &Env) -> Result<MigrationCommitment, ContractError> {
    if env
        .storage()
        .persistent()
        .get::<_, bool>(&MigrationKey::ImportFinalized)
        .unwrap_or(false)
    {
        return Err(ContractError::MigrationAlreadyFinalized);
    }
    _expected_commitment(env)
}

/// Imports a single canonical balance record with a Merkle proof (admin only).
pub fn import_balance(
    env: Env,
    rec: MigrationBalance,
    proof: MerkleProof,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let _admin = _require_admin(&env)?;
    let expected = _check_import_ready(&env)?;

    if env
        .storage()
        .persistent()
        .has(&MigrationKey::ImportedBalance(rec.user.clone()))
    {
        return Err(ContractError::MigrationRecordAlreadyImported);
    }
    let leaf = _balance_leaf(&env, expected.source_version, &rec);
    if !_merkle_verify(&env, &leaf, &proof, &expected.root) {
        return Err(ContractError::MigrationProofInvalid);
    }

    crate::common::_set_balance(&env, rec.user.clone(), rec.amount);
    env.storage()
        .persistent()
        .set(&MigrationKey::ImportedBalance(rec.user.clone()), &true);
    _bump_imported_count(&env);
    Ok(())
}

/// Imports a single canonical pending-claim record with a Merkle proof
/// (admin only).
pub fn import_pending(
    env: Env,
    rec: MigrationPending,
    proof: MerkleProof,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let _admin = _require_admin(&env)?;
    let expected = _check_import_ready(&env)?;

    if env
        .storage()
        .persistent()
        .has(&MigrationKey::ImportedPending(rec.user.clone()))
    {
        return Err(ContractError::MigrationRecordAlreadyImported);
    }
    let leaf = _pending_leaf(&env, expected.source_version, &rec);
    if !_merkle_verify(&env, &leaf, &proof, &expected.root) {
        return Err(ContractError::MigrationProofInvalid);
    }

    let key = DataKeyScoped::PendingWinnings(rec.user.clone());
    env.storage().persistent().set(&key, &rec.amount);
    _extend_persistent_ttl(&env, &key);
    let updated_key = crate::types::PendingWinningsUpdatedAtKey(rec.user.clone());
    env.storage()
        .persistent()
        .set(&updated_key, &env.ledger().sequence());
    _extend_persistent_ttl(&env, &updated_key);
    env.storage()
        .persistent()
        .set(&MigrationKey::ImportedPending(rec.user.clone()), &true);
    _bump_imported_count(&env);
    Ok(())
}

/// Applies the canonical configuration subset to the destination (admin only),
/// verified against a single config-leaf Merkle proof.
pub fn import_config(
    env: Env,
    cfg: MigrationConfig,
    proof: MerkleProof,
) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let _admin = _require_admin(&env)?;
    let expected = _check_import_ready(&env)?;

    if env
        .storage()
        .persistent()
        .get::<_, bool>(&MigrationKey::ImportedConfig)
        .unwrap_or(false)
    {
        return Err(ContractError::MigrationRecordAlreadyImported);
    }
    let leaf = _config_leaf(&env, expected.source_version, &cfg);
    if !_merkle_verify(&env, &leaf, &proof, &expected.root) {
        return Err(ContractError::MigrationProofInvalid);
    }

    crate::config::_apply_imported_config(&env, &cfg)?;
    env.storage()
        .persistent()
        .set(&MigrationKey::ImportedConfig, &true);
    _bump_imported_count(&env);
    Ok(())
}

/// Finalizes the destination migration once the full expected subset has been
/// imported (admin only). Requires `ImportedRecords == leaf_count` so a
/// partial import cannot be finalized; repeated finalize fails safely.
pub fn import_finalize(env: Env) -> Result<(), ContractError> {
    _require_supported_schema(&env)?;
    let _admin = _require_admin(&env)?;
    let expected = _check_import_ready(&env)?;

    let imported = env
        .storage()
        .persistent()
        .get::<_, u32>(&MigrationKey::ImportedRecords)
        .unwrap_or(0);
    if imported != expected.leaf_count {
        return Err(ContractError::MigrationExportIncomplete);
    }

    env.storage()
        .persistent()
        .set(&MigrationKey::ImportFinalized, &true);
    _extend_persistent_ttl(&env, &MigrationKey::ImportFinalized);
    Ok(())
}

// Imported for API stability; asserts the referenced data keys remain in use.
#[allow(dead_code)]
const _USED: () = {
    let _ = CURRENT_SCHEMA_VERSION;
};
