//! Synchronization primitives

mod cancellation_token;
pub use cancellation_token::{
    guard::DropGuard, guard_ref::DropGuardRef, CancellationToken, WaitForCancellationFuture,
    WaitForCancellationFutureOwned,
};
#[path = "../../../../../../test/foundation/runtime/scheduled-io/unit/src/sync/tests/mod.rs"]
#[cfg(test)]
mod tests;
