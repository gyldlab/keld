//! Canonical durable platform profile-data purge intent, strict record and barriers.
//! Platform effects, registry purge and lifecycle remain with existing owners.

use serde::{Deserialize, Serialize};

use super::{ProfileError, ProfileErrorKind, ProfileIdentity, ProfilePlatform};

/// Durable phase for an exact-identity platform data purge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfilePurgePhase {
    /// Intent is durable before platform data is removed.
    Prepared,
    /// The platform data removal or clear barrier completed.
    DataRemoved,
    /// Absence was verified before the intent is cleared.
    Completed,
}

/// Strict platform purge intent retained outside engine-managed data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfilePurgeRecord {
    identity: ProfileIdentity,
    platform: ProfilePlatform,
    phase: ProfilePurgePhase,
}

impl ProfilePurgeRecord {
    /// Creates the intent which must be durable before destructive work.
    #[must_use]
    pub const fn prepared(identity: ProfileIdentity, platform: ProfilePlatform) -> Self {
        Self {
            identity,
            platform,
            phase: ProfilePurgePhase::Prepared,
        }
    }

    /// Returns the exact identity authorized for this purge.
    #[must_use]
    pub const fn identity(&self) -> ProfileIdentity {
        self.identity
    }

    /// Returns the owning platform.
    #[must_use]
    pub const fn platform(&self) -> ProfilePlatform {
        self.platform
    }

    /// Returns the durable purge phase.
    #[must_use]
    pub const fn phase(&self) -> ProfilePurgePhase {
        self.phase
    }

    /// Advances one exact purge phase without skipping a barrier.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for skipped or reversed phases.
    pub fn advance(self, next: ProfilePurgePhase) -> Result<Self, ProfileError> {
        if !matches!(
            (self.phase, next),
            (ProfilePurgePhase::Prepared, ProfilePurgePhase::DataRemoved)
                | (ProfilePurgePhase::DataRemoved, ProfilePurgePhase::Completed)
        ) {
            return Err(ProfileError::new(ProfileErrorKind::ActiveIntent));
        }
        Ok(Self {
            phase: next,
            ..self
        })
    }

    /// Encodes canonical schema-v1 purge bytes.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` if serialization fails.
    pub fn to_record_bytes(&self) -> Result<Vec<u8>, ProfileError> {
        serde_json::to_vec(&ProfilePurgeDocument {
            schema: 1,
            profile_identity: self.identity.namespace_segment(),
            platform: self.platform,
            phase: self.phase,
        })
        .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))
    }

    /// Decodes stored purge data without granting purge authority.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for malformed or noncanonical data.
    pub fn from_record_bytes(bytes: &[u8]) -> Result<Self, ProfileError> {
        let document: ProfilePurgeDocument = serde_json::from_slice(bytes)
            .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))?;
        if document.schema != 1 {
            return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
        }
        Ok(Self {
            identity: ProfileIdentity::from_namespace_segment(&document.profile_identity)?,
            platform: document.platform,
            phase: document.phase,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfilePurgeDocument {
    schema: u8,
    profile_identity: String,
    platform: ProfilePlatform,
    phase: ProfilePurgePhase,
}
