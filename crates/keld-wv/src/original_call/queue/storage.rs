//! Per-view physical disposal signaling shared by separate Send and numeric owners.

use crate::original_call::bridge_error;
use crate::webview2::WindowsLoopEvent;
use crate::{WebviewId, WvError};
use std::alloc::Layout;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tao::event_loop::EventLoopProxy;

pub(crate) struct StorageSignal {
    released: AtomicBool,
    wake: Option<EventLoopProxy<WindowsLoopEvent>>,
    view: WebviewId,
}
impl StorageSignal {
    pub(crate) fn new(wake: Option<EventLoopProxy<WindowsLoopEvent>>, view: WebviewId) -> Self {
        Self {
            released: AtomicBool::new(true),
            wake,
            view,
        }
    }
    pub(crate) fn reusable(signal: &Arc<Self>) -> bool {
        signal.released.load(Ordering::Acquire)
            && Arc::strong_count(signal) == 1
            && Arc::weak_count(signal) == 0
    }
    pub(super) fn held(&self) -> bool {
        !self.released.load(Ordering::Acquire)
    }
    pub(super) fn view(&self) -> WebviewId {
        self.view
    }
}

/// Requested allocation of pinned Rust `ArcInner`: two `AtomicUsize` counters and T
/// with C layout, including value alignment/tail padding. Allocator/RSS overhead
/// is not claimed to be this byte count.
pub(crate) fn allocation_bytes() -> Result<usize, WvError> {
    Layout::new::<[AtomicUsize; 2]>()
        .extend(Layout::new::<StorageSignal>())
        .map(|(layout, _)| layout.pad_to_align().size())
        .map_err(|_| bridge_error("original storage witness layout overflow"))
}

/// Physical storage only. No numeric prepayment, role, or logical disposition.
#[must_use = "keep the token after every buffer whose disposal it witnesses"]
pub(crate) struct StorageToken {
    signal: Option<Arc<StorageSignal>>,
}

impl std::fmt::Debug for StorageToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageToken").finish_non_exhaustive()
    }
}

impl StorageToken {
    pub(crate) fn try_acquire(signal: &Arc<StorageSignal>) -> Option<Self> {
        StorageSignal::reusable(signal).then(|| Self::acquire(signal))
    }

    // Numeric admission already checked this same slot before moving its request.
    pub(super) fn acquire(signal: &Arc<StorageSignal>) -> Self {
        signal.released.store(false, Ordering::Release);
        Self {
            signal: Some(Arc::clone(signal)),
        }
    }

    pub(super) fn signal(&self) -> Option<&Arc<StorageSignal>> {
        self.signal.as_ref()
    }
}

impl Drop for StorageToken {
    fn drop(&mut self) {
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
