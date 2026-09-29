// SPDX-License-Identifier: MIT
//! Fee incidence compiled from the contract source tree (same code path as live settlement).

#[path = "../../contracts/src/fee_incidence.rs"]
mod inner;

pub use inner::*;
