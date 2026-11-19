//! Shared opaque native document observations and conditional effect admission.
//! Only a platform native owner can mint a checked lease.

use crate::native_document::{NativeAttempt, NativeCommittedStamp};
use crate::{WebviewId, resource_source::AdmittedWebAppSource};
use std::sync::Arc;

/// Immutable **observation** of the checked native main-frame commit. The
/// inner native witness/identity is opaque to Core and the page. Storing or
/// comparing this snapshot does NOT prove that it remains current. Each
/// privilege-bearing native operation must recheck it against the living
/// `NativeDocumentOwner` on the UI owner in the same admission turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeDocumentStamp(pub(crate) NativeCommittedStamp);

impl NativeDocumentStamp {
    /// Actual native view that produced this immutable observation.
    #[must_use]
    pub fn view(self) -> WebviewId {
        self.0.view
    }

    /// Observed native committed generation; not future-operation authority.
    #[must_use]
    pub fn observed_generation(self) -> u32 {
        self.0.generation.get()
    }

    /// Actual native attempt retained by this committed observation.
    /// This accessor cannot mint or authorize a document.
    #[must_use]
    pub const fn attempt(self) -> NativeAttemptObservation {
        NativeAttemptObservation(self.0.attempt)
    }
}

/// A host-started native navigation attempt, without a committed document.
/// No IPC or page authority can derive from this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeAttemptObservation(pub(crate) NativeAttempt);

impl NativeAttemptObservation {
    /// Actual native view to which this opaque attempt belongs.
    #[must_use]
    pub fn view(self) -> WebviewId {
        self.0.view()
    }
}

/// Revocable current-document witness for the final Host IPC admission cut.
/// Only the native document owner can mint/revoke it. A snapshot or true result
/// does not replace rechecking the UI owner for a later native side effect.
#[derive(Clone, Debug)]
pub struct CheckedNativeDocumentLease(pub(crate) crate::native_document::NativeDocumentLease);
impl PartialEq for CheckedNativeDocumentLease {
    fn eq(&self, other: &Self) -> bool {
        self.same_binding(other)
    }
}
impl Eq for CheckedNativeDocumentLease {}
impl CheckedNativeDocumentLease {
    /// Acquire the native owner's synchronous revocation state now.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.0.is_current()
    }
    /// Read the exact immutable native commit this witness belongs to.
    #[must_use]
    pub fn stamp(&self) -> NativeDocumentStamp {
        NativeDocumentStamp(self.0.stamp())
    }
    /// Match the same actual admitted source allocation, including policy aliases.
    #[must_use]
    pub fn belongs_to_source(&self, source: &Arc<AdmittedWebAppSource>) -> bool {
        self.0.belongs_to_source(source)
    }
    /// Observe the living owner's exact current lease and retained source in
    /// this turn. The result is not authority for a later native operation.
    pub(crate) fn belongs_to_owner(
        &self,
        owner: &crate::native_document::NativeDocumentOwner,
    ) -> bool {
        owner
            .lease_for(self.0.stamp())
            .is_some_and(|current| current.same_binding(&self.0))
            && owner
                .source_binding()
                .is_some_and(|source| self.belongs_to_source(&source))
    }
    /// Match the exact original irreversible native lease, not numeric coincidence.
    #[must_use]
    pub fn same_binding(&self, other: &Self) -> bool {
        self.0.same_binding(&other.0)
    }
}
