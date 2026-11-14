//! The same Windows application wake worker retains its actual delivery inputs.
//! Tao carries a unit hint; successful posting is never payload/native credit.
#![forbid(unsafe_code)]

use crate::WvError;
use crate::engine::AppWindowCommand;
use crate::renderer_bridge::RendererBridgeOutcome;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError, mpsc::Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tao::event_loop::EventLoopProxy;

use super::WindowsLoopEvent;
pub(super) mod delivery;
mod input;
mod post;
pub(super) mod service;
use input::BridgeInput;

/// Shared by the existing worker poll and the same Tao loop's idle health tick.
/// This observation bounds failed/unwound posting even when no unit wake arrives.
/// It is not a payload, consumer, callback, or native completion deadline.
pub(super) const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug)]
enum Failure {
    InputPoisoned,
    PostRejected,
    PostUnwound,
    WorkerUnwound,
    JoinUnwound,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    TestControlFailed,
}

/// One actual delivery; the run attaches it before any fallible native turn.
pub(super) enum AppWakeDelivery {
    Command(Option<AppWindowCommand>),
    Renderer(Option<RendererBridgeOutcome>),
}

enum Worker {
    Prepared,
    StartFailed,
    Running(JoinHandle<Result<(), Failure>>),
    Finished(Result<(), Failure>),
}

/// Attach this same owner before start. Original Receivers and its one current
/// delivery stay in the same cold shared input on spawn/post/unwind failure.
#[must_use]
pub(super) struct AppWakeBridge {
    stop: Arc<AtomicBool>,
    terminal_intent: Arc<AtomicBool>,
    input: Arc<Mutex<BridgeInput>>,
    worker: Worker,
}

impl AppWakeBridge {
    pub(super) fn prepare(
        commands: Receiver<AppWindowCommand>,
        outcomes: Option<Receiver<RendererBridgeOutcome>>,
        stop: Arc<AtomicBool>,
        terminal_intent: Arc<AtomicBool>,
    ) -> Self {
        Self {
            stop,
            terminal_intent,
            input: Arc::new(Mutex::new(BridgeInput::new(commands, outcomes))),
            worker: Worker::Prepared,
        }
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn install_outcome_hold(
        &mut self,
        hold: super::managed_test_control::outcome_hold::OutcomeHold,
    ) -> Result<(), WvError> {
        if !matches!(self.worker, Worker::Prepared) {
            return Err(error("outcome hold must be attached before worker start"));
        }
        let installed = self
            .input
            .lock()
            .map_err(|_| failure(Failure::InputPoisoned))?
            .install_outcome_hold(hold);
        if !installed {
            return Err(error("outcome hold was already attached"));
        }
        Ok(())
    }

    pub(super) fn start(&mut self, proxy: EventLoopProxy<WindowsLoopEvent>) -> Result<(), WvError> {
        if !matches!(self.worker, Worker::Prepared) {
            return Err(error("application wake worker already attempted"));
        }
        self.worker = Worker::StartFailed;
        let input = Arc::clone(&self.input);
        let stop = Arc::clone(&self.stop);
        let terminal = Arc::clone(&self.terminal_intent);
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let observing = super::t2_test_observation::enabled();
        let worker = thread::Builder::new()
            .name(String::from("keld-wv-windows-app-wake"))
            .spawn(move || {
                // Actual input/payload ownership remains in input, outside this
                // barrier. The native handoff closure can own only unit metadata.
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    input::run(
                        &input,
                        &stop,
                        &terminal,
                        &mut || {
                            proxy
                                .send_event(WindowsLoopEvent::BridgeWake)
                                .map_err(|_| ())
                        },
                        &mut |name| {
                            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                            if let Some(name) = name {
                                super::t2_test_observation::bridge_posted(observing, name);
                            }
                            #[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
                            let _ = name;
                        },
                    )
                }))
                .unwrap_or(Err(Failure::WorkerUnwound))
            })
            .map_err(|source| {
                WvError::EventLoop(format!("failed to start app wake bridge: {source}"))
            })?;
        self.worker = Worker::Running(worker);
        Ok(())
    }

    /// A late/duplicate unit is only a hint. Move the actual retained delivery
    /// directly into the run's empty current position before a native turn.
    pub(super) fn take_delivery(
        &self,
        target: &mut Option<AppWakeDelivery>,
    ) -> Result<bool, WvError> {
        if target.is_some() {
            return Ok(false);
        }
        let mut input = match self.input.try_lock() {
            Ok(input) => input,
            Err(TryLockError::WouldBlock) => return Ok(false),
            Err(TryLockError::Poisoned(_)) => return Err(failure(Failure::InputPoisoned)),
        };
        if let Some(fault) = input.failure() {
            return Err(failure(fault));
        }
        let Some(delivery) = input.take_current() else {
            return Ok(false);
        };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let observed = super::managed_test_control::queued_close_observation::Incoming::capture(
            &delivery,
            input.test_outcomes_held(),
        );
        *target = Some(delivery); // Infallible attachment before any further work.
        drop(input);
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(observed) = observed {
            observed.emit();
        }
        if let Worker::Running(worker) = &self.worker {
            worker.thread().unpark();
        }
        Ok(true)
    }

    pub(super) fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
        if let Worker::Running(worker) = &self.worker {
            worker.thread().unpark();
        }
    }

    /// The existing Tao loop merges this idle health observation with its
    /// current deadlines, outside native/managed borrows. One poll per100ms is
    /// the explicit idle cost; this cannot authorize dropping any delivery.
    pub(super) fn next_failure_observation(&self) -> Option<Instant> {
        matches!(self.worker, Worker::Running(_)).then(|| Instant::now() + POLL_INTERVAL)
    }

    /// Observe a completed worker without waiting on a live one. A failed unit
    /// post is visible here even if Tao never dequeued any corresponding wake.
    pub(super) fn poll_failure(&mut self) -> Result<(), WvError> {
        if let Worker::Running(worker) = &self.worker
            && worker.is_finished()
        {
            self.join_running();
        }
        if let Worker::Finished(Err(fault)) = self.worker {
            return Err(failure(fault));
        }
        match self.input.try_lock() {
            Ok(input) => input.failure().map_or(Ok(()), |fault| Err(failure(fault))),
            Err(TryLockError::WouldBlock) => Ok(()),
            Err(TryLockError::Poisoned(_)) => Err(failure(Failure::InputPoisoned)),
        }
    }

    /// Worker completion alone is not endpoint drain. Original Receivers stay
    /// attached here; Core's actual records and native drain decide release.
    pub(super) fn try_finish(&mut self) -> Result<bool, WvError> {
        self.poll_failure()?;
        match self.worker {
            Worker::Running(_) => return Ok(false),
            Worker::Prepared | Worker::StartFailed => {
                return Err(error("application wake worker did not start"));
            }
            Worker::Finished(_) => {}
        }
        let mut input = self
            .input
            .try_lock()
            .map_err(|_| error("application wake input custody is unavailable at final drain"))?;
        input.check_drained()?;
        Ok(true)
    }

    /// Preserve the existing standalone loop stop/join path. An installed
    /// renderer endpoint instead stops on the Tao cadence and calls `try_finish`
    /// only after this worker is observed finished.
    pub(super) fn finish_without_renderer(&mut self) -> Result<(), WvError> {
        self.request_stop();
        self.join_running();
        self.try_finish().map(|_| ())
    }

    fn join_running(&mut self) {
        let previous = std::mem::replace(&mut self.worker, Worker::StartFailed);
        self.worker = match previous {
            Worker::Running(worker) => {
                Worker::Finished(worker.join().unwrap_or(Err(Failure::JoinUnwound)))
            }
            other => other,
        };
    }
}

impl Drop for AppWakeBridge {
    fn drop(&mut self) {
        // Ordinary existing stop/join only. Root retains this entire owner on
        // every unproved path, before Drop; Drop supplies no release authority.
        self.request_stop();
        self.join_running();
    }
}

fn error(detail: &str) -> WvError {
    WvError::EventLoop(String::from(detail))
}
fn failure(fault: Failure) -> WvError {
    WvError::EventLoop(format!(
        "application wake delivery remains retained: {fault:?}"
    ))
}

#[cfg(test)]
mod tests;
// Preserve the original ownership controls under the actual worker owner.
#[cfg(test)]
#[path = "tests/app_wake_bridge.rs"]
mod ownership_tests;
