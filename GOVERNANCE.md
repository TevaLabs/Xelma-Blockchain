# Governance

## Maintainer Roles

- **Core Maintainers**: own contract correctness, release quality, and incident response.
- **Bindings Maintainers**: own TypeScript bindings and parity checks.
- **Ops Maintainers**: own CI workflows and release automation.

## Decision Policy

- Routine changes: majority approval by active maintainers in the affected area.
- Security-sensitive or breaking changes: require explicit sign-off from a core maintainer. On-chain parameter updates must be scheduled through the configuration timelock (`CONFIG_TIMELOCK_LEDGERS = 1440` ledgers, ~2 hours at ~5 seconds per ledger) before activation via `apply_scheduled_changes`.
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

Only maintainers listed for contract and workflow paths may approve release-impacting changes. Critical on-chain parameter updates are subject to the mandatory configuration timelock (`CONFIG_TIMELOCK_LEDGERS = 1440` ledgers, ~2 hours at ~5 seconds per ledger) before they can be applied.
