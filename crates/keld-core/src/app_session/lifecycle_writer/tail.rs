//! Fixed accepted-tail output retains the original link after reader admission stops.
//! These records grant no authority: every enqueue rechecks the existing cause/link fence.

use keld_ipc::ChannelId;
use std::sync::Arc;
use std::time::Instant;

use super::exact_target;
use super::retirement::finish_terminal_receipt;
use crate::app_session::primary_writer::{
    self, ControlMessage, PreparedFrame, WriteReceipt, WriterLink,
};
use crate::app_session::{
    APP_LINK_IO_DEADLINE, ActivePrimaryGeneration, BootstrapStream, CorrelationId, FrameKind,
    HOST_ANSWER_BUDGET, HostAppError, IpcError, LIFECYCLE_CHANNEL, Ordering, PrimaryRouterHandle,
    QuitDrainEnd, SESSION_CLI_LEASE_LOST, SESSION_HANDOVER, app_detail, collapse_app_results,
    is_forced_cause, read_quit_drain_frame_with_privileged_calls,
};

pub(super) struct AcceptedTail {
    pub(super) target: Arc<WriterLink>,
    pub(super) cause: u8,
}

impl AcceptedTail {
    pub(super) fn matches(
        &self,
        router: &PrimaryRouterHandle,
        active: &ActivePrimaryGeneration,
    ) -> bool {
        router.shutdown.cause() == self.cause
            && router.shutdown.reserved_cause.load(Ordering::Acquire) == self.cause
            && router.shutdown.tail_started.load(Ordering::Acquire)
            && exact_target(active, &self.target)
    }
}

impl PrimaryRouterHandle {
    pub(super) fn admit_tail_control(
        &self,
        tail: &AcceptedTail,
        frame: PreparedFrame,
    ) -> Result<Option<WriteReceipt>, HostAppError> {
        let _transition = self.shutdown.transition_guard();
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail("accepted tail admission", "current lock poisoned"))?;
        let Some(active) = current.as_ref().filter(|active| tail.matches(self, active)) else {
            return Ok(None);
        };
        primary_writer::admit_locked(active, frame).map(Some)
    }

    fn write_quit_drained(
        &self,
        tail: &AcceptedTail,
        channel: ChannelId,
        correlation: CorrelationId,
        deadline: Instant,
    ) -> Result<bool, HostAppError> {
        let reservation =
            self.reserve_primary_tail_control(deadline, "accepted Quit drain reservation")?;
        let frame = ControlMessage::QuitDrained {
            channel,
            correlation,
        }
        .encode(reservation, Arc::clone(&tail.target))?;
        let Some(receipt) = self.admit_tail_control(tail, frame)? else {
            return Ok(false);
        };
        finish_terminal_receipt(receipt, "accepted Quit drain write")?
            .into_result("accepted Quit drain write")?;
        Ok(true)
    }

    pub(super) fn answer_calls_after_quit_with_writer(
        &self,
        tail: &AcceptedTail,
        reader: &mut BootstrapStream,
    ) {
        let started = Instant::now();
        let (Some(idle_deadline), Some(hard_deadline)) = (
            started.checked_add(HOST_ANSWER_BUDGET),
            started.checked_add(APP_LINK_IO_DEADLINE),
        ) else {
            return;
        };
        #[cfg(test)]
        let idle_deadline = self
            .stall_quit_drain_for_test(started)
            .unwrap_or(idle_deadline);
        let end = self.drain_quit_calls_with_writer(tail, reader, idle_deadline, hard_deadline);
        #[cfg(test)]
        self.report_quit_drain_end(end);
        #[cfg(not(test))]
        let _ = end;
    }

    fn drain_quit_calls_with_writer(
        &self,
        tail: &AcceptedTail,
        reader: &mut BootstrapStream,
        idle_deadline: Instant,
        hard_deadline: Instant,
    ) -> QuitDrainEnd {
        let privileged = self.privileged_calls();
        loop {
            let header = match read_quit_drain_frame_with_privileged_calls(
                reader,
                privileged.as_slice(),
                idle_deadline,
                hard_deadline,
            ) {
                Ok(Some((header, _payload))) => header,
                Ok(None) => return QuitDrainEnd::IdleBackstop,
                Err(IpcError::Io(_)) => return QuitDrainEnd::PeerClosed,
                Err(IpcError::Timeout) => return QuitDrainEnd::Deadline,
                Err(_) => return QuitDrainEnd::NotAdmitted,
            };
            let declares_err = header.channel() == LIFECYCLE_CHANNEL
                || header.channel() == keld_ipc::channel_table::WINDOW.id()
                || privileged
                    .as_slice()
                    .iter()
                    .any(|entry| entry.id() == header.channel());
            if header.kind() != FrameKind::Call || !declares_err {
                continue;
            }
            let deadline = hard_deadline.min(Instant::now() + HOST_ANSWER_BUDGET);
            match self.write_quit_drained(tail, header.channel(), header.corr(), deadline) {
                Ok(true) => {}
                Ok(false) => return QuitDrainEnd::GenerationGone,
                Err(_) => return QuitDrainEnd::AnswerLost,
            }
        }
    }

    pub(in crate::app_session) fn cli_lease_lost_with_writer(&self) -> Result<(), HostAppError> {
        let reserved = self.shutdown.reserved_cause.load(Ordering::Acquire);
        if !is_forced_cause(reserved) || self.shutdown.tail_started.load(Ordering::Acquire) {
            return Ok(());
        }
        let (detail, phase) = if reserved == SESSION_HANDOVER {
            (
                "application call retired by the profile handover",
                "profile handover",
            )
        } else {
            debug_assert_eq!(reserved, SESSION_CLI_LEASE_LOST);
            (
                "application call retired by CLI lease loss",
                "CLI lease loss",
            )
        };
        self.guardian.prepare_accepted_shutdown()?;
        let captured = {
            let _transition = self.shutdown.transition_guard();
            if !self.shutdown.begin_tail() {
                return Ok(());
            }
            (|| {
                let target = self
                    .current
                    .lock()
                    .map_err(|_| app_detail("forced primary tail", "current lock poisoned"))?
                    .as_ref()
                    .and_then(|active| active.writer_link.as_ref().map(Arc::clone));
                let ending = self.accept_forced_tail_guarded(reserved);
                Ok::<_, HostAppError>((
                    ending,
                    target.map(|target| AcceptedTail {
                        target,
                        cause: reserved,
                    }),
                ))
            })()
        };
        let _run = self.shutdown.tail_run();
        // A fallible capture cannot bypass the tail completion guard.
        let (ending, tail) = captured?;
        let fs = self.quiesce_and_drain_fs();
        let pending = self.fail_any_pending_echo(detail);
        let retired = tail
            .as_ref()
            .map_or(Ok(()), |tail| self.retire_accepted_tail_with_writer(tail));
        let finished = self.finish_tail(phase);
        collapse_app_results([ending, fs, pending, retired, finished])
    }
}
