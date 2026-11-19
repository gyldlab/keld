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
                ) && entry
                    .original
                    .as_ref()
                    .is_some_and(|binding| Arc::ptr_eq(&binding.caller, &caller.reader_stop))
            })
    }
}
