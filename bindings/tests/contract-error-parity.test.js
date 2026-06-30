#!/usr/bin/env node
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import assert from "assert";

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
  console.error("Could not find ContractError map in bindings/src/index.ts");
  process.exit(1);
}

const tsCodes = new Map();
const tsEntryRegex = /^\s*(\d+)\s*:\s*\{message:"([^"]+)"\}/gm;
let entry;
while ((entry = tsEntryRegex.exec(tsMapMatch[1])) !== null) {
  tsCodes.set(parseInt(entry[1], 10), entry[2]);
}

const rustCodes = new Map(rustVariants.map(v => [v.code, v.name]));

const missingInTS = [];
const extraInTS = [];
const nameMismatches = [];

for (const [code, name] of rustCodes) {
  const tsName = tsCodes.get(code);
  if (!tsName) {
    missingInTS.push(`${code}: ${name}`);
  } else if (tsName !== name) {
    nameMismatches.push(`Code ${code}: Rust name is "${name}", TS name is "${tsName}"`);
  }
}

for (const [code, name] of tsCodes) {
  if (!rustCodes.has(code)) {
    extraInTS.push(`${code}: ${name}`);
  }
}

assert.strictEqual(missingInTS.length, 0, `Missing error codes in TS: ${missingInTS.join(", ")}`);
assert.strictEqual(extraInTS.length, 0, `Extra error codes in TS (not in Rust): ${extraInTS.join(", ")}`);
assert.strictEqual(nameMismatches.length, 0, `Error name mismatches: ${nameMismatches.join("; ")}`);

assert(
  bindingsCode.includes("export function decodeContractError"),
  "decodeContractError helper is missing from bindings"
);
assert(
  bindingsCode.includes("export function formatContractError"),
  "formatContractError helper is missing from bindings"
);

console.log(`✅ Error code parity check passed: ${rustCodes.size} variants mapped correctly.`);
