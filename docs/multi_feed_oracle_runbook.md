# Multi-feed oracle runbook

## Purpose

This runbook documents the production path for the multi-feed oracle settlement flow used by `resolve_round_multi`. The goal is to keep quorum math, outlier filtering, and replay protection aligned with the same security boundaries as the single-oracle path.

## Configuration contract

1. Set the admin-owned quorum configuration with `set_oracle_quorum_config`.
2. Keep `min_observations` at or above the protocol minimum and ensure `quorum_threshold` is never greater than the configured minimum.
3. Keep `outlier_threshold_bps` non-zero and within the protocol range so distorted feed values are rejected before settlement.

## Operational flow

1. A round is created in the normal lifecycle.
2. The oracle publishes a `MultiFeedPayload` with many independent feed observations and a unique nonce.
3. The contract validates the network binding, contract binding, timestamp window, and heartbeat health before accepting the payload.
4. The median is computed, outliers beyond the configured threshold are excluded, and the remaining surviving observations must still satisfy the configured quorum threshold.
5. If the payload passes filters, the round settles on the median price and the nonce is consumed to prevent replays.

## Security notes

- Data is bound to the active round by `round_id` and the contract address.
- A reused nonce for the same round is rejected with `OracleNonceReused`.
- Outlier filtering keeps malicious or corrupt feed data from dominating the final settlement price.
- Quorum thresholds prevent a tiny subset of feed values from satisfying settlement without broad consensus.
