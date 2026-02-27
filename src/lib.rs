//! gameQA — non-intrusive game data collection pipeline.
//!
//! All production paths must use `?` for error propagation.
//! `unwrap()` and `expect()` are forbidden in non-test code.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod audit;
pub mod automation;
pub mod capture;
pub mod config;
pub mod debug;
pub mod hooks;
pub mod pipeline;
pub mod roi;
pub mod session;

// ─── Newtype wrappers for clock-source distinction ────────────────────────────
//
// Using the *newtype pattern* (NOT type aliases). Type aliases (`type MonotonicNs = u64`)
// collapse to the same underlying type, allowing silent mixing. These structs are
// **distinct types**: passing `WallNs` where `MonotonicNs` is expected is a compile error.

/// Nanoseconds elapsed since session start (`std::time::Instant`, monotonic).
///
/// Used for ALL frame and event timestamps within a session.
///
/// **Compile-enforced**: `MonotonicNs` cannot be used where `WallNs` is expected,
/// and vice versa.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct MonotonicNs(pub u64);

/// Nanoseconds since UNIX epoch (`std::time::SystemTime`, wall-clock).
///
/// Used ONLY in `session.json` for anchoring absolute time and external log joining.
///
/// **Compile-enforced**: `WallNs` cannot be used where `MonotonicNs` is expected,
/// and vice versa.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct WallNs(pub u64);

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that MonotonicNs and WallNs are NOT the same type.
    /// This test documents the compile-time guarantee; it will fail to compile
    /// if someone replaces the newtypes with type aliases.
    #[test]
    fn newtype_distinct_types() {
        let mono = MonotonicNs(1_000_000);
        let wall = WallNs(1_000_000);

        // Both hold the same inner value but are different types.
        assert_eq!(mono.0, wall.0);

        // The following would be a COMPILE ERROR (which is the desired behavior):
        // let _bad: MonotonicNs = wall;
        // let _bad: WallNs = mono;
    }

    #[test]
    fn monotonic_ns_ordering() {
        let a = MonotonicNs(100);
        let b = MonotonicNs(200);
        assert!(a < b);
    }

    #[test]
    fn wall_ns_ordering() {
        let a = WallNs(100);
        let b = WallNs(200);
        assert!(a < b);
    }
}
