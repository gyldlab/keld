//! Scalar, thread-affine native proof support. It cannot change an operation result.
use super::{AFTER_WALK_TEST_HOOK, FS_TEST_COUNTERS, FsTestCounters};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::Duration;

/// Actual current-thread native invocation/I/O counters and sticky evidence failure.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Snapshot {
    /// Entries inside the real `FsBroker::handle_request` function.
    pub request_entries: usize,
    /// Real normal returns, including typed failures; unwinding is not a return.
    pub request_returns: usize,
    /// Entries in the existing retained traversal owner.
    pub walk_entries: usize,
    /// Existing target-open attempts.
    pub target_opens: usize,
    /// Existing create attempts.
    pub create_calls: usize,
    /// Existing truncate attempts.
    pub truncate_calls: usize,
    /// Existing content-read attempts.
    pub content_reads: usize,
    /// Existing content-write attempts.
    pub content_writes: usize,
    /// Overflow, disconnection, nesting, hold failure or unwind invalidated evidence.
    pub fixture_fault: bool,
}

/// A one-shot bounded wait at the existing retained after-walk/pre-effect seam.
#[derive(Debug)]
pub struct Hold {
    held_sink: SyncSender<Snapshot>,
    release: Receiver<()>,
    timeout: Duration,
}

impl Hold {
    /// Constructs a fixture wait of more than zero and at most ten seconds.
    /// This bound does not renew or replace the native operation budget.
    #[must_use]
    pub fn new(
        held_sink: SyncSender<Snapshot>,
        release: Receiver<()>,
        timeout: Duration,
    ) -> Option<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(10) {
            return None;
        }
        Some(Self {
            held_sink,
            release,
            timeout,
        })
    }
}

struct State {
    entries: usize,
    returns: usize,
    fault: bool,
    returned: SyncSender<Snapshot>,
    final_sink: SyncSender<Snapshot>,
}

std::thread_local! {
    static ACTIVE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Opaque scope retained and dropped on the native worker thread (!Send, !Sync).
/// Drop publishes the final snapshot before restoring prior private test state.
pub struct ObservationGuard {
    previous_counters: FsTestCounters,
    previous_hook: Option<Box<dyn FnOnce()>>,
    thread_affine: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for ObservationGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ObservationGuard")
            .finish_non_exhaustive()
    }
}

fn snapshot(state: &State) -> Snapshot {
    let io = FS_TEST_COUNTERS.with(std::cell::Cell::get);
    Snapshot {
        request_entries: state.entries,
        request_returns: state.returns,
        walk_entries: io.walk_entries,
        target_opens: io.target_opens,
        create_calls: io.create_calls,
        truncate_calls: io.truncate_calls,
        content_reads: io.content_reads,
        content_writes: io.content_writes,
        fixture_fault: state.fault,
    }
}

fn publish(state: &mut State, sink: &SyncSender<Snapshot>) {
    if sink.try_send(snapshot(state)).is_err() {
        state.fault = true;
    }
}

/// Installs only on the current thread, rejecting nesting or an existing private hook.
/// Every `None`, missing final fence or fault snapshot is invalid fixture evidence.
/// Sinks are bounded and nonblocking; neither their failure nor a hold changes results.
#[must_use]
pub fn install_current_thread(
    hold: Option<Hold>,
    native_return_sink: SyncSender<Snapshot>,
    final_snapshot_sink: SyncSender<Snapshot>,
) -> Option<ObservationGuard> {
    let nested = ACTIVE.with(|slot| {
        let mut active = slot.borrow_mut();
        if let Some(state) = active.as_mut() {
            state.fault = true;
            true
        } else {
            false
        }
    });
    if nested || AFTER_WALK_TEST_HOOK.with(|slot| slot.borrow().is_some()) {
        return None;
    }
    let previous_counters = FS_TEST_COUNTERS.with(|slot| slot.replace(FsTestCounters::new()));
    ACTIVE.with(|slot| {
        *slot.borrow_mut() = Some(State {
            entries: 0,
            returns: 0,
            fault: false,
            returned: native_return_sink,
            final_sink: final_snapshot_sink,
        });
    });
    let hook: Option<Box<dyn FnOnce()>> = hold.map(|hold| {
        Box::new(move || {
            ACTIVE.with(|slot| {
                if let Some(state) = slot.borrow_mut().as_mut() {
                    publish(state, &hold.held_sink);
                }
            });
            if hold.release.recv_timeout(hold.timeout).is_err() {
                ACTIVE.with(|slot| {
                    if let Some(state) = slot.borrow_mut().as_mut() {
                        state.fault = true;
                    }
                });
            }
        }) as Box<dyn FnOnce()>
    });
    let previous_hook = AFTER_WALK_TEST_HOOK.with(|slot| slot.replace(hook));
    Some(ObservationGuard {
        previous_counters,
        previous_hook,
        thread_affine: PhantomData,
    })
}

pub(super) fn request_entered() {
    ACTIVE.with(|slot| {
        if let Some(state) = slot.borrow_mut().as_mut() {
            match state.entries.checked_add(1) {
                Some(count) => state.entries = count,
                None => state.fault = true,
            }
        }
    });
}

pub(super) fn request_returned() {
    ACTIVE.with(|slot| {
        if let Some(state) = slot.borrow_mut().as_mut() {
            match state.returns.checked_add(1) {
                Some(count) => state.returns = count,
                None => state.fault = true,
            }
            let sink = state.returned.clone();
            publish(state, &sink);
        }
    });
}

impl Drop for ObservationGuard {
    fn drop(&mut self) {
        ACTIVE.with(|slot| {
            if let Some(mut state) = slot.borrow_mut().take() {
                state.fault |= std::thread::panicking();
                let sink = state.final_sink.clone();
                publish(&mut state, &sink);
            }
        });
        AFTER_WALK_TEST_HOOK.with(|slot| {
            slot.replace(self.previous_hook.take());
        });
        FS_TEST_COUNTERS.with(|slot| slot.set(self.previous_counters));
    }
}

#[cfg(test)]
#[path = "acceptance_observer_tests.rs"]
mod tests;
