//! One Host-wide bounded reader. Only safe file/source data crosses this thread.
//! The existing native wait owner proves actual thread/TLS termination on the UI.

use super::{Source, failure};

mod custody;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod test_hold;
use crate::resource_intake::{ORIGINAL_WEBAPP_LIMITS, RetainedByteCredit, TaskTicket};
use crate::resource_source::{ResourceSourceError, SelectedRead, VerifiedBytes};
use crate::webview2::{BrowserExitWake, WindowsLoopEvent};
use crate::{WebviewId, WvError};
pub(super) use custody::{ReaderCustody, with_reader_on_ui};
use std::os::windows::io::AsHandle as _;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{SyncSender, TrySendError, sync_channel},
};
use std::thread::{self, JoinHandle};
use tao::event_loop::EventLoopProxy;

pub(super) struct Work {
    pub(super) ticket: TaskTicket,
    pub(super) view: WebviewId,
    pub(super) source: Arc<Source>,
    pub(super) selected: SelectedRead,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) credit: RetainedByteCredit,
}

pub(super) struct Completed {
    pub(super) ticket: TaskTicket,
    pub(super) result: Result<VerifiedBytes, ResourceSourceError>,
    pub(super) _credit: RetainedByteCredit,
}

fn read_work(
    work: Work,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))] hold_directory: Option<
        &std::path::Path,
    >,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))] wake_failed: &AtomicBool,
) -> Completed {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    let observed_view = work.view;
    let Work {
        ticket,
        source,
        selected,
        cancelled,
        credit,
        ..
    } = work;
    let result = source.verified_read(
        &selected,
        ORIGINAL_WEBAPP_LIMITS.max_asset_bytes,
        &cancelled,
    );
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    if let Some(directory) = hold_directory {
        let observation = test_hold::ReadObservation {
            ticket,
            view: observed_view,
            selected: &selected,
            source: &source,
            credit: &credit,
            result: &result,
            cancelled: &cancelled,
        };
        if let Err(error) = test_hold::wait_for_fixture_release(directory, &observation) {
            use std::io::Write as _;
            // Keep and publish this exact real Completed/body/credit below.
            // The existing failure flag refuses a successful reader drain.
            wake_failed.store(true, Ordering::Release);
            let _ = writeln!(
                std::io::stderr(),
                "KELD_ORIGINAL_RESOURCE_HOLD_FAILED {error}"
            );
        }
    }
    // Job metadata and source aliases are quiescent before the UI can observe
    // this result; its actual buffer and reservation move together.
    drop(selected);
    drop(source);
    drop(cancelled);
    Completed {
        ticket,
        result,
        _credit: credit,
    }
}

pub(super) struct Reader {
    submit: Option<SyncSender<Work>>,
    worker: Option<JoinHandle<()>>,
    wait: Option<BrowserExitWake>,
    result: Arc<Mutex<Option<Completed>>>,
    wake_failed: Arc<AtomicBool>,
    active: Option<TaskTicket>,
    shutdown: bool,
    joined: bool,
    unproved: bool,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    test_hold_directory: Option<std::path::PathBuf>,
}

impl Reader {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fn observe_custody(&self) -> serde_json::Value {
        let (result_available, result_ticket) = match self.result.try_lock() {
            Ok(slot) => (
                true,
                slot.as_ref().map(|value| format!("{:?}", value.ticket)),
            ),
            Err(_) => (false, None),
        };
        serde_json::json!({
            "phase": "retained",
            "activeTicket": self.active.map(|ticket| format!("{ticket:?}")),
            "shutdown": self.shutdown,
            "joined": self.joined,
            "unproved": self.unproved,
            "workerRetained": self.worker.is_some(),
            "waitRetained": self.wait.is_some(),
            "resultAvailable": result_available,
            "resultTicket": result_ticket,
            "wakeFailed": self.wake_failed.load(Ordering::Acquire),
        })
    }

    pub(super) fn new() -> Self {
        Self {
            submit: None,
            worker: None,
            wait: None,
            result: Arc::new(Mutex::new(None)),
            wake_failed: Arc::new(AtomicBool::new(false)),
            active: None,
            shutdown: false,
            joined: false,
            unproved: false,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            test_hold_directory: None,
        }
    }

    fn start(&mut self, proxy: &EventLoopProxy<WindowsLoopEvent>) -> Result<(), WvError> {
        if self.shutdown || self.unproved {
            return Err(failure("resource reader is retiring or unproved"));
        }
        if self.worker.is_some() {
            return Ok(());
        }
        let (sender, receiver) = sync_channel::<Work>(1);
        let completed = Arc::clone(&self.result);
        let wake_failed = Arc::clone(&self.wake_failed);
        let wake = proxy.clone();
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let mut hold_directory = self.test_hold_directory.take();
        let worker = thread::Builder::new()
            .name(String::from("keld-original-resource"))
            .spawn(move || {
                while let Ok(work) = receiver.recv() {
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    let selected_hold = if work.selected.path == test_hold::FIXTURE_ASSET {
                        hold_directory.take()
                    } else {
                        None
                    };
                    let response = read_work(
                        work,
                        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                        selected_hold.as_deref(),
                        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                        &wake_failed,
                    );
                    let Ok(mut slot) = completed.lock() else {
                        wake_failed.store(true, Ordering::Release);
                        break;
                    };
                    if slot.is_some() {
                        // Never overwrite another retained body/credit. The sole
                        // ledger should make this impossible; fail closed if not.
                        wake_failed.store(true, Ordering::Release);
                        break;
                    }
                    *slot = Some(response);
                    drop(slot);
                    if wake.send_event(WindowsLoopEvent::OriginalWake).is_err() {
                        wake_failed.store(true, Ordering::Release);
                        break;
                    }
                }
            })
            .map_err(|_| failure("cannot start the bounded resource reader"))?;
        let thread_handle = worker.as_handle().try_clone_to_owned();
        self.submit = Some(sender);
        self.worker = Some(worker);
        let wait = thread_handle
            .map_err(|_| failure("cannot retain resource reader thread identity"))
            .and_then(|handle| {
                BrowserExitWake::register_for_resource_reader(handle, proxy.clone())
            });
        match wait {
            Ok(wait) => {
                self.wait = Some(wait);
                Ok(())
            }
            Err(error) => {
                self.shutdown = true;
                self.unproved = true;
                self.submit.take();
                // No detaching/join-on-UI fallback. Retain the actual JoinHandle;
                // failed wait registration cannot claim Host shutdown success.
                Err(error)
            }
        }
    }

    pub(super) fn submit(
        &mut self,
        staged: &mut Option<Work>,
        proxy: &EventLoopProxy<WindowsLoopEvent>,
    ) -> Result<(), WvError> {
        if self.active.is_some() {
            return Err(failure("resource reader already owns a task"));
        }
        if staged.is_none() {
            return Err(failure("resource submission has no reserved work"));
        }
        self.start(proxy)?;
        let Some(sender) = self.submit.as_ref() else {
            return Err(failure("resource reader sender is unavailable"));
        };
        let Some(work) = staged.take() else {
            return Err(failure("reserved resource work disappeared"));
        };
        let ticket = work.ticket;
        match sender.try_send(work) {
            Ok(()) => {
                self.active = Some(ticket);
                Ok(())
            }
            Err(TrySendError::Full(work) | TrySendError::Disconnected(work)) => {
                *staged = Some(work);
                Err(failure(
                    "bounded resource reader did not accept its reserved task",
                ))
            }
        }
    }

    pub(super) fn take_result(&mut self) -> Result<Option<Completed>, WvError> {
        if self.unproved {
            return Err(failure("resource reader custody is unproved"));
        }
        let mut slot = self
            .result
            .lock()
            .map_err(|_| failure("resource completion owner is poisoned"))?;
        if let Some(result) = slot.as_ref() {
            if self.active != Some(result.ticket) {
                self.unproved = true;
                return Err(failure(
                    "resource completion does not own the active reader ticket",
                ));
            }
            self.active = None;
        }
        Ok(slot.take())
    }

    fn mark_unproved(&mut self) {
        self.unproved = true;
        self.shutdown = true;
        self.submit.take();
    }

    pub(super) fn retain_unproved(&mut self, completed: Completed) {
        self.mark_unproved();
        retain_result(&self.result, completed);
    }

    pub(super) fn retire(&mut self) {
        if self.shutdown {
            return;
        }
        self.shutdown = true;
        self.submit.take();
        if self.worker.is_none() {
            self.joined = true;
        }
    }

    pub(super) fn poll_termination(&mut self) -> Result<(), WvError> {
        if self.unproved || self.wake_failed.load(Ordering::Acquire) {
            return Err(failure("resource reader termination/wake is unproved"));
        }
        let Some(wait) = self.wait.as_mut() else {
            return Ok(());
        };
        if !wait.resource_reader_signaled()? {
            return Ok(());
        }
        // The same kernel thread is signaled, including TLS destruction. The
        // pool callback has no source/credit. Retire it before consuming JoinHandle.
        wait.retire_checked()
            .inspect_err(|_| self.unproved = true)?;
        // Retirement succeeded on that same owner. Only now may its retired
        // shell be dropped, followed by the already-signaled JoinHandle.
        self.wait.take();
        let Some(worker) = self.worker.take() else {
            self.unproved = true;
            return Err(failure("signaled resource thread lost its JoinHandle"));
        };
        let joined = worker.join();
        if joined.is_err() || !self.shutdown {
            self.unproved = true;
            return Err(failure(
                "resource reader ended without proved orderly retirement",
            ));
        }
        self.joined = true;
        Ok(())
    }

    pub(super) fn is_drained(&self) -> bool {
        self.shutdown
            && self.joined
            && !self.unproved
            && self.active.is_none()
            && self.result.lock().is_ok_and(|slot| slot.is_none())
    }
}

// One existing result slot is shared with the native-entry phase. This is the
// same contradiction-retention branch as before, not another completion queue.
fn retain_result(result: &Mutex<Option<Completed>>, completed: Completed) {
    let mut slot = result
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if slot.is_none() {
        *slot = Some(completed);
    } else {
        // Two owned results contradict the single-worker invariant. Keep
        // the unaccounted owner rather than releasing unknown live credit.
        std::mem::forget(completed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource_intake::TaskLedger;
    use crate::resource_source::windows::tests::Fixture;

    fn read(cancelled: bool) {
        let fixture = Fixture::new();
        let source = fixture.source_arc();
        let selected = source
            .qualify_url(&fixture.url("views/editor.html"))
            .unwrap();
        let account = source.retained_credit_account();
        let before = account.load(Ordering::Acquire);
        let charge = usize::try_from(selected.size).unwrap();
        let credit = RetainedByteCredit::reserve(&account, charge).unwrap();
        let mut ledger = TaskLedger::new(ORIGINAL_WEBAPP_LIMITS);
        let ticket = ledger.begin(1).unwrap();
        let work = Work {
            ticket,
            view: WebviewId(1),
            source,
            selected,
            cancelled: Arc::new(AtomicBool::new(cancelled)),
            credit,
        };
        // This test joins its real Rust worker; it does not claim the native UI
        // thread-termination/wake contract, which needs the Windows host probe.
        let completed = thread::spawn(move || {
            read_work(
                work,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                None,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                &AtomicBool::new(false),
            )
        })
        .join()
        .unwrap();
        assert_eq!(completed.ticket, ticket);
        assert_eq!(account.load(Ordering::Acquire), before + charge);
        if cancelled {
            assert!(matches!(
                completed.result,
                Err(ResourceSourceError::Cancelled)
            ));
        } else {
            assert_eq!(completed.result.as_ref().unwrap().bytes.len(), charge);
        }
        drop(completed);
        assert_eq!(account.load(Ordering::Acquire), before);
    }

    #[test]
    fn verified_reader_result_retains_body_credit_until_ui_disposal() {
        read(false);
    }

    #[test]
    fn cancelled_reader_result_keeps_the_same_credit_until_disposal() {
        read(true);
    }
}
