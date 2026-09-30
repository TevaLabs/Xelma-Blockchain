# Pause Policy Matrix

Canonical reference for what the protocol permits in each runtime mode, and
which contract entrypoints enforce it. Addresses #551.

The matrix itself is implemented in exactly one place —
[`admin::_policy_gate`](../contracts/src/admin.rs) — and every gated
entrypoint reaches it through one of two thin wrappers:

| Wrapper | Delegates to | Blocked by |
|---------|--------------|-----------|
| `_ensure_normal_mode` | `_policy_gate(env, PolicyAction::RoundMutation)` | any non-`Normal` mode |
| `_ensure_not_paused`  | `_policy_gate(env, PolicyAction::AdminConfig)`  | `FullyPaused` only |

New entrypoints are onboarded by calling a wrapper, never by re-deriving the
mode rules at the call site. That is what keeps this table from drifting.

## 1. Runtime modes

`RuntimeMode` is persisted under `DataKeyCore::Paused` and readable via
`get_runtime_mode()`:

| Value | Mode | Meaning |
|------:|------|---------|
| `0` | `Normal` | Full operation. |
| `1` | `ClaimsOnly` | Betting is frozen. Users can still withdraw what they are owed; admins can still reconfigure; the oracle can still settle or cancel an in-flight round. |
| `2` | `FullyPaused` | Emergency stop. Every mutating entrypoint is refused. |

## 2. Mode × action matrix

This is the table operators want. The only asymmetry is the
`ClaimsOnly` column.

| Action class | `Normal` | `ClaimsOnly` | `FullyPaused` |
|--------------|:--------:|:------------:|:-------------:|
| `RoundMutation` | ✅ | ❌ | ❌ |
| `Claim` | ✅ | ✅ | ❌ |
| `Settlement` | ✅ | ✅ | ❌ |
| `AdminConfig` | ✅ | ✅ | ❌ |
| *mode transitions* | ✅ | ✅ | ✅ |
| *reads / heartbeat* | ✅ | ✅ | ✅ |

`RoundMutation` is the only class blocked by `ClaimsOnly`: once the protocol
has stopped accepting bets there is no round to bet into, so continuing to
allow new wagers would let money in during an incident. Everything else stays
available so an operator can wind the protocol down without stranding user
funds.

The last two rows are deliberate and are covered in §4.

## 3. Entrypoint inventory

Derived from the call sites of the two wrappers across
`admin.rs`, `betting.rs`, `config.rs`, `settlement.rs`, `access_control.rs`,
`collateral.rs`, `insurance.rs`, `leaderboard.rs` and `oracle_committee.rs`.
`contracts/src/contract.rs` is a thin dispatcher over those modules and
carries no gates of its own.

### 3.1 `RoundMutation` — blocked in `ClaimsOnly` and `FullyPaused`

`apply_scheduled_changes`, `cash_out_early`, `commit_prediction`,
`mint_initial`, `place_bet`, `place_precision_prediction`,
`reveal_prediction`

`predict_price` is a thin alias that delegates to
`place_precision_prediction`, so it is gated transitively and is tested as
part of the same class.

`apply_scheduled_changes` is the one deliberate oddity in the whole config
surface: it is `RoundMutation`-gated, not `AdminConfig`, so an incident also
freezes pending config activations rather than letting them land mid-incident.
`cancel_config_change`, by contrast, *is* `AdminConfig` so an operator can
always back a scheduled change out.

### 3.2 `Claim` — blocked in `FullyPaused`

`claim_many`, `claim_winnings`

### 3.3 `Settlement` — blocked in `FullyPaused`

`finalize_round`, `resolve_round`, `resolve_round_multi`, `void_round`

`cancel_round` is listed here by policy intent but is **not** actually gated —
see §5.1.

### 3.4 `AdminConfig` — blocked in `FullyPaused`

`add_allowlisted`, `add_denylisted`, `arm_hb_override`,
`arm_oracle_deviation_override`, `batch_touch_ttl`, `cancel_config_change`,
`clear_round_template`, `create_next_from_template`, `create_round`,
`migrate_schema_v1_to_v2`, `migrate_schema_v2_to_v3`,
`reclaim_expired_pending_winnings`, `remove_allowlisted`, `remove_denylisted`,
`reset_leaderboard_season`, `set_access_control_enabled`,
`set_archive_retention`, `set_attestation_key`, `set_close_buffer_ledgers`,
`set_deviation_ref_mode`, `set_dispute_ledgers`, `set_early_cashout_bps`,
`set_epoch_mint_budget`, `set_fee_model`, `set_hb_grace_seconds`,
`set_hb_strict_mode`, `set_insurance_coverage_bps`,
`set_insurance_eligible_events`, `set_insurance_split_bps`,
`set_max_precision_participants`, `set_min_participants`, `set_mint_limit`,
`set_oracle_min_confidence_bps`, `set_oracle_quorum_config`,
`set_oracle_strict_mode`, `set_precision_payout_policy`, `set_round_template`,
`top_up_insurance_fund`, `withdraw_insurance_fund`, `withdraw_protocol_fee`

`create_round` and `create_next_from_template` are `AdminConfig` rather than
`RoundMutation` on purpose: they are how an operator brings the protocol back
to `Active` after a claims-only incident, so they must stay callable there.

## 4. Deliberate exemptions

These are not oversights.

**Mode-transition controls** — `pause_contract`, `unpause_contract`,
`set_runtime_mode` call `_set_mode` directly and never touch `_policy_gate`.
They must remain callable in every mode, `FullyPaused` included, or an
incident would be unrecoverable. They are still admin-authenticated, and
still rejected with `GovUnauthorized` when a governance approver is
configured — just not by the runtime-mode gate. **Do not add a
`_policy_gate` call to these.**

**Reads and heartbeat recording** — `is_*` / `get_*` queries are ungated
because reads never mutate state. `update_oracle_heartbeat` is ungated so
`get_protocol_health` keeps reflecting live oracle status during an incident;
a protocol that stopped reporting a stale oracle because it was paused would
be exactly the wrong failure mode.

## 5. Known divergences between this document and the implementation

Found while auditing the implementation against the documented matrix. These
are **recorded and pinned by tests, not fixed** — changing what an emergency
stop blocks needs core-maintainer sign-off per `GOVERNANCE.md`, and silently
changing it would be worse than documenting it.

### 5.1 `cancel_round` is not gated

The matrix places `cancel_round` in `Settlement` (§3.3), so `FullyPaused`
should block it. `settlement::cancel_round` contains no gate call, so a
cancellation **succeeds during a full emergency stop**.

Cancelling only returns stakes rather than paying them out, so the current
behaviour is not obviously harmful — but "the emergency stop does not stop
this" is a policy decision that belongs to maintainers, not to whatever the
code happens to do today.

Pinned by `test_cancel_round_is_ungated_and_diverges_from_the_matrix`. If
that test starts failing, the gate was added: drop this section and add
`cancel_round` to the enforced `Settlement` list in the test.

### 5.2 `announce_next_schema` / `clear_next_schema` are not gated

Both are admin-authenticated writes to `DataKeyCore::NextSchemaVersion` with
no gate call, so they also succeed during `FullyPaused`. They are purely
informational — they announce a target version for a *future* migration and
change no live state — so this is lower-stakes than §5.1, but it is still an
ungated admin write.

### 5.3 The whole governance surface is not gated

No function in `governance.rs` calls the policy gate. `execute`, `approve`,
`cancel`, `propose`, `establish_constitution`, `propose_amendment`,
`veto_amendment` and `activate_amendment` are all reachable during
`FullyPaused`.

This may well be correct — governance is the mechanism an incident response
uses, and freezing it during an incident could be the worse failure. But it
is a large, deliberate-looking hole, and it is not currently written down
anywhere.

## 6. Test coverage

Every cell in §2 is enforced by
[`contracts/src/tests/pause_policy_matrix.rs`](../contracts/src/tests/pause_policy_matrix.rs).

| Test | Enforces |
|------|----------|
| `test_fully_paused_blocks_every_admin_config_entrypoint` | §3.4, `FullyPaused` column |
| `test_claims_only_does_not_block_any_admin_config_entrypoint` | §3.4, `ClaimsOnly` column |
| `test_claims_only_blocks_every_round_mutation_entrypoint` | §3.1, `ClaimsOnly` column |
| `test_fully_paused_blocks_every_round_mutation_entrypoint` | §3.1, `FullyPaused` column |
| `test_claims_only_allows_claims_and_fully_paused_denies_them` | §3.2 both columns, and that funds actually move |
| `test_claims_only_does_not_block_settlement` | §3.3, `ClaimsOnly` column |
| `test_fully_paused_blocks_settlement` | §3.3, `FullyPaused` column |
| `test_claims_only_does_not_block_the_rest_of_the_matrix` | the §2 row asymmetry |
| `test_set_runtime_mode_is_never_blocked_by_its_own_gate` | §4 mode transitions |
| `test_apply_scheduled_changes_is_blocked_in_claims_only_and_fully_paused` | the §3.1 `apply_scheduled_changes` oddity |
| `test_cancel_round_is_ungated_and_diverges_from_the_matrix` | §5.1 |

A "blocked" assertion requires the call to fail with exactly
`ContractError::ContractPaused` — asserting the specific error is what proves
the policy gate is the cause rather than some unrelated precondition. An
"allowed" assertion only requires that the call does *not* fail with
`ContractPaused`; it may still fail for an orthogonal reason such as "no
pending rotation". The one exception is
`test_claims_only_allows_claims_and_fully_paused_denies_them`, which settles
a real round and asserts the winner's balance grows by exactly the pending
amount.

The matrix rows in §2 are also covered at the class level by
`tests/policy_gate.rs`, and end-to-end during an incident by
`tests/drill.rs`.

## 7. Keeping this in sync

When you add a gated entrypoint, update the relevant list in §3 and the
inventory comment on `admin::_policy_gate`. The test file drives the
entrypoints it lists explicitly, so a new gated entrypoint that is added to
the code but not to the test is a gap the test cannot detect on its own —
the §3 lists and the test's `call_every_*` helpers must be updated together.

`tests/drill.rs` is the operational drill for this matrix and is what an
operator should run to rehearse an incident response.
