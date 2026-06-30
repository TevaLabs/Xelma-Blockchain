/**
 * Integration test: validate WASM exports match the TypeScript bindings method list.
 *
 * In CI this test receives the path to the same-commit-built WASM artifact via
 * the WASM_PATH environment variable. Without WASM_PATH the test is skipped.
 */
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import { describe, it, expect, beforeAll } from "vitest";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const WASM_PATH = process.env.WASM_PATH;

// ─── helpers ──────────────────────────────────────────────────────────────────

function parseParity() {
  const contractPath = path.resolve(__dirname, "../../contracts/src/contract.rs");
  const code = fs.readFileSync(contractPath, "utf8");
  const fns = [];
  const segments = code.split("impl VirtualTokenContract");
  if (segments.length > 1) {
    for (const line of segments[1].split("\n")) {
      const m = line.match(/^\s*pub\s+fn\s+([a-zA-Z0-9_]+)\s*\(/);
      const isCrate = line.match(/^\s*pub\(crate\)\s+fn/);
      if (m && !isCrate) fns.push(m[1]);
    }
  }
  return fns;
}

// ─── tests ────────────────────────────────────────────────────────────────────

describe("WASM parity", () => {
  let wasmExports = null;
  let expectedFns = [];

  beforeAll(async () => {
    expectedFns = parseParity();
    if (!WASM_PATH) return; // skip WASM checks below when not in CI

    const wasmBytes = fs.readFileSync(WASM_PATH);

    // Validate magic bytes — fail fast with a clear message.
    const magic = Buffer.from(wasmBytes.slice(0, 4)).toString("hex");
    expect(magic, `${WASM_PATH} does not start with the WASM magic bytes (\\0asm). Got: ${magic}`).toBe(
      "0061736d"
    );

    const module = await WebAssembly.compile(wasmBytes);
    wasmExports = WebAssembly.Module.exports(module);
  });

  it("contract.rs has at least one exported public fn", () => {
    expect(expectedFns.length).toBeGreaterThan(0);
  });

  it("WASM is present and valid when WASM_PATH is set", () => {
    if (!WASM_PATH) {
      console.log("WASM_PATH not set — skipping WASM validation (run in CI for full check)");
      return;
    }
    expect(wasmExports).not.toBeNull();
    expect(wasmExports.length).toBeGreaterThan(0);
  });

  it("all contract public fns are exported from the WASM", () => {
    if (!WASM_PATH) {
      console.log("WASM_PATH not set — skipping WASM validation (run in CI for full check)");
      return;
    }

    const wasmFnExports = new Set(
      wasmExports.filter((e) => e.kind === "function").map((e) => e.name)
    );

    const missing = [];
    for (const fn of expectedFns) {
      // Soroban contract fns are mangled; look for the fn name as a substring of any export.
      const found = [...wasmFnExports].some((name) => name === fn || name.endsWith(`_${fn}`));
      if (!found) missing.push(fn);
    }

    if (missing.length > 0) {
      throw new Error(
        `WASM is missing bindings for the following contract methods:\n` +
          missing.map((fn) => `  - ${fn}`).join("\n") +
          `\n\nPresent WASM exports (functions):\n` +
          [...wasmFnExports].map((n) => `  - ${n}`).join("\n")
      );
    }
  });
});
