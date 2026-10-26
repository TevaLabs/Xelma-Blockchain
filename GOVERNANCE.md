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

## Config Change Timelock

Critical protocol parameters (bet/run windows, stake and exposure caps, oracle
thresholds, protocol fee, min bet, pending-winnings expiry, ...) are not
changed instantly. Every change routed through `schedule_*` in
`contracts/src/config.rs` (`_schedule_config_change`) stores a
`PendingConfigChange` whose activation ledger is:

```
activation_ledger = scheduled_at_ledger + CONFIG_TIMELOCK_LEDGERS
```

- `CONFIG_TIMELOCK_LEDGERS` = 1440 ledgers (about 2 hours at 5 s per ledger),
  defined in `contracts/src/common.rs` and consumed by
  `_schedule_config_change` in `contracts/src/config.rs`.
- Schedule: admin-only, rejected while paused, and only one pending change per
  `ConfigChangeKind`.
- During the timelock: anyone can inspect it via `get_pending_config_change`;
  the admin may cancel it with `cancel_config_change` (only before
  `activation_ledger`).
- After `activation_ledger`: anyone may call `apply_scheduled_changes` in
  normal mode; the payload is re-validated at apply time.
- Some admin setters (for example `set_fee_model`, `set_close_buffer_ledgers`,
  `set_min_participants`, `set_archive_retention`) apply immediately without a
  timelock, but still require admin auth and an unpaused contract.

## Escalation

If a review stalls, open a comment tagging relevant owners from `CODEOWNERS` and include:

1. What is blocked
2. Current risk level
3. Requested decision and deadline

## Release Authority

Only maintainers listed for contract and workflow paths may approve release-impacting changes.
