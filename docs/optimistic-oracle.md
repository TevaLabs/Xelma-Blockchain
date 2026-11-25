# Optimistic oracle bonded challenges (Issue #358)

The oracle locks a configured virtual-token bond when proposing a positive
price. One distinct challenger may lock a counter-bond and submit an alternate
price before the configured ledger deadline.

Unchallenged proposals can be finalized permissionlessly after the deadline;
the oracle bond is returned. Challenged proposals cannot finalize automatically.
Anyone may escalate after the deadline, but only an explicitly authorized
governance address can choose the winning side. The selected side receives both
bonds. Terminal records zero both liabilities. Deadline and balance arithmetic
are checked, and proposal, challenge, escalation, and decision events are emitted.

## Threat model

A positive counter-bond raises the cost of frivolous challenges. One challenge
per proposal prevents repeated challenge griefing. Challenged capital remains
locked until governance resolves escalation, so operators should monitor
escalation events and resolve promptly. The governance address should be a
multisig or committee account. Duplicate IDs and invalid state transitions are
rejected.

## Integration boundary

This module adds the bonded proposal/challenge/finality state machine and ABI
entrypoints. Existing market payout settlement is not yet wired to consume this
state machine's final price. Do not treat these APIs as a replacement for legacy
round settlement until that integration is completed.
