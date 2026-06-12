# Compatibility Policy

This document defines the protocol-level semantic versioning policy for Xelma contract ABI, storage layout, and event schema changes. It gives contributors and integrators clear rules for classifying impact and communicating migration expectations.

---

## Versioning Model

Xelma follows [Semantic Versioning 2.0](https://semver.org):

| Version component | When to bump | Trigger examples |
|---|---|---|
| **MAJOR** | Breaking change | Removed/renamed public entry point, storage key removed or retyped, event topic changed |
| **MINOR** | Backward-compatible addition | New public entry point, new optional storage key, new event topic alongside existing ones |
| **PATCH** | Bug fix with no observable ABI/storage/event change | Internal logic correction, bounds tightened, error message clarified |

---

## What Counts as a Breaking Change

### ABI (contract entry points)
| Change | Breaking? |
|---|---|
| Remove a public function | Yes — MAJOR |
| Rename a public function | Yes — MAJOR |
| Add or remove a required parameter | Yes — MAJOR |
| Add an optional parameter (with sensible default) | No — MINOR |
| Add a new public function | No — MINOR |
| Change a return type | Yes — MAJOR |

### Storage layout
| Change | Breaking? |
|---|---|
| Remove a `DataKey` variant | Yes — MAJOR |
| Rename a `DataKey` variant | Yes — MAJOR |
| Change the value type stored under an existing key | Yes — MAJOR |
| Add a new `DataKey` variant | No — MINOR |
| Add a new field to a stored struct (with serialization compat) | See note |

> **Note on struct field additions:** Soroban encodes contract types as XDR. Adding a field to a `#[contracttype]` struct changes its XDR encoding and is therefore a MAJOR change unless the old encoding can still be decoded (e.g. via a migration path documented in `MIGRATION.md`).

### Events
| Change | Breaking? |
|---|---|
| Change an existing event topic string | Yes — MAJOR |
| Remove an existing event | Yes — MAJOR |
| Add a new event | No — MINOR |
| Add a field to an existing event payload | Yes — MAJOR (consumers expecting fixed arity will break) |
| Remove a field from an existing event payload | Yes — MAJOR |

### Errors
| Change | Breaking? |
|---|---|
| Remove a `ContractError` variant | Yes — MAJOR |
| Renumber a `ContractError` variant | Yes — MAJOR |
| Add a new `ContractError` variant | No — MINOR |
| Change which error a code path returns | MAJOR if callers check the specific variant |

---

## Release Checklist

Before merging a PR that includes contract changes, the author must annotate their diff against this matrix:

- [ ] Identify every ABI, storage, or event change in the PR.
- [ ] Classify each change as MAJOR / MINOR / PATCH using the tables above.
- [ ] Set the PR title prefix accordingly: `feat!` (MAJOR), `feat` (MINOR), `fix` (PATCH).
- [ ] Update `Cargo.toml` version to reflect the highest-severity change.
- [ ] If MAJOR: add a migration section to `MIGRATION.md` describing how existing deployments should upgrade.
- [ ] If MAJOR: confirm that bindings are regenerated (`npm run build` in `bindings/`) and parity tests pass (`npm run test:parity`).

---

## Common Scenarios

### Adding a new validation error to an existing function

**Example:** `create_round` now returns `StartPriceTooHigh` instead of `InvalidPrice` for oversized prices.

**Classification:** MAJOR — callers that match `InvalidPrice` will no longer match the new path.

**Migration:** Document the new variant in release notes; integrators must handle the new error code.

---

### Adding a new optional config entry point (`set_max_stake`)

**Classification:** MINOR — new callable function, no existing behavior changed.

**Migration:** None required. Integrators may opt in.

---

### Adding a one-sided pool event (`pool/onesided`)

**Classification:** MINOR — new event emitted in a previously silent code path; existing event listeners are unaffected.

**Migration:** None required. Indexers may subscribe to the new topic.

---

### Removing the legacy `UpDownPositions` storage key

**Classification:** MAJOR — any reader relying on the legacy key will find it absent.

**Migration:** Document the cutover ledger height. Integrators must migrate before that height.

---

## Maintainer Notes

- The canonical ABI surface is the set of `pub fn` items in `contracts/src/contract.rs`.
- The canonical storage schema is the `DataKey` enum in `contracts/src/types.rs`.
- The canonical event schema is the `env.events().publish(…)` calls in `contracts/src/contract.rs`, with topics and payloads documented inline.
- When in doubt, treat a change as MAJOR. It is cheaper to bump a version than to strand an integrator.
