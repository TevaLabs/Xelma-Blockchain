# Implementation Plan: Exhaustive Pause Policy Matrix (Issue #880)

This document provides the complete analysis, pause policy matrix, proposed test plan, and documentation plan for Issue #880: *"fully document the pause policy matrix by testing every pause mode against every contract action, so the behavior of the pause system is unambiguous and machine-verified."*

---

## Section 1: Findings

### 1.1 Pause Modes and Behavioral Semantics

The contract defines three runtime modes in `contracts/src/types.rs` (`RuntimeMode`), persisted under the storage key `DataKeyCore::Paused`:

| Mode Enum | Value | Name | Behavioral Semantics |
|---|:---:|---|---|
| `RuntimeMode::Normal` | `0` | **Normal** | The standard operational mode of the contract. All mutating operations across all policy classes (`RoundMutation`, `Claim`, `Settlement`, `AdminConfig`) are **ALLOWED**. Bets and reveals can be accepted (if a round is active and within window), claims are processed, oracle settlements execute, and admin configs can be applied. |
| `RuntimeMode::ClaimsOnly` | `1` | **ClaimsOnly** | Degraded / emergency incident mode (e.g. market pause or resolution phase). All `RoundMutation` actions are **BLOCKED** with `ContractError::ContractPaused`. Users cannot mint initial tokens, place bets, make precision predictions, commit/reveal predictions, or cash out early. Activating timelocked config changes (`apply_scheduled_changes`) is also BLOCKED. However, `Claim` actions (`claim_winnings`, `claim_many`), `Settlement` actions (`resolve_round`, `resolve_round_multi`, `void_round`, `finalize_round`, `cancel_round`), and `AdminConfig` actions (`create_round`, `set_windows`, etc.) remain **ALLOWED** so users can withdraw winnings, in-flight rounds can be resolved/refunded, and admins can configure or restart rounds. |
| `RuntimeMode::FullyPaused` | `2` | **FullyPaused** | Complete circuit-breaker / emergency stop. Every state-mutating entrypoint across all four policy classes (`RoundMutation`, `Claim`, `Settlement`, `AdminConfig`) is **BLOCKED** with `ContractError::ContractPaused`. Only mode-transition controls (`pause_contract`, `unpause_contract`, `set_runtime_mode`), oracle heartbeat reporting (`update_oracle_heartbeat`), and read-only queries are **ALLOWED** so operators can diagnose and exit an incident. |

---

### 1.2 Pause-Gated Contract Actions and Guard Logic

The contract enforces mode gating via the central policy gate in `contracts/src/admin.rs`:

```rust
pub fn _policy_gate(env: &Env, action: PolicyAction) -> Result<(), ContractError> {
    let mode = _current_mode(env);
    let blocked = match action {
        PolicyAction::RoundMutation => mode != RuntimeMode::Normal,
        PolicyAction::Claim | PolicyAction::AdminConfig | PolicyAction::Settlement => {
            mode == RuntimeMode::FullyPaused
        }
    };
    if blocked {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}
```

Two convenience wrappers dispatch through this gate:
- `_ensure_normal_mode(env)` $\rightarrow$ `_policy_gate(env, PolicyAction::RoundMutation)`: Blocks when `mode != RuntimeMode::Normal`.
- `_ensure_not_paused(env)` $\rightarrow$ `_policy_gate(env, PolicyAction::AdminConfig)`: Blocks when `mode == RuntimeMode::FullyPaused`.

#### Action Inventory by Policy Class

1. **`PolicyAction::RoundMutation`** (Blocked in `ClaimsOnly` and `FullyPaused`; Allowed in `Normal`):
   - `place_bet` (`betting.rs:304` via `_ensure_normal_mode`)
   - `place_precision_prediction` (`betting.rs:449` via `_ensure_normal_mode`)
   - `predict_price` (`betting.rs:1036` via `place_precision_prediction` $\rightarrow$ `_ensure_normal_mode`)
   - `commit_prediction` (`betting.rs:584` via `_ensure_normal_mode`)
   - `reveal_prediction` (`betting.rs:691` via `_ensure_normal_mode`)
   - `mint_initial` (`betting.rs:940` via `_ensure_normal_mode`)
   - `apply_scheduled_changes` (`config.rs:276` via `_ensure_normal_mode`)
   - `cash_out_early` (`betting.rs:798` via `_ensure_normal_mode`)

2. **`PolicyAction::Claim`** (Blocked in `FullyPaused`; Allowed in `Normal` and `ClaimsOnly`):
   - `claim_winnings` (`settlement.rs:374` via `_ensure_not_paused`)
   - `claim_many` (`settlement.rs:451` via `_ensure_not_paused`)

3. **`PolicyAction::Settlement`** (Blocked in `FullyPaused`; Allowed in `Normal` and `ClaimsOnly`):
   - `resolve_round` (`settlement.rs:516` via `_ensure_not_paused`)
   - `resolve_round_multi` (`settlement.rs:872` via `_ensure_not_paused`)
   - `void_round` (`settlement.rs:1401` via `_ensure_not_paused`)
   - `finalize_round` (`settlement.rs:1504` via `_ensure_not_paused`)
   - *`cancel_round`*: Documented under `Settlement` in `admin.rs` doc comments and `docs/STATUS_CODES.md`, but see Section 1.5 finding regarding implementation.

4. **`PolicyAction::AdminConfig`** (Blocked in `FullyPaused`; Allowed in `Normal` and `ClaimsOnly`):
   - Core round management: `create_round`, `create_next_from_template`, `set_round_template`, `clear_round_template`.
   - Windows & limits: `set_windows`, `schedule_windows`, `set_close_buffer_ledgers`, `set_max_stake`, `schedule_max_stake`, `set_min_bet`, `schedule_min_bet`, `set_max_user_exposure`, `schedule_max_user_exposure`, `set_max_pending_winnings`, `schedule_max_pending_winnings`, `set_min_participants`, `set_max_precision_participants`, `set_precision_payout_policy`, `set_mint_limit`, `set_epoch_mint_budget`, `set_archive_retention`, `set_pending_winnings_expiry`, `schedule_pending_winnings_expiry`, `reclaim_expired_pending_winnings`.
   - Oracle & heartbeat: `set_oracle_max_deviation_bps`, `schedule_oracle_deviation_bps`, `set_deviation_ref_mode`, `set_attestation_key`, `arm_oracle_deviation_override`, `set_oracle_min_confidence_bps`, `set_oracle_strict_mode`, `set_hb_strict_mode`, `arm_hb_override`, `set_hb_grace_seconds`, `set_oracle_stale_threshold`, `schedule_oracle_stale_threshold`, `schedule_oracle_timestamp_skew`, `set_oracle_quorum_config`, `propose_oracle_rotation`, `accept_oracle_rotation`, `cancel_oracle_rotation`.
   - Fees & treasury: `set_protocol_fee_bps`, `schedule_protocol_fee_bps`, `withdraw_protocol_fee`, `set_fee_model`, `set_early_cashout_bps`.
   - Timelock management: `cancel_config_change`.
   - Dispute & insurance: `set_dispute_ledgers`, `set_insurance_split_bps`, `set_insurance_coverage_bps`, `set_insurance_eligible_events`, `top_up_insurance_fund`, `withdraw_insurance_fund`.
   - Access control: `set_access_control_enabled`, `add_allowlisted`, `remove_allowlisted`, `add_denylisted`, `remove_denylisted`.
   - Season & schema: `reset_leaderboard_season`, `migrate_schema_v1_to_v2`, `migrate_schema_v2_to_v3`, `batch_touch_ttl`.

5. **Mode-Transition Controls** (Allowed in **ALL** modes — Emergency Exit Path):
   - `set_runtime_mode`, `pause_contract`, `unpause_contract`, `is_paused`, `get_runtime_mode`, `is_action_allowed`.
   - *Guard rule*: Must bypass `_policy_gate` entirely so operators can transition modes and escape an incident.

6. **Read-Only Queries & Heartbeat** (Allowed in **ALL** modes):
   - `update_oracle_heartbeat`, `get_protocol_health`, `get_protocol_status`, `get_round_status`, `balance`, `get_active_round`, `get_market_snapshot`, `simulate_payout`, and all getter queries (`get_*`).

---

### 1.3 Full Mode $\times$ Action Matrix Table

| Contract Action | Category / Policy Class | Normal (`0`) | ClaimsOnly (`1`) | FullyPaused (`2`) | Expected Block Error |
|---|---|:---:|:---:|:---:|---|
| `place_bet` | RoundMutation | **ALLOWED** | **BLOCKED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `place_precision_prediction` | RoundMutation | **ALLOWED** | **BLOCKED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `predict_price` | RoundMutation | **ALLOWED** | **BLOCKED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `commit_prediction` | RoundMutation | **ALLOWED** | **BLOCKED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `reveal_prediction` | RoundMutation | **ALLOWED** | **BLOCKED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `mint_initial` | RoundMutation | **ALLOWED** | **BLOCKED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `apply_scheduled_changes` | RoundMutation | **ALLOWED** | **BLOCKED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `cash_out_early` | RoundMutation | **ALLOWED** | **BLOCKED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `claim_winnings` | Claim | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `claim_many` | Claim | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `resolve_round` | Settlement | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `resolve_round_multi` | Settlement | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `void_round` | Settlement | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `finalize_round` | Settlement | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `cancel_round` *(see finding 1.5)* | Settlement (Admin) | **ALLOWED** | **ALLOWED** | **ALLOWED** *(code)* / **BLOCKED** *(spec)* | `ContractError::ContractPaused` (22) |
| `create_round` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `create_next_from_template` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_windows` / `schedule_windows` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_max_stake` / `schedule_max_stake` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_min_bet` / `schedule_min_bet` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_max_user_exposure` / `schedule_*` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_max_pending_winnings` / `schedule_*` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `schedule_oracle_stale_threshold` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `schedule_oracle_deviation_bps` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `schedule_oracle_timestamp_skew` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_protocol_fee_bps` / `schedule_*` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `withdraw_protocol_fee` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `cancel_config_change` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_min_participants` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_max_precision_participants` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_precision_payout_policy` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_mint_limit` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_epoch_mint_budget` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_archive_retention` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_pending_winnings_expiry` / `schedule_*` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `reclaim_expired_pending_winnings` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_close_buffer_ledgers` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_round_template` / `clear_*` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_oracle_quorum_config` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_early_cashout_bps` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_fee_model` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_dispute_ledgers` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_oracle_max_deviation_bps` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_deviation_ref_mode` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_attestation_key` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `arm_oracle_deviation_override` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_oracle_min_confidence_bps` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_oracle_strict_mode` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_hb_strict_mode` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `arm_hb_override` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_hb_grace_seconds` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_oracle_stale_threshold` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `batch_touch_ttl` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `propose_oracle_rotation` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `accept_oracle_rotation` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `cancel_oracle_rotation` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_access_control_enabled` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `add_allowlisted` / `remove_*` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `add_denylisted` / `remove_*` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_insurance_split_bps` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_insurance_coverage_bps` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_insurance_eligible_events` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `top_up_insurance_fund` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `withdraw_insurance_fund` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `reset_leaderboard_season` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `migrate_schema_v1_to_v2` / `v2_to_v3` | AdminConfig | **ALLOWED** | **ALLOWED** | **BLOCKED** | `ContractError::ContractPaused` (22) |
| `set_runtime_mode` | Mode Control | **ALLOWED** | **ALLOWED** | **ALLOWED** | None (Incident exit path) |
| `pause_contract` / `unpause_contract` | Mode Control | **ALLOWED** | **ALLOWED** | **ALLOWED** | None (Incident exit path) |
| `is_paused` / `get_runtime_mode` | Mode Control / Query | **ALLOWED** | **ALLOWED** | **ALLOWED** | None (Query) |
| `is_action_allowed` | Mode Control / Query | **ALLOWED** | **ALLOWED** | **ALLOWED** | None (Query) |
| `update_oracle_heartbeat` | Heartbeat / Oracle | **ALLOWED** | **ALLOWED** | **ALLOWED** | None (Must flow during incident) |
| Read queries (`balance`, `get_*`, etc.) | Read-Only Queries | **ALLOWED** | **ALLOWED** | **ALLOWED** | None (Non-mutating) |

---

### 1.4 State of `contracts/src/tests/policy_gate.rs`

The current file contains 6 tests (102 lines):
1. `test_policy_gate_normal_mode_allows_everything`: Calls `client.is_action_allowed` for all 4 `PolicyAction` enum variants in Normal mode.
2. `test_policy_gate_claims_only_blocks_only_round_mutation`: Calls `client.is_action_allowed` in ClaimsOnly mode.
3. `test_policy_gate_fully_paused_blocks_everything`: Calls `client.is_action_allowed` in FullyPaused mode.
4. `test_policy_gate_matches_actual_entrypoint_behaviour_round_mutation`: Calls `client.try_place_bet` (asserting `result.is_err()`) and `client.create_round` (asserting allowed) in ClaimsOnly mode.
5. `test_policy_gate_admin_config_still_allowed_in_claims_only`: Calls `client.pause_contract` in ClaimsOnly mode.
6. `test_policy_gate_transitions_round_trip`: Checks transitions between modes 0 $\rightarrow$ 1 $\rightarrow$ 2 $\rightarrow$ 0.

#### Missing Coverage & Gaps
- **Normal mode**: No actual entrypoint calls are tested (only `is_action_allowed`).
- **ClaimsOnly mode**:
  - `RoundMutation`: Only `place_bet` is tested. Missing: `place_precision_prediction`, `predict_price`, `commit_prediction`, `reveal_prediction`, `mint_initial`, `apply_scheduled_changes`, `cash_out_early`.
  - Error assertion in test 4 only checks `result.is_err()`, not the exact error variant `Err(Ok(ContractError::ContractPaused))`.
  - `Claim`: No claim entrypoint (`claim_winnings`, `claim_many`) is verified to succeed in ClaimsOnly.
  - `Settlement`: No settlement entrypoint (`resolve_round`, `resolve_round_multi`, `void_round`, `finalize_round`) is verified to be allowed in ClaimsOnly.
  - `AdminConfig`: Only `create_round` and `pause_contract` are tested.
- **FullyPaused mode**:
  - **Zero actual entrypoint calls are tested**; only `is_action_allowed` is tested.
  - Not a single test asserts that `RoundMutation`, `Claim`, `Settlement`, or `AdminConfig` entrypoints return `Err(Ok(ContractError::ContractPaused))`.
  - No test asserts that mode transitions (`unpause_contract`, `set_runtime_mode`) and read queries remain allowed in FullyPaused.

---

### 1.5 Ambiguities and Key Findings for Maintainer Decision

1. **`cancel_round` Gating Discrepancy**:
   - In `contracts/src/admin.rs` doc comments (line 926) and `docs/STATUS_CODES.md` (line 230), `cancel_round` is documented under `Settlement`, which is specified as blocked in `FullyPaused`.
   - In `contracts/src/settlement.rs` line 269, however, `pub fn cancel_round(...)` does **not** call `_ensure_not_paused(&env)` or `_policy_gate`. It only checks schema version and admin auth (`admin.require_auth()`).
   - Consequently, in the production contract code, `cancel_round` is currently **callable even when `FullyPaused`**.
   - *Maintainer decision required*: Should `cancel_round` be kept as callable in `FullyPaused` (acting like an emergency admin unwind tool, similar to mode controls), or should production contract logic eventually be updated? Per the scope rules, production contract logic will **not** be modified, but tests and documentation must accurately describe this exact behavior.

2. **Existing Test Distribution vs Scope of Issue #880**:
   - `contracts/src/tests/pause_policy_matrix.rs` (Issue #402) tests 41 entrypoints under `AdminConfig`.
   - `contracts/src/tests/drill.rs` contains drill scenarios for `ClaimsOnly` and `FullyPaused`.
   - Per the user prompt scope rules, changes must be constrained to `contracts/src/tests/policy_gate.rs` and documentation files. `contracts/src/tests/policy_gate.rs` should be expanded to systematically test every pause mode against every contract action category and representative entrypoints with exact error code assertions.

---

## Section 2: Proposed Test Plan

We propose expanding `contracts/src/tests/policy_gate.rs` with organized, exhaustive test suites.

### 2.1 Test Grouping and Naming Conventions

Follow the existing conventions in `contracts/src/tests/`:
- Test function prefix: `test_policy_gate_<mode>_<action_or_category>_<outcome>`
- Exact assertion pattern: `assert_eq!(res, Err(Ok(ContractError::ContractPaused)))` for blocked calls, and `assert!(res.is_ok())` or `assert_ne!(res, Err(Ok(ContractError::ContractPaused)))` for allowed calls.
- Setup helper: reuse existing `setup(&env) -> (VirtualTokenContractClient<'_>, Address, Address)`.

### 2.2 Explicit New Test Cases to Add

#### Suite 1: `Normal` Mode (Mode 0) Entrypoint Verification
1. `test_policy_gate_normal_mode_allows_round_mutation_entrypoints`
   - **Mode**: `Normal` (0)
   - **Actions**: `place_bet`, `place_precision_prediction`, `predict_price`, `commit_prediction`, `reveal_prediction`, `mint_initial`, `cash_out_early`
   - **Expected Result**: **ALLOWED** (none fail with `ContractPaused`)
2. `test_policy_gate_normal_mode_allows_claim_entrypoints`
   - **Mode**: `Normal` (0)
   - **Actions**: `claim_winnings`, `claim_many`
   - **Expected Result**: **ALLOWED** (none fail with `ContractPaused`)
3. `test_policy_gate_normal_mode_allows_settlement_entrypoints`
   - **Mode**: `Normal` (0)
   - **Actions**: `resolve_round`, `resolve_round_multi`, `void_round`, `finalize_round`, `cancel_round`
   - **Expected Result**: **ALLOWED** (none fail with `ContractPaused`)
4. `test_policy_gate_normal_mode_allows_admin_config_entrypoints`
   - **Mode**: `Normal` (0)
   - **Actions**: `create_round`, `set_windows`, `set_oracle_max_deviation_bps`, `set_max_stake`, `withdraw_protocol_fee`
   - **Expected Result**: **ALLOWED** (none fail with `ContractPaused`)

#### Suite 2: `ClaimsOnly` Mode (Mode 1) Entrypoint Verification
5. `test_policy_gate_claims_only_blocks_every_round_mutation_entrypoint`
   - **Mode**: `ClaimsOnly` (1)
   - **Actions**:
     - `place_bet` $\rightarrow$ **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
     - `place_precision_prediction` $\rightarrow$ **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
     - `predict_price` $\rightarrow$ **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
     - `commit_prediction` $\rightarrow$ **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
     - `reveal_prediction` $\rightarrow$ **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
     - `mint_initial` $\rightarrow$ **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
     - `apply_scheduled_changes` $\rightarrow$ **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
     - `cash_out_early` $\rightarrow$ **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
6. `test_policy_gate_claims_only_allows_claim_entrypoints`
   - **Mode**: `ClaimsOnly` (1)
   - **Actions**: `claim_winnings`, `claim_many`
   - **Expected Result**: **ALLOWED** (users can claim winnings; never returns `ContractPaused`)
7. `test_policy_gate_claims_only_allows_settlement_entrypoints`
   - **Mode**: `ClaimsOnly` (1)
   - **Actions**: `resolve_round`, `cancel_round`, `void_round`, `finalize_round`
   - **Expected Result**: **ALLOWED** (settlements complete or fail for domain reasons, never `ContractPaused`)
8. `test_policy_gate_claims_only_allows_admin_config_entrypoints`
   - **Mode**: `ClaimsOnly` (1)
   - **Actions**: `create_round`, `set_windows`, `set_max_stake`, `withdraw_protocol_fee`, `cancel_config_change`
   - **Expected Result**: **ALLOWED** (never returns `ContractPaused`)

#### Suite 3: `FullyPaused` Mode (Mode 2) Entrypoint Verification
9. `test_policy_gate_fully_paused_blocks_round_mutation_entrypoints`
   - **Mode**: `FullyPaused` (2)
   - **Actions**: `place_bet`, `place_precision_prediction`, `predict_price`, `commit_prediction`, `reveal_prediction`, `mint_initial`, `apply_scheduled_changes`, `cash_out_early`
   - **Expected Result**: **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
10. `test_policy_gate_fully_paused_blocks_claim_entrypoints`
    - **Mode**: `FullyPaused` (2)
    - **Actions**: `claim_winnings`, `claim_many`
    - **Expected Result**: **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
11. `test_policy_gate_fully_paused_blocks_settlement_entrypoints`
    - **Mode**: `FullyPaused` (2)
    - **Actions**: `resolve_round`, `resolve_round_multi`, `void_round`, `finalize_round`
    - **Expected Result**: **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
12. `test_policy_gate_fully_paused_blocks_admin_config_entrypoints`
    - **Mode**: `FullyPaused` (2)
    - **Actions**: `create_round`, `create_next_from_template`, `set_windows`, `set_max_stake`, `set_oracle_max_deviation_bps`, `withdraw_protocol_fee`, `set_round_template`
    - **Expected Result**: **BLOCKED** with `Err(Ok(ContractError::ContractPaused))`
13. `test_policy_gate_fully_paused_allows_mode_controls_and_queries`
    - **Mode**: `FullyPaused` (2)
    - **Actions**:
      - Mode controls: `pause_contract`, `unpause_contract`, `set_runtime_mode`, `is_paused`, `get_runtime_mode`, `is_action_allowed`
      - Queries & Heartbeat: `get_protocol_status`, `get_protocol_health`, `balance`, `get_active_round`, `update_oracle_heartbeat`
    - **Expected Result**: **ALLOWED** (all succeed and do not return `ContractPaused`)

#### Suite 4: Existing Test Updates
14. Update `test_policy_gate_matches_actual_entrypoint_behaviour_round_mutation`:
    - Harden `assert!(result.is_err())` to assert the exact error variant: `assert_eq!(result, Err(Ok(ContractError::ContractPaused)))`.

---

## Section 3: Proposed Documentation Plan

### 3.1 Document Updates & Placement

1. **New Dedicated Document**:
   - Create `docs/PAUSE_POLICY_MATRIX.md`.
   - Content: The complete, machine-verified Pause Policy Matrix (copied directly from Section 1.3 above), detailing each action, its exact function signature, its policy classification, and its behavioral outcome across `Normal`, `ClaimsOnly`, and `FullyPaused`.
   - Cross-link in `docs/STATUS_CODES.md` under Section 2 ("`RuntimeMode` Entrypoint Policy Matrix") to point to `docs/PAUSE_POLICY_MATRIX.md` for the exhaustive action-by-action mapping.
   - Note the `cancel_round` behavior clearly in both documents.

2. **Verify Error Reference**:
   - `docs/CONTRACT_ERRORS.md` already registers error code 22: `| 22 | ContractPaused |`. Ensure any cross-references to the error match.

---

## Section 4: Verification Plan

### 4.1 Scoped Cargo Test Verification

All commands match the CI workflow specification in `.github/workflows/ci.yml`:

```bash
# 1. Scoped execution of policy_gate test suite
cargo test --package xelma-contract --lib tests::policy_gate --locked -- --nocapture

# 2. Regression verification across closely coupled pause/status suites
cargo test --package xelma-contract --lib tests::pause_policy_matrix --locked -- --nocapture
cargo test --package xelma-contract --lib tests::status --locked -- --nocapture
cargo test --package xelma-contract --lib tests::drill --locked -- --nocapture
cargo test --package xelma-contract --lib tests::pause --locked -- --nocapture

# 3. Security clippy audit check (mirroring CI security-audit & clippy jobs)
cargo clippy --workspace --all-targets --locked -- -D warnings

# 4. Workspace-wide test suite
cargo test --workspace --locked
```

### 4.2 Bindings & Parity Verification

```bash
# Error code parity check across Rust, TypeScript bindings, and documentation
cd bindings && npm run test:parity && npm run test:errors
```

---

## Section 5: Open Questions

1. **`cancel_round` Behavior in `FullyPaused`**:
   - `cancel_round` in `contracts/src/settlement.rs` currently lacks an `_ensure_not_paused` check, making it executable in `FullyPaused` mode while `resolve_round` and `finalize_round` are blocked.
   - **Question**: Should the test suite assert `cancel_round`'s current executable behavior in `FullyPaused` (documenting it as an emergency admin unwinding path), or should the maintainer confirm if an issue will be filed to add `_ensure_not_paused` in a subsequent protocol release?
2. **Matrix Scope in `policy_gate.rs` vs `pause_policy_matrix.rs`**:
   - `pause_policy_matrix.rs` already iterates 41 `AdminConfig` functions through a loop helper.
   - **Question**: Is it preferred for `policy_gate.rs` to test representative functions from each policy action class cleanly, or should it duplicate the exhaustive 40+ admin function loop in `policy_gate.rs` as well? (Recommended: test all 8 `RoundMutation` entrypoints, both `Claim` entrypoints, all `Settlement` entrypoints, key representative `AdminConfig` entrypoints, and all mode controls / queries in `policy_gate.rs`).
