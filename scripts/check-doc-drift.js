// scripts/check-doc-drift.js
/**
 * Checks that docs/WALLET_ERROR_GUIDE.md stays in sync with the contract error
 * enum (contracts/src/errors.rs), in both directions:
 *
 *  - every enum variant has a row in the guide,
 *  - every guide row names a variant that still exists in the enum,
 *  - the documented hex/decimal columns agree with the enum value, and
 *  - the documented identifier matches the enum identifier for that value.
 *
 * Exits with code 1 on any drift so CI fails before a stale guide ships.
 */
const fs = require('fs');
const path = require('path');

const ERRORS_RS = path.join(__dirname, '..', 'contracts', 'src', 'errors.rs');
const GUIDE_MD = path.join(__dirname, '..', 'docs', 'WALLET_ERROR_GUIDE.md');

function parseEnum(fileContent) {
  const enumMap = new Map();
  const regex = /\s*([A-Za-z0-9_]+)\s*=\s*(\d+),/g; // matches "Identifier = number," inside enum
  let match;
  while ((match = regex.exec(fileContent)) !== null) {
    const [, identifier, num] = match;
    enumMap.set(parseInt(num, 10), identifier);
  }
  return enumMap;
}

/**
 * Parses guide rows shaped `| `0x16` | 22 | `ContractPaused` | ... |`.
 * Returns Map<decimal, { identifier, hex }>.
 */
function parseGuide(fileContent) {
  const rows = new Map();
  const tableRegex =
    /\|\s*`?0x([0-9a-fA-F]+)`?\s*\|\s*(\d+)\s*\|\s*`?([A-Za-z0-9_]+)`?\s*\|/g;
  let match;
  while ((match = tableRegex.exec(fileContent)) !== null) {
    const [, hex, dec, identifier] = match;
    rows.set(parseInt(dec, 10), {
      identifier,
      hex: parseInt(hex, 16),
    });
  }
  return rows;
}

function main() {
  if (!fs.existsSync(ERRORS_RS) || !fs.existsSync(GUIDE_MD)) {
    console.error('Required files not found.');
    process.exit(1);
  }
  const enumContent = fs.readFileSync(ERRORS_RS, 'utf8');
  const guideContent = fs.readFileSync(GUIDE_MD, 'utf8');
  const enumMap = parseEnum(enumContent);
  const guideRows = parseGuide(guideContent);

  const missing = [];
  for (const [code, name] of enumMap) {
    if (!guideRows.has(code)) missing.push({ code, name });
  }

  const stale = [];
  const mismatched = [];
  for (const [code, row] of guideRows) {
    const enumName = enumMap.get(code);
    if (enumName === undefined) {
      stale.push({ code, name: row.identifier });
      continue;
    }
    if (row.hex !== code) {
      mismatched.push(`code ${code} (${row.identifier}) lists hex 0x${row.hex.toString(16)}`);
    }
    if (enumName !== row.identifier) {
      mismatched.push(`code ${code} is ${enumName} in errors.rs but ${row.identifier} in the guide`);
    }
  }

  if (missing.length > 0) {
    console.error('Documentation drift detected: the following error codes are missing from WALLET_ERROR_GUIDE.md');
    missing.forEach((e) => console.error(` - ${e.code} (${e.name})`));
  }
  if (stale.length > 0) {
    console.error('Stale documentation detected: WALLET_ERROR_GUIDE.md documents codes the contract no longer declares');
    stale.forEach((e) => console.error(` - ${e.code} (${e.name})`));
  }
  if (mismatched.length > 0) {
    console.error('Inconsistent documentation detected: WALLET_ERROR_GUIDE.md columns disagree with contracts/src/errors.rs');
    mismatched.forEach((m) => console.error(` - ${m}`));
  }

  if (missing.length + stale.length + mismatched.length > 0) {
    process.exit(1);
  }

  console.log(
    `Documentation drift check passed (${enumMap.size} variants documented in ${path.relative(process.cwd(), GUIDE_MD)}).`
  );
  process.exit(0);
}

main();
