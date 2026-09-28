import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import { describe, it, expect } from "vitest";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const errorsPath = path.resolve(__dirname, "../../contracts/src/errors.rs");
const bindingsPath = path.resolve(__dirname, "../src/index.ts");
const docsPath = path.resolve(__dirname, "../../docs/CONTRACT_ERRORS.md");

const errorsCode = fs.readFileSync(errorsPath, "utf8");
const bindingsCode = fs.readFileSync(bindingsPath, "utf8");
const docsCode = fs.readFileSync(docsPath, "utf8");

const rustVariants = [];
for (const line of errorsCode.split("\n")) {
  const m = line.match(/^\s*(\w+)\s*=\s*(\d+),?\s*$/);
  if (m) {
    rustVariants.push({ name: m[1], code: parseInt(m[2], 10) });
  }
}

const tsMapMatch = bindingsCode.match(/export\s+const\s+ContractError\s*=\s*\{([\s\S]*)\}/);
if (!tsMapMatch) {
  throw new Error("Could not find ContractError map in bindings/src/index.ts");
}

const tsCodes = new Map();
const tsEntryRegex = /^\s*(\d+)\s*:\s*\{message:"([^"]+)"\}/gm;
let entry;
while ((entry = tsEntryRegex.exec(tsMapMatch[1])) !== null) {
  tsCodes.set(parseInt(entry[1], 10), entry[2]);
}

const rustCodes = new Map(rustVariants.map(v => [v.code, v.name]));
const docsCodes = new Map();
for (const line of docsCode.split("\n")) {
  const match = line.match(/^\|\s*(\d+)\s*\|\s*`?(\w+)`?\s*\|/);
  if (match) {
    docsCodes.set(parseInt(match[1], 10), match[2]);
  }
}

describe("Contract Error Parity", () => {
  it("maps AccessDenied to its stable contract code", () => {
    expect(rustCodes.get(79)).toBe("AccessDenied");
    expect(tsCodes.get(79)).toBe("AccessDenied");
  });

  it("has no missing error codes in TS", () => {
    const missingInTS = [];
    for (const [code, name] of rustCodes) {
      const tsName = tsCodes.get(code);
      if (!tsName) {
        missingInTS.push(`${code}: ${name}`);
      }
    }
    expect(missingInTS).toEqual([]);
  });

  it("has no extra error codes in TS", () => {
    const extraInTS = [];
    for (const [code, name] of tsCodes) {
      if (!rustCodes.has(code)) {
        extraInTS.push(`${code}: ${name}`);
      }
    }
    expect(extraInTS).toEqual([]);
  });

  it("has no error name mismatches", () => {
    const nameMismatches = [];
    for (const [code, name] of rustCodes) {
      const tsName = tsCodes.get(code);
      if (tsName && tsName !== name) {
        nameMismatches.push(`Code ${code}: Rust name is "${name}", TS name is "${tsName}"`);
      }
    }
    expect(nameMismatches).toEqual([]);
  });

  it("keeps the documented registry identical to Rust", () => {
    const byCode = ([left], [right]) => left - right;
    expect([...docsCodes.entries()].sort(byCode)).toEqual(
      [...rustCodes.entries()].sort(byCode)
    );
  });

  it("contains decodeContractError helper", () => {
    expect(bindingsCode.includes("export function decodeContractError")).toBe(true);
  });

  it("contains formatContractError helper", () => {
    expect(bindingsCode.includes("export function formatContractError")).toBe(true);
  });
});
