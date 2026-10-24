//! Private window service composed by the existing primary app-link owner.
//!
//! Lock order is shutdown transition -> current generation/writer -> windows.
//! In particular the windows guard is dropped before failed-write retirement,
//! because retirement re-enters it to fence the old owner. No native handle,
//! frame parser, output queue, principal or subscription policy is minted here.

use keld_ipc::channel_table::{Authority, ReceiveClass};
use keld_ipc::window::{Rect, WindowRequest, WindowResponse, WindowState};
use keld_ipc::{ChannelId, SubscribeRefusal};
use keld_wv::{BuildStage, WindowSnapshot};

use super::window_registry::{
    BuildFailureOutcome, BuildOutcome, CloseOutcome, CreateOutcome, FactOutcome, NativeSetter,
    OwnedWindowEvent, ReleaseOutcome, SessionEffect, WindowOwner, WindowRegistry, WindowReply,
};
use super::{
    ActivePrimaryGeneration, AppWindowCommand, AppWindowEvent, CallError, CorrelationId, FrameKind,
    HostAppError, IpcError, LIFECYCLE_CHANNEL, LifecycleEvent, LifecycleResponse, Ordering,
    PrimaryCallerContext, PrimaryRouterHandle, app_detail, app_ipc, append_app_cleanup,
    collapse_app_results, decode, encode, with_primary_caller_locked, write_frame,
};

// Current maximum payloads: WindowEvent Created = 50 bytes, WindowResponse
// Created/Adopted = 16, LifecycleResponse SubscribeRefused = 2. The literal
// codec vectors and maximum Created socket test qualify the current schema.
const WINDOW_PAYLOAD_STORAGE_BYTES: usize = 64;

/// One optional facade service for the whole app session. None in the parent is
/// the permanent renderer mode; current production boot constructors pass None.
pub(super) struct WindowSession {
    registry: WindowRegistry,
    native_supported: bool,
    // The one subscription owner is this live link, not the window instances.
    subscribed: Option<WindowOwner>,
}
impl WindowSession {
    pub(super) fn new(native_supported: bool) -> Self {
        Self {
            registry: WindowRegistry::new(),
            native_supported,
            subscribed: None,
        }
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
fn owner(active: &ActivePrimaryGeneration) -> WindowOwner {
    WindowOwner::new(active.identity, active.attempt)
}
fn active_owner(
    current: &mut Option<ActivePrimaryGeneration>,
    expected: WindowOwner,
) -> Option<&mut ActivePrimaryGeneration> {
    current
        .as_mut()
        .filter(|active| owner(active) == expected && !active.reader_stop.load(Ordering::Acquire))
}
fn write_payload(
    active: &mut ActivePrimaryGeneration,
    kind: FrameKind,
    channel: ChannelId,
    correlation: CorrelationId,
    payload: &[u8],
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    write_frame(&mut active.writer, kind, 0, channel, correlation, payload).map_err(|source| {
        *failed = Some(active.attempt);
        app_ipc("window ordered writer", &source)
    })
}
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

fn window_build_failed_locked(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    outcome: BuildFailureOutcome,
    stage: BuildStage,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
    if outcome.diagnostic {
        eprintln!(
            "KELD-CORE-041: ignored unexpected native build failure; inspect the UI build correlation"
        );
    }
    if let Some(call) = outcome.call
        && subscribed == Some(call.owner)
        && let Some(active) = active_owner(current, call.owner)
    {
        let stage = match stage {
            BuildStage::Window => "window",
            BuildStage::Webview => "webview",
        };
        call_error(
            active,
            call.correlation,
            "KELD-CORE-041",
            format!(
                "KELD-CORE-041: native {stage} creation failed. Inspect the UI owner's typed error and correct the requested window resources."
            ),
            failed,
        )?;
    }
    last_window_locked(current, subscribed, outcome.last_window, failed)
}
fn window_released_locked(
    current: &mut Option<ActivePrimaryGeneration>,
    subscribed: Option<WindowOwner>,
    outcome: ReleaseOutcome,
    failed: &mut Option<u32>,
) -> Result<(), HostAppError> {
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
        owned_event(current, subscribed, fact, failed)?;
    }
    last_window_locked(current, subscribed, outcome.last_window, failed)
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
            session.registry.role_lost(retired);
        }
        Ok(())
    }
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
        let mut failed = None;
        let result = with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let result = (|| {
                let active = current.as_mut().ok_or_else(|| {
                    app_detail("window Subscribe", "validated generation disappeared")
                })?;
                let entry = keld_ipc::channel_table::CHANNEL_TABLE
                    .iter()
                    .find(|entry| entry.wire_id() == channel);
                let refusal = entry.map_or(Some(SubscribeRefusal::UnknownChannel), |entry| {
                    subscription_refusal(
                        entry.class(),
                        entry.authority(),
                        entry.id() == LIFECYCLE_CHANNEL,
                        caller.identity.is_primary(),
                    )
                });
                if let Some(reason) = refusal {
                    return lifecycle_reply(
                        active,
                        correlation,
                        LifecycleResponse::SubscribeRefused { reason },
                        &mut failed,
                    );
                }
                if channel != keld_ipc::channel_table::WINDOW.wire_id() || self.windows.is_none() {
                    return lifecycle_reply(
                        active,
                        correlation,
                        LifecycleResponse::SubscribeRefused {
                            reason: SubscribeRefusal::NotSubscribable,
                        },
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
        let mut failed = None;
        let result = with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let result = (|| {
                let active = current.as_mut().ok_or_else(|| {
                    app_detail("window request", "validated generation disappeared")
                })?;
                let caller_owner = WindowOwner::new(caller.identity, caller.attempt);
                let unavailable = || {
                    "KELD-CORE-045: window link is not subscribed. Complete the accepted window subscription and replay before issuing window calls.".to_owned()
                };
                let Some(windows) = &self.windows else {
                    return call_error(
                        active,
                        correlation,
                        "KELD-CORE-045",
                        unavailable(),
                        &mut failed,
                    );
                };
                let mut session = windows
                    .lock()
                    .map_err(|_| app_detail("window request", "registry lock poisoned"))?;
                //045 precedes readiness, platform and even body decoding. A refused
                //call cannot allocate a pair, enqueue work or mutate adoption marks.
                if session.subscribed != Some(caller_owner) {
                    return call_error(
                        active,
                        correlation,
                        "KELD-CORE-045",
                        unavailable(),
                        &mut failed,
                    );
                }
                if !self.window_ready.load(Ordering::Acquire) {
                    return call_error(active, correlation, "KELD-CORE-040", "KELD-CORE-040: registry is not ready. Wait for host Ready and accepted subscription/replay.".to_owned(), &mut failed);
                }
                if !session.native_supported {
                    return call_error(active, correlation, "KELD-CORE-042", "KELD-CORE-042: native window calls are unsupported by this platform adapter. Use a qualified window backend.".to_owned(), &mut failed);
                }
                if session
                    .registry
                    .has_pending_create(caller_owner, correlation)
                {
                    failed = Some(active.attempt);
                    return Err(app_ipc(
                        "window correlation admission",
                        &IpcError::Protocol {
                            detail: "window CALL reuses an already-pending correlation",
                        },
                    ));
                }
                let request: WindowRequest =
                    decode(payload).map_err(|source| app_ipc("window request decode", &source))?;
                match request {
                    WindowRequest::Create { width, height, x, y } => match session.registry.create(caller_owner, correlation, width, height, x, y) {
                        Ok(CreateOutcome::Build { build, width, height, x, y, .. }) => self.window_commands.send(AppWindowCommand::CreateWindow { build, width, height, x, y }).map_err(|_| app_detail("window native build", "UI command owner is unavailable")),
                        Ok(CreateOutcome::Adopted(window)) => window_reply(active, correlation, WindowResponse::Adopted { window }, &mut failed),
                        Err(error) => call_error(active, correlation, error.code(), error.to_string(), &mut failed),
                    },
                    WindowRequest::SetMaximized { window, maximized } => match session.registry.set_maximized(caller_owner, window, maximized) {
                        Ok(setter) => self.send_window_setter(active, correlation, setter, &mut failed),
                        Err(error) => call_error(active, correlation, error.code(), error.to_string(), &mut failed),
                    },
                    WindowRequest::SetFullScreen { window, full_screen } => match session.registry.set_full_screen(caller_owner, window, full_screen) {
                        Ok(setter) => self.send_window_setter(active, correlation, setter, &mut failed),
                        Err(error) => call_error(active, correlation, error.code(), error.to_string(), &mut failed),
                    },
                    // Strict staged ingress, not placeholder close implementations.
                    // GH-450 must replace this refusal with its complete close owner.
                    WindowRequest::RequestClose { .. } | WindowRequest::CloseReply { .. } | WindowRequest::Destroy { .. } => call_error(active, correlation, "KELD-CORE-038", "KELD-CORE-038: this T1 window state does not admit cancelable-close operations. Use the complete GH-450 close implementation before issuing them.".to_owned(), &mut failed),
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
    fn send_window_setter(
        &self,
        active: &mut ActivePrimaryGeneration,
        correlation: CorrelationId,
        setter: NativeSetter,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let command = match setter {
            NativeSetter::Maximized { view, maximized } => {
                AppWindowCommand::SetMaximized { view, maximized }
            }
            NativeSetter::FullScreen { view, full_screen } => {
                AppWindowCommand::SetFullScreen { view, full_screen }
            }
        };
        self.window_commands
            .send(command)
            .map_err(|_| app_detail("window setter", "UI command owner is unavailable"))?;
        window_reply(active, correlation, WindowResponse::Accepted, failed)
    }
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
            BuildOutcome::Cleanup(teardown) => self
                .window_commands
                .send(AppWindowCommand::DestroyWindow {
                    view: teardown.view,
                })
                .map_err(|_| {
                    app_detail(
                        "window late-build cleanup",
                        "UI command owner is unavailable",
                    )
                }),
            BuildOutcome::Ignored => {
                eprintln!(
                    "KELD-CORE-041: ignored unexpected native build result; inspect the UI build correlation"
                );
                Ok(())
            }
        }
    }
    fn window_native_close_locked(&self, outcome: CloseOutcome) -> Result<(), HostAppError> {
        match outcome {
            CloseOutcome::Teardown(teardown) => self
                .window_commands
                .send(AppWindowCommand::DestroyWindow {
                    view: teardown.view,
                })
                .map_err(|_| app_detail("window native close", "UI command owner is unavailable")),
            CloseOutcome::Unmapped => {
                eprintln!(
                    "KELD-CORE-038: native close references an unmapped view; inspect UI event ordering"
                );
                Ok(())
            }
            CloseOutcome::AlreadyClosing => Ok(()),
        }
    }
    // Caller owns transition -> current -> windows. This dispatch performs the
    // registry transition and its ordered effects without changing lock custody.
    fn window_native_event_locked(
        &self,
        session: &mut WindowSession,
        current: &mut Option<ActivePrimaryGeneration>,
        native_event: AppWindowEvent,
        failed: &mut Option<u32>,
    ) -> Result<(), HostAppError> {
        let subscribed = session.subscribed;
        let fact = match native_event {
            AppWindowEvent::WindowBuilt {
                build,
                view,
                snapshot,
            } => {
                let outcome = session
                    .registry
                    .native_built(build, view, native_state(snapshot));
                return self.window_build_outcome_locked(current, subscribed, outcome, failed);
            }
            AppWindowEvent::WindowBuildFailed { build, stage } => {
                let outcome = session.registry.native_failed(build);
                return window_build_failed_locked(current, subscribed, outcome, stage, failed);
            }
            AppWindowEvent::WindowCloseRequested { view } => {
                return self.window_native_close_locked(session.registry.native_close(view));
            }
            AppWindowEvent::WindowReleased { view } => {
                let outcome = session.registry.native_released(view);
                return window_released_locked(current, subscribed, outcome, failed);
            }
            AppWindowEvent::WindowBounds {
                view,
                x,
                y,
                width,
                height,
            } => session.registry.native_bounds(
                view,
                Rect {
                    x,
                    y,
                    width,
                    height,
                },
            ),
            AppWindowEvent::WindowFocus { view, focused } => {
                session.registry.native_focus(view, focused)
            }
            AppWindowEvent::WindowMaximized { view, maximized } => {
                session.registry.native_maximized(view, maximized)
            }
            AppWindowEvent::WindowFullScreen { view, full_screen } => {
                session.registry.native_full_screen(view, full_screen)
            }
            AppWindowEvent::NavigationReady | AppWindowEvent::LastWindowClosed => {
                return Err(app_detail(
                    "window native event",
                    "legacy lifecycle fact entered managed registry dispatch",
                ));
            }
        };
        match fact {
            FactOutcome::Event(fact) => owned_event(current, subscribed, fact, failed),
            FactOutcome::Unmapped => {
                eprintln!(
                    "KELD-CORE-038: native state references an unmapped view; inspect UI event ordering"
                );
                Ok(())
            }
            FactOutcome::Coalesced | FactOutcome::Suppressed | FactOutcome::Dropped => Ok(()),
        }
    }
    pub(super) fn window_native_event(
        &self,
        native_event: AppWindowEvent,
    ) -> Result<(), HostAppError> {
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
                let next = self
                    .window_commands
                    .send(AppWindowCommand::DestroyWindow {
                        view: teardown.view,
                    })
                    .map_err(|_| {
                        app_detail("window session teardown", "UI command owner is unavailable")
                    });
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
