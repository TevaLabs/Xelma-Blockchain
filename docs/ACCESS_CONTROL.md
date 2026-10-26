# Participant Access Control

Optional participant gating for private / hackathon deployments (Issue #274).
The contract is **open by default**: deploying the feature changes nothing until
an admin explicitly enables allowlist mode or denylists an address.

Source of truth: [`contracts/src/access_control.rs`](../contracts/src/access_control.rs).

## Modes

| Allowlist mode (`is_access_control_enabled`) | User on denylist | User on allowlist | Neither |
| --- | --- | --- | --- |
| `false` (**Open**) | ❌ `AccessDenied` | ✅ admitted | ✅ admitted |
| `true` (**Allowlist**) | ❌ `AccessDenied` | ✅ admitted | ❌ `AccessDenied` |

Two independent pieces of state decide the outcome:

- `AccessControlEnabled` — a single admin-owned on/off switch for allowlist mode.
- Per-address `Allowlisted` / `Denylisted` markers.

## Open mode does **not** bypass the denylist

This is intentional and is the most common source of confusion (Issue #706):

> **Open mode means "no allowlist", not "no access control".**

A denylisted address is refused on **every** participating entrypoint regardless
of the mode: `mint_initial`, `place_bet`, `place_precision_prediction`,
`commit_prediction`, `reveal_prediction`, and `cash_out_early`. The denylist is
an emergency ban/block that must work on open deployments too, so it is checked
*before* the allowlist-mode switch and always wins. The address cannot "get
around" it by waiting for the mode to be toggled off, because open mode blocks
it as well.

The denylist also wins over the allowlist: `add_denylisted` clears a conflicting
`Allowlisted` marker, so the two lists never disagree.

## The only bypass is an admin mutation

`AccessDenied` (code `79`) is returned until an admin explicitly changes policy:

- `remove_denylisted(user)` — lifts the ban; idempotent and the intended path
  for a mistaken or temporary block.
- `add_allowlisted(user)` — also reconciles state by clearing a stale denylist
  marker, so an allowlisted address is admitted as soon as the deny marker is
  gone.

Both entrypoints require `admin.require_auth()`, are blocked while the contract
is paused, and emit an `("access", "deny_rm" | "allow_add")` event.

A denied user cannot self-serve, and there is no "open mode" flag to flip from
the participant side. If a deployment wants no enforcement at all, the admin
must remove the address from the denylist (the allowlist switch alone is not
enough while a deny marker exists).

## Admin entrypoints

| Entrypoint | Effect |
| --- | --- |
| `set_access_control_enabled(enabled)` | Toggle allowlist mode. `false` removes the flag (open mode). |
| `add_allowlisted(user)` | Admit `user`; clears a conflicting deny marker. |
| `remove_allowlisted(user)` | Remove from the allowlist; no-op if absent. |
| `add_denylisted(user)` | Ban `user`; clears a conflicting allow marker. |
| `remove_denylisted(user)` | Lift the ban; no-op if absent. |

## Queries

- `is_access_control_enabled()` → allowlist mode on/off.
- `is_allowlisted(user)` / `is_denylisted(user)` → raw markers.
- `get_access_state(user)` → `Open` / `Allowlisted` / `Denylisted` (denylist wins).
- `get_access_policy(user)` → `(allowlist_enabled, state)`.
- `get_protocol_health()` reports status `7 (ACCESS_RESTRICTED)` whenever
  allowlist mode is enabled and everything else is healthy.

## Operational example

```bash
# Open deployment, ban a compromised key.
stellar contract invoke --id $CID --source admin --network testnet \
  -- add_denylisted --user G...

# The same address is still refused with open mode:
stellar contract invoke ... -- place_bet --user G... --amount 10000000 --side Up
# → AccessDenied (79)

# Unblock after the incident is resolved.
stellar contract invoke ... -- remove_denylisted --user G...
```

Tests covering these rules live in
[`contracts/src/tests/access_control.rs`](../contracts/src/tests/access_control.rs),
including `test_denylist_blocks_bet_in_open_mode` and
`test_open_mode_denylist_has_no_bypass`.
