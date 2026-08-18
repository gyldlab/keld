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
//! bsdiff/zstd patches with BLAKE3 post-conditions and ed25519-signed
//! manifests, static-host-compatible feeds, atomic swap with N-1 rollback,
//! on all three platforms. Normative spec:
//! zstd-compressed delta patches (diff algorithm selected by KEL-53 AC2,
//! not yet chosen — see below) with BLAKE3 post-conditions and
//! ed25519-signed manifests, static-host-compatible feeds, atomic swap
//! with N-1 rollback, on all three platforms. Normative spec:
//! `docs/architecture/06-runtime-and-tooling.md` §4 (§4a: the byte-level
//! manifest/feed wire contract — KEL-53's trigger condition); threat model
//! lives here with the code per `docs/architecture/03-security.md` §5.
//! Still a skeleton: §4a specifies the wire contract so KEL-53's fixtures
//! can be written as executable acceptance tests, but no code here reads or
//! verifies it yet — that is KEL-53 itself (bsdiff-vs-HDiffPatch benchmark,
//! then the dependency-review-gated `ed25519-dalek`/`zstd`/delta crate
//! additions), not this doc update.

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
