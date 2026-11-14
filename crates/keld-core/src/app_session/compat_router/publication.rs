//! Windows Compat sources share the existing Window publication turn and writer.
use super::{
    Arc, CallError, CompatControlRequest, CompatLedger, CompatSession, ConsumerDisposition,
    CorrelationId, Direction, FrameKind, HostAppError, LifecycleResponse, MAX_CONTROL_BYTES,
    MacNativeDocumentLease, Mutex, NativeRouteKey, PreflightAdmission, PrimaryCallerContext,
    PrimaryRouterHandle, RendererBridgeOperation, RendererBridgeOutcome, RendererBridgeRequest,
    RendererChannelQuery, RendererRoute, SubscribeRefusal, WebviewId, WindowOwner,
    WvTerminalDisposition, app_detail, app_ipc, compat_control, dispatch_privileged_query,
    exact_document, key, native_terminal, preflight_privileged_query, preflight_role_matches,
    principal, query, refused, same_caller, with_primary_caller_locked,
};
use crate::app_session::primary_writer::{ControlMessage, WriterLink, admit_locked};
use crate::app_session::window_router::{
    NativePublicationDisposition, PublicationResult, WindowPublicationTurn,
};
use std::time::Instant;

mod call;
mod call_admission;
mod native_document;
mod native_input;
mod native_send;
mod retired_routes;
mod route_control;
mod routes;
mod subscription;
pub(in crate::app_session) use native_input::{
    RendererFailureDisposition, RendererRequestDisposition,
};

/// Queue admission is not a native post or consumer completion receipt.
pub(in crate::app_session) enum NativeControlPublication {
    Enqueued,
    Stale,
    Full,
}

fn same_route(
    windows: &super::super::window_router::WindowSession,
    caller: &PrimaryCallerContext,
    captured: &RendererRoute,
) -> bool {
    windows
        .renderer_route_for_caller(
            caller,
            captured.document.stamp().view(),
            Some(captured.document.stamp().observed_generation()),
        )
        .is_ok_and(|current| {
            current.window == captured.window && current.document.same_binding(&captured.document)
        })
}
