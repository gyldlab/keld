//! One UI-thread native builder for renderer and registry-owned Windows views.
#![deny(unsafe_op_in_unsafe_fn)]

use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use tao::platform::windows::WindowExtWindows as _;
use tao::window::WindowBuilder;

use super::browser_instance::BrowserExitReceipt;
use super::managed_windows::DeferredCreate;
use super::navigation_diagnostics::FirstNavigationDiagnostic;
use super::view_host::install_controller_focus;
use super::{
    ICoreWebView2Environment, PROFILE_LAUNCH_DEADLINE, ProfileReleaseWait, View, WindowsLoopEvent,
    install_app_navigation_handler, install_guarded_media_permissions, navigate_initial,
    reconcile_saved_media_permissions, saved_permission_reconciliation_enabled,
};
use crate::WebviewId;
use crate::engine::{AppWindowEvent, WebviewSpec};
use crate::error::WvError;
use crate::media::webview_media_principal;
use crate::native_window::next_view_id;

mod candidate;
mod geometry;
mod original;
mod resources;
use geometry::correct_outer_frame;
pub(super) use geometry::snapshot;
pub(super) use resources::{BuildFailure, BuildFailureDisposition, BuildResources, CompletedBuild};

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use super::managed_test_control::{BuildPoint, TestControl};

/// State already owned by the engine. A clone of the environment interface
/// retains the same native environment; it does not create another profile.
pub(super) struct NativeBuild {
    // Actual native candidate precedes the environment in this same owner.
    candidate: Option<BuildResources>,
    completed: Option<BuildCompletion>,
    pub(super) environment: ICoreWebView2Environment,
    pub(super) next_id: u32,
    pub(super) saved_permissions_reconciled: bool,
}

pub(super) struct InitialNavigation {
    pub(super) proxy: EventLoopProxy<WindowsLoopEvent>,
    pub(super) events: Sender<AppWindowEvent>,
    pub(super) ready: Arc<AtomicBool>,
    pub(super) failed: Arc<AtomicBool>,
    pub(super) diagnostic: Arc<Mutex<FirstNavigationDiagnostic>>,
}

pub(super) struct ManagedBuild<'a> {
    pub(super) create: &'a DeferredCreate,
    pub(super) release: &'a Rc<ProfileReleaseWait>,
    pub(super) sender: Weak<Sender<BrowserExitReceipt>>,
}

pub(super) struct OriginalBuild {
    pub(super) inputs: super::original_view::OriginalBuildInputs,
    pub(super) session: Rc<super::original_resource::ResourceSession>,
    pub(super) endpoint: crate::renderer_bridge::RendererBridgeEndpoint,
    pub(super) events: Sender<AppWindowEvent>,
}

pub(super) enum BuildNavigation {
    Renderer(Option<InitialNavigation>),
    Original(OriginalBuild),
}

pub(super) enum BuildProgress {
    Complete(CompletedBuild),
    PendingOriginal,
}

#[derive(Clone, Copy, PartialEq)]
enum BuildCompletion {
    Ready {
        id: WebviewId,
        snapshot: Option<crate::engine::WindowSnapshot>,
    },
    OriginalPending {
        build: u64,
        id: WebviewId,
        deadline: Instant,
    },
}

impl NativeBuild {
    /// `NativeBuild` is already retained in its managed/engine owner before entry.
    /// The one absolute deadline and candidate survive error and inner unwind.
    pub(super) fn build(
        &mut self,
        target: &EventLoopWindowTarget<WindowsLoopEvent>,
        proxy: &EventLoopProxy<WindowsLoopEvent>,
        spec: &WebviewSpec,
        managed: Option<&ManagedBuild<'_>>,
        navigation: BuildNavigation,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))] test_control: Option<
            &mut TestControl,
        >,
    ) -> Result<BuildProgress, BuildFailure> {
        if !self.candidate_is_empty() {
            return Err(BuildFailure::blocked(candidate::candidate_error(
                "another construction candidate is still owned by this builder",
            )));
        }
        if matches!(&navigation, BuildNavigation::Original(_)) && managed.is_none() {
            return Err(BuildFailure::blocked(WvError::EventLoop(String::from(
                "original construction requires the retained managed Host owner",
            ))));
        }
        let deadline = Instant::now() + PROFILE_LAUNCH_DEADLINE;
        resources::contain_build(|| {
            self.build_started(
                target,
                proxy,
                spec,
                managed,
                navigation,
                deadline,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                test_control,
            )
        })
    }

    #[allow(clippy::too_many_lines)] // One original deadline spans all native construction stages.
    #[cfg_attr(
        all(feature = "profile-test-hooks", debug_assertions),
        expect(
            clippy::too_many_arguments,
            reason = "The existing debug control is passed beside the same native owners and original absolute deadline."
        )
    )]
    fn build_started(
        &mut self,
        target: &EventLoopWindowTarget<WindowsLoopEvent>,
        proxy: &EventLoopProxy<WindowsLoopEvent>,
        spec: &WebviewSpec,
        managed: Option<&ManagedBuild<'_>>,
        navigation: BuildNavigation,
        deadline: Instant,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))] mut test_control: Option<
            &mut TestControl,
        >,
    ) -> Result<BuildProgress, BuildFailure> {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let mut build_attempt = managed.and_then(|build| {
            test_control
                .as_deref_mut()
                .map(|control| control.begin_build(build.create.build))
        });
        #[cfg(all(feature = "media-acceptance", test))]
        super::saved_media_permissions::test_control::record_build(deadline);
        let mut builder = WindowBuilder::new()
            .with_title(&spec.title)
            .with_visible(managed.is_none())
            .with_resizable(true)
            .with_minimizable(true)
            .with_closable(true);
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let (Some(control), Some(build)) = (test_control.as_deref(), managed) {
            // Tao 0.35.3 retains this flag through hidden construction and uses
            // SW_SHOWNOACTIVATE for the real first show. No focus fact is supplied.
            // https://docs.rs/tao/0.35.3/tao/window/struct.WindowBuilder.html#method.with_focused
            builder = builder.with_focused(
                control
                    .initially_focused(build.create.build)
                    .map_err(BuildFailure::window)?,
            );
        }
        if managed.is_none() {
            builder = builder.with_inner_size(LogicalSize::new(spec.size.width, spec.size.height));
        }
        if let Some(build) = managed {
            // An omitted axis preserves the OS-chosen position. Apply either
            // supplied axis after creation, when that position can be sampled.
            if let (Some(x), Some(y)) = (build.create.x, build.create.y) {
                builder = builder.with_position(LogicalPosition::new(x, y));
            }
        }
        let window = builder
            .build(target)
            .map_err(|error| BuildFailure::window(WvError::Window(error.to_string())))?;
        self.store_window(window);
        if let Some(build) = managed {
            correct_outer_frame(self.window().map_err(BuildFailure::webview)?, build.create)
                .map_err(BuildFailure::webview)?;
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let (Some(control), Some(attempt)) =
            (test_control.as_deref_mut(), build_attempt.as_mut())
        {
            let window = self.window().map_err(BuildFailure::webview)?;
            control
                .fail_at(attempt, BuildPoint::Hwnd, window.id(), window.hwnd())
                .map_err(BuildFailure::webview)?;
        }
        let managed_release = managed.map(|build| Rc::downgrade(build.release));
        self.begin_controller(managed_release.clone(), deadline)
            .map_err(BuildFailure::webview)?;
        self.promote_controller().map_err(BuildFailure::webview)?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let (Some(control), Some(attempt), Some(build)) =
            (test_control.as_deref_mut(), build_attempt.as_mut(), managed)
        {
            let window = self.window().map_err(BuildFailure::webview)?;
            if control
                .fail_at(attempt, BuildPoint::Webview, window.id(), window.hwnd())
                .is_err()
            {
                return Err(BuildFailure::webview(
                    build.release.latch_unidentified_controller(),
                ));
            }
        }
        let controller = self.controller().map_err(BuildFailure::webview)?;
        // SAFETY: this exact controller is retained by the candidate's one Close
        // custodian on the creating STA, before this getter can fail or reenter.
        let webview = match unsafe { controller.CoreWebView2() } {
            Ok(webview) => webview,
            Err(error) => {
                let error = managed.map_or_else(
                    || WvError::Webview(format!("controller returned no webview: {error}")),
                    |build| build.release.latch_unidentified_controller(),
                );
                return Err(BuildFailure::webview(error));
            }
        };
        if let Some(build) = managed {
            build
                .release
                .bind_controller(&webview, proxy.clone())
                .and_then(|()| {
                    build.release.install_process_failed(
                        &webview,
                        build.sender.clone(),
                        proxy.clone(),
                    )
                })
                .map_err(BuildFailure::webview)?;
        }
        self.promote_view(webview, managed_release)
            .map_err(BuildFailure::webview)?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let (Some(control), Some(attempt)) =
            (test_control.as_deref_mut(), build_attempt.as_mut())
        {
            let view = self.view().map_err(BuildFailure::webview)?;
            control
                .fail_at(
                    attempt,
                    BuildPoint::View,
                    view.window.id(),
                    view.window.hwnd(),
                )
                .map_err(BuildFailure::webview)?;
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let (Some(control), Some(build)) = (test_control.as_deref(), managed) {
            control
                .hold_build_pump(build.create.build, deadline)
                .map_err(BuildFailure::webview)?;
        }
        // The View stays in the candidate during every remaining fallible step.
        self.reconcile_permissions(deadline)
            .map_err(BuildFailure::webview)?;
        let id = next_view_id(&mut self.next_id).map_err(BuildFailure::webview)?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        {
            self.view_mut().map_err(BuildFailure::webview)?.test_view_id = Some(id);
        }
        let navigation = self
            .attach_original(navigation, proxy, id, deadline)
            .map_err(BuildFailure::webview)?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(directory) = test_control
            .as_deref()
            .and_then(super::managed_test_control::TestControl::synthetic_post_directory)
        {
            let view = self.view_mut().map_err(BuildFailure::webview)?;
            if let Some(original) = &view.original {
                original
                    .attach_synthetic_post(directory)
                    .map_err(BuildFailure::webview)?;
            }
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if test_control
            .as_deref()
            .is_some_and(super::managed_test_control::TestControl::observes_frame_ingress)
        {
            let view = self.view_mut().map_err(BuildFailure::webview)?;
            if let Some(original) = &mut view.original {
                original
                    .attach_frame_observer(&view.webview, Rc::clone(&view.close_custody))
                    .map_err(BuildFailure::webview)?;
            }
        }
        Self::configure(
            self.view().map_err(BuildFailure::webview)?,
            id,
            spec,
            managed.map(|_| proxy),
            navigation,
        )
        .map_err(BuildFailure::webview)?;
        if self
            .view()
            .map_err(BuildFailure::webview)?
            .original
            .is_some()
        {
            let build = managed.ok_or_else(|| {
                BuildFailure::webview(WvError::EventLoop(String::from(
                    "original build lost its managed owner",
                )))
            })?;
            self.completed = Some(BuildCompletion::OriginalPending {
                build: build.create.build,
                id,
                deadline,
            });
            return self.resume_original(build.release);
        }
        let snapshot = if managed.is_some() {
            Some(
                snapshot(self.view().map_err(BuildFailure::webview)?)
                    .map_err(BuildFailure::webview)?,
            )
        } else {
            None
        };
        if let Some(build) = managed {
            build.release.poll();
            if let Some(fault) = build.release.fault() {
                return Err(BuildFailure::webview(super::profile_failure_with(
                    crate::profile::ProfileErrorKind::LifecycleUnproven,
                    fault.context(build.release.site),
                )));
            }
        }
        self.completed = Some(BuildCompletion::Ready { id, snapshot });
        Ok(BuildProgress::Complete(CompletedBuild { id, snapshot }))
    }

    fn reconcile_permissions(&mut self, deadline: Instant) -> Result<(), WvError> {
        if !self.saved_permissions_reconciled && saved_permission_reconciliation_enabled() {
            reconcile_saved_media_permissions(&self.view()?.webview, deadline)?;
            self.saved_permissions_reconciled = true;
        }
        Ok(())
    }

    fn configure(
        view: &View,
        id: WebviewId,
        spec: &WebviewSpec,
        managed_proxy: Option<&EventLoopProxy<WindowsLoopEvent>>,
        navigation: Option<InitialNavigation>,
    ) -> Result<(), WvError> {
        #[cfg(not(all(feature = "media-acceptance", test)))]
        let manifest = keld_guard::PermissionsManifest::default();
        #[cfg(all(feature = "media-acceptance", test))]
        let manifest = super::media_acceptance::fixture_manifest();
        let guard = install_guarded_media_permissions(
            &view.webview,
            manifest,
            webview_media_principal(id),
        )?;
        // SAFETY: settings and controller stay on the creating UI thread.
        unsafe {
            view.webview
                .Settings()
                .and_then(|settings| settings.SetAreDevToolsEnabled(cfg!(debug_assertions)))
        }
        .map_err(|error| WvError::Webview(format!("devtools setting: {error}")))?;
        view.set_controller_bounds(view.window.inner_size())
            .map_err(|error| WvError::Webview(format!("initial bounds: {error}")))?;
        // SAFETY: the controller remains retained on its UI thread.
        unsafe { view.controller.SetIsVisible(true) }
            .map_err(|error| WvError::Webview(format!("initial visibility: {error}")))?;
        if let Some(proxy) = managed_proxy {
            install_controller_focus(&view.controller, id, proxy)?;
        }
        if let Some(original) = &view.original {
            // The exact same View's guard above already denies permissions/popups.
            // Resource and preload owners were attached before this native setup.
            original.install_on_ui()?;
            return Ok(());
        }
        if let Some(navigation) = navigation {
            install_app_navigation_handler(
                &view.webview,
                navigation.events,
                navigation.ready,
                navigation.failed,
                Arc::clone(&navigation.diagnostic),
                navigation.proxy,
            )?;
            navigation
                .diagnostic
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .dispatch_attempted();
        }
        navigate_initial(&guard, &spec.initial)?;
        if managed_proxy.is_some() {
            view.window.set_visible(true);
        }
        let _ = view.return_focus_if_active();
        Ok(())
    }
}
