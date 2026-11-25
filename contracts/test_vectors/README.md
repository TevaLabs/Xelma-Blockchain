# Settlement math golden vectors

`settlement_math.json` pins the expected output of the pure functions in
[`contracts/src/settlement_math.rs`](../src/settlement_math.rs) — fee
splitting, UpDown payouts, Precision winner-finding, pot splitting, and
oracle-deviation math — against known inputs. It is loaded and checked by
[`contracts/src/tests/settlement_math_vectors.rs`](../src/tests/settlement_math_vectors.rs)
on every `cargo test`, so an unintentional change to `settlement_math`'s
behavior fails CI immediately, with a diffable JSON file to point at instead
of a inline Rust literal buried in a test function.

`settlement_math.rs` takes no `Env`, does no storage I/O, and only ever
returns errors for real overflow — so these vectors can be verified with the
plain Rust test harness, no Soroban test environment required.

## Sections

| Section | Function under test | Notes |
|---|---|---|
| `updown_fee` | `compute_updown_fee` | Includes the thin-losing-pool fee-spillover case. |
| `precision_fee` | `compute_precision_fee` | Includes zero/negative pot no-ops. |
| `updown_payouts` | `compute_updown_payouts` | Full composite: direction classification, one-sided refunds, fee, proportional split. |
| `precision_winners` | `find_precision_winners` | Winner-finding only (no payout split). Covers ties and unrevealed-always-loses. |
| `split_pot_among_winners` | `split_pot_among_winners` | Pure remainder-distribution math. |
| `deviation_bps` | `compute_deviation_bps` | Oracle price-deviation basis points. |
| `precision_remainder_ordering` | `compute_precision_payouts` (composite) | See below — the canonical-ordering rule for Issue #404. |

## The remainder-to-first-winner rule (`precision_remainder_ordering`)

When a Precision round pot doesn't divide evenly among tied winners, the
indivisible remainder goes to the **first winner in canonical order**
(`split_pot_among_winners`, index 0). "Canonical order" means:
`settlement.rs` sorts `RoundParticipants` by address
(`sort_addresses`, ascending byte order) *before* building the list of
entries passed into `settlement_math` — so index 0 among the winners always
corresponds to the participant with the lexicographically-lowest address,
never insertion/bet order. This is documented in `PROTOCOL_SPEC.md` under
"Precision mode".

`settlement_math.rs` itself has no concept of an `Address` — it only sees
indices — so these vectors encode the rule the same way the real pipeline
does: the input `entries` array is written in the order settlement.rs would
hand it to `settlement_math` for two participants where index 0 is already
known to sort first. The corresponding end-to-end proof, with real
`Address` values compared directly, lives in
`tests::resolution::precision::test_precision_remainder_goes_to_lexicographically_lowest_winner`
and its 3-way sibling
(`test_precision_remainder_3way_tie_goes_to_lexicographically_lowest_winner`);
the 5-way case is covered only at the pure-math level in this file's
`precision_remainder_ordering` vectors.

## Regenerating intentionally

Do this only when you've deliberately changed `settlement_math.rs`'s
behavior and have reviewed why every affected number should change.

1. Run the regeneration test, which recomputes every `expected_*` field
   from the current implementation while keeping each case's declared
   inputs (`name`, `note`, arguments) untouched, and overwrites this file:

   ```sh
   cargo test --package xelma-contract --lib \
     tests::settlement_math_vectors::regenerate_vectors_file \
     --features testutils -- --ignored --nocapture
   ```

2. `git diff contracts/test_vectors/settlement_math.json` and read every
   changed number. If a value changed that you did *not* expect from your
   code change, you likely introduced a regression — do not commit.
3. Re-run the checked (non-`--ignored`) tests in
   `settlement_math_vectors.rs` to confirm the file now round-trips clean.
4. To add a *new* case rather than update an existing one, add it by hand
   to the relevant array with a placeholder `expected_*`/`expected` value,
   then run step 1 to fill in the real value, then review as in step 2.

## Adding a whole new section

The regeneration test only knows how to recompute the seven sections above.
If you add a new settlement_math function worth pinning, add a matching
section to the JSON, a matching case struct + loader in
`settlement_math_vectors.rs`, a checked test function for it, and extend
`regenerate_vectors_file` to recompute its `expected_*` field(s) too — don't
let a section silently go unregenerated.
