#!/usr/bin/env python3
"""Append protocol-fee event entries (#162) to:
   docs/EVENT_SCHEMA.md  -- formal 'protocol' namespace spec
   docs/event_schema_guide.md -- observability narrative
"""
import sys

# ---------------------------------------------------------------------------
# EVENT_SCHEMA.md  (canonical reference)
# Append a "protocol" namespace section just before the existing "JavaScript
# / TypeScript (Stellar SDK)" decoding-section tail.
# ---------------------------------------------------------------------------
ES = '/workspaces/Xelma-Blockchain/docs/EVENT_SCHEMA.md'
es = open(ES).read()

# Find the canonical "## Field units quick reference" anchor at the end and
# insert BEFORE its "---" separator line.
ANCHOR = '---\n\n## Field units quick reference\n'
if ANCHOR not in es:
    print('[EVENT_SCHEMA.md] tail anchor missing', file=sys.stderr); sys.exit(1)
if '("protocol", "fee_collected")' in es:
    print('[EVENT_SCHEMA.md] already has fee_collected entry -- skip')
else:
    insert = r"""---

## `("protocol", "fee_collected")` — Competitive-settlement fee accrual

Emitted by every competitive-settlement path (UpDown indexed/legacy, Precision
indexed/legacy) when the protocol fee is enabled (Issue #162). NOT emitted on
refund / cancel / fallback paths — those return users' full stake and the
treasury stays flat.

| Field         | Type      | Description                                                |
|---------------|-----------|------------------------------------------------------------|
| `round_id`    | `u64`     | The id of the settled round.                                |
| `fee_amount`  | `i128`    | Stroops routed to the on-chain treasury this round.         |
| `treasury_balance` | `i128` | Cumulative treasury balance AFTER this round's credit.     |
| `bps_active`  | `u32`     | The fee's bps that produced `fee_amount` (echoes storage).  |

**Topics**: `("protocol", "fee_collected")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `_record_winnings_indexed`, `_record_winnings_legacy`,
`_resolve_precision_mode`, `_resolve_precision_legacy`.

The conservation invariant
`Σ payout_i + fee_amount == total_pot` holds for every emission. In the
UpDown pathological case `fee > losing_pool` (very thin losing-side
liquidity near the bps cap) the spillover is deducted from `winning_pool`
so the invariant still holds and winners receive only their residual
principal — documented inline in `_apply_protocol_fee_updown`.

---

## `("protocol", "fee_bps_set")` — Timelocked fee schedule applied

Emitted exactly once when a previously-scheduled `ProtocolFeeBps` change
passes its `activation_ledger` and is written to storage (Issue #162).

| Field   | Type          | Description                                              |
|---------|---------------|----------------------------------------------------------|
| `bps`   | `Option<u32>` | New fee (None = fee disabled; Some(bps) = active).       |

**Topics**: `("protocol", "fee_bps_set")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `_apply_config_payload` arm for `ConfigChangeKind::ProtocolFeeBps`.

---

## `("protocol", "fee_withdrawn")` — Treasury drain to recipient

Emitted when the admin drains accumulated fees to an on-chain recipient
(Issue #162). Recipient receives the credited amount through the existing
`PendingWinnings` → `claim_winnings` flow used by competitive payouts and
refunds, so no new authorization surface is added.

| Field           | Type     | Description                                              |
|-----------------|----------|----------------------------------------------------------|
| `recipient`     | `Address`| The credited account.                                    |
| `amount`        | `i128`   | Stroops transferred out of the treasury this call.       |
| `new_treasury`  | `i128`   | Treasury balance after withdrawal.                       |

**Topics**: `("protocol", "fee_withdrawn")`
**Source contracts**: `VirtualTokenContract`
**Emitted by**: `withdraw_protocol_fee`.

---

"""
    es = es.replace(ANCHOR, insert + ANCHOR)
    open(ES, 'w').write(es)
    print(f'[EVENT_SCHEMA.md] appended fee entries; now {len(es.splitlines())} lines')

# ---------------------------------------------------------------------------
# event_schema_guide.md  (observability narrative)
# Append a section at the end.
# ---------------------------------------------------------------------------
GUIDE = '/workspaces/Xelma-Blockchain/docs/event_schema_guide.md'
g = open(GUIDE).read()

if 'Section: protocol fees (Issue #162)' in g:
    print('[event_schema_guide.md] already has fee section -- skip')
else:
    insert = r"""
## Section: Protocol fee events (Issue #162)

The optional protocol fee introduces a new top-level event namespace
`("protocol", ...)` for treasury-related observability. Three event types
emitted, all gated on admin-controlled timelock activation:

### `("protocol", "fee_collected")` — competitive settlement fee accrued

Emitted exactly once per competitive settlement (UpDown indexed/legacy,
Precision indexed/legacy) when `get_protocol_fee_bps` returns
`Some(active_bps)`. Payload `(round_id: u64, fee_amount: i128,
treasury_balance: i128, bps_active: u32)`.

Conservation `Σ payouts + fee_amount == total_pot` is enforced in
`_apply_protocol_fee_*`. UpDown conservatively deducts from the losing
pool first, then spills over into the winning pool — so winners
receive their remaining principal when the fee exceeds losing liquidity.
Refund paths (`("round","fallback")`, `("pool","onesided")`, price-unchanged
refunds, admin cancellations) do NOT emit this event.

### `("protocol", "fee_bps_set")` — timelock applied

Emitted exactly once when a previously-scheduled `ProtocolFeeBps` change
is written to storage at its `activation_ledger`. Payload is
`(Option<u32>,)` — `None` means "fee disabled again", `Some(bps)` carries
the new active bps.

### `("protocol", "fee_withdrawn")` — treasury drained to recipient

Admin-only. Payload `(recipient: Address, amount: i128,
new_treasury: i128)`. Recipient is credited via the existing
`PendingWinnings` ledger — so claim semantics are identical to
competitive winnings, and no additional surface is needed for users
to spend the credited amount.

### Indexer guidance

A fee-aware indexer can rely on `fee_collected` events as the canonical
record of fee accrual. Treasury balance computations should:

1. Subscribe to `("protocol", fee_collected)` for per-round accruals.
2. Subscribe to `("protocol", fee_withdrawn)` for treasury drains.
3. Optionally cross-reference `("config", applied)` events associated
   with `("protocol", fee_bps_set)` for rate changes.

Conservations across event streams:

* For each `fee_collected` event: Σ of `("claim","winnings")` for the
  same round's winners + `fee_amount` == `round.pool_up + round.pool_down`
  (UpDown) or `Σ prediction.amount` (Precision mode, including
  unrevealed-commitment stakes).
* Treasury balance monotonically increases across `fee_collected`
  events and monotonically decreases across `fee_withdrawn` events.

"""
    open(GUIDE, 'w').write(g.rstrip() + '\n' + insert)
    print(f'[event_schema_guide.md] appended fee section; now {len(g.splitlines()) + insert.count(chr(10))} lines')
