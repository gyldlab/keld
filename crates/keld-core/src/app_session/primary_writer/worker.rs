//! The sole worker owns physical writes; Core guards end before pipe entry.

use super::super::{BootstrapStream, PrimaryRouterHandle, app_io};
use super::{
    Arc, HostAppError, Instant, Mutex, Packet, Position, PrimaryCallerContext, PrimaryWriter,
    Shared, WriteCompletion, WriterRefusal, app_detail, failure,
};
use keld_ipc::AppLinkDeadlines as _;
use std::io::{self, Write};
use std::thread::{self, JoinHandle};

pub(in crate::app_session) struct WriterLink {
    stream: Mutex<BootstrapStream>,
    closer: BootstrapStream,
    caller: PrimaryCallerContext,
    #[cfg(test)]
    write_deadline_observer:
        Mutex<Option<std::sync::mpsc::SyncSender<Option<std::time::Duration>>>>,
}
impl WriterLink {
    pub(in crate::app_session) fn new(
        stream: BootstrapStream,
        caller: PrimaryCallerContext,
    ) -> Result<Arc<Self>, HostAppError> {
        let closer = stream
            .try_clone()
            .map_err(|source| app_io("primary writer close handle", &source))?;
        Ok(Arc::new(Self {
            stream: Mutex::new(stream),
            closer,
            caller,
            #[cfg(test)]
            write_deadline_observer: Mutex::new(None),
        }))
    }

    #[cfg(test)]
    pub(in crate::app_session) fn observe_next_write_deadline_for_test(
        &self,
        observer: std::sync::mpsc::SyncSender<Option<std::time::Duration>>,
    ) {
        *self
            .write_deadline_observer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(observer);
    }

    #[cfg(test)]
    fn observe_configured_write_deadline(&self, stream: &BootstrapStream) {
        let observer = self
            .write_deadline_observer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(observer) = observer {
            let _ = observer.try_send(stream.app_link_write_deadline().ok().flatten());
        }
    }

    pub(in crate::app_session) fn matches(&self, caller: &PrimaryCallerContext) -> bool {
        self.caller.attempt == caller.attempt
            && self.caller.identity == caller.identity
            && Arc::ptr_eq(&self.caller.reader_stop, &caller.reader_stop)
    }

    pub(in crate::app_session) fn caller(&self) -> &PrimaryCallerContext {
        &self.caller
    }

    pub(in crate::app_session) fn shutdown(&self) -> io::Result<()> {
        // This separate retained close handle never waits for the physical-write
        // mutex, so canonical role retirement can interrupt a blocked pipe write.
        self.closer.shutdown_app_link()
    }
}

pub(in crate::app_session) struct PrimaryWriterWorker {
    writer: PrimaryWriter,
    join: Option<JoinHandle<Result<(), HostAppError>>>,
}
impl PrimaryWriterWorker {
    pub(in crate::app_session) fn start(
        mut router: PrimaryRouterHandle,
    ) -> Result<(PrimaryWriter, Self), HostAppError> {
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        let physical_hold = super::physical_value_hold::PhysicalValueHold::from_env()?;
        let writer = PrimaryWriter::new(router.window_commands.clone());
        router.primary_writer = Some(writer.clone());
        if let Some(compat) = &router.compat {
            compat
                .lock()
                .map_err(|_| app_detail("primary writer bind", "compat session lock poisoned"))?
                .bind_primary_writer(writer.clone())?;
        }
        let shared = Arc::clone(&writer.shared);
        let join = thread::Builder::new()
            .name("keld-core-primary-writer".to_owned())
            .spawn(move || {
                // This outer scope outlives every packet/native call in run,
                // including unwinding. Never-written cleanup must also happen
                // when the worker panics, before a quiescence waiter can finish.
                let _ended = WorkerEnded { shared: &shared };
                run(
                    &shared,
                    &router,
                    #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
                    physical_hold,
                )
            })
            .map_err(|source| app_io("primary writer worker", &source))?;
        Ok((
            writer.clone(),
            Self {
                writer,
                join: Some(join),
            },
        ))
    }

    pub(in crate::app_session) fn stop_and_join(&mut self) -> Result<(), HostAppError> {
        self.writer.close();
        self.join.take().map_or(Ok(()), |join| {
            join.join()
                .map_err(|_| app_detail("primary writer worker", "worker panicked"))?
        })
    }
}
impl Drop for PrimaryWriterWorker {
    fn drop(&mut self) {
        // This owning worker lives in PrimaryRouter, whose teardown releases all
        // Core fences before joining I/O owners. Handles and receipts cannot join.
        let _ = self.stop_and_join();
    }
}

/// Drop performs only fixed-state cleanup and the existing coalesced wake. Packet
/// backing and a live native call cannot outlive this outer thread-scope guard.
struct WorkerEnded<'a> {
    shared: &'a Shared,
}
impl Drop for WorkerEnded<'_> {
    fn drop(&mut self) {
        let wake = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .worker_ended();
        self.shared.changed.notify_all();
        self.shared.wake(wake);
    }
}

struct AbsoluteDeadline<'a> {
    stream: &'a mut BootstrapStream,
    deadline: Instant,
    #[cfg(test)]
    target: &'a WriterLink,
}
impl AbsoluteDeadline<'_> {
    fn set_remaining(&self) -> io::Result<()> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "original primary write deadline expired",
                )
            })?;
        self.stream.set_app_link_write_deadline(Some(remaining))?;
        #[cfg(test)]
        self.target.observe_configured_write_deadline(self.stream);
        Ok(())
    }
}
impl Write for AbsoluteDeadline<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // Canonical write_all may issue several native writes. Every one uses
        // the remaining original budget, never a reset full per-call duration.
        self.set_remaining()?;
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.set_remaining()?;
        self.stream.flush()
    }
}

fn write(packet: &Packet) -> (WriteCompletion, Instant) {
    let Ok(mut stream) = packet.target.stream.lock() else {
        return (WriteCompletion::OutcomeUnknown, Instant::now());
    };
    let mut writer = AbsoluteDeadline {
        stream: &mut stream,
        deadline: packet.deadline,
        #[cfg(test)]
        target: &packet.target,
    };
    let returned = keld_ipc::link::write_frame(
        &mut writer,
        packet.header.kind,
        packet.header.flags,
        packet.header.channel,
        packet.header.corr,
        &packet.payload,
    );
    // Sample before even releasing the stream guard; producer scheduling and
    // canonical retirement cleanup cannot move this actual completion time.
    let completed_at = Instant::now();
    let outcome = if returned.is_ok() {
        WriteCompletion::Written
    } else {
        WriteCompletion::OutcomeUnknown
    };
    (outcome, completed_at)
}

fn next(shared: &Shared) -> Result<Option<(Position, Packet)>, HostAppError> {
    let mut state = shared
        .state
        .lock()
        .map_err(|_| WriterRefusal::Poisoned.into_host_error("primary writer dequeue"))?;
    loop {
        if let Some(packet) = state
            .dequeue()
            .map_err(|error| error.into_host_error("primary writer dequeue"))?
        {
            return Ok(Some(packet));
        }
        if state.closed {
            return Ok(None);
        }
        state = shared
            .changed
            .wait(state)
            .map_err(|_| WriterRefusal::Poisoned.into_host_error("primary writer idle"))?;
    }
}

fn run(
    shared: &Arc<Shared>,
    router: &PrimaryRouterHandle,
    #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
    mut physical_hold: Option<super::physical_value_hold::PhysicalValueHold>,
) -> Result<(), HostAppError> {
    while let Some((position, packet)) = next(shared)? {
        #[cfg(test)]
        {
            let hook = shared
                .state
                .lock()
                .ok()
                .and_then(|mut state| state.after_dequeue.take());
            if let Some(hook) = hook {
                hook.arrived
                    .send(())
                    .map_err(|_| app_detail("writer dequeue fixture", "observer disappeared"))?;
                hook.resume
                    .lock()
                    .map_err(|_| app_detail("writer dequeue fixture", "hook poisoned"))?
                    .recv()
                    .map_err(|_| app_detail("writer dequeue fixture", "release disappeared"))?;
                assert!(
                    !hook.panic_after_resume,
                    "actual primary writer unwind control"
                );
            }
        }
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        let held_packet = match physical_hold.as_mut() {
            Some(control) => control.hold(position, &packet, shared)?,
            None => None,
        };
        // A producer may have enqueued before committing its existing terminal
        // record. Crossing the exact canonical fence orders that commit first.
        // Do NOT recheck current role/live status here: this packet already won
        // admission for its fixed A link and cannot be redirected or discarded.
        {
            let _transition = router.shutdown.transition_guard();
            let _current = router.current.lock().map_err(|_| {
                app_detail("primary writer publication fence", "current owner poisoned")
            })?;
        }
        let (outcome, completed_at) = write(&packet);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        if let Some(observed) = held_packet {
            observed.observe("packet-returned", shared, Some(outcome), Some(&packet));
        }
        let target = Arc::clone(&packet.target);
        // Drop the sole payload before a slot could become free. Releasing at
        // dequeue, timeout, role replacement, or before this drop over-admits.
        drop(packet);
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        if let Some(observed) = held_packet {
            observed.observe("packet-dropped", shared, Some(outcome), None);
        }
        let retirement = if outcome == WriteCompletion::OutcomeUnknown {
            // This is the actual canonical owner, not a copied stop flag. The
            // Runtime recovery request belongs to the awaiting producer/progress
            // service; the I/O worker must never await a successor's Ready write.
            super::super::collapse_app_results([
                target
                    .shutdown()
                    .map_err(|source| app_io("failed original writer link close", &source)),
                router.record_primary_write_failure(
                    &failure::PrimaryWriteFailure::returned_unknown(shared, Arc::clone(&target)),
                ),
            ])
        } else {
            Ok(())
        };
        let wake = shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned.into_host_error("primary writer completion"))?
            .complete_at(position, outcome, completed_at);
        shared.changed.notify_all();
        shared.wake(wake);
        retirement?;
    }
    Ok(())
}

#[cfg(test)]
pub(in crate::app_session) struct DequeueHook {
    pub(in crate::app_session) arrived: std::sync::mpsc::SyncSender<()>,
    pub(in crate::app_session) resume: Mutex<std::sync::mpsc::Receiver<()>>,
    pub(in crate::app_session) panic_after_resume: bool,
}
