//! Fixed H4 scheduling metadata on the original releasing row.
//! Only a real Destroyed call can fill the receipt bit. No native owner, queue,
//! deadline or production build decision is added here.
#![forbid(unsafe_code)]

use super::{ManagedCustody, Releasing};
use crate::WvError;

#[derive(Debug, Default, PartialEq, Eq)]
enum Wake {
    #[default]
    Waiting,
    Posting,
    Posted,
    Failed,
    Consumed,
}

/// Fixed fields only; the owning `ReleasingEntry` retains its original park time.
#[derive(Debug, Default)]
pub(super) struct ReceiptPrefill {
    builds: [u64; 2],
    admitted: u8,
    destroyed: bool,
    cancelled: bool,
    wake: Wake,
}

// This is a compiled layout witness, not a new admission or storage allowance.
// It includes the optional field as actually stored on the one existing row.
const _: () = assert!(std::mem::size_of::<Option<ReceiptPrefill>>() <= 32);

impl ReceiptPrefill {
    pub(super) fn withhold_destroyed(&mut self, hwnd: isize) -> bool {
        if self.wake == Wake::Consumed {
            return false;
        }
        if !self.destroyed {
            self.destroyed = true;
            eprintln!("DESTROYED_PREFILL_CAPTURED {hwnd}");
        }
        true
    }

    fn observe_create(&mut self, build: u64) -> Option<u8> {
        let count = usize::from(self.admitted);
        if self.cancelled || count == self.builds.len() || self.builds[..count].contains(&build) {
            return None;
        }
        self.builds[count] = build;
        self.admitted += 1;
        Some(self.admitted)
    }

    fn begin_post(&mut self) -> bool {
        if !self.destroyed
            || !(self.cancelled || usize::from(self.admitted) == self.builds.len())
            || self.wake != Wake::Waiting
        {
            return false;
        }
        self.wake = Wake::Posting;
        true
    }

    fn finish_post(&mut self, posted: bool) {
        self.wake = if posted { Wake::Posted } else { Wake::Failed };
    }

    fn take(&mut self) -> bool {
        if self.wake != Wake::Posted {
            return false;
        }
        self.wake = Wake::Consumed;
        true
    }
}

impl<K: Ord + Copy, O> ManagedCustody<K, O> {
    /// Arm only after this exact Destroy answer was parked, before native retire.
    /// The once-consumed `NativeHold` selector permits at most this one row.
    pub(in crate::webview2) fn arm_destroyed_prefill(&mut self, key: K) -> Result<(), WvError> {
        if self.releasing.values().any(|entry| entry.prefill.is_some()) {
            return Err(prefill_error("an original receipt row is already selected"));
        }
        let entry = self
            .releasing
            .get_mut(&key)
            .ok_or_else(|| prefill_error("the actual parked Destroy row is absent"))?;
        if entry.staged || !matches!(entry.answer, Releasing::Destroy(_)) {
            return Err(prefill_error(
                "only an unstaged actual Destroy row can be selected",
            ));
        }
        entry.prefill = Some(ReceiptPrefill::default());
        eprintln!("DESTROYED_PREFILL_ARMED {}", entry.hwnd);
        Ok(())
    }

    /// Called from actual admission after terminal rejection, before Start/Queued.
    pub(super) fn observe_prefill_create(&mut self, build: u64) {
        for entry in self.releasing.values_mut() {
            if let Some(prefill) = entry.prefill.as_mut()
                && let Some(ordinal) = prefill.observe_create(build)
            {
                eprintln!(
                    "DESTROYED_PREFILL_CREATE {} build={build} ordinal={ordinal}",
                    entry.hwnd
                );
            }
        }
    }

    /// Called only at the existing callback-end deferred-receipt hook.
    /// A failed post keeps the same captured receipt and is never retried.
    pub(in crate::webview2) fn post_prefilled_destroyed(
        &mut self,
        post: impl FnOnce() -> bool,
    ) -> Result<(), WvError> {
        for entry in self.releasing.values_mut() {
            if let Some(prefill) = entry.prefill.as_mut()
                && prefill.begin_post()
            {
                let posted = post();
                prefill.finish_post(posted);
                if !posted {
                    return Err(prefill_error("the retained receipt wake post failed"));
                }
                eprintln!("DESTROYED_PREFILL_WAKE {}", entry.hwnd);
                return Ok(());
            }
        }
        Ok(())
    }

    /// The caller then uses the unchanged `stage_destroyed` function.
    /// The row remains present and publication still requires a later native read.
    pub(in crate::webview2) fn take_prefilled_destroyed(&mut self) -> Option<K> {
        for (key, entry) in &mut self.releasing {
            if let Some(prefill) = entry.prefill.as_mut()
                && prefill.take()
            {
                eprintln!("DESTROYED_PREFILL_CONSUMED {}", entry.hwnd);
                return Some(*key);
            }
        }
        None
    }

    /// Stop cancels only the artificial wait for more creates. Without an actual
    /// Destroyed receipt there is still nothing to stage or publish.
    pub(in crate::webview2) fn cancel_prefill_wait(&mut self) {
        for entry in self.releasing.values_mut() {
            if let Some(prefill) = entry.prefill.as_mut() {
                prefill.cancelled = true;
            }
        }
    }
}

fn prefill_error(detail: &str) -> WvError {
    WvError::EventLoop(format!("native receipt prefill fixture: {detail}"))
}

#[cfg(test)]
mod tests;
