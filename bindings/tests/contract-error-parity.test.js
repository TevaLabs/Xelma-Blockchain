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
const rustVariants = [];
for (const line of errorsCode.split("\n")) {
  const m = line.match(/^\s*(\w+)\s*=\s*(\d+),?\s*$/);
  if (m) {
    rustVariants.push({ name: m[1], code: parseInt(m[2], 10) });
  }
}

// --- TypeScript: parse the ContractError map ---
const tsMapMatch = bindingsCode.match(/export\s+const\s+ContractError\s*=\s*\{([\s\S]*)\}/);
if (!tsMapMatch) {
  throw new Error("Could not find ContractError map in bindings/src/index.ts");
}

const tsCodes = new Map();
const tsEntryRegex = /^\s*(\d+)\s*:\s*\{message:"[^"]+"\}/gm;
let entry;
while ((entry = tsEntryRegex.exec(tsMapMatch[1])) !== null) {
  tsCodes.set(parseInt(entry[1], 10), entry[2]);
}

const rustCodes = new Map(rustVariants.map(v => [v,code, v.name]));

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

  it("has no error name mismatches", () => {
    const nameMismatches = [];
    for (const [code, name] of rustCodes) {
      const tsName = tsCodes.get(code);
      if (tsName && tsName !== name) {
        nameMismatches.push(`Code ${code}: Rust name is "${name}", TS name is "${tsName}"`);
      }
    }
    expect(nameMismatches, formatMismatches("Name mismatches", nameMismatches)).toEqual([]);
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

  it("has a type-level ContractErrorCode enum in sync with Rust", () => {
    const enumMatch = bindingsCode.match(/export\s+enum\s+ContractErrorCode\s*\{([\s\S]*?)\n\}/);
    expect(enumMatch, "ContractErrorCode enum not found in bindings/src/index.ts").toBeTruthy();

    const enumCodes = new Map();
    const enumEntryRegex = /^\s*(\w+)\s*=\s*(\d+),?\s*$/gm;
    let enumEntry;
    while ((enumEntry = enumEntryRegex.exec(enumMatch[1])) !== null) {
      enumCodes.set(parseInt(enumEntry[2], 10), enumEntry[1]);
    }

    const missingInEnum = [];
    for (const [code, name] of rustCodes) {
      if (enumCodes.get(code) !== name) {
        missingInEnum.push(`${code}: expected ${name}, got ${enumCodes.get(code) ?? "<missing>"}`);
      }
    }
    expect(missingInEnum).toEqual([]);

    const extraInEnum = [];
    for (const [code, name] of enumCodes) {
      if (!rustCodes.has(code)) {
        extraInEnum.push(`${code}: ${name}`);
      }
    }
    expect(extraInEnum).toEqual([]);
  });
});
