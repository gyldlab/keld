//! One cold, reusable numeric-result storage witness; no accounting counter.
//! Only the original UI pending record debits/releases `LocalCallBudget`.

use crate::original_call::bridge_error;
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::webview2::WindowsLoopEvent;
use crate::wv_link::{MAX_RENDERER_PAYLOAD_LEN, MAX_WV_LINK_ENVELOPE_LEN};
use crate::{WebviewId, WvError};
use std::alloc::Layout;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tao::event_loop::EventLoopProxy;

/// A prepayment derived from existing bounds, not another quota: raw ERR and
/// decoded strings may coexist, followed by canonical JSON and NUL UTF16.
/// Actual retained capacities (including Core metadata) must fit before growth.
pub(super) const RESULT_STORAGE_BYTES: usize =
    2 * MAX_RENDERER_PAYLOAD_LEN + MAX_WV_LINK_ENVELOPE_LEN + 2 * (MAX_WV_LINK_ENVELOPE_LEN + 1);

pub(super) struct StorageSignal {
    released: AtomicBool,
    wake: Option<EventLoopProxy<WindowsLoopEvent>>,
    view: WebviewId,
}
impl StorageSignal {
    pub(super) fn new(wake: Option<EventLoopProxy<WindowsLoopEvent>>, view: WebviewId) -> Self {
        Self {
            released: AtomicBool::new(true),
            wake,
            view,
        }
    }
    pub(super) fn reusable(signal: &Arc<Self>) -> bool {
        signal.released.load(Ordering::Acquire)
            && Arc::strong_count(signal) == 1
            && Arc::weak_count(signal) == 0
    }
    pub(super) fn view(&self) -> WebviewId {
        self.view
    }
}

/// Requested allocation of pinned Rust `ArcInner`: two `AtomicUsize` counters and T
/// with C layout, including value alignment/tail padding. Allocator/RSS overhead
/// is not claimed to be this byte count.
pub(super) fn allocation_bytes() -> Result<usize, WvError> {
    Layout::new::<[AtomicUsize; 2]>()
        .extend(Layout::new::<StorageSignal>())
        .map(|(layout, _)| layout.pad_to_align().size())
        .map_err(|_| bridge_error("numeric result witness layout overflow"))
}

/// Non-clone storage custody prepaid on the original native numeric call.
///
/// Move this SAME token through Core's pending call and actual Reply/Error.
/// Owned buffers precede this field and must be disposed before token Drop.
/// Drop reports storage disposal only: no `native_done`, `consumer_done`, Function
/// completion, role/source grant, or logical request success is supplied here.
#[must_use = "retain the numeric result reservation with all actual result storage"]
pub struct NativeNumericResultReservation {
    signal: Option<Arc<StorageSignal>>,
    document: Option<CheckedNativeDocumentLease>,
    request: u32,
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
        signal.released.store(false, Ordering::Release);
        Self {
            signal: Some(Arc::clone(signal)),
            document: Some(document),
            request,
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
            && self
                .signal
                .as_ref()
                .is_some_and(|signal| !signal.released.load(Ordering::Acquire))
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
                .signal
                .as_ref()
                .is_some_and(|signal| signal.view == view)
            && self.document.as_ref().is_some_and(|bound| {
                bound.stamp().view() == view
                    && u64::from(bound.stamp().observed_generation()) == navigation
                    && bound.same_binding(document)
            })
    }

    pub(super) fn belongs_to(&self, signal: &Arc<StorageSignal>) -> bool {
        self.signal
            .as_ref()
            .is_some_and(|bound| Arc::ptr_eq(bound, signal))
    }
}
impl Drop for NativeNumericResultReservation {
    fn drop(&mut self) {
        // Dispose this token's owned lease before publishing storage disposal.
        drop(self.document.take());
        let Some(signal) = self.signal.take() else {
            return;
        };
        signal.released.store(true, Ordering::Release);
        let wake = signal.wake.clone();
        // Restore sole ownership BEFORE posting; an immediate UI turn must not
        // consume the only notification while our Arc is still outstanding.
        drop(signal);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            if let Some(wake) = wake {
                let _ = wake.send_event(WindowsLoopEvent::OriginalWake);
            }
        }));
    }
}
