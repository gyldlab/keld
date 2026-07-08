//! keld-update — delta updates as a default.
//!
//! bsdiff/zstd patches with BLAKE3 post-conditions and ed25519-signed
//! manifests, static-host-compatible feeds, atomic swap with N-1 rollback,
//! on all three platforms. Normative spec:
//! `docs/architecture/06-runtime-and-tooling.md` §4; threat model lives here
//! with the code per `docs/architecture/03-security.md` §5.

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
