#!/usr/bin/env python3
"""Protocol Specification Drift Checker

Ensures that all public contract entrypoints (pub fn) and error identifiers
defined in the Rust sources are present in PROTOCOL_SPEC.md wrapped in backticks.
The script exits with status 1 if any token is missing, printing a clear list.
"""
import re, sys, os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
CONTRACT_RS = os.path.join(ROOT, "contracts", "src", "contract.rs")
ERRORS_RS = os.path.join(ROOT, "contracts", "src", "errors.rs")
SPEC_MD = os.path.join(ROOT, "PROTOCOL_SPEC.md")

def extract_functions(path):
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    # capture public function names
    return set(re.findall(r"pub\s+fn\s+([a-zA-Z0-9_]+)", content))

def extract_error_ids(path):
    ids = set()
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            m = re.search(r"^\s*([A-Za-z0-9_]+)\s*=\s*(\d+),", line)
            if m:
                ident = m.group(1)
                num = int(m.group(2))
                ids.add(ident)
                ids.add(str(num))
                ids.add(f"0x{num:02x}")
    return ids

def extract_backticked_tokens(path):
    with open(path, "r", encoding="utf-8") as f:
        text = f.read()
    return set(re.findall(r"`([^`]+)`", text))

def main():
    funcs = extract_functions(CONTRACT_RS)
    errors = extract_error_ids(ERRORS_RS)
    spec_tokens = extract_backticked_tokens(SPEC_MD)
    missing = []
    for token in sorted(funcs | errors):
        if token not in spec_tokens:
            missing.append(token)
    if missing:
        print("Protocol spec drift detected. Missing documentation for the following tokens:")
        for t in missing:
            print(f"- `{t}`")
        sys.exit(1)
    else:
        print("All contract functions and error identifiers are documented in PROTOCOL_SPEC.md.")
        sys.exit(0)

if __name__ == "__main__":
    main()
