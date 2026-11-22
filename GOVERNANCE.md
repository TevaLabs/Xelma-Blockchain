# Governance

## Maintainer Roles

- **Core Maintainers**: own contract correctness, release quality, and incident response.
- **Bindings Maintainers**: own TypeScript bindings and parity checks.
- **Ops Maintainers**: own CI workflows and release automation.

## Decision Policy

- Routine changes: majority approval by active maintainers in the affected area.
- Security-sensitive or breaking changes: require explicit sign-off from a core maintainer.
- In urgent incidents, a core maintainer may temporarily pause normal process to ship a patch.

## Review SLAs

- Initial triage target: within 2 business days.
- PR review target for standard changes: within 3 business days.
- High-priority incident/security fixes: as soon as practical.

## Escalation

If a review stalls, open a comment tagging relevant owners from `CODEOWNERS` and include:

1. What is blocked
2. Current risk level
3. Requested decision and deadline

## Release Authority

Only maintainers listed for contract and workflow paths may approve release-impacting changes.

## On-Chain Parameter Governance & Timelock Constants

Critical administrative and parameter changes are enforced on-chain by timelock and dual-approval constants defined in `contracts/src/common.rs` and `contracts/src/governance.rs`:

| Constant | Value | Source | Purpose |
|----------|-------|--------|---------|
| `CONFIG_TIMELOCK_LEDGERS` | `720` ledgers (~1 hour at 5s/ledger) | `contracts/src/common.rs` | Mandatory delay before scheduled parameter updates (`schedule_min_bet`, `schedule_max_stake`, `schedule_oracle_stale_threshold`, `schedule_oracle_deviation_bps`, `schedule_oracle_timestamp_skew`, `schedule_protocol_fee_bps`, `schedule_pending_winnings_expiry`) can be applied via `apply_scheduled_changes`. |
| `DEFAULT_GOV_PROPOSAL_TTL_LEDGERS` | `17_280` ledgers (~24 hours) | `contracts/src/common.rs` | Default expiration window for dual-approval governance proposals (`propose_gov_action` / `approve_gov_action` / `execute_gov_action`) and constitutional amendments (`propose_amendment` / `approve_amendment` / `execute_amendment`). |
| `MIN_ROTATION_DELAY_SECONDS` | `86_400` seconds (24 hours) | `contracts/src/common.rs` | Minimum wall-clock timelock required between `propose_oracle_rotation` and `accept_oracle_rotation`. |
