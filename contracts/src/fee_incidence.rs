// SPDX-License-Identifier: MIT
//! Fee *incidence* — who a protocol fee is charged against.
//!
//! This module is deliberately dependency-free (no `Env`, no `soroban_sdk`,
//! no storage) so that the same source file can be compiled into:
//!
//! * `xelma-contract` — as `crate::fee_incidence`
//! * `xelma-replay`    — via the `#[path = "../../contracts/src/fee_incidence.rs"]`
//!   shim in `replay-engine/src/fee_incidence.rs`
//!
//! `settlement_math` is likewise shared by both crates, so the incidence type
//! it needs must live in a shared, SDK-free module. The on-chain
//! `soroban_sdk::contracttype` enum stays in `types.rs` as `FeeModel` (it is
//! part of the contract ABI); `FeeModel` converts losslessly into
//! `FeeIncidence` via [`From`], and the two share identical discriminants.
//!
//! # Semantics
//!
//! * [`FeeIncidence::FeeOnPot`] — the fee is `bps` of the **total pot**.
//!   The whole pool (winners' principal included) bears the fee.
//! * [`FeeIncidence::FeeOnWinnings`] — the fee is `bps` of **net winnings**
//!   only, i.e. the profit that winning participants realise. Winners get
//!   their principal back untouched.
//!
//! See `docs/FEE_MODEL.md` for the per-mode incidence tables and worked
//! examples.

/// Fee incidence model shared by the contract and the offline replay engine.
///
/// Discriminants are part of the contract ABI (mirrored by
/// `types::FeeModel`) and of the replay transcript format, so they must not
/// be reordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FeeIncidence {
    /// Fee charged on the total round pot (protocol default).
    FeeOnPot = 0,
    /// Fee charged only on net winnings / realised profit.
    FeeOnWinnings = 1,
}

impl FeeIncidence {
    /// Numeric discriminant used on-chain and in replay transcripts.
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// Decodes a transcript / storage discriminant.
    ///
    /// Unknown codes fall back to [`FeeIncidence::FeeOnPot`] so that a
    /// corrupted or future transcript can never be replayed as
    /// "no fee" or panic the audit tooling. This is the conservative
    /// choice: it reproduces the historical (pre-#268) behaviour and still
    /// lets `diagnostics` flag the mismatch via `total_fee`.
    pub const fn from_code(code: u32) -> Self {
        match code {
            1 => FeeIncidence::FeeOnWinnings,
            _ => FeeIncidence::FeeOnPot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FeeIncidence;

    #[test]
    fn codes_round_trip() {
        assert_eq!(FeeIncidence::FeeOnPot.code(), 0);
        assert_eq!(FeeIncidence::FeeOnWinnings.code(), 1);
        assert_eq!(FeeIncidence::from_code(0), FeeIncidence::FeeOnPot);
        assert_eq!(FeeIncidence::from_code(1), FeeIncidence::FeeOnWinnings);
    }

    #[test]
    fn unknown_codes_fall_back_to_fee_on_pot() {
        assert_eq!(FeeIncidence::from_code(2), FeeIncidence::FeeOnPot);
        assert_eq!(FeeIncidence::from_code(u32::MAX), FeeIncidence::FeeOnPot);
    }
}
