#!/usr/bin/env bash
#
# deploy_testnet.sh — Controlled Soroban testnet deployment with dry-run safety.
#
# Usage:
#   ./scripts/deploy_testnet.sh              # deploy mode
#   ./scripts/deploy_testnet.sh --dry-run    # validate everything, broadcast nothing
#
# Required environment variables:
#   SOROBAN_RPC_URL              RPC endpoint for Stellar Testnet
#   SOROBAN_NETWORK_PASSPHRASE   Network passphrase (Testnet / Futurenet / …)
#   DEPLOYER_SECRET_KEY          Secret key of the account paying deployment fees
#   SOROBAN_ADMIN_ADDRESS        Public address of the contract admin
#   ORACLE_ADDRESS               Public address of the oracle signer
#
# Optional:
#   CONTRACT_WASM_PATH           Path to the compiled WASM file
#                                (default: target/wasm32-unknown-unknown/release/xelma_contract.wasm)
#

set -euo pipefail

# ─────────────────────────────────────────────────────────────────────
# 0.  Parse flags
# ─────────────────────────────────────────────────────────────────────
DRY_RUN=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=true; shift ;;
    *)
      echo "Usage: $0 [--dry-run]"
      exit 1
      ;;
  esac
done

# ─────────────────────────────────────────────────────────────────────
# 1.  Paths & build
# ─────────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

WASM_PATH="${CONTRACT_WASM_PATH:-"$REPO_ROOT/target/wasm32-unknown-unknown/release/xelma_contract.wasm"}"

echo "============================================"
if $DRY_RUN; then
  echo " MODE:  DRY RUN (no transactions broadcast)"
else
  echo " MODE:  DEPLOY"
fi
echo "============================================"
echo ""

# ── Build WASM ──────────────────────────────────────────────────────
echo "[1/5] Building contract WASM …"
cargo build \
  --target wasm32-unknown-unknown \
  --release \
  --package xelma-contract \
  --locked \
  2>&1

echo "  ✓ Build complete"

# ─────────────────────────────────────────────────────────────────────
# 2.  Hash the WASM artifact
# ─────────────────────────────────────────────────────────────────────
echo "[2/5] Hashing WASM artifact …"
if [[ ! -f "$WASM_PATH" ]]; then
  echo "  ERROR: WASM artifact not found at: $WASM_PATH"
  echo "  Check CONTRACT_WASM_PATH or build output."
  exit 1
fi

WASM_HASH=$(sha256sum "$WASM_PATH" | cut -d' ' -f1)
WASM_SIZE=$(stat --format=%s "$WASM_PATH" 2>/dev/null || stat -f%z "$WASM_PATH" 2>/dev/null || wc -c < "$WASM_PATH")

echo "  Path:  $WASM_PATH"
echo "  Size:  $WASM_SIZE bytes"
echo "  SHA25: $WASM_HASH"

# ─────────────────────────────────────────────────────────────────────
# 3.  Validate required configuration
# ─────────────────────────────────────────────────────────────────────
echo "[3/5] Validating configuration …"

ERRORS=0

# 3a. Network
RPC_URL="${SOROBAN_RPC_URL:-}"
NETWORK_PASSPHRASE="${SOROBAN_NETWORK_PASSPHRASE:-}"
if [[ -z "$RPC_URL" ]]; then
  echo "  ✘ SOROBAN_RPC_URL is not set"
  ERRORS=$((ERRORS + 1))
fi
if [[ -z "$NETWORK_PASSPHRASE" ]]; then
  echo "  ✘ SOROBAN_NETWORK_PASSPHRASE is not set"
  ERRORS=$((ERRORS + 1))
fi

# Validate expected testnet passphrase
if [[ -n "$NETWORK_PASSPHRASE" ]] && [[ "$NETWORK_PASSPHRASE" != "Test SDF Network ; September 2015" ]]; then
  echo "  ⚠  Non-testnet passphrase detected: $NETWORK_PASSPHRASE"
  echo "     Expected: Test SDF Network ; September 2015"
  echo "     Continuing — make sure this is intentional."
fi

# 3b. Deployer key
DEPLOYER_KEY="${DEPLOYER_SECRET_KEY:-}"
if [[ -z "$DEPLOYER_KEY" ]]; then
  echo "  ✘ DEPLOYER_SECRET_KEY is not set"
  ERRORS=$((ERRORS + 1))
fi

# 3c. Admin address
ADMIN_ADDRESS="${SOROBAN_ADMIN_ADDRESS:-}"
if [[ -z "$ADMIN_ADDRESS" ]]; then
  echo "  ✘ SOROBAN_ADMIN_ADDRESS is not set"
  ERRORS=$((ERRORS + 1))
fi

# 3d. Oracle address
ORACLE_ADDRESS="${ORACLE_ADDRESS:-}"
if [[ -z "$ORACLE_ADDRESS" ]]; then
  echo "  ✘ ORACLE_ADDRESS is not set"
  ERRORS=$((ERRORS + 1))
fi

# 3e. WASM artifact
if [[ ! -f "$WASM_PATH" ]]; then
  echo "  ✘ WASM artifact missing: $WASM_PATH"
  ERRORS=$((ERRORS + 1))
fi

echo ""
if [[ $ERRORS -gt 0 ]]; then
  echo "  FAILED — $ERRORS configuration error(s) found."
  exit 1
fi
echo "  ✓ All required values present"

# ── Determine stellar CLI ────────────────────────────────────────────
STELLAR_CMD=""
for cmd in stellar soroban; do
  if command -v "$cmd" &>/dev/null; then
    STELLAR_CMD="$cmd"
    break
  fi
done

if [[ -z "$STELLAR_CMD" ]]; then
  echo "  ✘ Neither 'stellar' nor 'soroban' CLI found in PATH"
  echo "    Install: https://developers.stellar.org/docs/soroban/cli"
  exit 1
fi
echo "  CLI: $STELLAR_CMD ($($STELLAR_CMD --version 2>/dev/null || echo 'unknown'))"

# ── Write a temporary identity for deployment ───────────────────────
SOROBAN_IDENTITY_DIR="$REPO_ROOT/.soroban/identity"
mkdir -p "$SOROBAN_IDENTITY_DIR"

DEPLOY_IDENTITY="deployer-$$"
cat > "$SOROBAN_IDENTITY_DIR/$DEPLOY_IDENTITY.toml" <<-IDENTITYEOF
secret_key = "$DEPLOYER_KEY"
IDENTITYEOF

cleanup() {
  rm -f "$SOROBAN_IDENTITY_DIR/$DEPLOY_IDENTITY.toml"
}
trap cleanup EXIT

echo ""

# ─────────────────────────────────────────────────────────────────────
# 4.  Dry-run summary — stop here if --dry-run
# ─────────────────────────────────────────────────────────────────────
echo "[4/5] Deployment readiness summary"
echo "  Network RPC:  $RPC_URL"
echo "  Passphrase:   $NETWORK_PASSPHRASE"
echo "  Admin:        $ADMIN_ADDRESS"
echo "  Oracle:       $ORACLE_ADDRESS"
echo "  Deployer:     ${DEPLOYER_KEY:0:4}…${DEPLOYER_KEY: -4}"
echo "  WASM:         $WASM_PATH ($WASM_SIZE bytes)"
echo "  WASM hash:    $WASM_HASH"
echo "  CLI:          $STELLAR_CMD"

if $DRY_RUN; then
  echo ""
  echo "═══════════════════════════════════════════════"
  echo "  DRY RUN — no transactions broadcast."
  echo "  All validations passed. Ready to deploy."
  echo "═══════════════════════════════════════════════"
  exit 0
fi

# ─────────────────────────────────────────────────────────────────────
# 5.  Deploy contract on testnet
# ─────────────────────────────────────────────────────────────────────
echo "[5/5] Deploying contract to testnet …"

DEPLOY_OUTPUT=$("$STELLAR_CMD" contract deploy \
  --wasm "$WASM_PATH" \
  --source "$DEPLOY_IDENTITY" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  2>&1)

echo "  $DEPLOY_OUTPUT"

CONTRACT_ID=$(echo "$DEPLOY_OUTPUT" | grep -oE 'C[A-Z0-9]{55}')

if [[ -z "$CONTRACT_ID" ]]; then
  echo ""
  echo "  ERROR: Could not parse contract ID from deployment output."
  echo "  Raw output:"
  echo "  $DEPLOY_OUTPUT"
  exit 1
fi

echo ""
echo "═══════════════════════════════════════════════"
echo "  DEPLOYMENT COMPLETE"
echo "═══════════════════════════════════════════════"
echo "  Contract ID:  $CONTRACT_ID"
echo "  WASM hash:    $WASM_HASH"
echo "  Network:      $NETWORK_PASSPHRASE"
echo "═══════════════════════════════════════════════"
echo ""

# ─────────────────────────────────────────────────────────────────────
# 6.  Initialization checklist
# ─────────────────────────────────────────────────────────────────────
echo "=== Initialization Checklist ==="
echo ""
echo "  After deployment, call \`initialize\` with the admin and oracle addresses:"
echo ""
echo "    stellar contract invoke \\"
echo "      --id $CONTRACT_ID \\"
echo "      --source $ADMIN_ADDRESS \\"
echo "      --rpc-url $RPC_URL \\"
echo "      --network-passphrase '$NETWORK_PASSPHRASE' \\"
echo "      -- \\"
echo "      initialize \\"
echo "      --admin $ADMIN_ADDRESS \\"
echo "      --oracle $ORACLE_ADDRESS"
echo ""
echo "  Steps remaining:"
echo "    1. Initialize contract with admin + oracle addresses"
echo "    2. Configure round windows with set_windows()"
echo "    3. Verify contract state with get_admin() and get_oracle()"
echo "    4. Publish contract ID to consumers (frontend, indexer)"
echo "    5. Register oracle heartbeat"
echo ""
