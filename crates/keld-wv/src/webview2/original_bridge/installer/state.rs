//! Registration receipts and the same input allocation; no native handle here.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use super::super::super::original_bootstrap::NativeBootstrapText;
use super::{InstallStart, InstallStatus};

#[derive(Clone, Copy)]
enum Entry {
    Prepared,
    InCall,
    Returned(bool),
    Unknown,
}

#[derive(Clone, Copy)]
struct Progress {
    entry: Entry,
    completion: Option<bool>,
    deadline: Instant,
    retired: bool,
    closed: bool,
    fault: bool,
}

pub(super) struct RegistrationState {
    input: RefCell<Option<NativeBootstrapText>>,
    progress: Cell<Progress>,
}

impl RegistrationState {
    pub(super) fn new(input: NativeBootstrapText, deadline: Instant) -> Self {
        Self {
            input: RefCell::new(Some(input)),
            progress: Cell::new(Progress {
                entry: Entry::Prepared,
                completion: None,
                deadline,
                retired: false,
                closed: false,
                fault: false,
            }),
        }
    }

    pub(super) fn previous_start(&self) -> Option<InstallStart> {
        match self.progress.get().entry {
            Entry::Prepared => None,
            Entry::Returned(_) => Some(InstallStart::AlreadyStarted),
            Entry::InCall | Entry::Unknown => Some(InstallStart::OutcomeUnknown),
        }
    }

    pub(super) fn input_pointer(&self) -> Option<*const u16> {
        self.input
            .try_borrow()
            .ok()?
            .as_ref()
            .map(|input| input.as_nul_terminated_utf16().as_ptr())
    }

    /// Pure local state only: called inside the exact Core role fence.
    pub(super) fn begin(&self, now: Instant) -> bool {
        let mut progress = self.progress.get();
        if !matches!(progress.entry, Entry::Prepared)
            || progress.retired
            || progress.closed
            || progress.fault
            || now >= progress.deadline
        {
            return false;
        }
        progress.entry = Entry::InCall;
        self.progress.set(progress);
        true
    }

    /// Check only this owner's local entry sentinel after the canonical fence.
    /// An inconsistent fence result cannot issue native work or cancel custody.
    pub(super) fn finish_admission(&self, admitted: bool) -> Result<(), InstallStart> {
        match (admitted, self.progress.get().entry) {
            (true, Entry::InCall) => Ok(()),
            (false, Entry::Prepared) => {
                self.retire();
                Err(InstallStart::NotAdmitted)
            }
            _ => {
                self.native_unwound();
                Err(InstallStart::OutcomeUnknown)
            }
        }
    }

    pub(super) fn native_return(&self, accepted: bool, now: Instant) {
        let mut progress = self.progress.get();
        if matches!(progress.entry, Entry::InCall) {
            progress.entry = Entry::Returned(accepted);
            progress.fault |= !accepted || now >= progress.deadline;
        } else {
            progress.fault = true;
        }
        self.progress.set(progress);
        self.release_quiesced_input();
    }

    pub(super) fn native_unwound(&self) {
        let mut progress = self.progress.get();
        progress.entry = Entry::Unknown;
        progress.fault = true;
        self.progress.set(progress);
        self.release_quiesced_input();
    }

    pub(super) fn complete(&self, succeeded: bool, now: Instant) {
        let mut progress = self.progress.get();
        progress.fault |= progress.completion.is_some()
            || !matches!(progress.entry, Entry::InCall | Entry::Returned(_))
            || !succeeded
            || now >= progress.deadline;
        progress.completion = Some(succeeded);
        self.progress.set(progress);
        self.release_quiesced_input();
    }

    pub(super) fn status(&self, now: Instant) -> InstallStatus {
        let progress = self.progress.get();
        if progress.retired || progress.closed || progress.fault {
            return InstallStatus::Refused;
        }
        if matches!(progress.entry, Entry::Returned(true)) && progress.completion == Some(true) {
            return InstallStatus::Ready;
        }
        if now >= progress.deadline {
            self.refuse();
            return InstallStatus::Refused;
        }
        InstallStatus::Pending
    }

    pub(super) fn retire(&self) {
        let mut progress = self.progress.get();
        progress.retired = true;
        self.progress.set(progress);
        self.release_quiesced_input();
    }

    pub(super) fn controller_closed(&self) {
        let mut progress = self.progress.get();
        progress.closed = true;
        self.progress.set(progress);
        self.release_quiesced_input();
    }

    pub(super) fn refuse(&self) {
        let mut progress = self.progress.get();
        progress.fault = true;
        self.progress.set(progress);
        self.release_quiesced_input();
    }

    pub(super) fn native_quiesced(&self) -> bool {
        let progress = self.progress.get();
        match progress.entry {
            Entry::Prepared => true,
            Entry::InCall => false,
            Entry::Returned(_) => progress.closed || progress.completion.is_some(),
            Entry::Unknown => progress.closed,
        }
    }

    fn release_quiesced_input(&self) {
        let progress = self.progress.get();
        let release = match progress.entry {
            Entry::Prepared => progress.retired || progress.closed || progress.fault,
            Entry::InCall => false,
            Entry::Returned(_) => progress.closed || progress.completion.is_some(),
            Entry::Unknown => progress.closed,
        };
        if release {
            if let Ok(mut input) = self.input.try_borrow_mut() {
                *input = None;
            } else {
                let mut progress = self.progress.get();
                progress.fault = true;
                self.progress.set(progress);
            }
        }
    }

    #[cfg(test)]
    pub(super) fn retained_input_bytes(&self) -> Result<usize, &'static str> {
        self.input
            .try_borrow()
            .map(|input| {
                input
                    .as_ref()
                    .map_or(0, NativeBootstrapText::allocated_bytes)
            })
            .map_err(|_| "preload input custody is borrowed")
    }
}
