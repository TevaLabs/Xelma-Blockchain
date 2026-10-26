import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import { describe, it, expect } from "vitest";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const errorsPath = path.resolve(__dirname, "../../contracts/src/errors.rs");
const bindingsPath = path.resolve(__dirname, "../src/index.ts");
const docsPath = path.resolve(__dirname, "../../docs/CONTRACT_ERRORS.md");

function readIfFound(p) {
  if (!fs.existsSync(p)) {
    throw new Error(`Expected file not found: ${p} (needed for error enum parity CI)`);
  }
  return fs.readFileSync(p, "utf8");
}

const errorsCode = readIfFound(errorsPath);
const bindingsCode = readIfFound(bindingsPath);
const docsCode = readIfFound(docsPath);

// --- Rust: parse enum variants and their explicit codes ---
const rustEnumMatch = errorsCode.match(/enum\s+ContractError\s*\{([\s\S]*?)^\}/m);
if (!rustEnumMatch) {
  throw new Error("Could not find ContractError enum in contracts/src/errors.rs");
}

const rustCodes = new Map();
for (const line of rustEnumMatch[1].split("\n")) {
  const match = line.match(/^\s*(\w+)\s*=\s*(\d+),?\s*$/);
  if (match) {
    rustCodes.set(Number(match[2]), match[1]);
  }
}

// --- TypeScript: parse the ContractError map ---
const tsMapMatch = bindingsCode.match(/export\s+const\s+ContractError\s*=\s*\{([\s\S]*?)^\}/m);
if (!tsMapMatch) {
  throw new Error("Could not find ContractError map in bindings/src/index.ts");
}

const tsCodes = new Map();
const tsEntryRegex = /^\s*(\d+)\s*:\s*\{message:"([^"]+)"\}/gm;
let entry;
while ((entry = tsEntryRegex.exec(tsMapMatch[1])) !== null) {
  tsCodes.set(Number(entry[1]), entry[2]);
}

// --- Docs: parse the markdown registry table ---
const docsCodes = new Map();
for (const line of docsCode.split("\n")) {
  const match = line.match(/^\|\s*(\d+)\s*\|\s*`?(\w+)`?\s*\|/);
  if (match) {
    docsCodes.set(parseInt(match[1], 10), match[2]);
  }
}

function formatMismatches(label, mismatches) {
  if (mismatches.length === 0) return "";
  return `${label}:\n${mismatches.map(m => `  - ${m}`).join("\n")}\n\nFix by updating the appropriate source and re-running this test.\n`;
}

describe("Contract Error Parity", () => {
  it("has no missing error codes in TS", () => {
    const missingInTS = [];
    for (const [code, name] of rustCodes) {
      if (!tsCodes.has(code)) {
        missingInTS.push(`${code}: ${name}`);
      }
    }
    expect(missingInTS, formatMismatches("Missing in TypeScript", missingInTS)).toEqual([]);
  });

  it("has no extra error codes in TS", () => {
    const extraInTS = [];
    for (const [code, name] of tsCodes) {
      if (!rustCodes.has(code)) {
        extraInTS.push(`${code}: ${name}`);
      }
    }
    expect(extraInTS, formatMismatches("Extra in TypeScript", extraInTS)).toEqual([]);
  });

  it("maps every public error code to its Rust variant message", () => {
    const messageMismatches = [];
    for (const [code, name] of rustCodes) {
      const message = tsCodes.get(code);
      if (message && message !== name) {
        messageMismatches.push(`Code ${code}: Rust variant is "${name}", TypeScript message is "${message}"`);
      }
    }
    expect(messageMismatches, formatMismatches("Variant/message mismatches", messageMismatches)).toEqual([]);
  });

  it("keeps dispute errors mapped to their public codes and messages", () => {
    const disputeErrors = [
      [91, "DisputeWindowExpired"],
      [92, "ClaimLocked"],
    ];
    const mismatches = [];
    for (const [code, expected] of disputeErrors) {
      if (rustCodes.get(code) !== expected) {
        mismatches.push(`Rust code ${code}: expected ${expected}, found ${rustCodes.get(code) ?? "missing"}`);
      }
      if (tsCodes.get(code) !== expected) {
        mismatches.push(`TypeScript code ${code}: expected message ${expected}, found ${tsCodes.get(code) ?? "missing"}`);
      }
    }
    expect(mismatches, formatMismatches("Dispute error parity failures", mismatches)).toEqual([]);
  });

  it("keeps the documented registry identical to Rust", () => {
    const byCode = ([left], [right]) => left - right;
    const docsSorted = [...docsCodes.entries()].sort(byCode);
    const rustSorted = [...rustCodes.entries()].sort(byCode);

    const mismatches = [];
    const max = Math.max(docsSorted.length, rustSorted.length);
    for (let i = 0; i < max; i++) {
      const d = docsSorted[i];
      const r = rustSorted[i];
      if (!d) {
        mismatches.push(`Code ${r[0]} (${r[1]}) missing from docs`);
      } else if (!r) {
        mismatches.push(`Code ${d[0]} (${d[1]}) extra in docs`);
      } else if (d[0] !== r[0] || d[1] !== r[1]) {
        mismatches.push(`Code ${r[0]}: Rust name is "${r[1]}", docs name is "${d[1]}"`);
      }
    }

    expect(mismatches, formatMismatches("Docs vs Rust mismatches", mismatches)).toEqual([]);
  });

  it("contains decodeContractError helper", () => {
    expect(bindingsCode.includes("export function decodeContractError")).toBe(true);
  });

  it("contains formatContractError helper", () => {
    expect(bindingsCode.includes("export function formatContractError")).toBe(true);
  });
});
