//! A synchronous Window source retains one ordering turn through actual write receipt.

use super::super::primary_writer::WriterRefusal;
use super::{
    HostAppError, Ordering, PrimaryCallerContext, PrimaryRouterHandle, WindowSession, app_detail,
};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

#[derive(Default)]
pub(super) struct WindowPublicationState {
    busy: bool,
    changed: Arc<Condvar>,
}

pub(in crate::app_session) struct WindowPublicationTurn<'session> {
    windows: &'session Arc<Mutex<WindowSession>>,
    changed: Arc<Condvar>,
}

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn claim_window_publication(
        &self,
        deadline: Instant,
        caller: Option<&PrimaryCallerContext>,
    ) -> Result<Option<WindowPublicationTurn<'_>>, HostAppError> {
        let Some(windows) = self.windows.as_ref() else {
            return Ok(None);
        };
        let mut session = windows
            .lock()
            .map_err(|_| app_detail("Window publication turn", "Registry lock poisoned"))?;
        let changed = Arc::clone(&session.publication.changed);
        loop {
            if caller.is_some_and(|caller| caller.reader_stop.load(Ordering::Acquire)) {
                return Ok(None);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(WriterRefusal::Deadline.into_host_error("Window publication turn"));
            }
            if !session.publication.busy {
                break;
            }
            let (next, _) = changed.wait_timeout(session, remaining).map_err(|_| {
                app_detail(
                    "Window publication turn",
                    "Registry lock poisoned while waiting",
                )
            })?;
            session = next;
        }
        session.publication.busy = true;
        Ok(Some(WindowPublicationTurn { windows, changed }))
    }
}

impl WindowPublicationTurn<'_> {
    pub(in crate::app_session) fn owns(
        &self,
        windows: &Arc<Mutex<WindowSession>>,
        session: &WindowSession,
    ) -> bool {
        Arc::ptr_eq(self.windows, windows) && session.publication.busy
    }
}

impl Drop for WindowPublicationTurn<'_> {
    fn drop(&mut self) {
        let mut session = self
            .windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        session.publication.busy = false;
        drop(session);
        self.changed.notify_all();
    }
}
