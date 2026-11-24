/**
 * RoundPhase decoding for wallets (issue #866).
 *
 * Pins the user-facing strings and flags returned by `decodeRoundPhase` and
 * checks the mapping stays in step with the contract's `RoundPhase` enum.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { RoundPhase } from '../src/index.js';
import {
  ROUND_PHASE_LABELS,
  decodeRoundPhase,
  parseRoundPhase,
  roundPhaseLabel,
} from '../src/helpers.js';

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const CONTRACT_TYPES = resolve(__dirname, '../../contracts/src/types.rs');

describe('ROUND_PHASE_LABELS', () => {
  it('covers every generated RoundPhase variant', () => {
    const variants = Object.values(RoundPhase).filter((v) => typeof v === 'number');
    expect(Object.keys(ROUND_PHASE_LABELS).map(Number).sort()).toEqual(
      (variants as number[]).sort(),
    );
  });

  it('matches the contract enum discriminants in types.rs', () => {
    const src = readFileSync(CONTRACT_TYPES, 'utf8');
    const block = src.match(/pub enum RoundPhase \{([^}]*)\}/);
    expect(block).not.toBeNull();
    const rust = Object.fromEntries(
      [...block![1].matchAll(/(\w+)\s*=\s*(\d+)/g)].map((m) => [m[1], Number(m[2])]),
    );
    const ts = Object.fromEntries(
      Object.entries(ROUND_PHASE_LABELS).map(([n, v]) => [v.key, Number(n)]),
    );
    expect(ts).toEqual(rust);
  });

  it('gives every phase a non-empty, distinct label', () => {
    const labels = Object.values(ROUND_PHASE_LABELS).map((v) => v.label);
    labels.forEach((l) => expect(l.trim().length).toBeGreaterThan(0));
    expect(new Set(labels).size).toBe(labels.length);
  });

  it('is frozen', () => {
    expect(Object.isFrozen(ROUND_PHASE_LABELS)).toBe(true);
  });
});

describe('decodeRoundPhase', () => {
  it('decodes Betting', () => {
    expect(decodeRoundPhase(RoundPhase.Betting)).toEqual({
      phase: RoundPhase.Betting,
      key: 'Betting',
      label: 'Betting open',
      description: 'Bets are open for this round.',
      canBet: true,
      canCashOut: false,
      awaitingResolution: false,
    });
  });

  it('decodes Running', () => {
    const info = decodeRoundPhase(RoundPhase.Running);
    expect(info.key).toBe('Running');
    expect(info.label).toBe('Round in progress');
    expect(info).toMatchObject({ canBet: false, canCashOut: true, awaitingResolution: false });
  });

  it('decodes Resolvable', () => {
    const info = decodeRoundPhase(RoundPhase.Resolvable);
    expect(info.key).toBe('Resolvable');
    expect(info.label).toBe('Awaiting result');
    expect(info).toMatchObject({ canBet: false, canCashOut: false, awaitingResolution: true });
  });

  it('exactly one capability flag is set for each known phase', () => {
    for (const p of [RoundPhase.Betting, RoundPhase.Running, RoundPhase.Resolvable]) {
      const { canBet, canCashOut, awaitingResolution } = decodeRoundPhase(p);
      expect([canBet, canCashOut, awaitingResolution].filter(Boolean)).toHaveLength(1);
    }
  });

  it.each([0, 4, 99, -1, 1.5, NaN, null, undefined, {}, 'Settled', '', 2n ** 40n])(
    'decodes %s as Unknown without throwing',
    (v) => {
      const info = decodeRoundPhase(v);
      expect(info.phase).toBeUndefined();
      expect(info.key).toBe('Unknown');
      expect(info.label).toBe('Unknown phase');
      expect(info).toMatchObject({ canBet: false, canCashOut: false, awaitingResolution: false });
    },
  );
});

describe('parseRoundPhase', () => {
  it.each([
    [1, RoundPhase.Betting],
    [2n, RoundPhase.Running],
    ['3', RoundPhase.Resolvable],
    [' 2 ', RoundPhase.Running],
    ['Betting', RoundPhase.Betting],
    ['running', RoundPhase.Running],
    ['RESOLVABLE', RoundPhase.Resolvable],
  ])('parses %s', (input, expected) => {
    expect(parseRoundPhase(input)).toBe(expected);
  });

  it('rejects non-phase values', () => {
    expect(parseRoundPhase('Betting open')).toBeUndefined();
    expect(parseRoundPhase(0)).toBeUndefined();
    expect(parseRoundPhase(-1n)).toBeUndefined();
  });
});

describe('roundPhaseLabel', () => {
  it('returns the label', () => {
    expect(roundPhaseLabel(RoundPhase.Betting)).toBe('Betting open');
    expect(roundPhaseLabel('2')).toBe('Round in progress');
    expect(roundPhaseLabel(42)).toBe('Unknown phase');
  });
});
