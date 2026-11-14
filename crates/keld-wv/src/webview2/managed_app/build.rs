//! The shared builder's result enters exactly one managed native custodian.
use std::rc::Rc;
use std::time::Instant;
use tao::event_loop::EventLoopWindowTarget;
use tao::platform::windows::WindowExtWindows as _;

use super::super::managed_windows::{DeferredCreate, Releasing};
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use super::super::t2_test_observation as observe;
use super::super::view_build::{
    BuildFailure, BuildFailureDisposition, BuildNavigation, BuildProgress, CompletedBuild,
    ManagedBuild,
};
use super::super::{ProfileReleaseWait, WindowsLoopEvent};
use super::{ManagedWindows, send, send_failure};
use crate::WvError;
use crate::engine::{AppWindowEvent, LogicalSize, NavTarget, WebviewSpec};

impl ManagedWindows {
    pub(super) fn build(
        &mut self,
        target: &EventLoopWindowTarget<WindowsLoopEvent>,
        release: &Rc<ProfileReleaseWait>,
        mut create: DeferredCreate,
    ) -> Result<(), WvError> {
        let build = create.build;
        let navigation = if let Some(inputs) = create.original.take() {
            match self.prepare_original_build(inputs) {
                Ok(original) => BuildNavigation::Original(original),
                Err(error) => {
                    eprintln!("KELD_WV_ORIGINAL_PREPARATION_FAILED build={build} {error}");
                    return send_failure(
                        &self.events,
                        create.failure_event(),
                        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                        self.test_control.as_ref(),
                    );
                }
            }
        } else {
            BuildNavigation::Renderer(None)
        };
        let spec = WebviewSpec {
            title: String::from("Keld"),
            size: LogicalSize {
                width: f64::from(create.width),
                height: f64::from(create.height),
            },
            initial: NavTarget::Html(String::new()),
        };
        match self.builder.build(
            target,
            &self.proxy,
            &spec,
            Some(&ManagedBuild {
                create: &create,
                release,
                sender: self.receipt_sender.clone(),
            }),
            navigation,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            self.test_control.as_mut(),
        ) {
            Ok(BuildProgress::Complete(completed)) => self.complete_build(build, completed),
            Ok(BuildProgress::PendingOriginal) => Ok(()),
            Err(failure) => self.failed_build(build, failure),
        }
    }

    pub(super) fn complete_build(
        &mut self,
        build: u64,
        completed: CompletedBuild,
    ) -> Result<(), WvError> {
        let Some(snapshot) = completed.snapshot() else {
            return self.failed_build(
                build,
                BuildFailure::webview(WvError::EventLoop(String::from(
                    "managed build omitted native state",
                ))),
            );
        };
        let view = match self
            .builder
            .transfer_completed_into(&mut self.views, completed)
        {
            Ok(view) => view,
            Err(source) => return self.failed_build(build, BuildFailure::collision(source)),
        };
        self.activation_observer.invalidate();
        send(
            &self.events,
            AppWindowEvent::WindowBuilt {
                build,
                view,
                snapshot,
            },
        )
    }

    pub(super) fn failed_build(
        &mut self,
        build: u64,
        failure: BuildFailure,
    ) -> Result<(), WvError> {
        let BuildFailure {
            disposition,
            stage,
            source,
        } = failure;
        eprintln!("KELD_WV_BUILD_FAILED build={build} {source}");
        if disposition == BuildFailureDisposition::NotStarted {
            // The occupied candidate belongs to an earlier invocation. Never
            // park, retire or relabel it with this new build correlation.
            return Err(source);
        }
        let cleanup = if let Some(window) = self.builder.candidate_window() {
            let parked = self.custody.park(
                window.id(),
                window.hwnd(),
                Releasing::FailedBuild { build, stage },
                Instant::now(),
            );
            if let Err(error) = parked {
                Err(error) // candidate remains stored before return
            } else {
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                observe::releasing(self.custody.len());
                if self.builder.retire_candidate_on_ui() {
                    // Only later Destroyed staging plus IsWindow(FALSE)
                    // can answer the scalar record just parked above.
                    Ok(())
                } else {
                    Err(super::super::profile_failure_with(
                        crate::profile::ProfileErrorKind::LifecycleUnproven,
                        "failed construction still owns an unresolved native candidate",
                    ))
                }
            }
        } else if disposition == BuildFailureDisposition::Unwound {
            // No returned resource does not turn a caught panic into ordinary
            // failure success. The outer owner receives the fatal source below.
            Ok(())
        } else {
            send_failure(
                &self.events,
                AppWindowEvent::WindowBuildFailed { build, stage },
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                self.test_control.as_ref(),
            )
        };
        if disposition == BuildFailureDisposition::Unwound {
            if let Err(error) = cleanup {
                eprintln!("KELD_WV_BUILD_UNWIND_CLEANUP {error}");
            }
            Err(source)
        } else {
            cleanup
        }
    }
}
