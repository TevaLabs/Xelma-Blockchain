# Deployment Runbook

This runbook covers staging-to-mainnet deployment for Xelma contract updates, including pre-flight checks, staged rollout, rollback, and incident communication.

## Build Target

**`wasm32v1-none`** is the canonical WASM target for all builds and deployments. This target uses the WASM MVP feature baseline that the Soroban host supports, avoiding `reference-types` which are rejected at deploy time.

All CI jobs, deploy scripts, and documentation reference this target. When building locally, always use:

```bash
cargo rustc --manifest-path=contracts/Cargo.toml --crate-type=cdylib --target=wasm32v1-none --release --locked
```

The output artifact is located at: `target/wasm32v1-none/release/xelma_contract.wasm`

## 1. Pre-deployment checklist

### Ownership
- Release owner: responsible for approving the release window and signing off on the checklist.
- Operator: executes the deployment, verifies on-chain state, and monitors health.
- Incident lead: owns communications and rollback decisions when health checks fail.

### Required inputs
- Target network and chain ID.
- Admin and oracle public keys.
- Contract WASM hash and deployment artifact path.
- Any planned config values, especially bet window, run window, and close-buffer values.
- Rollback plan and pause contact list.

### Checklist
- [ ] Confirm target network and chain ID match the intended environment.
- [ ] Execute testnet dry-run validation: `./scripts/deploy_testnet.sh --dry-run` or trigger `Deploy Testnet` GitHub Actions workflow with `dry_run: true`.
- [ ] Verify admin and oracle addresses are the intended role keys.
- [ ] Confirm the deployed artifact hash/SHA256 checksum matches the reviewed build.
- [ ] Run the release checklist script: `python3 scripts/check_release_checklist.py --network mainnet --strict`.
- [ ] Review the pending config changes and ensure the timelock is acceptable.
- [ ] Confirm an operator can access the pause/rollback path before deployment starts.

## 2. Staged rollout expectations

1. Deploy to staging first and verify the contract initializes correctly.
2. Exercise a full round lifecycle in staging, including create round, place bet, and resolution.
3. If staging passes, deploy a canary instance or a low-risk mainnet update with a small initial audience.
4. Monitor events, role permissions, and ledger-level behavior for at least one full round.
5. Only expand traffic after the health checks remain green.

### Canary exit criteria
- No unexpected reverts in admin or oracle flows.
- No increase in pause-triggering incidents.
- Round lifecycle behaves as expected across betting and resolution.

## 3. During deployment

### Execution sequence
1. Build and verify the artifact.
2. Deploy the contract to the target network.
3. Initialize or migrate the contract using the intended admin/oracle pair.
4. Apply any required config values and confirm the values on-chain.
5. Create a smoke-test round and verify basic operations.

### Monitoring signals
- Contract events for round creation, config updates, and settlement.
- Admin and oracle role access.
- Contract pause status and any rejected actions.
- Ledger-level errors in the oracle and operator tooling.

## 4. Rollback and incident response

### Pause path
- If the deployment introduces a high-risk regression, pause the contract immediately.
- Keep the pause state until the incident lead confirms the fix path.

### Rollback path
- Roll back to the previous deployed WASM if the new deployment is not safe.
- Revert any config changes that were applied during the rollout.
- Re-run smoke tests against the previous artifact before re-enabling activity.

### Escalation
- Escalate to the incident lead if pause, config, or settlement behavior diverges from expectations.
- Notify the release owner, operator, and any downstream indexers/frontends of the incident and recovery timeline.

## 5. Failure-mode decision tree

- If initialization or migration fails: stop, preserve the previous deployment, and investigate configuration.
- If the contract is unusable after deployment: pause and rollback.
- If only a specific role or oracle flow breaks: disable the affected workflow, communicate the issue, and patch before reopening the system.
- If the issue is limited to a non-critical UI or indexer problem: keep the contract paused or isolated until downstream systems are updated.

## 7. Operator Playbook: Archive Retention & Expired Pending Winnings Reclaim

### 7.1 FIFO Archive Prune

Operators manage the on-chain archive depth using the `archive_retention` threshold and the `prune_archived_rounds` entrypoint in [contracts/src/admin.rs](file:///C:/Users/SOSA/Downloads/od/Xelma-Blockchain/contracts/src/admin.rs).

* **Authorization**: Admin-only (`admin.require_auth()`).
* **CLI Command**:
  ```bash
  soroban contract invoke --id <CONTRACT_ID> --source-account <ADMIN_KEY> --network <NETWORK> -- prune_archived_rounds --max_prune_count 50
  ```
* **Threshold Configuration**:
  ```bash
  soroban contract invoke --id <CONTRACT_ID> --source-account <ADMIN_KEY> --network <NETWORK> -- set_archive_retention --retention_count 100
  ```
* **Monitoring & Failure Modes**:
  - Monitor `("admin", "archive_pruned")` contract events for pruned counts.
  - If `prune_archived_rounds` fails with `NotInitialized` or authorization error, verify admin signature.
  - Excessive archival depth without pruning increases persistent storage footprint.

### 7.2 Reclaiming Expired Pending Winnings

Unclaimed user winnings past the global expiration threshold can be reclaimed into the protocol fee treasury.

* **Authorization**: Admin-only (`admin.require_auth()`).
* **CLI Command**:
  ```bash
  soroban contract invoke --id <CONTRACT_ID> --source-account <ADMIN_KEY> --network <NETWORK> -- reclaim_expired_pending_winnings --max_users 50
  ```
* **Failure Modes & Safety**:
  - Unclaimed amounts that have not reached the expiration threshold remain untouched.
  - Emits `("admin", "pending_reclaimed")` with `(user, amount, reclaimed_to_treasury)`.

---

## 8. Post-deployment

- [ ] Confirm the admin and oracle addresses match the intended identities.
- [ ] Verify the deployed artifact hash and network ID.
- [ ] Confirm round windows, close-buffer, and any fee settings are correct.
- [ ] Record the deployment timestamp, contract ID, and operator notes.
- [ ] Publish a short release summary to operators and stakeholders.
