//! One Host-wide bounded reader. Only safe file/source data crosses this thread.
//! The existing native wait owner proves actual thread/TLS termination on the UI.

use super::{Source, failure};
use crate::resource_intake::{ORIGINAL_WEBAPP_LIMITS, RetainedByteCredit, TaskTicket};
use crate::resource_source::{ResourceSourceError, SelectedRead, VerifiedBytes};
use crate::webview2::{BrowserExitWake, WindowsLoopEvent};
use crate::{WebviewId, WvError};
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

fn read_work(work: Work) -> Completed {
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
}

impl Reader {
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
        }
    }

    fn start(
        &mut self,
        proxy: &EventLoopProxy<WindowsLoopEvent>,
        view: WebviewId,
    ) -> Result<(), WvError> {
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
        let worker = thread::Builder::new()
            .name(String::from("keld-original-resource"))
            .spawn(move || {
                while let Ok(work) = receiver.recv() {
                    let view = work.view;
                    let response = read_work(work);
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
                    if wake
                        .send_event(WindowsLoopEvent::OriginalWake(view))
                        .is_err()
                    {
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
                BrowserExitWake::register_for_resource_reader(handle, proxy.clone(), view)
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
        let Some(work) = staged.as_ref() else {
            return Err(failure("resource submission has no reserved work"));
        };
        self.start(proxy, work.view)?;
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

    pub(super) fn retain_unproved(&mut self, completed: Completed) {
        self.unproved = true;
        self.shutdown = true;
        self.submit.take();
        // Restore the one result to its existing slot; never turn an internal
        // accounting disagreement into a fictitious buffer-release receipt.
        let mut slot = self
            .result
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
        let Some(wait) = &self.wait else {
            return Ok(());
        };
        if !wait.resource_reader_signaled()? {
            return Ok(());
        }
        // The same kernel thread is signaled, including TLS destruction. The
        // pool callback has no source/credit. Retire it before consuming JoinHandle.
        let Some(wait) = self.wait.take() else {
            self.unproved = true;
            return Err(failure("signaled resource thread lost its retained wait"));
        };
        wait.retire().inspect_err(|_| self.unproved = true)?;
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
        let completed = thread::spawn(move || read_work(work)).join().unwrap();
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
