//! Logic with no interface: it builds for the browser and the server alike, and its tests run
//! natively and fast.

/// The counter after a press of a button. It stops at the ends instead of wrapping.
#[must_use]
pub const fn step(count: i64, delta: i64) -> i64 {
    count.saturating_add(delta)
}
