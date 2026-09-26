import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const bindingsRoot = resolve(fileURLToPath(new URL("..", import.meta.url)));
const repositoryRoot = resolve(bindingsRoot, "..");
const wasmPath = resolve(
  process.env.WASM_PATH ?? join(repositoryRoot, "target/wasm32v1-none/release/xelma_contract.wasm"),
);
const checkedInPath = join(bindingsRoot, "src", "index.ts");
const errorsPath = join(repositoryRoot, "contracts", "src", "errors.rs");
const checkOnly = process.argv.includes("--check");
const wasmArgumentIndex = process.argv.indexOf("--wasm");
const requestedWasm = wasmArgumentIndex >= 0 ? process.argv[wasmArgumentIndex + 1] : wasmPath;
const inputWasmPath = resolve(repositoryRoot, requestedWasm);

function fail(message) {
  console.error(`Bindings generation failed: ${message}`);
  process.exit(1);
}

if (!existsSync(inputWasmPath)) {
  fail(`WASM artifact not found at ${inputWasmPath}. Build the contract first or set WASM_PATH.`);
}

if (!existsSync(checkedInPath)) {
  fail(`checked-in generated file not found at ${checkedInPath}.`);
}

if (!existsSync(errorsPath)) {
  fail(`contract error source not found at ${errorsPath}.`);
}

const errorEntries = [...readFileSync(errorsPath, "utf8").matchAll(/^\s*([A-Za-z0-9_]+)\s*=\s*(\d+)\s*,/gm)]
  .map(([, name, code]) => ({ name, code }));

if (errorEntries.length === 0) {
  fail(`no contract error variants found in ${errorsPath}.`);
}

const errorSupport = `

/** Stable contract error codes retained alongside the generated ABI client. */
export const ContractError = {
${errorEntries.map(({ name, code }) => `  ${code}: {message:"${name}"},`).join("\n")}
};

export function decodeContractError(code: number): { code: number; variant: string; message: string } | undefined {
  const entry = ContractError[code as keyof typeof ContractError];
  return entry ? { code, variant: entry.message, message: entry.message } : undefined;
}

export function formatContractError(code: number): string | undefined {
  const decoded = decodeContractError(code);
  return decoded ? \`${"${decoded.variant}"} (code ${"${decoded.code}"})\` : undefined;
}

export function ContractErrorDecoder(code: number): string {
  return formatContractError(code) ?? \`Unknown contract error (code ${"${code}"})\`;
}
`;

const temporaryRoot = mkdtempSync(join(tmpdir(), "xelma-bindings-"));
const outputDir = join(temporaryRoot, "generated-bindings");
mkdirSync(outputDir, { recursive: true });

try {
  const command = process.platform === "win32" ? "stellar.exe" : "stellar";
  const result = spawnSync(
    command,
    [
      "contract",
      "bindings",
      "typescript",
      "--wasm",
      inputWasmPath,
      "--output-dir",
      outputDir,
      "--overwrite",
    ],
    { encoding: "utf8", stdio: "inherit", shell: false },
  );

  if (result.error) {
    const installHint = process.platform === "win32"
      ? `Install the Stellar CLI and ensure it is on PATH (for example, %USERPROFILE%\\.stellar\\bin).`
      : "Install the Stellar CLI and ensure it is on PATH.";
    fail(`${result.error.message}. ${installHint}`);
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }

  const generatedPath = join(outputDir, "src", "index.ts");
  if (!existsSync(generatedPath)) {
    fail(`Stellar CLI did not produce ${generatedPath}.`);
  }

  const generated = `${readFileSync(generatedPath, "utf8").replaceAll("\r\n", "\n")}${errorSupport}`;
  const checkedIn = readFileSync(checkedInPath, "utf8").replaceAll("\r\n", "\n");

  if (checkOnly) {
    if (generated !== checkedIn) {
      console.error(`Generated bindings are out of date: ${checkedInPath}`);
      console.error("Run `npm run generate -- --wasm <path-to-contract.wasm>` and commit the result.");
      process.exit(1);
    }
    console.log("Generated bindings are up to date.");
    process.exit(0);
  }

  writeFileSync(checkedInPath, generated);
  console.log(`Generated ${checkedInPath}`);
} finally {
  rmSync(temporaryRoot, { recursive: true, force: true });
}
