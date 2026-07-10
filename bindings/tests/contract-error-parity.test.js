#!/usr/bin/env node
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import { describe, it, expect } from "vitest";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const errorsPath = path.resolve(__dirname, "../../contracts/src/errors.rs");
const bindingsPath = path.resolve(__dirname, "../src/index.ts");

const errorsCode = fs.readFileSync(errorsPath, "utf8");
const bindingsCode = fs.readFileSync(bindingsPath, "utf8");

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

describe("Contract Error Parity", () => {
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

  it("contains decodeContractError helper", () => {
    expect(bindingsCode.includes("export function decodeContractError")).toBe(true);
  });

  it("contains formatContractError helper", () => {
    expect(bindingsCode.includes("export function formatContractError")).toBe(true);
  });
});

