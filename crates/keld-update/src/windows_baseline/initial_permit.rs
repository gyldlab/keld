//! KEL-270 D-46: consume live initial-install admission at the KEL-53 effect boundary.
//!
//! This module is intentionally closed until the single KEL-53 metadata writer can
//! issue live admission under its existing repository lease. The canonical
//! D-64/D-68 codecs are data, not witnesses.
//! There is no constructor from a path, record, Boolean, or caller callback.

use std::fs::File;
use std::marker::PhantomData;
use std::rc::Rc;
use std::thread::{self, ThreadId};

use super::error;
use crate::UpdateError;

// The native handle-identity denial can compile and exercise real directory
// oracles before Guard commissions its OS-known-folder issuer. Until then no
// legacy initializer uses this observation as an authority decision.
pub(super) mod legacy_v2_denial;

/// Private native issuer required before the first installation-tree mutation.
///
/// The eventual KEL-53 implementation in this module must own the admitted
/// worker, pinned source and destination, and *the existing repository lease*.
/// It must perform the D-42 through D-45 checks, then under this exact lease
/// publish and reread D-9's seven-reference policy evidence, D-64's admitted
/// record, D-68's pending pointer, and D-64's creation-armed record in order.
/// Neither an independently decoded record nor a reopening by pathname can
/// implement these operations. No implementation is commissioned yet.
trait NativeInitialAdmissionIssuer {
    /// Recheck actual worker identity, sealed source, generation, deadline,
    /// expiry, discovery, and repository ownership under the retained lease.
    fn revalidate_under_repository_lease(&mut self) -> Result<(), UpdateError>;

    /// With this one-use permit already consumed, durably write and reread
    /// the D-9 evidence, D-64 admitted record, D-68 pending pointer, and
    /// D-64 creation-armed record, in that order. A partial write is a
    /// journaled refusal; it may never grant a second permit.
    fn publish_and_read_back_creation_armed(&mut self) -> Result<(), UpdateError>;

    /// Independently reread the exact matching armed journal and pending
    /// pointer through retained native objects before an installation effect.
    fn require_durable_creation_armed(&mut self) -> Result<(), UpdateError>;

    /// Verify the actual exclusive bootstrap handle and repeat D-42/D-43
    /// destination ownership checks while both locks are still held.
    fn revalidate_bootstrap_lock(&mut self, lock: &File) -> Result<(), UpdateError>;

    /// Persist and reread D-64 `bootstrap-owned` while still holding the
    /// metadata lease and the exact exclusive bootstrap handle.
    fn publish_and_read_back_bootstrap_owned(&mut self, lock: &File) -> Result<(), UpdateError>;
}

/// A unique D-46 admission for one real initial installation.
///
/// Construction is private to this module and requires the not-yet-commissioned
/// native issuer above. The value cannot be cloned, copied, serialized,
/// moved to another worker, or reconstructed after a process restart.
/// Its lifetime borrows the native writer's live lease through the journal
/// publication and the later first installation effect.
#[must_use = "unconsumed admission cannot authorize an installation effect"]
pub(super) struct WindowsInitialInstallPermit<'lease> {
    worker: ThreadId,
    issuer: &'lease mut dyn NativeInitialAdmissionIssuer,
    // Keep the permit on the admitted worker even if the future witness type
    // is accidentally made Send or Sync.
    _worker_only: PhantomData<Rc<()>>,
}

/// The only state that can enter the first protected creation, issued by
/// consuming the original permit and completing D-9/D-64/D-68 publication.
///
/// It is affine, thread-bound and retains the same native metadata lease.
#[must_use = "an armed admission must be resolved by the original transaction owner"]
pub(super) struct CreationArmedEffectGate<'lease> {
    worker: ThreadId,
    issuer: &'lease mut dyn NativeInitialAdmissionIssuer,
    _worker_only: PhantomData<Rc<()>>,
}

impl<'lease> WindowsInitialInstallPermit<'lease> {
    /// Consume one live permit BEFORE the first D-64 admission record write.
    ///
    /// After entry, failure at any D-9/D-64/D-68 cut consumes this permit:
    /// recovery belongs to the original bound attempt, never a reissued
    /// permit or a replay. This method cannot perform an installation effect.
    pub(super) fn consume_for_admission_journal(
        self,
    ) -> Result<CreationArmedEffectGate<'lease>, UpdateError> {
        if thread::current().id() != self.worker {
            return Err(error(
                "initial install admission",
                "actual worker changed before admission journal publication",
            ));
        }
        self.issuer.revalidate_under_repository_lease()?;
        self.issuer.publish_and_read_back_creation_armed()?;
        Ok(CreationArmedEffectGate {
            worker: self.worker,
            issuer: self.issuer,
            _worker_only: self._worker_only,
        })
    }
}

impl CreationArmedEffectGate<'_> {
    /// Enter the first KEL-53 installation effect, once, on the admitted worker.
    ///
    /// This is the ONLY permitted route into the effect closure for a v2
    /// initializer. `self` moves and the closure is `FnOnce`; safe Rust cannot
    /// replay the effect. The closure must create any admitted scaffold and
    /// return the actual exclusive KEL-53 bootstrap lock handle. This method
    /// retains the metadata lease until that handle has been revalidated and
    /// D-64 `bootstrap-owned` has been durably reread.
    /// No network/refresh/async dispatch is allowed between revalidation and
    /// first protected creation.
    pub(super) fn enter_bootstrap_effect(
        self,
        effect: impl FnOnce() -> Result<File, UpdateError>,
    ) -> Result<File, UpdateError> {
        if thread::current().id() != self.worker {
            return Err(error(
                "initial install admission",
                "actual worker changed before first protected effect",
            ));
        }
        self.issuer.revalidate_under_repository_lease()?;
        self.issuer.require_durable_creation_armed()?;
        let lock = effect()?;
        self.issuer.revalidate_bootstrap_lock(&lock)?;
        self.issuer.publish_and_read_back_bootstrap_owned(&lock)?;
        Ok(lock)
    }
}
