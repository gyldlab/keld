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

mod geometry;
mod resources;
use geometry::correct_outer_frame;
pub(super) use geometry::snapshot;
pub(super) use resources::{BuildFailure, BuildResources, BuiltView};

#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
use super::managed_test_control::{BuildPoint, TestControl};

/// State already owned by the engine. A clone of the environment interface
/// retains the same native environment; it does not create another profile.
pub(super) struct NativeBuild {
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

impl NativeBuild {
    /// Every build owns one absolute deadline, including every nested pump.
    /// Content cannot run before the existing guarded media/popup installer.
    #[allow(clippy::too_many_lines)] // One deadline and partial-resource custody span all build stages.
    pub(super) fn build(
        &mut self,
        target: &EventLoopWindowTarget<WindowsLoopEvent>,
        proxy: &EventLoopProxy<WindowsLoopEvent>,
        spec: &WebviewSpec,
        managed: Option<&ManagedBuild<'_>>,
        navigation: Option<InitialNavigation>,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))] mut test_control: Option<
            &mut TestControl,
        >,
    ) -> Result<BuiltView, BuildFailure> {
        let deadline = Instant::now() + PROFILE_LAUNCH_DEADLINE;
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
        let hwnd = HWND(window.hwnd() as _);
        if let Some(build) = managed
            && let Err(error) = correct_outer_frame(&window, &build.create)
        {
            return Err(BuildFailure::held(error, BuildResources::Window(window)));
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let (Some(control), Some(attempt)) =
            (test_control.as_deref_mut(), build_attempt.as_mut())
            && let Err(error) =
                control.fail_at(attempt, BuildPoint::Hwnd, window.id(), window.hwnd())
        {
            return Err(BuildFailure::held(error, BuildResources::Window(window)));
        }
        let managed_release = managed.map(|build| Rc::downgrade(build.release));
        let controller_result = if managed.is_some() {
            create_controller_for_build(&self.environment, hwnd, deadline, managed_release.clone())
                .and_then(PendingController::adopt)
        } else {
            create_controller(&self.environment, hwnd, deadline)
        };
        let controller = match controller_result {
            Ok(controller) => controller,
            Err(error) => {
                return Err(BuildFailure::held(error, BuildResources::Window(window)));
            }
        };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let (Some(control), Some(attempt), Some(build)) =
            (test_control.as_deref_mut(), build_attempt.as_mut(), managed)
            && control
                .fail_at(attempt, BuildPoint::Webview, window.id(), window.hwnd())
                .is_err()
        {
            // This point replaces CoreWebView2: the controller exists, but its
            // browser identity cannot be read. Use the existing D8 fault latch.
            let error = build.release.latch_unidentified_controller();
            return Err(BuildFailure::held(
                error,
                BuildResources::Controller {
                    window,
                    controller,
                    release: managed_release,
                },
            ));
        }
        // SAFETY: the controller was just created on this UI thread.
        let webview = match unsafe { controller.CoreWebView2() } {
            Ok(webview) => webview,
            Err(error) => {
                let error = managed.map_or_else(
                    || WvError::Webview(format!("controller returned no webview: {error}")),
                    |build| build.release.latch_unidentified_controller(),
                );
                return Err(BuildFailure::held(
                    error,
                    BuildResources::Controller {
                        window,
                        controller,
                        release: managed_release,
                    },
                ));
            }
        };
        if let Some(build) = managed {
            let bound = build
                .release
                .bind_controller(&webview, proxy.clone())
                .and_then(|()| {
                    build.release.install_process_failed(
                        &webview,
                        build.sender.clone(),
                        proxy.clone(),
                    )
                });
            if let Err(error) = bound {
                return Err(BuildFailure::held(
                    error,
                    BuildResources::Controller {
                        window,
                        controller,
                        release: managed_release,
                    },
                ));
            }
        }
        let view = View {
            window,
            controller,
            webview,
            hidden_for_minimize: std::cell::Cell::new(false),
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            test_view_id: None,
            managed_release,
        };
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let (Some(control), Some(attempt)) =
            (test_control.as_deref_mut(), build_attempt.as_mut())
            && let Err(error) = control.fail_at(
                attempt,
                BuildPoint::View,
                view.window.id(),
                view.window.hwnd(),
            )
        {
            return Err(BuildFailure::held(error, BuildResources::View(view)));
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let (Some(control), Some(build)) = (test_control.as_deref(), managed)
            && let Err(error) = control.hold_build_pump(build.create.build, deadline)
        {
            return Err(BuildFailure::held(error, BuildResources::View(view)));
        }
        // Keep failed setup from consuming the next internal view identity.
        // D12 retains post-HWND custody while this checked candidate is local.
        let mut next_id = self.next_id;
        let id = match next_view_id(&mut next_id) {
            Ok(id) => id,
            Err(source) => return Err(BuildFailure::held(source, BuildResources::View(view))),
        };
        // Label this same owner only after the existing allocator checked its ID.
        // The unchanged BuiltView return below carries exactly this candidate.
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let view = {
            let mut view = view;
            view.test_view_id = Some(id);
            view
        };
        if let Err(error) = self.configure(
            &view,
            id,
            spec,
            managed.map(|_| proxy),
            navigation,
            deadline,
        ) {
            return Err(BuildFailure::held(error, BuildResources::View(view)));
        }
        let snapshot = match managed.is_some().then(|| snapshot(&view)).transpose() {
            Ok(snapshot) => snapshot,
            Err(source) => return Err(BuildFailure::held(source, BuildResources::View(view))),
        };
        if let Some(build) = managed {
            build.release.poll();
            if let Some(fault) = build.release.fault() {
                return Err(BuildFailure::held(
                    super::profile_failure_with(
                        crate::profile::ProfileErrorKind::LifecycleUnproven,
                        fault.context(build.release.site),
                    ),
                    BuildResources::View(view),
                ));
            }
        }
        self.next_id = next_id;
        Ok(BuiltView { id, view, snapshot })
    }

    fn configure(
        &mut self,
        view: &View,
        id: WebviewId,
        spec: &WebviewSpec,
        managed_proxy: Option<&EventLoopProxy<WindowsLoopEvent>>,
        navigation: Option<InitialNavigation>,
        deadline: Instant,
    ) -> Result<(), WvError> {
        if !self.saved_permissions_reconciled && saved_permission_reconciliation_enabled() {
            reconcile_saved_media_permissions(&view.webview, deadline)?;
            self.saved_permissions_reconciled = true;
        }
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
