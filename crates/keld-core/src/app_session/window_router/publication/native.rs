//! Consume each native fact once, retain its exact source, then publish outside
//! Registry/current/transition guards. Native custody is never a writer receipt.
use super::super::super::window_registry::NativePublicationSource;
use super::super::native_outcome::NativeOutcome;
use super::{
    AppWindowEvent, BuildOutcome, CloseOutcome, FactOutcome, HostAppError, Instant,
    NativePublicationDisposition, PrimaryRouterHandle, PublicationError, PublicationResult,
    ReleaseOutcome, WindowEmitter, WindowRefusal, WindowReply, WindowResponse, app_detail,
    with_primary_caller_locked,
};

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn window_native_event(
        &self,
        event: AppWindowEvent,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        self.window_native_event_until(event, Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE)
    }

    pub(super) fn window_native_event_until(
        &self,
        event: AppWindowEvent,
        deadline: Instant,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        let writer = self.primary_writer_owner()?;
        if matches!(event, AppWindowEvent::HostWriterProgress) {
            writer
                .acknowledge_progress()
                .map_err(|error| error.into_host_error("primary writer progress"))?;
            self.service_compat_writer_progress()?;
            self.service_dialog_writer_progress()?;
            self.service_numeric_echo_progress()?;
            return Ok(NativePublicationDisposition::Handled);
        }
        let event = match event {
            AppWindowEvent::DesktopObserved { facts } => {
                return self.publish_native_desktop(None, facts, deadline);
            }
            AppWindowEvent::WindowsDesktopObserved { capture, facts } => {
                return self.publish_native_desktop(Some(capture), facts, deadline);
            }
            event => event,
        };
        // Capture before the ordering wait; neither wake nor timeout recaptures B.
        let captured = self.capture_window_native_source(&event)?;
        let turn = match self.claim_window_publication(deadline, None) {
            Ok(Some(turn)) => turn,
            Ok(None) => {
                return self.window_native_turn_refused(
                    event,
                    &captured,
                    app_detail(
                        "Window native source",
                        "managed fact arrived without the Registry",
                    ),
                );
            }
            Err(error) => return self.window_native_turn_refused(event, &captured, error),
        };
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        let observed_build = match &event {
            AppWindowEvent::WindowBuilt { build, view, .. } => Some((*build, *view)),
            _ => None,
        };
        let outcome = self.prepare_window_native_source(event, Some(&turn));
        #[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
        observe_window_view_association(observed_build, &outcome);
        let disposition = if let (Some(source), Some(target)) =
            (captured.source.as_ref(), captured.target.as_ref())
        {
            let emitter = WindowEmitter {
                router: self,
                turn: &turn,
                target,
                deadline,
            };
            let result = outcome
                .map_err(PublicationError::from)
                .and_then(|outcome| emitter.native(source, outcome));
            drop(turn);
            self.finish_captured_window_native(&captured, result)?
        } else {
            // The same Registry transition and physical cleanup run even with
            // no eligible role. There is no successor route lookup.
            let cleanup = match outcome {
                Ok(outcome) => self.cleanup_unpublished_window_native(outcome),
                Err(error) if captured.source.is_none() => Err(error),
                Err(_) => Ok(()),
            };
            drop(turn);
            cleanup?;
            NativePublicationDisposition::Handled
        };
        if matches!(disposition, NativePublicationDisposition::Handled) {
            self.finish_window_publication_service(Ok(()))?;
        }
        Ok(disposition)
    }
}

impl WindowEmitter<'_, '_> {
    fn native(
        &self,
        source: &NativePublicationSource,
        outcome: NativeOutcome,
    ) -> PublicationResult<()> {
        match outcome {
            NativeOutcome::None => Ok(()),
            NativeOutcome::Fact(fact) => self.native_fact(fact),
            NativeOutcome::Build(BuildOutcome::Created { fact, reply }) => {
                self.fact(&fact)?;
                self.owned_reply(reply, source.window)
            }
            NativeOutcome::Build(BuildOutcome::Cleanup(action)) => self
                .router
                .cleanup_window_action(action, "Window late-build cleanup")
                .map_err(PublicationError::from),
            NativeOutcome::Build(BuildOutcome::Ignored) => {
                eprintln!(
                    "KELD-CORE-041: ignored unexpected native build result; inspect the UI build correlation"
                );
                Ok(())
            }
            NativeOutcome::BuildFailed { outcome, stage } => {
                if outcome.diagnostic {
                    eprintln!(
                        "KELD-CORE-041: ignored unexpected native build failure; inspect the UI build correlation"
                    );
                }
                if let Some(call) = outcome.call.filter(|call| call.owner == self.owner()) {
                    self.refusal(
                        call.correlation,
                        WindowRefusal::Build(stage),
                        true,
                        Some(source.window),
                    )?;
                }
                self.last_window(outcome.last_window, source.window)
            }
            NativeOutcome::Released(outcome) => self.native_released(source, outcome),
            NativeOutcome::Close(outcome) => self.native_close(outcome),
            NativeOutcome::OriginalDestroy {
                resolution,
                admission,
            } => {
                let result = self.original_destroy_resolution(&resolution, Some(source.window));
                drop(admission); // actual native ticket survives its source publication
                result
            }
            NativeOutcome::Committed(committed) => self.native_committed(committed),
            NativeOutcome::Finished(completed) => {
                self.fact(&completed.fact)?;
                self.owned_reply(completed.reply, source.window)
            }
            NativeOutcome::OriginalFailure { call, code, detail } => {
                if let Some(call) = call.filter(|call| call.owner == self.owner()) {
                    self.refusal(
                        call.correlation,
                        WindowRefusal::Native { code, detail },
                        true,
                        Some(source.window),
                    )?;
                }
                Ok(())
            }
        }
    }

    fn native_fact(&self, outcome: FactOutcome) -> PublicationResult<()> {
        match outcome {
            FactOutcome::Event(fact) => self.fact(&fact),
            FactOutcome::Unmapped => {
                eprintln!(
                    "KELD-CORE-038: native state references an unmapped view; inspect UI event ordering"
                );
                Ok(())
            }
            FactOutcome::Coalesced | FactOutcome::Suppressed | FactOutcome::Dropped => Ok(()),
        }
    }

    fn native_released(
        &self,
        source: &NativePublicationSource,
        outcome: ReleaseOutcome,
    ) -> PublicationResult<()> {
        if outcome.diagnostic {
            eprintln!("KELD-CORE-038: unexpected native release; inspect UI teardown ordering");
        }
        if outcome.dropped_facts != 0 {
            eprintln!(
                "window teardown dropped {} native facts",
                outcome.dropped_facts
            );
        }
        if let Some(fact) = outcome.fact {
            self.fact(&fact)?;
        }
        self.last_window(outcome.last_window, source.window)?;
        // Move the existing bounded Destroy correlations; encode one at a time.
        for call in outcome.destroy_calls {
            self.owned_reply(
                WindowReply {
                    owner: call.owner,
                    correlation: call.correlation,
                    response: WindowResponse::Destroyed,
                },
                source.window,
            )?;
        }
        Ok(())
    }

    fn native_close(&self, outcome: CloseOutcome) -> PublicationResult<()> {
        match outcome {
            CloseOutcome::Requested {
                fact: Some(fact), ..
            } => self.fact(&fact),
            CloseOutcome::Unmapped => {
                eprintln!(
                    "KELD-CORE-038: native close references an unmapped view; inspect UI event ordering"
                );
                Ok(())
            }
            CloseOutcome::Exhausted => {
                eprintln!(
                    "KELD-CORE-039: native close_seq exhausted; destroy the window or end the session"
                );
                Ok(())
            }
            CloseOutcome::AlreadyClosing
            | CloseOutcome::Merged { .. }
            | CloseOutcome::Requested { fact: None, .. } => Ok(()),
            CloseOutcome::Teardown(_) | CloseOutcome::Vetoed | CloseOutcome::Stale => {
                Err(app_detail(
                    "Window native close",
                    "Registry returned a call-only close outcome",
                )
                .into())
            }
        }
    }

    fn native_committed(
        &self,
        committed: super::super::super::window_registry::CommittedOriginalLoad,
    ) -> PublicationResult<()> {
        if committed.owner != self.owner() {
            return Ok(());
        }
        let mut source = None;
        with_primary_caller_locked(
            &self.router.current,
            &self.router.shutdown,
            self.caller(),
            |_| {
                let windows =
                    self.router.windows.as_ref().ok_or_else(|| {
                        app_detail("Window committed source", "Registry disappeared")
                    })?;
                let session = windows
                    .lock()
                    .map_err(|_| app_detail("Window committed source", "Registry lock poisoned"))?;
                if self.subscribed_source(&session, Some(committed.window)) {
                    source = session.registry.retain_committed_document(
                        committed.owner,
                        committed.window,
                        committed.document_generation,
                    )?;
                }
                Ok(())
            },
        )?;
        if let Some(source) = source
            && self.document(&source, false)?
        {
            self.router.publish_window_compat_route(
                self.turn,
                self.caller(),
                self.target,
                source.window,
                &source.document,
                self.deadline,
            )?;
        }
        Ok(())
    }
}

#[cfg(all(windows, feature = "profile-test-hooks", debug_assertions))]
fn observe_window_view_association(
    observed: Option<(u64, keld_wv::WebviewId)>,
    outcome: &Result<NativeOutcome, HostAppError>,
) {
    use std::io::Write as _;
    if std::env::var_os(keld_wv::TEST_WINDOWS_T2_OBSERVE_ENV).as_deref()
        != Some(std::ffi::OsStr::new("1"))
    {
        return;
    }
    let (Some((build, view)), Ok(NativeOutcome::Build(BuildOutcome::Created { fact, .. }))) =
        (observed, outcome)
    else {
        return;
    };
    let keld_ipc::window::WindowEvent::Created { window, .. } = &fact.event else {
        return;
    };
    // Only borrowed scalars from the actual committed Registry association.
    // All transition/current/Registry guards ended before this diagnostic.
    // This is neither a publication receipt nor renderer permission.
    let _ = writeln!(
        std::io::stderr().lock(),
        "KELD_WINDOW_VIEW_ASSOCIATION {{\"build\":{build},\"window\":{},\"incarnation\":{},\"view\":{}}}",
        window.id.0,
        window.incarnation.0,
        view.0
    );
}
