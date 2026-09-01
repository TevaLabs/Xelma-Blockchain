#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Generate the offline migration witness for Issue #366 (blue/green upgrade).

This tool computes, from a canonical export manifest, the exact Merkle
commitment (root) and per-record proof that the Xelma contract's migration
module verifies on-chain. It is the Python-side complement of `migration.rs`
and MUST stay byte-for-byte compatible with the canonical encoding documented
in `docs/UPGRADE_BLUE_GREEN.md`.

Canonical encoding (shared with the contract):

    preimage = b"XELMA-CPAY-V1" ++ u32le(source_version) ++ record

    record (tag 0x00, config):
        0x00
        ++ presence(u32 protocol_fee_bps)
        ++ u32le(fee_model)
        ++ i128le(protocol_fee_treasury)
        ++ u32le(bet_window_ledgers) ++ u32le(run_window_ledgers)
        ++ u32le(close_buffer_ledgers)
        ++ presence(i128 max_stake) ++ presence(i128 max_user_round_exposure)
        ++ presence(i128 max_pending_winnings) ++ presence(i128 min_bet)
        ++ presence(u32 min_participants)
        ++ u32le(max_precision_participants) ++ u32le(precision_payout_policy)
        ++ u32le(dispute_ledgers) ++ presence(u32 early_cashout_bps)

    record (tag 0x01, balance): 0x01 ++ addr ++ i128le(amount)
    record (tag 0x02, pending):  0x02 ++ addr ++ i128le(amount)

    addr = u32le(len(addr_string_bytes)) ++ addr_string_bytes

Leaf order is deterministic and independent of storage iteration: the config
leaf first, then balance leaves sorted by their StrKey string, then pending
leaves sorted by their StrKey string. The tree is padded to the next power of
two with the null leaf `sha256(b"XELMA-CPAY-V1" ++ b"\xff")`, then hashed pair
by pair (`sha256(left ++ right)`) bottom-up. An empty set yields the root of a
single null leaf.

Usage:
    python scripts/generate_migration_witness.py \
        --source-version 3 \
        --manifest export.json \
        --output witness.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

DOMAIN = b"XELMA-CPAY-V1"
NULL_LEAF = hashlib.sha256(DOMAIN + b"\xff").digest()


def u32le(v: int) -> bytes:
    return int(v).to_bytes(4, "little", signed=False)


def i128le(v: int) -> bytes:
    return int(v).to_bytes(16, "little", signed=True)


def presence(value) -> bytes:
    return b"\x01" if value is not None else b"\x00"


def addr_bytes(addr: str) -> bytes:
    if not addr.isascii():
        raise ValueError(f"address is not ASCII: {addr!r}")
    raw = addr.encode("ascii")
    if len(raw) > 0xFFFFFFFF:
        raise ValueError("address too long")
    return u32le(len(raw)) + raw


def opt_i128(v, out: bytearray) -> None:
    out += presence(v)
    if v is not None:
        out += i128le(v)


def opt_u32(v, out: bytearray) -> None:
    out += presence(v)
    if v is not None:
        out += u32le(v)


def config_record(cfg: dict) -> bytes:
    out = bytearray()
    opt_u32(cfg.get("protocol_fee_bps"), out)
    out += u32le(cfg["fee_model"])
    out += i128le(cfg["protocol_fee_treasury"])
    out += u32le(cfg["bet_window_ledgers"])
    out += u32le(cfg["run_window_ledgers"])
    out += u32le(cfg["close_buffer_ledgers"])
    opt_i128(cfg.get("max_stake"), out)
    opt_i128(cfg.get("max_user_round_exposure"), out)
    opt_i128(cfg.get("max_pending_winnings"), out)
    opt_i128(cfg.get("min_bet"), out)
    opt_u32(cfg.get("min_participants"), out)
    out += u32le(cfg["max_precision_participants"])
    out += u32le(cfg["precision_payout_policy"])
    out += u32le(cfg["dispute_ledgers"])
    opt_u32(cfg.get("early_cashout_bps"), out)
    return bytes(out)


def record_preimage(source_version: int, tag: int, record: bytes) -> bytes:
    return DOMAIN + u32le(source_version) + bytes([tag]) + record


def balance_leaf(source_version: int, rec: dict) -> bytes:
    payload = addr_bytes(rec["user"]) + i128le(rec["amount"])
    return hashlib.sha256(record_preimage(source_version, 0x01, payload)).digest()


def pending_leaf(source_version: int, rec: dict) -> bytes:
    payload = addr_bytes(rec["user"]) + i128le(rec["amount"])
    return hashlib.sha256(record_preimage(source_version, 0x02, payload)).digest()


def config_leaf(source_version: int, cfg: dict) -> bytes:
    return hashlib.sha256(record_preimage(source_version, 0x00, config_record(cfg))).digest()


def build_leaves(source_version: int, manifest: dict) -> list[bytes]:
    leaves = [config_leaf(source_version, manifest["config"])]
    balances = sorted(manifest.get("balances", []), key=lambda r: r["user"])
    for b in balances:
        leaves.append(balance_leaf(source_version, b))
    pendings = sorted(manifest.get("pendings", []), key=lambda r: r["user"])
    for p in pendings:
        leaves.append(pending_leaf(source_version, p))
    return leaves


def tree(leaves: list[bytes]) -> tuple[bytes, int]:
    """Return (root, tree_height) for the padded binary Merkle tree."""
    level = list(leaves)
    if not level:
        return NULL_LEAF, 0
    size = 1
    while size < len(level):
        size *= 2
    while len(level) < size:
        level.append(NULL_LEAF)
    height = 0
    while len(level) > 1:
        nxt = []
        for i in range(0, len(level) - 1, 2):
            nxt.append(hashlib.sha256(level[i] + level[i + 1]).digest())
        if len(level) % 2 == 1:
            nxt.append(level[-1])
        level = nxt
        height += 1
    return level[0], height


def proof_for(leaves: list[bytes], target: int) -> tuple[int, list[str]]:
    """Return (leaf_index, hex-sibling-list) for the padded tree."""
    if not leaves:
        raise ValueError("empty leaf set")
    size = 1
    while size < len(leaves):
        size *= 2
    level = list(leaves)
    while len(level) < size:
        level.append(NULL_LEAF)
    index = target
    siblings = []
    while len(level) > 1:
        nxt = []
        for i in range(0, len(level) - 1, 2):
            nxt.append(hashlib.sha256(level[i] + level[i + 1]).digest())
        if len(level) % 2 == 1:
            nxt.append(level[-1])
        sib = index + 1 if index % 2 == 0 else index - 1
        if sib < len(level):
            siblings.append(level[sib])
        else:
            siblings.append(NULL_LEAF)
        index //= 2
        level = nxt
    return target, [s.hex() for s in siblings]


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate Xelma migration witness (Issue #366)")
    parser.add_argument("--source-version", type=int, required=True, help="schema version of the source contract")
    parser.add_argument("--manifest", type=Path, required=True, help="canonical export manifest (JSON)")
    parser.add_argument("--output", type=Path, required=True, help="output witness (JSON)")
    args = parser.parse_args()

    with args.manifest.open("r", encoding="utf-8") as fh:
        manifest = json.load(fh)

    leaves = build_leaves(args.source_version, manifest)
    root, _height = tree(leaves)

    proof_set = {"config": proof_for(leaves, 0)}
    for idx, b in enumerate(sorted(manifest.get("balances", []), key=lambda r: r["user"]), start=1):
        proof_set["balance:" + b["user"]] = proof_for(leaves, idx)
    pending_start = 1 + len(manifest.get("balances", []))
    for off, p in enumerate(sorted(manifest.get("pendings", []), key=lambda r: r["user"])):
        proof_set["pending:" + p["user"]] = proof_for(leaves, pending_start + off)

    witness = {
        "source_version": args.source_version,
        "destination_version": 4,
        "leaf_count": len(leaves),
        "root": root.hex(),
        "proofs": {
            k: {"leaf_index": v[0], "tree_height": len(v[1]), "siblings": v[1]}
            for k, v in proof_set.items()
        },
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as fh:
        json.dump(witness, fh, indent=2, sort_keys=True)
        fh.write("\n")

    print(f"leaf_count={witness['leaf_count']}")
    print(f"root={witness['root']}")
    for k, v in witness["proofs"].items():
        print(f"  {k}: leaf_index={v['leaf_index']} height={v['tree_height']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
