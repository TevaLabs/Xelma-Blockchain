# Protocol Fee Incidence

How Xelma charges its protocol fee, who ultimately bears it, and the
invariants that hold regardless of configuration.

Canonical implementation: [`contracts/src/settlement_math.rs`](../contracts/src/settlement_math.rs).
The incidence type itself lives in
[`contracts/src/fee_incidence.rs`](../contracts/src/fee_incidence.rs).

---

## The two models

| Model | `FeeModel` | Taxable base | Effect |
|---|---|---|---|
| **Fee on pot** | `FeeOnPot` (`0`) — **default** | the whole round pot | winners' *principal* is taxed alongside their profit |
| **Fee on winnings** | `FeeOnWinnings` (`1`) | net winnings (realised profit) only | winners get their principal back untouched |

Both are the same *rate*; they differ only in what the rate is applied to.

### Configuration

```
set_fee_model(model: FeeModel)   // admin, applies immediately (no timelock)
get_fee_model() -> FeeModel       // defaults to FeeOnPot
set_protocol_fee_bps(bps: Option<u32>)  // admin, TIMELOCKED
get_protocol_fee_bps() -> Option<u32>
```

`set_protocol_fee_bps` is timelocked: it schedules a pending change that takes
effect after the config delay. `set_fee_model` is not — switching incidence
takes effect on the next settlement. A fee rate of `None` means "fee disabled"
and yields zero under both models.

`MAX_PROTOCOL_FEE_BPS = 1000` (10%).

---

## Incidence by round mode

### UpDown

Winning participants split the entire pot proportionally. Their *profit* is
exactly the losing side's stake, so the loser's money is the profit pool.

| | Taxable base | Drawn from |
|---|---|---|
| `FeeOnPot` | `winning_pool + losing_pool` | losing pool first, then spills onto the winning pool |
| `FeeOnWinnings` | `losing_pool` | losing pool only — the winning pool is never touched |

**Worked example — price goes Up.** Alice stakes 60 Up, Bob 40 Up, Charlie 100
Down. So `pool_up = 100`, `pool_down = 100`, pot `200`, at 10%:

- `FeeOnPot` → fee `= 200 × 10% = 20`, all of it from the losing pool.
  Distributable `180`, split 60/40 → Alice `108`, Bob `72`, Charlie `0`.
- `FeeOnWinnings` → fee `= 100 × 10% = 10`, all from the losing pool.
  Distributable `190`, split 60/40 → Alice `114`, Bob `76`, Charlie `0`.

The two only differ when the pools are **unequal**. When they are equal, both
models charge the same fee — the tax base is the same number either way.

**Worked example — unequal pools.** Alice 700 Up, Bob 300 Down, price falls so
Down wins. `winning_pool = 300`, `losing_pool = 700`, pot `1000`, at 10%:

- `FeeOnPot` → fee `= 1000 × 10% = 100`, spread across both pools.
- `FeeOnWinnings` → fee `= 700 × 10% = 70`, all from the losing (Up) pool.
  Bob's winning pool is untouched at `300`.

Note the direction: the *losing* pool is the taxable base in both models. A fee
implementation that taxed `pool_up` without first resolving which side won
would silently compute the wrong fee whenever the pools differ.

### Precision

Winners split the pot. Their realised profit is the pot minus their own stake —
i.e. the losing participants' stakes.

| | Taxable base | Charged to |
|---|---|---|
| `FeeOnPot` | `total_pot` | winners' payout is reduced by the fee |
| `FeeOnWinnings` | `total_pot - winner_stakes` | only the winners' profit is reduced |

**Worked example — multi-winner.** Alice 50, Bob 50 (both exact), Carol 30,
Dave 30 (both wrong). `total_pot = 160`, `winner_stakes = 100`, profit `60`,
at 10%:

- `FeeOnPot` → fee `= 160 × 10% = 16`. Distributable `144`, split between
  Alice and Bob (`72` each under the `Equal` policy).
- `FeeOnWinnings` → fee `= 60 × 10% = 6`. Distributable `154` (`77` each).

**Worked example — a lone winner staking the whole pot.** Alice stakes 100 and
is the only participant.

- `FeeOnPot` → fee `= 10`, Alice receives `90`.
- `FeeOnWinnings` → profit is `0`, so **the fee is `0`** and Alice receives the
  full `100` back.

This is the sharpest behavioural difference between the two models, and the
reason `FeeOnWinnings` cannot be implemented by simply changing a base in one
place without a zero-profit guard.

---

## When no fee is charged

Both models charge **nothing** in any of these cases, because no participant
realised winnings:

- **UpDown tie** — settlement price equals the start price.
- **UpDown one-sided pool** — exactly one of `pool_up` / `pool_down` is zero.
- **Precision all-unrevealed** — no commitment was revealed and no direct
  prediction was placed, so every stake is refunded.
- **Precision zero-profit** — `FeeOnWinnings` only, when
  `total_pot <= winner_stakes`.
- **Fee rate unset or `0`.**

Cancelled and voided rounds refund in full and never reach the fee path.

---

## Conservation

No stroop is created or destroyed, under any model, at any rate, in any mode.

**Precision — exact:**

```
sum(payouts) + fee == total_pot
```

The remainder policy assigns every leftover stroop to the first winner (by
participant order), so the distributable amount is fully allocated.

**UpDown — bounded by per-winner truncation:**

```
total_pot - (winner_count - 1)  <=  sum(payouts) + fee  <=  total_pot
```

The slack is integer truncation in the proportional split
(`stake × distributable / winning_pool`, floored). It is at most one stroop per
winner, it can never exceed the pot, and it is never silently credited to the
treasury.

The fee is drawn from the losing pool first and only spills onto the winning
pool when it would otherwise exceed the losing pool. At the protocol cap
(`bps <= 1000`) that spillover is unreachable in both models; the branch exists
so the invariant is structural rather than contingent on a configuration
bound, which also means a replay transcript carrying an out-of-range rate
degrades to a smaller winner payout instead of an arithmetic failure.

---

## Single source of truth

The fee formula exists in **one** place. Everything else delegates to it:

| Caller | Delegates to |
|---|---|
| live settlement (`settlement.rs`) | `config::_apply_protocol_fee_*` |
| `simulate_payout` preview (`queries.rs`) | `config::calculate_protocol_fee_*` |
| audit / golden vectors / property tests | `settlement_math::compute_*_fee_with_model` |
| offline replay engine (`replay-engine`) | same file, via `#[path]` include |

`config::calculate_protocol_fee_updown` and `calculate_protocol_fee_precision`
are thin adapters over the engine; they read configuration and apply the fee,
but hold no arithmetic of their own.

Issue #531 was raised because this was not true. Three implementations of the
same formula existed, and only the one wired to live settlement understood
`FeeOnWinnings`:

- the audit engine (`settlement_math`) was hard-coded to `FeeOnPot`, so a
  `FeeOnWinnings` round could not be verified against it, and the offline
  replay engine — which includes that same file — replayed a fee the chain
  never charged;
- a further private `FeeOnPot`-only copy survived in `contract.rs` with no
  callers, differing in both incidence and error type.

The matrix tests in
[`contracts/src/tests/fee_model_matrix.rs`](../contracts/src/tests/fee_model_matrix.rs)
assert, for every `FeeModel` × `RoundMode` cell, that the engine, the preview,
and the live settlement all agree, and that conservation holds.

### Error type

Fee and pot arithmetic is *payout* arithmetic, so overflow surfaces as
`ContractError::PayoutOverflow` (Issue #405) rather than a generic `Overflow`,
in every path.

### Replay transcripts

`RoundTranscript` carries an optional `fee_model` discriminant. An absent value
means the pre-#268 behaviour, which was always fee-on-pot. The field is
`skip_serializing_if = "Option::is_none"`, so transcripts recorded before it
existed serialise — and therefore hash — exactly as they did before, keeping
older dispute cases verifiable.

---

## Events

Fee collection emits `("protocol", "fee_coll")`:

```
(round_id, fee_amount, treasury_balance_after, bps_active, fee_model)
```

`fee_model` is the discriminant (`0` / `1`), so an indexer can attribute a fee
to an incidence model without an extra storage read. Note the topic is
`fee_coll`; an older `collected` topic from the removed duplicate
implementation is no longer emitted.
