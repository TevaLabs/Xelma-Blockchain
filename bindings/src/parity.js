import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const contractPath = path.resolve(__dirname, '../../contracts/src/contract.rs');
const errorsContractPath = path.resolve(__dirname, '../../contracts/src/errors.rs');
const bindingsPath = path.resolve(__dirname, './index.ts');
const errorDocsPath = path.resolve(__dirname, '../../docs/CONTRACT_ERRORS.md');

if (!fs.existsSync(contractPath) || !fs.existsSync(errorsContractPath) || !fs.existsSync(bindingsPath) || !fs.existsSync(errorDocsPath)) {
    console.error(`Could not find required files.\nContract: ${contractPath}\nErrors: ${errorsContractPath}\nBindings: ${bindingsPath}\nError docs: ${errorDocsPath}`);
    process.exit(1);
}

const contractCode = fs.readFileSync(contractPath, 'utf8');
const errorsContractCode = fs.readFileSync(errorsContractPath, 'utf8');
const bindingsCode = fs.readFileSync(bindingsPath, 'utf8');
const errorDocsCode = fs.readFileSync(errorDocsPath, 'utf8');

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

export function parseRustErrorVariants(errorsContractCode) {
    const errorVariants = {};
    const duplicates = [];
    const codeToName = {};
    const errorEnumSegments = errorsContractCode.split('pub enum ContractError');
    if (errorEnumSegments.length > 1) {
        const enumBlock = errorEnumSegments[1].split('}')[0];
        const lines = enumBlock.split('\n');
        for (const line of lines) {
            const match = line.match(/^\s*([a-zA-Z0-9_]+)\s*=\s*([0-9]+)\s*,?/);
            if (match) {
                const name = match[1];
                const code = parseInt(match[2], 10);
                if (codeToName[code] !== undefined) {
                    duplicates.push(`Duplicate error code ${code} in Rust ContractError: '${codeToName[code]}' and '${name}'.`);
                } else {
                    codeToName[code] = name;
                }
                errorVariants[name] = code;
            }
        }
    }
    return { errorVariants, duplicates };
}

export function parseBindingsErrors(bindingsCode) {
    const bindingsErrors = {};
    const duplicates = [];
    const codeToName = {};
    const lines = bindingsCode.split('\n');
    let insideContractErrorBlock = false;
    for (const line of lines) {
        if (line.includes('export const ContractError = {')) {
            insideContractErrorBlock = true;
            continue;
        }
        if (insideContractErrorBlock) {
            if (line.trim() === '}' || line.trim() === '};') {
                insideContractErrorBlock = false;
                continue;
            }
            const match = line.match(/^\s*([0-9]+)\s*:\s*\{\s*message\s*:\s*["']([a-zA-Z0-9_]+)["']\s*\}/);
            if (match) {
                const code = parseInt(match[1], 10);
                const name = match[2];
                if (codeToName[code] !== undefined) {
                    duplicates.push(`Duplicate error code ${code} in bindings ContractError: '${codeToName[code]}' and '${name}'.`);
                } else {
                    codeToName[code] = name;
                }
                bindingsErrors[name] = code;
            }
        }
    }
    return { bindingsErrors, duplicates };
}

export function parseDocumentedErrors(errorDocsCode) {
    const documentedErrors = {};
    const duplicates = [];
    const codeToName = {};
    for (const line of errorDocsCode.split('\n')) {
        const match = line.match(/^\|\s*([0-9]+)\s*\|\s*`?([a-zA-Z0-9_]+)`?\s*\|/);
        if (match) {
            const code = parseInt(match[1], 10);
            const name = match[2];
            if (codeToName[code] !== undefined) {
                duplicates.push(`Duplicate error code ${code} in docs table: '${codeToName[code]}' and '${name}'.`);
            } else {
                codeToName[code] = name;
            }
            documentedErrors[name] = code;
        }
    }
    return { documentedErrors, duplicates };
}

export function checkErrorEnumParity(errorsContractCode, bindingsCode, errorDocsCode) {
    const { errorVariants, duplicates: rustDuplicates } = parseRustErrorVariants(errorsContractCode);
    const { bindingsErrors, duplicates: tsDuplicates } = parseBindingsErrors(bindingsCode);
    const { documentedErrors, duplicates: docDuplicates } = parseDocumentedErrors(errorDocsCode);

    const errorsDrift = [...rustDuplicates, ...tsDuplicates, ...docDuplicates];

    for (const [name, val] of Object.entries(errorVariants)) {
        if (bindingsErrors[name] === undefined) {
            errorsDrift.push(`Error variant '${name}' (value ${val}) exists in contract but is missing from bindings error map.`);
        } else if (bindingsErrors[name] !== val) {
            errorsDrift.push(`Error variant '${name}' has value ${val} in contract but value ${bindingsErrors[name]} in bindings.`);
        }
    }

    for (const [name, val] of Object.entries(errorVariants)) {
        if (documentedErrors[name] === undefined) {
            errorsDrift.push(`Error variant '${name}' (value ${val}) exists in contract but is missing from docs.`);
        } else if (documentedErrors[name] !== val) {
            errorsDrift.push(`Error variant '${name}' has value ${val} in contract but value ${documentedErrors[name]} in docs.`);
        }
    }
    for (const [name, val] of Object.entries(documentedErrors)) {
        if (errorVariants[name] === undefined) {
            errorsDrift.push(`Error variant '${name}' (value ${val}) exists in docs but is missing from contract.`);
        }
    }
    for (const [name, val] of Object.entries(bindingsErrors)) {
        if (errorVariants[name] === undefined) {
            errorsDrift.push(`Error variant '${name}' (value ${val}) exists in bindings error map but is missing from contract.`);
        }
    }

    return {
        errorVariants,
        bindingsErrors,
        documentedErrors,
        errorsDrift,
    };
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === __filename;

if (isMain) {
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

    const {
        errorVariants,
        bindingsErrors,
        documentedErrors,
        errorsDrift,
    } = checkErrorEnumParity(errorsContractCode, bindingsCode, errorDocsCode);

    if (Object.keys(errorVariants).length === 0) {
        console.error("Failed to parse error variants from:", errorsContractPath);
        process.exit(1);
    }

    if (Object.keys(bindingsErrors).length === 0) {
        console.error("Failed to parse binding errors from:", bindingsPath);
        process.exit(1);
    }

    if (Object.keys(documentedErrors).length === 0) {
        console.error("Failed to parse documented errors from:", errorDocsPath);
        process.exit(1);
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
        console.log("✅ Error enum parity check passed: Rust, TypeScript, and docs are synced.");
    }

    if (failed) {
        console.error("\n\n💡 To resolve parity drift:");
        console.error("  1. For method drift, regenerate/update the fromJSON map in bindings/src/index.ts.");
        console.error("  2. For error drift, update bindings/src/index.ts and docs/CONTRACT_ERRORS.md to match contracts/src/errors.rs.");
        console.error("  3. Run `npm run test:parity` and consult CONTRIBUTING.md before committing.");
        process.exit(1);
    } else {
        process.exit(0);
    }
}
