//! The existing `WindowSession` serializes synchronous publication without
//! retaining Registry/current/transition guards through writer capacity or I/O.
use super::super::primary_writer::WriterLink;
use super::{
    CorrelationId, HostAppError, PrimaryCallerContext, PrimaryRouterHandle, WindowSession,
    app_detail,
};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplayStage {
    Desktop,
    Created,
    Documents,
    Subscribed,
}

pub(super) struct PendingWindowReplay {
    pub(super) target: Arc<WriterLink>,
    pub(super) correlation: CorrelationId,
    pub(super) deadline: Instant,
    pub(super) stage: ReplayStage,
    pub(super) after: u32,
}

pub(super) struct WindowPublicationState {
    busy: bool,
    changed: Arc<Condvar>,
    pub(super) replay: Option<PendingWindowReplay>,
    #[cfg(test)]
    wait_observer: Option<std::sync::mpsc::SyncSender<()>>,
}
impl Default for WindowPublicationState {
    fn default() -> Self {
        Self {
            busy: false,
            changed: Arc::new(Condvar::new()),
            replay: None,
            #[cfg(test)]
            wait_observer: None,
        }
    }
}

/// The sole non-clone turn owner. Neither retirement nor an observation/wake
/// can release this turn. A later source cannot acquire it before Drop finishes.
pub(in crate::app_session) struct WindowPublicationTurn<'session> {
    windows: &'session Arc<Mutex<WindowSession>>,
    changed: Arc<Condvar>,
}

impl PrimaryRouterHandle {
    #[cfg(test)]
    pub(in crate::app_session::window_router) fn observe_window_turn_wait_for_test(
        &self,
        observer: std::sync::mpsc::SyncSender<()>,
    ) -> Result<(), HostAppError> {
        self.windows
            .as_ref()
            .ok_or_else(|| app_detail("window turn test", "Registry absent"))?
            .lock()
            .map_err(|_| app_detail("window turn test", "Registry poisoned"))?
            .publication
            .wait_observer = Some(observer);
        Ok(())
    }

    /// Claim before taking transition/current/Compat. This replaces the old
    /// Registry admission serialization; it promises no FIFO among independent
    /// concurrent callers. The single native Receiver still determines native
    /// source arrival order. The source captures ONE absolute deadline before
    /// this claim and carries it through all later reservation/write stages.
    /// A captured reader caller is cancellation observation only; final publication
    /// still revalidates its actual caller/link. Native cleanup passes no caller.
    /// None means absent Registry or an already-stopped captured reader.
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
            .map_err(|_| app_detail("window publication turn", "registry lock poisoned"))?;
        let changed = Arc::clone(&session.publication.changed);
        loop {
            // Recheck after every actual wake, including timeout, before taking
            // the turn. A late old reader never acquires custody for its successor.
            if caller.is_some_and(|caller| caller.reader_stop.load(Ordering::Acquire)) {
                return Ok(None);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(super::super::primary_writer::WriterRefusal::Deadline
                    .into_host_error("window publication turn"));
            }
            if !session.publication.busy {
                break;
            }
            #[cfg(test)]
            if let Some(observer) = session.publication.wait_observer.take() {
                let _ = observer.try_send(());
            }
            let (next, _) = changed.wait_timeout(session, remaining).map_err(|_| {
                app_detail(
                    "window publication turn",
                    "registry lock poisoned while waiting",
                )
            })?;
            session = next;
        }
        if session.publication.replay.is_some() {
            return Err(app_detail(
                "window publication turn",
                "an unfinished replay lost its owning turn",
            ));
        }
        session.publication.busy = true;
        drop(session);
        Ok(Some(WindowPublicationTurn { windows, changed }))
    }
}

impl WindowPublicationTurn<'_> {
    pub(in crate::app_session) fn owns(
        &self,
        windows: &Arc<Mutex<WindowSession>>,
        session: &WindowSession,
    ) -> bool {
        // Construction is private and only this non-clone guard clears busy.
        // This is source ordering custody, never caller or native authority.
        Arc::ptr_eq(self.windows, windows) && session.publication.busy
    }
}

impl Drop for WindowPublicationTurn<'_> {
    fn drop(&mut self) {
        let mut session = self
            .windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Error, early return and unwind clear only this turn's unfinished replay.
        // A successor cannot own a replay while this guard still owns busy.
        // Successful final Written commit already removed replay and installed
        // subscribed; no successful subscription is cleared here.
        session.publication.replay = None;
        session.publication.busy = false;
        drop(session);
        self.changed.notify_all();
    }
}

#[cfg(test)]
mod tests;
