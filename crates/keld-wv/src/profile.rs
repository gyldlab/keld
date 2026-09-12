//! Host-owned persistent webview profile identity and common state policy.
//!
//! The public constructor in this module is a host-TCB boundary. It validates
//! canonical bytes, but its publisher input is trustworthy only after a
//! platform package verifier has authenticated those bytes. KEL-135/T1 adds no
//! such verifier and selects no native store.

use core::fmt;
use std::{collections::BTreeMap, sync::Arc};

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest as _, Sha256};

const PROFILE_IDENTITY_DOMAIN: &[u8] = b"keld.profile.identity/v1\0";
const APPLE_STORE_DOMAIN: &[u8] = b"keld.wk-store/v1\0";

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
}

impl ProfileError {
    const fn new(kind: ProfileErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the stable failure category without parsing display text.
    #[must_use]
    pub const fn kind(&self) -> ProfileErrorKind {
        self.kind
    }

    /// Constructs the release-mode missing-identity failure.
    #[must_use]
    pub const fn missing_authenticated_identity() -> Self {
        Self::new(ProfileErrorKind::MissingAuthenticatedIdentity)
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
                "durable profile lifecycle state cannot prove safe reuse. Complete the platform release oracle or required boot-scoped quarantine recovery before lookup"
            }
        };
        write!(f, "KELD-WV-009: profile selection failed: {detail}.")
    }
}

impl std::error::Error for ProfileError {}

/// Opaque 32-byte identity derived from authenticated publisher and app-id parts.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileIdentity([u8; 32]);

impl ProfileIdentity {
    /// Derives an identity from bytes already authenticated by trusted host code.
    ///
    /// This function validates the app-id syntax and applies the approved hash
    /// construction. It does not authenticate the publisher scope or app id;
    /// callers must obtain both from a platform package verifier.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when the app id is not canonical.
    pub fn from_host_verified_parts(
        publisher_scope: [u8; 32],
        canonical_app_id: &str,
    ) -> Result<Self, ProfileError> {
        validate_canonical_app_id(canonical_app_id)?;
        let app_id_len = u16::try_from(canonical_app_id.len())
            .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidAppId))?;

        let mut hasher = Sha256::new();
        hasher.update(PROFILE_IDENTITY_DOMAIN);
        hasher.update(publisher_scope);
        hasher.update(app_id_len.to_be_bytes());
        hasher.update(canonical_app_id.as_bytes());
        let digest = hasher.finalize();
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(&digest);
        Ok(Self(bytes))
    }

    /// Returns the full identity bytes retained in every ownership record.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns the only canonical filesystem segment for this identity.
    #[must_use]
    pub fn namespace_segment(&self) -> String {
        encode_lower_hex(&self.0)
    }

    /// Derives the deterministic RFC-variant `UUIDv8` bytes for a macOS store.
    #[must_use]
    pub fn apple_store_uuid(&self) -> AppleStoreUuid {
        let mut hasher = Sha256::new();
        hasher.update(APPLE_STORE_DOMAIN);
        hasher.update(self.0);
        let digest = hasher.finalize();
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[6] = (bytes[6] & 0x0f) | 0x80;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        AppleStoreUuid(bytes)
    }

    fn from_namespace_segment(value: &str) -> Result<Self, ProfileError> {
        decode_lower_hex::<32>(value).map(Self)
    }
}

impl fmt::Debug for ProfileIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ProfileIdentity")
            .field(&self.namespace_segment())
            .finish()
    }
}

/// Deterministic `UUIDv8` used only as the shorter Apple store identifier.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppleStoreUuid([u8; 16]);

impl AppleStoreUuid {
    /// Returns the canonical UUID bytes with RFC variant and version 8 bits.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    fn parse(value: &str) -> Result<Self, ProfileError> {
        if value.len() != 36
            || value.as_bytes().get(8) != Some(&b'-')
            || value.as_bytes().get(13) != Some(&b'-')
            || value.as_bytes().get(18) != Some(&b'-')
            || value.as_bytes().get(23) != Some(&b'-')
        {
            return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
        }
        let compact: String = value
            .chars()
            .filter(|character| *character != '-')
            .collect();
        let bytes = decode_lower_hex::<16>(&compact)?;
        if bytes[6] >> 4 != 8 || bytes[8] >> 6 != 2 {
            return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
        }
        Ok(Self(bytes))
    }
}

impl fmt::Display for AppleStoreUuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = encode_lower_hex(&self.0);
        write!(
            f,
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }
}

impl fmt::Debug for AppleStoreUuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Host-minted identity for one explicitly ephemeral development launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EphemeralProfile {
    launch_nonce: [u8; 32],
}

impl EphemeralProfile {
    /// Creates a launch identity from fresh OS randomness owned by the host.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when the nonce is all zeroes.
    pub fn from_host_random(launch_nonce: [u8; 32]) -> Result<Self, ProfileError> {
        if launch_nonce.iter().all(|byte| *byte == 0) {
            return Err(ProfileError::new(ProfileErrorKind::InvalidEphemeralNonce));
        }
        Ok(Self { launch_nonce })
    }

    /// Returns the opaque nonce for a platform's owner-private ephemeral store.
    #[must_use]
    pub const fn launch_nonce(&self) -> &[u8; 32] {
        &self.launch_nonce
    }
}

/// Host-selected profile mode before any platform store is constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebProfileSelection {
    /// Persistent release identity supplied by an authenticated package verifier.
    Persistent(ProfileIdentity),
    /// Explicit per-launch development state with no persistent identity fallback.
    EphemeralDev(EphemeralProfile),
}

impl WebProfileSelection {
    /// Selects a persistent release profile or fails when identity is absent.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when release identity is absent.
    pub fn persistent(identity: Option<ProfileIdentity>) -> Result<Self, ProfileError> {
        identity
            .map(Self::Persistent)
            .ok_or_else(ProfileError::missing_authenticated_identity)
    }

    /// Selects one explicit per-launch ephemeral development profile.
    #[must_use]
    pub const fn ephemeral_dev(profile: EphemeralProfile) -> Self {
        Self::EphemeralDev(profile)
    }
}

/// Platform named by a Keld-owned profile marker or lease key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfilePlatform {
    /// Microsoft Windows and the `WebView2` profile policy.
    Windows,
    /// Apple macOS and the identified website-data-store policy.
    Macos,
    /// Linux and the explicit `WebKitGTK` manager policy.
    Linux,
}

/// Logical role of one Keld-owned profile leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileRootRole {
    /// Nondeletable control state containing leases and durable intents.
    Control,
    /// Persistent engine data root.
    Data,
    /// Persistent engine cache root.
    Cache,
    /// Windows `WebView2` user-data child.
    WebView2,
    /// macOS Keld metadata registry.
    Metadata,
    /// One owner-private development launch leaf.
    Ephemeral,
}

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

/// Full identity-to-Apple-UUID tuple retained in both registry directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreBinding {
    identity: ProfileIdentity,
    store_uuid: AppleStoreUuid,
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
    binding: StoreBinding,
    phase: RegistryIntentPhase,
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

/// One directional registry-record observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryRecord {
    /// No durable record exists at this direction's exact key.
    Missing,
    /// The record exists and decoded to this tuple.
    Present(StoreBinding),
}

/// Result of platform store enumeration for the requested UUID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreObservation {
    /// Enumeration has not run for this transition yet.
    Unknown,
    /// The requested UUID is absent.
    Absent,
    /// The requested UUID exists with the observed persistence property.
    Present {
        /// Identifier reported by the platform store.
        uuid: AppleStoreUuid,
        /// Whether the platform reports this store as persistent.
        persistent: bool,
    },
}

/// Complete observations consumed by one pure registry transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistrySnapshot {
    intent: Option<RegistryIntent>,
    forward: RegistryRecord,
    reverse: RegistryRecord,
    store: StoreObservation,
    active: bool,
}

impl RegistrySnapshot {
    /// Creates a snapshot from independently read durable/platform observations.
    #[must_use]
    pub const fn new(
        intent: Option<RegistryIntent>,
        forward: RegistryRecord,
        reverse: RegistryRecord,
        store: StoreObservation,
        active: bool,
    ) -> Self {
        Self {
            intent,
            forward,
            reverse,
            store,
            active,
        }
    }
}

/// Requested registry operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryRequest {
    /// Create, recover, or reuse an exact identity-to-UUID binding.
    Bind(StoreBinding),
    /// Purge the exact bound store and remove its metadata in order.
    Purge(StoreBinding),
}

/// One idempotent operation emitted by the common registry state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryAction {
    /// Persist the initial binding intent before metadata changes.
    WriteBindingIntent(RegistryIntent),
    /// Persist the initial purge intent before destructive change.
    WritePurgeIntent(RegistryIntent),
    /// Atomically create the reverse UUID record without replacement.
    WriteReverseRecord(StoreBinding),
    /// Atomically create the forward identity record without replacement.
    WriteForwardRecord(StoreBinding),
    /// Advance and fsync the binding intent to this phase.
    AdvanceBindingIntent(BindingPhase),
    /// Advance and fsync the purge intent to this phase.
    AdvancePurgeIntent(PurgePhase),
    /// Enumerate the public platform store identifiers.
    EnumerateStores,
    /// Construct the exact identified persistent store after authorization.
    ConstructStore(StoreBinding),
    /// Durably mark the verified two-way binding active.
    MarkBindingActive,
    /// Reuse the exact active persistent store.
    ReuseStore,
    /// Remove the exact platform store and await its completion barrier.
    RemoveStore(StoreBinding),
    /// Remove the reverse UUID record first.
    RemoveReverseRecord,
    /// Remove the forward identity record after the reverse record.
    RemoveForwardRecord,
    /// Remove the active-binding status after both records are absent.
    ClearActiveStatus,
    /// Clear the completed durable intent last.
    ClearIntent,
}

/// Computes one independently observable registry recovery action.
///
/// # Errors
///
/// Returns `KELD-WV-009` for conflicting or incomplete records, an operation
/// blocked by the other intent kind, an unowned pre-existing store, or an
/// active binding whose store disappeared.
pub fn next_registry_action(
    request: RegistryRequest,
    snapshot: RegistrySnapshot,
) -> Result<RegistryAction, ProfileError> {
    match request {
        RegistryRequest::Bind(binding) => next_binding_action(binding, snapshot),
        RegistryRequest::Purge(binding) => next_purge_action(binding, snapshot),
    }
}

fn next_binding_action(
    binding: StoreBinding,
    snapshot: RegistrySnapshot,
) -> Result<RegistryAction, ProfileError> {
    let forward = record_matches(snapshot.forward, binding)?;
    let reverse = record_matches(snapshot.reverse, binding)?;
    let store = store_matches(snapshot.store, binding)?;
    let Some(intent) = snapshot.intent else {
        if !forward && !reverse && !snapshot.active && store != Some(true) {
            return Ok(RegistryAction::WriteBindingIntent(RegistryIntent::binding(
                binding,
            )));
        }
        if forward && reverse && snapshot.active {
            return match store {
                None => Ok(RegistryAction::EnumerateStores),
                Some(true) => Ok(RegistryAction::ReuseStore),
                Some(false) => Err(ProfileError::new(ProfileErrorKind::RegistryCorruption)),
            };
        }
        return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
    };
    if intent.binding != binding {
        return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
    }
    let RegistryIntentPhase::Binding(phase) = intent.phase else {
        return Err(ProfileError::new(ProfileErrorKind::ActiveIntent));
    };
    match phase {
        BindingPhase::Prepared => {
            if forward || snapshot.active {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            if reverse {
                Ok(RegistryAction::AdvanceBindingIntent(
                    BindingPhase::ReverseWritten,
                ))
            } else {
                Ok(RegistryAction::WriteReverseRecord(binding))
            }
        }
        BindingPhase::ReverseWritten => {
            if !reverse || snapshot.active {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            if forward {
                Ok(RegistryAction::AdvanceBindingIntent(
                    BindingPhase::ForwardWritten,
                ))
            } else {
                Ok(RegistryAction::WriteForwardRecord(binding))
            }
        }
        BindingPhase::ForwardWritten => {
            require_complete_inactive_binding(forward, reverse, snapshot.active)?;
            match store {
                None => Ok(RegistryAction::EnumerateStores),
                Some(false) => Ok(RegistryAction::AdvanceBindingIntent(
                    BindingPhase::StoreCreationAuthorized,
                )),
                Some(true) => Err(ProfileError::new(ProfileErrorKind::RegistryCorruption)),
            }
        }
        BindingPhase::StoreCreationAuthorized => {
            require_complete_inactive_binding(forward, reverse, snapshot.active)?;
            match store {
                None => Ok(RegistryAction::EnumerateStores),
                Some(false) => Ok(RegistryAction::ConstructStore(binding)),
                Some(true) => Ok(RegistryAction::AdvanceBindingIntent(
                    BindingPhase::StoreVerified,
                )),
            }
        }
        BindingPhase::StoreVerified => {
            if !forward || !reverse || store != Some(true) {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            if snapshot.active {
                Ok(RegistryAction::ClearIntent)
            } else {
                Ok(RegistryAction::MarkBindingActive)
            }
        }
    }
}

fn next_purge_action(
    binding: StoreBinding,
    snapshot: RegistrySnapshot,
) -> Result<RegistryAction, ProfileError> {
    let forward = record_matches(snapshot.forward, binding)?;
    let reverse = record_matches(snapshot.reverse, binding)?;
    let store = store_matches(snapshot.store, binding)?;
    let Some(intent) = snapshot.intent else {
        if forward && reverse && snapshot.active {
            return Ok(RegistryAction::WritePurgeIntent(RegistryIntent::purging(
                binding,
            )));
        }
        return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
    };
    if intent.binding != binding {
        return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
    }
    let RegistryIntentPhase::Purging(phase) = intent.phase else {
        return Err(ProfileError::new(ProfileErrorKind::ActiveIntent));
    };
    match phase {
        PurgePhase::Prepared => {
            if !forward || !reverse || !snapshot.active {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            match store {
                None => Ok(RegistryAction::EnumerateStores),
                Some(true) => Ok(RegistryAction::AdvancePurgeIntent(
                    PurgePhase::StoreRemovalAuthorized,
                )),
                Some(false) => Ok(RegistryAction::AdvancePurgeIntent(PurgePhase::StoreAbsent)),
            }
        }
        PurgePhase::StoreRemovalAuthorized => {
            if !forward || !reverse || !snapshot.active {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            match store {
                None => Ok(RegistryAction::EnumerateStores),
                Some(true) => Ok(RegistryAction::RemoveStore(binding)),
                Some(false) => Ok(RegistryAction::AdvancePurgeIntent(PurgePhase::StoreAbsent)),
            }
        }
        PurgePhase::StoreAbsent => {
            if store != Some(false) || !snapshot.active || !forward {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            if reverse {
                Ok(RegistryAction::RemoveReverseRecord)
            } else {
                Ok(RegistryAction::AdvancePurgeIntent(
                    PurgePhase::ReverseRemoved,
                ))
            }
        }
        PurgePhase::ReverseRemoved => {
            if store != Some(false) || reverse || !snapshot.active {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            if forward {
                Ok(RegistryAction::RemoveForwardRecord)
            } else {
                Ok(RegistryAction::AdvancePurgeIntent(
                    PurgePhase::ForwardRemoved,
                ))
            }
        }
        PurgePhase::ForwardRemoved => {
            if store != Some(false) || forward || reverse {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            if snapshot.active {
                Ok(RegistryAction::ClearActiveStatus)
            } else {
                Ok(RegistryAction::AdvancePurgeIntent(PurgePhase::Inactive))
            }
        }
        PurgePhase::Inactive => {
            if store == Some(false) && !forward && !reverse && !snapshot.active {
                Ok(RegistryAction::ClearIntent)
            } else {
                Err(ProfileError::new(ProfileErrorKind::RegistryCorruption))
            }
        }
    }
}

fn record_matches(record: RegistryRecord, expected: StoreBinding) -> Result<bool, ProfileError> {
    match record {
        RegistryRecord::Missing => Ok(false),
        RegistryRecord::Present(actual) if actual == expected => Ok(true),
        RegistryRecord::Present(_) => Err(ProfileError::new(ProfileErrorKind::RegistryCorruption)),
    }
}

fn store_matches(
    observation: StoreObservation,
    expected: StoreBinding,
) -> Result<Option<bool>, ProfileError> {
    match observation {
        StoreObservation::Unknown => Ok(None),
        StoreObservation::Absent => Ok(Some(false)),
        StoreObservation::Present {
            uuid,
            persistent: true,
        } if uuid == expected.store_uuid => Ok(Some(true)),
        StoreObservation::Present { .. } => {
            Err(ProfileError::new(ProfileErrorKind::RegistryCorruption))
        }
    }
}

fn require_complete_inactive_binding(
    forward: bool,
    reverse: bool,
    active: bool,
) -> Result<(), ProfileError> {
    if forward && reverse && !active {
        Ok(())
    } else {
        Err(ProfileError::new(ProfileErrorKind::RegistryCorruption))
    }
}

/// Exact common key whose native realization must be exclusively owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProfileLeaseKey {
    identity: ProfileIdentity,
    platform: ProfilePlatform,
    user_scope: [u8; 32],
    namespace_roots: [u8; 32],
}

impl ProfileLeaseKey {
    /// Creates a key from an identity and already validated user/root facts.
    ///
    /// The byte arrays are opaque stable fingerprints produced by the future
    /// platform owner after validating the current OS user and namespace roots.
    #[must_use]
    pub const fn from_host_validated_parts(
        identity: ProfileIdentity,
        platform: ProfilePlatform,
        user_scope: [u8; 32],
        namespace_roots: [u8; 32],
    ) -> Self {
        Self {
            identity,
            platform,
            user_scope,
            namespace_roots,
        }
    }
}

/// Opaque process-lifetime owner identity supplied by trusted host code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LeaseOwner([u8; 16]);

impl LeaseOwner {
    /// Validates an opaque host process identity for lease bookkeeping.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when the process identity is absent.
    pub fn from_host_process_identity(bytes: [u8; 16]) -> Result<Self, ProfileError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self(bytes))
    }
}

/// Unforgeable token returned by the common exclusive lease table.
#[derive(Debug)]
pub struct ProfileLease {
    key: ProfileLeaseKey,
    owner: LeaseOwner,
    generation: u64,
    capability: Arc<LeaseTableCapability>,
}

impl ProfileLease {
    /// Returns the exact validated lease key.
    #[must_use]
    pub const fn key(&self) -> ProfileLeaseKey {
        self.key
    }

    /// Returns the host process identity that acquired the token.
    #[must_use]
    pub const fn owner(&self) -> LeaseOwner {
        self.owner
    }
}

impl PartialEq for ProfileLease {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.owner == other.owner
            && self.generation == other.generation
            && Arc::ptr_eq(&self.capability, &other.capability)
    }
}

impl Eq for ProfileLease {}

/// In-process common exclusivity model used in addition to native platform locks.
///
/// This table cannot prove cross-process exclusion. T2, T3, and T4 must pair the
/// same key policy with a real OS lease and engine release evidence.
#[derive(Debug)]
pub struct ProfileLeaseTable {
    owners: BTreeMap<ProfileLeaseKey, (LeaseOwner, u64)>,
    next_generation: u64,
    capability: Arc<LeaseTableCapability>,
}

#[derive(Debug)]
struct LeaseTableCapability;

impl Default for ProfileLeaseTable {
    fn default() -> Self {
        Self {
            owners: BTreeMap::new(),
            next_generation: 0,
            capability: Arc::new(LeaseTableCapability),
        }
    }
}

impl ProfileLeaseTable {
    /// Acquires the exact key or rejects every same-key second owner.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when any owner already holds the key.
    pub fn acquire(
        &mut self,
        key: ProfileLeaseKey,
        owner: LeaseOwner,
    ) -> Result<ProfileLease, ProfileError> {
        if self.owners.contains_key(&key) {
            return Err(ProfileError::new(ProfileErrorKind::ProfileInUse));
        }
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or_else(|| ProfileError::new(ProfileErrorKind::LifecycleUnproven))?;
        let generation = self.next_generation;
        self.owners.insert(key, (owner, generation));
        Ok(ProfileLease {
            key,
            owner,
            generation,
            capability: Arc::clone(&self.capability),
        })
    }

    /// Releases only the exact token previously returned by this table.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when the token is absent or does not match.
    pub fn release(&mut self, lease: &ProfileLease) -> Result<(), ProfileError> {
        if !Arc::ptr_eq(&self.capability, &lease.capability)
            || self.owners.get(&lease.key) != Some(&(lease.owner, lease.generation))
        {
            return Err(ProfileError::new(ProfileErrorKind::ProfileInUse));
        }
        self.owners.remove(&lease.key);
        Ok(())
    }
}

/// Global lock levels ordered from outer lifecycle authority to engine action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProfileLockLevel {
    /// Shared ordinary-start or exclusive update/rollback/uninstall lock.
    PackageLifecycle,
    /// Platform reverse-index registry lock.
    PlatformRegistry,
    /// Exact profile-identity control lease.
    ProfileLease,
    /// Platform engine/store operation and durable intent.
    EngineStoreIntent,
}

/// Pure acquisition/release-order model; it does not implement an OS lock.
#[derive(Debug, Default)]
pub struct ProfileLockOrder {
    held: Vec<ProfileLockLevel>,
}

impl ProfileLockOrder {
    /// Records an acquisition only when its level strictly follows held locks.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for inversion or same-level re-entry.
    pub fn acquire(&mut self, level: ProfileLockLevel) -> Result<(), ProfileError> {
        let allowed = matches!(
            (self.held.as_slice(), level),
            ([], ProfileLockLevel::PackageLifecycle)
                | (
                    [ProfileLockLevel::PackageLifecycle],
                    ProfileLockLevel::PlatformRegistry | ProfileLockLevel::ProfileLease
                )
                | (
                    [
                        ProfileLockLevel::PackageLifecycle,
                        ProfileLockLevel::PlatformRegistry
                    ],
                    ProfileLockLevel::ProfileLease
                )
                | (
                    [.., ProfileLockLevel::ProfileLease],
                    ProfileLockLevel::EngineStoreIntent
                )
        );
        if !allowed {
            return Err(ProfileError::new(ProfileErrorKind::LockOrderViolation));
        }
        self.held.push(level);
        Ok(())
    }

    /// Records a release only in exact reverse acquisition order.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for a non-LIFO or unheld release.
    pub fn release(&mut self, level: ProfileLockLevel) -> Result<(), ProfileError> {
        if self.held.last() != Some(&level) {
            return Err(ProfileError::new(ProfileErrorKind::LockOrderViolation));
        }
        self.held.pop();
        Ok(())
    }
}

/// Strict boot-scoped identity bytes supplied by a future platform parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BootIdentity([u8; 16]);

impl BootIdentity {
    /// Accepts nonzero bytes after the platform owner validated its UUID source.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when the boot identity is absent.
    pub fn from_host_verified_bytes(bytes: [u8; 16]) -> Result<Self, ProfileError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self(bytes))
    }
}

impl<'de> Deserialize<'de> for BootIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = <[u8; 16]>::deserialize(deserializer)?;
        Self::from_host_verified_bytes(bytes).map_err(serde::de::Error::custom)
    }
}

/// Exact OS process identity retained in non-idle lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProfileProcessIdentity {
    pid: u32,
    process_birth: u64,
}

impl ProfileProcessIdentity {
    /// Creates a process identity only when both OS observations are present.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` when PID or process birth is absent.
    pub fn from_host_observation(pid: u32, process_birth: u64) -> Result<Self, ProfileError> {
        if pid == 0 || process_birth == 0 {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self { pid, process_birth })
    }
}

impl<'de> Deserialize<'de> for ProfileProcessIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let document = ProcessIdentityDocument::deserialize(deserializer)?;
        Self::from_host_observation(document.pid, document.process_birth)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessIdentityDocument {
    pid: u32,
    process_birth: u64,
}

/// Durable common lifecycle phase for one persistent profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileLifecyclePhase {
    /// No durable engine state remains and restart is admissible.
    Idle,
    /// Startup was committed before constructing the engine.
    Starting,
    /// The selected store is attached to the running engine.
    Running,
    /// Teardown began but its platform release barrier is incomplete.
    Stopping,
    /// A dead non-idle predecessor requires platform recovery.
    Quarantined,
}

/// Schema-v1 lifecycle record interpreted by the common recovery policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileLifecycleRecord {
    phase: ProfileLifecyclePhase,
    boot_identity: BootIdentity,
    owner: Option<ProfileProcessIdentity>,
}

impl ProfileLifecycleRecord {
    /// Creates an admissible idle record for the current boot.
    #[must_use]
    pub const fn idle(boot_identity: BootIdentity) -> Self {
        Self {
            phase: ProfileLifecyclePhase::Idle,
            boot_identity,
            owner: None,
        }
    }

    /// Begins startup by committing the exact process owner before engine work.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` unless this record is valid idle state.
    pub fn begin_startup(self, owner: ProfileProcessIdentity) -> Result<Self, ProfileError> {
        if self.phase != ProfileLifecyclePhase::Idle || self.owner.is_some() {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self {
            phase: ProfileLifecyclePhase::Starting,
            owner: Some(owner),
            ..self
        })
    }

    /// Advances the clean startup/teardown chain by exactly one phase.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for a skipped, reversed, or recovery-only phase.
    pub fn advance(self, next: ProfileLifecyclePhase) -> Result<Self, ProfileError> {
        let allowed = matches!(
            (self.phase, next),
            (
                ProfileLifecyclePhase::Starting,
                ProfileLifecyclePhase::Running
            ) | (
                ProfileLifecyclePhase::Running,
                ProfileLifecyclePhase::Stopping
            ) | (ProfileLifecyclePhase::Stopping, ProfileLifecyclePhase::Idle)
        );
        if !allowed {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self {
            phase: next,
            owner: if next == ProfileLifecyclePhase::Idle {
                None
            } else {
                self.owner
            },
            ..self
        })
    }

    /// Marks a dead non-idle predecessor quarantined before returning an error.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` unless a non-idle record retains an owner.
    pub fn quarantine(self) -> Result<Self, ProfileError> {
        if !matches!(
            self.phase,
            ProfileLifecyclePhase::Starting
                | ProfileLifecyclePhase::Running
                | ProfileLifecyclePhase::Stopping
        ) || self.owner.is_none()
        {
            return Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven));
        }
        Ok(Self {
            phase: ProfileLifecyclePhase::Quarantined,
            ..self
        })
    }

    /// Returns the durable phase.
    #[must_use]
    pub const fn phase(&self) -> ProfileLifecyclePhase {
        self.phase
    }

    /// Encodes the canonical strict schema-v1 lifecycle record.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` if the record cannot be encoded.
    pub fn to_record_bytes(&self) -> Result<Vec<u8>, ProfileError> {
        serde_json::to_vec(&LifecycleDocument {
            schema: 1,
            phase: self.phase,
            boot_identity: encode_lower_hex(&self.boot_identity.0),
            owner: self.owner,
        })
        .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))
    }

    /// Decodes and validates one durable lifecycle record.
    ///
    /// # Errors
    ///
    /// Returns `KELD-WV-009` for malformed, noncanonical, wrong-schema, or
    /// phase/owner-inconsistent records.
    pub fn from_record_bytes(bytes: &[u8]) -> Result<Self, ProfileError> {
        let document: LifecycleDocument = serde_json::from_slice(bytes)
            .map_err(|_| ProfileError::new(ProfileErrorKind::InvalidRecord))?;
        if document.schema != 1 {
            return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
        }
        let record = Self {
            phase: document.phase,
            boot_identity: BootIdentity::from_host_verified_bytes(decode_lower_hex::<16>(
                &document.boot_identity,
            )?)?,
            owner: document.owner,
        };
        let valid_owner = match record.phase {
            ProfileLifecyclePhase::Idle => record.owner.is_none(),
            ProfileLifecyclePhase::Starting
            | ProfileLifecyclePhase::Running
            | ProfileLifecyclePhase::Stopping
            | ProfileLifecyclePhase::Quarantined => record.owner.is_some(),
        };
        if !valid_owner {
            return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
        }
        Ok(record)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LifecycleDocument {
    schema: u8,
    phase: ProfileLifecyclePhase,
    boot_identity: String,
    owner: Option<ProfileProcessIdentity>,
}

/// Host observation of the exact process recorded in durable state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordedProcessObservation {
    /// The exact PID and birth identity is still live.
    Live,
    /// The exact recorded process identity is no longer live.
    Dead,
    /// Liveness could not be proved either way.
    Unknown,
}

/// One common lifecycle recovery action; platform effects remain future proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileLifecycleAction {
    /// Commit `starting` for a newly admitted owner.
    BeginStartup,
    /// Commit `quarantined` and fail this startup attempt.
    WriteQuarantined,
    /// Revalidate metadata, commit `idle`, and retry startup afterward.
    RestoreIdleAfterBoot,
}

/// Selects the next safe lifecycle action from independently supplied facts.
///
/// # Errors
///
/// Returns `KELD-WV-009` for a live owner, unknown liveness, malformed idle
/// state, or same-boot quarantine.
pub fn next_lifecycle_action(
    record: ProfileLifecycleRecord,
    process: RecordedProcessObservation,
    current_boot: BootIdentity,
) -> Result<ProfileLifecycleAction, ProfileError> {
    match record.phase {
        ProfileLifecyclePhase::Idle if record.owner.is_none() => {
            Ok(ProfileLifecycleAction::BeginStartup)
        }
        ProfileLifecyclePhase::Quarantined if record.boot_identity != current_boot => {
            Ok(ProfileLifecycleAction::RestoreIdleAfterBoot)
        }
        ProfileLifecyclePhase::Quarantined => {
            Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven))
        }
        ProfileLifecyclePhase::Starting
        | ProfileLifecyclePhase::Running
        | ProfileLifecyclePhase::Stopping => match process {
            RecordedProcessObservation::Live => {
                Err(ProfileError::new(ProfileErrorKind::ProfileInUse))
            }
            RecordedProcessObservation::Dead => Ok(ProfileLifecycleAction::WriteQuarantined),
            RecordedProcessObservation::Unknown => {
                Err(ProfileError::new(ProfileErrorKind::LifecycleUnproven))
            }
        },
        ProfileLifecyclePhase::Idle => Err(ProfileError::new(ProfileErrorKind::InvalidRecord)),
    }
}

fn validate_canonical_app_id(value: &str) -> Result<(), ProfileError> {
    if value.is_empty() || value.len() > 255 || !value.is_ascii() {
        return Err(ProfileError::new(ProfileErrorKind::InvalidAppId));
    }
    for segment in value.as_bytes().split(|byte| *byte == b'.') {
        let Some(first) = segment.first() else {
            return Err(ProfileError::new(ProfileErrorKind::InvalidAppId));
        };
        let Some(last) = segment.last() else {
            return Err(ProfileError::new(ProfileErrorKind::InvalidAppId));
        };
        if !is_lower_alphanumeric(*first)
            || !is_lower_alphanumeric(*last)
            || !segment
                .iter()
                .all(|byte| is_lower_alphanumeric(*byte) || *byte == b'-')
        {
            return Err(ProfileError::new(ProfileErrorKind::InvalidAppId));
        }
    }
    Ok(())
}

const fn is_lower_alphanumeric(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn decode_lower_hex<const N: usize>(value: &str) -> Result<[u8; N], ProfileError> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ProfileError::new(ProfileErrorKind::InvalidRecord));
    }
    let mut output = [0_u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (decode_hex_nibble(pair[0])? << 4) | decode_hex_nibble(pair[1])?;
    }
    Ok(output)
}

fn decode_hex_nibble(byte: u8) -> Result<u8, ProfileError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(ProfileError::new(ProfileErrorKind::InvalidRecord)),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppleStoreUuid, BindingPhase, BootIdentity, EphemeralProfile, LeaseOwner, MarkerAction,
        MarkerObservation, ProfileError, ProfileErrorKind, ProfileIdentity, ProfileLeaseKey,
        ProfileLeaseTable, ProfileLifecycleAction, ProfileLifecyclePhase, ProfileLifecycleRecord,
        ProfileLockLevel, ProfileLockOrder, ProfileMarker, ProfilePlatform, ProfileProcessIdentity,
        ProfileRootRole, PurgePhase, RecordedProcessObservation, RegistryAction, RegistryIntent,
        RegistryRecord, RegistryRequest, RegistrySnapshot, StoreBinding, StoreObservation,
        WebProfileSelection, next_lifecycle_action, next_marker_action, next_registry_action,
    };

    #[test]
    fn identity_vectors() {
        let mut publisher = [0_u8; 32];
        for (index, byte) in publisher.iter_mut().enumerate() {
            *byte = u8::try_from(index).unwrap_or_default();
        }
        let identity = ProfileIdentity::from_host_verified_parts(publisher, "com.example.app")
            .expect("canonical identity vector");
        assert_eq!(
            identity.namespace_segment(),
            "17f0e6ecb5be8eeda9f15ad672cde10c140b2cc4ff7a49e18da02635bfb81229"
        );
        assert_eq!(
            identity.apple_store_uuid().to_string(),
            "c6c0d963-7457-8cf1-b099-cfe5d370837d"
        );
        assert_eq!(identity.namespace_segment().len(), 64);
        assert_eq!(identity.apple_store_uuid().as_bytes()[6] >> 4, 8);
        assert_eq!(identity.apple_store_uuid().as_bytes()[8] >> 6, 2);
    }

    #[test]
    fn canonical_app_id_boundaries_reject_aliases() {
        for invalid in [
            "",
            ".com.example",
            "com.example.",
            "com..example",
            "Com.example",
            "com.Example",
            "com.-example",
            "com.example-",
            "com.ex_ample",
            "com.exämple",
        ] {
            let error = ProfileIdentity::from_host_verified_parts([7; 32], invalid)
                .expect_err("noncanonical id must fail");
            assert_eq!(error.kind(), ProfileErrorKind::InvalidAppId, "{invalid}");
        }
        let max = format!("a{}", "b".repeat(254));
        ProfileIdentity::from_host_verified_parts([7; 32], &max).expect("255-byte canonical id");
        let too_long = format!("a{}", "b".repeat(255));
        let error = ProfileIdentity::from_host_verified_parts([7; 32], &too_long)
            .expect_err("256-byte id must fail");
        assert_eq!(error.kind(), ProfileErrorKind::InvalidAppId);
    }

    #[test]
    fn identity_changes_for_each_verified_tuple_part() {
        let base = ProfileIdentity::from_host_verified_parts([3; 32], "dev.keld.app")
            .expect("base identity");
        let publisher_changed = ProfileIdentity::from_host_verified_parts([4; 32], "dev.keld.app")
            .expect("publisher variant");
        let app_changed = ProfileIdentity::from_host_verified_parts([3; 32], "dev.keld.other")
            .expect("app variant");
        assert_ne!(base, publisher_changed);
        assert_ne!(base, app_changed);
    }

    #[test]
    fn namespace_and_uuid_parsers_reject_noncanonical_text() {
        let identity =
            ProfileIdentity::from_host_verified_parts([5; 32], "dev.keld.app").expect("identity");
        let uppercase = identity.namespace_segment().to_ascii_uppercase();
        assert!(ProfileIdentity::from_namespace_segment(&uppercase).is_err());
        assert!(ProfileIdentity::from_namespace_segment("../outside").is_err());

        let uuid = identity.apple_store_uuid().to_string();
        assert_eq!(
            AppleStoreUuid::parse(&uuid),
            Ok(identity.apple_store_uuid())
        );
        assert!(AppleStoreUuid::parse(&uuid.to_ascii_uppercase()).is_err());
    }

    #[test]
    fn dev_mode_state_has_no_persistent_fallback() {
        let missing =
            WebProfileSelection::persistent(None).expect_err("release identity must be mandatory");
        assert_eq!(
            missing.kind(),
            ProfileErrorKind::MissingAuthenticatedIdentity
        );
        let zero = EphemeralProfile::from_host_random([0; 32])
            .expect_err("missing host randomness must fail");
        assert_eq!(zero.kind(), ProfileErrorKind::InvalidEphemeralNonce);

        let first = EphemeralProfile::from_host_random([1; 32]).expect("first launch");
        let second = EphemeralProfile::from_host_random([2; 32]).expect("second launch");
        assert_ne!(first, second);
        assert_eq!(
            WebProfileSelection::ephemeral_dev(first),
            WebProfileSelection::EphemeralDev(first)
        );
    }

    #[test]
    fn marker_records_and_leaf_decisions_are_strict() {
        let expected = ProfileMarker::new(
            test_identity(9),
            ProfilePlatform::Linux,
            ProfileRootRole::Data,
        );
        let bytes = expected.to_record_bytes().expect("encode marker");
        assert_eq!(
            ProfileMarker::from_record_bytes(&bytes),
            Ok(expected),
            "stored bytes decode but do not select identity without comparison"
        );
        assert_eq!(
            next_marker_action(expected, MarkerObservation::NewlyCreatedEmpty),
            Ok(MarkerAction::WriteNew(expected))
        );
        assert_eq!(
            next_marker_action(expected, MarkerObservation::ExistingMarked(expected)),
            Ok(MarkerAction::Reuse)
        );

        for observation in [
            MarkerObservation::ExistingEmptyUnmarked,
            MarkerObservation::ExistingNonemptyUnmarked,
            MarkerObservation::LinkOrReparse,
            MarkerObservation::EscapingFinalPath,
            MarkerObservation::UnsafeOwnerOrPermissions,
            MarkerObservation::ExistingMarked(ProfileMarker::new(
                expected.identity(),
                expected.platform(),
                ProfileRootRole::Cache,
            )),
        ] {
            let error = next_marker_action(expected, observation).expect_err("unsafe leaf");
            assert_eq!(error.kind(), ProfileErrorKind::MarkerMismatch);
        }

        let text = String::from_utf8(bytes).expect("marker JSON is UTF-8");
        let unknown = text.replace("\"schema\":1", "\"schema\":1,\"extra\":true");
        assert!(ProfileMarker::from_record_bytes(unknown.as_bytes()).is_err());
        let duplicate = text.replace("\"schema\":1", "\"schema\":1,\"schema\":1");
        assert!(ProfileMarker::from_record_bytes(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn registry_binding_writes_reverse_then_forward() {
        let binding = test_binding();
        assert_eq!(
            next_registry_action(RegistryRequest::Bind(binding), registry_empty()),
            Ok(RegistryAction::WriteBindingIntent(RegistryIntent::binding(
                binding
            )))
        );
        let prepared = RegistryIntent::binding(binding);
        assert_eq!(
            bind_action(
                prepared,
                RegistryRecord::Missing,
                RegistryRecord::Missing,
                StoreObservation::Unknown,
                false
            ),
            Ok(RegistryAction::WriteReverseRecord(binding))
        );
        assert_eq!(
            bind_action(
                prepared,
                RegistryRecord::Missing,
                RegistryRecord::Present(binding),
                StoreObservation::Unknown,
                false
            ),
            Ok(RegistryAction::AdvanceBindingIntent(
                BindingPhase::ReverseWritten
            ))
        );
        assert_registry_corruption(bind_action(
            prepared,
            RegistryRecord::Present(binding),
            RegistryRecord::Missing,
            StoreObservation::Unknown,
            false,
        ));
        let reverse = prepared.with_binding_phase(BindingPhase::ReverseWritten);
        assert_eq!(
            bind_action(
                reverse,
                RegistryRecord::Missing,
                RegistryRecord::Present(binding),
                StoreObservation::Unknown,
                false
            ),
            Ok(RegistryAction::WriteForwardRecord(binding))
        );
        assert_eq!(
            bind_action(
                reverse,
                RegistryRecord::Present(binding),
                RegistryRecord::Present(binding),
                StoreObservation::Unknown,
                false
            ),
            Ok(RegistryAction::AdvanceBindingIntent(
                BindingPhase::ForwardWritten
            ))
        );
    }

    #[test]
    fn registry_binding_authorizes_creation_before_accepting_store() {
        let binding = test_binding();
        let exact = RegistryRecord::Present(binding);
        let prepared = RegistryIntent::binding(binding);
        let forward = prepared.with_binding_phase(BindingPhase::ForwardWritten);
        assert_eq!(
            bind_action(forward, exact, exact, StoreObservation::Unknown, false),
            Ok(RegistryAction::EnumerateStores)
        );
        assert_eq!(
            bind_action(forward, exact, exact, StoreObservation::Absent, false),
            Ok(RegistryAction::AdvanceBindingIntent(
                BindingPhase::StoreCreationAuthorized
            ))
        );
        assert_registry_corruption(bind_action(
            forward,
            exact,
            exact,
            present_store(binding),
            false,
        ));
        let authorized = prepared.with_binding_phase(BindingPhase::StoreCreationAuthorized);
        assert_eq!(
            bind_action(authorized, exact, exact, StoreObservation::Absent, false),
            Ok(RegistryAction::ConstructStore(binding))
        );
        assert_eq!(
            bind_action(authorized, exact, exact, present_store(binding), false),
            Ok(RegistryAction::AdvanceBindingIntent(
                BindingPhase::StoreVerified
            ))
        );
    }

    #[test]
    fn registry_active_binding_requires_two_records_and_present_store() {
        let binding = test_binding();
        let exact = RegistryRecord::Present(binding);
        let verified =
            RegistryIntent::binding(binding).with_binding_phase(BindingPhase::StoreVerified);
        assert_eq!(
            bind_action(verified, exact, exact, present_store(binding), false),
            Ok(RegistryAction::MarkBindingActive)
        );
        assert_eq!(
            bind_action(verified, exact, exact, present_store(binding), true),
            Ok(RegistryAction::ClearIntent)
        );
        let active = RegistrySnapshot::new(None, exact, exact, present_store(binding), true);
        assert_eq!(
            next_registry_action(RegistryRequest::Bind(binding), active),
            Ok(RegistryAction::ReuseStore)
        );
        assert_registry_corruption(next_registry_action(
            RegistryRequest::Bind(binding),
            RegistrySnapshot::new(
                None,
                exact,
                RegistryRecord::Missing,
                present_store(binding),
                true,
            ),
        ));
        assert_registry_corruption(next_registry_action(
            RegistryRequest::Bind(binding),
            RegistrySnapshot::new(None, exact, exact, StoreObservation::Absent, true),
        ));
        let intent_bytes = verified.to_record_bytes().expect("encode intent");
        assert_eq!(
            RegistryIntent::from_record_bytes(&intent_bytes),
            Ok(verified)
        );
        let binding_bytes = binding.to_record_bytes().expect("encode binding");
        assert_eq!(StoreBinding::from_record_bytes(&binding_bytes), Ok(binding));
    }

    #[test]
    fn registry_purge_blocks_lookup_and_removes_store_first() {
        let binding = test_binding();
        let exact = RegistryRecord::Present(binding);
        let active = RegistrySnapshot::new(None, exact, exact, present_store(binding), true);
        assert_eq!(
            next_registry_action(RegistryRequest::Purge(binding), active),
            Ok(RegistryAction::WritePurgeIntent(RegistryIntent::purging(
                binding
            )))
        );
        let prepared = RegistryIntent::purging(binding);
        let during =
            RegistrySnapshot::new(Some(prepared), exact, exact, present_store(binding), true);
        let blocked = next_registry_action(RegistryRequest::Bind(binding), during)
            .expect_err("purge intent blocks lookup");
        assert_eq!(blocked.kind(), ProfileErrorKind::ActiveIntent);
        assert_eq!(
            next_registry_action(RegistryRequest::Purge(binding), during),
            Ok(RegistryAction::AdvancePurgeIntent(
                PurgePhase::StoreRemovalAuthorized
            ))
        );
        let removal = prepared.with_purge_phase(PurgePhase::StoreRemovalAuthorized);
        assert_eq!(
            purge_action(removal, exact, exact, present_store(binding), true),
            Ok(RegistryAction::RemoveStore(binding))
        );
        assert_eq!(
            purge_action(removal, exact, exact, StoreObservation::Absent, true),
            Ok(RegistryAction::AdvancePurgeIntent(PurgePhase::StoreAbsent))
        );
    }

    #[test]
    fn registry_purge_recovers_reverse_then_forward_removal() {
        let binding = test_binding();
        let exact = RegistryRecord::Present(binding);
        let prepared = RegistryIntent::purging(binding);
        let absent = prepared.with_purge_phase(PurgePhase::StoreAbsent);
        assert_eq!(
            purge_action(absent, exact, exact, StoreObservation::Absent, true),
            Ok(RegistryAction::RemoveReverseRecord)
        );
        assert_eq!(
            purge_action(
                absent,
                exact,
                RegistryRecord::Missing,
                StoreObservation::Absent,
                true
            ),
            Ok(RegistryAction::AdvancePurgeIntent(
                PurgePhase::ReverseRemoved
            ))
        );
        let reverse_removed = prepared.with_purge_phase(PurgePhase::ReverseRemoved);
        assert_eq!(
            purge_action(
                reverse_removed,
                exact,
                RegistryRecord::Missing,
                StoreObservation::Absent,
                true
            ),
            Ok(RegistryAction::RemoveForwardRecord)
        );
        assert_eq!(
            purge_action(
                reverse_removed,
                RegistryRecord::Missing,
                RegistryRecord::Missing,
                StoreObservation::Absent,
                true
            ),
            Ok(RegistryAction::AdvancePurgeIntent(
                PurgePhase::ForwardRemoved
            ))
        );
        let forward_removed = prepared.with_purge_phase(PurgePhase::ForwardRemoved);
        assert_eq!(
            purge_action(
                forward_removed,
                RegistryRecord::Missing,
                RegistryRecord::Missing,
                StoreObservation::Absent,
                true
            ),
            Ok(RegistryAction::ClearActiveStatus)
        );
        assert_eq!(
            purge_action(
                forward_removed,
                RegistryRecord::Missing,
                RegistryRecord::Missing,
                StoreObservation::Absent,
                false
            ),
            Ok(RegistryAction::AdvancePurgeIntent(PurgePhase::Inactive))
        );
        let inactive = prepared.with_purge_phase(PurgePhase::Inactive);
        assert_eq!(
            purge_action(
                inactive,
                RegistryRecord::Missing,
                RegistryRecord::Missing,
                StoreObservation::Absent,
                false
            ),
            Ok(RegistryAction::ClearIntent)
        );
    }

    #[test]
    fn lock_order_and_exclusive_lease_reject_aliases() {
        let identity = test_identity(3);
        let key = ProfileLeaseKey::from_host_validated_parts(
            identity,
            ProfilePlatform::Windows,
            [1; 32],
            [2; 32],
        );
        let other_key = ProfileLeaseKey::from_host_validated_parts(
            test_identity(4),
            ProfilePlatform::Windows,
            [1; 32],
            [2; 32],
        );
        let first_owner = LeaseOwner::from_host_process_identity([1; 16]).expect("owner");
        let second_owner = LeaseOwner::from_host_process_identity([2; 16]).expect("owner");
        let mut leases = ProfileLeaseTable::default();
        let first = leases.acquire(key, first_owner).expect("first owner wins");
        let conflict = leases
            .acquire(key, second_owner)
            .expect_err("same validated key must stay exclusive");
        assert_eq!(conflict.kind(), ProfileErrorKind::ProfileInUse);
        let independent = leases
            .acquire(other_key, second_owner)
            .expect("different identity has an independent lease");
        leases
            .release(&independent)
            .expect("release independent key");
        leases.release(&first).expect("release exact first token");
        let successor = leases
            .acquire(key, first_owner)
            .expect("successor may acquire after exact release");
        let stale = leases
            .release(&first)
            .expect_err("old generation token cannot release its successor");
        assert_eq!(stale.kind(), ProfileErrorKind::ProfileInUse);
        leases.release(&successor).expect("release successor");

        let mut order = ProfileLockOrder::default();
        for level in [
            ProfileLockLevel::PackageLifecycle,
            ProfileLockLevel::PlatformRegistry,
            ProfileLockLevel::ProfileLease,
            ProfileLockLevel::EngineStoreIntent,
        ] {
            order.acquire(level).expect("canonical acquisition order");
        }
        let wrong_release = order
            .release(ProfileLockLevel::ProfileLease)
            .expect_err("release must be LIFO");
        assert_eq!(wrong_release.kind(), ProfileErrorKind::LockOrderViolation);
        for level in [
            ProfileLockLevel::EngineStoreIntent,
            ProfileLockLevel::ProfileLease,
            ProfileLockLevel::PlatformRegistry,
            ProfileLockLevel::PackageLifecycle,
        ] {
            order.release(level).expect("canonical reverse release");
        }
        let missing_package = order
            .acquire(ProfileLockLevel::ProfileLease)
            .expect_err("package lifecycle lock is always outermost");
        assert_eq!(missing_package.kind(), ProfileErrorKind::LockOrderViolation);
        order
            .acquire(ProfileLockLevel::PackageLifecycle)
            .expect("ordinary startup package lock");
        order
            .acquire(ProfileLockLevel::ProfileLease)
            .expect("registry lock is optional when no reverse index is touched");
        let inversion = order
            .acquire(ProfileLockLevel::PlatformRegistry)
            .expect_err("upward acquisition must fail");
        assert_eq!(inversion.kind(), ProfileErrorKind::LockOrderViolation);
        let reentry = order
            .acquire(ProfileLockLevel::ProfileLease)
            .expect_err("same-level reentry must fail");
        assert_eq!(reentry.kind(), ProfileErrorKind::LockOrderViolation);
    }

    #[test]
    fn lifecycle_state_requires_release_or_boot_recovery() {
        let boot = BootIdentity::from_host_verified_bytes([1; 16]).expect("boot identity");
        let owner = ProfileProcessIdentity::from_host_observation(42, 700).expect("process");
        let idle = ProfileLifecycleRecord::idle(boot);
        assert_eq!(
            next_lifecycle_action(idle, RecordedProcessObservation::Unknown, boot,),
            Ok(ProfileLifecycleAction::BeginStartup)
        );
        let starting = idle.begin_startup(owner).expect("begin startup");
        let live = next_lifecycle_action(starting, RecordedProcessObservation::Live, boot)
            .expect_err("live predecessor owns the profile");
        assert_eq!(live.kind(), ProfileErrorKind::ProfileInUse);
        assert_eq!(
            next_lifecycle_action(starting, RecordedProcessObservation::Dead, boot,),
            Ok(ProfileLifecycleAction::WriteQuarantined)
        );
        let unknown = next_lifecycle_action(starting, RecordedProcessObservation::Unknown, boot)
            .expect_err("unknown liveness fails closed");
        assert_eq!(unknown.kind(), ProfileErrorKind::LifecycleUnproven);

        let quarantined = starting.quarantine().expect("commit quarantine");
        assert_eq!(quarantined.phase(), ProfileLifecyclePhase::Quarantined);
        let same_boot = next_lifecycle_action(quarantined, RecordedProcessObservation::Dead, boot)
            .expect_err("same boot cannot clear quarantine");
        assert_eq!(same_boot.kind(), ProfileErrorKind::LifecycleUnproven);
        let changed_boot = BootIdentity::from_host_verified_bytes([2; 16]).expect("new boot");
        assert_eq!(
            next_lifecycle_action(quarantined, RecordedProcessObservation::Dead, changed_boot,),
            Ok(ProfileLifecycleAction::RestoreIdleAfterBoot)
        );

        let clean_idle = starting
            .advance(ProfileLifecyclePhase::Running)
            .and_then(|record| record.advance(ProfileLifecyclePhase::Stopping))
            .and_then(|record| record.advance(ProfileLifecyclePhase::Idle))
            .expect("clean teardown reaches idle");
        assert_eq!(clean_idle.phase(), ProfileLifecyclePhase::Idle);
        let bytes = clean_idle.to_record_bytes().expect("encode lifecycle");
        assert_eq!(
            ProfileLifecycleRecord::from_record_bytes(&bytes),
            Ok(clean_idle)
        );
        let text = String::from_utf8(bytes).expect("lifecycle JSON is UTF-8");
        let unknown_field = text.replace("\"schema\":1", "\"schema\":1,\"extra\":0");
        assert!(ProfileLifecycleRecord::from_record_bytes(unknown_field.as_bytes()).is_err());
        let starting_text = String::from_utf8(
            starting
                .to_record_bytes()
                .expect("encode starting lifecycle"),
        )
        .expect("lifecycle JSON is UTF-8");
        let zero_pid = starting_text.replace("\"pid\":42", "\"pid\":0");
        assert!(ProfileLifecycleRecord::from_record_bytes(zero_pid.as_bytes()).is_err());
        let nested_unknown = starting_text.replace("\"pid\":42", "\"pid\":42,\"extra\":0");
        assert!(ProfileLifecycleRecord::from_record_bytes(nested_unknown.as_bytes()).is_err());
    }

    #[test]
    fn deserialization_preserves_identity_validation() {
        let zero_boot_json = serde_json::to_string(&[0_u8; 16]).expect("encode boot fixture");
        let zero_boot = serde_json::from_str::<BootIdentity>(&zero_boot_json);
        if let Ok(forged_boot) = zero_boot.as_ref().copied() {
            let admitted = next_lifecycle_action(
                ProfileLifecycleRecord::idle(forged_boot),
                RecordedProcessObservation::Unknown,
                forged_boot,
            );
            assert!(
                admitted.is_err(),
                "a directly decoded zero boot identity must not admit startup"
            );
        }
        assert!(zero_boot.is_err(), "zero boot identity must fail decoding");

        let zero_pid =
            serde_json::from_str::<ProfileProcessIdentity>(r#"{"pid":0,"process_birth":700}"#);
        if let Ok(forged_owner) = zero_pid.as_ref().copied() {
            let boot = BootIdentity::from_host_verified_bytes([1; 16]).expect("valid boot");
            assert!(
                ProfileLifecycleRecord::idle(boot)
                    .begin_startup(forged_owner)
                    .is_err(),
                "a directly decoded zero PID must not begin startup"
            );
        }
        assert!(zero_pid.is_err(), "zero PID must fail decoding");
        assert!(
            serde_json::from_str::<ProfileProcessIdentity>(r#"{"pid":42,"process_birth":0}"#)
                .is_err(),
            "zero process birth must fail decoding"
        );

        let valid_boot_json = serde_json::to_string(&[2_u8; 16]).expect("encode valid boot");
        let valid_boot =
            serde_json::from_str::<BootIdentity>(&valid_boot_json).expect("valid boot decodes");
        let valid_owner =
            serde_json::from_str::<ProfileProcessIdentity>(r#"{"pid":42,"process_birth":700}"#)
                .expect("valid process identity decodes");
        ProfileLifecycleRecord::idle(valid_boot)
            .begin_startup(valid_owner)
            .expect("validated decoded identities begin startup");
        assert!(
            serde_json::from_str::<ProfileProcessIdentity>(
                r#"{"pid":42,"process_birth":700,"extra":1}"#
            )
            .is_err(),
            "unknown process identity fields remain denied"
        );
    }

    #[test]
    fn lease_rejects_foreign_table_token() {
        let key = ProfileLeaseKey::from_host_validated_parts(
            test_identity(6),
            ProfilePlatform::Linux,
            [3; 32],
            [4; 32],
        );
        let original_owner = LeaseOwner::from_host_process_identity([3; 16]).expect("owner");
        let other_owner = LeaseOwner::from_host_process_identity([4; 16]).expect("owner");
        let mut first_table = ProfileLeaseTable::default();
        let mut second_table = ProfileLeaseTable::default();
        let first_token = first_table
            .acquire(key, original_owner)
            .expect("first table lease");
        let second_token = second_table
            .acquire(key, original_owner)
            .expect("second table lease");
        assert_ne!(
            first_token, second_token,
            "tokens from distinct tables carry distinct provenance"
        );

        let foreign = second_table
            .release(&first_token)
            .expect_err("foreign table token must be rejected");
        assert_eq!(foreign.kind(), ProfileErrorKind::ProfileInUse);
        let conflict = second_table
            .acquire(key, other_owner)
            .expect_err("foreign release must preserve the original owner");
        assert_eq!(conflict.kind(), ProfileErrorKind::ProfileInUse);
        second_table
            .release(&second_token)
            .expect("the original second-table token still releases");
        second_table
            .acquire(key, other_owner)
            .expect("a new owner acquires only after exact release");
        first_table
            .release(&first_token)
            .expect("first table remains independently releasable");
    }

    fn test_identity(seed: u8) -> ProfileIdentity {
        ProfileIdentity::from_host_verified_parts([seed; 32], "dev.keld.profile")
            .expect("test identity")
    }

    fn test_binding() -> StoreBinding {
        StoreBinding::for_identity(test_identity(8))
    }

    fn present_store(binding: StoreBinding) -> StoreObservation {
        StoreObservation::Present {
            uuid: binding.store_uuid(),
            persistent: true,
        }
    }

    const fn registry_empty() -> RegistrySnapshot {
        RegistrySnapshot::new(
            None,
            RegistryRecord::Missing,
            RegistryRecord::Missing,
            StoreObservation::Unknown,
            false,
        )
    }

    fn bind_action(
        intent: RegistryIntent,
        forward: RegistryRecord,
        reverse: RegistryRecord,
        store: StoreObservation,
        active: bool,
    ) -> Result<RegistryAction, ProfileError> {
        next_registry_action(
            RegistryRequest::Bind(intent.store_binding()),
            RegistrySnapshot::new(Some(intent), forward, reverse, store, active),
        )
    }

    fn purge_action(
        intent: RegistryIntent,
        forward: RegistryRecord,
        reverse: RegistryRecord,
        store: StoreObservation,
        active: bool,
    ) -> Result<RegistryAction, ProfileError> {
        next_registry_action(
            RegistryRequest::Purge(intent.store_binding()),
            RegistrySnapshot::new(Some(intent), forward, reverse, store, active),
        )
    }

    fn assert_registry_corruption(result: Result<RegistryAction, ProfileError>) {
        let error = result.expect_err("registry corruption must fail closed");
        assert_eq!(error.kind(), ProfileErrorKind::RegistryCorruption);
    }
}
