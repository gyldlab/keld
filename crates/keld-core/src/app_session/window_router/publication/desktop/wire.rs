//! Desktop uses the existing VALUE lane and the same source/caller final fence.
use super::super::{
    Arc, CallError, CorrelationId, FrameKind, LIFECYCLE_CHANNEL, PublicationResult, WindowEmitter,
    WindowEvent, WindowSession, app_detail,
};

pub(super) const UNAVAILABLE_CODE: &str = "KELD-CORE-049";
pub(super) const UNAVAILABLE_MESSAGE: &str = "The native desktop snapshot is unavailable. Correct the Host's native display/appearance error and relaunch.";

pub(in crate::app_session::window_router::publication) fn retains_snapshot(
    session: &WindowSession,
    snapshot: &Arc<WindowEvent>,
) -> bool {
    session.desktop_required
        && session.desktop_seen
        && session
            .desktop_latest
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, snapshot))
}

impl WindowEmitter<'_, '_> {
    pub(in crate::app_session::window_router::publication) fn desktop_snapshot(
        &self,
        snapshot: &Arc<WindowEvent>,
        check: impl FnOnce(&WindowSession) -> bool,
    ) -> PublicationResult<bool> {
        if !matches!(snapshot.as_ref(), WindowEvent::DesktopSnapshot { .. }) {
            return Err(app_detail("desktop publication", "foreign source variant").into());
        }
        let reservation = self.router.reserve_primary_value(
            self.caller(),
            self.deadline,
            "desktop snapshot reservation",
        )?;
        let frame = reservation.encode_value(
            Arc::clone(self.target),
            FrameKind::Event,
            keld_ipc::channel_table::WINDOW.id(),
            CorrelationId(0),
            snapshot.as_ref(),
            keld_ipc::window::DESKTOP_EVENT_STORAGE_BYTES,
        )?;
        self.admit(
            frame,
            |session| retains_snapshot(session, snapshot) && check(session),
            "desktop snapshot completion",
        )
    }

    pub(in crate::app_session::window_router::publication) fn desktop_unavailable(
        &self,
        correlation: CorrelationId,
        snapshot: &Arc<WindowEvent>,
        check: impl FnOnce(&WindowSession) -> bool,
    ) -> PublicationResult<bool> {
        self.desktop_call_error(
            correlation,
            UNAVAILABLE_CODE,
            UNAVAILABLE_MESSAGE,
            |session| retains_snapshot(session, snapshot) && check(session),
        )
    }

    pub(in crate::app_session::window_router::publication) fn desktop_pending_conflict(
        &self,
        correlation: CorrelationId,
        pending_correlation: CorrelationId,
    ) -> PublicationResult<bool> {
        self.desktop_call_error(
            correlation,
            "KELD-IPC-005",
            "a desktop Subscribe is already pending on this role",
            |session| {
                session.pending_desktop.as_ref().is_some_and(|pending| {
                    Arc::ptr_eq(&pending.target, self.target)
                        && pending.correlation == pending_correlation
                        && pending.correlation != correlation
                })
            },
        )
    }

    fn desktop_call_error(
        &self,
        correlation: CorrelationId,
        code: &'static str,
        message: &'static str,
        check: impl FnOnce(&WindowSession) -> bool,
    ) -> PublicationResult<bool> {
        let reservation = self.router.reserve_primary_value(
            self.caller(),
            self.deadline,
            "desktop Subscribe error reservation",
        )?;
        let error = CallError {
            code: code.to_owned(),
            message: format!("{code}: {message}"),
        };
        let frame = reservation.encode_error_value(
            Arc::clone(self.target),
            LIFECYCLE_CHANNEL,
            correlation,
            &error,
        )?;
        drop(error);
        self.admit(frame, check, "desktop Subscribe error completion")
    }
}
