//! Stable typed KELD-WV-009 profile failure classification and diagnostic rendering.
//! All existing callers retain the same error types, text and context semantics.

use core::fmt;

/// Stable category for a fail-closed profile operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileErrorKind {
    /// The signed application identifier is not in canonical form.
    InvalidAppId,
    /// Release startup has no authenticated application identity.
    MissingAuthenticatedIdentity,
    /// Host randomness for an ephemeral development session is missing.
    InvalidEphemeralNonce,
    /// A schema-v1 profile record is malformed or noncanonical.
    InvalidRecord,
    /// A marker does not prove ownership of the expected logical leaf.
    MarkerMismatch,
    /// Bidirectional store metadata is incomplete, conflicting, or corrupt.
    RegistryCorruption,
    /// A durable profile intent blocks the requested operation.
    ActiveIntent,
    /// Another owner already holds the same validated profile lease key.
    ProfileInUse,
    /// Lock acquisition or release violates the global order.
    LockOrderViolation,
    /// Durable lifecycle state cannot safely admit or recover startup.
    LifecycleUnproven,
}

/// Typed `KELD-WV-009` profile-selection failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileError {
    kind: ProfileErrorKind,
    context: Option<&'static str>,
}

impl ProfileError {
    pub(super) const fn new(kind: ProfileErrorKind) -> Self {
        Self {
            kind,
            context: None,
        }
    }

    /// Returns the stable failure category without parsing display text.
    #[must_use]
    pub const fn kind(&self) -> ProfileErrorKind {
        self.kind
    }

    /// Returns the platform-owned detail naming the exact barrier or
    /// observable that failed, when the failing site recorded one.
    #[must_use]
    pub const fn context(&self) -> Option<&'static str> {
        self.context
    }

    /// Constructs the release-mode missing-identity failure.
    #[must_use]
    pub const fn missing_authenticated_identity() -> Self {
        Self::new(ProfileErrorKind::MissingAuthenticatedIdentity)
    }

    #[cfg(target_os = "windows")]
    pub(crate) const fn platform_failure(kind: ProfileErrorKind) -> Self {
        Self::new(kind)
    }

    /// Windows failure that names the exact barrier or observable that failed.
    #[cfg(target_os = "windows")]
    pub(crate) const fn platform_failure_with_context(
        kind: ProfileErrorKind,
        context: &'static str,
    ) -> Self {
        Self {
            kind,
            context: Some(context),
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) const fn platform_failure(kind: ProfileErrorKind) -> Self {
        Self::new(kind)
    }
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = match self.kind {
            ProfileErrorKind::InvalidAppId => {
                "the authenticated app id is not canonical. Use 1-255 bytes of lowercase ASCII dot-separated segments whose edges are alphanumeric"
            }
            ProfileErrorKind::MissingAuthenticatedIdentity => {
                "release startup has no verified package identity. Verify the package publisher and signed app id, or use an explicit ephemeral development session"
            }
            ProfileErrorKind::InvalidEphemeralNonce => {
                "the development launch nonce is missing. Mint fresh host-owned OS randomness for this launch; do not select a persistent or shared temporary store"
            }
            ProfileErrorKind::InvalidRecord => {
                "a profile control record is malformed or noncanonical. Preserve the schema-v1 record and repair it through the owning recovery flow"
            }
            ProfileErrorKind::MarkerMismatch => {
                "the profile ownership marker does not match the expected identity, platform, and root role. Refuse the leaf and repair or purge it through its validated owner"
            }
            ProfileErrorKind::RegistryCorruption => {
                "the profile registry cannot prove one full-identity/store binding. Resume its matching durable intent or repair the conflicting metadata before store lookup"
            }
            ProfileErrorKind::ActiveIntent => {
                "a durable profile intent blocks normal lookup. Resume the exact binding or purge operation before starting a webview"
            }
            ProfileErrorKind::ProfileInUse => {
                "the validated profile is already in use. Close the owning host and wait for the platform release barrier; do not create a suffix, default, or temporary fallback"
            }
            ProfileErrorKind::LockOrderViolation => {
                "profile locks were acquired or released out of order. Use package lifecycle, platform registry, profile lease, then engine/store intent order"
            }
            ProfileErrorKind::LifecycleUnproven => {
                "durable profile lifecycle state cannot prove safe reuse because recovery state cannot be proven on this platform/version. If persistent profiles are unsupported here, use explicit ephemeral mode; otherwise complete engine-release or boot recovery before lookup"
            }
        };
        write!(f, "KELD-WV-009: profile selection failed: {detail}.")?;
        if let Some(context) = self.context {
            write!(f, " Observed: {context}.")?;
        }
        Ok(())
    }
}

impl std::error::Error for ProfileError {}
