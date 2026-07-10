// scripts/check-doc-drift.js
/**
 * Checks that the Wallet Error Integration Guide stays in sync with the contract error enum.
 * Exits with code 1 if any contract error is missing from the markdown table.
 */
const fs = require('fs');
const path = require('path');

const ERRORS_RS = path.join(__dirname, '..', 'contracts', 'src', 'errors.rs');
const GUIDE_MD = path.join(__dirname, '..', 'docs', 'WALLET_ERROR_GUIDE.md');

function parseEnum(fileContent) {
  const enumMap = new Map();
  const regex = /\s*(\w+)\s*=\s*(\d+),/g; // matches "Identifier = number," inside enum
  let match;
  while ((match = regex.exec(fileContent)) !== null) {
    const [, identifier, num] = match;
    enumMap.set(parseInt(num, 10), identifier);
  }
  return enumMap;
}

function parseGuide(fileContent) {
  const guideSet = new Set();
  const tableRegex = /\|\s*`?0x[0-9a-fA-F]+`?\s*\|\s*(\d+)\s*\|/g; // capture decimal column
  let match;
  while ((match = tableRegex.exec(fileContent)) !== null) {
    const [, dec] = match;
    guideSet.add(parseInt(dec, 10));
  }
  return guideSet;
}

function main() {
  if (!fs.existsSync(ERRORS_RS) || !fs.existsSync(GUIDE_MD)) {
    console.error('Required files not found.');
    process.exit(1);
  }
  const enumContent = fs.readFileSync(ERRORS_RS, 'utf8');
  const guideContent = fs.readFileSync(GUIDE_MD, 'utf8');
  const enumMap = parseEnum(enumContent);
  const guideSet = parseGuide(guideContent);

  const missing = [];
  for (const code of enumMap.keys()) {
    if (!guideSet.has(code)) missing.push({code, name: enumMap.get(code)});
  }
  if (missing.length > 0) {
    console.error('Documentation drift detected: the following error codes are missing from WALLET_ERROR_GUIDE.md');
    missing.forEach(e => console.error(` - ${e.code} (${e.name})`));
    process.exit(1);
  }
  console.log('Documentation drift check passed.');
  process.exit(0);
}

main();
