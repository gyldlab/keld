//! Shared opaque native document observations and conditional effect admission.
//! Only a platform native owner can mint a checked lease.

use crate::native_document::{NativeAttempt, NativeCommittedStamp};
use crate::{WebviewId, resource_source::AdmittedWebAppSource};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

/// Immutable **observation** of the one real GH542 `WKNavigation` commit. The
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

/// Terminal choice at the native conditional document-destroy admission cut.
/// Acceptance means irreversible retirement, never physical window release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OriginalDocumentDestroyDecision {
    /// Neither native admission nor Core retirement has won yet.
    Pending = 0,
    /// Native accepted the exact current document and retired it on its UI turn.
    Accepted = 1,
    /// Native refused before changing document, bridge, resources or window.
    Refused = 2,
    /// Core retirement won before native admission; no conditional effect may run.
    Cancelled = 3,
}

/// One bounded in-flight conditional native effect shared with the Core Entry.
/// Arc allocation identity associates the command and receipt; this is neither
/// a document identity nor a principal. Only the living native owner may accept.
pub struct OriginalDocumentDestroyAdmission {
    decision: AtomicU8,
}

impl OriginalDocumentDestroyAdmission {
    /// Create one pending admission before dispatching the immutable command.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            decision: AtomicU8::new(OriginalDocumentDestroyDecision::Pending as u8),
        }
    }

    /// Observe the same cancellation/admission winner before successor adoption.
    #[must_use]
    pub fn decision(&self) -> OriginalDocumentDestroyDecision {
        // Only the constructors and terminal CAS below write these four values.
        // Any unrecognized representation refuses effect/adoption as cancelled.
        match self.decision.load(Ordering::Acquire) {
            0 => OriginalDocumentDestroyDecision::Pending,
            1 => OriginalDocumentDestroyDecision::Accepted,
            2 => OriginalDocumentDestroyDecision::Refused,
            _ => OriginalDocumentDestroyDecision::Cancelled,
        }
    }

    /// Cancel only before the native effect is admitted. False requires reading
    /// `decision()`: Accepted must synchronously become nonadoptable in Core.
    #[must_use]
    pub fn cancel_before_admission(&self) -> bool {
        self.finish_pending(OriginalDocumentDestroyDecision::Cancelled)
    }

    pub(crate) fn try_accept_on_ui(&self) -> bool {
        self.finish_pending(OriginalDocumentDestroyDecision::Accepted)
    }

    pub(crate) fn refuse_on_ui(&self) -> bool {
        self.finish_pending(OriginalDocumentDestroyDecision::Refused)
    }

    fn finish_pending(&self, terminal: OriginalDocumentDestroyDecision) -> bool {
        self.decision
            .compare_exchange(
                OriginalDocumentDestroyDecision::Pending as u8,
                terminal as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// Construct Accepted for Core state-only reconciliation tests. This mints
    /// no native lease, view, document or authority and is absent in production.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    #[must_use]
    pub const fn fixture_accepted() -> Self {
        Self {
            decision: AtomicU8::new(OriginalDocumentDestroyDecision::Accepted as u8),
        }
    }

    /// Compete with Core cancellation in state-only retirement-order tests.
    /// This supplies no native lease or effect and is absent in production.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    #[must_use]
    pub fn fixture_try_accept(&self) -> bool {
        self.try_accept_on_ui()
    }

    /// Construct Refused for Core state-only reconciliation tests, never native
    /// evidence. This fixture is absent from production builds.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    #[must_use]
    pub const fn fixture_refused() -> Self {
        Self {
            decision: AtomicU8::new(OriginalDocumentDestroyDecision::Refused as u8),
        }
    }
}

impl Default for OriginalDocumentDestroyAdmission {
    fn default() -> Self {
        Self::new()
    }
}
impl std::fmt::Debug for OriginalDocumentDestroyAdmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OriginalDocumentDestroyAdmission")
            .field("decision", &self.decision())
            .finish()
    }
}
impl PartialEq for OriginalDocumentDestroyAdmission {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}
impl Eq for OriginalDocumentDestroyAdmission {}
