//! The single reusable backing of the existing active/preparing call.
//! Only the queue's sole Rc can change P. A non-clone invocation borrows P
//! immutably across native entry without retaining an `AppState` `RefCell` guard.

use super::{OriginalInvocationKind, WvError, bridge_error};
use crate::renderer_contract::CheckedNativeDocumentLease;
use std::alloc::Layout;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum PreparationPhase {
    Empty,
    Available,
    Borrowed,
    Entered,
    Returned,
    Unproved,
}

pub(super) struct PreparedBacking<P> {
    pub(super) payload: Option<P>,
    pub(super) phase: Cell<PreparationPhase>,
}
impl<P> PreparedBacking<P> {
    pub(super) const fn empty() -> Self {
        Self {
            payload: None,
            phase: Cell::new(PreparationPhase::Empty),
        }
    }
}

/// Requested Rc allocation, including its header, value alignment and tail pad.
/// This mirrors pinned Rust 1.99.1 `alloc::rc::RcInner`: repr(C, align(2)),
/// two Cell<usize> counters followed by T. Allocator bookkeeping/RSS is not
/// misrepresented as a Rust requested allocation. No extra byte cap is added.
fn rc_header_layout() -> Result<Layout, WvError> {
    Layout::from_size_align(
        std::mem::size_of::<[Cell<usize>; 2]>(),
        std::mem::align_of::<Cell<usize>>().max(2),
    )
    .map(|layout| layout.pad_to_align())
    .map_err(|_| bridge_error("original Rc header allocation layout overflow"))
}
pub(super) fn allocation_bytes<P>() -> Result<usize, WvError> {
    rc_header_layout()?
        .extend(Layout::new::<PreparedBacking<P>>())
        .map(|(layout, _)| layout.pad_to_align().size())
        .map_err(|_| bridge_error("original call backing allocation layout overflow"))
}

/// Only the same queue constructs this identity; it grants no current authority.
pub(crate) struct PreparingOriginalIdentity {
    pub(super) id: u32,
    pub(super) lease: CheckedNativeDocumentLease,
}
impl PreparingOriginalIdentity {
    pub(crate) const fn id(&self) -> u32 {
        self.id
    }
    pub(crate) const fn lease(&self) -> &CheckedNativeDocumentLease {
        &self.lease
    }
}

/// The one outstanding immutable reference to the queue's existing backing.
/// No payload leaves that backing, even if a callback, validation or native entry
/// fails. There is no owned-payload return path and no per-call Rc allocation.
pub(crate) struct PreparingOriginalInvocation<P> {
    pub(super) identity: PreparingOriginalIdentity,
    pub(super) kind: OriginalInvocationKind,
    pub(super) backing: Rc<PreparedBacking<P>>,
}
impl<P> PreparingOriginalInvocation<P> {
    pub(crate) const fn identity(&self) -> &PreparingOriginalIdentity {
        &self.identity
    }
    #[cfg(any(target_os = "macos", test))]
    pub(crate) const fn kind(&self) -> OriginalInvocationKind {
        self.kind
    }
    pub(crate) fn payload(&self) -> Result<&P, WvError> {
        if !matches!(
            self.backing.phase.get(),
            PreparationPhase::Borrowed | PreparationPhase::Entered
        ) {
            return Err(bridge_error(
                "original carrier is not available for native preparation",
            ));
        }
        self.backing
            .payload
            .as_ref()
            .ok_or_else(|| bridge_error("original carrier lost its retained payload"))
    }

    /// Explicitly return without native entry. This cannot undo Entered and is
    /// not a physical completion receipt. The queue still owns the same P.
    pub(super) fn defer(self) -> Result<(), WvError> {
        if self.backing.phase.get() != PreparationPhase::Borrowed {
            self.backing.phase.set(PreparationPhase::Unproved);
            return Err(bridge_error(
                "original carrier cannot defer after native entry",
            ));
        }
        self.backing.phase.set(PreparationPhase::Available);
        Ok(())
    }

    /// Call only after the actual native API returned, including an actual
    /// failure return. Function, native completion and consumer facts remain
    /// independent. Dropping this handle then restores the queue's sole Rc.
    pub(crate) fn native_returned(self) -> Result<(), WvError> {
        if self.backing.phase.get() != PreparationPhase::Entered {
            self.backing.phase.set(PreparationPhase::Unproved);
            return Err(bridge_error(
                "original native return lacks the exact entered carrier",
            ));
        }
        self.backing.phase.set(PreparationPhase::Returned);
        Ok(())
    }
}
impl<P> Drop for PreparingOriginalInvocation<P> {
    fn drop(&mut self) {
        if matches!(
            self.backing.phase.get(),
            PreparationPhase::Borrowed | PreparationPhase::Entered
        ) {
            // An abandoned preparation or unwound native entry cannot be
            // promoted to an API-return or no-entry receipt by Rust Drop.
            self.backing.phase.set(PreparationPhase::Unproved);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_current_payload_backings_are_one_48_byte_x64_allocation() {
        #[cfg(target_pointer_width = "64")]
        for bytes in [
            allocation_bytes::<String>().unwrap(),
            allocation_bytes::<Vec<u16>>().unwrap(),
        ] {
            assert_eq!(bytes, 48);
        }
    }
    #[test]
    fn generic_backing_layout_includes_overalignment_and_tail_padding() {
        #[repr(align(64))]
        struct Aligned([u8; 3]);
        let (layout, offset) = rc_header_layout()
            .unwrap()
            .extend(Layout::new::<PreparedBacking<Aligned>>())
            .unwrap();
        assert_eq!(offset % 64, 0);
        assert_eq!(
            allocation_bytes::<Aligned>().unwrap(),
            layout.pad_to_align().size()
        );
        assert_eq!(allocation_bytes::<Aligned>().unwrap() % 64, 0);
        let value = Aligned([1, 2, 3]);
        assert_eq!(value.0, [1, 2, 3]);
    }
}
