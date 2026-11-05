//! Guarded menu installation on the existing primary app link and WV main owner.
//! Lock order: shutdown transition -> current generation/writer -> menu state.
//! Native handles and one-shot activation stamps remain in keld-native on main.

use std::io::Write as _;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use keld_guard::{Principal, capability, verified_manifest::VerifiedManifest};
use keld_ipc::guard_dispatch::dispatch_privileged;
use keld_ipc::{CallError, CorrelationId, FrameKind, IpcError, LIFECYCLE_CHANNEL, LifecycleEvent};
use keld_native::menu::macos::{
    MacApplicationMenu, MacMenuOutcome, MenuActivation, MenuCloseRequest, MenuFocusEvidence,
};
use keld_native::menu::{
    MenuAction, MenuFault, MenuInstallReceipt, MenuRequestV1, MenuRole, decode_menu_request,
};
use keld_wv::wkwebview::AppWindowCommand;
use objc2::MainThreadMarker;

use super::window_registry::WindowOwner;
use super::{
    ActivePrimaryGeneration, HostAppError, PrimaryCallerContext, PrimaryRouterHandle, app_detail,
    app_ipc, encode, with_primary_caller_locked, write_frame,
};

const GUARD_FIX: &str = "Add the exact menu literal to app.system in the verified permissions source and rebuild the app.";
const RETIRED_FIX: &str = "Install a fresh menu from the current authenticated app role; never replay a retired installation.";
const CAPACITY_FIX: &str =
    "Wait for the outstanding menu installation to finish before submitting another installation.";
const UNKNOWN_FIX: &str = "Inspect the Host installation outcome before a fresh deliberate install; never automatically replay an unknown effect.";

struct MenuWork {
    caller: PrimaryCallerContext,
    correlation: CorrelationId,
    request: MenuRequestV1,
}
#[derive(Clone)]
struct InstalledOwner {
    caller: PrimaryCallerContext,
    generation: u64,
}
#[derive(Default)]
struct MenuState {
    pending: Option<MenuWork>,
    installed: Option<InstalledOwner>,
}

/// Immutable shared Guard snapshot and one bounded main-thread handoff slot.
/// Neither a channel id nor a subscription confers native menu authority.
pub(super) struct MenuSession {
    verified: VerifiedManifest,
    state: Mutex<MenuState>,
    wake: Sender<AppWindowCommand>,
}

fn same_caller(left: &PrimaryCallerContext, right: &PrimaryCallerContext) -> bool {
    left.identity == right.identity
        && left.attempt == right.attempt
        && Arc::ptr_eq(&left.reader_stop, &right.reader_stop)
}
fn window_owner(caller: &PrimaryCallerContext) -> WindowOwner {
    WindowOwner::new(caller.identity, caller.attempt)
}

impl MenuSession {
    pub(super) fn new(verified: VerifiedManifest, wake: Sender<AppWindowCommand>) -> Self {
        Self {
            verified,
            state: Mutex::new(MenuState::default()),
            wake,
        }
    }
    fn authorize<T>(&self, effect: impl FnOnce() -> T) -> Result<T, CallError> {
        dispatch_privileged(
            self.verified.manifest(),
            Principal::AppProcess,
            capability::MENU,
            "menu",
            |_| effect(),
        )
        .map_err(|denial| CallError::from(&denial))
    }
    /// Caller holds the existing shutdown/current-generation fence. A failure
    /// consumes no native state and is answered once by that caller's writer.
    fn reserve(&self, work: MenuWork) -> Result<(), CallError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| MenuFault::NativeFailure.call_error())?;
        if state.pending.is_some() {
            return Err(CallError::admission_capacity());
        }
        state.pending = Some(work);
        // Existing neutral main-service wake: no new event loop/transport/queue.
        if self.wake.send(AppWindowCommand::MacAuthWake).is_err() {
            state.pending = None;
            return Err(MenuFault::NativeFailure.call_error());
        }
        Ok(())
    }
    /// Called under the current-generation lock before closing its link. The
    /// existing retirement writer owns any best-effort ERR; take exactly once.
    pub(super) fn take_pending_for_retirement(
        &self,
        retired: WindowOwner,
        error: &CallError,
    ) -> Result<Option<CorrelationId>, HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("menu retirement", "menu lock poisoned"))?;
        let Some(pending) = state
            .pending
            .as_ref()
            .filter(|work| window_owner(&work.caller) == retired)
        else {
            return Ok(None);
        };
        let correlation = pending.correlation;
        let attempt = pending.caller.attempt;
        state.pending = None;
        report_retired_install(error, attempt, correlation);
        Ok(Some(correlation))
    }
    /// All role-retirement paths already converge on Core's window-owner
    /// retirement; no second generation state machine is introduced here.
    pub(super) fn retire(&self, retired: WindowOwner) -> Result<(), HostAppError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| app_detail("menu retirement", "menu lock poisoned"))?;
        let pending = state
            .pending
            .as_ref()
            .is_some_and(|work| window_owner(&work.caller) == retired);
        let installed = state
            .installed
            .as_ref()
            .is_some_and(|owner| window_owner(&owner.caller) == retired);
        if pending && let Some(work) = state.pending.take() {
            report_retired_install(
                &CallError::generation_retired(),
                work.caller.attempt,
                work.correlation,
            );
        }
        if installed {
            state.installed = None;
        }
        drop(state);
        if pending || installed {
            let _ = self.wake.send(AppWindowCommand::MacAuthWake);
        }
        Ok(())
    }
    /// A duplicate outstanding correlation is terminal; distinct fresh calls
    /// remain decodable so the route can emit one typed/captured capacity ERR.
    pub(super) fn admission(
        &self,
        attempt: u32,
        correlation: CorrelationId,
    ) -> keld_ipc::link::GuardedCallAdmission {
        let Ok(state) = self.state.lock() else {
            return keld_ipc::link::GuardedCallAdmission::RejectSession;
        };
        if state
            .pending
            .as_ref()
            .is_some_and(|work| work.caller.attempt == attempt && work.correlation == correlation)
        {
            keld_ipc::link::GuardedCallAdmission::RejectSession
        } else {
            keld_ipc::link::GuardedCallAdmission::Admit
        }
    }
}

// This is the one formatting/output owner, shared by every install terminal and
// unsupported native activation. Only trusted code/fix metadata, native stamp,
// menu identity result, and static native failure detail are rendered.
struct Diagnostic<'a> {
    error: &'a CallError,
    phase: &'static str,
    result: &'static str,
    correlation: u32,
    role: Option<u32>,
    generation: Option<u64>,
    ticket: Option<&'static str>,
    fix: &'static str,
    activation: Option<ActivationEvidence>,
}
/// Only Host-minted stamp fields and a native-owned static failure reason.
/// No menu label, window address, IPC payload or application callback is logged.
#[derive(Clone, Copy)]
struct ActivationEvidence {
    stamp: MenuActivation,
    action: Option<MenuAction>,
    tree_current_before_dispatch: bool,
    focus: Option<MenuFocusEvidence>,
    reason: &'static str,
}
fn native_reason(error: &IpcError) -> &'static str {
    match error {
        IpcError::Protocol { detail } => detail,
        _ => "native action returned a non-protocol error",
    }
}
impl std::fmt::Display for Diagnostic<'_> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            out,
            "{} phase={} result={} correlation={} role={:?} menu-generation={:?} ticket={:?} fix={:?}",
            self.error.code,
            self.phase,
            self.result,
            self.correlation,
            self.role,
            self.generation,
            self.ticket,
            self.fix
        )?;
        if let Some(evidence) = self.activation {
            write!(
                out,
                " activation-generation={} activation-command={} activation-sequence={} native-action={:?} native-tree-current-before-dispatch={} native-cause={:?}",
                evidence.stamp.generation,
                evidence.stamp.command,
                evidence.stamp.sequence,
                evidence.action,
                evidence.tree_current_before_dispatch,
                evidence.reason,
            )?;
            if let Some(focus) = evidence.focus {
                write!(
                    out,
                    " app-active-at-click={} key-window-at-click={} main-window-at-click={} app-active-before-dispatch={} key-window-before-dispatch={} main-window-before-dispatch={}",
                    focus.at_click.app_active,
                    focus.at_click.key_window_present,
                    focus.at_click.main_window_present,
                    focus.before_dispatch.app_active,
                    focus.before_dispatch.key_window_present,
                    focus.before_dispatch.main_window_present,
                )?;
            }
        }
        Ok(())
    }
}
fn diagnostic(record: &Diagnostic<'_>) {
    // Logging is best effort; do not retry an already-performed native effect
    // or produce a second terminal when the inherited stderr sink is closed.
    let _ = writeln!(std::io::stderr().lock(), "{record}");
}
/// Existing post-Quit drain and pending-call retirement share this exact
/// diagnostic owner; only the generic writer emits their canonical ERR frame.
pub(super) fn report_retired_install(error: &CallError, attempt: u32, correlation: CorrelationId) {
    let result = if error.code == "KELD-IPC-024" {
        "denied"
    } else {
        "effect-unknown"
    };
    diagnostic(&Diagnostic {
        error,
        phase: "install",
        result,
        correlation: correlation.0,
        role: Some(attempt),
        generation: None,
        ticket: None,
        fix: RETIRED_FIX,
        activation: None,
    });
}

fn install_error(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    error: &CallError,
    result: &'static str,
    fix: &'static str,
) -> Result<(), HostAppError> {
    diagnostic(&Diagnostic {
        error,
        phase: "install",
        result,
        correlation: correlation.0,
        role: Some(active.attempt),
        generation: None,
        ticket: None,
        fix,
        activation: None,
    });
    keld_ipc::write_call_error(
        &mut active.writer,
        keld_ipc::channel_table::MENU.id(),
        correlation,
        error,
    )
    .map_err(|source| app_ipc("menu correlated ERR", &source))
}
fn install_reply(
    active: &mut ActivePrimaryGeneration,
    correlation: CorrelationId,
    receipt: MenuInstallReceipt,
) -> Result<(), HostAppError> {
    let result = (|| {
        let bytes = encode(&receipt).map_err(|source| app_ipc("menu receipt encode", &source))?;
        write_frame(
            &mut active.writer,
            FrameKind::Reply,
            0,
            keld_ipc::channel_table::MENU.id(),
            correlation,
            &bytes,
        )
        .map_err(|source| app_ipc("menu correlated receipt", &source))
    })();
    if result.is_err() {
        diagnostic(&Diagnostic {
            error: &MenuFault::NativeFailure.call_error(),
            phase: "install",
            result: "effect-unknown",
            correlation: correlation.0,
            role: Some(active.attempt),
            generation: Some(receipt.generation),
            ticket: None,
            fix: UNKNOWN_FIX,
            activation: None,
        });
    }
    result
}

impl PrimaryRouterHandle {
    fn original_menu_quit_selected(
        &self,
        caller: &PrimaryCallerContext,
    ) -> Result<bool, HostAppError> {
        if !self.window_ready.load(super::Ordering::Acquire) {
            return Ok(false);
        }
        self.windows.as_ref().map_or(Ok(false), |windows| {
            let windows = windows
                .lock()
                .map_err(|_| app_detail("native menu Quit selection", "window lock poisoned"))?;
            Ok(windows.original_menu_quit_selected(window_owner(caller)))
        })
    }
    pub(super) fn dispatch_menu(
        &self,
        caller: &PrimaryCallerContext,
        correlation: CorrelationId,
        payload: &[u8],
    ) -> Result<(), HostAppError> {
        with_primary_caller_locked(&self.current, &self.shutdown, caller, |current| {
            let active = current
                .as_mut()
                .ok_or_else(|| app_detail("menu request", "current caller disappeared"))?;
            let Some(session) = &self.menu else {
                return install_error(
                    active,
                    correlation,
                    &MenuFault::NativeFailure.call_error(),
                    "native-failed",
                    MenuFault::NativeFailure.fix(),
                );
            };
            // Guard precedes payload decoding/native admission. Caller identity
            // was fenced by the existing authenticated reader immediately above.
            if let Err(error) = session.authorize(|| ()) {
                return install_error(active, correlation, &error, "denied", GUARD_FIX);
            }
            let Ok(request) = decode_menu_request(payload) else {
                return install_error(
                    active,
                    correlation,
                    &MenuFault::InvalidProjection.call_error(),
                    "denied",
                    MenuFault::InvalidProjection.fix(),
                );
            };
            if let Err(error) = session.reserve(MenuWork {
                caller: caller.clone(),
                correlation,
                request,
            }) {
                let fix = if error.code == "KELD-IPC-005" {
                    CAPACITY_FIX
                } else {
                    MenuFault::NativeFailure.fix()
                };
                let result = if error.code == "KELD-IPC-005" {
                    "denied"
                } else {
                    "native-failed"
                };
                return install_error(active, correlation, &error, result, fix);
            }
            Ok(())
        })
    }
}

// Called only inside the final canonical menu Guard effect and current-role
// fence. This unit carries intent, not a terminal Quit/permission/window cause.
fn publish_quit_requested(active: &mut ActivePrimaryGeneration) -> Result<(), IpcError> {
    let payload = encode(&LifecycleEvent::QuitRequested)?;
    write_frame(
        &mut active.writer,
        FrameKind::Event,
        0,
        LIFECYCLE_CHANNEL,
        CorrelationId(0),
        &payload,
    )
}
fn retire_quit_intent_write_failure(
    router: &PrimaryRouterHandle,
    current: &mut Option<ActivePrimaryGeneration>,
    attempt: u32,
    source: &IpcError,
    menu_state: std::sync::MutexGuard<'_, MenuState>,
) -> HostAppError {
    // Retirement re-enters this same menu owner. The current/transition fence
    // remains held by the caller, while the menu state lock ends before it.
    drop(menu_state);
    let primary = app_ipc("native menu QuitRequested write", source);
    let retirement = router.retire_current_generation_locked(
        current,
        attempt,
        "native menu Quit intent retired after failed write",
        "native menu Quit intent link close",
    );
    super::append_app_cleanup(primary, [retirement])
}

/// Main-thread-only native owner composed into the existing WV service closure.
pub(super) struct MenuMainOwner {
    session: Arc<MenuSession>,
    native: MacApplicationMenu,
}
impl MenuMainOwner {
    pub(super) fn new(main: MainThreadMarker, session: Arc<MenuSession>) -> Self {
        let wake = session.wake.clone();
        Self {
            session,
            native: MacApplicationMenu::new(main, move || {
                let _ = wake.send(AppWindowCommand::MacAuthWake);
            }),
        }
    }
    /// Main-service drive is cold native work. Shutdown/current locks remain
    /// held through the final Guard callback and effect to linearize retirement.
    pub(super) fn drive(
        &mut self,
        router: &PrimaryRouterHandle,
        request_close: &mut MenuCloseRequest<'_>,
        stopping: bool,
    ) -> Result<(), HostAppError> {
        if stopping || !router.shutdown.is_running() {
            self.native.cancel();
            let mut state = self
                .session
                .state
                .lock()
                .map_err(|_| app_detail("menu shutdown", "menu lock poisoned"))?;
            state.installed = None;
            if let Some(work) = state.pending.take() {
                report_retired_install(
                    &CallError::generation_retired(),
                    work.caller.attempt,
                    work.correlation,
                );
            }
            return Ok(());
        }
        self.install_pending(router)?;
        self.dispatch_activation(router, request_close)
    }
    fn install_pending(&mut self, router: &PrimaryRouterHandle) -> Result<(), HostAppError> {
        let caller = self
            .session
            .state
            .lock()
            .map_err(|_| app_detail("menu install", "menu lock poisoned"))?
            .pending
            .as_ref()
            .map(|work| work.caller.clone());
        let Some(caller) = caller else {
            return Ok(());
        };
        let mut entered = false;
        let result =
            with_primary_caller_locked(&router.current, &router.shutdown, &caller, |current| {
                entered = true;
                let active = current
                    .as_mut()
                    .ok_or_else(|| app_detail("menu install", "current caller disappeared"))?;
                let mut state = self
                    .session
                    .state
                    .lock()
                    .map_err(|_| app_detail("menu install", "menu lock poisoned"))?;
                let work = state
                    .pending
                    .take()
                    .ok_or_else(|| app_detail("menu install", "reserved request disappeared"))?;
                let installed = self
                    .session
                    .authorize(|| self.native.install(&work.request));
                match installed {
                    Err(error) => {
                        install_error(active, work.correlation, &error, "denied", GUARD_FIX)
                    }
                    Ok(Err(_)) => install_error(
                        active,
                        work.correlation,
                        &MenuFault::NativeFailure.call_error(),
                        "native-failed",
                        MenuFault::NativeFailure.fix(),
                    ),
                    Ok(Ok(receipt)) => {
                        state.installed = Some(InstalledOwner {
                            caller: work.caller,
                            generation: receipt.generation,
                        });
                        install_reply(active, work.correlation, receipt)
                    }
                }
            });
        if !entered {
            self.session.retire(window_owner(&caller))?;
        }
        result
    }
    fn dispatch_activation(
        &mut self,
        router: &PrimaryRouterHandle,
        request_close: &mut MenuCloseRequest<'_>,
    ) -> Result<(), HostAppError> {
        let owner = self
            .session
            .state
            .lock()
            .map_err(|_| app_detail("menu activation", "menu lock poisoned"))?
            .installed
            .clone();
        let Some(owner) = owner else {
            self.native.cancel();
            return Ok(());
        };
        let mut entered = false;
        let result = with_primary_caller_locked(
            &router.current,
            &router.shutdown,
            &owner.caller,
            |current| {
                entered = true;
                let active = current
                    .as_mut()
                    .ok_or_else(|| app_detail("native menu Quit", "current caller disappeared"))?;
                let Some(activation) = self.native.poll_activation() else {
                    return Ok(());
                };
                // Observe the exact AppKit menu before the final dispatch gate.
                // This is never cached as permission or used to skip that gate.
                let tree_current_before_dispatch = self.native.installed_tree_matches_main_menu();
                let focus = self.native.focus_evidence(activation);
                let action = self.native.action(activation);
                let evidence = ActivationEvidence {
                    stamp: activation,
                    action: action.as_ref().ok().copied(),
                    tree_current_before_dispatch,
                    focus,
                    reason: "native menu activation reached dispatch",
                };
                if activation.generation != owner.generation || action.is_err() {
                    discard_invalid_activation(&mut self.native, &owner, &action, evidence);
                    return Ok(());
                }
                // Selection is queried only for Quit, before the menu-state
                // lock; no new window lock/dependency is added to native Edit.
                let original_selected =
                    matches!(action, Ok(MenuAction::NativeRole(MenuRole::Quit)))
                        && router.original_menu_quit_selected(&owner.caller)?;
                let mut state = self
                    .session
                    .state
                    .lock()
                    .map_err(|_| app_detail("menu activation", "menu lock poisoned"))?;
                if !state.installed.as_ref().is_some_and(|current| {
                    current.generation == owner.generation
                        && same_caller(&current.caller, &owner.caller)
                }) {
                    self.native.cancel();
                    return Ok(());
                }
                // The sole final guard callback contains native dispatch, rather
                // than returning a transferable permit or cached allow bit.
                let mut quit_requested = false;
                let mut quit_write_failure = None;
                let outcome = self.session.authorize(|| {
                    let outcome = self.native.dispatch(activation, request_close)?;
                    if outcome == MacMenuOutcome::UnsupportedQuit && original_selected {
                        match publish_quit_requested(active) {
                            Ok(()) => quit_requested = true,
                            Err(source) => quit_write_failure = Some(source),
                        }
                    }
                    Ok::<_, IpcError>(outcome)
                });
                if let Some(source) = quit_write_failure {
                    // Retirement re-enters the existing menu owner: release that
                    // state lock first while retaining transition/current fences.
                    return Err(retire_quit_intent_write_failure(
                        router,
                        current,
                        owner.caller.attempt,
                        &source,
                        state,
                    ));
                }
                finish_menu_activation(
                    &mut self.native,
                    &mut state,
                    &owner,
                    outcome,
                    quit_requested,
                    evidence,
                );
                Ok(())
            },
        );
        if !entered {
            self.native.cancel();
            self.session.retire(window_owner(&owner.caller))?;
        }
        result
    }
}
fn discard_invalid_activation(
    native: &mut MacApplicationMenu,
    owner: &InstalledOwner,
    action: &Result<MenuAction, IpcError>,
    evidence: ActivationEvidence,
) {
    let reason = if evidence.stamp.generation == owner.generation {
        action
            .as_ref()
            .err()
            .map_or("native menu action lookup failed", native_reason)
    } else {
        "activation generation differs from installed menu owner"
    };
    native.discard(evidence.stamp);
    activation_fault(
        MenuFault::NativeFailure,
        owner.caller.attempt,
        owner.generation,
        None,
        ActivationEvidence { reason, ..evidence },
    );
}
fn finish_menu_activation(
    native: &mut MacApplicationMenu,
    state: &mut MenuState,
    owner: &InstalledOwner,
    outcome: Result<Result<MacMenuOutcome, IpcError>, CallError>,
    quit_requested: bool,
    evidence: ActivationEvidence,
) {
    match outcome {
        Err(error) => {
            native.cancel();
            state.installed = None;
            diagnostic(&Diagnostic {
                error: &error,
                phase: "activation",
                result: "denied",
                correlation: 0,
                role: Some(owner.caller.attempt),
                generation: Some(owner.generation),
                ticket: None,
                fix: GUARD_FIX,
                activation: None,
            });
        }
        Ok(Err(error)) => activation_fault(
            MenuFault::NativeFailure,
            owner.caller.attempt,
            owner.generation,
            None,
            ActivationEvidence {
                reason: native_reason(&error),
                ..evidence
            },
        ),
        Ok(Ok(MacMenuOutcome::UnsupportedResponder)) => activation_fault(
            MenuFault::NativeFailure,
            owner.caller.attempt,
            owner.generation,
            None,
            ActivationEvidence {
                reason: "native focused responder did not handle menu action",
                ..evidence
            },
        ),
        Ok(Ok(MacMenuOutcome::UnsupportedClick)) => activation_fault(
            MenuFault::UnsupportedClick,
            owner.caller.attempt,
            owner.generation,
            Some("GH556"),
            ActivationEvidence {
                reason: "unsupported native menu callback",
                ..evidence
            },
        ),
        // An intent write is not native/app Quit completion.
        Ok(Ok(MacMenuOutcome::UnsupportedQuit)) if quit_requested => {}
        Ok(Ok(MacMenuOutcome::UnsupportedQuit)) => activation_fault(
            MenuFault::UnsupportedQuit,
            owner.caller.attempt,
            owner.generation,
            Some("GH563"),
            ActivationEvidence {
                reason: "native Quit lacks admitted original selection",
                ..evidence
            },
        ),
        Ok(Ok(MacMenuOutcome::Applied | MacMenuOutcome::CloseRequested)) => {}
    }
}

fn activation_fault(
    fault: MenuFault,
    attempt: u32,
    generation: u64,
    ticket: Option<&'static str>,
    evidence: ActivationEvidence,
) {
    let result = if ticket.is_some() {
        "unsupported"
    } else {
        "native-failed"
    };
    diagnostic(&Diagnostic {
        error: &fault.call_error(),
        phase: "activation",
        result,
        correlation: 0,
        role: Some(attempt),
        generation: Some(generation),
        ticket,
        fix: fault.fix(),
        activation: Some(evidence),
    });
}

#[cfg(test)]
mod activation_diagnostic_tests {
    use super::*;
    use keld_native::menu::macos::MenuFocusSnapshot;

    #[test]
    fn native_failure_retains_stamped_command_tree_identity_and_exact_static_cause() {
        let diagnostic = Diagnostic {
            error: &MenuFault::NativeFailure.call_error(),
            phase: "activation",
            result: "native-failed",
            correlation: 0,
            role: Some(1),
            generation: Some(1),
            ticket: None,
            fix: MenuFault::NativeFailure.fix(),
            activation: Some(ActivationEvidence {
                stamp: MenuActivation {
                    generation: 1,
                    command: 13,
                    sequence: 2,
                },
                action: Some(MenuAction::NativeRole(MenuRole::Quit)),
                tree_current_before_dispatch: false,
                focus: Some(MenuFocusEvidence {
                    at_click: MenuFocusSnapshot {
                        app_active: false,
                        key_window_present: false,
                        main_window_present: true,
                    },
                    before_dispatch: MenuFocusSnapshot {
                        app_active: true,
                        key_window_present: true,
                        main_window_present: true,
                    },
                }),
                reason: native_reason(&IpcError::Protocol {
                    detail: "native menu tree is no longer installed",
                }),
            }),
        };
        let line = diagnostic.to_string();
        assert!(line.starts_with(
            "KELD-NATIVE-017 phase=activation result=native-failed correlation=0 role=Some(1) menu-generation=Some(1) ticket=None"
        ));
        assert!(line.contains(
            "activation-generation=1 activation-command=13 activation-sequence=2 native-action=Some(NativeRole(Quit)) native-tree-current-before-dispatch=false native-cause=\"native menu tree is no longer installed\""
        ));
        assert!(line.contains(
            "app-active-at-click=false key-window-at-click=false main-window-at-click=true app-active-before-dispatch=true key-window-before-dispatch=true main-window-before-dispatch=true"
        ));
        assert!(!line.contains("0x"), "native handles must stay private");
    }

    #[test]
    fn focus_diagnostic_preserves_all_six_independent_observations() {
        for mask in 0_u8..64 {
            let flags = std::array::from_fn::<_, 6, _>(|bit| mask & (1 << bit) != 0);
            let diagnostic = Diagnostic {
                error: &MenuFault::NativeFailure.call_error(),
                phase: "activation",
                result: "native-failed",
                correlation: 0,
                role: Some(1),
                generation: Some(1),
                ticket: None,
                fix: MenuFault::NativeFailure.fix(),
                activation: Some(ActivationEvidence {
                    stamp: MenuActivation {
                        generation: 1,
                        command: 17,
                        sequence: 3,
                    },
                    action: Some(MenuAction::NativeRole(MenuRole::Close)),
                    tree_current_before_dispatch: true,
                    focus: Some(MenuFocusEvidence {
                        at_click: MenuFocusSnapshot {
                            app_active: flags[0],
                            key_window_present: flags[1],
                            main_window_present: flags[2],
                        },
                        before_dispatch: MenuFocusSnapshot {
                            app_active: flags[3],
                            key_window_present: flags[4],
                            main_window_present: flags[5],
                        },
                    }),
                    reason: "native menu action had no key window at click",
                }),
            };
            let expected = format!(
                "app-active-at-click={} key-window-at-click={} main-window-at-click={} app-active-before-dispatch={} key-window-before-dispatch={} main-window-before-dispatch={}",
                mask & 1 != 0,
                mask & 2 != 0,
                mask & 4 != 0,
                mask & 8 != 0,
                mask & 16 != 0,
                mask & 32 != 0,
            );
            assert!(diagnostic.to_string().ends_with(&expected), "mask {mask}");
        }
    }
}

#[cfg(test)]
mod tests;
