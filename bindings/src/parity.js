import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const contractPath = path.resolve(__dirname, '../../contracts/src/contract.rs');
const errorsContractPath = path.resolve(__dirname, '../../contracts/src/errors.rs');
const bindingsPath = path.resolve(__dirname, './index.ts');

if (!fs.existsSync(contractPath) || !fs.existsSync(errorsContractPath) || !fs.existsSync(bindingsPath)) {
    console.error(`Could not find required files.\nContract: ${contractPath}\nErrors: ${errorsContractPath}\nBindings: ${bindingsPath}`);
    process.exit(1);
}

const contractCode = fs.readFileSync(contractPath, 'utf8');
const errorsContractCode = fs.readFileSync(errorsContractPath, 'utf8');
const bindingsCode = fs.readFileSync(bindingsPath, 'utf8');

// Parse contract exports inside `impl VirtualTokenContract`
const contractFns = [];
const contractSegments = contractCode.split('impl VirtualTokenContract');
if (contractSegments.length > 1) {
    const implBlock = contractSegments[1];
    const lines = implBlock.split('\n');
    for (const line of lines) {
        const match = line.match(/^\s*pub\s+fn\s+([a-zA-Z0-9_]+)\s*\(/);
        const isPubCrate = line.match(/^\s*pub\(crate\)\s+fn/);
        if (match && !isPubCrate) {
            contractFns.push(match[1]);
        }
    }
}

// Parse bindings for exported methods listed in `fromJSON` block
const bindingsFns = [];
const bindingsSegments = bindingsCode.split('public readonly fromJSON = {');
if (bindingsSegments.length > 1) {
    const fromJsonBlock = bindingsSegments[1].split('}')[0];
    const lines = fromJsonBlock.split('\n');
    for (const line of lines) {
        const match = line.match(/(?:^\s*|\s+)([a-zA-Z0-9_]+)\s*:\s*this\.txFromJSON/);
        if (match) {
            bindingsFns.push(match[1]);
        }
    }
}

if (contractFns.length === 0) {
    console.error("Failed to parse contract functions from:", contractPath);
    process.exit(1);
}

if (bindingsFns.length === 0) {
    console.error("Failed to parse binding functions from:", bindingsPath);
    process.exit(1);
}

const missingInBindings = contractFns.filter(fn => !bindingsFns.includes(fn));
const missingInContract = bindingsFns.filter(fn => !contractFns.includes(fn));

// Parse contract error variants inside `pub enum ContractError`
const errorVariants = {};
const errorEnumSegments = errorsContractCode.split('pub enum ContractError');
if (errorEnumSegments.length > 1) {
    const enumBlock = errorEnumSegments[1].split('}')[0];
    const lines = enumBlock.split('\n');
    for (const line of lines) {
        const match = line.match(/^\s*([a-zA-Z0-9_]+)\s*=\s*([0-9]+)\s*,/);
        if (match) {
            errorVariants[match[1]] = parseInt(match[2], 10);
        }
    }
}

// Parse bindings for ContractError object mapping
const bindingsErrors = {};
const lines = bindingsCode.split('\n');
let insideContractErrorBlock = false;
for (const line of lines) {
    if (line.includes('export const ContractError = {')) {
        insideContractErrorBlock = true;
        continue;
    }
    if (insideContractErrorBlock) {
        if (line.trim() === '}') {
            insideContractErrorBlock = false;
            continue;
        }
        const match = line.match(/^\s*([0-9]+)\s*:\s*\{\s*message\s*:\s*["']([a-zA-Z0-9_]+)["']\s*\}/);
        if (match) {
            bindingsErrors[match[2]] = parseInt(match[1], 10);
        }
    }
}

if (Object.keys(errorVariants).length === 0) {
    console.error("Failed to parse error variants from:", errorsContractPath);
    process.exit(1);
}

if (Object.keys(bindingsErrors).length === 0) {
    console.error("Failed to parse binding errors from:", bindingsPath);
    process.exit(1);
}

// Build a reverse map: code → [rust variant names] to detect duplicate discriminants
const variantsByCode = {};
for (const [name, val] of Object.entries(errorVariants)) {
    if (!variantsByCode[val]) variantsByCode[val] = [];
    variantsByCode[val].push(name);
}

// Also build a reverse map from bindings: code → ts variant name
const bindingsByCode = {};
for (const [name, val] of Object.entries(bindingsErrors)) {
    if (!bindingsByCode[val]) bindingsByCode[val] = [];
    bindingsByCode[val].push(name);
}

const errorsDrift = [];
const handledDuplicateCodes = new Set();

for (const [name, val] of Object.entries(errorVariants)) {
    // For duplicate discriminants, check if at least one variant for this code exists in TS
    if (variantsByCode[val].length > 1) {
        if (handledDuplicateCodes.has(val)) continue;
        handledDuplicateCodes.add(val);

        const anyMatch = variantsByCode[val].some(n => bindingsErrors[n] === val);
        if (!anyMatch) {
            errorsDrift.push(`Duplicate discriminant ${val} (${variantsByCode[val].join(', ')}): no matching variant found in bindings error map.`);
        }
        continue;
    }

    // Normal single-variant check
    if (bindingsErrors[name] === undefined) {
        errorsDrift.push(`Error variant '${name}' (value ${val}) exists in contract but is missing from bindings error map.`);
    } else if (bindingsErrors[name] !== val) {
        errorsDrift.push(`Error variant '${name}' has value ${val} in contract but value ${bindingsErrors[name]} in bindings.`);
    }
}
for (const [name, val] of Object.entries(bindingsErrors)) {
    if (errorVariants[name] === undefined) {
        errorsDrift.push(`Error variant '${name}' (value ${val}) exists in bindings error map but is missing from contract.`);
    }
}

let failed = false;

if (missingInBindings.length > 0 || missingInContract.length > 0) {
    console.error("❌ ABI parity check failed: Drift detected");
    failed = true;

    if (missingInBindings.length > 0) {
        console.error("- The following methods are present in the contract but missing from the bindings map:");
        missingInBindings.forEach(fn => console.error(`  - ${fn}`));
    }

    if (missingInContract.length > 0) {
        console.error("- The following methods are in the bindings map but missing from the contract:");
        missingInContract.forEach(fn => console.error(`  - ${fn}`));
    }
} else {
    console.log("✅ ABI parity check passed: All contract methods are synced with TS bindings.");
}

if (errorsDrift.length > 0) {
    console.error("❌ Error enum parity check failed: Drift detected");
    failed = true;
    errorsDrift.forEach(err => console.error(`  - ${err}`));
} else {
    console.log("✅ Error enum parity check passed: All contract error variants are synced with TS bindings.");
}

if (failed) {
    console.error("\n💡 To resolve parity drift:");
    console.error("  1. Update bindings/src/index.ts and bindings/src/helpers.ts to include missing methods and error codes.");
    console.error("  2. Run `npm run test:parity` to verify alignment.");
    console.error("  3. Consult CONTRIBUTING.md for contract and SDK binding release guidelines.");
    process.exit(1);
} else {
    process.exit(0);
}
