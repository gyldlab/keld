//! Move the same reader outside its `RefCell` only for native wait operations.
//! The in-entry phase aliases its existing result slot and cannot be drained.
//! No caller can install a replacement reader or a second completion slot.

use super::{Completed, Reader, failure, retain_result};
use crate::WvError;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

pub(in crate::webview2::original_resource) struct ReaderCustody {
    phase: Phase,
}

enum Phase {
    Retained(Reader),
    NativeEntry {
        result: Arc<Mutex<Option<Completed>>>,
        // This is the same reader's pending unproved transition while its body
        // is locally owned; restoration transfers it to Reader::unproved.
        unproved: bool,
    },
}

impl ReaderCustody {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(in crate::webview2::original_resource) fn observe_custody(&self) -> serde_json::Value {
        match &self.phase {
            Phase::Retained(reader) => reader.observe_custody(),
            Phase::NativeEntry { unproved, .. } => serde_json::json!({
                "phase": "native-entry",
                "unproved": unproved,
            }),
        }
    }

    /// Configure only the same fresh Reader before it owns a task or native wait.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(in crate::webview2::original_resource) fn configure_test_hold(
        &mut self,
        directory: Option<std::path::PathBuf>,
    ) -> Result<(), WvError> {
        let Phase::Retained(reader) = &mut self.phase else {
            return Err(failure("test hold configuration met a native reader entry"));
        };
        if reader.worker.is_some()
            || reader.submit.is_some()
            || reader.active.is_some()
            || reader.wait.is_some()
            || reader.shutdown
            || reader.joined
            || reader.unproved
        {
            return Err(failure(
                "test hold configuration requires the fresh existing reader",
            ));
        }
        reader.test_hold_directory = directory;
        Ok(())
    }

    pub(in crate::webview2::original_resource) fn new() -> Self {
        Self {
            phase: Phase::Retained(Reader::new()),
        }
    }

    pub(in crate::webview2::original_resource) fn take_result(
        &mut self,
    ) -> Result<Option<Completed>, WvError> {
        match &mut self.phase {
            Phase::Retained(reader) => reader.take_result(),
            Phase::NativeEntry { unproved, .. } => {
                *unproved = true;
                Err(failure(
                    "resource reader result observed during native entry",
                ))
            }
        }
    }

    pub(in crate::webview2::original_resource) fn retain_unproved(&mut self, completed: Completed) {
        match &mut self.phase {
            Phase::Retained(reader) => reader.retain_unproved(completed),
            Phase::NativeEntry { result, unproved } => {
                *unproved = true;
                // Completed is already owned, outside a fallible closure.
                // The same result Arc accepts its existing buffer/credit.
                retain_result(result, completed);
            }
        }
    }

    pub(in crate::webview2::original_resource) fn retire(&mut self) -> Result<(), WvError> {
        match &mut self.phase {
            Phase::Retained(reader) => {
                reader.retire();
                Ok(())
            }
            Phase::NativeEntry { unproved, .. } => {
                *unproved = true;
                Err(failure(
                    "resource reader retirement overlapped native entry",
                ))
            }
        }
    }

    pub(in crate::webview2::original_resource) fn is_drained(&self) -> bool {
        matches!(&self.phase, Phase::Retained(reader) if reader.is_drained())
    }
}

/// Only this helper replaces the phase after construction. A reentrant operation
/// observes `NativeEntry`, latches its failure and cannot install another Reader.
/// The owned Reader stays outside `catch_unwind` and returns to its exact field
/// before any error escapes. Operations return no owned body/native payload.
pub(in crate::webview2::original_resource) fn with_reader_on_ui(
    slot: &RefCell<ReaderCustody>,
    operation: impl FnOnce(&mut Reader) -> Result<(), WvError>,
) -> Result<(), WvError> {
    let mut reader = {
        let mut custody = slot.borrow_mut();
        let result = match &mut custody.phase {
            Phase::Retained(reader) => Arc::clone(&reader.result),
            Phase::NativeEntry { unproved, .. } => {
                *unproved = true;
                return Err(failure("resource reader native operation reentered"));
            }
        };
        let Phase::Retained(reader) = std::mem::replace(
            &mut custody.phase,
            Phase::NativeEntry {
                result,
                unproved: false,
            },
        ) else {
            // The immediately preceding exclusive match proved Retained.
            return Err(failure("resource reader phase changed without an owner"));
        };
        reader
    };
    // No RefCell borrow survives native entry, zero-time signal or unregister.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(&mut reader)));
    let mut custody = slot.borrow_mut();
    let entry_unproved = matches!(&custody.phase, Phase::NativeEntry { unproved: true, .. });
    if entry_unproved || !matches!(&outcome, Ok(Ok(()))) {
        reader.mark_unproved();
    }
    // All replacement writes are private to this helper. Nested entry only
    // changes NativeEntry.unproved; it cannot replace or consume this owner.
    custody.phase = Phase::Retained(reader);
    drop(custody);
    match outcome {
        Ok(Ok(())) if !entry_unproved => Ok(()),
        Ok(Err(error)) => Err(error),
        Ok(Ok(())) => Err(failure("resource reader overlap left custody unproved")),
        Err(_) => Err(failure(
            "resource reader native operation panicked; custody retained",
        )),
    }
}

#[cfg(test)]
#[path = "custody_tests.rs"]
mod tests;
