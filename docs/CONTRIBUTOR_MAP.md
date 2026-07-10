# Contributor Map

This map helps new contributors find the right protocol area, key files, tests, and starter tasks quickly.

## Architecture domains

### Oracle and round lifecycle
- Primary files: [contracts/src/contract.rs](../contracts/src/contract.rs), [contracts/src/betting.rs](../contracts/src/betting.rs), [contracts/src/settlement.rs](../contracts/src/settlement.rs)
- Key tests: [contracts/src/tests/commit_reveal_e2e.rs](../contracts/src/tests/commit_reveal_e2e.rs), [contracts/src/tests/lifecycle.rs](../contracts/src/tests/lifecycle.rs), [contracts/src/tests/resolution.rs](../contracts/src/tests/resolution.rs)
- Good first tasks: add or fix a lifecycle edge case, tighten round-phase assertions, or expand coverage around resolution.
- Advanced tasks: change settlement ordering or add new oracle payload validation rules.

### Windows, timing, and fairness
- Primary files: [contracts/src/config.rs](../contracts/src/config.rs), [contracts/src/common.rs](../contracts/src/common.rs), [contracts/src/tests/windows.rs](../contracts/src/tests/windows.rs)
- Key tests: [contracts/src/tests/windows.rs](../contracts/src/tests/windows.rs), [contracts/src/tests/config_timelock.rs](../contracts/src/tests/config_timelock.rs)
- Good first tasks: add a boundary test for ledger timing or adjust a validation message.
- Advanced tasks: introduce new time-based config parameters or fairness controls around round close.

### Statistics, archive, and query paths
- Primary files: [contracts/src/queries.rs](../contracts/src/queries.rs), [contracts/src/types.rs](../contracts/src/types.rs), [contracts/src/tests/leaderboard.rs](../contracts/src/tests/leaderboard.rs)
- Key tests: [contracts/src/tests/leaderboard.rs](../contracts/src/tests/leaderboard.rs), [contracts/src/tests/archive_retention.rs](../contracts/src/tests/archive_retention.rs)
- Good first tasks: add a query field, improve a pagination edge case, or update archive retention docs.
- Advanced tasks: add richer historical metrics or new archive query endpoints.

### Events, storage, and safety
- Primary files: [contracts/src/errors.rs](../contracts/src/errors.rs), [contracts/src/admin.rs](../contracts/src/admin.rs), [contracts/src/tests/security.rs](../contracts/src/tests/security.rs)
- Key tests: [contracts/src/tests/security.rs](../contracts/src/tests/security.rs), [contracts/src/tests/guard_tests.rs](../contracts/src/tests/guard_tests.rs)
- Good first tasks: add a new guardrail test or validate an error path.
- Advanced tasks: harden pause handling, timelocks, or schema migration logic.

## Starter paths by experience level
- Beginner: pick one window-related test in [contracts/src/tests/windows.rs](../contracts/src/tests/windows.rs) and extend it.
- Intermediate: inspect [contracts/src/config.rs](../contracts/src/config.rs) and add a new admin-config path with matching tests.
- Advanced: follow the settlement flow in [contracts/src/settlement.rs](../contracts/src/settlement.rs) and add a protocol invariant test.
