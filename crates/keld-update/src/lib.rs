//! keld-update — signed update-channel contracts.
//!
//! [`Channel`] names release-feed channels. The manifest/feed contract and update
//! lifecycle are in `docs/architecture/06-runtime-and-tooling.md` §4; the threat model
//! is in `docs/architecture/03-security.md` §5. The specified baseline verifies and
//! activates full packages before any optional delta optimization; this crate currently
//! implements only the channel type. Repository maturity and evidence live in
//! `docs/engineering/product-status.tsv`.

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
