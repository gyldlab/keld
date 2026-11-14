//! Native resources for registry-owned Windows views.
//!
//! Core retains window identity, generations, close policy and last-sent state.
//! This adapter retains live native resources and unfinished native teardown.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{Weak, mpsc::Sender};
use std::time::Instant;

use tao::event_loop::{ControlFlow, EventLoopProxy, EventLoopWindowTarget};
use tao::platform::windows::WindowExtWindows as _;
use tao::window::Fullscreen;

use super::activation_observer::ActivationObserver;
use super::browser_instance::BrowserExitReceipt;
use super::managed_windows::{CreateAdmission, DeferredCreate, ManagedCustody, Releasing};
use super::view_build::NativeBuild;
use super::{ProfileReleaseFault, ProfileReleaseWait, View, WindowsLoopEvent};
use crate::engine::{AppWindowCommand, AppWindowEvent};
use crate::error::WvError;

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use super::{managed_test_control::TestControl, t2_test_observation as observe};

mod build;
mod observation;
mod original;

#[cfg(test)]
#[path = "managed_app/tests/stop_order.rs"]
mod stop_order_tests;

pub(super) struct ManagedWindows {
    views: BTreeMap<u32, View>,
    custody: ManagedCustody,
    builder: NativeBuild,
    desktop: super::desktop_session::DesktopSession,
    events: Sender<AppWindowEvent>,
    proxy: EventLoopProxy<WindowsLoopEvent>,
    activation_observer: Rc<ActivationObserver>,
    receipt_sender: Weak<Sender<BrowserExitReceipt>>,
    renderer_endpoint: Option<crate::renderer_bridge::RendererBridgeEndpoint>,
    original_session: Option<Rc<super::original_resource::ResourceSession>>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    test_control: Option<TestControl>,
}

impl ManagedWindows {
    /// Same retained UI owner; native sampling holds no shared `RefCell` guard.
    pub(super) fn desktop_first_init(&mut self, ending: impl Fn() -> bool) -> bool {
        self.desktop.first_init(&self.events, ending)
    }

    pub(super) fn refresh_desktop(&mut self) -> Result<(), WvError> {
        self.desktop.refresh(&self.events)
    }

    /// Observe actual native resource drainage after the UI loop returns.
    /// An unavailable/staged map cannot provide this observation.
    pub(super) fn native_resources_drained(&self) -> bool {
        self.views.is_empty()
            && self.builder.candidate_is_empty()
            && self.custody.exit_ready(true, true)
            && self
                .original_session
                .as_ref()
                .is_none_or(|session| session.is_drained())
    }

    /// Read the existing fixture's exact retained view without native entry.
    #[cfg(all(feature = "media-acceptance", test))]
    #[expect(
        clippy::used_underscore_binding,
        reason = "The test observes the unchanged retention-only production field to prove the same release identity."
    )]
    pub(super) fn native_close_fixture_view(
        &self,
        hwnd: isize,
        release: &Rc<ProfileReleaseWait>,
    ) -> Option<(crate::WebviewId, bool)> {
        if self.views.len() != 1 || !self.builder.candidate_is_empty() {
            return None;
        }
        self.views.iter().find_map(|(id, view)| {
            let same_release = view
                ._managed_release
                .as_ref()
                .and_then(std::rc::Weak::upgrade)
                .is_some_and(|actual| Rc::ptr_eq(&actual, release));
            (view.window.hwnd() == hwnd && same_release)
                .then_some((crate::WebviewId(*id), view.close_custody.actual_closed()))
        })
    }

    pub(super) fn dialog_parent(
        &self,
        view: crate::WebviewId,
    ) -> Option<super::WindowsDialogParent> {
        self.views
            .get(&view.0)
            .map(|view| super::WindowsDialogParent::new(Rc::clone(&view.window)))
    }

    pub(super) fn new(
        builder: NativeBuild,
        events: Sender<AppWindowEvent>,
        proxy: EventLoopProxy<WindowsLoopEvent>,
        activation_observer: Rc<ActivationObserver>,
        receipt_sender: Weak<Sender<BrowserExitReceipt>>,
        renderer_endpoint: Option<crate::renderer_bridge::RendererBridgeEndpoint>,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))] test_control: Option<
            TestControl,
        >,
    ) -> Self {
        Self {
            views: BTreeMap::new(),
            custody: ManagedCustody::default(),
            builder,
            desktop: super::desktop_session::DesktopSession::default(),
            events,
            proxy,
            activation_observer,
            receipt_sender,
            renderer_endpoint,
            original_session: None,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            test_control,
        }
    }

    /// Called at the top of a later native turn, before expiry and exit checks.
    pub(super) fn publish_retired(&mut self) -> Result<(), WvError> {
        let events = &self.events;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let test_control = self.test_control.as_ref();
        let result = self.custody.publish_staged(|hwnd, owed, event| {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            observe::teardown_confirmed(hwnd);
            #[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
            let _ = hwnd;
            match owed {
                Releasing::Destroy(view) => {
                    let result = send(events, event);
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    observe::released(view.0);
                    #[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
                    let _ = view;
                    result
                }
                Releasing::FailedBuild { .. } => send_failure(
                    events,
                    event,
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    test_control,
                ),
            }
        });
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        observe::releasing(self.custody.len());
        result.map(|_| ())
    }

    pub(super) fn expire(&mut self, release: &ProfileReleaseWait, now: Instant) {
        if self.custody.expire(now, |_, hwnd| {
            eprintln!("KELD_WV_RELEASING_EXPIRED hwnd={hwnd}");
        }) != 0
        {
            release.latch_fault(ProfileReleaseFault::ReleasingExpired);
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        observe::releasing(self.custody.len());
    }

    /// D7 closes managed controllers before the terminal/fault owner commits
    /// Stopping. Deferred refusals then observe that committed state.
    /// Parked answers remain owned until independent native retirement.
    pub(super) fn stop(&mut self, after_views_closed: impl FnOnce()) -> Result<(), WvError> {
        // Terminal cleanup cancels only the fixture wait, never a native receipt.
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        self.custody.cancel_prefill_wait();
        let events = &self.events;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let test_control = self.test_control.as_ref();
        let candidate_drained = self.builder.retire_candidate_on_ui();
        let stopped = close_managed_resources(
            &mut self.views,
            &mut self.custody,
            View::retire_on_ui,
            after_views_closed,
            |event| {
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                if let AppWindowEvent::WindowBuildFailed { build, .. } = &event {
                    observe::build_refused(*build);
                }
                send_failure(
                    events,
                    event,
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    test_control,
                )
            },
        );
        if !candidate_drained {
            if let Err(error) = stopped {
                eprintln!("KELD_WV_BUILD_STOP_CLEANUP {error}");
            }
            return Err(super::profile_failure_with(
                crate::profile::ProfileErrorKind::LifecycleUnproven,
                "native stop retained an unresolved construction candidate",
            ));
        }
        stopped
    }

    pub(super) fn exit_ready(&self, terminal: bool, barrier_ready: bool) -> bool {
        terminal && barrier_ready && self.native_resources_drained()
    }

    /// The outer `AppRun` retains this exact command through `native_turn` refusal.
    /// None after entry records a consumed command, including a returned error;
    /// errors stop the run and are never replayed. Existing build/load handlers
    /// attach their actual inputs before any native entry.
    pub(super) fn command_retained(
        &mut self,
        pending: &mut Option<AppWindowCommand>,
        target: &EventLoopWindowTarget<WindowsLoopEvent>,
        release: &Rc<ProfileReleaseWait>,
        terminated: bool,
        terminal_intent: bool,
    ) -> Result<(), WvError> {
        super::app_wake_bridge::delivery::accept(pending, |command| {
            self.command(command, target, release, terminated, terminal_intent)
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Keep the closed command dispatch within its one retained native UI owner and terminal cut"
    )]
    pub(super) fn command(
        &mut self,
        command: AppWindowCommand,
        target: &EventLoopWindowTarget<WindowsLoopEvent>,
        release: &Rc<ProfileReleaseWait>,
        terminated: bool,
        terminal_intent: bool,
    ) -> Result<(), WvError> {
        match command {
            AppWindowCommand::DialogWake => Ok(()),
            AppWindowCommand::WindowsDesktopProducer { capture } => {
                if terminated || terminal_intent {
                    Ok(())
                } else {
                    self.desktop.bind(capture, &self.events)
                }
            }
            AppWindowCommand::CreateWindow {
                build,
                width,
                height,
                x,
                y,
                original_source,
                preload_image,
                original_role,
            } => {
                let original = match (original_source, preload_image, original_role) {
                    (None, None, None) => None,
                    (Some(source), Some(image), Some(role)) => {
                        Some(super::original_view::OriginalBuildInputs {
                            source,
                            image,
                            role,
                        })
                    }
                    _ => {
                        return send_failure(
                            &self.events,
                            AppWindowEvent::WindowBuildFailed {
                                build,
                                stage: crate::BuildStage::Webview,
                            },
                            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                            self.test_control.as_ref(),
                        );
                    }
                };
                let create = DeferredCreate {
                    build,
                    width,
                    height,
                    x,
                    y,
                    original,
                };
                match self.custody.admit(
                    create,
                    terminated || terminal_intent,
                    !self.builder.candidate_is_empty(),
                ) {
                    CreateAdmission::Start(create) => self.build(target, release, create),
                    CreateAdmission::Queued => {
                        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                        observe::build_deferred(build);
                        Ok(())
                    }
                    CreateAdmission::Refused(create) => {
                        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                        observe::build_refused(create.build);
                        send_failure(
                            &self.events,
                            create.failure_event(),
                            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                            self.test_control.as_ref(),
                        )
                    }
                }
            }
            AppWindowCommand::DestroyWindow { view } if !terminated => self.destroy_view(view),
            AppWindowCommand::SetMaximized { view, maximized } if !terminated => {
                if let Some(native) = self.views.get(&view.0) {
                    native.window.set_maximized(maximized);
                } else {
                    eprintln!("KELD_WV_STALE_MAXIMIZE view={}", view.0);
                }
                Ok(())
            }
            AppWindowCommand::SetFullScreen { view, full_screen } if !terminated => {
                if let Some(native) = self.views.get(&view.0) {
                    native
                        .window
                        .set_fullscreen(full_screen.then_some(Fullscreen::Borderless(None)));
                } else {
                    eprintln!("KELD_WV_STALE_FULLSCREEN view={}", view.0);
                }
                Ok(())
            }
            AppWindowCommand::DestroyWindow { .. }
            | AppWindowCommand::SetMaximized { .. }
            | AppWindowCommand::SetFullScreen { .. } => {
                eprintln!("KELD_WV_WINDOW_COMMAND_AFTER_TERMINAL");
                Ok(())
            }
            AppWindowCommand::DestroyOriginalDocument {
                view,
                expected,
                admission,
            } => {
                let accepted = if terminated || terminal_intent {
                    let _ = admission.refuse_on_ui();
                    Ok(false)
                } else if let Some(original) = self
                    .views
                    .get(&view.0)
                    .and_then(|view| view.original.as_ref())
                {
                    original.admit_destroy(&expected, &admission)
                } else {
                    let _ = admission.refuse_on_ui();
                    Ok(false)
                };
                send(
                    &self.events,
                    AppWindowEvent::OriginalDocumentDestroyDecision { view, admission },
                )?;
                if accepted? {
                    self.destroy_view(view)?;
                }
                Ok(())
            }
            AppWindowCommand::LoadAdmittedOriginal {
                context,
                query,
                role_witness,
            } => {
                let fact = if terminated || terminal_intent {
                    crate::OriginalNavigationFact::Refused {
                        error: crate::NativeNavigationRefusal::Unavailable,
                    }
                } else if let Some(view) = self.views.get_mut(&context.view().0) {
                    match view.original.as_mut() {
                        Some(original) => original.load_on_ui(
                            &view.webview,
                            &self.builder.environment,
                            context,
                            &query,
                            role_witness,
                        ),
                        None => crate::OriginalNavigationFact::Refused {
                            error: crate::NativeNavigationRefusal::MissingSource,
                        },
                    }
                } else {
                    crate::OriginalNavigationFact::Refused {
                        error: crate::NativeNavigationRefusal::UnknownView,
                    }
                };
                send(
                    &self.events,
                    AppWindowEvent::OriginalNavigation { context, fact },
                )
            }
            AppWindowCommand::HostWriterProgress => {
                send(&self.events, AppWindowEvent::HostWriterProgress)
            }
            AppWindowCommand::Quit | AppWindowCommand::Fatal => {
                // Root's existing terminal owner performs Stopping and arming.
                Ok(())
            }
        }
    }

    pub(super) fn deferred_wake(
        &mut self,
        target: &EventLoopWindowTarget<WindowsLoopEvent>,
        release: &Rc<ProfileReleaseWait>,
        ending: bool,
    ) -> Result<(), WvError> {
        if let Some(create) = self
            .custody
            .take_deferred_wake(ending, !self.builder.candidate_is_empty())
        {
            self.build(target, release, create)?;
        }
        Ok(())
    }

    pub(super) fn schedule(&mut self, control_flow: &mut ControlFlow, ending: bool) {
        let now = Instant::now();
        let wake_retry = self.custody.post_deferred_wake(
            ending,
            !self.builder.candidate_is_empty(),
            now,
            || {
                self.proxy
                    .send_event(WindowsLoopEvent::DeferredBuildWake)
                    .is_ok()
            },
        );
        for deadline in [
            self.custody.next_deadline(now),
            wake_retry,
            self.original_deadline(),
        ]
        .into_iter()
        .flatten()
        {
            if !matches!(*control_flow, ControlFlow::WaitUntil(current) if current <= deadline) {
                *control_flow = ControlFlow::WaitUntil(deadline);
            }
        }
    }
}

/// One resource/queue sequence for the production stop path and pure Drop
/// witnesses. The callback belongs to the existing terminal/fault authority;
/// this helper neither commits policy nor arms a release barrier.
fn close_managed_resources<V, K: Ord + Copy, O, E>(
    views: &mut BTreeMap<u32, V>,
    custody: &mut ManagedCustody<K, O>,
    mut retire: impl FnMut(&V) -> bool,
    after_views_closed: impl FnOnce(),
    send: impl FnMut(AppWindowEvent) -> Result<(), E>,
) -> Result<(), E> {
    // The same map keeps any value whose actual retirement is unproved.
    // View's one close custodian prevents a repeated stop from retrying Close.
    views.retain(|_, value| !retire(value));
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    observe::native_release_step(
        if views.is_empty() {
            "managed-stop.views-cleared"
        } else {
            "managed-stop.views-retained"
        },
        None,
    );
    after_views_closed();
    custody.refuse_deferred(send)
}

/// Every failed build answer passes this function, including immediate refusal
/// and the later native-retirement answer. Its observation is not receipt credit.
fn send_failure(
    events: &Sender<AppWindowEvent>,
    event: AppWindowEvent,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))] test_control: Option<
        &TestControl,
    >,
) -> Result<(), WvError> {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    if let AppWindowEvent::WindowBuildFailed { build, .. } = &event
        && (observe::enabled()
            || test_control.is_some_and(|control| control.observes_failed_build(*build)))
    {
        eprintln!("FAILED_BUILD_ANSWERED {build}");
    }
    send(events, event)
}

fn send(events: &Sender<AppWindowEvent>, event: AppWindowEvent) -> Result<(), WvError> {
    events
        .send(event)
        .map_err(|error| WvError::EventLoop(format!("native window fact receiver ended: {error}")))
}
