//! A native retirement observation uses the same lease, never matching numbers
//! from another native owner, and its fixed notification uses the existing writer.
use super::{
    Arc, CompatLedger, CompatSession, ControlMessage, HostAppError, Instant,
    MacNativeDocumentLease, Mutex, NativePublicationDisposition, NativeRouteKey,
    PrimaryCallerContext, PrimaryRouterHandle, PublicationResult, RendererBridgeRequest,
    RendererRoute, admit_locked, app_detail, exact_document, with_primary_caller_locked,
};

impl CompatLedger {
    fn document_retired_exact(&mut self, document: &MacNativeDocumentLease) {
        if self
            .preflight
            .as_ref()
            .is_some_and(|pending| pending.document.same_binding(document))
        {
            self.preflight = None;
        }
        for charge in self.data.iter_mut().flatten() {
            if charge
                .document
                .as_ref()
                .is_some_and(|held| held.same_binding(document))
            {
                charge.retired = true;
            }
        }
    }
}

fn observed_route(
    windows: &crate::app_session::window_router::WindowSession,
    caller: &PrimaryCallerContext,
    captured: &RendererRoute,
) -> bool {
    windows
        .observed_registered_renderer_route(
            caller,
            captured.document.stamp().view(),
            &captured.document,
        )
        .is_some_and(|route| {
            route.window == captured.window && route.document.same_binding(&captured.document)
        })
}

impl PrimaryRouterHandle {
    #[expect(
        clippy::too_many_lines,
        reason = "The exact retired lease, publication turn and final disposition stay in one bounded owner sequence."
    )]
    pub(super) fn retire_renderer_document_owned(
        &self,
        compat: &Arc<Mutex<CompatSession>>,
        request: &RendererBridgeRequest,
        key: NativeRouteKey,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        let Some(document) = request
            .document_lease()
            .filter(|lease| exact_document(lease, key))
        else {
            return Ok(NativePublicationDisposition::Handled);
        };
        compat
            .lock()
            .map_err(|_| app_detail("compat native retirement", "ledger lock poisoned"))?
            .ledger
            .document_retired_exact(document);
        let deadline = Instant::now() + keld_ipc::APP_LINK_IO_DEADLINE;
        let caller = self
            .current
            .lock()
            .map_err(|_| app_detail("compat retirement notification", "generation lock poisoned"))?
            .as_ref()
            .map(|active| PrimaryCallerContext {
                identity: active.identity,
                attempt: active.attempt,
                reader_stop: Arc::clone(&active.reader_stop),
            });
        let Some(caller) = caller else {
            return Ok(NativePublicationDisposition::Handled);
        };
        let turn = self.claim_window_publication(deadline, None);
        let mut route = None;
        with_primary_caller_locked(&self.current, &self.shutdown, &caller, |_| {
            let Some(windows) = &self.windows else {
                return Ok(());
            };
            let windows = windows.lock().map_err(|_| {
                app_detail("compat retirement notification", "Registry lock poisoned")
            })?;
            route =
                windows.observed_registered_renderer_route(&caller, request.webview(), document);
            Ok(())
        })?;
        let Some(route) = route else {
            return Ok(NativePublicationDisposition::Handled);
        };
        let turn = match turn {
            Ok(Some(turn)) => turn,
            Ok(None) => return Ok(NativePublicationDisposition::Handled),
            Err(error) => {
                return self.finish_native_publication(
                    &caller,
                    |windows| observed_route(windows, &caller, &route),
                    Err(error.into()),
                );
            }
        };
        let result = (|| -> PublicationResult<()> {
            let Some(target) = self.primary_writer_target(&caller)? else {
                return Ok(());
            };
            let mut frame = Some(self.prepare_primary_control(
                &caller,
                Arc::clone(&target),
                ControlMessage::DocumentRetired {
                    webview: key.webview,
                    navigation: key.navigation,
                },
                deadline,
                "compat DocumentRetired preparation",
            )?);
            let mut receipt = None;
            with_primary_caller_locked(&self.current, &self.shutdown, &caller, |current| {
                let Some(windows) = &self.windows else {
                    return Ok(());
                };
                let windows = windows.lock().map_err(|_| {
                    app_detail("compat DocumentRetired admission", "Registry lock poisoned")
                })?;
                if !turn.owns(
                    self.windows.as_ref().ok_or_else(|| {
                        app_detail("compat DocumentRetired", "Registry disappeared")
                    })?,
                    &windows,
                ) || !observed_route(&windows, &caller, &route)
                {
                    return Ok(());
                }
                if !compat
                    .lock()
                    .map_err(|_| app_detail("compat DocumentRetired", "ledger lock poisoned"))?
                    .subscribed_to(&caller)
                {
                    return Ok(());
                }
                let active = current.as_ref().ok_or_else(|| {
                    app_detail("compat DocumentRetired", "validated caller disappeared")
                })?;
                receipt = Some(admit_locked(
                    active,
                    frame.take().ok_or_else(|| {
                        app_detail("compat DocumentRetired", "prepared frame disappeared")
                    })?,
                )?);
                Ok(())
            })?;
            drop(frame);
            if let Some(receipt) = receipt {
                self.observe_publication(
                    &target,
                    receipt,
                    deadline,
                    "compat DocumentRetired completion",
                )?;
            }
            Ok(())
        })();
        drop(turn);
        let disposition = self.finish_native_publication(
            &caller,
            |windows| observed_route(windows, &caller, &route),
            result,
        )?;
        if matches!(disposition, NativePublicationDisposition::Handled) {
            self.service_compat_writer_progress()?;
            self.service_dialog_writer_progress()?;
        }
        Ok(disposition)
    }
}
