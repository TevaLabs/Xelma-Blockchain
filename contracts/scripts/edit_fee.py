#!/usr/bin/env python3
"""Insert protocol-fee helpers, _apply_config_payload arm, and integrate
fee deduction into all 4 settlement paths of contracts/src/contract.rs
(Issue #162).

Usage: python3 scripts/edit_fee.py
"""
import sys

p = '/workspaces/Xelma-Blockchain/contracts/src/contract.rs'
src = open(p).read()

# ---------------------------------------------------------------------------
# Edit 1: helpers after _validate_oracle_max_deviation_bps
# ---------------------------------------------------------------------------
ANCHOR_HELPERS = (
    "    fn _validate_oracle_max_deviation_bps(bps: Option<u32>) -> Result<(), ContractError> {\n"
    "        if let Some(v) = bps {\n"
    "            if v == 0 || v > MAX_ORACLE_DEVIATION_BPS {\n"
    "                return Err(ContractError::InvalidOracleDeviationBps);\n"
    "            }\n"
    "        }\n"
    "        Ok(())\n"
    "    }\n"
)

INSERT_HELPERS = r"""
    /// Validates a requested protocol-fee bps (Issue #162).
    /// `None` always allowed (disables fee entirely, restoring pre-#162
    /// byte-for-byte behaviour). `Some(0)` is rejected — only explicit `None`
    /// is the legitimate way to express "fee disabled". `Some(bps)` must
    /// satisfy `1 <= bps <= MAX_PROTOCOL_FEE_BPS`.
    fn _validate_protocol_fee_bps(bps: Option<u32>) -> Result<(), ContractError> {
        if let Some(v) = bps {
            if v == 0 || v > MAX_PROTOCOL_FEE_BPS {
                return Err(ContractError::InvalidProtocolFeeBps);
            }
        }
        Ok(())
    }

    /// Reads the currently-configured protocol fee in bps (Issue #162).
    /// Bumps TTL only when the key is present (avoids extra storage writes
    /// on the hot "fee disabled" path through every competitive settlement).
    fn _read_protocol_fee_bps(env: &Env) -> Option<u32> {
        let key = DataKey::ProtocolFeeBps;
        let v: Option<u32> = env.storage().persistent().get(&key);
        if v.is_some() {
            Self::_extend_persistent_ttl(env, &key);
        }
        v
    }

    /// Credits `fee_amount` stroops to the protocol fee treasury and emits
    /// `("protocol", "fee_collected")` (Issue #162). TTL on the treasury
    /// key is extended on every write so the cumulative balance never
    /// falls into archival. Payload mirrors the active bps so indexers
    /// do not need an extra storage read.
    fn _collect_protocol_fee(
        env: &Env,
        round_id: u64,
        fee_amount: i128,
        bps_active: Option<u32>,
    ) -> Result<(), ContractError> {
        if fee_amount <= 0 {
            return Ok(());
        }
        let treasury_key = DataKey::ProtocolFeeTreasury;
        let current: i128 = env
            .storage()
            .persistent()
            .get(&treasury_key)
            .unwrap_or(0);
        let new_treasury = current
            .checked_add(fee_amount)
            .ok_or(ContractError::Overflow)?;
        env.storage().persistent().set(&treasury_key, &new_treasury);
        Self::_extend_persistent_ttl(env, &treasury_key);

        let bps_value: u32 = bps_active.unwrap_or(0);

        #[allow(deprecated)]
        env.events().publish(
            (symbol_short!("protocol"), symbol_short!("fee_collected")),
            (round_id, fee_amount, new_treasury, bps_value),
        );

        Ok(())
    }

    /// Splits a `(winning_pool, losing_pool)` pair into the post-fee pools
    /// and the treasury's cut, used by both UpDown settlement paths
    /// (Issue #162). Conservation invariant
    ///   dist_winning + dist_losing + fee == winning + losing
    /// holds ALWAYS, even in the pathological case `fee > losing_pool`
    /// (very thin losing-side liquidity near the bps cap): the spillover
    /// is then deducted from `winning_pool`, so winners lose a portion
    /// of their principal rather than the fee being silently dropped.
    /// Behaviour is documented in `docs/EVENT_SCHEMA.md` and exercised
    /// by `test_protocol_fee_thin_losing_pool`.
    fn _apply_protocol_fee_updown(
        env: &Env,
        round_id: u64,
        winning_pool: i128,
        losing_pool: i128,
    ) -> Result<(i128, i128, i128), ContractError> {
        let bps = Self::_read_protocol_fee_bps(env);
        if bps.is_none() {
            return Ok((winning_pool, losing_pool, 0));
        }
        let bps_value = bps.unwrap();
        let total_pot = Self::payout_add(winning_pool, losing_pool)?;
        let fee_amount = total_pot
            .checked_mul(bps_value as i128)
            .ok_or(ContractError::Overflow)?
            / BPS_DENOMINATOR;
        if fee_amount == 0 {
            return Ok((winning_pool, losing_pool, 0));
        }
        let fee_from_losing = fee_amount.min(losing_pool);
        let fee_from_winning = fee_amount
            .checked_sub(fee_from_losing)
            .ok_or(ContractError::Overflow)?;
        let dist_winning = winning_pool
            .checked_sub(fee_from_winning)
            .ok_or(ContractError::Overflow)?;
        let dist_losing = losing_pool
            .checked_sub(fee_from_losing)
            .ok_or(ContractError::Overflow)?;
        Self::_collect_protocol_fee(env, round_id, fee_amount, Some(bps_value))?;
        Ok((dist_winning, dist_losing, fee_amount))
    }

    /// Splits a precision-mode `total_pot` into the distributable amount
    /// (split among winners per the existing remainder policy) and the
    /// treasury's cut (Issue #162). Returns `(distributable, fee_amount)`.
    fn _apply_protocol_fee_precision(
        env: &Env,
        round_id: u64,
        total_pot: i128,
    ) -> Result<(i128, i128), ContractError> {
        let bps = Self::_read_protocol_fee_bps(env);
        if bps.is_none() || total_pot <= 0 {
            return Ok((total_pot, 0));
        }
        let bps_value = bps.unwrap();
        let fee_amount = total_pot
            .checked_mul(bps_value as i128)
            .ok_or(ContractError::Overflow)?
            / BPS_DENOMINATOR;
        let distributable = total_pot
            .checked_sub(fee_amount)
            .ok_or(ContractError::Overflow)?;
        if fee_amount > 0 {
            Self::_collect_protocol_fee(env, round_id, fee_amount, Some(bps_value))?;
        }
        Ok((distributable, fee_amount))
    }
"""

# ---------------------------------------------------------------------------
# Edit 2: add ProtocolFeeBps arm to _apply_config_payload  (before the tail `_ =>` arm)
# We splice immediately before:
#     (ConfigChangeKind::OracleMaxDeviationBps, ...) ... } _ => return Err(...)
# ---------------------------------------------------------------------------
ARM_ANCHOR = (
    "            (\n"
    "                ConfigChangeKind::OracleMaxDeviationBps,\n"
    "                ConfigChangePayload::OracleMaxDeviationBps(bps),\n"
    "            ) => {\n"
    "                Self::_validate_oracle_max_deviation_bps(*bps)?;\n"
    "                let key = DataKey::OracleMaxDeviationBps;\n"
    "                if let Some(v) = bps {\n"
    "                    env.storage().persistent().set(&key, v);\n"
    "                    Self::_extend_persistent_ttl(env, &key);\n"
    "                } else {\n"
    "                    env.storage().persistent().remove(&key);\n"
    "                }\n"
    "            }\n"
)

ARM_INSERT = r"""
            (
                ConfigChangeKind::ProtocolFeeBps,
                ConfigChangePayload::ProtocolFeeBps(bps),
            ) => {
                Self::_validate_protocol_fee_bps(*bps)?;
                let key = DataKey::ProtocolFeeBps;
                if let Some(v) = bps {
                    env.storage().persistent().set(&key, v);
                    Self::_extend_persistent_ttl(env, &key);
                } else {
                    env.storage().persistent().remove(&key);
                }
                #[allow(deprecated)]
                env.events().publish(
                    (symbol_short!("protocol"), symbol_short!("fee_bps_set")),
                    (*bps,),
                );
            }
"""

# ---------------------------------------------------------------------------
# Apply edits
# ---------------------------------------------------------------------------
def must_replace(s, anchor, insert, label):
    if anchor not in s:
        print(f'[{label}] ANCHOR NOT FOUND', file=sys.stderr)
        sys.exit(1)
    if s.count(anchor) != 1:
        print(f'[{label}] anchor matches {s.count(anchor)} times, expected 1', file=sys.stderr)
        sys.exit(1)
    if not isinstance(insert, str):
        insert = str(insert)
    return s.replace(anchor, anchor + insert if not insert.startswith('\n') else anchor + insert)

new_src = must_replace(src, ANCHOR_HELPERS, INSERT_HELPERS, 'helpers')
new_src = must_replace(new_src, ARM_ANCHOR, ARM_INSERT, 'apply_cfg_arm')

if new_src == src:
    print('no changes')
else:
    open(p, 'w').write(new_src)
    print(f'OK: file now {len(new_src.splitlines())} lines (was {len(src.splitlines())})')
