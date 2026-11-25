# Participant Access Control

Optional admission control for private and hackathon deployments (Issues #274,
#838, #842). The feature is inert until an admin configures it.

## Defaults

Deployments are **open**: until `set_access_control_enabled(true)` is called,
every address may call `mint_initial` and every betting entrypoint, subject only
to the normal amount / cap / mode / window rules.

## Policy matrix

The gate is `_enforce_access_control` in `contracts/src/access_control.rs` and
runs after the schema/pause checks and before any balance or pool write:

| Allowlist mode | Allowlisted | Denylisted | Result              |
|:--------------:|:-----------:|:----------:|---------------------|
| any            | any         | yes        | `AccessDenied` (79) |
| off (open)     | any         | no         | admitted            |
| on             | yes         | no         | admitted            |
| on             | no          | no         | `AccessDenied` (79) |

Gated entrypoints: `mint_initial`, `place_bet`, `place_precision_prediction`,
`predict_price`, `commit_prediction`, `reveal_prediction`, `cash_out_early`.

## Why open mode still blocks denylisted users

Open mode means "no allowlist required" — it is not a denylist bypass. The
denylist is an emergency block (ban / exploit response) and always takes
precedence, so a denied address is refused even when allowlist mode is off. An
address carrying both markers is treated as denylisted.

## Unblocking a denied address

There is no mode toggle that restores access. Two admin-only calls do:

1. **`remove_denylisted(address)`** — clears the ban. The address resolves back
   to `Open` and can participate immediately in open mode.
2. **`add_allowlisted(address)`** — clears the conflicting deny marker and adds
   the address to the allowlist, so it can participate in both open and
   allowlist mode.

Disabling allowlist mode (`set_access_control_enabled(false)`) does **not**
clear denylist entries. Removing an address that is not on a list is an
idempotent no-op that emits no event.

## Admin entrypoints

| Entrypoint | Effect |
|---|---|
| `set_access_control_enabled(enabled)` | Turn allowlist mode on/off |
| `add_allowlisted(address)` | Allowlist an address; clears a stale deny marker |
| `remove_allowlisted(address)` | Remove from the allowlist (idempotent) |
| `add_denylisted(address)` | Ban an address; clears a stale allow marker |
| `remove_denylisted(address)` | Unban an address (idempotent) |

Every mutation calls `admin.require_auth()` and is rejected while the contract is
paused.

## Read-only queries

| Query | Returns |
|---|---|
| `is_access_control_enabled()` | `true` when allowlist mode is on |
| `is_allowlisted(address)` | Raw allowlist marker |
| `is_denylisted(address)` | Raw denylist marker |
| `get_access_state(address)` | `Open`, `Allowlisted`, or `Denylisted` (deny wins) |
| `get_access_policy(address)` | `(allowlist_enabled, access_state)` |
| `get_protocol_health()` | Reports `ACCESS_RESTRICTED` (7) while allowlist mode is on |

## Events

All mutations publish under the `("access", <detail>)` topic with
`(address, reconciled)` data, where `reconciled` is `true` when the write also
cleared a conflicting marker on the other list:

| Event | Emitted when |
|---|---|
| `("access", "mode")` | Allowlist mode toggled |
| `("access", "allow_add")` / `("access", "allow_rm")` | Allowlist mutation |
| `("access", "deny_add")` / `("access", "deny_rm")` | Denylist mutation |

## Error

Both rejection paths return `AccessDenied` (79). Wallets should present it as
"Participation is restricted for this address" and point the user at the
operator, who can inspect `get_access_state` and, if appropriate, clear the
denylist entry.

## Test coverage

`contracts/src/tests/access_control.rs` covers the default-open behavior, the
denylist-wins rule (including both markers present), open-mode blocking, the
unblock paths, admin-only mutations, events, cash-out gating, and the shared
mint/bet admission matrix (#842, #839).
