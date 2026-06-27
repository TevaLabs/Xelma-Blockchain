#!/usr/bin/env python3
"""Integrate protocol-fee deduction into the 4 settlement paths
(Issue #162). Operates on contracts/src/contract.rs.

Strategy: each settlement helper computes `dist_*` pools (after fee) at the
top of its hot loop; no signature changes to callers (resolve_round etc).
"""
import sys

p = '/workspaces/Xelma-Blockchain/contracts/src/contract.rs'
src = open(p).read()

# ---------------------------------------------------------------------------
# 1) _record_winnings_indexed  (UpDown, indexed path)
# ---------------------------------------------------------------------------
A_INDEXED = (
    "    fn _record_winnings_indexed(\n"
    "        env: &Env,\n"
    "        round_id: u64,\n"
    "        participants: &Vec<Address>,\n"
    "        winning_side: BetSide,\n"
    "        winning_pool: i128,\n"
    "        losing_pool: i128,\n"
    "    ) -> Result<(), ContractError> {\n"
    "        if winning_pool == 0 {\n"
    "            return Ok(());\n"
    "        }\n"
    "\n"
    "        for i in 0..participants.len() {\n"
)
B_INDEXED = (
    "    fn _record_winnings_indexed(\n"
    "        env: &Env,\n"
    "        round_id: u64,\n"
    "        participants: &Vec<Address>,\n"
    "        winning_side: BetSide,\n"
    "        winning_pool: i128,\n"
    "        losing_pool: i128,\n"
    "    ) -> Result<(), ContractError> {\n"
    "        if winning_pool == 0 {\n"
    "            return Ok(());\n"
    "        }\n"
    "\n"
    "        // Apply protocol fee (Issue #162). Conservation invariant\n"
    "        // `dist_winning + dist_losing + fee == winning + losing` always\n"
    "        // holds; in the pathological case `fee > losing_pool` the spillover\n"
    "        // is taken from `winning_pool`. Fee event already emitted inside\n"
    "        // the helper.\n"
    "        let (winning_pool, losing_pool, _fee_amount) =\n"
    "            Self::_apply_protocol_fee_updown(env, round_id, winning_pool, losing_pool)?;\n"
    "\n"
    "        for i in 0..participants.len() {\n"
)

# ---------------------------------------------------------------------------
# 2) _record_winnings_legacy  (UpDown, legacy bulk-map path)
# ---------------------------------------------------------------------------
A_LEGACY_UP = (
    "    fn _record_winnings_legacy(\n"
    "        env: &Env,\n"
    "        round_id: u64,\n"
    "        positions: &Map<Address, UserPosition>,\n"
    "        winning_side: BetSide,\n"
    "        winning_pool: i128,\n"
    "        losing_pool: i128,\n"
    "    ) -> Result<(), ContractError> {\n"
    "        if winning_pool == 0 {\n"
    "            return Ok(());\n"
    "        }\n"
    "        let keys: Vec<Address> = positions.keys();\n"
)
B_LEGACY_UP = (
    "    fn _record_winnings_legacy(\n"
    "        env: &Env,\n"
    "        round_id: u64,\n"
    "        positions: &Map<Address, UserPosition>,\n"
    "        winning_side: BetSide,\n"
    "        winning_pool: i128,\n"
    "        losing_pool: i128,\n"
    "    ) -> Result<(), ContractError> {\n"
    "        if winning_pool == 0 {\n"
    "            return Ok(());\n"
    "        }\n"
    "\n"
    "        // Apply protocol fee (Issue #162); see `_record_winnings_indexed`.\n"
    "        let (winning_pool, losing_pool, _fee_amount) =\n"
    "            Self::_apply_protocol_fee_updown(env, round_id, winning_pool, losing_pool)?;\n"
    "\n"
    "        let keys: Vec<Address> = positions.keys();\n"
)

# ---------------------------------------------------------------------------
# 3) _resolve_precision_mode  (Precision, indexed path)
# ---------------------------------------------------------------------------
# Anchor: the winner-distribution `if !winners.is_empty() && total_pot > 0` block.
A_PRECISION_INDEXED = (
    "        if !winners.is_empty() && total_pot > 0 {\n"
    "            let winner_count = winners.len() as i128;\n"
    "            let payout_per_winner = total_pot / winner_count;\n"
    "            let remainder = total_pot % winner_count;\n"
    "\n"
    "            // Award to each winner\n"
    "            for i in 0..winners.len() {\n"
)
B_PRECISION_INDEXED = (
    "        if !winners.is_empty() && total_pot > 0 {\n"
    "            // Apply protocol fee (Issue #162) before splitting the pot.\n"
    "            // Conservation invariant `distributable + fee == total_pot`.\n"
    "            let (payout_pool, _fee_amount) =\n"
    "                Self::_apply_protocol_fee_precision(env, round_id, total_pot)?;\n"
    "            let winner_count = winners.len() as i128;\n"
    "            let payout_per_winner = payout_pool / winner_count;\n"
    "            let remainder = payout_pool % winner_count;\n"
    "\n"
    "            // Award to each winner\n"
    "            for i in 0..winners.len() {\n"
)

# ---------------------------------------------------------------------------
# 4) _resolve_precision_legacy  (Precision, legacy bulk-map path)
# ---------------------------------------------------------------------------
A_PRECISION_LEGACY = (
    "        // Remainder policy: `predictions_map` is a `Map<Address, PrecisionPrediction>`, which\n"
    "        // Soroban keeps sorted by XDR-encoded key bytes. `winners` is built by iterating\n"
    "        // `predictions_map.values()` in that stable key order, so index 0 always refers to\n"
    "        // the lexicographically-lowest Address. Any integer remainder from the even split is\n"
    "        // assigned exclusively to that winner, making the distribution fully deterministic.\n"
    "        if !winners.is_empty() && total_pot > 0 {\n"
    "            let winner_count = winners.len() as i128;\n"
    "            let payout_per_winner = total_pot / winner_count;\n"
    "            let remainder = total_pot % winner_count;\n"
)
B_PRECISION_LEGACY = (
    "        // Remainder policy: `predictions_map` is a `Map<Address, PrecisionPrediction>`, which\n"
    "        // Soroban keeps sorted by XDR-encoded key bytes. `winners` is built by iterating\n"
    "        // `predictions_map.values()` in that stable key order, so index 0 always refers to\n"
    "        // the lexicographically-lowest Address. Any integer remainder from the even split is\n"
    "        // assigned exclusively to that winner, making the distribution fully deterministic.\n"
    "        if !winners.is_empty() && total_pot > 0 {\n"
    "            // Apply protocol fee (Issue #162) before splitting the pot.\n"
    "            let (payout_pool, _fee_amount) =\n"
    "                Self::_apply_protocol_fee_precision(env, round_id, total_pot)?;\n"
    "            let winner_count = winners.len() as i128;\n"
    "            let payout_per_winner = payout_pool / winner_count;\n"
    "            let remainder = payout_pool % winner_count;\n"
)


def must_replace(s, anchor, new, label):
    if anchor not in s:
        print(f'[{label}] ANCHOR NOT FOUND', file=sys.stderr)
        sys.exit(1)
    if s.count(anchor) != 1:
        print(f'[{label}] anchor matches {s.count(anchor)} times, expected 1', file=sys.stderr)
        sys.exit(1)
    return s.replace(anchor, new)


EDITS = [
    (A_INDEXED, B_INDEXED, 'updown_indexed'),
    (A_LEGACY_UP, B_LEGACY_UP, 'updown_legacy'),
    (A_PRECISION_INDEXED, B_PRECISION_INDEXED, 'precision_indexed'),
    (A_PRECISION_LEGACY, B_PRECISION_LEGACY, 'precision_legacy'),
]

new_src = src
for a, b, label in EDITS:
    new_src = must_replace(new_src, a, b, label)

if new_src == src:
    print('no changes')
else:
    open(p, 'w').write(new_src)
    print(f'OK: file now {len(new_src.splitlines())} lines (was {len(src.splitlines())})')
