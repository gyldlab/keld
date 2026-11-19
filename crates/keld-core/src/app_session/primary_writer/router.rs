//! Existing synchronous producers prepare outside, admit inside, and wait outside
//! the one canonical caller fence. Retained native producers use the same seam.

use super::super::{
    ActivePrimaryGeneration, PrimaryRouterHandle, append_app_cleanup, with_primary_caller_locked,
};
use super::{
    Arc, ControlMessage, CorrelationId, DeferredPrimaryWrite, FrameKind, HostAppError, Instant,
    PreparedFrame, PrimaryCallerContext, PrimaryWriter, TrySendError, WriteReceipt, WriterLink,
    WriterRefusal, app_detail,
};
use std::sync::atomic::Ordering;

impl PrimaryRouterHandle {
    /// Standalone historical component fixtures have no running writer owner.
    /// Every production Windows router requires the actual session-wide owner;
    /// absence is an error, never a raw-stream fallback in a product launch.
    #[cfg_attr(
        not(test),
        expect(
            clippy::unused_self,
            reason = "Only the historical test composition can omit the mandatory production writer."
        )
    )]
    pub(in crate::app_session) fn uses_primary_writer(&self) -> bool {
        #[cfg(test)]
        {
            self.primary_writer.is_some()
        }
        #[cfg(not(test))]
        {
            true
        }
    }

    pub(in crate::app_session) fn primary_writer_owner(
        &self,
    ) -> Result<&PrimaryWriter, HostAppError> {
        self.primary_writer.as_ref().ok_or_else(|| {
            app_detail(
                "primary writer owner",
                "the session-wide physical writer is unavailable",
            )
        })
    }

    pub(in crate::app_session) fn retire_generation_after_write_failure(
        &self,
        caller: &PrimaryCallerContext,
    ) -> Result<(), HostAppError> {
        let mut transition = self.shutdown.transition_guard();
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("primary writer failure", "current lock poisoned"))?;
        if current.as_ref().is_none_or(|active| {
            active
                .writer_link
                .as_ref()
                .is_none_or(|link| !link.matches(caller))
        }) {
            return Ok(());
        }
        self.retire_pending_ready_locked(&mut transition, caller);
        self.retire_current_generation_locked(
            &mut current,
            caller.attempt,
            "application call retired after primary writer failure",
            "failed exact primary link close",
        )
    }

    pub(in crate::app_session) fn service_primary_writer_progress(
        &self,
    ) -> Result<(), HostAppError> {
        self.primary_writer_owner()?
            .acknowledge_progress()
            .map_err(|error| error.into_host_error("primary writer progress"))?;
        self.service_dialog_writer_progress()
    }

    /// Capture only the already admitted exact stream while the original caller
    /// is current. This snapshot gives storage custody, not final effect authority.
    pub(in crate::app_session) fn primary_writer_target(
        &self,
        caller: &PrimaryCallerContext,
    ) -> Result<Option<Arc<WriterLink>>, HostAppError> {
        let mut target = None;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let active = current
                .as_ref()
                .ok_or_else(|| app_detail("primary writer target", "validated role disappeared"))?;
            let link = active
                .writer_link
                .as_ref()
                .filter(|link| link.matches(caller))
                .ok_or_else(|| {
                    app_detail(
                        "primary writer target",
                        "exact authenticated writer link is unavailable",
                    )
                })?;
            target = Some(Arc::clone(link));
            Ok(())
        })?;
        Ok(target)
    }

    pub(in crate::app_session) fn current_primary_writer_target(
        &self,
    ) -> Result<Option<Arc<WriterLink>>, HostAppError> {
        let _transition = self.shutdown.transition_guard();
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail("primary writer target", "current lock poisoned"))?;
        if !self.shutdown.is_running() {
            return Ok(None);
        }
        let Some(active) = current.as_ref() else {
            return Ok(None);
        };
        let link = active.writer_link.as_ref().ok_or_else(|| {
            app_detail("primary writer target", "exact writer link is unavailable")
        })?;
        if !active.identity.is_primary() || active.reader_stop.load(Ordering::Acquire) {
            return Ok(None);
        }
        Ok(Some(Arc::clone(link)))
    }

    /// Final admission never looks up a replacement target. A caller that lost
    /// the fence leaves its frame never written and retains no receipt.
    pub(in crate::app_session) fn admit_primary_frame(
        &self,
        caller: &PrimaryCallerContext,
        frame: PreparedFrame,
    ) -> Result<Option<WriteReceipt>, HostAppError> {
        let mut prepared = Some(frame);
        let mut receipt = None;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let active = current.as_ref().ok_or_else(|| {
                app_detail("primary writer admission", "validated role disappeared")
            })?;
            let frame = prepared.take().ok_or_else(|| {
                app_detail("primary writer admission", "prepared frame disappeared")
            })?;
            receipt = Some(admit_locked(active, frame)?);
            Ok(())
        })?;
        Ok(receipt)
    }

    /// This method is called only after every transition/current/native lock is
    /// released. The receipt owns charge until actual completion is inspected.
    pub(in crate::app_session) fn await_primary_write(
        &self,
        target: &Arc<WriterLink>,
        receipt: WriteReceipt,
        deadline: Instant,
        phase: &'static str,
    ) -> Result<(), HostAppError> {
        let observation: DeferredPrimaryWrite =
            self.observe_primary_write(target, receipt, deadline, phase);
        let (result, failure) = observation.into_parts();
        let recovery = failure.map_or(Ok(()), |failure| self.report_primary_write_failure(failure));
        match result {
            Ok(_) => recovery,
            Err(primary) => Err(append_app_cleanup(primary, [recovery])),
        }
    }

    pub(in crate::app_session) fn submit_primary_control(
        &self,
        caller: &PrimaryCallerContext,
        message: ControlMessage,
        deadline: Instant,
        phase: &'static str,
    ) -> Result<(), HostAppError> {
        let Some(target) = self.primary_writer_target(caller)? else {
            return Ok(());
        };
        let frame =
            self.prepare_primary_control(caller, Arc::clone(&target), message, deadline, phase)?;
        if let Some(receipt) = self.admit_primary_frame(caller, frame)? {
            self.await_primary_write(&target, receipt, deadline, phase)?;
        }
        Ok(())
    }

    pub(in crate::app_session) fn reply_primary_echo_owned(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        let Some(target) = self.primary_writer_target(caller)? else {
            return Ok(());
        };
        let reservation =
            self.reserve_primary_value(caller, deadline, "primary Echo reservation")?;
        let response = keld_ipc::echo::prepare_echo(payload)
            .map_err(|source| super::super::app_ipc("echo dispatch", &source))?;
        let frame = reservation.encode_value(
            Arc::clone(&target),
            FrameKind::Reply,
            keld_ipc::ECHO_CHANNEL,
            correlation,
            &response,
            keld_ipc::MAX_FRAME_LEN,
        )?;
        if let Some(receipt) = self.admit_primary_frame(caller, frame)? {
            self.await_primary_write(&target, receipt, deadline, "primary Echo reply")?;
        }
        Ok(())
    }
}

/// The enclosing owner already holds the actual current/retirement fence. This
/// narrow function moves the one charged frame only to its exact retained link.
pub(in crate::app_session) fn admit_locked(
    active: &ActivePrimaryGeneration,
    frame: PreparedFrame,
) -> Result<WriteReceipt, HostAppError> {
    let matches = active
        .writer_link
        .as_ref()
        .is_some_and(|target| frame.targets(target));
    if !matches {
        return Err(app_detail(
            "primary writer admission",
            "prepared frame does not retain this exact original link",
        ));
    }
    if Instant::now() >= frame.reservation.deadline {
        return Err(WriterRefusal::Deadline.into_host_error("primary writer admission"));
    }
    frame.try_send().map_err(|failure| {
        let refusal = match failure {
            TrySendError::Full(frame) => {
                drop(frame);
                WriterRefusal::Full
            }
            TrySendError::Disconnected(frame) => {
                drop(frame);
                WriterRefusal::Disconnected
            }
        };
        refusal.into_host_error("primary writer admission")
    })
}
