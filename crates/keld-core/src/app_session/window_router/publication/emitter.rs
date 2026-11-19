//! Reserve/encode, fence/enqueue and observe the actual original write receipt.

use super::super::super::primary_writer::{
    ControlMessage, PreparedFrame, WriterLink, admit_locked,
};
use super::super::super::window_registry::{ClosingCleanup, Teardown, fixed_event_window};
use super::super::request::WindowRefusal;
use super::super::{
    ActivePrimaryGeneration, CallError, CorrelationId, FrameKind, HostAppError, LifecycleEvent,
    OwnedWindowEvent, PrimaryCallerContext, PrimaryRouterHandle, WINDOW_PAYLOAD_STORAGE_BYTES,
    WindowOwner, WindowReply, WindowResponse, WindowSession, app_detail,
    with_primary_caller_locked,
};
use super::{PublicationResult, WindowEmitter};
use keld_ipc::window::{WindowEvent, WindowRef};
use std::sync::Arc;
use std::time::Instant;

impl WindowEmitter<'_, '_> {
    pub(super) fn caller(&self) -> &PrimaryCallerContext {
        self.target.caller()
    }

    pub(super) fn owner(&self) -> WindowOwner {
        WindowOwner::new(self.caller().identity, self.caller().attempt)
    }

    pub(super) fn admit(
        &self,
        frame: PreparedFrame,
        check: impl FnOnce(&WindowSession) -> bool,
        phase: &'static str,
    ) -> PublicationResult<bool> {
        let receipt = self
            .with_registry(phase, |session, active| {
                if check(session) {
                    admit_locked(active, frame).map(Some)
                } else {
                    Ok(None)
                }
            })?
            .flatten();
        let Some(receipt) = receipt else {
            return Ok(false);
        };
        self.router
            .observe_publication(self.target, receipt, self.deadline, phase)?;
        Ok(true)
    }

    pub(super) fn with_registry<T>(
        &self,
        phase: &'static str,
        inspect: impl FnOnce(&mut WindowSession, &ActivePrimaryGeneration) -> Result<T, HostAppError>,
    ) -> Result<Option<T>, HostAppError> {
        let mut result = None;
        with_primary_caller_locked(
            &self.router.current,
            &self.router.shutdown,
            self.caller(),
            |current| {
                let Some(windows) = self.router.windows.as_ref() else {
                    return Ok(());
                };
                let mut session = windows
                    .lock()
                    .map_err(|_| app_detail(phase, "Registry lock poisoned"))?;
                if !self.turn.owns(windows, &session) {
                    return Err(app_detail(phase, "foreign Window publication turn"));
                }
                let active = current
                    .as_ref()
                    .ok_or_else(|| app_detail(phase, "validated caller disappeared"))?;
                result = Some(inspect(&mut session, active)?);
                Ok(())
            },
        )?;
        Ok(result)
    }

    pub(super) fn control(
        &self,
        message: ControlMessage,
        check: impl FnOnce(&WindowSession) -> bool,
        phase: &'static str,
    ) -> PublicationResult<bool> {
        let frame = self.router.prepare_primary_control(
            self.caller(),
            Arc::clone(self.target),
            message,
            self.deadline,
            phase,
        )?;
        self.admit(frame, check, phase)
    }

    pub(super) fn subscribed(&self, session: &WindowSession, source: Option<WindowRef>) -> bool {
        session.subscribed == Some(self.owner())
            && source.is_none_or(|window| {
                session
                    .registry
                    .retains_publication_window(self.owner(), window)
            })
    }

    pub(super) fn reply(
        &self,
        correlation: CorrelationId,
        response: WindowResponse,
        source: Option<WindowRef>,
    ) -> PublicationResult<()> {
        self.control(
            ControlMessage::WindowReply {
                correlation,
                response,
            },
            |session| self.subscribed(session, source),
            "Window reply completion",
        )?;
        Ok(())
    }

    pub(super) fn owned_reply(
        &self,
        reply: WindowReply,
        source: WindowRef,
    ) -> PublicationResult<()> {
        if reply.owner == self.owner() {
            self.reply(reply.correlation, reply.response, Some(source))?;
        }
        Ok(())
    }

    pub(super) fn refusal(
        &self,
        correlation: CorrelationId,
        refusal: WindowRefusal,
        require_subscription: bool,
        source: Option<WindowRef>,
    ) -> PublicationResult<()> {
        let frame = self.router.prepare_window_refusal(
            self.caller(),
            self.target,
            correlation,
            refusal,
            self.deadline,
        )?;
        self.admit(
            frame,
            |session| {
                (!require_subscription || self.subscribed(session, None))
                    && source.is_none_or(|window| {
                        session
                            .registry
                            .retains_publication_window(self.owner(), window)
                    })
            },
            "Window error completion",
        )?;
        Ok(())
    }

    pub(super) fn prepare_fact(&self, fact: &WindowEvent) -> Result<PreparedFrame, HostAppError> {
        self.router
            .reserve_primary_value(self.caller(), self.deadline, "Window fact reservation")?
            .encode_value(
                Arc::clone(self.target),
                FrameKind::Event,
                keld_ipc::channel_table::WINDOW.id(),
                CorrelationId(0),
                fact,
                WINDOW_PAYLOAD_STORAGE_BYTES,
            )
    }

    pub(super) fn fact(&self, fact: &OwnedWindowEvent) -> PublicationResult<()> {
        if fact.owner != self.owner() {
            return Ok(());
        }
        let window = fixed_event_window(&fact.event).ok_or_else(|| {
            app_detail(
                "Window fact publication",
                "nonordinary event entered the fixed Window source",
            )
        })?;
        self.admit(
            self.prepare_fact(&fact.event)?,
            |session| {
                self.subscribed(session, Some(window))
                    && session
                        .registry
                        .matches_publication_fact(self.owner(), &fact.event)
            },
            "Window fact completion",
        )?;
        Ok(())
    }

    pub(super) fn last_window(
        &self,
        recipient: Option<WindowOwner>,
        source: WindowRef,
    ) -> PublicationResult<()> {
        if recipient == Some(self.owner()) {
            self.control(
                ControlMessage::LifecycleEvent(LifecycleEvent::LastWindowClosed),
                |session| self.subscribed(session, Some(source)),
                "Window last-window completion",
            )?;
        }
        Ok(())
    }
}

impl PrimaryRouterHandle {
    pub(super) fn prepare_window_refusal(
        &self,
        caller: &PrimaryCallerContext,
        target: &Arc<WriterLink>,
        correlation: CorrelationId,
        refusal: WindowRefusal,
        deadline: Instant,
    ) -> Result<PreparedFrame, HostAppError> {
        let reservation =
            self.reserve_primary_value(caller, deadline, "Window error reservation")?;
        let (code, message) = refusal.into_parts();
        reservation.encode_error_value(
            Arc::clone(target),
            keld_ipc::channel_table::WINDOW.id(),
            correlation,
            &CallError {
                code: code.to_owned(),
                message,
            },
        )
    }

    pub(super) fn cleanup_window_action(
        &self,
        command: Teardown,
        phase: &'static str,
    ) -> Result<(), HostAppError> {
        let _transition = self.shutdown.transition_guard();
        let _current = self
            .current
            .lock()
            .map_err(|_| app_detail(phase, "current lock poisoned"))?;
        let windows = self
            .windows
            .as_ref()
            .ok_or_else(|| app_detail(phase, "Registry disappeared"))?;
        let session = windows
            .lock()
            .map_err(|_| app_detail(phase, "Registry lock poisoned"))?;
        match session
            .registry
            .closing_cleanup(command)
            .map_err(|error| app_detail(phase, error.to_string()))?
        {
            ClosingCleanup::Required => self.send_window_teardown(command, phase),
            ClosingCleanup::Released => Ok(()),
        }
    }
}
