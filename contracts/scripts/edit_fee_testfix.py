#!/usr/bin/env python3
"""Repair protocol-fee tests: drop redundant mid-file `use` line, swap
`oracle_payload(&env, ...)` helper call for inline `OraclePayload` struct
literal, and replace `cancel_protocol_fee_change()` with the actual public
method `cancel_config_change(&ConfigChangeKind::ProtocolFeeBps)`.

Operates on contracts/src/tests/resolution.rs and
contracts/src/tests/config_timelock.rs (Issue #162).
"""
import sys, re

RES = '/workspaces/Xelma-Blockchain/contracts/src/tests/resolution.rs'
CFG = '/workspaces/Xelma-Blockchain/contracts/src/tests/config_timelock.rs'


def fix_resolution(src):
    # Drop the redundant mid-file use line.
    BAD_USE = "use soroban_sdk::{symbol_short, BytesN, Vec};\n"
    if BAD_USE not in src:
        print('[resolution.rs] mid-file use already absent')
    else:
        src = src.replace(BAD_USE, '')
        print('[resolution.rs] removed mid-file `use`')

    # Map oracle_payload(&env, &contract_id, PRICE, ROUND, NONCE) to an
    # inline OraclePayload struct literal matching the existing tests.
    pat = re.compile(
        r'oracle_payload\(&env,\s*&contract_id,\s*([0-9_]+),\s*([0-9]+),\s*([0-9]+)\)'
    )

    def repl(m):
        price, round_id, nonce = m.group(1), m.group(2), m.group(3)
        return (
            'OraclePayload {\n'
            f'                price: {price},\n'
            '                timestamp: env.ledger().timestamp(),\n'
            f'                round_id: {round_id},\n'
            '                nonce: 0u64,'
        )
    # The above returns prefix of the literal; we need to close the brace
    # and add the trailing fields plus a closing brace. Use a different
    # approach: replace the WHOLE call site with a complete literal.
    def repl2(m):
        price, round_id, nonce = m.group(1), m.group(2), m.group(3)
        return (
            'OraclePayload {\n'
            f'                    price: {price},\n'
            '                    timestamp: env.ledger().timestamp(),\n'
            f'                    round_id: {round_id}u32,\n'
            f'                    nonce: {nonce}u64,\n'
            '                    network_id: env.ledger().network_id(),\n'
            '                    contract_addr: contract_id.clone(),\n'
            '                }'
        )
    new = pat.sub(repl2, src)
    calls_before = len(pat.findall(src))
    calls_after = len(pat.findall(new))
    src = new
    print(f'[resolution.rs] replaced oracle_payload call sites: '
          f'{calls_before} -> {calls_after}')

    # Adjust indentation depth: the test bodies have arbitrary indent because
    # of the original `client.resolve_round(&oracle_payload(...))` form. The
    # inline literal is multi-line and may have inconsistent indent. The plain
    # 20-space indent above works for the surrounding resolve_round call site.
    return src


def fix_config_timelock(src):
    BAD_USE = "use soroban_sdk::{symbol_short};\n"
    if BAD_USE in src:
        # symbol_short already imported at top of config_timelock.rs; safe to
        # drop the redundant local one (we don't use BytesN/Vec here).
        src = src.replace(BAD_USE, '')
        print('[config_timelock.rs] removed redundant `use soroban_sdk::{symbol_short};`')

    # Replace `cancel_protocol_fee_change()` with the actual public method.
    bad = 'client.cancel_protocol_fee_change();\n'
    good = ('client.cancel_config_change(&crate::types::ConfigChangeKind::ProtocolFeeBps);\n')
    if bad in src:
        src = src.replace(bad, good)
        print('[config_timelock.rs] replaced cancel_protocol_fee_change() -> cancel_config_change(...)')
    else:
        print('[config_timelock.rs] cancel_protocol_fee_change() call not found')

    # RoundPin: get_pending_config_change signature -- it returns `Option<PendingConfigChange>`. .unwrap() will panic on None, expected for our test which scheduled.
    return src


new_res = fix_resolution(open(RES).read())
open(RES, 'w').write(new_res)
print(f'resolution.rs: {len(open(RES).read().splitlines())} lines total')

new_cfg = fix_config_timelock(open(CFG).read())
open(CFG, 'w').write(new_cfg)
print(f'config_timelock.rs: {len(open(CFG).read().splitlines())} lines total')
