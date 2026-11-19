//! One UI-thread native builder for renderer and registry-owned Windows views.
#![deny(unsafe_op_in_unsafe_fn)]

use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use tao::platform::windows::WindowExtWindows as _;
use tao::window::WindowBuilder;
use windows::Win32::Foundation::HWND;

use super::browser_instance::BrowserExitReceipt;
use super::controller_custody::PendingController;
use super::managed_windows::DeferredCreate;
use super::navigation_diagnostics::FirstNavigationDiagnostic;
use super::view_host::install_controller_focus;
use super::{
    ICoreWebView2Environment, PROFILE_LAUNCH_DEADLINE, ProfileReleaseWait, View, WindowsLoopEvent,
    create_controller, create_controller_for_build, install_app_navigation_handler,
    install_guarded_media_permissions, navigate_initial, reconcile_saved_media_permissions,
    saved_permission_reconciliation_enabled,
};
use crate::WebviewId;
use crate::engine::{AppWindowEvent, WebviewSpec};
use crate::error::WvError;
use crate::media::webview_media_principal;
use crate::native_window::next_view_id;

mod candidate;
mod geometry;
pub(super) mod original;
mod resources;
use geometry::correct_outer_frame;
pub(super) use geometry::snapshot;
use original::OriginalBuild;
use resources::BuildResources;
pub(super) use resources::{BuildFailure, BuildFailureDisposition, CompletedBuild};

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use super::managed_test_control::{BuildPoint, TestControl};

/// State already owned by the engine. A clone of the environment interface
/// retains the same native environment; it does not create another profile.
pub(super) struct NativeBuild {
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
    pub(super) create: DeferredCreate,
    pub(super) release: &'a Rc<ProfileReleaseWait>,
    pub(super) sender: Weak<Sender<BrowserExitReceipt>>,
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

enum BuildNavigation {
    Ordinary(Option<InitialNavigation>),
    Original(OriginalBuild),
}

enum BuildProgress {
    Complete(CompletedBuild),
    OriginalPending,
}

impl NativeBuild {
    /// Every build owns one absolute deadline, including every nested pump.
    /// Content cannot run before the existing guarded media/popup installer.
    pub(super) fn build(
        &mut self,
        target: &EventLoopWindowTarget<WindowsLoopEvent>,
        proxy: &EventLoopProxy<WindowsLoopEvent>,
        spec: &WebviewSpec,
        managed: Option<&ManagedBuild<'_>>,
        navigation: Option<InitialNavigation>,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))] test_control: Option<
            &mut TestControl,
        >,
    ) -> Result<CompletedBuild, BuildFailure> {
        let progress = self.build_navigation(
            target,
            proxy,
            spec,
            managed,
            BuildNavigation::Ordinary(navigation),
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            test_control,
        )?;
        match progress {
            BuildProgress::Complete(completed) => Ok(completed),
            BuildProgress::OriginalPending => {
                Err(BuildFailure::webview(candidate::candidate_error(
                    "ordinary construction entered the selected preload path",
                )))
            }
        }
    }

    fn build_navigation(
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
            return Err(BuildFailure::blocked(candidate::candidate_error(
                "selected construction requires the managed native owner",
            )));
        }
        resources::contain_build(|| {
            self.build_started(
                target,
                proxy,
                spec,
                managed,
                navigation,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                test_control,
            )
        })
    }

    #[allow(clippy::too_many_lines)] // One deadline and partial-resource custody span all build stages.
    fn build_started(
        &mut self,
        target: &EventLoopWindowTarget<WindowsLoopEvent>,
        proxy: &EventLoopProxy<WindowsLoopEvent>,
        spec: &WebviewSpec,
        managed: Option<&ManagedBuild<'_>>,
        navigation: BuildNavigation,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))] mut test_control: Option<
            &mut TestControl,
        >,
    ) -> Result<BuildProgress, BuildFailure> {
        let deadline = match &navigation {
            BuildNavigation::Ordinary(_) => Instant::now() + PROFILE_LAUNCH_DEADLINE,
            BuildNavigation::Original(original) => original.deadline,
        };
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
        let window = self.window().map_err(BuildFailure::webview)?;
        let hwnd = HWND(window.hwnd() as _);
        if let Some(build) = managed {
            correct_outer_frame(window, &build.create).map_err(BuildFailure::webview)?;
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let (Some(control), Some(attempt)) =
            (test_control.as_deref_mut(), build_attempt.as_mut())
        {
            control
                .fail_at(attempt, BuildPoint::Hwnd, window.id(), window.hwnd())
                .map_err(BuildFailure::webview)?;
        }
        let managed_release = managed.map(|build| Rc::downgrade(build.release));
        let controller = if managed.is_some() {
            create_controller_for_build(&self.environment, hwnd, deadline, managed_release.clone())
                .and_then(PendingController::adopt)
        } else {
            create_controller(&self.environment, hwnd, deadline)
        }
        .map_err(BuildFailure::webview)?;
        self.promote_controller(controller, managed_release)
            .map_err(BuildFailure::webview)?;
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
        // SAFETY: this controller is retained by the candidate's one Close
        // custodian on the creating STA before this getter can fail or reenter.
        let webview = unsafe { controller.CoreWebView2() }.map_err(|error| {
            BuildFailure::webview(managed.map_or_else(
                || WvError::Webview(format!("controller returned no webview: {error}")),
                |build| build.release.latch_unidentified_controller(),
            ))
        })?;
        // Install the full same View before binding or installing observers.
        self.promote_view(webview).map_err(BuildFailure::webview)?;
        let view = self.view().map_err(BuildFailure::webview)?;
        if let Some(build) = managed {
            build
                .release
                .bind_controller(&view.webview, proxy.clone())
                .and_then(|()| {
                    build.release.install_process_failed(
                        &view.webview,
                        build.sender.clone(),
                        proxy.clone(),
                    )
                })
                .map_err(BuildFailure::webview)?;
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let (Some(control), Some(attempt)) =
            (test_control.as_deref_mut(), build_attempt.as_mut())
        {
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
        if let (Some(control), Some(build)) = (test_control.as_deref_mut(), managed) {
            control
                .hold_build_pump(build.create.build, deadline)
                .map_err(BuildFailure::webview)?;
        }
        // Reconciliation has no view principal yet. Its failure preserves the
        // first ID for the healthy retry under the same construction deadline.
        self.reconcile_permissions(deadline)
            .map_err(BuildFailure::webview)?;
        // D4 consumes the checked identity before a guard can capture it.
        // Any later failure retains native custody but never reuses that ID.
        let id = next_view_id(&mut self.next_id).map_err(BuildFailure::webview)?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        {
            self.view_mut().map_err(BuildFailure::webview)?.test_view_id = Some(id);
        }
        let navigation = match navigation {
            BuildNavigation::Ordinary(navigation) => navigation,
            BuildNavigation::Original(inputs) => {
                let build = managed.ok_or_else(|| {
                    BuildFailure::webview(candidate::candidate_error(
                        "selected build lost its managed native owner",
                    ))
                })?;
                self.attach_original(id, deadline, build.create.build, inputs, proxy)
                    .map_err(BuildFailure::webview)?;
                return Ok(BuildProgress::OriginalPending);
            }
        };
        let view = self.view().map_err(BuildFailure::webview)?;
        Self::configure(view, id, spec, managed.map(|_| proxy), navigation)
            .map_err(BuildFailure::webview)?;
        let snapshot = managed
            .is_some()
            .then(|| snapshot(view))
            .transpose()
            .map_err(BuildFailure::webview)?;
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
        let guard = Self::install_guard(&view.webview, id)?;
        Self::configure_guarded_shell(view, id, managed_proxy)?;
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

    fn install_guard(
        view: &super::ICoreWebView2,
        id: WebviewId,
    ) -> Result<super::guarded_navigation::GuardInstalled<'_>, WvError> {
        #[cfg(not(all(feature = "media-acceptance", test)))]
        let manifest = keld_guard::PermissionsManifest::default();
        #[cfg(all(feature = "media-acceptance", test))]
        let manifest = super::media_acceptance::fixture_manifest();
        install_guarded_media_permissions(view, manifest, webview_media_principal(id))
    }

    fn configure_guarded_shell(
        view: &View,
        id: WebviewId,
        managed_proxy: Option<&EventLoopProxy<WindowsLoopEvent>>,
    ) -> Result<(), WvError> {
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
        Ok(())
    }
}
