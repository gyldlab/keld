//! The existing Registry entry retains the source and current Runtime allocation.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::{
    CorrelationId, CreateOutcome, Entry, OriginalWindowResources, PendingWindowCall, Phase,
    WindowError, WindowOwner, WindowRef, WindowRegistry,
};
use crate::app_session::PrimaryCallerContext;

pub(super) struct OriginalWindowBinding {
    resources: OriginalWindowResources,
    caller: Arc<AtomicBool>,
    native_offers_closed: bool,
}

pub(super) fn adopt_original_binding(
    entry: &mut Entry,
    requested: &Option<OriginalWindowBinding>,
) -> Result<(), WindowError> {
    match (&mut entry.original, requested) {
        (None, None) => Ok(()),
        (Some(existing), Some(requested))
            if Arc::ptr_eq(&existing.resources.source, &requested.resources.source)
                && existing
                    .resources
                    .preload
                    .same_configuration(&requested.resources.preload) =>
        {
            existing.caller = Arc::clone(&requested.caller);
            Ok(())
        }
        _ => Err(WindowError::WrongOwner),
    }
}

impl WindowRegistry {
    pub(in crate::app_session) fn create_original_for_caller(
        &mut self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        size: (u32, u32),
        position: (Option<i32>, Option<i32>),
        resources: OriginalWindowResources,
    ) -> Result<CreateOutcome, WindowError> {
        self.create_with_resources(
            PendingWindowCall {
                owner: WindowOwner::new(caller.identity, caller.attempt),
                correlation,
            },
            size,
            position,
            Some(OriginalWindowBinding {
                resources,
                caller: Arc::clone(&caller.reader_stop),
                native_offers_closed: false,
            }),
        )
    }

    /// Called with the canonical Runtime caller fence held. Opening is eligible
    /// for document-start registration; closing cannot admit new work.
    pub(in crate::app_session) fn current_original_binding(
        &self,
        caller: &PrimaryCallerContext,
        window: WindowRef,
    ) -> bool {
        self.checked_entry(WindowOwner::new(caller.identity, caller.attempt), window)
            .is_ok_and(|entry| {
                matches!(
                    entry.phase,
                    Phase::Opening(_) | Phase::Open(_) | Phase::ClosePending(_, _)
                ) && entry.original.as_ref().is_some_and(|binding| {
                    !binding.native_offers_closed
                        && Arc::ptr_eq(&binding.caller, &caller.reader_stop)
                })
            })
    }

    /// The physical scope survives caller retirement until native release.
    pub(in crate::app_session) fn native_view_scope_status(
        &self,
        window: WindowRef,
    ) -> Option<bool> {
        let entry = self
            .entries
            .get(&window.id.0)
            .filter(|entry| entry.window == window)?;
        entry
            .original
            .as_ref()
            .map(|binding| binding.native_offers_closed)
    }

    pub(in crate::app_session) fn close_native_view_offers(&mut self, window: WindowRef) -> bool {
        let Some(binding) = self
            .entries
            .get_mut(&window.id.0)
            .filter(|entry| entry.window == window)
            .and_then(|entry| entry.original.as_mut())
        else {
            return false;
        };
        binding.native_offers_closed = true;
        true
    }
}
