//! A synchronous source keeps one exact target and one publication turn. Only
//! the shared writer stores encoded bytes; this value owns no payload queue.
use super::{
    Arc, CallError, ControlMessage, CorrelationId, FrameKind, HostAppError, Instant,
    LifecycleEvent, OwnedWindowEvent, PendingWindowCall, PreparedFrame, PrimaryCallerContext,
    PrimaryRouterHandle, PublicationResult, RetainedWindowDocument, WINDOW_PAYLOAD_STORAGE_BYTES,
    WindowError, WindowEvent, WindowEventValue, WindowOwner, WindowPublicationTurn, WindowRef,
    WindowRefusal, WindowReply, WindowResponse, WindowSession, WriterLink, admit_locked,
    app_detail, report_abandoned, with_primary_caller_locked,
};
use std::io::Write as _;

pub(super) struct WindowEmitter<'source, 'session> {
    pub(super) router: &'source PrimaryRouterHandle,
    pub(super) turn: &'source WindowPublicationTurn<'session>,
    pub(super) target: &'source Arc<WriterLink>,
    pub(super) deadline: Instant,
}

impl WindowEmitter<'_, '_> {
    pub(super) fn caller(&self) -> &PrimaryCallerContext {
        self.target.caller()
    }
    pub(super) fn owner(&self) -> WindowOwner {
        WindowOwner::new(self.caller().identity, self.caller().attempt)
    }

    /// Storage/encoding precede this short final source/caller/target fence.
    /// No physical write or capacity wait occurs while its guards are held.
    pub(super) fn admit(
        &self,
        frame: PreparedFrame,
        check: impl FnOnce(&WindowSession) -> bool,
        phase: &'static str,
    ) -> PublicationResult<bool> {
        let mut frame = Some(frame);
        let mut check = Some(check);
        let mut receipt = None;
        with_primary_caller_locked(
            &self.router.current,
            &self.router.shutdown,
            self.caller(),
            |current| {
                let Some(windows) = self.router.windows.as_ref() else {
                    return Ok(());
                };
                let session = windows
                    .lock()
                    .map_err(|_| app_detail(phase, "Registry lock poisoned"))?;
                if !self.turn.owns(windows, &session) {
                    return Err(app_detail(phase, "foreign Window publication turn"));
                }
                let predicate = check
                    .take()
                    .ok_or_else(|| app_detail(phase, "source predicate entered twice"))?;
                if !predicate(&session) {
                    return Ok(());
                }
                let active = current
                    .as_ref()
                    .ok_or_else(|| app_detail(phase, "validated caller disappeared"))?;
                receipt = Some(admit_locked(
                    active,
                    frame
                        .take()
                        .ok_or_else(|| app_detail(phase, "prepared frame disappeared"))?,
                )?);
                Ok(())
            },
        )?;
        drop(frame);
        let Some(receipt) = receipt else {
            return Ok(false);
        };
        self.router
            .observe_publication(self.target, receipt, self.deadline, phase)?;
        Ok(true)
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

    pub(super) fn subscribed_source(
        &self,
        session: &WindowSession,
        source: Option<WindowRef>,
    ) -> bool {
        session.subscribed == Some(self.owner())
            && source.is_none_or(|window| {
                session
                    .registry
                    .retains_caller_window(self.caller(), window)
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
            |session| self.subscribed_source(session, source),
            "Window reply completion",
        )?;
        Ok(())
    }

    /// Formatted/path-bearing diagnostics always use VALUE, even when short.
    /// The source diagnostic is consumed only after the same writer slot exists.
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
                (!require_subscription || session.subscribed == Some(self.owner()))
                    && source.is_none_or(|window| {
                        session
                            .registry
                            .retains_caller_window(self.caller(), window)
                    })
            },
            "Window error completion",
        )?;
        Ok(())
    }

    pub(super) fn fact(&self, fact: &OwnedWindowEvent) -> PublicationResult<()> {
        if fact.owner != self.owner() {
            return Ok(());
        }
        let window = fixed_window(&fact.event).ok_or_else(|| {
            app_detail(
                "Window fixed fact",
                "variable or foreign-platform event entered the fixed source",
            )
        })?;
        let reservation = self.router.reserve_primary_value(
            self.caller(),
            self.deadline,
            "Window fact reservation",
        )?;
        let frame = reservation.encode_value(
            Arc::clone(self.target),
            FrameKind::Event,
            keld_ipc::channel_table::WINDOW.id(),
            CorrelationId(0),
            &fact.event,
            WINDOW_PAYLOAD_STORAGE_BYTES,
        )?;
        self.admit(
            frame,
            |session| {
                self.subscribed_source(session, Some(window))
                    && session
                        .registry
                        .matches_publication_fact(fact.owner, &fact.event)
            },
            "Window fact completion",
        )?;
        Ok(())
    }

    pub(super) fn document(
        &self,
        source: &RetainedWindowDocument,
        recovered: bool,
    ) -> PublicationResult<bool> {
        if source.owner != self.owner() {
            return Ok(false);
        }
        let reservation = self.router.reserve_primary_value(
            self.caller(),
            self.deadline,
            "Window document reservation",
        )?;
        let generation = source.document.stamp().observed_generation();
        let event = if recovered {
            WindowEventValue::DocumentRecovered {
                window: source.window,
                document_generation: generation,
                native_url: source.native_url.as_str(),
            }
        } else {
            WindowEventValue::NavigationCommitted {
                window: source.window,
                document_generation: generation,
                native_url: source.native_url.as_str(),
            }
        };
        let frame = reservation.encode_value(
            Arc::clone(self.target),
            FrameKind::Event,
            keld_ipc::channel_table::WINDOW.id(),
            CorrelationId(0),
            &event,
            keld_ipc::window::NAVIGATION_EVENT_STORAGE_BYTES,
        )?;
        self.admit(
            frame,
            |session| {
                (recovered || session.subscribed == Some(self.owner()))
                    && session
                        .registry
                        .retains_caller_window(self.caller(), source.window)
                    && session
                        .registry
                        .retains_document_publication(source, recovered)
            },
            "Window document completion",
        )
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

    pub(super) fn original_interruption(
        &self,
        call: Option<PendingWindowCall>,
        source: Option<WindowRef>,
    ) -> PublicationResult<()> {
        if let Some(call) = call.filter(|call| call.owner == self.owner()) {
            self.refusal(call.correlation, WindowRefusal::fixed("KELD-CORE-053",
                "KELD-CORE-053: the window closed before its admitted original load finished; use a current live window for a new explicit load."), true, source)?;
        }
        Ok(())
    }

    pub(super) fn original_destroy_resolution(
        &self,
        outcome: &super::super::super::window_registry::OriginalDocumentDestroyResolution,
        source: Option<WindowRef>,
    ) -> PublicationResult<()> {
        if let Some(close) = outcome.abandoned {
            report_abandoned(close);
        }
        if let Some(call) = outcome.refused.filter(|call| call.owner == self.owner()) {
            self.refusal(
                call.correlation,
                WindowRefusal::Registry(WindowError::OriginalDocumentCloseRefused),
                true,
                source,
            )?;
        }
        self.original_interruption(outcome.interrupted, source)
    }

    pub(super) fn last_window(
        &self,
        recipient: Option<WindowOwner>,
        source: WindowRef,
    ) -> PublicationResult<()> {
        if recipient == Some(self.owner()) {
            self.control(
                ControlMessage::LifecycleEvent(LifecycleEvent::LastWindowClosed),
                |session| self.subscribed_source(session, Some(source)),
                "Window last-window completion",
            )?;
        }
        Ok(())
    }
}

pub(super) fn fixed_window(fact: &WindowEvent) -> Option<WindowRef> {
    match fact {
        WindowEvent::Created { window, .. }
        | WindowEvent::Bounds { window, .. }
        | WindowEvent::Focus { window, .. }
        | WindowEvent::Maximized { window, .. }
        | WindowEvent::FullScreen { window, .. }
        | WindowEvent::CloseRequested { window, .. }
        | WindowEvent::Destroyed { window }
        | WindowEvent::NavigationFinished { window, .. } => Some(*window),
        WindowEvent::NavigationCommitted { .. }
        | WindowEvent::DocumentRecovered { .. }
        | WindowEvent::DesktopSnapshot { .. } => None,
    }
}

impl PrimaryRouterHandle {
    pub(in crate::app_session::window_router) fn prepare_window_refusal(
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
        let error = CallError {
            code: code.to_owned(),
            message,
        };
        let frame = reservation.encode_error_value(
            Arc::clone(target),
            keld_ipc::channel_table::WINDOW.id(),
            correlation,
            &error,
        )?;
        drop(error);
        let _ = writeln!(
            std::io::stderr().lock(),
            "{code} phase=window-request correlation={}",
            correlation.0
        );
        Ok(frame)
    }
}
