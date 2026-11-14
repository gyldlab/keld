//! Windows Window/Compat source ordering on the one session physical writer.
use super::super::primary_writer::{ControlMessage, PreparedFrame, WriterLink, admit_locked};
use super::super::window_registry::{PendingWindowCall, RetainedWindowDocument};
use super::request::{WindowRefusal, WindowRequestOutcome};
use super::{
    ActivePrimaryGeneration, AppWindowEvent, BuildOutcome, CallError, CloseOutcome, CorrelationId,
    FactOutcome, FrameKind, HostAppError, LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleResponse,
    Ordering, OwnedWindowEvent, PrimaryCallerContext, PrimaryRouterHandle, ReleaseOutcome,
    SubscribeRefusal, WINDOW_PAYLOAD_STORAGE_BYTES, WindowError, WindowEvent, WindowOwner,
    WindowPublicationTurn, WindowRegistry, WindowReply, WindowRequest, WindowResponse,
    WindowSession, app_detail, collapse_app_results, owner, report_abandoned, subscription_refusal,
    with_primary_caller_locked,
};
use keld_ipc::window::{WindowEventValue, WindowRef};
use std::sync::Arc;
use std::time::Instant;

mod common;
pub(super) mod desktop;
mod emitter;
mod input;
mod native;
mod native_wait;
mod replay;
mod request;

pub(in crate::app_session) use common::{
    NativePublicationDisposition, PublicationError, PublicationResult,
};
use emitter::WindowEmitter;
