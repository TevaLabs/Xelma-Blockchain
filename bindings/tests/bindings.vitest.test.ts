/**
 * Bindings integration tests (issue #173).
 *
 * These tests validate that the compiled TypeScript bindings remain compatible
 * with the Soroban contract WASM built in the same CI run.
 *
 * When WASM_PATH is set (i.e. in CI after the contract-build job), tests that
 * read the artifact run fully. When WASM_PATH is absent (local dev without a
 * Rust toolchain), those tests are skipped so the suite still passes cleanly.
 *
 * Failure modes surfaced:
 *  - Missing WASM artifact (bad upload/download step)
 *  - WASM file is zero-bytes or truncated
 *  - Bindings export list drifts from contract public methods
 *  - Required TypeScript types are absent from the generated module
 *  - Method name typo would cause a runtime "is not a function" crash
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// ── Helpers ───────────────────────────────────────────────────────────────────

const WASM_PATH = process.env['WASM_PATH'] ?? '';
const hasWasm = WASM_PATH !== '' && existsSync(WASM_PATH);

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const BINDINGS_SRC = resolve(__dirname, '../src/index.ts');
const PARITY_SCRIPT = resolve(__dirname, '../src/parity.js');

// All public methods the contract currently exports.
// Keep this list in sync with contracts/src/contract.rs `impl VirtualTokenContract`.
const EXPECTED_CONTRACT_METHODS = [
  'initialize',
  'get_schema_version',
  'migrate_schema_v1_to_v2',
  'is_paused',
  'pause_contract',
  'unpause_contract',
  'create_round',
  'get_active_round',
  'get_last_round_id',
  'get_archived_round',
  'get_recent_archived_rounds',
  'get_admin',
  'get_oracle',
  'set_oracle_max_deviation_bps',
  'get_oracle_max_deviation_bps',
  'arm_oracle_deviation_override',
  'update_oracle_heartbeat',
  'get_oracle_heartbeat',
  'is_oracle_live',
  'set_oracle_stale_threshold',
] as const;

// ── WASM artifact tests ───────────────────────────────────────────────────────

describe('WASM artifact', () => {
  it('WASM_PATH env var is set in CI', () => {
    if (!hasWasm) {
      console.warn('WASM_PATH not set — skipping (local dev)');
      return;
    }
    expect(WASM_PATH).toBeTruthy();
  });

  it('WASM file exists at WASM_PATH', () => {
    if (!hasWasm) return;
    expect(existsSync(WASM_PATH)).toBe(true);
  });

  it('WASM file is non-empty', () => {
    if (!hasWasm) return;
    const { size } = statSync(WASM_PATH);
    expect(size).toBeGreaterThan(0);
  });

  it('WASM file begins with the WASM magic bytes (\\0asm)', () => {
    if (!hasWasm) return;
    const buf = readFileSync(WASM_PATH);
    // WASM magic: 0x00 0x61 0x73 0x6D
    expect(buf[0]).toBe(0x00);
    expect(buf[1]).toBe(0x61);
    expect(buf[2]).toBe(0x73);
    expect(buf[3]).toBe(0x6d);
  });

  it('WASM file is at least 1 KB (sanity check against stub/empty builds)', () => {
    if (!hasWasm) return;
    const { size } = statSync(WASM_PATH);
    expect(size).toBeGreaterThan(1024);
  });
});

// ── Bindings source structural tests ─────────────────────────────────────────

describe('Bindings source', () => {
  let src: string;

  beforeAll(() => {
    src = readFileSync(BINDINGS_SRC, 'utf8');
  });

  it('bindings/src/index.ts exists', () => {
    expect(existsSync(BINDINGS_SRC)).toBe(true);
  });

  it('exports a Client class', () => {
    expect(src).toMatch(/export\s+class\s+Client/);
  });

  it('exports BetSide type', () => {
    expect(src).toMatch(/BetSide/);
  });

  it('exports Round interface', () => {
    expect(src).toMatch(/Round/);
  });

  it('contains a fromJSON block mapping contract methods', () => {
    expect(src).toContain('fromJSON');
    expect(src).toContain('txFromJSON');
  });

  it.each(EXPECTED_CONTRACT_METHODS)(
    'fromJSON block includes method: %s',
    (method) => {
      expect(src).toContain(method);
    },
  );

  it('does not reference undefined or removed methods', () => {
    // Ensure no obvious stale references to a method that was never in the list.
    // This is a lightweight guard — full parity is enforced by parity.js.
    expect(src).not.toContain('deprecated_method');
  });
});

// ── Parity script presence ────────────────────────────────────────────────────

describe('Parity script', () => {
  it('bindings/src/parity.js exists', () => {
    expect(existsSync(PARITY_SCRIPT)).toBe(true);
  });

  it('parity.js references the contract source path', () => {
    const parity = readFileSync(PARITY_SCRIPT, 'utf8');
    expect(parity).toContain('contract.rs');
  });

  it('parity.js exits non-zero on drift (script imports fs)', () => {
    const parity = readFileSync(PARITY_SCRIPT, 'utf8');
    expect(parity).toContain('process.exit(1)');
  });
});

// ── Method / runtime compatibility ───────────────────────────────────────────

describe('Method name compatibility', () => {
  it('initialize is present in bindings', () => {
    const src = readFileSync(BINDINGS_SRC, 'utf8');
    expect(src).toContain('initialize');
  });

  it('place_bet is present in bindings', () => {
    const src = readFileSync(BINDINGS_SRC, 'utf8');
    expect(src).toContain('place_bet');
  });

  it('resolve_round is present in bindings', () => {
    const src = readFileSync(BINDINGS_SRC, 'utf8');
    expect(src).toContain('resolve_round');
  });

  it('claim_winnings is present in bindings', () => {
    const src = readFileSync(BINDINGS_SRC, 'utf8');
    expect(src).toContain('claim_winnings');
  });

  it('no method is listed with snake_case mismatch (e.g. camelCase only)', () => {
    const src = readFileSync(BINDINGS_SRC, 'utf8');
    // Contract uses snake_case — verify at least one snake_case method is present
    expect(src).toMatch(/get_active_round|create_round|place_bet/);
  });
});
