/**
 * Issue #865 — Vitest WASM simulation for `cash_out_early`.
 *
 * The bindings test suite previously validated the WASM artifact only for magic
 * bytes and export names. This suite exercises the early cash-out entrypoint from
 * three angles:
 *
 *  1. **ABI level** — parse the contract spec out of the CI-built WASM and assert
 *     `cash_out_early` exists with the documented `user: Address` input. Invocation
 *     arguments are encoded with the SDK's spec encoder so the naming and type of
 *     the argument are checked against the artifact, not against a copied list.
 *
 *  2. **Semantics level** — run a deterministic reference simulation of the
 *     contract's cash-out rules (see `contracts/src/betting.rs::cash_out_early`)
 *     over the fee, phase, mode, and position guards, asserting the conservation
 *     invariants:
 *         cashout + forfeit == stake
 *         pool_delta == -stake
 *         pending_delta == cashout
 *         treasury_delta == forfeit
 *
 *  3. **Bindings parity** — assert the generated TypeScript client exposes the same
 *     single-argument surface the WASM spec declares.
 *
 * A Soroban host is not available in the Node test environment, so the contract
 * itself cannot execute here; `scripts/e2e_smoke.sh` covers real on-chain
 * execution. This suite is the fast, deterministic guard that the ABI and the
 * cash-out accounting rules in the bindings stay aligned with the built WASM.
 *
 * When `WASM_PATH` is unset (local dev without a Rust build) the WASM/spec tests
 * are skipped and the pure accounting tests still run.
 */

import { describe, it, expect, beforeAll } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Address, Keypair, contract, xdr } from "@stellar/stellar-sdk";

const WASM_PATH = process.env["WASM_PATH"] ?? "";
const hasWasm = WASM_PATH !== "" && existsSync(WASM_PATH);
const describeWasm = (name: string, fn: () => void) =>
  hasWasm ? describe(name, fn) : describe.skip(name, fn);

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const BINDINGS_SRC = resolve(__dirname, "../src/index.ts");
const ERRORS_RS = resolve(__dirname, "../../contracts/src/errors.rs");
const MATH_COMMON_RS = resolve(__dirname, "../../contracts/src/math_common.rs");

const USER = Keypair.random().publicKey();
const OTHER = Keypair.random().publicKey();

// ── Contract error codes (mirrors contracts/src/errors.rs) ────────────────────

const ERRORS = {
  AdminNotSet: 2,
  InvalidBetAmount: 6,
  NoActiveRound: 7,
  ContractPaused: 22,
  EarlyCashoutDisabled: 95,
  PositionNotFound: 96,
  InvalidPhaseForCashout: 97,
  WrongModeForCashout: 98,
} as const;

const BPS_DENOMINATOR = 10_000n;

// ── ABI level: contract spec parsed from the CI-built WASM ────────────────────

describeWasm("WASM contract spec — cash_out_early", () => {
  let spec: contract.Spec;

  beforeAll(async () => {
    const wasm = readFileSync(WASM_PATH);
    // Fail fast and clearly if the artifact is not a real Soroban contract.
    expect(Buffer.from(wasm.subarray(0, 4)).toString("hex")).toBe("0061736d");
    // `await` tolerates both the sync and async forms of `Spec.fromWasm`.
    spec = await contract.Spec.fromWasm(wasm);
  });

  it("is declared as a contract function in the spec", () => {
    const names = spec.funcs().map((fn) => fn.name().toString());
    expect(names).toContain("cash_out_early");
  });

  it("declares exactly one input named `user`, typed as Address", () => {
    const fn = spec.getFunc("cash_out_early");
    const inputs = fn.inputs();

    expect(inputs.map((input) => input.name().toString())).toEqual(["user"]);
    expect(inputs[0].type().switch().value).toBe(
      xdr.ScSpecType.scSpecTypeAddress().value,
    );
  });

  it("encodes invocation arguments from the WASM spec and round-trips the address", () => {
    const scArgs = spec.funcArgsToScVals("cash_out_early", { user: USER });

    expect(scArgs).toHaveLength(1);
    expect(scArgs[0].switch().value).toBe(xdr.ScValType.scvAddress().value);
    expect(Address.fromScVal(scArgs[0]).toString()).toBe(USER);
  });

  it("rejects an invocation missing the required `user` argument", () => {
    expect(() => spec.funcArgsToScVals("cash_out_early", {})).toThrow(
      /Missing field user/,
    );
  });

  it("rejects a malformed address before submission", () => {
    expect(() =>
      spec.funcArgsToScVals("cash_out_early", { user: "not-an-address" }),
    ).toThrow();
  });
});

// ── Semantics level: deterministic reference simulation ───────────────────────

/**
 * A snapshot of the contract state that `cash_out_early` reads or mutates.
 * Field names mirror the contract's storage: UpDown positions live in
 * `pool_up` / `pool_down`, payouts accrue to pending winnings, and the forfeit
 * is collected into the protocol fee treasury.
 */
interface CashOutWorld {
  paused: boolean;
  activeRound: boolean;
  penaltyBps: number | null;
  /** 0 = UpDown, 1 = Precision */
  mode: 0 | 1;
  currentLedger: number;
  betEndLedger: number;
  endLedger: number;
  position: { stake: bigint; side: 0 | 1 } | null;
  participants: string[];
  poolUp: bigint;
  poolDown: bigint;
  pendingWinnings: bigint;
  treasury: bigint;
}

type Simulation =
  | { ok: true; cashout: bigint; forfeit: bigint; world: CashOutWorld }
  | { ok: false; error: number };

/**
 * Reference implementation of `contracts/src/betting.rs::cash_out_early`.
 *
 * Guard order matches the contract:
 *   paused → penalty config → active round → mode → phase → position → stake
 *
 * Then the accounting:
 *   forfeit = floor(stake * penalty_bps / 10_000)
 *   cashout = stake - forfeit
 *   pool[side] -= stake (the FULL stake, not the cashout)
 *   pending_winnings += cashout
 *   treasury += forfeit
 *   position and participant entry removed
 */
function simulateCashOutEarly(world: CashOutWorld, user: string): Simulation {
  if (world.paused) return { ok: false, error: ERRORS.ContractPaused };

  const penaltyBps = world.penaltyBps;
  if (
    penaltyBps === null ||
    penaltyBps === 0 ||
    penaltyBps > Number(BPS_DENOMINATOR)
  ) {
    return { ok: false, error: ERRORS.EarlyCashoutDisabled };
  }

  if (!world.activeRound) return { ok: false, error: ERRORS.NoActiveRound };
  if (world.mode !== 0) return { ok: false, error: ERRORS.WrongModeForCashout };

  if (
    world.currentLedger < world.betEndLedger ||
    world.currentLedger >= world.endLedger
  ) {
    return { ok: false, error: ERRORS.InvalidPhaseForCashout };
  }

  const position = world.position;
  if (position === null) return { ok: false, error: ERRORS.PositionNotFound };
  if (position.stake <= 0n) return { ok: false, error: ERRORS.InvalidBetAmount };

  const stake = position.stake;
  const forfeit = (stake * BigInt(penaltyBps)) / BPS_DENOMINATOR;
  const cashout = stake - forfeit;

  const next: CashOutWorld = {
    ...world,
    position: null,
    participants: world.participants.filter((participant) => participant !== user),
    poolUp:
      position.side === 0 ? world.poolUp - stake : world.poolUp,
    poolDown:
      position.side === 1 ? world.poolDown - stake : world.poolDown,
    pendingWinnings: world.pendingWinnings + cashout,
    treasury: world.treasury + forfeit,
  };

  return { ok: true, cashout, forfeit, world: next };
}

function baseWorld(overrides: Partial<CashOutWorld> = {}): CashOutWorld {
  return {
    paused: false,
    activeRound: true,
    penaltyBps: 1_000, // 10 %
    mode: 0,
    currentLedger: 1_050,
    betEndLedger: 1_000,
    endLedger: 1_100,
    position: { stake: 1_000_000000n, side: 0 },
    participants: [USER, OTHER],
    poolUp: 3_000_000000n,
    poolDown: 2_000_000000n,
    pendingWinnings: 0n,
    treasury: 0n,
    ...overrides,
  };
}

describe("cash_out_early reference simulation", () => {
  it("pays stake minus the penalty and moves the full stake out of the pool", () => {
    const result = simulateCashOutEarly(baseWorld(), USER);

    expect(result.ok).toBe(true);
    if (!result.ok) return;

    expect(result.forfeit).toBe(100_000000n); // 10 % of 1_000_000000
    expect(result.cashout).toBe(900_000000n);
    expect(result.world.poolUp).toBe(2_900_000000n);
    expect(result.world.poolDown).toBe(2_000_000000n); // untouched
    expect(result.world.pendingWinnings).toBe(900_000000n);
    expect(result.world.treasury).toBe(100_000000n);
    expect(result.world.position).toBeNull();
    expect(result.world.participants).not.toContain(USER);
    expect(result.world.participants).toContain(OTHER);
  });

  it("deducts a Down position from pool_down only", () => {
    const result = simulateCashOutEarly(
      baseWorld({
        penaltyBps: 250, // 2.5 %
        position: { stake: 500_000000n, side: 1 },
      }),
      USER,
    );

    expect(result.ok).toBe(true);
    if (!result.ok) return;

    expect(result.forfeit).toBe(12_500_000n);
    expect(result.cashout).toBe(487_500_000n);
    expect(result.world.poolUp).toBe(3_000_000000n);
    expect(result.world.poolDown).toBe(1_500_000000n);
  });

  it("floors the forfeit, so tiny stakes exit at a full refund without dust", () => {
    const result = simulateCashOutEarly(
      baseWorld({ penaltyBps: 7, position: { stake: 100n, side: 0 } }),
      USER,
    );

    expect(result.ok).toBe(true);
    if (!result.ok) return;

    // floor(100 * 7 / 10_000) == 0
    expect(result.forfeit).toBe(0n);
    expect(result.cashout).toBe(100n);
    expect(result.world.treasury).toBe(0n);
    expect(result.world.pendingWinnings).toBe(100n);
    expect(result.world.poolUp).toBe(2_999_999_900n);
  });

  it("supports a 100 % penalty (zero cashout, full forfeit to treasury)", () => {
    const result = simulateCashOutEarly(
      baseWorld({ penaltyBps: 10_000 }),
      USER,
    );

    expect(result.ok).toBe(true);
    if (!result.ok) return;

    expect(result.forfeit).toBe(1_000_000000n);
    expect(result.cashout).toBe(0n);
    expect(result.world.pendingWinnings).toBe(0n);
    expect(result.world.treasury).toBe(1_000_000000n);
  });

  it("removes the position so a second call cannot double-pay", () => {
    const first = simulateCashOutEarly(baseWorld(), USER);
    expect(first.ok).toBe(true);
    if (!first.ok) return;

    const second = simulateCashOutEarly(first.world, USER);
    expect(second.ok).toBe(false);
    if (second.ok) return;
    expect(second.error).toBe(ERRORS.PositionNotFound);
  });

  const disabledCases: Array<[string, number | null]> = [
    ["disabled (unset)", null],
    ["disabled (zero bps)", 0],
    ["invalid (above 10 000 bps)", 10_001],
  ];

  it.each(disabledCases)("rejects when early cash-out is %s", (_label, penaltyBps) => {
    const result = simulateCashOutEarly(baseWorld({ penaltyBps }), USER);

    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBe(ERRORS.EarlyCashoutDisabled);
  });

  it("rejects Precision rounds", () => {
    const result = simulateCashOutEarly(baseWorld({ mode: 1 }), USER);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBe(ERRORS.WrongModeForCashout);
  });

  it("rejects when there is no active round", () => {
    const result = simulateCashOutEarly(baseWorld({ activeRound: false }), USER);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBe(ERRORS.NoActiveRound);
  });

  it("rejects while the contract is paused", () => {
    const result = simulateCashOutEarly(baseWorld({ paused: true }), USER);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBe(ERRORS.ContractPaused);
  });

  const wrongPhaseCases: Array<[string, number]> = [
    ["one ledger before betting closes", 999],
    ["at the round end ledger", 1_100],
    ["after the round ends", 1_250],
  ];

  it.each(wrongPhaseCases)("rejects in the wrong phase: %s", (_label, currentLedger) => {
    const result = simulateCashOutEarly(baseWorld({ currentLedger }), USER);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBe(ERRORS.InvalidPhaseForCashout);
  });

  const runningPhaseCases: Array<[string, number]> = [
    ["at bet_end_ledger", 1_000],
    ["one ledger before round end", 1_099],
  ];

  it.each(runningPhaseCases)("accepts the Running phase boundary: %s", (_label, currentLedger) => {
    const result = simulateCashOutEarly(baseWorld({ currentLedger }), USER);
    expect(result.ok).toBe(true);
  });

  it("rejects a user with no position", () => {
    const result = simulateCashOutEarly(
      baseWorld({ position: null, participants: [OTHER] }),
      USER,
    );

    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBe(ERRORS.PositionNotFound);
  });

  it("rejects a non-positive stake", () => {
    const result = simulateCashOutEarly(
      baseWorld({ position: { stake: 0n, side: 0 } }),
      USER,
    );
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toBe(ERRORS.InvalidBetAmount);
  });

  it("conserves value across stake × penalty combinations", () => {
    const stakes = [
      1n,
      7n,
      99n,
      100n,
      1_000_000000n,
      12_345_678_901n,
    ];
    const penalties = [1, 7, 50, 999, 1_000, 5_000, 9_999, 10_000];

    for (const stake of stakes) {
      for (const penaltyBps of penalties) {
        const before = baseWorld({
          penaltyBps,
          position: { stake, side: 0 },
        });
        const result = simulateCashOutEarly(before, USER);

        expect(result.ok, `stake=${stake} bps=${penaltyBps}`).toBe(true);
        if (!result.ok) continue;

        // 1. Every stroop of the stake is accounted for.
        expect(result.cashout + result.forfeit).toBe(stake);
        expect(result.forfeit).toBe((stake * BigInt(penaltyBps)) / BPS_DENOMINATOR);

        // 2. The full stake leaves the pool; nothing else moves.
        expect(before.poolUp - result.world.poolUp).toBe(stake);
        expect(result.world.poolDown).toBe(before.poolDown);

        // 3. Payouts and forfeits land in their respective buckets.
        expect(result.world.pendingWinnings - before.pendingWinnings).toBe(
          result.cashout,
        );
        expect(result.world.treasury - before.treasury).toBe(result.forfeit);

        // 4. The position is closed exactly once.
        expect(result.world.position).toBeNull();
        expect(result.world.participants).toEqual([OTHER]);
      }
    }
  });
});

// ── Bindings parity: the generated client matches the WASM ABI ────────────────

describe("bindings cash_out_early surface", () => {
  const src = readFileSync(BINDINGS_SRC, "utf8");

  it("mirrors the contract's BPS denominator and cash-out error codes", () => {
    // Guard the reference simulation above against contract drift: the constants
    // it hardcodes must still match the Rust sources it models.
    const math = readFileSync(MATH_COMMON_RS, "utf8");
    expect(math).toMatch(/BPS_DENOMINATOR\s*:\s*i128\s*=\s*10_000/);

    const errors = readFileSync(ERRORS_RS, "utf8");
    for (const [variant, code] of Object.entries(ERRORS)) {
      expect(errors).toMatch(new RegExp(`${variant}\\s*=\\s*${code}\\b`));
    }
  });

  it("exposes cash_out_early in the generated Client", () => {
    expect(src).toMatch(/cash_out_early\s*:/);
  });

  it("uses the same single `user` argument as the WASM spec", () => {
    expect(src).toMatch(
      /cash_out_early:\s*\(\{\s*user\s*\}\s*:\s*\{\s*user:\s*string\s*\}/,
    );
  });

  it("returns a Result-bearing AssembledTransaction", () => {
    const declaration = src
      .split("\n")
      .find((line) => line.includes("cash_out_early:"));

    expect(declaration).toBeDefined();
    expect(declaration).toContain("AssembledTransaction<Result<void>>");
  });
});
