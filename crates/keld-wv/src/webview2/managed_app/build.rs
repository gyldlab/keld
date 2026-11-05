//! The shared builder's result enters exactly one managed native custodian.
use std::rc::Rc;
use std::time::Instant;
use tao::event_loop::EventLoopWindowTarget;
use tao::platform::windows::WindowExtWindows as _;

use super::super::managed_windows::{DeferredCreate, Releasing};
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use super::super::t2_test_observation as observe;
use super::super::view_build::{BuildFailure, BuildResources, ManagedBuild};
use super::super::{ProfileReleaseWait, WindowsLoopEvent};
use super::{ManagedWindows, send, send_failure};
use crate::WvError;
use crate::engine::{AppWindowEvent, BuildStage, LogicalSize, NavTarget, WebviewSpec};

impl ManagedWindows {
    pub(super) fn build(
        &mut self,
        target: &EventLoopWindowTarget<WindowsLoopEvent>,
        release: &Rc<ProfileReleaseWait>,
        create: DeferredCreate,
    ) -> Result<(), WvError> {
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
            self.proxy.clone(),
            &spec,
            Some(ManagedBuild {
                create,
                release,
                sender: self.receipt_sender.clone(),
            }),
            None,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            self.test_control.as_mut(),
        ) {
            Ok(built) => {
                // A20: never replace an existing live native custodian, even
                // if the backend ID counter or a native identity is faulty.
                if self.views.contains_key(&built.id.0)
                    || self
                        .views
                        .values()
                        .any(|view| view.window.id() == built.view.window.id())
                {
                    return self.failed_build(
                        create,
                        BuildFailure {
                            stage: BuildStage::Window,
                            source: WvError::Window(String::from(
                                "native window identity collision",
                            )),
                            resources: Some(BuildResources::View(built.view)),
                        },
                    );
                }
                let Some(snapshot) = built.snapshot else {
                    return self.failed_build(
                        create,
                        BuildFailure::held(
                            WvError::EventLoop(String::from("managed build omitted native state")),
                            BuildResources::View(built.view),
                        ),
                    );
                };
                self.views.insert(built.id.0, built.view);
                send(
                    &self.events,
                    AppWindowEvent::WindowBuilt {
                        build: create.build,
                        view: built.id,
                        snapshot,
                    },
                )
            }
            Err(failure) => self.failed_build(create, failure),
        }
    }

    fn failed_build(
        &mut self,
        create: DeferredCreate,
        failure: BuildFailure,
    ) -> Result<(), WvError> {
        eprintln!(
            "KELD_WV_BUILD_FAILED build={} {}",
            create.build, failure.source
        );
        if let Some(resources) = failure.resources {
            let window = resources.window();
            self.custody.park(
                window.id(),
                window.hwnd(),
                Releasing::FailedBuild {
                    build: create.build,
                    stage: failure.stage,
                },
                Instant::now(),
            )?;
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            observe::releasing(self.custody.len());
            drop(resources);
            // Only later Destroyed staging plus IsWindow(FALSE) can answer.
            Ok(())
        } else {
            send_failure(
                &self.events,
                AppWindowEvent::WindowBuildFailed {
                    build: create.build,
                    stage: failure.stage,
                },
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                self.test_control.as_ref(),
            )
        }
    }
}
