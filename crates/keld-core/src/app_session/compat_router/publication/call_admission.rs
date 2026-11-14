//! One final Guard/Registry policy cut. No encoding or physical writer wait here.
use super::call::{CompatCallAdmission, CompatCallAnswer};
use super::{
    Arc, CompatControlRequest, CompatSession, ConsumerDisposition, CorrelationId, Direction,
    HostAppError, Mutex, NativeRouteKey, PrimaryCallerContext, PrimaryRouterHandle,
    RendererBridgeOutcome, RendererChannelQuery, WebviewId, app_detail, dispatch_privileged_query,
    native_terminal, principal, with_primary_caller_locked,
};

impl PrimaryRouterHandle {
    #[expect(
        clippy::too_many_lines,
        reason = "One Guard and Registry cut commits the same source record and exact native outcome without intermediate ownership."
    )]
    pub(super) fn admit_compat_request_owned(
        &self,
        compat: &Arc<Mutex<CompatSession>>,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        request: CompatControlRequest,
    ) -> Result<Option<CompatCallAdmission>, HostAppError> {
        let mut request = Some(request);
        let mut admission = None;
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |_| {
            let request = request
                .take()
                .ok_or_else(|| app_detail("compat CALL effect", "source request disappeared"))?;
            if let CompatControlRequest::ReleaseConsumed {
                webview,
                navigation,
                message,
                disposition,
            } = request
            {
                let key = NativeRouteKey {
                    webview,
                    navigation,
                    message,
                };
                let windows = self
                    .windows
                    .as_ref()
                    .map(|windows| {
                        windows.lock().map_err(|_| {
                            app_detail("compat consumer receipt", "Registry lock poisoned")
                        })
                    })
                    .transpose()?;
                let mut state = compat
                    .lock()
                    .map_err(|_| app_detail("compat consumer receipt", "ledger lock poisoned"))?;
                let matched = state.ledger.data[Direction::RendererToMain.index()]
                    .as_ref()
                    .is_some_and(|charge| {
                        charge.key == key && charge.owned_by(caller) && !charge.consumer_done
                    });
                if matched {
                    let additional = match &disposition {
                        ConsumerDisposition::Refused { error } => error
                            .code
                            .capacity()
                            .checked_add(error.message.capacity())
                            .ok_or_else(|| {
                                app_detail(
                                    "compat consumer receipt",
                                    "diagnostic capacity overflow",
                                )
                            })?,
                        _ => 0,
                    };
                    let window = state.ledger.data[Direction::RendererToMain.index()]
                        .as_ref()
                        .and_then(|record| record.native_window);
                    let control = if let Some(window) = window.filter(|window| {
                        windows
                            .as_ref()
                            .is_some_and(|windows| windows.native_offers_open_for(caller, *window))
                    }) {
                        Some(
                            state
                                .lease_existing_control_for_window(
                                    caller,
                                    correlation.0,
                                    window,
                                    additional,
                                    Arc::clone(compat),
                                )
                                .ok_or_else(|| {
                                    app_detail(
                                        "compat consumer terminal",
                                        "original CONTROL custody is unavailable",
                                    )
                                })?,
                        )
                    } else {
                        None
                    };
                    state
                        .ledger
                        .consumer_released(Direction::RendererToMain, key, caller);
                    if let Some(control) = control {
                        state.send(RendererBridgeOutcome::SendResult {
                            webview: WebviewId(webview),
                            navigation: u64::from(navigation),
                            request: message,
                            disposition: native_terminal(disposition),
                            control: Some(control),
                        })?;
                    }
                }
                admission = Some(CompatCallAdmission::Immediate(CompatCallAnswer::Released));
                return Ok(());
            }
            let (webview, navigation, topic, value) = match request {
                CompatControlRequest::DeliverToCurrentMain {
                    webview,
                    topic,
                    payload,
                } => (webview, None, topic, payload),
                CompatControlRequest::DeliverToOriginalDocument {
                    webview,
                    navigation,
                    topic,
                    payload,
                } => (webview, Some(navigation), topic, payload),
                CompatControlRequest::ReleaseConsumed { .. } => {
                    return Err(app_detail("compat CALL effect", "release source changed"));
                }
            };
            let windows = self
                .windows
                .as_ref()
                .map(|windows| {
                    windows
                        .lock()
                        .map_err(|_| app_detail("compat delivery", "Registry lock poisoned"))
                })
                .transpose()?;
            let route = windows.as_ref().and_then(|windows| {
                windows
                    .renderer_route_for_caller(caller, WebviewId(webview), navigation)
                    .ok()
            });
            let mut state = compat
                .lock()
                .map_err(|_| app_detail("compat delivery", "ledger lock poisoned"))?;
            if !state.subscribed_to(caller) {
                admission = Some(CompatCallAdmission::Immediate(
                    CompatCallAnswer::Unsubscribed,
                ));
                return Ok(());
            }
            let Some(route) = route.filter(|route| route.document.is_current()) else {
                admission = Some(CompatCallAdmission::Immediate(CompatCallAnswer::Stale));
                return Ok(());
            };
            if state.ledger.occupied(Direction::MainToRenderer) {
                admission = Some(CompatCallAdmission::Immediate(CompatCallAnswer::Capacity));
                return Ok(());
            }
            let key = NativeRouteKey {
                webview,
                navigation: route.document.stamp().observed_generation(),
                message: correlation.0,
            };
            // The only extra source copy is the <=256-byte topic required while
            // the same Guard query borrows it. Both representations are charged.
            let retained = topic
                .capacity()
                .checked_add(topic.len())
                .and_then(|bytes| bytes.checked_add(value.capacity()))
                .ok_or_else(|| app_detail("compat delivery", "source backing capacity overflow"))?;
            let CompatSession {
                verified,
                ledger,
                outcomes,
                ..
            } = &mut *state;
            let guarded = dispatch_privileged_query(
                verified.manifest(),
                principal(&route),
                RendererChannelQuery {
                    channel: &topic,
                    current_window: verified
                        .manifest()
                        .window_channel_grants(route.window_key()),
                },
                |_| {
                    if !route.document.is_current() {
                        return Ok(CompatCallAdmission::Immediate(CompatCallAnswer::Stale));
                    }
                    ledger
                        .reserve(Direction::MainToRenderer, key, caller, retained)
                        .map_err(|_| {
                            app_detail(
                                "compat DATA admission",
                                "bounded native DATA lane is occupied",
                            )
                        })?;
                    let charge = ledger.data[Direction::MainToRenderer.index()]
                        .as_mut()
                        .ok_or_else(|| {
                            app_detail("compat DATA admission", "reserved source disappeared")
                        })?;
                    charge.document = Some(route.document.clone());
                    charge.native_window = Some(route.window);
                    charge.pending_reply = Some(correlation);
                    let Some(storage) =
                        ledger.lease_native_data(caller, key, &route.document, Arc::clone(compat))
                    else {
                        ledger.data[Direction::MainToRenderer.index()] = None;
                        return Err(app_detail(
                            "compat native DATA",
                            "original source cannot retain native backing metadata",
                        ));
                    };
                    let outcome = RendererBridgeOutcome::Event {
                        webview: WebviewId(webview),
                        navigation: u64::from(key.navigation),
                        request: key.message,
                        channel: topic.clone(),
                        payload: value,
                        document: route.document.clone(),
                        role: self.renderer_role_witness_for_window(caller, route.window),
                        storage: Some(storage),
                    };
                    if let Err(error) = outcomes.send(outcome) {
                        // This exact outcome was never queued; drop its real backing
                        // before canceling only the just-reserved, never-dispatched source.
                        drop(error.0);
                        ledger.data[Direction::MainToRenderer.index()] = None;
                        return Err(app_detail(
                            "compat native event",
                            "existing native endpoint ended before DATA transfer",
                        ));
                    }
                    Ok(CompatCallAdmission::Deferred)
                },
            );
            admission = Some(match guarded {
                Ok(result) => result?,
                Err(reason) => CompatCallAdmission::Immediate(CompatCallAnswer::Denied(reason)),
            });
            Ok(())
        })?;
        Ok(admission)
    }
}
