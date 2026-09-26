# Portfolio Risk Engine

The contract maintains one `PortfolioExposure` record per user. The record
contains `total`, `up`, and `down` amounts. `total` is the user's outstanding
economic exposure across unresolved round positions and pending settlement
value; claimed balances are not exposure. The Up/Down buckets preserve the
correlation dimension for future side-concentration policies. Precision entries
use the total bucket because their exact price is not an Up/Down side.

`MaxUserRoundExposure` remains the per-round cap and is also the portfolio
hard limit. A bet is rejected before balance mutation when the user's existing
portfolio total plus the new stake exceeds that limit. Settlement value is
tracked after it is produced, but is not re-checked against the entry cap;
otherwise a valid winning payout could make its own settlement fail. This
preserves the existing per-round behavior while preventing a user from
bypassing the limit by spreading bets across rounds. The error is
`PortfolioExposureCapExceeded`.

Exposure updates are bounded: a bet, cash-out, settlement cleanup, claim, or
expired-pending reclaim performs a constant number of reads and writes for the
affected user. Round cleanup still iterates the bounded participant list, as it
already did for position deletion. Settlement first records pending value and
then removes the round position, so exposure remains correct through the
claim-only period. Claiming or expiry removes the pending value.

The index is additive for new positions and tolerant of legacy positions that
were created before the index existed. No historical round scan is required.