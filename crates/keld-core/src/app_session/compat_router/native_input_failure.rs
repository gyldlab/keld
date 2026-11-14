//! Exact-source classification and one retained failed Host completion.
//! No generic failure is reinterpreted as writer/EOF evidence or replayed.
use super::native_event_retirement::original_role_matches;
use super::publication::{RendererFailureDisposition, RendererRequestDisposition};
use super::{
    Arc, Charge, Direction, HostAppError, MacNativeDocumentLease, NativeRouteKey, Ordering,
    PrimaryCallerContext, PrimaryRouterHandle, RendererBridgeOperation, RendererBridgeRequest,
    RendererRoleWitness, WebviewId, app_detail, key, native_data, same_caller,
};
#[cfg(all(test, debug_assertions))]
use super::{CorrelationId, FrameKind, RendererBridgeOutcome, compat_control};
use crate::app_session::window_router::NativePublicationDisposition;

pub(super) struct NativeInputFailureSource {
    caller: PrimaryCallerContext,
    key: NativeRouteKey,
    original_call: Option<u32>,
    document: MacNativeDocumentLease,
}

fn request_role(request: &RendererBridgeRequest) -> Option<&RendererRoleWitness> {
    match request.operation() {
        RendererBridgeOperation::EventRetired { retirement } => Some(retirement.role()),
        _ => request.role_witness(),
    }
}

fn record_matches(charge: &Charge, source: &NativeInputFailureSource) -> bool {
    charge.key == source.key
        && charge.owned_by(&source.caller)
        && charge
            .document
            .as_ref()
            .is_some_and(|held| held.same_binding(&source.document))
}

enum FailureAction {
    Transferred,
    CurrentFatal,
    Retired,
}

impl PrimaryRouterHandle {
    /// Capture the actual existing caller before any effectful dispatch. An
    /// absent/ambiguous source cannot later be replaced with today's caller.
    pub(super) fn capture_renderer_input_source(
        &self,
        request: &RendererBridgeRequest,
    ) -> Result<Option<NativeInputFailureSource>, HostAppError> {
        let (Some(key), Some(document), Some(role)) = (
            key(request),
            request.document_lease(),
            request_role(request),
        ) else {
            return Ok(None);
        };
        let original_call = match request.operation() {
            RendererBridgeOperation::Send { original_call, .. } => Some(*original_call),
            RendererBridgeOperation::Preflight { .. }
            | RendererBridgeOperation::PreflightAborted => Some(request.request()),
            _ => None,
        };
        let _transition = self.shutdown.transition_guard();
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail("renderer input source", "current lock poisoned"))?;
        if let Some(compat) = &self.compat {
            let state = compat
                .lock()
                .map_err(|_| app_detail("renderer input source", "ledger lock poisoned"))?;
            if let Some(charge) = state.ledger.data.iter().flatten().find(|charge| {
                charge.key == key
                    && original_role_matches(role, charge)
                    && charge
                        .document
                        .as_ref()
                        .is_some_and(|held| held.same_binding(document))
            }) {
                return Ok(Some(NativeInputFailureSource {
                    caller: charge.owner.clone(),
                    key,
                    original_call,
                    document: document.clone(),
                }));
            }
            if let Some(preflight) = state.ledger.preflight.as_ref().filter(|pending| {
                original_call == Some(pending.call)
                    && pending.document.same_binding(document)
                    && role.same_role(&RendererRoleWitness::new(Arc::clone(
                        &pending.caller.reader_stop,
                    )))
            }) {
                return Ok(Some(NativeInputFailureSource {
                    caller: preflight.caller.clone(),
                    key,
                    original_call,
                    document: document.clone(),
                }));
            }
        }
        let Some(active) = current.as_ref() else {
            return Ok(None);
        };
        let caller = PrimaryCallerContext {
            identity: active.identity,
            attempt: active.attempt,
            reader_stop: Arc::clone(&active.reader_stop),
        };
        if !role.same_role(&RendererRoleWitness::new(Arc::clone(&caller.reader_stop))) {
            return Ok(None);
        }
        let Some(windows) = &self.windows else {
            return Ok(None);
        };
        let windows = windows
            .lock()
            .map_err(|_| app_detail("renderer input source", "Registry lock poisoned"))?;
        if windows
            .observed_registered_renderer_route(&caller, request.webview(), document)
            .is_none()
        {
            return Ok(None);
        }
        Ok(Some(NativeInputFailureSource {
            caller,
            key,
            original_call,
            document: document.clone(),
        }))
    }

    #[expect(
        clippy::too_many_lines,
        reason = "One exact source cut classifies Fatal or transfers the fixed input into its original charged record."
    )]
    pub(super) fn retain_renderer_input_failure(
        &self,
        retained: &mut Option<RendererBridgeRequest>,
        source: Option<&NativeInputFailureSource>,
        cause: HostAppError,
    ) -> RendererRequestDisposition {
        // This closure keeps the original cause outside every fallible lock/send.
        let classified = (|| -> Result<FailureAction, HostAppError> {
            let Some(source) = source else {
                return Ok(FailureAction::Retired);
            };
            let _transition = self.shutdown.transition_guard();
            let current = self
                .current
                .lock()
                .map_err(|_| app_detail("renderer input failure", "current lock poisoned"))?;
            let registry_match = if let Some(windows) = &self.windows {
                windows
                    .lock()
                    .map_err(|_| app_detail("renderer input failure", "Registry lock poisoned"))?
                    .observed_registered_renderer_route(
                        &source.caller,
                        WebviewId(source.key.webview),
                        &source.document,
                    )
                    .is_some()
            } else {
                false
            };
            let mut state = self
                .compat
                .as_ref()
                .map(|compat| {
                    compat
                        .lock()
                        .map_err(|_| app_detail("renderer input failure", "ledger lock poisoned"))
                })
                .transpose()?;
            let record_match = state.as_ref().is_some_and(|state| {
                state
                    .ledger
                    .data
                    .iter()
                    .flatten()
                    .any(|charge| record_matches(charge, source))
                    || state.ledger.preflight.as_ref().is_some_and(|pending| {
                        same_caller(&pending.caller, &source.caller)
                            && source.original_call == Some(pending.call)
                            && pending.document.same_binding(&source.document)
                    })
            });
            let current_source = self.shutdown.is_running()
                && current.as_ref().is_some_and(|active| {
                    active.identity == source.caller.identity
                        && active.attempt == source.caller.attempt
                        && Arc::ptr_eq(&active.reader_stop, &source.caller.reader_stop)
                        && !active.reader_stop.load(Ordering::Acquire)
                });
            if current_source && (record_match || registry_match) {
                self.window_commands
                    .send(keld_wv::AppWindowCommand::Fatal)
                    .map_err(|_| {
                        app_detail("renderer input failure", "window command owner ended")
                    })?;
                return Ok(FailureAction::CurrentFatal);
            }
            // Only this exact fixed Host failure may move into its SAME original
            // DATA record. All other ambiguous inputs remain attached and blocked.
            let transferable = retained.as_ref().is_some_and(|request| {
                request.has_fixed_event_failure_metadata()
                    && key(request) == Some(source.key)
                    && request
                        .document_lease()
                        .is_some_and(|lease| lease.same_binding(&source.document))
                    && request.role_witness().is_some_and(|role| {
                        role.same_role(&RendererRoleWitness::new(Arc::clone(
                            &source.caller.reader_stop,
                        )))
                    })
            });
            let Some(charge) = state
                .as_mut()
                .and_then(|state| state.ledger.data[Direction::MainToRenderer.index()].as_mut())
            else {
                return Ok(FailureAction::Retired);
            };
            if !transferable
                || !record_matches(charge, source)
                || !charge.native_done
                || charge.retained_native_failure.is_some()
                || charge.native_storage.is_none()
                || charge.retained < native_data::DATA_STORAGE_METADATA_BYTES
            {
                return Ok(FailureAction::Retired);
            }
            let Some(request) = RendererBridgeRequest::take_fixed_event_failure(retained) else {
                return Ok(FailureAction::Retired);
            };
            // The lossless fixed form moves the same identifiers/lease/role, with
            // no payload/fact/token allocation to discard. Refusal keeps current;
            // successful conversion has no fallible step before this SAME row owns it.
            charge.retained_native_failure = Some(request);
            Ok(FailureAction::Transferred)
        })();
        match classified {
            Ok(FailureAction::Transferred) => {
                RendererRequestDisposition::Complete(NativePublicationDisposition::Handled)
            }
            Ok(FailureAction::CurrentFatal) => RendererRequestDisposition::RetainedFailure {
                cause,
                disposition: RendererFailureDisposition::FatalQueued,
            },
            Ok(FailureAction::Retired) => RendererRequestDisposition::RetainedFailure {
                cause,
                disposition: RendererFailureDisposition::RetiredSource,
            },
            Err(cleanup) => RendererRequestDisposition::RetainedFailure {
                cause: crate::app_session::append_app_cleanup(cause, [Err(cleanup)]),
                disposition: RendererFailureDisposition::RetiredSource,
            },
        }
    }
}

#[cfg(all(test, debug_assertions))]
mod tests;
