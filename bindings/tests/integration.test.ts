import { describe, it, expect, vi } from "vitest";
import {
  mintIfNeeded,
  placeBetChecked,
  claimIfPending,
  simulateBet,
  XelmaError,
  InsufficientBalanceError,
  NoActiveRoundError,
  ContractPausedError,
  AlreadyBetError,
  StakeExceedsMaxError,
  ExposureCapExceededError,
  NoRoundTemplateError,
  wrapContractError,
  type MintResult,
  type ClaimResult,
  type PlaceBetParams,
  type SimulateBetResult,
} from "../src/helpers.js";

// ─── Helpers ───────────────────────────────────────────────────

function mockTx<T>(result: T) {
  return {
    result,
    signAndSend: vi.fn().mockResolvedValue({ result }),
  };
}

function createMockClient() {
  return {
    balance: vi.fn(),
    mint_initial: vi.fn(),
    is_paused: vi.fn(),
    get_active_round: vi.fn(),
    place_bet: vi.fn(),
    get_pending_winnings: vi.fn(),
    claim_winnings: vi.fn(),
  } as any;
}

// ─── mintIfNeeded ──────────────────────────────────────────────

describe("mintIfNeeded", () => {
  it("returns minted=false when balance > 0", async () => {
    const client = createMockClient();
    client.balance.mockResolvedValue(mockTx(50_000_000n));

    const result = await mintIfNeeded(client, "GABC");
    expect(result.minted).toBe(false);
    expect(result.balance).toBe(50_000_000n);
    expect(client.mint_initial).not.toHaveBeenCalled();
  });

  it("mints and returns minted=true when balance is 0", async () => {
    const client = createMockClient();
    client.balance
      .mockResolvedValueOnce(mockTx(0n))
      .mockResolvedValueOnce(mockTx(10_000_000_000n));
    client.mint_initial.mockResolvedValue(mockTx(10_000_000_000n));

    const result = await mintIfNeeded(client, "GABC");
    expect(result.minted).toBe(true);
    expect(result.balance).toBe(10_000_000_000n);
    expect(client.mint_initial).toHaveBeenCalledWith({ user: "GABC" }, undefined);
  });

  it("forwards options to client calls", async () => {
    const client = createMockClient();
    client.balance
      .mockResolvedValueOnce(mockTx(0n))
      .mockResolvedValueOnce(mockTx(10_000_000_000n));
    client.mint_initial.mockResolvedValue(mockTx(10_000_000_000n));

    const opts = { simulate: true } as any;
    await mintIfNeeded(client, "GABC", opts);
    expect(client.balance).toHaveBeenCalledWith({ user: "GABC" }, opts);
    expect(client.mint_initial).toHaveBeenCalledWith({ user: "GABC" }, opts);
  });
});

// ─── placeBetChecked ───────────────────────────────────────────

describe("placeBetChecked", () => {
  const params: PlaceBetParams = {
    user: "GABC",
    amount: 500_000_000n,
    side: { tag: "Up", values: undefined },
  };

  it("places bet when all checks pass", async () => {
    const client = createMockClient();
    client.is_paused.mockResolvedValue(mockTx(false));
    client.get_active_round.mockResolvedValue(mockTx({ round_id: 1n }));
    client.balance.mockResolvedValue(mockTx(1_000_000_000n));
    client.place_bet.mockResolvedValue(mockTx(undefined));

    const result = await placeBetChecked(client, params);
    expect(result).toBeDefined();
    expect(client.place_bet).toHaveBeenCalledWith(params, undefined);
  });

  it("throws ContractPausedError when paused", async () => {
    const client = createMockClient();
    client.is_paused.mockResolvedValue(mockTx(true));

    await expect(placeBetChecked(client, params)).rejects.toThrow(ContractPausedError);
    expect(client.place_bet).not.toHaveBeenCalled();
  });

  it("throws NoActiveRoundError when no active round", async () => {
    const client = createMockClient();
    client.is_paused.mockResolvedValue(mockTx(false));
    client.get_active_round.mockResolvedValue(mockTx(null));

    await expect(placeBetChecked(client, params)).rejects.toThrow(NoActiveRoundError);
    expect(client.place_bet).not.toHaveBeenCalled();
  });

  it("throws InsufficientBalanceError when balance too low", async () => {
    const client = createMockClient();
    client.is_paused.mockResolvedValue(mockTx(false));
    client.get_active_round.mockResolvedValue(mockTx({ round_id: 1n }));
    client.balance.mockResolvedValue(mockTx(100_000_000n));

    await expect(placeBetChecked(client, params)).rejects.toThrow(InsufficientBalanceError);
    expect(client.place_bet).not.toHaveBeenCalled();
  });

  it("maps contract errors from signAndSend to typed exceptions", async () => {
    const client = createMockClient();
    client.is_paused.mockResolvedValue(mockTx(false));
    client.get_active_round.mockResolvedValue(mockTx({ round_id: 1n }));
    client.balance.mockResolvedValue(mockTx(1_000_000_000n));

    const err = new Error("simulation failed: contract error code 10");
    client.place_bet.mockResolvedValue({
      result: undefined,
      signAndSend: vi.fn().mockRejectedValue(err),
    });

    await expect(placeBetChecked(client, params)).rejects.toThrow(AlreadyBetError);
  });
});

// ─── claimIfPending ────────────────────────────────────────────

describe("claimIfPending", () => {
  it("returns claimed=false when no pending winnings", async () => {
    const client = createMockClient();
    client.get_pending_winnings.mockResolvedValue(mockTx(0n));

    const result = await claimIfPending(client, "GABC");
    expect(result.claimed).toBe(false);
    expect(client.claim_winnings).not.toHaveBeenCalled();
  });

  it("claims and returns claimed=true when winnings > 0", async () => {
    const client = createMockClient();
    client.get_pending_winnings.mockResolvedValue(mockTx(2_500_000_000n));
    client.claim_winnings.mockResolvedValue(mockTx(2_500_000_000n));

    const result = await claimIfPending(client, "GABC");
    expect(result.claimed).toBe(true);
    expect(result.amount).toBe(2_500_000_000n);
    expect(client.claim_winnings).toHaveBeenCalledWith({ user: "GABC" }, undefined);
  });

  it("forwards options to client calls", async () => {
    const client = createMockClient();
    client.get_pending_winnings.mockResolvedValue(mockTx(1n));
    client.claim_winnings.mockResolvedValue(mockTx(1n));

    const opts = { simulate: true } as any;
    await claimIfPending(client, "GABC", opts);
    expect(client.get_pending_winnings).toHaveBeenCalledWith({ user: "GABC" }, opts);
    expect(client.claim_winnings).toHaveBeenCalledWith({ user: "GABC" }, opts);
  });
});

// ─── wrapContractError ─────────────────────────────────────────

describe("wrapContractError", () => {
  it("passes through XelmaError instances unchanged", () => {
    const err = new InsufficientBalanceError();
    expect(wrapContractError(err)).toBe(err);
  });

  it("maps numeric code 9 to InsufficientBalanceError", () => {
    const err = wrapContractError({ code: 9, message: "n/a" });
    expect(err).toBeInstanceOf(InsufficientBalanceError);
  });

  it("parses code from error message string", () => {
    const err = wrapContractError(new Error("contract error code 28"));
    expect(err).toBeInstanceOf(StakeExceedsMaxError);
  });

  it("passes through unrecognized errors unchanged", () => {
    const original = new Error("network timeout");
    expect(wrapContractError(original)).toBe(original);
  });

  it("passes through ExposureCapExceededError", () => {
    const err = wrapContractError({ code: 29 });
    expect(err).toBeInstanceOf(ExposureCapExceededError);
  });

  it("passes through NoRoundTemplateError for code 65", () => {
    const err = wrapContractError({ code: 65 });
    expect(err).toBeInstanceOf(NoRoundTemplateError);
  });
});

// ─── simulateBet ───────────────────────────────────────────────

describe("simulateBet", () => {
  it("returns simulated=true and minResourceFee on success", async () => {
    const client = createMockClient();
    client.place_bet.mockResolvedValue({
      simulate: vi.fn().mockResolvedValue({ minResourceFee: "100" }),
    });

    const result = await simulateBet(client, {
      user: "GABC",
      amount: 100n,
      side: { tag: "Up", values: undefined },
    });

    expect(result.simulated).toBe(true);
    expect(result.minResourceFee).toBe("100");
  });

  it("returns simulated=false and wrapped error on simulation failure", async () => {
    const client = createMockClient();
    client.place_bet.mockRejectedValue({ code: 22 });

    const result = await simulateBet(client, {
      user: "GABC",
      amount: 100n,
      side: { tag: "Up", values: undefined },
    });

    expect(result.simulated).toBe(false);
    expect(result.error).toBeInstanceOf(ContractPausedError);
  });
});

// ─── Type exports ──────────────────────────────────────────────

describe("type exports", () => {
  it("MintResult has expected shape", () => {
    const r: MintResult = { minted: true, balance: 0n };
    expect(r.minted).toBe(true);
  });

  it("ClaimResult has expected shape", () => {
    const r: ClaimResult = { claimed: true, amount: 100n };
    expect(r.claimed).toBe(true);
  });

  it("PlaceBetParams has expected shape", () => {
    const p: PlaceBetParams = {
      user: "G",
      amount: 1n,
      side: { tag: "Down", values: undefined },
    };
    expect(p.side.tag).toBe("Down");
  });

  it("SimulateBetResult has expected shape", () => {
    const s: SimulateBetResult = { simulated: true, minResourceFee: "100" };
    expect(s.simulated).toBe(true);
  });
});
