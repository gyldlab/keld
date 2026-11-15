//! Exact profile-leaf ownership marker schema and fail-closed admission policy.
//! Path validation and profile identity authority stay owned by platform/parent.

use serde::{Deserialize, Serialize};

use super::{ProfileError, ProfileErrorKind, ProfileIdentity, ProfilePlatform, ProfileRootRole};

/// Schema-v1 ownership marker for a Keld-owned logical profile leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileMarker {
    identity: ProfileIdentity,
    platform: ProfilePlatform,
    root_role: ProfileRootRole,
}

impl ProfileMarker {
    /// Constructs the exact marker expected at a validated logical leaf.
    #[must_use]
    pub const fn new(
        identity: ProfileIdentity,
        platform: ProfilePlatform,
        root_role: ProfileRootRole,
    ) -> Self {
        Self {
            identity,
            platform,
            root_role,
        }
    }

    /// Returns the full profile identity bound by this marker.
    #[must_use]
    pub const fn identity(&self) -> ProfileIdentity {
        self.identity
    }

    /// Returns the platform role bound by this marker.
    #[must_use]
    pub const fn platform(&self) -> ProfilePlatform {
        self.platform
    }

    /// Returns the logical root role bound by this marker.
    #[must_use]
    pub const fn root_role(&self) -> ProfileRootRole {
        self.root_role
    }

    /// Encodes the canonical strict schema-v1 marker bytes.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` if the record cannot be encoded.
    pub fn to_record_bytes(&self) -> Result<Vec<u8>, ProfileError> {
        serde_json::to_vec(&MarkerDocument {
            schema: 1,
            profile_identity: self.identity.namespace_segment(),
            platform: self.platform,
            root_role: self.root_role,
        })
        .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))
    }

    /// Decodes stored marker bytes without granting identity-selection authority.
    ///
    /// The returned identity proves only what the record says. Callers must
    /// compare it to an independently derived expected identity before reuse.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for malformed, unknown, duplicate, wrong-schema,
    /// or noncanonical fields.
    pub fn from_record_bytes(bytes: &[u8]) -> Result<Self, ProfileError> {
        let document: MarkerDocument = serde_json::from_slice(bytes)
            .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))?;
        if document.schema != 1 {
            return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
        }
        Ok(Self {
            identity: ProfileIdentity::from_namespace_segment(&document.profile_identity)?,
            platform: document.platform,
            root_role: document.root_role,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkerDocument {
    schema: u8,
    profile_identity: String,
    platform: ProfilePlatform,
    root_role: ProfileRootRole,
}

/// Caller-observed state of a validated logical profile leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerObservation {
    /// The caller created this leaf atomically and proved it is still empty.
    NewlyCreatedEmpty,
    /// A pre-existing leaf contains a decoded ownership marker.
    ExistingMarked(ProfileMarker),
    /// A pre-existing empty leaf has no ownership marker.
    ExistingEmptyUnmarked,
    /// A pre-existing nonempty leaf has no ownership marker.
    ExistingNonemptyUnmarked,
    /// A link or reparse point was observed in the validated path.
    LinkOrReparse,
    /// The final resolved path escaped the independently retained root.
    EscapingFinalPath,
    /// Platform ownership or permission validation failed.
    UnsafeOwnerOrPermissions,
}

/// Next marker operation permitted by the common policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerAction {
    /// Atomically create and durably persist this exact marker.
    WriteNew(ProfileMarker),
    /// Reuse the leaf whose existing marker exactly matches.
    Reuse,
}

/// Selects the only safe marker action for an independently validated leaf.
///
/// # Errors
///
/// Returns `KELD-WV-009` unless an existing marker matches exactly or a newly
/// created leaf is still empty.
pub fn next_marker_action(
    expected: ProfileMarker,
    observation: MarkerObservation,
) -> Result<MarkerAction, ProfileError> {
    match observation {
        MarkerObservation::NewlyCreatedEmpty => Ok(MarkerAction::WriteNew(expected)),
        MarkerObservation::ExistingMarked(actual) if actual == expected => Ok(MarkerAction::Reuse),
        MarkerObservation::ExistingMarked(_)
        | MarkerObservation::ExistingEmptyUnmarked
        | MarkerObservation::ExistingNonemptyUnmarked
        | MarkerObservation::LinkOrReparse
        | MarkerObservation::EscapingFinalPath
        | MarkerObservation::UnsafeOwnerOrPermissions => {
            Err(ProfileError::new(ProfileErrorKind::MarkerMismatch))
        }
    }
}
