import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export const defaultPaths = {
    contractPath: path.resolve(__dirname, '../../contracts/src/contract.rs'),
    errorsContractPath: path.resolve(__dirname, '../../contracts/src/errors.rs'),
    bindingsPath: path.resolve(__dirname, './index.ts'),
    errorDocsPath: path.resolve(__dirname, '../../docs/CONTRACT_ERRORS.md'),
};

/**
 * Parse public contract methods inside the `#[contractimpl] impl VirtualTokenContract` block.
 */
export function parseContractFunctions(contractCode) {
    const contractFns = [];
    const contractSegments = contractCode.split('impl VirtualTokenContract');
    if (contractSegments.length > 1) {
        const implBlock = contractSegments[1];
        const lines = implBlock.split('\n');
        for (const line of lines) {
            const match = line.match(/^\s*pub\s+fn\s+([a-zA-Z0-9_]+)\s*\(/);
            const isPubCrate = line.match(/^\s*pub\(crate\)\s+fn/);
            if (match && !isPubCrate && !match[1].startsWith('_')) {
                contractFns.push(match[1]);
            }
        }
    }
    return contractFns;
}

/**
 * Parse bindings for exported methods listed in the `fromJSON` block.
 */
export function parseBindingsFunctions(bindingsCode) {
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
    return bindingsFns;
}

/**
 * Parse `pub enum ContractError` variants and numeric discriminants from `contracts/src/errors.rs`.
 */
export function parseRustContractErrors(errorsContractCode) {
    const errorVariants = {};
    const byCode = new Map();
    const duplicates = [];

    const errorEnumSegments = errorsContractCode.split('pub enum ContractError');
    if (errorEnumSegments.length > 1) {
        const enumBlock = errorEnumSegments[1].split('}')[0];
        const lines = enumBlock.split('\n');
        for (const line of lines) {
            const match = line.match(/^\s*([a-zA-Z0-9_]+)\s*=\s*([0-9]+)\s*,?/);
            if (match) {
                const name = match[1];
                const code = parseInt(match[2], 10);
                if (errorVariants[name] !== undefined) {
                    duplicates.push(
                        `Duplicate Rust ContractError variant '${name}' (values ${errorVariants[name]} and ${code}).`
                    );
                }
                if (byCode.has(code)) {
                    duplicates.push(
                        `Duplicate Rust ContractError code ${code} shared by '${byCode.get(code)}' and '${name}'.`
                    );
                }
                errorVariants[name] = code;
                byCode.set(code, name);
            }
        }
    }

    return { errorVariants, byCode, duplicates };
}

/**
 * Parse `export const ContractError = { ... }` entries from `bindings/src/index.ts`.
 */
export function parseBindingsContractErrors(bindingsCode) {
    const bindingsErrors = {};
    const byCode = new Map();
    const duplicates = [];

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
                const code = parseInt(match[1], 10);
                const name = match[2];
                if (byCode.has(code)) {
                    duplicates.push(
                        `Duplicate bindings ContractError code ${code} shared by '${byCode.get(code)}' and '${name}'.`
                    );
                }
                if (bindingsErrors[name] !== undefined) {
                    duplicates.push(
                        `Duplicate bindings ContractError variant '${name}' (codes ${bindingsErrors[name]} and ${code}).`
                    );
                }
                bindingsErrors[name] = code;
                byCode.set(code, name);
            }
        }
    }

    return { bindingsErrors, byCode, duplicates };
}

/**
 * Parse the canonical documentation table (`| 94 | PageSizeExceeded |`) from `docs/CONTRACT_ERRORS.md`.
 */
export function parseDocumentedContractErrors(errorDocsCode) {
    const documentedErrors = {};
    const byCode = new Map();
    const duplicates = [];

    for (const line of errorDocsCode.split('\n')) {
        const match = line.match(/^\|\s*([0-9]+)\s*\|\s*`?([a-zA-Z0-9_]+)`?\s*\|/);
        if (match) {
            const code = parseInt(match[1], 10);
            const name = match[2];
            if (byCode.has(code)) {
                duplicates.push(
                    `Duplicate documented ContractError code ${code} shared by '${byCode.get(code)}' and '${name}'.`
                );
            }
            if (documentedErrors[name] !== undefined) {
                duplicates.push(
                    `Duplicate documented ContractError variant '${name}' (codes ${documentedErrors[name]} and ${code}).`
                );
            }
            documentedErrors[name] = code;
            byCode.set(code, name);
        }
    }

    return { documentedErrors, byCode, duplicates };
}

/**
 * Validate `ContractError` enum parity across Rust (`contracts/src/errors.rs`),
 * TypeScript bindings (`bindings/src/index.ts`), and docs (`docs/CONTRACT_ERRORS.md`).
 */
export function checkContractErrorParity({ errorsContractCode, bindingsCode, errorDocsCode }) {
    const parseErrors = [];
    const errorsDrift = [];

    const {
        errorVariants,
        byCode: rustByCode,
        duplicates: rustDuplicates,
    } = parseRustContractErrors(errorsContractCode);
    const {
        bindingsErrors,
        byCode: bindingsByCode,
        duplicates: bindingsDuplicates,
    } = parseBindingsContractErrors(bindingsCode);
    const {
        documentedErrors,
        byCode: docsByCode,
        duplicates: docsDuplicates,
    } = parseDocumentedContractErrors(errorDocsCode);

    if (Object.keys(errorVariants).length === 0) {
        parseErrors.push('Failed to parse error variants from contracts/src/errors.rs');
    }
    if (Object.keys(bindingsErrors).length === 0) {
        parseErrors.push('Failed to parse binding errors from bindings/src/index.ts');
    }
    if (errorDocsCode !== undefined && Object.keys(documentedErrors).length === 0) {
        parseErrors.push('Failed to parse documented errors from docs/CONTRACT_ERRORS.md');
    }

    errorsDrift.push(...rustDuplicates, ...bindingsDuplicates, ...docsDuplicates);

    for (const [name, val] of Object.entries(errorVariants)) {
        if (bindingsErrors[name] === undefined) {
            const mappedName = bindingsByCode.get(val);
            if (mappedName !== undefined && mappedName !== name) {
                errorsDrift.push(
                    `Error code ${val} is '${name}' in contract but '${mappedName}' in bindings error map.`
                );
            } else {
                errorsDrift.push(
                    `Error variant '${name}' (value ${val}) exists in contract but is missing from bindings error map.`
                );
            }
        } else if (bindingsErrors[name] !== val) {
            errorsDrift.push(
                `Error variant '${name}' has value ${val} in contract but value ${bindingsErrors[name]} in bindings.`
            );
        }
    }

    for (const [name, val] of Object.entries(bindingsErrors)) {
        if (errorVariants[name] === undefined) {
            const rustName = rustByCode.get(val);
            if (rustName === undefined) {
                errorsDrift.push(
                    `Error variant '${name}' (value ${val}) exists in bindings error map but is missing from contract.`
                );
            }
        }
    }

    if (errorDocsCode !== undefined && Object.keys(documentedErrors).length > 0) {
        for (const [name, val] of Object.entries(errorVariants)) {
            if (documentedErrors[name] === undefined) {
                const docsName = docsByCode.get(val);
                if (docsName !== undefined && docsName !== name) {
                    errorsDrift.push(
                        `Error code ${val} is '${name}' in contract but '${docsName}' in docs.`
                    );
                } else {
                    errorsDrift.push(
                        `Error variant '${name}' (value ${val}) exists in contract but is missing from docs.`
                    );
                }
            } else if (documentedErrors[name] !== val) {
                errorsDrift.push(
                    `Error variant '${name}' has value ${val} in contract but value ${documentedErrors[name]} in docs.`
                );
            }
        }

        for (const [name, val] of Object.entries(documentedErrors)) {
            if (errorVariants[name] === undefined) {
                const rustName = rustByCode.get(val);
                if (rustName === undefined) {
                    errorsDrift.push(
                        `Error variant '${name}' (value ${val}) exists in docs but is missing from contract.`
                    );
                }
            }
        }
    }

    return {
        ok: parseErrors.length === 0 && errorsDrift.length === 0,
        parseErrors,
        errorsDrift,
        errorVariants,
        bindingsErrors,
        documentedErrors,
    };
}

/**
 * Run both ABI method parity and `ContractError` enum parity checks.
 */
export function runParityCheck(paths = defaultPaths) {
    const { contractPath, errorsContractPath, bindingsPath, errorDocsPath } = paths;

    if (
        !fs.existsSync(contractPath) ||
        !fs.existsSync(errorsContractPath) ||
        !fs.existsSync(bindingsPath) ||
        !fs.existsSync(errorDocsPath)
    ) {
        console.error(
            `Could not find required files.\nContract: ${contractPath}\nErrors: ${errorsContractPath}\nBindings: ${bindingsPath}\nError docs: ${errorDocsPath}`
        );
        return false;
    }

    const contractCode = fs.readFileSync(contractPath, 'utf8');
    const errorsContractCode = fs.readFileSync(errorsContractPath, 'utf8');
    const bindingsCode = fs.readFileSync(bindingsPath, 'utf8');
    const errorDocsCode = fs.readFileSync(errorDocsPath, 'utf8');

    const contractFns = parseContractFunctions(contractCode);
    const bindingsFns = parseBindingsFunctions(bindingsCode);

    if (contractFns.length === 0) {
        console.error('Failed to parse contract functions from:', contractPath);
        return false;
    }

    if (bindingsFns.length === 0) {
        console.error('Failed to parse binding functions from:', bindingsPath);
        return false;
    }

    const missingInBindings = contractFns.filter((fn) => !bindingsFns.includes(fn));
    const missingInContract = bindingsFns.filter((fn) => !contractFns.includes(fn));

    const errorParity = checkContractErrorParity({
        errorsContractCode,
        bindingsCode,
        errorDocsCode,
    });

    if (errorParity.parseErrors.length > 0) {
        errorParity.parseErrors.forEach((msg) => console.error(msg));
        return false;
    }

    let failed = false;

    if (missingInBindings.length > 0 || missingInContract.length > 0) {
        console.error('❌ ABI parity check failed: Drift detected');
        failed = true;

        if (missingInBindings.length > 0) {
            console.error('- The following methods are present in the contract but missing from the bindings map:');
            missingInBindings.forEach((fn) => console.error(`  - ${fn}`));
        }

        if (missingInContract.length > 0) {
            console.error('- The following methods are in the bindings map but missing from the contract:');
            missingInContract.forEach((fn) => console.error(`  - ${fn}`));
        }
    } else {
        console.log('✅ ABI parity check passed: All contract methods are synced with TS bindings.');
    }

    if (errorParity.errorsDrift.length > 0) {
        console.error('❌ Error enum parity check failed: Drift detected');
        failed = true;
        errorParity.errorsDrift.forEach((err) => console.error(`  - ${err}`));
    } else {
        console.log('✅ Error enum parity check passed: Rust, TypeScript, and docs are synced.');
    }

    if (failed) {
        console.error('\n\n💡 To resolve parity drift:');
        console.error('  1. For method drift, regenerate/update the fromJSON map in bindings/src/index.ts.');
        console.error('  2. For error drift, update bindings/src/index.ts and docs/CONTRACT_ERRORS.md to match contracts/src/errors.rs.');
        console.error('  3. Run `npm run test:parity` and consult CONTRIBUTING.md before committing.');
        return false;
    }

    return true;
}

const isDirectExecution =
    process.argv[1] && path.resolve(process.argv[1]) === path.resolve(__filename);

if (isDirectExecution) {
    const ok = runParityCheck();
    if (!ok) {
        process.exit(1);
    } else {
        process.exit(0);
    }
}
