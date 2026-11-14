//! Native input stays owned through the actual request Drop. No role is recaptured
//! after a source has been selected, and no source callback owns a second writer.
use super::{
    Arc, CompatSession, Direction, HostAppError, MacNativeDocumentLease, Mutex,
    NativePublicationDisposition, NativeRouteKey, PreflightAdmission, PrimaryCallerContext,
    PrimaryRouterHandle, RendererBridgeOperation, RendererBridgeOutcome, RendererBridgeRequest,
    RendererRoute, app_detail, app_ipc, compat_control, exact_document, key,
    preflight_privileged_query, preflight_role_matches, principal, query, same_caller,
    with_primary_caller_locked,
};
use keld_ipc::compat_control::CompatControlEventValue;

pub(in crate::app_session) enum RendererRequestDisposition {
    Invoke,
    Complete(NativePublicationDisposition),
    /// Keep the attached input/source and typed failure; never redispatch or
    /// convert a retired source into a generic Host-wide Fatal.
    RetainedFailure {
        cause: HostAppError,
        disposition: RendererFailureDisposition,
    },
}

pub(in crate::app_session) enum RendererFailureDisposition {
    RetiredSource,
    FatalQueued,
}

pub(super) fn preflight_matches(
    preflight: &PreflightAdmission,
    caller: &PrimaryCallerContext,
    original_call: u32,
    document: &MacNativeDocumentLease,
    channel: &str,
) -> bool {
    preflight.call == original_call
        && same_caller(&preflight.caller, caller)
        && preflight.channel == channel
        && preflight.document.same_binding(document)
}

pub(super) fn borrowed_renderer_event<'a>(
    request: &'a RendererBridgeRequest,
    route: &RendererRoute,
    key: NativeRouteKey,
    channel: &'a str,
    payload: &'a [u8],
) -> Result<CompatControlEventValue<&'a str, &'a [u8]>, HostAppError> {
    let event = CompatControlEventValue::RendererMessage {
        webview: key.webview,
        navigation: key.navigation,
        message: key.message,
        window_id: route.window.id.0,
        window_incarnation: route.window.incarnation.0,
        url: request.url().ok_or_else(|| {
            app_detail(
                "compat native provenance",
                "qualified native Source URL is unavailable",
            )
        })?,
        origin: request.origin().ok_or_else(|| {
            app_detail(
                "compat native provenance",
                "qualified origin serialization is unavailable",
            )
        })?,
        topic: channel,
        payload,
    };
    compat_control::validate_event(&event)
        .map_err(|source| app_ipc("compat renderer event validation", &source))?;
    Ok(event)
}

impl PrimaryRouterHandle {
    /// The existing dispatcher keeps this exact request attached across failure.
    /// An error is terminal for that owner and must never be redispatched: some
    /// fields may already belong to the same Core records. No replacement queue.
    pub(in crate::app_session) fn dispatch_renderer_request_retained(
        &self,
        retained: &mut Option<RendererBridgeRequest>,
    ) -> Result<RendererRequestDisposition, HostAppError> {
        let request = retained.as_ref().ok_or_else(|| {
            app_detail(
                "renderer retained dispatch",
                "attached input is unavailable",
            )
        })?;
        if matches!(request.operation(), RendererBridgeOperation::Invoke) {
            return Ok(RendererRequestDisposition::Invoke);
        }
        let failure_source = match self.capture_renderer_input_source(request) {
            Ok(source) => source,
            Err(cause) => {
                return Ok(RendererRequestDisposition::RetainedFailure {
                    cause,
                    disposition: RendererFailureDisposition::RetiredSource,
                });
            }
        };
        let physical_input = matches!(request.operation(), RendererBridgeOperation::Send { .. })
            .then(|| key(request).zip(request.document_lease().cloned()))
            .flatten();
        // Any fallible path before this successful disposition leaves SAME Some,
        // even if Core already committed an exact source record before the error.
        let result = match self.dispatch_named_renderer_owned(request) {
            Ok(NativePublicationDisposition::Handled) => NativePublicationDisposition::Handled,
            Ok(NativePublicationDisposition::FatalQueued(cause)) => {
                return Ok(RendererRequestDisposition::RetainedFailure {
                    cause,
                    disposition: RendererFailureDisposition::FatalQueued,
                });
            }
            Err(cause) => {
                return Ok(self.retain_renderer_input_failure(
                    retained,
                    failure_source.as_ref(),
                    cause,
                ));
            }
        };
        drop(retained.take()); // actual successful input backing ends before its release half
        let released = (|| {
            if let (Some(compat), Some((key, document))) = (&self.compat, physical_input) {
                let mut state = compat.lock().map_err(|_| {
                    app_detail("compat inbound physical release", "session lock poisoned")
                })?;
                if state.ledger.data[Direction::RendererToMain.index()]
                    .as_ref()
                    .is_some_and(|charge| {
                        charge.key == key
                            && charge
                                .document
                                .as_ref()
                                .is_some_and(|held| held.same_binding(&document))
                    })
                {
                    state.ledger.native_quiesced(Direction::RendererToMain, key);
                }
            }
            Ok(())
        })();
        let disposition = match (result, released) {
            (NativePublicationDisposition::FatalQueued(cause), cleanup) => {
                NativePublicationDisposition::FatalQueued(crate::app_session::append_app_cleanup(
                    cause,
                    [cleanup],
                ))
            }
            (NativePublicationDisposition::Handled, Ok(())) => {
                NativePublicationDisposition::Handled
            }
            (NativePublicationDisposition::Handled, Err(error)) => {
                // Successful source transfer already disposed the input. A
                // bookkeeping failure retains the actual Core record/cause.
                return Ok(self.retain_renderer_input_failure(
                    retained,
                    failure_source.as_ref(),
                    error,
                ));
            }
        };
        Ok(RendererRequestDisposition::Complete(disposition))
    }

    fn dispatch_named_renderer_owned(
        &self,
        request: &RendererBridgeRequest,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        let Some(compat) = &self.compat else {
            return Ok(NativePublicationDisposition::Handled);
        };
        let key = key(request).ok_or_else(|| {
            app_detail(
                "compat native request",
                "native request has an invalid selector",
            )
        })?;
        match request.operation() {
            RendererBridgeOperation::Registered => self.note_renderer_registered(request, key),
            RendererBridgeOperation::Preflight { channel } => {
                self.preflight_renderer_owned(compat, request, key, channel)
            }
            RendererBridgeOperation::Send {
                channel,
                original_call,
            } => self.send_renderer_owned(compat, request, key, channel, *original_call),
            RendererBridgeOperation::Retired => {
                self.retire_renderer_document_owned(compat, request, key)
            }
            RendererBridgeOperation::PreflightAborted => {
                let mut state = compat
                    .lock()
                    .map_err(|_| app_detail("compat preflight abort", "session lock poisoned"))?;
                if state.ledger.preflight.as_ref().is_some_and(|pending| {
                    pending.call == key.message
                        && preflight_role_matches(&pending.caller, request.role_witness())
                        && request
                            .document_lease()
                            .is_some_and(|lease| lease.same_binding(&pending.document))
                }) {
                    state.ledger.preflight = None;
                }
                Ok(NativePublicationDisposition::Handled)
            }
            RendererBridgeOperation::EventRetired { retirement } => {
                self.retire_native_event(compat, retirement)
            }
            RendererBridgeOperation::Consumed { .. }
            | RendererBridgeOperation::EventQuiesced { .. } => {
                let caller = compat
                    .lock()
                    .map_err(|_| app_detail("compat native completion", "session lock poisoned"))?
                    .ledger
                    .data[Direction::MainToRenderer.index()]
                .as_ref()
                .map(|charge| charge.owner.clone());
                if let Some(caller) = caller {
                    self.complete_native_event(compat, &caller, key, request)?;
                }
                Ok(NativePublicationDisposition::Handled)
            }
            RendererBridgeOperation::Invoke => Err(app_detail(
                "compat native request",
                "numeric request entered named dispatcher",
            )),
        }
    }

    fn preflight_renderer_owned(
        &self,
        compat: &Arc<Mutex<CompatSession>>,
        request: &RendererBridgeRequest,
        key: NativeRouteKey,
        channel: &str,
    ) -> Result<NativePublicationDisposition, HostAppError> {
        let caller = self
            .current
            .lock()
            .map_err(|_| app_detail("compat preflight", "generation lock poisoned"))?
            .as_ref()
            .map(|active| PrimaryCallerContext {
                identity: active.identity,
                attempt: active.attempt,
                reader_stop: Arc::clone(&active.reader_stop),
            });
        let mut accepted = false;
        if let Some(caller) = &caller {
            with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
                if !preflight_role_matches(caller, request.role_witness()) {
                    return Ok(());
                }
                let Some(windows) = &self.windows else {
                    return Ok(());
                };
                let windows = windows
                    .lock()
                    .map_err(|_| app_detail("compat preflight", "Registry lock poisoned"))?;
                let Some(route) = Self::checked_request_route(&windows, caller, request, key)
                else {
                    return Ok(());
                };
                let mut state = compat
                    .lock()
                    .map_err(|_| app_detail("compat preflight", "ledger lock poisoned"))?;
                if !state.subscribed_to(caller)
                    || state.ledger.occupied(Direction::RendererToMain)
                    || state.ledger.preflight.is_some()
                {
                    return Ok(());
                }
                #[expect(
                    clippy::manual_let_else,
                    reason = "Keep decoder-result destruction in the existing guarded rejection branch."
                )]
                let event = match borrowed_renderer_event(request, &route, key, channel, &[]) {
                    Ok(event) => event,
                    Err(_) => return Ok(()),
                };
                accepted = preflight_privileged_query(
                    state.verified.manifest(),
                    principal(&route),
                    query(&state, &route, channel),
                )
                .is_ok()
                    && keld_ipc::codec::encoded_len(&event)
                        .is_ok_and(|length| length <= compat_control::COMPAT_DATA_FRAME_BYTES);
                if accepted {
                    state.ledger.preflight = Some(PreflightAdmission {
                        caller: caller.clone(),
                        document: route.document,
                        call: key.message,
                        channel: channel.to_owned(),
                    });
                }
                Ok(())
            })?;
        }
        compat
            .lock()
            .map_err(|_| app_detail("compat preflight outcome", "ledger lock poisoned"))?
            .send(RendererBridgeOutcome::Preflight {
                webview: request.webview(),
                navigation: request.navigation(),
                request: key.message,
                accepted,
            })?;
        Ok(NativePublicationDisposition::Handled)
    }

    pub(super) fn checked_request_route(
        windows: &crate::app_session::window_router::WindowSession,
        caller: &PrimaryCallerContext,
        request: &RendererBridgeRequest,
        key: NativeRouteKey,
    ) -> Option<RendererRoute> {
        let document = request
            .document_lease()
            .filter(|lease| lease.is_current() && exact_document(lease, key))?;
        windows
            .renderer_route_for_caller(caller, request.webview(), Some(key.navigation))
            .ok()
            .filter(|route| route.document.same_binding(document))
    }
}
