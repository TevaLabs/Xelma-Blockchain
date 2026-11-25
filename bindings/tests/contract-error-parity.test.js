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
const tsEntries = [];
const tsEntryRegex = /^\s*(\d+)\s*:\s*\{message:"([^"]+)"\}/gm;
let entry;
while ((entry = tsEntryRegex.exec(tsMapMatch[1])) !== null) {
  const code = parseInt(entry[1], 10);
  tsEntries.push({ name: entry[2], code });
  tsCodes.set(code, entry[2]);
}

const rustCodes = new Map(rustVariants.map(v => [v.code, v.name]));

// --- Docs: parse the markdown registry table ---
const docsEntries = [];
const docsCodes = new Map();
for (const line of docsCode.split("\n")) {
  const match = line.match(/^\|\s*(\d+)\s*\|\s*`?(\w+)`?\s*\|/);
  if (match) {
    const code = parseInt(match[1], 10);
    docsEntries.push({ name: match[2], code });
    docsCodes.set(code, match[2]);
  }
}

// Maps keyed by code collapse duplicates silently, so collisions are
// detected on the raw entry lists (Issue #861).
function findDuplicates(entries, key) {
  const seen = new Map();
  for (const e of entries) {
    const k = e[key];
    if (!seen.has(k)) seen.set(k, []);
    seen.get(k).push(key === "code" ? e.name : e.code);
  }
  return [...seen.entries()]
    .filter(([, values]) => values.length > 1)
    .map(([k, values]) => `${key} ${k} used by ${values.join(", ")}`);
}

const sources = [
  ["Rust (contracts/src/errors.rs)", rustVariants],
  ["TypeScript (bindings/src/index.ts)", tsEntries],
  ["Docs (docs/CONTRACT_ERRORS.md)", docsEntries],
];

function formatMismatches(label, mismatches) {
  if (mismatches.length === 0) return "";
  return `${label}:\n${mismatches.map(m => `  - ${m}`).join("\n")}\n\nFix by updating the appropriate source and re-running this test.\n`;
}

describe("Contract Error Parity", () => {
  it("parses every source", () => {
    expect(rustVariants.length).toBeGreaterThan(0);
    expect(tsEntries.length).toBe(rustVariants.length);
    expect(docsEntries.length).toBe(rustVariants.length);
    expect(tsEntries.every(e => typeof e.name === "string" && e.name.length > 0)).toBe(true);
  });

  for (const [label, entries] of sources) {
    it(`has unique error codes in ${label}`, () => {
      const dups = findDuplicates(entries, "code");
      expect(dups, formatMismatches(`Duplicate codes in ${label}`, dups)).toEqual([]);
    });

    it(`has unique variant names in ${label}`, () => {
      const dups = findDuplicates(entries, "name");
      expect(dups, formatMismatches(`Duplicate names in ${label}`, dups)).toEqual([]);
    });
  }

  it("detects colliding codes (self-check)", () => {
    const sample = [
      { name: "A", code: 1 },
      { name: "B", code: 1 },
      { name: "C", code: 2 },
      { name: "C", code: 3 },
    ];
    expect(findDuplicates(sample, "code")).toEqual(["code 1 used by A, B"]);
    expect(findDuplicates(sample, "name")).toEqual(["name C used by 2, 3"]);
  });

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
});
