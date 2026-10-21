#!/usr/bin/env python3
"""Oracle payload simulator — build, validate, and preview OraclePayloads before submission.

Usage:
  python scripts/oracle_payload_sim.py \\
    --price 12345 \\
    --round-id 1234567 \\
    --network-id a1b2c3d4... \\
    --contract-addr C...

  # Validate an existing payload (e.g. a CI fixture) without building one:
  python scripts/oracle_payload_sim.py --check-only payload.json

See --help for full options.
"""

import argparse
import hashlib
import json
import re
import sys
import time

EXIT_SUCCESS = 0
EXIT_VALIDATION_FAILED = 2
EXIT_BAD_ARGS = 3

STALE_WINDOW_SECONDS = 300
MAX_CONFIDENCE_BPS = 10_000
U128_MAX = (1 << 128) - 1
U64_MAX = (1 << 64) - 1
U32_MAX = (1 << 32) - 1

STELLAR_ADDR_RE = re.compile(r"^C[A-Z0-9]{55}$")
HEX_64_RE = re.compile(r"^[0-9a-fA-F]{64}$")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build, validate, and preview OraclePayloads before on-chain submission.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Examples:\n"
            "  # Basic payload validation\n"
            "  %(prog)s --price 12345 --round-id 100 --network-id $(python -c \"import hashlib; print(hashlib.sha256(b'Test SDF Network ; September 2015').hexdigest())\") --contract-addr CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABLF4\n\n"
            "  # Full validation with deviation guardrails\n"
            "  %(prog)s --price 15500 --round-id 100 --network-id <hex> --contract-addr C... --start-price 15000 --max-deviation-bps 5000\n\n"
            "  # With confidence score\n"
            "  %(prog)s --price 12345 --round-id 100 --network-id <hex> --contract-addr C... --confidence 9500 --min-confidence-bps 5000\n\n"
            "  # Compute network ID from passphrase instead of passing hex\n"
            "  %(prog)s --price 12345 --round-id 100 --network-id-from-passphrase \"Test SDF Network ; September 2015\" --contract-addr C...\n\n"
            "  # Generate full stellar CLI invocation\n"
            "  %(prog)s --price 12345 --round-id 100 --network-id <hex> --contract-addr C... --stellar-cli --contract-id CID --oracle-key my-oracle\n\n"
            "  # Check an existing payload file's JSON layout (CI fixture guard)\n"
            "  %(prog)s --check-only fixtures/updown.json\n\n"
            "  # Same, reading the payload from stdin, machine-readable output\n"
            "  cat payload.json | %(prog)s --check-only --json\n"
        ),
    )

    g_build = parser.add_argument_group("payload fields")
    g_build.add_argument("--price", type=str, default=None, help="Settlement price (u128, 4 decimals, e.g. 12345 = $1.2345)")
    g_build.add_argument("--timestamp", type=int, default=None, help="Unix epoch seconds (default: current wall clock)")
    g_build.add_argument("--round-id", type=int, default=None, help="ActiveRound.start_ledger (u32)")
    g_build.add_argument("--nonce", type=int, default=1, help="Per-round replay protection (u64, default: 1)")
    g_build.add_argument("--network-id", type=str, default=None, help="SHA-256 hex of network passphrase (64 hex chars)")
    g_build.add_argument("--contract-addr", type=str, default=None, help="Deployed contract address (starts with C)")
    g_build.add_argument("--confidence", type=int, default=None, help="Optional confidence score 0–10000 bps")

    g_netid = parser.add_argument_group("network ID helper")
    g_netid.add_argument("--network-id-from-passphrase", type=str, default=None, metavar="PASSPHRASE",
                         help="Compute --network-id from a Stellar network passphrase instead of providing hex")

    g_validation = parser.add_argument_group("additional validation inputs")
    g_validation.add_argument("--start-price", type=str, default=None, help="Round.price_start for deviation check (u128, 4 decimals)")
    g_validation.add_argument("--max-deviation-bps", type=int, default=None, help="OracleMaxDeviationBps threshold")
    g_validation.add_argument("--min-confidence-bps", type=int, default=None, help="Minimum acceptable confidence bps")

    g_output = parser.add_argument_group("output options")
    g_output.add_argument("--check-only", action="store_true",
                          help="Validate the JSON layout of an existing payload instead of building one")
    g_output.add_argument("--stellar-cli", action="store_true", help="Emit a copy-paste stellar contract invoke command")
    g_output.add_argument("--json", action="store_true", help="Emit only the JSON payload (no validation output); with --check-only, emit a machine-readable result")
    g_output.add_argument("--contract-id", type=str, default=None, help="Contract ID for stellar CLI command")
    g_output.add_argument("--oracle-key", type=str, default=None, help="Stellar key identity for signing")
    g_output.add_argument("--network", type=str, default="testnet", help="Stellar network name (default: testnet)")

    parser.add_argument("payload_file", nargs="?", default=None, metavar="PAYLOAD_JSON",
                        help="Payload JSON file to check (with --check-only; default: stdin)")

    args = parser.parse_args(argv)

    if args.payload_file is not None and not args.check_only:
        parser.error("PAYLOAD_JSON is only valid with --check-only")
    if args.check_only:
        return args

    missing = [
        flag
        for flag, value in (
            ("--price", args.price),
            ("--round-id", args.round_id),
            ("--contract-addr", args.contract_addr),
        )
        if value is None
    ]
    if missing:
        parser.error(f"the following arguments are required: {', '.join(missing)}")
    if not args.network_id and not args.network_id_from_passphrase:
        parser.error("one of --network-id or --network-id-from-passphrase is required")
    if args.network_id_from_passphrase:
        if args.network_id:
            parser.error("--network-id and --network-id-from-passphrase are mutually exclusive")
        args.network_id = hashlib.sha256(args.network_id_from_passphrase.encode("utf-8")).hexdigest()

    return args


def validate_price(price_str: str) -> list[dict]:
    issues = []
    try:
        v = int(price_str)
    except (ValueError, TypeError):
        issues.append(dict(severity="ERROR", field="price", msg=f"not a valid integer: {price_str!r}"))
        return issues

    if v < 0:
        issues.append(dict(severity="ERROR", field="price", msg=f"must be non-negative, got {v}"))
    elif v == 0:
        issues.append(dict(severity="ERROR", field="price", msg="must be > 0 (zero is always rejected by contract)"))
    elif v > U128_MAX:
        issues.append(dict(severity="ERROR", field="price", msg=f"exceeds u128 max ({U128_MAX})"))
    else:
        issues.append(dict(severity="OK", field="price", msg=f"{v} (valid u128, {'> 0' if v > 0 else 'IS ZERO'})"))
    return issues


def validate_timestamp(ts: int, enforce_freshness: bool = True) -> list[dict]:
    issues = []
    now = int(time.time())

    if ts < 0:
        issues.append(dict(severity="ERROR", field="timestamp", msg=f"negative timestamp: {ts}"))
        return issues

    issues.append(dict(severity="OK", field="timestamp", msg=f"{ts} (Unix epoch second)"))

    if not enforce_freshness:
        return issues

    age = now - ts
    if age < 0:
        issues.append(dict(severity="ERROR", field="timestamp", msg=f"in the future by {-age}s — contract rejects future timestamps"))
    elif age > STALE_WINDOW_SECONDS:
        issues.append(dict(severity="ERROR", field="timestamp",
                           msg=f"{age}s old — exceeds {STALE_WINDOW_SECONDS}s stale window (StaleOracleData)"))
    elif age > STALE_WINDOW_SECONDS // 2:
        issues.append(dict(severity="WARN", field="timestamp",
                           msg=f"{age}s old — within stale window but approaching limit ({STALE_WINDOW_SECONDS}s)"))
    else:
        issues.append(dict(severity="OK", field="timestamp", msg=f"fresh ({age}s old, within {STALE_WINDOW_SECONDS}s window)"))
    return issues


def validate_u32(val: int, field: str, label: str) -> list[dict]:
    issues = []
    if not isinstance(val, int) or val < 0 or val > U32_MAX:
        issues.append(dict(severity="ERROR", field=field, msg=f"out of range (u32: 0–{U32_MAX}), got {val}"))
    else:
        issues.append(dict(severity="OK", field=field, msg=f"{val} (valid u32)"))
    return issues


def validate_u64(val: int, field: str, label: str) -> list[dict]:
    issues = []
    if not isinstance(val, int) or val < 0 or val > U64_MAX:
        issues.append(dict(severity="ERROR", field=field, msg=f"out of range (u64: 0–{U64_MAX}), got {val}"))
    else:
        issues.append(dict(severity="OK", field=field, msg=f"{val} (valid u64)"))
    return issues


def validate_network_id(hex_str: str) -> list[dict]:
    issues = []
    if not HEX_64_RE.match(hex_str):
        issues.append(dict(severity="ERROR", field="network_id",
                           msg=f"must be 64 hex chars (32 bytes SHA-256), got {len(hex_str)} chars"))
    else:
        issues.append(dict(severity="OK", field="network_id", msg=f"{hex_str[:16]}...{hex_str[-16:]} (32 bytes hex)"))
    return issues


def validate_contract_addr(addr: str) -> list[dict]:
    issues = []
    if not STELLAR_ADDR_RE.match(addr):
        issues.append(dict(severity="ERROR", field="contract_addr",
                           msg="must be a Stellar contract address (56 chars, starts with C)"))
    else:
        issues.append(dict(severity="OK", field="contract_addr", msg=f"{addr} (valid Stellar contract address)"))
    return issues


def validate_confidence(confidence: int | None, min_confidence_bps: int | None) -> list[dict]:
    issues = []
    if confidence is None:
        issues.append(dict(severity="OK", field="confidence", msg="None (legacy mode)"))
        if min_confidence_bps is not None:
            issues.append(dict(severity="WARN", field="confidence",
                               msg=f"confidence is None but --min-confidence-bps={min_confidence_bps} set — will fail in strict mode"))
        return issues

    if confidence < 0 or confidence > MAX_CONFIDENCE_BPS:
        issues.append(dict(severity="ERROR", field="confidence",
                           msg=f"out of range (0–{MAX_CONFIDENCE_BPS} bps), got {confidence}"))
    else:
        issues.append(dict(severity="OK", field="confidence", msg=f"{confidence} bps (in range 0–{MAX_CONFIDENCE_BPS})"))

    if min_confidence_bps is not None and confidence < min_confidence_bps:
        issues.append(dict(severity="WARN", field="confidence",
                           msg=f"{confidence} bps < min {min_confidence_bps} bps — contract will reject"))
    return issues


def validate_deviation(price_str: str, start_price_str: str | None, max_deviation_bps: int | None) -> list[dict]:
    issues = []
    if start_price_str is None or max_deviation_bps is None:
        if start_price_str is None and max_deviation_bps is not None:
            issues.append(dict(severity="WARN", field="deviation", msg="--start-price not provided, skipping deviation check"))
        elif start_price_str is not None and max_deviation_bps is None:
            issues.append(dict(severity="WARN", field="deviation", msg="--max-deviation-bps not provided, skipping deviation check"))
        return issues

    try:
        price = int(price_str)
        start_price = int(start_price_str)
    except ValueError:
        issues.append(dict(severity="WARN", field="deviation", msg="cannot parse price as integer, skipping deviation check"))
        return issues

    if start_price == 0:
        issues.append(dict(severity="WARN", field="deviation", msg="start_price is 0, cannot compute deviation"))
        return issues

    diff = abs(price - start_price)
    diff_bps = (diff * 10_000) // start_price

    issues.append(dict(severity="OK", field="deviation",
                       msg=f"|{price} - {start_price}| = {diff} → {diff_bps} bps (max: {max_deviation_bps})"))

    if diff_bps > max_deviation_bps:
        issues.append(dict(severity="ERROR", field="deviation",
                           msg=f"{diff_bps} bps exceeds max {max_deviation_bps} bps — will be rejected (OracleDeviationExceeded)"))
    return issues


REQUIRED_PAYLOAD_KEYS = (
    "price",
    "timestamp",
    "round_id",
    "nonce",
    "network_id",
    "contract_addr",
    "confidence",
)


def load_payload_json(source: str) -> dict:
    """Read a payload JSON document from a file path or stdin (`-`)."""
    if source == "-":
        raw = sys.stdin.read()
    else:
        with open(source, "r", encoding="utf-8") as fh:
            raw = fh.read()
    payload = json.loads(raw)
    if not isinstance(payload, dict):
        raise ValueError("payload must be a JSON object")
    return payload


def validate_payload_layout(payload: dict) -> list[dict]:
    """Validate the exact JSON layout of a pre-built oracle payload.

    The layout is the one emitted by `--json`: every key in
    `REQUIRED_PAYLOAD_KEYS` must be present, no unknown keys are allowed, and
    each value must have the type the simulator produces. Field ranges are
    then validated with the same rules used when building a payload.
    """
    issues: list[dict] = []

    present = set(payload)
    required = set(REQUIRED_PAYLOAD_KEYS)
    for key in sorted(required - present):
        issues.append(dict(severity="ERROR", field=key, msg="missing required key in payload layout"))
    for key in sorted(present - required):
        issues.append(dict(severity="ERROR", field=key, msg="unknown key in payload layout"))

    def type_error(field: str, expected: str, value) -> None:
        issues.append(
            dict(severity="ERROR", field=field, msg=f"expected {expected}, got {type(value).__name__}")
        )

    price = payload.get("price")
    if "price" in present and not (isinstance(price, str) or (isinstance(price, int) and not isinstance(price, bool))):
        type_error("price", "integer or decimal string", price)
    else:
        if "price" in present:
            issues.extend(validate_price(price if isinstance(price, str) else str(price)))

    for key in ("timestamp", "round_id", "nonce"):
        value = payload.get(key)
        if key in present and not (isinstance(value, int) and not isinstance(value, bool)):
            type_error(key, "integer", value)

    if isinstance(payload.get("timestamp"), int) and not isinstance(payload.get("timestamp"), bool):
        # Fixture vectors are static, so only the layout/range is checked here;
        # freshness is a submission-time concern, not a layout concern.
        issues.extend(validate_timestamp(payload["timestamp"], enforce_freshness=False))
    if isinstance(payload.get("round_id"), int) and not isinstance(payload.get("round_id"), bool):
        issues.extend(validate_u32(payload["round_id"], "round_id", "round id"))
    if isinstance(payload.get("nonce"), int) and not isinstance(payload.get("nonce"), bool):
        issues.extend(validate_u64(payload["nonce"], "nonce", "nonce"))

    network_id = payload.get("network_id")
    if "network_id" in present and not isinstance(network_id, str):
        type_error("network_id", "string", network_id)
    elif "network_id" in present:
        issues.extend(validate_network_id(network_id))

    contract_addr = payload.get("contract_addr")
    if "contract_addr" in present and not isinstance(contract_addr, str):
        type_error("contract_addr", "string", contract_addr)
    elif "contract_addr" in present:
        issues.extend(validate_contract_addr(contract_addr))

    confidence = payload.get("confidence")
    if "confidence" in present and confidence is not None and not (
        isinstance(confidence, int) and not isinstance(confidence, bool)
    ):
        type_error("confidence", "integer or null", confidence)
    elif "confidence" in present:
        issues.extend(validate_confidence(confidence, None))

    return issues


def build_payload_json(args: argparse.Namespace) -> str:
    payload = {
        "price": args.price,
        "timestamp": args.timestamp if args.timestamp is not None else int(time.time()),
        "round_id": args.round_id,
        "nonce": args.nonce,
        "network_id": args.network_id,
        "contract_addr": args.contract_addr,
        "confidence": args.confidence,
    }
    return json.dumps(payload, indent=2)


def build_stellar_cli(args: argparse.Namespace, payload_json: str) -> str:
    cid = args.contract_id or "<CONTRACT_ID>"
    skey = args.oracle_key or "<ORACLE_KEY>"
    network = args.network
    compact = json.dumps(json.loads(payload_json), separators=(",", ":"))
    return (
        f"stellar contract invoke \\\n"
        f"  --id {cid} \\\n"
        f"  --source {skey} \\\n"
        f"  --network {network} \\\n"
        f"  resolve_round \\\n"
        f"  --payload '{compact}'"
    )


def print_validation(issues: list[dict]):
    errors = [i for i in issues if i["severity"] == "ERROR"]
    warnings = [i for i in issues if i["severity"] == "WARN"]
    ok = [i for i in issues if i["severity"] == "OK"]

    print("── Validation ──────────────────────────────")
    for i in ok:
        print(f"  ✓ {i['field']}: {i['msg']}")
    for i in warnings:
        print(f"  ⚠ {i['field']}: {i['msg']}")
    for i in errors:
        print(f"  ✘ {i['field']}: {i['msg']}")

    print()
    if not errors and not warnings:
        print("  ✔ All checks passed — payload ready to submit.")
    elif not errors:
        print(f"  ⚠ {len(warnings)} warning(s) — payload may still work, review warnings.")
    else:
        print(f"  ✘ {len(errors)} error(s) — fix before submitting (will be rejected on-chain).")


def print_header():
    print("╔═══════════════════════════════════════════╗")
    print("║  Oracle Payload Simulator                 ║")
    print("╚═══════════════════════════════════════════╝")
    print()


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv or sys.argv[1:])

    if args.check_only is not None:
        return run_check_only(args)

    if not args.timestamp:
        args.timestamp = int(time.time())

    all_issues: list[dict] = []
    all_issues.extend(validate_price(args.price))
    all_issues.extend(validate_timestamp(args.timestamp))
    all_issues.extend(validate_u32(args.round_id, "round_id", "round id"))
    all_issues.extend(validate_u64(args.nonce, "nonce", "nonce"))
    all_issues.extend(validate_network_id(args.network_id))
    all_issues.extend(validate_contract_addr(args.contract_addr))
    all_issues.extend(validate_confidence(args.confidence, args.min_confidence_bps))
    all_issues.extend(validate_deviation(args.price, args.start_price, args.max_deviation_bps))

    payload_json = build_payload_json(args)
    errors = [i for i in all_issues if i["severity"] == "ERROR"]

    if args.json:
        print(payload_json)
        return EXIT_SUCCESS if not errors else EXIT_VALIDATION_FAILED

    print_header()
    print_validation(all_issues)

    print()
    print("── JSON Payload ────────────────────────────")
    print(payload_json)

    if args.stellar_cli:
        print()
        print("── stellar CLI Command ────────────────────")
        print(build_stellar_cli(args, payload_json))

    return EXIT_SUCCESS if not errors else EXIT_VALIDATION_FAILED


def run_check_only(args: argparse.Namespace) -> int:
    source = args.payload_file or "-"
    try:
        payload = load_payload_json(source)
    except (OSError, ValueError) as exc:
        print(f"error: cannot read payload: {exc}", file=sys.stderr)
        return EXIT_BAD_ARGS

    issues = validate_payload_layout(payload)
    if args.min_confidence_bps is not None:
        issues.extend(validate_confidence(payload.get("confidence"), args.min_confidence_bps))
    if args.start_price is not None or args.max_deviation_bps is not None:
        issues.extend(
            validate_deviation(
                str(payload.get("price")),
                args.start_price,
                args.max_deviation_bps,
            )
        )

    errors = [i for i in issues if i["severity"] == "ERROR"]
    valid = not errors

    if args.json:
        print(json.dumps({"valid": valid, "issues": issues}, indent=2))
        return EXIT_SUCCESS if valid else EXIT_VALIDATION_FAILED

    print_header()
    print(f"Check-only mode: {source}")
    print()
    print_validation(issues)
    return EXIT_SUCCESS if valid else EXIT_VALIDATION_FAILED


if __name__ == "__main__":
    sys.exit(main())
