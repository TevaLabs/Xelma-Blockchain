# Precision Sealed-Bid Commit Fee

Issue #534 — an optional, admin-configurable fee on Precision sealed-bid
commitments, used to deter commitment spam.

Canonical implementation:
[`contracts/src/config.rs`](../contracts/src/config.rs) (config and fee
computation) and
[`contracts/src/betting.rs`](../contracts/src/betting.rs) (`commit_prediction`,
the only place the fee is charged).

---

## Why

A Precision commitment is cheap to place and its stake is forfeit to the pot
if it is never revealed. An attacker can therefore commit a large number of
throwaway bids for almost no cost, inflating participant counts, storage, and
resolution work — and skewing the commit-reveal dynamics that the mode exists
to provide.

Charging a small fee per commitment makes that spam economically irrational
without affecting honest participants, who commit once and reveal.

---

## Configuration

```
set_precision_commit_fee_bps(bps: Option<u32>)   // admin, immediate (no timelock)
get_precision_commit_fee_bps() -> Option<u32>
```

| Setting | Meaning |
|---|---|
| `None` (key absent) | Commitments are free. **The default.** |
| `Some(bps)` | Fee is `bps` of the committed amount. `1 <= bps <= 1000`. |

`Some(0)` is rejected with `InvalidProtocolFeeBps`, so "configured but zero" is
never a reachable state — disabling always means `None`. The cap is
`MAX_COMMIT_FEE_BPS = 1000` (10%), matching `MAX_PROTOCOL_FEE_BPS`.

### Why this setter is not timelocked

`set_protocol_fee_bps` schedules a pending change. This one applies
immediately, on purpose: the fee is a spam-response control, and an operator
must be able to raise it *during* an attack. A timelock would leave the window
open for exactly as long as the delay.

The fee only affects commitments placed after it is set. Commitments already
in a round are untouched, so a fee can never retroactively change a live
round's economics.

---

## Incidence

| Property | Behaviour |
|---|---|
| Charged on | the committed `amount`, at `bps` |
| Debited from | the committer's balance, **in addition to** the stake |
| Added to the round pot? | **No** — it is pure protocol revenue |
| Credited to | the protocol fee treasury (after the insurance split) |
| Refunded? | **Never** |

A committer therefore needs `stake + fee` available. If they do not have it,
the commit is rejected with `InsufficientBalance` and **no** state changes: no
debit, no commitment record, no fee. The fee can never confiscate a stake.

Because the fee is not part of the pot, settlement maths is untouched by it —
pot conservation at resolution is exactly the pre-#534 identity.

### What is *not* charged

- Direct Precision predictions (`place_precision_prediction`) — the fee targets
  sealed-bid commitment spam specifically, and direct predictions are already
  one-per-user and self-limiting.
- Up/Down bets.
- Commitments in a non-Precision round (rejected regardless).

---

## Refund policy

The fee is **never refunded**. In particular:

- **Not** on a successful reveal and a normal win or loss.
- **Not** on the all-unrevealed refund path, where stakes are returned in
  full. An attacker who commits and never reveals still pays — this is the
  core of the deterrent.
- **Not** on round cancellation or any other terminal transition.

This is the single most important behavioural property, and it is what
distinguishes the fee from a refundable deposit.

---

## Conservation

Value is conserved: the fee moves from the user's balance into the fee ledger.
It is never minted and never burned.

```
balance_drop == stake + fee                    (at commit time)
fee_ledger_after == fee_ledger_before + fee    (treasury + insurance)
```

Payouts are credited to `pending_winnings` and claimed separately, so a
participant's liquid-balance drop is exactly `stake + fee`; the payout is a
credit, not a rebate of that debit. The same distinction applies to refunded
stakes.

Integer flooring means a very small commitment at a very small rate can round
the fee down to zero, in which case no fee is charged and no fee event is
emitted. That is safe: the fee is a deterrent, not a revenue line.

---

## Events

| Event | Payload | Purpose |
|---|---|---|
| `("commit", "fee_chrg")` | `(round_id, user, stake, fee)` | per-user attribution, which the round-level fee event cannot carry |
| `("protocol", "fee_coll")` | `(round_id, fee, treasury_after, bps, fee_model)` | standard fee accounting, including the insurance split |
| `("config", "pc_fee")` | `(bps)` | the admin configuration change |
| `("config", "updated")` | see `_emit_config_updated` | `ConfigChangeKind::PrecisionCommitFeeBps` (20), old and new value |

The fee is routed through the same `_collect_protocol_fee` helper as
settlement fees, so the insurance split and the treasury accounting apply
uniformly and cannot drift between the two fee surfaces.

---

## Storage and compatibility

- Stored under `DataKeyCore::PrecisionCommitFeeBps` (persistent, TTL-extended
  on read and write).
- `PrecisionCommitment` is **unchanged** — no `fee_paid` field was added — so
  commitments written before this feature remain readable and keep their
  existing XDR shape.
- With no configuration the fee is `0` and `commit_prediction` follows the
  pre-#534 code path exactly. The feature is opt-in, so existing deployments
  are unaffected until an admin enables it.

---

## Tests

[`contracts/src/tests/commit_fee.rs`](../contracts/src/tests/commit_fee.rs)
(18 tests) covers the acceptance criteria:

| Criterion | Tests |
|---|---|
| Backward-compatible default | `default_commit_fee_is_disabled_and_commits_are_free`, `sub_stroop_commit_fee_rounds_to_zero_without_events` |
| Configurable fee | `commit_fee_is_configurable_and_applies_immediately`, `commit_fee_can_be_disabled`, `commit_fee_rejects_out_of_range_rates`, `commit_fee_can_only_be_set_by_admin` |
| Treasury + events | `commit_fee_emits_treasury_and_per_user_events`, `setting_commit_fee_emits_config_event`, `commit_fee_respects_the_insurance_split` |
| Conservation | `commit_fee_is_not_part_of_the_pot`, `commit_fee_is_not_refunded_on_all_unrevealed`, `commit_fee_is_not_refunded_on_cancellation`, `balance_boundary_is_exact`, `insufficient_balance_for_stake_plus_fee_changes_nothing` |
| No collateral damage | `direct_precision_predictions_are_not_charged`, `updown_round_is_unaffected_by_the_commit_fee`, `commit_guards_still_apply_with_fee_enabled`, `revealing_still_settles_normally_with_fee_enabled` |
