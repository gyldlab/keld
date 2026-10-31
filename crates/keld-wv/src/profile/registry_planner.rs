//! Pure profile registry binding/purge action planner and observed-state model.
//! Encoded durable binding/intent records stay owned by the parent.

use super::{
    AppleStoreUuid, BindingPhase, ProfileError, ProfileErrorKind, PurgePhase, RegistryIntent,
    RegistryIntentPhase, StoreBinding,
};

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
            if !forward || !reverse {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            match store {
                None => Ok(RegistryAction::EnumerateStores),
                Some(false) => Err(ProfileError::new(ProfileErrorKind::RegistryCorruption)),
                Some(true) if snapshot.active => Ok(RegistryAction::ClearIntent),
                Some(true) => Ok(RegistryAction::MarkBindingActive),
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
            if !snapshot.active || !forward {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            if let Some(action) = absent_store_purge_action(store)? {
                return Ok(action);
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
            if reverse || !snapshot.active {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            if let Some(action) = absent_store_purge_action(store)? {
                return Ok(action);
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
            if forward || reverse {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            if let Some(action) = absent_store_purge_action(store)? {
                return Ok(action);
            }
            if snapshot.active {
                Ok(RegistryAction::ClearActiveStatus)
            } else {
                Ok(RegistryAction::AdvancePurgeIntent(PurgePhase::Inactive))
            }
        }
        PurgePhase::Inactive => {
            if forward || reverse || snapshot.active {
                return Err(ProfileError::new(ProfileErrorKind::RegistryCorruption));
            }
            if let Some(action) = absent_store_purge_action(store)? {
                Ok(action)
            } else {
                Ok(RegistryAction::ClearIntent)
            }
        }
    }
}

fn absent_store_purge_action(store: Option<bool>) -> Result<Option<RegistryAction>, ProfileError> {
    match store {
        None => Ok(Some(RegistryAction::EnumerateStores)),
        Some(false) => Ok(None),
        Some(true) => Err(ProfileError::new(ProfileErrorKind::RegistryCorruption)),
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
