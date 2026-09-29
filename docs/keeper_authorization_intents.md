# Keeper authorization and intent integrity

## Scope

This note documents the authorization model around keeper-driven flows such as `create_next_from_template`, oracle settlement (`resolve_round` and `resolve_round_multi`), and user-initiated payouts (`claim_winnings`). The contract depends on explicit `require_auth()` checks rather than a global trust model.

## Delegation constraints

1. Admin-authenticated keeper actions are restricted to the admin address configured at initialization.
2. Oracle settlement is restricted to the configured oracle address, not any arbitrary caller.
3. User payout paths require the claimant's own address to be present in the auth envelope.
4. The round creation and settlement logic applies its checks before any state mutation, preventing an untrusted keeper from bypassing the mutator boundary.

## Threat model

The primary threat is privilege delegation: a party can call into the contract only if it is the specific address the contract expects for that role. The design intentionally avoids a catch-all auth policy such as `env.mock_all_auths()` in tests, because that would mask subtle auth drift. The relevant authorization checks are bound to the correct contract entry points and remain in place before the contract commits any state mutation.

## Operational guidance

- Treat `create_next_from_template` as an admin-controlled keeper convenience action, not a generalized public settlement endpoint.
- Keep `resolve_round` and `resolve_round_multi` scoped to the configured oracle key and the current round binding.
- Require the claimant identity on `claim_winnings` so an unauthorized third party cannot trigger a payout.
