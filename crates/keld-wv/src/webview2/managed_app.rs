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

#[cfg(test)]
#[path = "managed_app/tests/stop_order.rs"]
mod stop_order_tests;

pub(super) struct ManagedWindows {
    views: BTreeMap<u32, View>,
    custody: ManagedCustody,
    builder: NativeBuild,
    events: Sender<AppWindowEvent>,
    proxy: EventLoopProxy<WindowsLoopEvent>,
    receipt_sender: Weak<Sender<BrowserExitReceipt>>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    test_control: Option<TestControl>,
}

impl ManagedWindows {
    pub(super) fn new(
        builder: NativeBuild,
        events: Sender<AppWindowEvent>,
        proxy: EventLoopProxy<WindowsLoopEvent>,
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
            receipt_sender,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            test_control,
        }
    }

    /// Called at the top of a later native turn, before expiry and exit checks.
    pub(super) fn publish_retired(&mut self) -> Result<(), WvError> {
        let events = &self.events;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let test_control = self.test_control.as_ref();
        let result = self.custody.publish_staged(|_hwnd, owed, event| {
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            observe::teardown_confirmed(_hwnd);
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
        let events = &self.events;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let test_control = self.test_control.as_ref();
        close_managed_resources(
            &mut self.views,
            &mut self.custody,
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
        )
    }

    pub(super) fn exit_ready(&self, terminal: bool, barrier_ready: bool) -> bool {
        self.custody.exit_ready(terminal, barrier_ready)
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
            AppWindowCommand::CreateWindow {
                build,
                width,
                height,
                x,
                y,
            } => {
                let create = DeferredCreate {
                    build,
                    width,
                    height,
                    x,
                    y,
                };
                match self.custody.admit(create, terminated || terminal_intent) {
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
                self.views.remove(&view.0);
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
        if let Some(create) = self.custody.take_deferred_wake(ending) {
            self.build(target, release, create)?;
        }
        Ok(())
    }

    pub(super) fn schedule(&mut self, control_flow: &mut ControlFlow, ending: bool) {
        let now = Instant::now();
        let wake_retry = self.custody.post_deferred_wake(ending, now, || {
            self.proxy
                .send_event(WindowsLoopEvent::DeferredBuildWake)
                .is_ok()
        });
        for deadline in [self.custody.next_deadline(now), wake_retry]
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
fn close_managed_resources<V, K: Ord + Copy, E>(
    views: &mut BTreeMap<u32, V>,
    custody: &mut ManagedCustody<K>,
    after_views_closed: impl FnOnce(),
    send: impl FnMut(AppWindowEvent) -> Result<(), E>,
) -> Result<(), E> {
    views.clear();
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
