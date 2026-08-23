# Threat Model: Authorization-Zone Intents for Third-Party Keepers

## 1. Overview

The authorization-zone intent/keeper framework (Issue #370) allows users to delegate operational contract execution (such as submitting oracle settlements, claiming pending winnings, or initializing next rounds) to third-party automation keepers without transferring funds or custody of user assets.

---

## 2. Assets and Trust Boundaries

### Assets
1. **User Funds / Balances**: vXLM balances and pending winnings.
2. **Contract State**: Active rounds, pool states, and configuration parameters.
3. **Intent Nonce States**: Sequence counter tracking valid vs consumed intent nonces per user and scope.

### Trust Assumptions
- **Contract Integrity**: Soroban VM correctly enforces `require_auth()` for signatures and storage isolation.
- **Custody Assumption**: Winnings claimed on behalf of a user MUST flow exclusively to `DataKeyScoped::Balance(user)`. Keepers have NO access to user funds.

---

## 3. Threat Scenarios and Mitigations

### 3.1 Replay Attacks (Re-submitting Consumed Intents)
- **Threat**: A rogue keeper or observer captures a signed intent from a past ledger and re-submits it to execute the action a second time.
- **Mitigation**:
  - Each intent contains a unique monotonic `nonce` assigned to `(user, scope)`.
  - When an intent is executed, its status is permanently updated to `Consumed` and a storage tombstone `IntentKey::ConsumedIntentNonce(user, scope, nonce)` is written with maximum storage TTL.
  - Subsequent execution attempts fail with `ContractError::IntentAlreadyConsumed`.

### 3.2 Privilege Escalation (Scope Bypassing)
- **Threat**: A keeper authorized for low-risk actions (e.g. `KeeperScope::Resolve`) attempts to use the intent to invoke higher-risk functions (e.g., claiming winnings or creating rounds).
- **Mitigation**:
  - Scopes are explicitly enumerated in `KeeperScope`: `Resolve` (0), `Claim` (1), `CreateNext` (2).
  - Each entry point (`execute_keeper_resolve`, `execute_keeper_claim`, `execute_keeper_create_next`) enforces an exact match between the storage intent's scope and the invoked function.
  - Cross-scope calls fail with `ContractError::IntentScopeMismatch` or `ContractError::IntentNotFound`.

### 3.3 Fund Diversion / Custody Theft
- **Threat**: A keeper executes `execute_keeper_claim` hoping to divert user winnings into the keeper's address.
- **Mitigation**:
  - `execute_keeper_claim` routes directly into `crate::settlement::claim_winnings(env, user)`.
  - The destination address is strictly hardcoded to `user`. Winnings are credited to `DataKeyScoped::Balance(user)` and cannot be overridden by caller parameters.

### 3.4 Stale / Delayed Intent Execution
- **Threat**: A keeper holds a signed intent for a long duration and executes it under unfavorable market conditions.
- **Mitigation**:
  - Intents carry an explicit `expires_at_ledger` field set at creation (`authorized_at_ledger + expiry_ledgers`).
  - Expiry is capped between `MIN_INTENT_EXPIRY_LEDGERS` (6 ledgers / ~30 s) and `MAX_INTENT_EXPIRY_LEDGERS` (1,036,800 ledgers / ~60 days).
  - At execution time, `env.ledger().sequence() > expires_at_ledger` is checked; expired intents fail with `ContractError::IntentExpired`.
  - Users can explicitly revoke any active intent prior to execution via `revoke_keeper_intent`.

### 3.5 Malicious / Unregistered Keeper Execution
- **Threat**: An arbitrary address executes user intents on public networks without authorization.
- **Mitigation**:
  - The intent explicitly names `keeper: Address`. `execute_keeper_*` requires `keeper.require_auth()`. An attacker cannot execute an intent naming another keeper.
  - Additionally, admins can enable `set_keeper_registration_required(true)` to restrict execution exclusively to keepers allowlisted via `register_keeper`. Unregistered keepers fail with `ContractError::KeeperNotRegistered`.

### 3.6 Pause and Emergency Respect
- **Threat**: A keeper attempts to execute intents while the contract is emergency-paused.
- **Mitigation**:
  - All entry points invoke `_ensure_not_paused(&env)` prior to state changes.
  - Fully paused runtime mode (`RuntimeMode::FullyPaused`) blocks all keeper executions with `ContractError::ContractPaused`.

---

## 4. Acceptance Criteria Checklist

- [x] Expired/replayed intents rejected (`IntentExpired`, `IntentAlreadyConsumed`).
- [x] Scope cannot escalate privileges (`KeeperScope` strict checking).
- [x] User funds movement still user-authorized where required (claims route strictly to user balance).
- [x] Keeper happy paths tested (`intents.rs` test suite).
- [x] Threat model documented (`docs/INTENT_THREAT_MODEL.md`).
