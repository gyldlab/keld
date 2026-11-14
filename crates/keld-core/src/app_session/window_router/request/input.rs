//! Canonical Window pre-decode policy; platform writers consume typed results.
use super::{
    ActivePrimaryGeneration, CorrelationId, HostAppError, IpcError, Ordering, PrimaryRouterHandle,
    WindowRefusal, WindowRequest, WindowSession, app_ipc, decode, owner,
};

pub(in crate::app_session::window_router) enum WindowRequestInput {
    Request(WindowRequest),
    Refused(WindowRefusal),
    DuplicateCorrelation,
}
impl PrimaryRouterHandle {
    pub(in crate::app_session::window_router) fn prepare_window_request_input_locked(
        &self,
        session: Option<&WindowSession>,
        active: &ActivePrimaryGeneration,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<WindowRequestInput, HostAppError> {
        if let Some(refusal) =
            self.check_window_request_locked(session, active, correlation, payload.len())
        {
            return Ok(refusal);
        }
        decode(payload)
            .map(WindowRequestInput::Request)
            .map_err(|source| app_ipc("window request decode", &source))
    }

    pub(in crate::app_session::window_router) fn check_window_request_locked(
        &self,
        session: Option<&WindowSession>,
        active: &ActivePrimaryGeneration,
        correlation: CorrelationId,
        payload_bytes: usize,
    ) -> Option<WindowRequestInput> {
        let Some(session) = session.filter(|session| session.subscribed == Some(owner(active)))
        else {
            return Some(WindowRequestInput::Refused(WindowRefusal::fixed(
                "KELD-CORE-045",
                "KELD-CORE-045: window link is not subscribed. Complete the accepted window subscription and replay before issuing window calls.",
            )));
        };
        if !self.window_ready.load(Ordering::Acquire) {
            return Some(WindowRequestInput::Refused(WindowRefusal::fixed(
                "KELD-CORE-040",
                "KELD-CORE-040: registry is not ready. Wait for host Ready and accepted subscription/replay.",
            )));
        }
        if !session.native_supported {
            return Some(WindowRequestInput::Refused(WindowRefusal::fixed(
                "KELD-CORE-042",
                "KELD-CORE-042: native window calls are unsupported by this platform adapter. Use a qualified window backend.",
            )));
        }
        if session
            .registry
            .has_pending_call(owner(active), correlation)
        {
            return Some(WindowRequestInput::DuplicateCorrelation);
        }
        if payload_bytes > keld_ipc::window::MAX_ORIGINAL_LOAD_REQUEST_BYTES {
            return Some(WindowRequestInput::Refused(WindowRefusal::fixed(
                "KELD-CORE-051",
                "KELD-CORE-051: window call exceeds the bounded original-load request size; send the exact selected original URI.",
            )));
        }
        None
    }
}

pub(in crate::app_session::window_router) fn duplicate_correlation() -> HostAppError {
    app_ipc(
        "window correlation admission",
        &IpcError::Protocol {
            detail: "window CALL reuses an already-pending correlation",
        },
    )
}
