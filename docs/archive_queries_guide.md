# Archive Queries Guide (Schema v3)

This guide is intended for indexers, UI clients, and other consumers querying historical participation data from the Xelma-Blockchain contract (Schema v3).

## Overview

Starting in schema v3, the contract maintains an on-chain archive of resolved and cancelled rounds, along with persistent records of individual user outcomes. This allows consumers to fetch historical data directly from the contract without needing to reconstruct state from events, subject to the contract's retention limits.

### Key Queries

The primary queries available in `contracts/src/queries.rs` for archive data are:

1. **`get_recent_archived_rounds(limit: u32) -> Vec<ArchivedRoundSummary>`**
   Fetches the most recently archived rounds (newest first).
2. **`get_archived_round(round_id: u64) -> Option<ArchivedRoundSummary>`**
   Fetches a specific archived round by ID.
3. **`get_user_archive_history(user: Address, offset: u32, limit: u32) -> Vec<ArchivedRoundSummary>`**
   Fetches paginated archived participation history for a specific user (newest first).
4. **`get_user_archived_participation(user: Address, round_id: u64) -> Option<UserRoundOutcome>`**
   Fetches a detailed outcome record for a user in a specific archived round.

---

## Pagination

Pagination is supported on list endpoints to keep resource usage within Soroban's read limits.

- **Limit:** The `limit` parameter is always capped by `MAX_PAGE_SIZE` internally (and by the `ArchiveRetention` config for global history). If you request a limit larger than the maximum allowed, the contract will safely truncate it.
- **Offset:** The `offset` parameter in `get_user_archive_history` acts as a zero-based index from the newest record. 
  - To get the first page of 10 items: `offset = 0, limit = 10`
  - To get the second page: `offset = 10, limit = 10`

---

## Status Meanings (UserOutcomeType)

The `UserRoundOutcome` struct returned by `get_user_archived_participation` includes an `outcome` field of type `UserOutcomeType` (represented as an integer in XDR). 

The status meanings are:

| Value | Name | Description |
| --- | --- | --- |
| `0` | **Win** | The user's prediction was correct. The `payout` field reflects their share of the winnings. |
| `1` | **Loss** | The user's prediction was incorrect. The `payout` is `0`. |
| `2` | **Refund** | The round ended in a draw (unchanged price) or was one-sided, resulting in a refund. `payout` equals `stake`. |
| `3` | **Cancel** | The round was cancelled by an admin or oracle. `payout` equals `stake`. |
| `4` | **Void** | The prediction was voided. |

---

## Claim Linkage

When a user wins (`outcome == 0`) or is refunded (`outcome == 2` or `3`), their payout is credited to their **pending winnings** balance rather than being pushed directly to their Stellar account.

- **Check Unclaimed Winnings:** Use `get_pending_winnings(user: Address) -> i128` to see their total aggregate unclaimed balance.
- **Claim Winnings:** The user must call the `claim_winnings` transaction to transfer these pending winnings to their Stellar account.

The `UserRoundOutcome` record tells you exactly how much a user earned in a *specific round*, but the claim state is decoupled and aggregated. To determine if a user has outstanding winnings to claim, check `get_pending_winnings`.

---

## Example Query Sequences

### 1. Displaying a User's Recent Activity
To build a "My History" view for a user:

1. Fetch their most recent participated rounds (page 1):
   `get_user_archive_history(user="G...", offset=0, limit=10)`
2. For each returned `ArchivedRoundSummary`, fetch their specific outcome:
   `get_user_archived_participation(user="G...", round_id=summary.round_id)`
3. Display the round details along with their `outcome` status (Win/Loss/Refund) and `payout`.

### 2. Global Recent Rounds Feed
To build a global "Recent Rounds" dashboard:

1. Fetch the 5 most recently resolved rounds:
   `get_recent_archived_rounds(limit=5)`
2. (Optional) Fetch a specific round if the user clicks for details:
   `get_archived_round(round_id=...)`

### 3. Checking for Actionable Winnings
When a user connects their wallet:

1. Fetch their pending balance:
   `get_pending_winnings(user="G...")`
2. If > 0, display a "Claim X Winnings" button that invokes `claim_winnings`.
