//! keld-update — signed delta updates (destination).
//!
//! **v0:** this crate exports [`Channel`] only — no patch engine, manifest
//! parser, feed client, or signing yet. `[dependencies]` is empty.
//!
//! **Destination:** bsdiff/zstd patches with BLAKE3 post-conditions and
//! ed25519-signed manifests, static-host-compatible feeds, atomic swap with
//! N-1 rollback on all three platforms. Normative spec:
//! `docs/architecture/06-runtime-and-tooling.md` §4. Threat model:
//! `docs/architecture/03-security.md` §5 (implementation lands with the updater).

/// Release channels supported by update feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Channel {
    /// Production releases.
    #[default]
    Stable,
    /// Pre-release testing.
    Beta,
    /// Continuous builds.
    Canary,
}
