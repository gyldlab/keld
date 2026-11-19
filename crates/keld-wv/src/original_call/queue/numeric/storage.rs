//! One cold, reusable numeric-result storage witness; no accounting counter.
//! Only the original UI pending record debits/releases `LocalCallBudget`.

use super::super::storage::{StorageSignal, StorageToken};
use crate::WebviewId;
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::wv_link::{MAX_RENDERER_PAYLOAD_LEN, MAX_WV_LINK_ENVELOPE_LEN};
use std::sync::Arc;

/// A prepayment derived from existing bounds, not another quota: raw ERR and
/// decoded strings may coexist, followed by canonical JSON and NUL UTF16.
/// Actual retained capacities (including Core metadata) must fit before growth.
pub(super) const RESULT_STORAGE_BYTES: usize =
    2 * MAX_RENDERER_PAYLOAD_LEN + MAX_WV_LINK_ENVELOPE_LEN + 2 * (MAX_WV_LINK_ENVELOPE_LEN + 1);

/// Non-clone storage custody prepaid on the original native numeric call.
///
/// Move this SAME token through Core's pending call and actual Reply/Error.
/// Owned buffers precede this field and must be disposed before token Drop.
/// Drop reports storage disposal only: no `native_done`, `consumer_done`, Function
/// completion, role/source grant, or logical request success is supplied here.
#[must_use = "retain the numeric result reservation with all actual result storage"]
pub struct NativeNumericResultReservation {
    document: Option<CheckedNativeDocumentLease>,
    request: u32,
    storage: StorageToken,
}
impl std::fmt::Debug for NativeNumericResultReservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeNumericResultReservation")
            .finish_non_exhaustive()
    }
}
impl NativeNumericResultReservation {
    pub(super) fn mint(
        signal: &Arc<StorageSignal>,
        document: CheckedNativeDocumentLease,
        request: u32,
    ) -> Self {
        Self {
            document: Some(document),
            request,
            storage: StorageToken::acquire(signal),
        }
    }

    /// Canonical reader checks this numeric-only body ceiling before allocation.
    #[must_use]
    pub const fn maximum_payload_len(&self) -> usize {
        MAX_RENDERER_PAYLOAD_LEN
    }

    /// Observe the prepayment without reserving, extending, or moving credit.
    /// Count simultaneous raw/decoded/carrier capacities and producer metadata;
    /// a failed check authorizes no allocation or hidden CONTROL fallback.
    #[must_use]
    pub fn covers_storage(&self, accounted_bytes: usize) -> bool {
        accounted_bytes <= RESULT_STORAGE_BYTES
            && self.storage.signal().is_some_and(|signal| signal.held())
    }

    /// Correlate storage to the original admitted request, including after
    /// logical revocation for cleanup. This is not a current-authority check.
    #[must_use]
    pub fn matches_request(
        &self,
        view: WebviewId,
        navigation: u64,
        request: u32,
        document: &CheckedNativeDocumentLease,
    ) -> bool {
        self.request == request
            && self
                .storage
                .signal()
                .is_some_and(|signal| signal.view() == view)
            && self.document.as_ref().is_some_and(|bound| {
                bound.stamp().view() == view
                    && u64::from(bound.stamp().observed_generation()) == navigation
                    && bound.same_binding(document)
            })
    }

    pub(super) fn belongs_to(&self, signal: &Arc<StorageSignal>) -> bool {
        self.storage
            .signal()
            .is_some_and(|bound| Arc::ptr_eq(bound, signal))
    }
}

impl Drop for NativeNumericResultReservation {
    fn drop(&mut self) {
        // Dispose the actual document lease before the storage field signals.
        drop(self.document.take());
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeNumericResultReservation, StorageSignal};
    use crate::WebviewId;
    use crate::native_document::{NativeCommitIdentity, NativeDocumentOwner};
    use crate::renderer_contract::CheckedNativeDocumentLease;
    use crate::resource_source::windows::tests::Fixture;
    use std::num::NonZeroU64;
    use std::sync::Arc;

    #[test]
    fn result_storage_tracks_only_the_original_view_request_and_document() {
        let fixture = Fixture::new();
        let source = fixture.source_arc();
        let view = WebviewId(9);
        let mut owner = NativeDocumentOwner::new(view);
        owner.bind_source(&source).expect("retain source");
        let attempt = owner
            .begin_host_load(source.source_id())
            .expect("start load");
        let stamp = owner
            .commit_admitted(
                attempt,
                source.source_id(),
                NativeCommitIdentity(NonZeroU64::new(73).expect("nonzero fixture identity")),
            )
            .expect("modeled native commit");
        let lease = CheckedNativeDocumentLease(owner.lease_for(stamp).expect("checked lease"));
        let generation = u64::from(lease.stamp().observed_generation());
        let signal = Arc::new(StorageSignal::new(None, view));
        let token = NativeNumericResultReservation::mint(&signal, lease.clone(), 17);

        assert_eq!(token.maximum_payload_len(), 4096);
        assert!(token.covers_storage(204_802));
        assert!(!token.covers_storage(204_803));
        assert!(token.matches_request(view, generation, 17, &lease));
        assert!(!token.matches_request(WebviewId(10), generation, 17, &lease));
        assert!(!token.matches_request(view, generation + 1, 17, &lease));
        assert!(!token.matches_request(view, generation, 18, &lease));
        owner.destroy();
        assert!(!lease.is_current());
        assert!(token.matches_request(view, generation, 17, &lease));
        assert!(!StorageSignal::reusable(&signal));
        drop(token);
        assert!(StorageSignal::reusable(&signal));
    }
}
