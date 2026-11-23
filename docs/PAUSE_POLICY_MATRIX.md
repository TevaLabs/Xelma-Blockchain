# Pause Policy Matrix Reference

> Canonical reference for contract pause modes, policy classes, and entrypoint gating rules (Issue #880). Machine-verified by `contracts/src/tests/policy_gate.rs`.

---

## 1. Runtime Modes and Behavioral Semantics

The contract defines three operational runtime modes (`RuntimeMode` in `contracts/src/types.rs`), stored under the storage key `DataKeyCore::Paused`:

| Mode Enum | Value | Name | Behavioral Semantics |
|---|:---:|---|---|
| `RuntimeMode::Normal` | `0` | **Normal** | The standard operational mode of the contract. All mutating operations across all policy classes (`RoundMutation`, `Claim`, `Settlement`, `AdminConfig`) are **ALLOWED**. Bets and reveals can be accepted (if a round is active and within window), claims are processed, oracle settlements execute, and admin configs can be applied. |
| `RuntimeMode::ClaimsOnly` | `1` | **ClaimsOnly** | Degraded / emergency incident mode (e.g. market pause or resolution phase). All `RoundMutation` actions are **BLOCKED** with `ContractError::ContractPaused`. Users cannot mint initial tokens, place bets, make precision predictions, commit/reveal predictions, or cash out early. Activating timelocked config changes (`apply_scheduled_changes`) is also BLOCKED. However, `Claim` actions (`claim_winnings`, `claim_many`), `Settlement` actions (`resolve_round`, `resolve_round_multi`, `void_round`, `finalize_round`, `cancel_round`), and `AdminConfig` actions (`create_round`, `set_windows`, etc.) remain **ALLOWED** so users can withdraw winnings, in-flight rounds can be resolved/refunded, and admins can configure or restart rounds. |
| `RuntimeMode::FullyPaused` | `2` | **FullyPaused** | Complete circuit-breaker / emergency stop. Every state-mutating entrypoint across all four policy classes (`RoundMutation`, `Claim`, `Settlement`, `AdminConfig`) is **BLOCKED** with `ContractError::ContractPaused`. Only mode-transition controls (`pause_contract`, `unpause_contract`, `set_runtime_mode`), oracle heartbeat reporting (`update_oracle_heartbeat`), and read-only queries are **ALLOWED** so operators can diagnose and exit an incident. |

---

## 2. Policy Classes and Enforcement Mechanism

State-mutating entrypoints are grouped into four policy action classes (`PolicyAction` in `contracts/src/types.rs`). Every mutating method funnels its mode check through `admin::_policy_gate` rather than maintaining bespoke per-entrypoint mode checks:

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

Two internal helper functions wrap `_policy_gate`:
- `_ensure_normal_mode(env)` $\rightarrow$ `_policy_gate(env, PolicyAction::RoundMutation)`: Enforces that the contract is strictly in `RuntimeMode::Normal`.
- `_ensure_not_paused(env)` $\rightarrow$ `_policy_gate(env, PolicyAction::AdminConfig)`: Enforces that the contract is not in `RuntimeMode::FullyPaused`.

### Policy Classes

1. **`RoundMutation`**: Entrypoints that create new market exposure or alter round participation (`place_bet`, `place_precision_prediction`, `predict_price`, `commit_prediction`, `reveal_prediction`, `mint_initial`, `cash_out_early`, and `apply_scheduled_changes`). Blocked in both `ClaimsOnly` and `FullyPaused`.
2. **`Claim`**: Entrypoints that withdraw accrued winnings or refunds (`claim_winnings`, `claim_many`). Blocked only by `FullyPaused`; always allowed in `ClaimsOnly` so users can access their capital during market pauses.
3. **`Settlement`**: Entrypoints that resolve or close rounds (`resolve_round`, `resolve_round_multi`, `void_round`, `finalize_round`, `cancel_round`). Blocked by `FullyPaused` (with the exception noted below); allowed in `ClaimsOnly` so pending markets can settle cleanly.
4. **`AdminConfig`**: Administrative and configuration entrypoints (`create_round`, `create_next_from_template`, `set_windows`, `schedule_windows`, `set_oracle_max_deviation_bps`, etc.). Blocked only by `FullyPaused`; allowed in `ClaimsOnly` so operators can prepare or restart markets.

### Gating Bypass Categories

1. **Mode-Transition Controls** (`set_runtime_mode`, `pause_contract`, `unpause_contract`, `is_paused`, `get_runtime_mode`, `is_action_allowed`):
   - These entrypoints deliberately bypass `_policy_gate` completely. They must remain callable in every mode, including `FullyPaused`, so operators have an unconditional path to recover from incidents.
2. **Read-Only Queries & Heartbeat** (`update_oracle_heartbeat`, `get_protocol_health`, `get_protocol_status`, `get_round_status`, `balance`, `get_active_round`, `get_*`):
   - Heartbeat updates must continue flowing so health monitors accurately reflect oracle vitality during pauses. Read queries do not mutate state and are never blocked.

---

## 3. Discrepancy Note on `cancel_round`

> **Note — `cancel_round` discrepancy**: In the current production contract (`settlement.rs`), `cancel_round` does not call `_ensure_not_paused` and is therefore executable in `FullyPaused` mode. This differs from its documented `Settlement` policy class, which specifies BLOCKED in FullyPaused. Tests reflect actual code behavior. A separate issue should be filed to add the missing guard if the intended behavior is BLOCKED.

---

## 4. Complete Mode $\times$ Action Policy Matrix

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
| `cancel_round` *(see Section 3)* | Settlement (Admin) | **ALLOWED** | **ALLOWED** | **ALLOWED** *(code)* / **BLOCKED** *(spec)* | `ContractError::ContractPaused` (22) |
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
