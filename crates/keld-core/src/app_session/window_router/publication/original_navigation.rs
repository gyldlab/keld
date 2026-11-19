//! Native callbacks retain facts in their Window entry. The Core coordinator
//! encodes and publishes those exact facts through the existing Window source.

use super::super::super::primary_writer::{ControlMessage, PreparedFrame, admit_locked};
use super::super::super::window_registry::{
    NavigationPublication, NavigationPublicationKind, NavigationTerminal,
};
use super::super::request::WindowRefusal;
use super::super::{
    CorrelationId, FrameKind, HostAppError, Ordering, PrimaryCallerContext, PrimaryRouterHandle,
    WindowResponse, app_detail,
};
use super::{NativePublicationDisposition, PublicationError, PublicationResult, WindowEmitter};
use keld_ipc::window::{NAVIGATION_EVENT_STORAGE_BYTES, WindowEvent, WindowEventValue};
use keld_wv::{NativeNavigationContext, OriginalNavigationFact};
use std::sync::Arc;
use std::time::Instant;

impl PrimaryRouterHandle {
    /// Called on the native UI owner. The exact record and the existing
    /// coalesced progress marker retain the fact without waiting for writes.
    pub(in crate::app_session) fn capture_original_navigation_fact(
        &self,
        context: NativeNavigationContext,
        fact: OriginalNavigationFact,
    ) -> Result<(), HostAppError> {
        let progressed = {
            let _transition = self.shutdown.transition_guard();
            let current = self.current.lock().map_err(|_| {
                app_detail("original navigation observation", "current owner poisoned")
            })?;
            let Some(active) = current.as_ref().filter(|active| {
                self.shutdown.is_running() && !active.reader_stop.load(Ordering::Acquire)
            }) else {
                return Ok(());
            };
            let Some(windows) = self.windows.as_ref() else {
                return Ok(());
            };
            let mut session = windows
                .lock()
                .map_err(|_| app_detail("original navigation observation", "Registry poisoned"))?;
            let caller = PrimaryCallerContext {
                attempt: active.attempt,
                identity: active.identity,
                reader_stop: Arc::clone(&active.reader_stop),
            };
            session
                .registry
                .capture_original_navigation(&caller, context, fact)
        };
        if progressed {
            self.primary_writer_owner()?
                .completed_source_progress()
                .map_err(|error| error.into_host_error("original navigation progress"))?;
        }
        Ok(())
    }

    pub(in crate::app_session) fn service_original_navigation(
        &self,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        let Some(target) = self.current_primary_writer_target()? else {
            return Ok(NativePublicationDisposition::Handled);
        };
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        let Some(turn) = self.claim_window_publication(deadline, Some(target.caller()))? else {
            return Ok(NativePublicationDisposition::Handled);
        };
        let emitter = WindowEmitter {
            router: self,
            turn: &turn,
            target: &target,
            deadline,
        };
        loop {
            let next = emitter
                .with_registry("original navigation publication", |session, _| {
                    Ok(emitter
                        .subscribed(session, None)
                        .then(|| session.registry.next_original_navigation(emitter.caller()))
                        .flatten())
                })?
                .flatten();
            let Some(publication) = next else {
                return Ok(NativePublicationDisposition::Handled);
            };
            if let Err(error) = emitter.publish_original_navigation(publication) {
                let owner = emitter.owner();
                drop(turn);
                return self.finish_native_publication(
                    target.caller(),
                    |session| {
                        session.subscribed == Some(owner)
                            && session
                                .registry
                                .retains_publication_window(owner, publication.window)
                    },
                    Err(error),
                );
            }
        }
    }
}

impl WindowEmitter<'_, '_> {
    pub(super) fn interrupt_original_navigation(
        &self,
        window: keld_ipc::window::WindowRef,
    ) -> PublicationResult<()> {
        let pending = self
            .with_registry("original navigation interruption", |session, _| {
                Ok(session
                    .registry
                    .interrupt_original_load(self.caller(), window))
            })?
            .flatten();
        if let Some(pending) = pending {
            self.publish_original_navigation(pending)?;
        }
        Ok(())
    }

    fn prepare_navigation_frame(
        &self,
        publication: NavigationPublication,
    ) -> PublicationResult<Option<PreparedFrame>> {
        let window = publication.window;
        let phase = "original navigation frame";
        let frame = match publication.kind {
            NavigationPublicationKind::Commit(stamp) => {
                let source = self
                    .with_registry(phase, |session, _| {
                        session
                            .registry
                            .retain_original_navigation_url(self.caller(), publication)
                            .map_err(|error| app_detail(phase, error.to_string()))
                    })?
                    .flatten();
                let Some(source) = source else {
                    return Ok(None);
                };
                self.router
                    .reserve_primary_value(self.caller(), self.deadline, phase)?
                    .encode_value(
                        Arc::clone(self.target),
                        FrameKind::Event,
                        keld_ipc::channel_table::WINDOW.id(),
                        CorrelationId(0),
                        &WindowEventValue::NavigationCommitted {
                            window,
                            document_generation: stamp.observed_generation(),
                            native_url: source.as_str(),
                        },
                        NAVIGATION_EVENT_STORAGE_BYTES,
                    )?
            }
            NavigationPublicationKind::Finish(stamp) => {
                self.prepare_fact(&WindowEvent::NavigationFinished {
                    window,
                    document_generation: stamp.observed_generation(),
                })?
            }
            NavigationPublicationKind::Reply(_) => self.router.prepare_primary_control(
                self.caller(),
                Arc::clone(self.target),
                ControlMessage::WindowReply {
                    correlation: publication.correlation,
                    response: WindowResponse::Loaded,
                },
                self.deadline,
                phase,
            )?,
            NavigationPublicationKind::Error(terminal) => self.router.prepare_window_refusal(
                self.caller(),
                self.target,
                publication.correlation,
                navigation_refusal(terminal)?,
                self.deadline,
            )?,
        };
        Ok(Some(frame))
    }

    fn publish_original_navigation(
        &self,
        publication: NavigationPublication,
    ) -> PublicationResult<()> {
        let Some(frame) = self.prepare_navigation_frame(publication)? else {
            return Ok(());
        };
        let phase = "original navigation completion";
        let receipt = self
            .with_registry(phase, |session, active| {
                if !self.subscribed(session, Some(publication.window))
                    || !session
                        .registry
                        .is_current_navigation_publication(self.caller(), publication)
                {
                    return Ok(None);
                }
                let receipt = admit_locked(active, frame)?;
                session.registry.commit_navigation_publication(publication);
                Ok(Some(receipt))
            })?
            .flatten();
        if let Some(receipt) = receipt {
            self.router
                .observe_publication(self.target, receipt, self.deadline, phase)?;
        }
        Ok(())
    }
}

fn navigation_refusal(terminal: NavigationTerminal) -> Result<WindowRefusal, PublicationError> {
    Ok(match terminal {
        NavigationTerminal::Refused(error) => WindowRefusal::NativeNavigation(error),
        NavigationTerminal::Failed => WindowRefusal::fixed(
            "KELD-CORE-052",
            "KELD-CORE-052: the admitted native navigation failed; use a current window for a new explicit load.",
        ),
        NavigationTerminal::Interrupted => WindowRefusal::fixed(
            "KELD-CORE-053",
            "KELD-CORE-053: the admitted original load lost its native document before completion; use a current window for a new explicit load.",
        ),
        NavigationTerminal::Finished(_) => {
            return Err(app_detail(
                "original navigation completion",
                "finished entered the error route",
            )
            .into());
        }
    })
}
