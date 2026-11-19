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
use tao::window::{Fullscreen, WindowId};

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

use original::SelectedCreate;
pub(super) use original::SelectedRenderer;

#[cfg(test)]
#[path = "managed_app/tests/stop_order.rs"]
mod stop_order_tests;

pub(super) struct ManagedWindows {
    views: BTreeMap<u32, View>,
    custody: ManagedCustody<WindowId, SelectedCreate>,
    builder: NativeBuild,
    events: Sender<AppWindowEvent>,
    proxy: EventLoopProxy<WindowsLoopEvent>,
    activation_observer: Rc<ActivationObserver>,
    receipt_sender: Weak<Sender<BrowserExitReceipt>>,
    renderer: Option<SelectedRenderer>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    test_control: Option<TestControl>,
}

impl ManagedWindows {
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
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))] test_control: Option<
            TestControl,
        >,
    ) -> Self {
        Self {
            views: BTreeMap::new(),
            custody: ManagedCustody::default(),
            builder,
            events,
            proxy,
            activation_observer,
            receipt_sender,
            renderer: None,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            test_control,
        }
    }

    pub(super) fn owns_desktop_refresh_window(&self, window: WindowId) -> bool {
        !self.custody.contains(window) && self.views.values().any(|view| view.window.id() == window)
    }

    /// The engine installs its one endpoint after fallible loop setup.
    pub(super) fn attach_selected_renderer(&mut self, renderer: SelectedRenderer) {
        self.renderer = Some(renderer);
    }

    /// Called at the top of a later native turn, before expiry and exit checks.
    pub(super) fn publish_retired(&mut self) -> Result<(), WvError> {
        let events = &self.events;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let test_control = self.test_control.as_ref();
        let views = &self.views;
        let candidate = self.builder.candidate_window().map(tao::window::Window::id);
        let result = self.custody.publish_staged(
            |key| {
                candidate == Some(key)
                    || views
                        .values()
                        .any(|view| view.original.is_some() && view.window.id() == key)
            },
            |hwnd, owed, event| {
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
            },
        );
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
        let candidate_drained = self.builder.retire_candidate_on_ui();
        let events = &self.events;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let test_control = self.test_control.as_ref();
        let stopped = close_managed_resources(
            &mut self.views,
            &mut self.custody,
            |native| native.original.is_none() || native.retire_on_ui(),
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
        // Each actual View first revokes and reports its own checked identity.
        // Shared resource retirement would otherwise erase that correlation.
        let selected_stopped = self.stop_selected();
        if !candidate_drained {
            if let Err(error) = stopped.and(selected_stopped) {
                eprintln!("KELD_WV_BUILD_STOP_CLEANUP {error}");
            }
            return Err(super::profile_failure_with(
                crate::profile::ProfileErrorKind::LifecycleUnproven,
                "native stop retained an unresolved construction candidate",
            ));
        }
        selected_stopped.and(stopped)
    }

    pub(super) fn has_build_candidate(&self) -> bool {
        !self.builder.candidate_is_empty()
    }

    pub(super) fn exit_ready(&self, terminal: bool, barrier_ready: bool, faulted: bool) -> bool {
        // A failed actual Close is final for this controller. Fault exit
        // transfers an occupied candidate to the engine's retained run owner.
        if terminal && faulted && self.retains_native_app_run() {
            return true;
        }
        !self.retains_native_app_run() && self.custody.exit_ready(terminal, barrier_ready)
    }

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
            AppWindowCommand::WindowsDesktopProducer { .. } => Err(WvError::EventLoop(
                String::from("desktop producer command bypassed its selected UI owner"),
            )),
            AppWindowCommand::HostWriterProgress => {
                send(&self.events, AppWindowEvent::HostWriterProgress)
            }
            AppWindowCommand::LoadOriginal {
                context,
                query,
                role,
            } => self.load_original(context, &query, role, terminated || terminal_intent),
            AppWindowCommand::CreateWindow {
                build,
                width,
                height,
                x,
                y,
                original_source,
                preload_image,
                original_role,
                original_application_name_for_user_agent,
                original_create_start_by,
            } => {
                let original = match (
                    original_source,
                    preload_image,
                    original_role,
                    original_application_name_for_user_agent,
                    original_create_start_by,
                ) {
                    (None, None, None, None, None) => None,
                    (
                        Some(source),
                        Some(image),
                        Some(role),
                        application_name_for_user_agent,
                        Some(start_by),
                    ) if self.renderer.is_some() => Some(SelectedCreate {
                        source,
                        image,
                        role,
                        start_by,
                        application_name_for_user_agent,
                    }),
                    _ => {
                        return send_failure(
                            &self.events,
                            AppWindowEvent::WindowBuildFailed {
                                build,
                                stage: crate::engine::BuildStage::Webview,
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
                    self.has_build_candidate(),
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
            AppWindowCommand::DestroyWindow { view } if !terminated => {
                let Some(native) = self.views.get(&view.0) else {
                    eprintln!("KELD_WV_STALE_DESTROY view={}", view.0);
                    return Ok(());
                };
                // Park before moving/dropping the custodian. A duplicate key
                // preserves the original pending entry and this live View.
                self.custody.park(
                    native.window.id(),
                    native.window.hwnd(),
                    Releasing::Destroy(view),
                    Instant::now(),
                )?;
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                observe::releasing(self.custody.len());
                if native.original.is_none() || native.retire_on_ui() {
                    self.views.remove(&view.0);
                }
                Ok(())
            }
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
        let building = self.has_build_candidate();
        if let Some(create) = self.custody.take_deferred_wake(ending, building) {
            self.build(target, release, create)?;
        }
        Ok(())
    }

    pub(super) fn schedule(&mut self, control_flow: &mut ControlFlow, ending: bool) {
        let now = Instant::now();
        let building = self.has_build_candidate();
        let wake_retry = self.custody.post_deferred_wake(ending, building, now, || {
            self.proxy
                .send_event(WindowsLoopEvent::DeferredBuildWake)
                .is_ok()
        });
        for deadline in [
            self.custody.next_deadline(now),
            wake_retry,
            self.original_deadline(ending),
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
    views.retain(|_, native| !retire(native));
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    observe::native_release_step("managed-stop.views-cleared", None);
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
