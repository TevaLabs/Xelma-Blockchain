## WASM Size Budget

### Current baseline
Stored in `.wasm-size-baseline` at the repo root (plain integer, bytes).

### Budget policy
CI allows up to **+5%** growth above baseline before failing.

### How CI checks it
The `wasm-size-gate` job in `.github/workflows/ci.yml`:
1. Builds `xelma_contract.wasm` in release mode
2. Runs `scripts/check_wasm_size.sh` against that artifact, which compares
   size against `.wasm-size-baseline` + 5%, prints a size report, and fails
   with an actionable error if the budget is exceeded

### Checking it locally
Run `./scripts/check_wasm_size.sh` with no arguments from anywhere in the
repo — it builds the release WASM the same way CI does and reports the same
budget check, so you can catch a size regression before pushing. Pass an
existing `.wasm` path as an argument to skip the build and just measure a
WASM you already built.

### Updating the baseline
When intentional size growth is merged (new feature, dependency bump):
1. Build locally: `cargo rustc --manifest-path=contracts/Cargo.toml --crate-type=cdylib --target=wasm32v1-none --release --locked`
2. Measure: `wc -c < target/wasm32v1-none/release/xelma_contract.wasm`
3. Update `.wasm-size-baseline` with the new byte count
4. Commit and push with message: `chore: update WASM size baseline to <N> bytes`

### Why this matters
Soroban contracts have deployment and execution constraints. Unbounded size growth
makes mainnet deployment riskier and more expensive. This gate ensures size changes
are intentional and reviewed.
