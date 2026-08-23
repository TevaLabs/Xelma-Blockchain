// SPDX-License-Identifier: MIT
//! Pool-aggregate commitments published during betting (Observability /
//! ZK-friendly design).
//!
//! While a round is in its betting window, the raw `pool_up`/`pool_down`
//! (and the Precision aggregate stake) are never included in the events this
//! module emits — only a rolling SHA-256 commitment is. Once
//! `bet_end_ledger` is reached, [`open_pool_commitment`] (or automatic
//! opening at settlement — see `settlement::_archive_round`, which calls
//! [`_open_pool_commitment`] on every terminal round transition) reveals the
//! salt and final aggregate, so any observer can recompute the
//! commitment and check it against the last `("pool", "commit")` event.
//!
//! **Trust boundary (documented in `docs/POOL_COMMITMENTS.md`):** this does
//! not hide values from a party reading raw ledger storage directly —
//! Soroban persistent entries are public via RPC regardless of which
//! contract functions expose them. It reduces the *observability* surface
//! (events and the dedicated queries below) that a passive indexer sees
//! pre-close, and gives a verifiable binding that a future ZK proof could
//! sit behind without reshaping the surface. `get_round_pool_stats` is left
//! fully unchanged (Issue: additive-only, no breaking change) and remains
//! the legacy full-disclosure surface for callers who don't need the
//! fairness properties here.
//!
//! Commitment format:
//! ```text
//! commitment = sha256(
//!     round_id.to_xdr() || seq.to_xdr() || pool_up.to_xdr() ||
//!     pool_down.to_xdr() || precision_total_stake.to_xdr() || salt.to_xdr()
//! )
//! ```
//! `salt` is generated once per round via `env.prng()` and never returned by
//! any getter until the round is opened. Soroban's PRNG is not suitable for
//! adversarial secrecy against validators (see `soroban_sdk::Env::prng`
//! docs) — this scheme is a fairness/binding commitment, not confidentiality.

use crate::common::_extend_persistent_ttl;
use crate::errors::ContractError;
use crate::types::{DataKeyScoped, PoolCommitment, PoolOpening, Round, RoundMode};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{symbol_short, Bytes, BytesN, Env};

fn _pool_commitment_salt(env: &Env, round_id: u64) -> BytesN<32> {
    let key = DataKeyScoped::PoolCommitmentSalt(round_id);
    if let Some(salt) = env.storage().persistent().get::<_, BytesN<32>>(&key) {
        _extend_persistent_ttl(env, &key);
        return salt;
    }
    let raw: [u8; 32] = env.prng().gen();
    let salt = BytesN::from_array(env, &raw);
    env.storage().persistent().set(&key, &salt);
    _extend_persistent_ttl(env, &key);
    salt
}

fn _precision_total_stake(env: &Env, round_id: u64) -> i128 {
    let key = DataKeyScoped::PrecisionCommitStake(round_id);
    env.storage().persistent().get(&key).unwrap_or(0)
}

/// O(1) increment to the running Precision-mode stake total, called by
/// `place_precision_prediction` and `commit_prediction` alongside their
/// existing storage writes. Keeps the pool commitment's Precision aggregate
/// gas-bounded (no participant scan).
pub fn _add_precision_commit_stake(env: &Env, round_id: u64, amount: i128) {
    let key = DataKeyScoped::PrecisionCommitStake(round_id);
    let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
    let updated = current.checked_add(amount).unwrap_or(current);
    env.storage().persistent().set(&key, &updated);
    _extend_persistent_ttl(env, &key);
}

fn _compute_commitment(
    env: &Env,
    round_id: u64,
    seq: u32,
    pool_up: i128,
    pool_down: i128,
    precision_total_stake: i128,
    salt: &BytesN<32>,
) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&round_id.to_xdr(env));
    preimage.append(&seq.to_xdr(env));
    preimage.append(&pool_up.to_xdr(env));
    preimage.append(&pool_down.to_xdr(env));
    preimage.append(&precision_total_stake.to_xdr(env));
    preimage.append(&salt.to_xdr(env));
    env.crypto().sha256(&preimage).into()
}

/// Advances a round's pool commitment. Called at the end of every mutating
/// betting action (`place_bet`, `place_precision_prediction`,
/// `commit_prediction`, `reveal_prediction`) — O(1): one salt lookup, one
/// hash over a handful of fixed-size fields, one commitment write, one
/// event. No iteration over participants.
pub fn _advance_pool_commitment(env: &Env, round: &Round) {
    let round_id = round.round_id;
    let salt = _pool_commitment_salt(env, round_id);
    let precision_total_stake = match round.mode {
        RoundMode::UpDown => 0,
        RoundMode::Precision => _precision_total_stake(env, round_id),
    };

    let commit_key = DataKeyScoped::PoolCommitment(round_id);
    let seq = env
        .storage()
        .persistent()
        .get::<_, PoolCommitment>(&commit_key)
        .map(|c| c.seq)
        .unwrap_or(0)
        .checked_add(1)
        .unwrap_or(u32::MAX);

    let commitment = _compute_commitment(
        env,
        round_id,
        seq,
        round.pool_up,
        round.pool_down,
        precision_total_stake,
        &salt,
    );
    let ledger = env.ledger().sequence();

    env.storage().persistent().set(
        &commit_key,
        &PoolCommitment {
            round_id,
            seq,
            commitment: commitment.clone(),
            ledger,
        },
    );
    _extend_persistent_ttl(env, &commit_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("pool"), symbol_short!("commit")),
        (round_id, seq, commitment, ledger),
    );
}

/// Reveals the final aggregate for a round's pool commitment, if not already
/// opened. Idempotent no-op when already opened (used both by the
/// permissionless `open_pool_commitment` entrypoint and by the automatic
/// open-at-settlement hook in `settlement::_ensure_pool_commitment_opened`).
pub fn _open_pool_commitment(env: &Env, round: &Round) {
    let round_id = round.round_id;
    let opening_key = DataKeyScoped::PoolOpening(round_id);
    if env.storage().persistent().has(&opening_key) {
        return;
    }

    let salt_key = DataKeyScoped::PoolCommitmentSalt(round_id);
    let salt: BytesN<32> = env
        .storage()
        .persistent()
        .get(&salt_key)
        .unwrap_or_else(|| BytesN::from_array(env, &[0u8; 32]));

    let commit_key = DataKeyScoped::PoolCommitment(round_id);
    let seq = env
        .storage()
        .persistent()
        .get::<_, PoolCommitment>(&commit_key)
        .map(|c| c.seq)
        .unwrap_or(0);

    let precision_total_stake = match round.mode {
        RoundMode::UpDown => 0,
        RoundMode::Precision => _precision_total_stake(env, round_id),
    };
    let opened_ledger = env.ledger().sequence();

    let opening = PoolOpening {
        round_id,
        seq,
        pool_up: round.pool_up,
        pool_down: round.pool_down,
        precision_total_stake,
        salt,
        opened_ledger,
    };
    env.storage().persistent().set(&opening_key, &opening);
    _extend_persistent_ttl(env, &opening_key);

    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("pool"), symbol_short!("open")),
        (
            round_id,
            seq,
            opening.pool_up,
            opening.pool_down,
            opening.precision_total_stake,
            opening.salt,
        ),
    );
}

/// Permissionless: reveals the pool commitment for `round_id` once betting
/// has closed. Anyone may call this — no auth required — so indexers get
/// verifiable numbers even if oracle resolution lags `bet_end_ledger`.
/// Errors `InvalidRevealWindow` if called too early, `AlreadyRevealed` if
/// already opened for this round.
pub fn open_pool_commitment(env: Env, round_id: u64) -> Result<(), ContractError> {
    let round: Round = env
        .storage()
        .persistent()
        .get(&crate::types::DataKeyCore::ActiveRound)
        .filter(|r: &Round| r.round_id == round_id)
        .ok_or(ContractError::NoActiveRound)?;

    if env.ledger().sequence() < round.bet_end_ledger {
        return Err(ContractError::InvalidRevealWindow);
    }
    if env
        .storage()
        .persistent()
        .has(&DataKeyScoped::PoolOpening(round_id))
    {
        return Err(ContractError::AlreadyRevealed);
    }

    _open_pool_commitment(&env, &round);
    Ok(())
}

/// Read-only: the latest published commitment for `round_id` — a hash, a
/// sequence number, and a ledger. Always available (pre- or post-close);
/// never carries raw aggregate values.
pub fn get_pool_commitment(env: Env, round_id: u64) -> Option<PoolCommitment> {
    env.storage()
        .persistent()
        .get(&DataKeyScoped::PoolCommitment(round_id))
}

/// Read-only: the revealed opening for `round_id`, or `None` if the round
/// hasn't been opened yet — the fail-closed signal for indexers querying
/// before `bet_end_ledger` (or before settlement/force-open has run).
pub fn get_pool_opening(env: Env, round_id: u64) -> Option<PoolOpening> {
    env.storage()
        .persistent()
        .get(&DataKeyScoped::PoolOpening(round_id))
}
