//! Private window service composed by the existing primary app-link owner.
//!
//! Lock order is shutdown transition -> current generation/writer -> windows.
//! In particular the windows guard is dropped before failed-write retirement,
//! because retirement re-enters it to fence the old owner. No native handle,
//! frame parser, output queue, principal or subscription policy is minted here.

#[cfg(any(not(windows), test))]
use keld_ipc::ChannelId;
use keld_ipc::SubscribeRefusal;
use keld_ipc::channel_table::{Authority, ReceiveClass};
use keld_ipc::window::{Rect, WindowRequest, WindowResponse, WindowState};
use keld_wv::{BuildStage, WindowSnapshot};

mod native_outcome;
#[cfg(windows)]
mod publication;
#[cfg(windows)]
pub(super) use publication::DesktopStartup;
#[cfg(windows)]
mod publication_turn;
#[cfg(windows)]
pub(super) use publication::NativePublicationDisposition;
#[cfg(windows)]
pub(super) mod original;
#[cfg(any(not(windows), test))]
use native_outcome::NativeOutcome;
mod request;
use request::{WindowRequestInput, WindowRequestOutcome, duplicate_correlation};

use super::window_registry::{
    AbandonedClose, BuildFailureOutcome, BuildOutcome, CloseOutcome, CreateOutcome, FactOutcome,
    NativeSetter, OwnedWindowEvent, ReleaseOutcome, SessionEffect, WindowError, WindowOwner,
    WindowRegistry, WindowReply,
};
use super::{
    ActivePrimaryGeneration, AppWindowCommand, AppWindowEvent, CallError, CorrelationId, FrameKind,
    HostAppError, IpcError, LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleResponse, Ordering,
    PrimaryCallerContext, PrimaryRouterHandle, app_detail, app_ipc, append_app_cleanup,
    collapse_app_results, decode, with_primary_caller_locked,
};

#[cfg(any(not(windows), test))]
use super::{encode, write_frame};

// Current maximum payloads: WindowEvent Created = 50 bytes, WindowResponse
// Created/Adopted = 16, LifecycleResponse SubscribeRefused = 2. The literal
// codec vectors and maximum Created socket test qualify the current schema.
const WINDOW_PAYLOAD_STORAGE_BYTES: usize = 64;

/// One optional facade service for the whole app session. None in the parent is
/// the permanent renderer mode; current production boot constructors pass None.
pub(super) struct WindowSession {
    registry: WindowRegistry,
    native_supported: bool,
    #[cfg(windows)]
    original_selection: Option<std::sync::Arc<super::original_webapp::OriginalLoadSelection>>,
    #[cfg(windows)]
    desktop: Option<publication::SelectedDesktop>,
    #[cfg(windows)]
    compat: Option<std::sync::Arc<super::compat_router::CompatSession>>,
    // The one subscription owner is this live link, not the window instances.
    subscribed: Option<WindowOwner>,
    #[cfg(windows)]
    publication: publication_turn::WindowPublicationState,
}
impl WindowSession {
    pub(super) fn new(native_supported: bool) -> Self {
        Self {
            registry: WindowRegistry::new(),
            native_supported,
            #[cfg(windows)]
            original_selection: None,
            #[cfg(windows)]
            desktop: None,
            #[cfg(windows)]
            compat: None,
            subscribed: None,
            #[cfg(windows)]
            publication: publication_turn::WindowPublicationState::default(),
        }
    }

    #[cfg(windows)]
    pub(super) fn with_original_selection(
        mut self,
        selection: Option<std::sync::Arc<super::original_webapp::OriginalLoadSelection>>,
    ) -> Self {
        self.desktop = selection
            .is_some()
            .then(publication::SelectedDesktop::default);
        self.original_selection = selection;
        self
    }

    #[cfg(windows)]
    pub(super) fn validate_original_runtime_versions(
        &self,
        versions: Option<&std::collections::BTreeMap<String, String>>,
    ) -> Result<(), HostAppError> {
        self.original_selection
            .as_ref()
            .map_or(Ok(()), |selection| {
                selection.validate_runtime_versions(versions)
            })
    }

    #[cfg(windows)]
    pub(super) fn create_original_for_caller(
        &mut self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        size: (u32, u32),
        position: (Option<i32>, Option<i32>),
        resources: super::window_registry::OriginalWindowResources,
    ) -> Result<CreateOutcome, WindowError> {
        self.registry
            .create_original_for_caller(caller, correlation, size, position, resources)
    }

    #[cfg(windows)]
    pub(super) fn current_original_binding(
        &self,
        caller: &PrimaryCallerContext,
        window: keld_ipc::window::WindowRef,
    ) -> bool {
        self.registry.current_original_binding(caller, window)
    }

    #[cfg(windows)]
    pub(super) fn original_load_can_enter_native(
        &self,
        caller: &PrimaryCallerContext,
        window: keld_ipc::window::WindowRef,
        context: keld_wv::NativeNavigationContext,
    ) -> bool {
        self.registry
            .original_load_can_enter_native(caller, window, context)
    }
    #[cfg(windows)]
    pub(super) fn bind_compat_writer(
        &mut self,
        writer: &super::primary_writer::PrimaryWriter,
    ) -> Result<(), HostAppError> {
        if self.original_selection.is_none() {
            return Ok(());
        }
        if self.compat.is_some() {
            return Err(app_detail(
                "compat CONTROL owner",
                "session writer is already bound",
            ));
        }
        self.compat = Some(std::sync::Arc::new(
            super::compat_router::CompatSession::new(writer.clone()),
        ));
        Ok(())
    }

    #[cfg(windows)]
    pub(super) fn compat(&self) -> Option<&std::sync::Arc<super::compat_router::CompatSession>> {
        self.compat.as_ref()
    }

    #[cfg(windows)]
    pub(super) fn native_view_scope_status(
        &self,
        window: keld_ipc::window::WindowRef,
    ) -> Option<bool> {
        self.registry.native_view_scope_status(window)
    }

    #[cfg(windows)]
    pub(super) fn close_native_view_offers(&mut self, window: keld_ipc::window::WindowRef) -> bool {
        self.registry.close_native_view_offers(window)
    }
    #[cfg(windows)]
    pub(super) fn note_native_registration(
        &mut self,
        caller: &PrimaryCallerContext,
        view: keld_wv::WebviewId,
        document: &keld_wv::renderer_contract::CheckedNativeDocumentLease,
    ) -> Result<(), WindowError> {
        self.compat.as_ref().ok_or(WindowError::NotReady)?;
        self.registry
            .note_native_registration(caller, view, document)
    }
    #[cfg(any(target_os = "macos", windows))]
    pub(super) fn admitted_dialog_view(
        &mut self,
        owner: WindowOwner,
        window: keld_ipc::window::WindowRef,
    ) -> Result<keld_wv::WebviewId, WindowError> {
        if self.subscribed != Some(owner) {
            return Err(WindowError::WrongOwner);
        }
        if !self.native_supported {
            return Err(WindowError::NotReady);
        }
        self.registry.admitted_dialog_view(owner, window)
    }
}

/// Resolution precedes this classification. Metadata comes from the one table;
/// only component tests can provide otherwise unallocated metadata combinations.
fn subscription_refusal(
    class: ReceiveClass,
    authority: Authority,
    lifecycle: bool,
    primary: bool,
) -> Option<SubscribeRefusal> {
    if !class.carries_host_events() || lifecycle {
        Some(SubscribeRefusal::NotSubscribable)
    } else if !matches!(authority, Authority::HostInternal) {
        Some(SubscribeRefusal::GuardedNotSupported)
    } else if !primary {
        Some(SubscribeRefusal::RoleNotAdmitted)
    } else {
        None
    }
}
fn window_subscribe_refusal(
    channel: u16,
    primary: bool,
    has_registry: bool,
) -> Option<SubscribeRefusal> {
    let entry = keld_ipc::channel_table::CHANNEL_TABLE
        .iter()
        .find(|entry| entry.wire_id() == channel);
    let refusal = entry.map_or(Some(SubscribeRefusal::UnknownChannel), |entry| {
        subscription_refusal(
            entry.class(),
            entry.authority(),
            entry.id() == LIFECYCLE_CHANNEL,
            primary,
        )
    });
    refusal.or_else(|| {
        (channel != keld_ipc::channel_table::WINDOW.wire_id() || !has_registry)
            .then_some(SubscribeRefusal::NotSubscribable)
    })
}
fn owner(active: &ActivePrimaryGeneration) -> WindowOwner {
    WindowOwner::new(active.identity, active.attempt)
}
#[cfg(any(not(windows), test))]
fn active_owner(
    current: &mut Option<ActivePrimaryGeneration>,
    expected: WindowOwner,
) -> Option<&mut ActivePrimaryGeneration> {
    current
        .as_mut()
        .filter(|active| owner(active) == expected && !active.reader_stop.load(Ordering::Acquire))
}
#[cfg(any(not(windows), test))]
fn write_payload(
    active: &mut ActivePrimaryGeneration,
    kind: FrameKind,
    channel: ChannelId,
    correlation: CorrelationId,
    payload: &[u8],
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    #[cfg(windows)]
    let stream = active.legacy_writer_mut()?;
    #[cfg(not(windows))]
    let stream = &mut active.writer;
    write_frame(stream, kind, 0, channel, correlation, payload).map_err(|source| {
        *failed = Some(active.attempt);
        app_ipc("window ordered writer", &source)
    })
}
#[cfg(any(not(windows), test))]
fn window_reply(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    reply: WindowResponse,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let mut scratch = [0_u8; WINDOW_PAYLOAD_STORAGE_BYTES];
    let bytes = keld_ipc::codec::encode_into(&reply, &mut scratch)
        .map_err(|source| app_ipc("window reply encode", &source))?;
    write_payload(
        active,
        FrameKind::Reply,
        keld_ipc::channel_table::WINDOW.id(),
        correlation,
        bytes,
        failed,
    )
}
#[cfg(any(not(windows), test))]
fn lifecycle_reply(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    reply: LifecycleResponse,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let mut scratch = [0_u8; WINDOW_PAYLOAD_STORAGE_BYTES];
    let bytes = keld_ipc::codec::encode_into(&reply, &mut scratch)
        .map_err(|source| app_ipc("window Subscribe reply encode", &source))?;
    write_payload(
        active,
        FrameKind::Reply,
        LIFECYCLE_CHANNEL,
        correlation,
        bytes,
        failed,
    )
}
#[cfg(any(not(windows), test))]
fn call_error(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    code: &'static str,
    message: String,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    let bytes = encode(&CallError {
        code: code.to_owned(),
        message,
    })
    .map_err(|source| app_ipc("window error encode", &source))?;
    write_payload(
        active,
        FrameKind::Err,
        keld_ipc::channel_table::WINDOW.id(),
        correlation,
        &bytes,
        failed,
    )
}
#[cfg(any(not(windows), test))]
fn event(
    active: &mut ActivePrimaryGeneration,
    fact: keld_ipc::window::WindowEvent,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    // Current WindowEvent's largest value is Created with both u64 maxima:
    // the owning codec test below proves its actual payload fits this storage.
    let mut scratch = [0_u8; WINDOW_PAYLOAD_STORAGE_BYTES];
    let bytes = keld_ipc::codec::encode_into(&fact, &mut scratch)
        .map_err(|source| app_ipc("window fact encode", &source))?;
    write_payload(
        active,
        FrameKind::Event,
        keld_ipc::channel_table::WINDOW.id(),
        CorrelationId(0),
        bytes,
        failed,
    )
}
#[cfg(any(not(windows), test))]
fn owned_event(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    fact: OwnedWindowEvent,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if subscribed == Some(fact.owner)
        && let Some(active) = active_owner(current, fact.owner)
    {
        event(active, fact.event, failed)?;
    }
    Ok(())
}
#[cfg(any(not(windows), test))]
fn owned_reply(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    reply: WindowReply,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if subscribed == Some(reply.owner)
        && let Some(active) = active_owner(current, reply.owner)
    {
        window_reply(active, reply.correlation, reply.response, failed)?;
    }
    Ok(())
}
#[cfg(any(not(windows), test))]
fn last_window_locked(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    recipient: Option<WindowOwner>,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if let Some(recipient) = recipient
        && subscribed == Some(recipient)
        && let Some(active) = active_owner(current, recipient)
    {
        let mut scratch = [0_u8; WINDOW_PAYLOAD_STORAGE_BYTES];
        let bytes = keld_ipc::codec::encode_into(&LifecycleEvent::LastWindowClosed, &mut scratch)
            .map_err(|source| app_ipc("window last-window encode", &source))?;
        write_payload(
            active,
            FrameKind::Event,
            LIFECYCLE_CHANNEL,
            CorrelationId(0),
            bytes,
            failed,
        )?;
        // Registry transitions own managed counts. The parent's retained
        // legacy flag must never synthesize a LastWindowClosed on recovery.
    }
    Ok(())
}

#[cfg(any(not(windows), test))]
fn window_build_failed_locked(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    outcome: BuildFailureOutcome,
    stage: BuildStage,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if outcome.diagnostic {
        native_outcome::report_ignored_build_failure();
    }
    if let Some(call) = outcome.call
        && subscribed == Some(call.owner)
        && let Some(active) = active_owner(current, call.owner)
    {
        let (code, message) = native_outcome::build_refusal(stage).into_parts();
        call_error(active, call.correlation, code, message, failed)?;
    }
    last_window_locked(current, subscribed, outcome.last_window, failed)
}
#[cfg(any(not(windows), test))]
fn window_released_locked(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    outcome: ReleaseOutcome,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    native_outcome::report_release(&outcome);
    if let Some(fact) = outcome.fact {
        owned_event(current, subscribed, fact, failed)?;
    }
    last_window_locked(current, subscribed, outcome.last_window, failed)?;
    for call in outcome.destroy_calls {
        owned_reply(
            current,
            subscribed,
            WindowReply {
                owner: call.owner,
                correlation: call.correlation,
                response: WindowResponse::Destroyed,
            },
            failed,
        )?;
    }
    Ok(())
}

fn report_abandoned(close: AbandonedClose) {
    eprintln!(
        "window {} close_seq {} Abandoned; the pending request will not be replayed",
        close.window.id.0, close.close_seq
    );
}

fn native_state(snapshot: WindowSnapshot) -> WindowState {
    WindowState {
        bounds: Rect {
            x: snapshot.x,
            y: snapshot.y,
            width: snapshot.width,
            height: snapshot.height,
        },
        focused: snapshot.focused,
        maximized: snapshot.maximized,
        full_screen: snapshot.full_screen,
    }
}

impl PrimaryRouterHandle {
    /// Caller already holds the shutdown transition. Ready publication cannot
    /// overtake terminal end or the registry's serviceability transition.
    pub(super) fn mark_window_registry_ready_guarded(&self) -> Result<bool, HostAppError> {
        let _current = self
            .current
            .lock()
            .map_err(|_| app_detail("window Ready", "generation lock poisoned"))?;
        self.windows.as_ref().map_or(Ok(true), |windows| {
            Ok(windows
                .lock()
                .map_err(|_| app_detail("window Ready", "registry lock poisoned"))?
                .registry
                .mark_ready())
        })
    }
    /// Called with the current writer lock held; never holds a windows guard on entry.
    pub(super) fn retire_window_owner(&self, retired: WindowOwner) -> Result<(), HostAppError> {
        if let Some(windows) = &self.windows {
            let mut session = windows
                .lock()
                .map_err(|_| app_detail("window role retirement", "registry lock poisoned"))?;
            if session.subscribed == Some(retired) {
                session.subscribed = None;
            }
            #[cfg(windows)]
            if let Some(desktop) = session.desktop.as_mut() {
                desktop.retire(retired);
            }
            session.registry.role_lost(retired, report_abandoned);
        }
        Ok(())
    }
    #[cfg(any(not(windows), test))]
    fn finish_window_write(
        &self,
        current: &mut Option<ActivePrimaryGeneration>,
        failed: Option<u32>,
        result: Result<(), HostAppError>,
    ) -> Result<(), HostAppError> {
        match (failed, result) {
            (Some(attempt), Err(primary)) => {
                // No windows guard survives the operation closure. Writer loss or
                // an owned-correlation violation retires this link before unlock.
                eprintln!("{primary}");
                let retirement = self.retire_current_generation_locked(
                    current,
                    attempt,
                    "window session failed",
                    "failed window session close",
                );
                Err(append_app_cleanup(primary, [retirement]))
            }
            (_, result) => result,
        }
    }
    pub(super) fn subscribe_window(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        channel: u16,
    ) -> Result<(), HostAppError> {
        #[cfg(all(windows, not(test)))]
        {
            self.subscribe_window_ordered(caller, correlation, channel)
        }
        #[cfg(any(not(windows), test))]
        {
            #[cfg(windows)]
            if self.uses_primary_writer() {
                return self.subscribe_window_ordered(caller, correlation, channel);
            }
            self.subscribe_window_legacy(caller, correlation, channel)
        }
    }
    #[cfg(any(not(windows), test))]
    fn subscribe_window_legacy(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        channel: u16,
    ) -> Result<(), HostAppError> {
        let mut failed = None;
        let result = with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let result = (|| {
                let active = current.as_mut().ok_or_else(|| {
                    app_detail("window Subscribe", "validated generation disappeared")
                })?;
                if let Some(reason) = window_subscribe_refusal(
                    channel,
                    caller.identity.is_primary(),
                    self.windows.is_some(),
                ) {
                    return lifecycle_reply(
                        active,
                        correlation,
                        LifecycleResponse::SubscribeRefused { reason },
                        &mut failed,
                    );
                }
                let Some(windows) = &self.windows else {
                    return Err(app_detail(
                        "window Subscribe",
                        "resolved window service disappeared",
                    ));
                };
                let mut session = windows
                    .lock()
                    .map_err(|_| app_detail("window Subscribe", "registry lock poisoned"))?;
                let caller_owner = WindowOwner::new(caller.identity, caller.attempt);
                if session.subscribed != Some(caller_owner) {
                    session.registry.subscribe_successor(caller_owner);
                    session.subscribed = Some(caller_owner);
                    // The existing current writer lock makes this complete replay
                    // and the correlated acceptance one ordered stream operation.
                    for fact in session.registry.replay(caller_owner) {
                        event(active, fact, &mut failed)?;
                    }
                }
                lifecycle_reply(
                    active,
                    correlation,
                    LifecycleResponse::Subscribed,
                    &mut failed,
                )
            })();
            self.finish_window_write(current, failed, result)
        });
        if let Some(attempt) = failed {
            // The owner can call back into generation state: release transition,
            // current and windows before asking Runtime to retire/recover the role.
            let owner_result = self.link_failed(attempt);
            collapse_app_results([result, owner_result])
        } else {
            result
        }
    }
    pub(super) fn dispatch_window(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        #[cfg(all(windows, not(test)))]
        {
            self.dispatch_window_ordered(caller, correlation, payload)
        }
        #[cfg(any(not(windows), test))]
        {
            #[cfg(windows)]
            if self.uses_primary_writer() {
                return self.dispatch_window_ordered(caller, correlation, payload);
            }
            self.dispatch_window_legacy(caller, correlation, payload)
        }
    }
    #[cfg(any(not(windows), test))]
    fn dispatch_window_legacy(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        let mut failed = None;
        let result = with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let result = (|| {
                let active = current.as_mut().ok_or_else(|| {
                    app_detail("window request", "validated generation disappeared")
                })?;
                let mut session = self
                    .windows
                    .as_ref()
                    .map(|windows| {
                        windows
                            .lock()
                            .map_err(|_| app_detail("window request", "registry lock poisoned"))
                    })
                    .transpose()?;
                let request = match self.prepare_window_request_input_locked(
                    session.as_deref(),
                    active,
                    correlation,
                    payload,
                )? {
                    WindowRequestInput::Request(request) => request,
                    WindowRequestInput::Refused(refusal) => {
                        let (code, message) = refusal.into_parts();
                        return call_error(active, correlation, code, message, &mut failed);
                    }
                    WindowRequestInput::DuplicateCorrelation => {
                        failed = Some(active.attempt);
                        return Err(duplicate_correlation());
                    }
                };
                let session = session.as_deref_mut().ok_or_else(|| {
                    app_detail("window request", "admitted request has no registry")
                })?;
                let outcome =
                    self.prepare_window_request_locked(session, active, correlation, request)?;
                match outcome {
                    WindowRequestOutcome::Deferred => Ok(()),
                    WindowRequestOutcome::Reply(reply) => {
                        window_reply(active, correlation, reply, &mut failed)
                    }
                    WindowRequestOutcome::Refused(refusal) => {
                        let (code, message) = refusal.into_parts();
                        call_error(active, correlation, code, message, &mut failed)
                    }
                    WindowRequestOutcome::Close(close) => {
                        self.window_close_call_locked(active, correlation, close, &mut failed)
                    }
                    WindowRequestOutcome::Teardown(teardown) => {
                        self.send_window_teardown(teardown, "window teardown")
                    }
                }
            })();
            self.finish_window_write(current, failed, result)
        });
        if let Some(attempt) = failed {
            collapse_app_results([result, self.link_failed(attempt)])
        } else {
            result
        }
    }
    fn send_window_teardown(
        &self,
        teardown: super::window_registry::Teardown,
        phase: &'static str,
    ) -> Result<(), HostAppError> {
        #[cfg(windows)]
        if let Some(dialog) = &self.dialog {
            dialog.parent_retiring(teardown.view)?;
        }
        self.window_commands
            .send(AppWindowCommand::DestroyWindow {
                view: teardown.view,
            })
            .map_err(|_| app_detail(phase, "UI command owner is unavailable"))
    }
    #[cfg(any(not(windows), test))]
    fn window_close_call_locked(
        &self,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        outcome: CloseOutcome,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        match request::prepare_close_reply(outcome)? {
            request::CloseReplyPublication::Reply { response, fact } => {
                if let Some(fact) = fact {
                    event(active, fact.event, failed)?;
                }
                window_reply(active, correlation, response, failed)
            }
            request::CloseReplyPublication::Teardown(teardown) => {
                // The close already committed. Even an unknown acknowledgement
                // cannot strand its physical view.
                let acknowledgement =
                    window_reply(active, correlation, WindowResponse::ReplyApplied, failed);
                let release = self.send_window_teardown(teardown, "window teardown");
                collapse_app_results([acknowledgement, release])
            }
        }
    }
    #[cfg(any(not(windows), test))]
    fn window_build_outcome_locked(
        &self,
        current: &mut Option<ActivePrimaryGeneration>,
        subscribed: Option<WindowOwner>,
        outcome: BuildOutcome,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        match outcome {
            BuildOutcome::Created { fact, reply } => {
                owned_event(current, subscribed, fact, failed)?;
                owned_reply(current, subscribed, reply, failed)
            }
            BuildOutcome::Cleanup(teardown) => {
                self.send_window_teardown(teardown, "window late-build cleanup")
            }
            BuildOutcome::Ignored => {
                native_outcome::report_ignored_build();
                Ok(())
            }
        }
    }
    #[cfg(any(not(windows), test))]
    fn window_native_close_locked(
        current: &mut Option<ActivePrimaryGeneration>,
        subscribed: Option<WindowOwner>,
        outcome: CloseOutcome,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        if let Some(fact) = native_outcome::report_native_close(outcome)? {
            owned_event(current, subscribed, fact, failed)?;
        }
        Ok(())
    }
    // Caller owns transition -> current -> windows. This dispatch performs the
    // registry transition and its ordered effects without changing lock custody.
    #[cfg(any(not(windows), test))]
    fn window_native_event_locked(
        &self,
        session: &mut WindowSession,
        current: &mut Option<ActivePrimaryGeneration>,
        native_event: AppWindowEvent,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let subscribed = session.subscribed;
        let fact = match Self::prepare_window_native_locked(session, native_event)? {
            NativeOutcome::Build(outcome) => {
                return self.window_build_outcome_locked(current, subscribed, outcome, failed);
            }
            NativeOutcome::BuildFailed { outcome, stage } => {
                return window_build_failed_locked(current, subscribed, outcome, stage, failed);
            }
            NativeOutcome::Close(outcome) => {
                return Self::window_native_close_locked(current, subscribed, outcome, failed);
            }
            NativeOutcome::Released(outcome) => {
                return window_released_locked(current, subscribed, outcome, failed);
            }
            NativeOutcome::Fact(fact) => fact,
        };
        if let Some(fact) = native_outcome::report_native_fact(fact) {
            owned_event(current, subscribed, fact, failed)?;
        }
        Ok(())
    }
    #[cfg(any(not(windows), test))]
    pub(super) fn window_native_event(
        &self,
        native_event: AppWindowEvent,
    ) -> Result<(), HostAppError> {
        #[cfg(windows)]
        if self.uses_primary_writer() {
            return match self.window_native_event_ordered(native_event)? {
                NativePublicationDisposition::Handled => Ok(()),
                NativePublicationDisposition::FatalQueued(error) => Err(error),
            };
        }
        let mut failed = None;
        let result = {
            let _transition = self.shutdown.transition_guard();
            let mut current = self
                .current
                .lock()
                .map_err(|_| app_detail("window native event", "generation lock poisoned"))?;
            let result = (|| {
                let windows = self.windows.as_ref().ok_or_else(|| {
                    app_detail(
                        "window native event",
                        "managed native fact arrived without a window registry",
                    )
                })?;
                let mut session = windows
                    .lock()
                    .map_err(|_| app_detail("window native event", "registry lock poisoned"))?;
                self.window_native_event_locked(
                    &mut session,
                    &mut current,
                    native_event,
                    &mut failed,
                )
            })();
            // The closure releases windows before retirement can re-enter it.
            self.finish_window_write(&mut current, failed, result)
        };
        // Recovery belongs to the existing Runtime owner, after releasing every lock.
        if let Some(attempt) = failed {
            let owner_result = self.link_failed(attempt);
            if owner_result.is_ok() {
                return Ok(());
            }
            collapse_app_results([result, owner_result])
        } else {
            result
        }
    }
    /// The existing WV main-owner service invokes this before terminal physical
    /// release. Some is facade-only (GH-446's no-legacy-anchor factory); the
    /// permanent None renderer path does not reinterpret an anchor close.
    #[cfg(any(target_os = "macos", test))]
    pub(super) fn window_main_owner_stopping(&self, stopping: bool) -> Result<(), HostAppError> {
        if !stopping || self.windows.is_none() {
            return Ok(());
        }
        let _transition = self.shutdown.transition_guard();
        // These are the existing synchronous installer fence and Runtime owner
        // request, before Ending or native release. None never enters this path.
        self.shutdown.stop_reader();
        self.guardian.deny_recovery();
        self.end_window_session_guarded()
    }
    /// Caller holds the shutdown transition; existing command queue owns effects.
    pub(super) fn end_window_session_guarded(&self) -> Result<(), HostAppError> {
        let _current = self
            .current
            .lock()
            .map_err(|_| app_detail("window session end", "generation lock poisoned"))?;
        let Some(windows) = &self.windows else {
            return Ok(());
        };
        let mut session = windows
            .lock()
            .map_err(|_| app_detail("window session end", "registry lock poisoned"))?;
        let mut result = Ok(());
        session.registry.session_end(|effect| match effect {
            SessionEffect::Teardown(teardown) => {
                let next = self.send_window_teardown(teardown, "window session teardown");
                result = collapse_app_results([std::mem::replace(&mut result, Ok(())), next]);
            }
            SessionEffect::CreateInterrupted(call) => {
                // Already admitted. There is no no-effect024 reply; closing its
                // actual role link gives022 with unknown effect and no replay.
                eprintln!(
                    "admitted window Create {} interrupted by session end; outcome unknown",
                    call.correlation.0
                );
            }
            SessionEffect::CloseAbandoned(close) => report_abandoned(close),
        });
        result
    }
    /// Only after the actual UI event source has drained, never on a timer.
    pub(super) fn finish_window_facts(&self) -> Result<(), HostAppError> {
        if let Some(windows) = &self.windows {
            let dropped = windows
                .lock()
                .map_err(|_| app_detail("window fact drain", "registry lock poisoned"))?
                .registry
                .take_destroyed_fact_drops();
            if dropped != 0 {
                eprintln!("window session dropped {dropped} retired native facts");
            }
        }
        Ok(())
    }
}

#[cfg(all(test, not(keld_core_native_renderer_main)))]
mod tests;
