import { describe, it, expect } from 'vitest';
import { decodeAccessPolicy, XelmaError } from '../src/helpers.js';

describe('decodeAccessPolicy (#841)', () => {
  it('open mode: Open and Allowlisted can bet, Denylisted cannot', () => {
    expect(decodeAccessPolicy([false, 0])).toEqual({ allowlistMode: false, state: 'Open', canBet: true });
    expect(decodeAccessPolicy([false, 1]).canBet).toBe(true);
    expect(decodeAccessPolicy([false, 2])).toEqual({ allowlistMode: false, state: 'Denylisted', canBet: false });
  });

  it('allowlist mode: only Allowlisted can bet', () => {
    expect(decodeAccessPolicy([true, 0]).canBet).toBe(false);
    expect(decodeAccessPolicy([true, 1]).canBet).toBe(true);
    expect(decodeAccessPolicy([true, 2]).canBet).toBe(false);
  });

  it('accepts string and tagged enum shapes', () => {
    expect(decodeAccessPolicy([true, 'Allowlisted']).state).toBe('Allowlisted');
    expect(decodeAccessPolicy([false, { tag: 'Denylisted' }]).canBet).toBe(false);
  });

  it('rejects unknown states', () => {
    expect(() => decodeAccessPolicy([false, 7])).toThrow(XelmaError);
    expect(() => decodeAccessPolicy([false, 'Banned' as never])).toThrow(XelmaError);
  });
});
