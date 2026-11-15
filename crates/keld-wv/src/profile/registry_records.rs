//! Canonical schema-v1 durable registry binding and recovery-intent records.
//! Pure next-action policy remains in the sibling registry planner.

use serde::{Deserialize, Serialize};

use super::{AppleStoreUuid, ProfileError, ProfileErrorKind, ProfileIdentity};

/// Full identity-to-Apple-UUID tuple retained in both registry directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreBinding {
    pub(super) identity: ProfileIdentity,
    pub(super) store_uuid: AppleStoreUuid,
}

impl StoreBinding {
    /// Creates the one expected tuple for an identity.
    #[must_use]
    pub fn for_identity(identity: ProfileIdentity) -> Self {
        Self {
            identity,
            store_uuid: identity.apple_store_uuid(),
        }
    }

    /// Returns the full profile identity.
    #[must_use]
    pub const fn identity(&self) -> ProfileIdentity {
        self.identity
    }

    /// Returns the shorter deterministic Apple store UUID.
    #[must_use]
    pub const fn store_uuid(&self) -> AppleStoreUuid {
        self.store_uuid
    }

    /// Encodes one canonical schema-v1 forward or reverse record.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` if the record cannot be encoded.
    pub fn to_record_bytes(&self) -> Result<Vec<u8>, ProfileError> {
        serde_json::to_vec(&BindingDocument {
            schema: 1,
            profile_identity: self.identity.namespace_segment(),
            store_uuid: self.store_uuid.to_string(),
        })
        .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))
    }

    /// Decodes a stored tuple without granting identity-selection authority.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for malformed, noncanonical, wrong-schema, or
    /// identity-to-UUID-mismatched records.
    pub fn from_record_bytes(bytes: &[u8]) -> Result<Self, ProfileError> {
        let document: BindingDocument = serde_json::from_slice(bytes)
            .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))?;
        if document.schema != 1 {
            return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
        }
        let binding = Self {
            identity: ProfileIdentity::from_namespace_segment(&document.profile_identity)?,
            store_uuid: AppleStoreUuid::parse(&document.store_uuid)?,
        };
        if binding.store_uuid != binding.identity.apple_store_uuid() {
            return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
        }
        Ok(binding)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingDocument {
    schema: u8,
    profile_identity: String,
    store_uuid: String,
}

/// Durable creation-intent phase for crash-recoverable store binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingPhase {
    /// Intent exists; neither directional record has been authorized yet.
    Prepared,
    /// The reverse UUID record has been durably observed.
    ReverseWritten,
    /// Both directional records have been durably observed.
    ForwardWritten,
    /// Enumeration proved absence and authorized creation of this UUID.
    StoreCreationAuthorized,
    /// The created store identifier and persistence were verified.
    StoreVerified,
}

/// Durable purge-intent phase for ordered crash recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurgePhase {
    /// Purge intent exists while both binding records remain active.
    Prepared,
    /// Store enumeration authorized removal of this exact UUID.
    StoreRemovalAuthorized,
    /// Enumeration proved the store is absent after removal or prior cleanup.
    StoreAbsent,
    /// The reverse UUID record is absent.
    ReverseRemoved,
    /// Both directional records are absent.
    ForwardRemoved,
    /// The durable active-binding status is absent.
    Inactive,
}

/// Operation and exact phase retained by one durable registry intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "operation", content = "phase")]
pub enum RegistryIntentPhase {
    /// Crash-recoverable binding creation.
    Binding(BindingPhase),
    /// Crash-recoverable store and binding removal.
    Purging(PurgePhase),
}

/// Durable schema-v1 intent binding one operation to one full tuple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistryIntent {
    pub(super) binding: StoreBinding,
    pub(super) phase: RegistryIntentPhase,
}

impl RegistryIntent {
    /// Starts a new binding intent before either directional record is written.
    #[must_use]
    pub const fn binding(binding: StoreBinding) -> Self {
        Self {
            binding,
            phase: RegistryIntentPhase::Binding(BindingPhase::Prepared),
        }
    }

    /// Starts a purge intent before store removal or metadata deletion.
    #[must_use]
    pub const fn purging(binding: StoreBinding) -> Self {
        Self {
            binding,
            phase: RegistryIntentPhase::Purging(PurgePhase::Prepared),
        }
    }

    /// Returns the exact tuple protected by this intent.
    #[must_use]
    pub const fn store_binding(&self) -> StoreBinding {
        self.binding
    }

    /// Returns the operation and durable recovery phase.
    #[must_use]
    pub const fn phase(&self) -> RegistryIntentPhase {
        self.phase
    }

    /// Returns the same tuple advanced to a binding recovery phase.
    #[must_use]
    pub const fn with_binding_phase(self, phase: BindingPhase) -> Self {
        Self {
            binding: self.binding,
            phase: RegistryIntentPhase::Binding(phase),
        }
    }

    /// Returns the same tuple advanced to a purge recovery phase.
    #[must_use]
    pub const fn with_purge_phase(self, phase: PurgePhase) -> Self {
        Self {
            binding: self.binding,
            phase: RegistryIntentPhase::Purging(phase),
        }
    }

    /// Encodes the canonical strict schema-v1 intent bytes.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` if the record cannot be encoded.
    pub fn to_record_bytes(&self) -> Result<Vec<u8>, ProfileError> {
        serde_json::to_vec(&IntentDocument {
            schema: 1,
            profile_identity: self.binding.identity.namespace_segment(),
            store_uuid: self.binding.store_uuid.to_string(),
            state: self.phase,
        })
        .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))
    }

    /// Decodes stored intent bytes without authorizing an operation by itself.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for malformed, noncanonical, wrong-schema, or
    /// identity-to-UUID-mismatched records.
    pub fn from_record_bytes(bytes: &[u8]) -> Result<Self, ProfileError> {
        let document: IntentDocument = serde_json::from_slice(bytes)
            .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))?;
        if document.schema != 1 {
            return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
        }
        let binding = StoreBinding {
            identity: ProfileIdentity::from_namespace_segment(&document.profile_identity)?,
            store_uuid: AppleStoreUuid::parse(&document.store_uuid)?,
        };
        if binding.store_uuid != binding.identity.apple_store_uuid() {
            return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
        }
        Ok(Self {
            binding,
            phase: document.state,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IntentDocument {
    schema: u8,
    profile_identity: String,
    store_uuid: String,
    state: RegistryIntentPhase,
}
