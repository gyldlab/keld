//! Host-owned echo link — re-exported from `keld-core` (KEL-30).
//!
//! The listener/token mint lives in the host stack so `keld-host` never needs
//! a dependency on this diagnostic CLI crate.

#[cfg(windows)]
pub use keld_core::EchoEndpoint;
pub use keld_core::{EchoServer, echo_roundtrip};
