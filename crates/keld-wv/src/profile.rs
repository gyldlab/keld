//! Host-owned persistent webview profile identity and common state policy.
//!
//! The public constructor in this module is a host-TCB boundary. It validates
//! canonical bytes, but its publisher input is trustworthy only after a
//! platform package verifier has authenticated those bytes. KEL-135/T1 adds no
//! such verifier and selects no native store.

use core::fmt;

use serde::{Deserialize, Serialize};

mod identity;
pub use identity::{AppleStoreUuid, EphemeralProfile, ProfileIdentity, WebProfileSelection};
use identity::{decode_lower_hex, encode_lower_hex};

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
    const fn new(kind: ProfileErrorKind) -> Self {
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

mod marker;
pub use marker::{MarkerAction, MarkerObservation, ProfileMarker, next_marker_action};

mod registry_records;
pub use registry_records::{
    BindingPhase, PurgePhase, RegistryIntent, RegistryIntentPhase, StoreBinding,
};

mod registry_planner;
pub use registry_planner::{
    RegistryAction, RegistryRecord, RegistryRequest, RegistrySnapshot, StoreObservation,
    next_registry_action,
};

mod lease;
pub use lease::{LeaseOwner, ProfileLease, ProfileLeaseKey, ProfileLeaseTable};

mod lock_order;
pub use lock_order::{ProfileLockLevel, ProfileLockOrder};

mod lifecycle;
pub use lifecycle::{
    BootIdentity, ProfileLifecycleAction, ProfileLifecyclePhase, ProfileLifecycleRecord,
    ProfileProcessIdentity, RecordedProcessObservation, next_lifecycle_action,
    next_windows_lifecycle_action,
};

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

#[cfg(test)]
mod tests {
    use super::{
        AppleStoreUuid, BindingPhase, BootIdentity, EphemeralProfile, LeaseOwner, MarkerAction,
        MarkerObservation, ProfileError, ProfileErrorKind, ProfileIdentity, ProfileLeaseKey,
        ProfileLeaseTable, ProfileLifecycleAction, ProfileLifecyclePhase, ProfileLifecycleRecord,
        ProfileLockLevel, ProfileLockOrder, ProfileMarker, ProfilePlatform, ProfileProcessIdentity,
        ProfilePurgePhase, ProfilePurgeRecord, ProfileRootRole, PurgePhase,
        RecordedProcessObservation, RegistryAction, RegistryIntent, RegistryRecord,
        RegistryRequest, RegistrySnapshot, StoreBinding, StoreObservation, WebProfileSelection,
        next_lifecycle_action, next_marker_action, next_registry_action,
        next_windows_lifecycle_action,
    };

    #[test]
    fn lifecycle_unproven_error_keeps_cross_platform_and_version_guidance() {
        let message = ProfileError::new(ProfileErrorKind::LifecycleUnproven).to_string();
        assert!(message.contains("lifecycle state"), "{message}");
        assert!(
            message.contains("recovery state cannot be proven"),
            "{message}"
        );
        assert!(message.contains("unsupported here"), "{message}");
    }

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
        assert_eq!(
            bind_action(verified, exact, exact, StoreObservation::Unknown, false),
            Ok(RegistryAction::EnumerateStores)
        );
        assert_eq!(
            bind_action(verified, exact, exact, StoreObservation::Unknown, true),
            Ok(RegistryAction::EnumerateStores)
        );
        assert_registry_corruption(bind_action(
            verified,
            exact,
            exact,
            StoreObservation::Absent,
            false,
        ));
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
    fn registry_purge_unknown_store_enumerates_before_later_phase_writes() {
        let binding = test_binding();
        let exact = RegistryRecord::Present(binding);
        let missing = RegistryRecord::Missing;
        let prepared = RegistryIntent::purging(binding);
        for (phase, forward, reverse, active) in [
            (PurgePhase::StoreAbsent, exact, exact, true),
            (PurgePhase::StoreAbsent, exact, missing, true),
            (PurgePhase::ReverseRemoved, exact, missing, true),
            (PurgePhase::ReverseRemoved, missing, missing, true),
            (PurgePhase::ForwardRemoved, missing, missing, true),
            (PurgePhase::ForwardRemoved, missing, missing, false),
            (PurgePhase::Inactive, missing, missing, false),
        ] {
            assert_eq!(
                purge_action(
                    prepared.with_purge_phase(phase),
                    forward,
                    reverse,
                    StoreObservation::Unknown,
                    active,
                ),
                Ok(RegistryAction::EnumerateStores),
                "phase {phase:?} must read the live store before metadata mutation"
            );
        }
        for (phase, forward, reverse, active) in [
            (PurgePhase::StoreAbsent, missing, exact, true),
            (PurgePhase::ReverseRemoved, exact, exact, true),
            (PurgePhase::ForwardRemoved, exact, missing, true),
            (PurgePhase::Inactive, missing, missing, true),
        ] {
            assert_eq!(
                purge_action(
                    prepared.with_purge_phase(phase),
                    forward,
                    reverse,
                    StoreObservation::Unknown,
                    active,
                )
                .map_err(|error| error.kind()),
                Err(ProfileErrorKind::RegistryCorruption),
                "phase {phase:?} must reject malformed metadata before enumeration"
            );
        }
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
        assert_eq!(
            quarantined
                .restore_idle_after_boot(changed_boot)
                .expect("changed-boot quarantine completes to idle"),
            ProfileLifecycleRecord::idle(changed_boot)
        );
        assert_eq!(
            quarantined
                .restore_idle_after_boot(boot)
                .expect_err("same-boot quarantine remains blocked")
                .kind(),
            ProfileErrorKind::LifecycleUnproven
        );
        assert_eq!(
            starting
                .restore_idle_after_boot(changed_boot)
                .expect_err("non-quarantined records do not skip quarantine")
                .kind(),
            ProfileErrorKind::LifecycleUnproven
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
    fn windows_lifecycle_requires_exclusive_udf_recovery() {
        let owner = ProfileProcessIdentity::from_host_observation(84, 1_200).expect("process");
        let idle = ProfileLifecycleRecord::windows_idle();
        assert_eq!(
            next_windows_lifecycle_action(idle, RecordedProcessObservation::Unknown),
            Ok(ProfileLifecycleAction::BeginStartup)
        );
        let running = idle
            .begin_startup(owner)
            .and_then(|record| record.advance(ProfileLifecyclePhase::Running))
            .expect("running");
        assert_eq!(
            next_windows_lifecycle_action(running, RecordedProcessObservation::Dead),
            Ok(ProfileLifecycleAction::WriteQuarantined)
        );
        let quarantined = running.quarantine().expect("quarantine");
        assert_eq!(
            next_windows_lifecycle_action(quarantined, RecordedProcessObservation::Unknown),
            Ok(ProfileLifecycleAction::RunWindowsExclusiveUdfRecovery)
        );
        assert!(
            quarantined.advance(ProfileLifecyclePhase::Idle).is_err(),
            "ordinary phase advancement must not bypass the WebView2 release oracle"
        );
        let recovered = quarantined
            .complete_windows_recovery()
            .expect("backend release proof restores idle");
        assert_eq!(recovered, ProfileLifecycleRecord::windows_idle());

        let bytes = quarantined.to_record_bytes().expect("encode Windows state");
        let text = String::from_utf8(bytes.clone()).expect("UTF-8 Windows state");
        assert!(text.contains(r#""schema":2"#));
        assert!(text.contains(r#""recovery_mode":"windows_exclusive_udf""#));
        assert!(!text.contains("boot_identity"));
        assert_eq!(
            ProfileLifecycleRecord::from_windows_record_bytes(&bytes),
            Ok(quarantined)
        );
        assert!(ProfileLifecycleRecord::from_boot_scoped_record_bytes(&bytes).is_err());

        let boot = BootIdentity::from_host_verified_bytes([7; 16]).expect("boot");
        let boot_bytes = ProfileLifecycleRecord::idle(boot)
            .to_record_bytes()
            .expect("encode boot state");
        assert!(ProfileLifecycleRecord::from_windows_record_bytes(&boot_bytes).is_err());
        assert!(
            next_windows_lifecycle_action(
                ProfileLifecycleRecord::idle(boot),
                RecordedProcessObservation::Dead,
            )
            .is_err()
        );
        assert!(
            next_lifecycle_action(quarantined, RecordedProcessObservation::Dead, boot).is_err()
        );
        let unknown = text.replace("\"schema\":2", "\"schema\":2,\"extra\":0");
        assert!(ProfileLifecycleRecord::from_record_bytes(unknown.as_bytes()).is_err());
    }

    #[test]
    fn platform_purge_record_is_exact_and_cannot_skip_barriers() {
        let identity = test_identity(19);
        let prepared = ProfilePurgeRecord::prepared(identity, ProfilePlatform::Windows);
        let removed = prepared
            .advance(ProfilePurgePhase::DataRemoved)
            .expect("data barrier");
        let completed = removed
            .advance(ProfilePurgePhase::Completed)
            .expect("absence verification");
        assert_eq!(completed.identity(), identity);
        assert_eq!(completed.platform(), ProfilePlatform::Windows);
        assert_eq!(completed.phase(), ProfilePurgePhase::Completed);
        assert!(prepared.advance(ProfilePurgePhase::Completed).is_err());
        assert!(completed.advance(ProfilePurgePhase::Prepared).is_err());

        let bytes = removed.to_record_bytes().expect("encode purge");
        assert_eq!(ProfilePurgeRecord::from_record_bytes(&bytes), Ok(removed));
        let text = String::from_utf8(bytes).expect("purge UTF-8");
        let unknown = text.replace("\"schema\":1", "\"schema\":1,\"extra\":0");
        assert!(ProfilePurgeRecord::from_record_bytes(unknown.as_bytes()).is_err());
    }

    #[test]
    fn changed_boot_quarantines_each_non_idle_phase_before_liveness() {
        let old_boot = BootIdentity::from_host_verified_bytes([5; 16]).expect("old boot");
        let new_boot = BootIdentity::from_host_verified_bytes([6; 16]).expect("new boot");
        let owner = ProfileProcessIdentity::from_host_observation(51, 900).expect("process");
        let starting = ProfileLifecycleRecord::idle(old_boot)
            .begin_startup(owner)
            .expect("starting");
        let running = starting
            .advance(ProfileLifecyclePhase::Running)
            .expect("running");
        let stopping = running
            .advance(ProfileLifecyclePhase::Stopping)
            .expect("stopping");

        for record in [starting, running, stopping] {
            assert_eq!(
                next_lifecycle_action(record, RecordedProcessObservation::Unknown, new_boot),
                Ok(ProfileLifecycleAction::WriteQuarantined),
                "a changed boot proves the old non-idle process cannot survive"
            );
            let same_boot =
                next_lifecycle_action(record, RecordedProcessObservation::Unknown, old_boot)
                    .expect_err("same-boot unknown liveness must fail closed");
            assert_eq!(same_boot.kind(), ProfileErrorKind::LifecycleUnproven);

            let quarantined = record
                .quarantine()
                .expect("preserve old boot in quarantine");
            assert_eq!(
                next_lifecycle_action(quarantined, RecordedProcessObservation::Unknown, new_boot,),
                Ok(ProfileLifecycleAction::RestoreIdleAfterBoot)
            );
            let blocked =
                next_lifecycle_action(quarantined, RecordedProcessObservation::Unknown, old_boot)
                    .expect_err("equal-boot quarantine remains blocked");
            assert_eq!(blocked.kind(), ProfileErrorKind::LifecycleUnproven);
        }
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
