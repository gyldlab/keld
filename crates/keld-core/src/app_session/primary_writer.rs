//! One session-wide physical primary writer (approved GH1185 §4.12).
//! Positions remain charged while encoding, queued, writing, and awaiting receipt.
//! The only payload FIFO is the fixed five-entry array in `queue`.

mod capacity;
mod failure;
mod filesystem;
mod frame;
mod observation;
mod progress;
mod queue;
mod receipt;
mod retained;
mod router;
pub(in crate::app_session) use observation::DeferredPrimaryWrite;
#[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
mod physical_value_hold;
mod worker;
#[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
pub(super) const TEST_PRIMARY_VALUE_HOLD_ENV: &str = "KELD_TEST_PRIMARY_VALUE_HOLD";

pub(super) use failure::{
    FailureDisposition, PrimaryReadyFailure, PrimaryWriteFailure, PrimaryWriterFailure,
    RetiredWriterFailure,
};
pub(super) use frame::{ControlMessage, ValueFreeTerminal};
pub(super) use receipt::{WriteObservation, WriteReceipt};
pub(super) use retained::RetainedWrite;
pub(super) use router::admit_locked;
pub(super) use worker::{PrimaryWriterWorker, WriterLink};

use std::sync::mpsc::{Sender, TrySendError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use keld_ipc::{ChannelId, CorrelationId, FrameHeader, FrameKind};
use keld_wv::AppWindowCommand;

use super::{HostAppError, PrimaryCallerContext, SessionShutdownState, app_detail};
use queue::{Phase, State};

pub(super) const POSITIONS: usize = 5;
pub(super) const CONTROL_BYTES: usize = keld_ipc::compat_control::COMPAT_CONTROL_FRAME_BYTES;
pub(super) const RECORD_METADATA_BYTES: usize = 1024;
pub(super) const WRITER_BUCKET_BYTES: usize =
    keld_ipc::MAX_FRAME_LEN + 4 * CONTROL_BYTES + POSITIONS * RECORD_METADATA_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Value,
    Control,
}
impl Class {
    const fn limit(self) -> usize {
        match self {
            Self::Value => keld_ipc::MAX_FRAME_LEN,
            Self::Control => CONTROL_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WriterRefusal {
    Full,
    Disconnected,
    Retired,
    Deadline,
    Exhausted,
    InvalidFrame,
    Poisoned,
}
impl WriterRefusal {
    pub(super) fn into_host_error(self, phase: &'static str) -> HostAppError {
        app_detail(
            phase,
            match self {
                Self::Full => "the bounded primary writer has no eligible position",
                Self::Disconnected => "the primary writer owner is disconnected",
                Self::Retired => "the original primary caller has retired",
                Self::Deadline => "the original primary write deadline expired",
                Self::Exhausted => "the primary writer position identity is exhausted",
                Self::InvalidFrame => "the frame does not fit its reserved primary writer class",
                Self::Poisoned => "the primary writer state is poisoned",
            },
        )
    }
}

/// Completion describes the actual original physical write, never consumer work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WriteCompletion {
    Written,
    OutcomeUnknown,
}
impl WriteCompletion {
    pub(super) fn into_result(self, phase: &'static str) -> Result<(), HostAppError> {
        match self {
            Self::Written => Ok(()),
            Self::OutcomeUnknown => Err(app_detail(
                phase,
                "the original primary frame did not complete; its effect is unknown and must not be replayed",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Position {
    index: usize,
    serial: u64,
}

struct Packet {
    header: FrameHeader,
    payload: Box<[u8]>,
    target: Arc<WriterLink>,
    deadline: Instant,
}

/// The handle has no stream. Only its worker can enter a physical pipe write.
#[derive(Clone)]
pub(super) struct PrimaryWriter {
    shared: Arc<Shared>,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    progress: Sender<AppWindowCommand>,
}

pub(super) struct Reservation {
    shared: Arc<Shared>,
    position: Option<Position>,
    class: Class,
    deadline: Instant,
}

/// Owns the only encoded allocation before its final admission. No Clone exists.
pub(super) struct PreparedFrame {
    reservation: Reservation,
    packet: Option<Packet>,
}

impl PrimaryWriter {
    #[cfg(test)]
    pub(in crate::app_session) fn hold_next_dequeue_for_test(
        &self,
    ) -> (
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::SyncSender<()>,
    ) {
        let (arrived, observed) = std::sync::mpsc::sync_channel(1);
        let (resume, proceed) = std::sync::mpsc::sync_channel(1);
        self.shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .after_dequeue = Some(Arc::new(worker::DequeueHook {
            arrived,
            resume: Mutex::new(proceed),
            panic_after_resume: false,
        }));
        (observed, resume)
    }

    #[cfg(test)]
    pub(in crate::app_session) fn fail_next_send_for_test(&self, failure: WriterRefusal) {
        self.shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .fail_next_send(failure);
    }

    #[cfg(test)]
    pub(in crate::app_session) fn observe_next_capacity_wait_for_test(
        &self,
        observed: std::sync::mpsc::SyncSender<()>,
    ) {
        self.shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .capacity_wait_observer = Some(observed);
    }

    pub(super) fn new(progress: Sender<AppWindowCommand>) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State::new()),
                changed: Condvar::new(),
                progress,
            }),
        }
    }

    /// Reserve before counting, encoding, or copying any framework frame bytes.
    /// Callers must not hold transition/current/Registry while this may wait.
    #[cfg(test)]
    pub(super) fn reserve_value(
        &self,
        caller: &PrimaryCallerContext,
        shutdown: &SessionShutdownState,
        deadline: Instant,
    ) -> Result<Reservation, WriterRefusal> {
        self.reserve(Class::Value, Some((caller, shutdown)), deadline)
    }

    /// Nonblocking counterpart for a settled native record's final admission.
    pub(super) fn try_reserve_value(
        &self,
        deadline: Instant,
    ) -> Result<Reservation, WriterRefusal> {
        self.reserve(Class::Value, None, deadline)
    }

    /// Nonblocking reserved-control admission for one already settled record.
    pub(super) fn try_reserve_control(
        &self,
        deadline: Instant,
    ) -> Result<Reservation, WriterRefusal> {
        self.reserve(Class::Control, None, deadline)
    }

    fn reserve(
        &self,
        class: Class,
        wait: Option<(&PrimaryCallerContext, &SessionShutdownState)>,
        deadline: Instant,
    ) -> Result<Reservation, WriterRefusal> {
        self.reserve_waiting(class, wait.is_some(), deadline, || {
            wait.is_some_and(|(caller, shutdown)| {
                caller
                    .reader_stop
                    .load(std::sync::atomic::Ordering::Acquire)
                    || !shutdown.is_running()
            })
        })
    }

    fn reserve_waiting(
        &self,
        class: Class,
        wait: bool,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Reservation, WriterRefusal> {
        self.reserve_waiting_serviced(class, wait, deadline, cancelled, || Ok(()))
    }

    /// Existing source owners may release only already completed receipts here.
    /// The callback has no writer/Core/source guard and starts no downstream work.
    fn reserve_waiting_serviced(
        &self,
        class: Class,
        wait: bool,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut service_completed: impl FnMut() -> Result<(), WriterRefusal>,
    ) -> Result<Reservation, WriterRefusal> {
        let mut serviced: Option<[Option<Position>; POSITIONS]> = None;
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| WriterRefusal::Poisoned)?;
        loop {
            if Instant::now() >= deadline {
                return Err(WriterRefusal::Deadline);
            }
            if cancelled() {
                return Err(WriterRefusal::Retired);
            }
            match state.reserve(class) {
                Ok(position) => {
                    return Ok(Reservation {
                        shared: Arc::clone(&self.shared),
                        position: Some(position),
                        class,
                        deadline,
                    });
                }
                Err(WriterRefusal::Full) if wait => {
                    let completed = state.completed_positions();
                    if serviced != Some(completed) {
                        drop(state);
                        service_completed()?;
                        state = self
                            .shared
                            .state
                            .lock()
                            .map_err(|_| WriterRefusal::Poisoned)?;
                        serviced = Some(completed);
                        // Retry the actual reservation first. If completion won
                        // between service and this lock, the existing serial/phase
                        // snapshot changes and forces another bounded service.
                        continue;
                    }
                    serviced = None;
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    #[cfg(test)]
                    if let Some(observed) = state.capacity_wait_observer.take() {
                        let _ = observed.try_send(());
                    }
                    let (next, _) = self
                        .shared
                        .changed
                        .wait_timeout(state, remaining)
                        .map_err(|_| WriterRefusal::Poisoned)?;
                    state = next;
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub(super) fn close(&self) {
        let targets = {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.closed = true;
            std::array::from_fn::<_, POSITIONS, _>(|index| state.slots[index].target.clone())
        };
        self.shared.changed.notify_all();
        for target in targets.into_iter().flatten() {
            let _ = target.shutdown();
        }
    }
}

impl Shared {
    fn release(&self, position: Position, reservation: bool) {
        let wake = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.release(position, reservation)
        };
        self.changed.notify_all();
        self.wake(wake);
    }
}

impl PreparedFrame {
    /// The enclosing Core fence must first validate the original caller/record
    /// and exact target. This operation moves the Box and has no fallible work
    /// after queue insertion; the caller commits its terminal state immediately.
    #[expect(
        clippy::manual_let_else,
        reason = "Preserve the poisoned-lock match scope while returning the same charged frame."
    )]
    pub(super) fn try_send(mut self) -> Result<WriteReceipt, TrySendError<Self>> {
        let shared = Arc::clone(&self.reservation.shared);
        let mut state = match shared.state.lock() {
            Ok(state) => state,
            Err(_) => return Err(TrySendError::Disconnected(self)),
        };
        let Some(position) = self.reservation.position else {
            return Err(TrySendError::Disconnected(self));
        };
        if let Err(error) = state.admit(position) {
            return Err(match error {
                WriterRefusal::Full => TrySendError::Full(self),
                _ => TrySendError::Disconnected(self),
            });
        }
        // All checks precede this move. The fixed slot and receipt are already
        // charged; no allocation, clone, validation or user callback follows.
        state.slots[position.index].packet = self.packet.take();
        state.slots[position.index].target = state.slots[position.index]
            .packet
            .as_ref()
            .map(|packet| Arc::clone(&packet.target));
        state.slots[position.index].deadline = state.slots[position.index]
            .packet
            .as_ref()
            .map(|packet| packet.deadline);
        state.slots[position.index].phase = Phase::Queued;
        state.enqueue(position.index);
        self.reservation.position = None;
        drop(state);
        shared.changed.notify_one();
        Ok(WriteReceipt {
            shared,
            position,
            deadline: self.reservation.deadline,
        })
    }

    pub(super) fn targets(&self, target: &Arc<WriterLink>) -> bool {
        self.packet
            .as_ref()
            .is_some_and(|packet| Arc::ptr_eq(&packet.target, target))
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(position) = self.position.take() {
            self.shared.release(position, true);
        }
    }
}

#[cfg(test)]
mod tests;
