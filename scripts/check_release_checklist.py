#!/usr/bin/env python3
"""Simple machine-checkable deployment checklist for release operators."""

import argparse
import os
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate deployment checklist inputs")
    parser.add_argument("--network", default="testnet")
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()

    checks = []
    checks.append(("network provided", bool(args.network)))
    checks.append(("working tree clean", _is_worktree_clean()))
    checks.append(("artifact present", _artifact_present()))

    failures = [name for name, passed in checks if not passed]
    if failures:
        print("Deployment checklist FAILED:")
        for failure in failures:
            print(f"- {failure}")
        return 1

    print("Deployment checklist passed")
    return 0


def _is_worktree_clean() -> bool:
    return os.system("git diff --quiet --ignore-submodules --exit-code") == 0


def _artifact_present() -> bool:
    artifact = Path("target/wasm32-unknown-unknown/release/xelma_contract.wasm")
    return artifact.exists()


if __name__ == "__main__":
    sys.exit(main())
