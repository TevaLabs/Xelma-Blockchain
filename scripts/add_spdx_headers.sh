#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
#
# add_spdx_headers.sh — Add SPDX license headers to Soroban contract Rust source files.
#
# Finds all .rs files under contracts/src/, checks whether an SPDX header already
# exists, and prepends one if missing. Safe to run multiple times — headers are
# never duplicated.

set -euo pipefail

HEADER='// SPDX-License-Identifier: MIT'
SEARCH_PATTERN='SPDX-License-Identifier'
TARGET_DIR='contracts/src'

cwd="${BASH_SOURCE[0]%/*}"
repo_root="$(cd "$cwd/.." && pwd)"

added=0
skipped=0
errors=0

while IFS= read -r -d '' file; do
    if head -n 5 "$file" | grep -q "$SEARCH_PATTERN"; then
        skipped=$((skipped + 1))
        continue
    fi

    tmp="$(mktemp)"
    printf '%s\n' "$HEADER" > "$tmp"
    cat "$file" >> "$tmp"
    mv "$tmp" "$file"
    added=$((added + 1))
    echo "  + $file"
done < <(find "$repo_root/$TARGET_DIR" -name '*.rs' -print0)

echo ""
echo "Done — ${added} header(s) added, ${skipped} already present, ${errors} error(s)."
