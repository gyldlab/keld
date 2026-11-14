//! The complete native View owns these exact original-document components.
//! No callback, native call, or resource session is owned by a temporary result.
#![forbid(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, mpsc::Sender};
use std::time::Instant;

use tao::event_loop::EventLoopProxy;
use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2, ICoreWebView2Environment};

use super::WindowsLoopEvent;
use super::controller_custody::AdoptedControllerClose;
use super::original_bridge::{InstallStart, InstallStatus, OriginalBridge, PreloadInstaller};
use super::original_callbacks::NavigationCallbacks;
use super::original_load::NativeLoadCall;
use super::original_navigation::OriginalNavigation;
use super::original_resource::{ResourceSession, ResourceView};
use crate::engine::{AppWindowEvent, NativeNavigationContext, OriginalNavigationFact};
use crate::preload_image::AppPreloadImage;
use crate::renderer_bridge::{RendererBridgeEndpoint, RendererBridgeOutcome, RendererRoleWitness};
use crate::resource_intake::RetainedByteCredit;
use crate::resource_source::AdmittedWebAppSource;
use crate::{WebviewId, WvError};

mod lifecycle;
mod load;

/// The actual `CreateWindow` inputs travel in its existing deferred FIFO.
#[derive(Debug)]
pub(super) struct OriginalBuildInputs {
    pub(super) source: Arc<AdmittedWebAppSource>,
    pub(super) image: Arc<AppPreloadImage>,
    pub(super) role: RendererRoleWitness,
}

pub(super) struct OriginalView {
    id: WebviewId,
    navigation: Rc<RefCell<OriginalNavigation>>,
    session: Rc<ResourceSession>,
    resource: Option<Rc<ResourceView>>,
    callbacks: Option<NavigationCallbacks>,
    installer: Option<PreloadInstaller>,
    bridge: Option<Rc<OriginalBridge>>,
    call: Option<NativeLoadCall>,
    inputs: OriginalBuildInputs,
    endpoint: Option<RendererBridgeEndpoint>,
    proxy: EventLoopProxy<WindowsLoopEvent>,
    events: Sender<AppWindowEvent>,
    deadline: Instant,
    retired: Cell<bool>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    controller_close_observed: Cell<bool>,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    frame_ingress: Option<Rc<super::managed_test_control::frame_ingress::FrameIngress>>,
    _metadata: RetainedByteCredit,
}

// Conservative fixed Rust component metadata, paid by the already shared source
// account. Native COM storage and the separately counted script/input payloads
// are not claimed to be measured by this reservation.
const METADATA_BYTES: usize = 8192;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
const FRAME_OBSERVER_METADATA_BYTES: usize =
    super::managed_test_control::frame_ingress::METADATA_BYTES;
#[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
const FRAME_OBSERVER_METADATA_BYTES: usize = 0;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
const NAV_OBSERVER_METADATA_BYTES: usize =
    super::managed_test_control::navigation_post::METADATA_BYTES;
#[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
const NAV_OBSERVER_METADATA_BYTES: usize = 0;
const _: () = assert!(
    FRAME_OBSERVER_METADATA_BYTES
        + NAV_OBSERVER_METADATA_BYTES
        + std::mem::size_of::<OriginalView>()
        + std::mem::size_of::<OriginalNavigation>()
        + std::mem::size_of::<NavigationCallbacks>()
        + std::mem::size_of::<PreloadInstaller>()
        <= METADATA_BYTES
);

fn failure(detail: &'static str) -> WvError {
    WvError::EventLoop(detail.to_owned())
}

impl OriginalView {
    /// Pure preparation. `NativeBuild` attaches Self to the retained View before
    /// `attach_native_owners` performs the first native identity/QI operation.
    pub(super) fn prepare(
        inputs: OriginalBuildInputs,
        session: Rc<ResourceSession>,
        endpoint: RendererBridgeEndpoint,
        proxy: EventLoopProxy<WindowsLoopEvent>,
        events: Sender<AppWindowEvent>,
        id: WebviewId,
        deadline: Instant,
    ) -> Result<Self, WvError> {
        // The creation witness already captures this physical WindowRef/Entry.
        // Missing scope cannot be replaced by current-role or queue emptiness.
        if !inputs.role.has_native_view_scope() {
            return Err(failure(
                "original view lacks its actual Core lifetime scope",
            ));
        }
        let metadata =
            RetainedByteCredit::reserve(&inputs.source.retained_credit_account(), METADATA_BYTES)
                .map_err(|_| failure("original view metadata budget exhausted"))?;
        let navigation = OriginalNavigation::new(id, Arc::clone(&inputs.source))
            .map_err(|_| failure("original view source binding refused"))?;
        Ok(Self {
            id,
            navigation: Rc::new(RefCell::new(navigation)),
            session,
            resource: None,
            callbacks: None,
            installer: None,
            bridge: None,
            call: None,
            inputs,
            endpoint: Some(endpoint),
            proxy,
            events,
            deadline,
            retired: Cell::new(false),
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            controller_close_observed: Cell::new(false),
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            frame_ingress: None,
            _metadata: metadata,
        })
    }

    /// Attach each owner immediately, before preparing/installing the next one.
    /// The containing View is already the `NativeBuild` candidate at every step.
    pub(super) fn attach_native_owners(
        &mut self,
        core: &ICoreWebView2,
        environment: &ICoreWebView2Environment,
        close: Rc<AdoptedControllerClose>,
    ) -> Result<(), WvError> {
        if self.resource.is_some() {
            return Err(failure("original native owners were already attached"));
        }
        self.resource = Some(ResourceView::bind(
            core,
            environment,
            Rc::clone(&self.navigation),
            Rc::clone(&self.session),
            self.id,
            Rc::clone(&close),
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            &self.inputs.image,
        )?);
        self.callbacks = Some(NavigationCallbacks::prepare(
            core.clone(),
            Rc::clone(&self.navigation),
            Rc::clone(&self.session),
            self.id,
        ));
        self.installer = Some(PreloadInstaller::prepare(
            core.clone(),
            self.inputs.role.clone(),
            &self.inputs.image,
            self.deadline,
            self.proxy.clone(),
            self.id,
        )?);
        let endpoint = self
            .endpoint
            .take()
            .ok_or_else(|| failure("original view lacks its retained endpoint"))?;
        self.bridge = Some(OriginalBridge::prepare(
            core,
            Rc::clone(&self.navigation),
            Rc::clone(&self.session),
            endpoint,
            self.proxy.clone(),
            self.id,
            close,
        )?);
        Ok(())
    }

    /// Configure only already-owned test metadata before navigation subscriptions.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn attach_synthetic_post(&self, directory: &std::path::Path) -> Result<(), WvError> {
        let navigation = self
            .navigation
            .try_borrow()
            .map_err(|_| failure("synthetic post metadata owner borrowed"))?;
        navigation.post_observer().configure_synthetic(directory)
    }

    /// Test-only observation attaches to the complete existing View before Add/QI.
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn attach_frame_observer(
        &mut self,
        core: &ICoreWebView2,
        close: Rc<AdoptedControllerClose>,
    ) -> Result<(), WvError> {
        if self.frame_ingress.is_some() {
            return Err(failure("frame observer was already attached"));
        }
        self.frame_ingress = Some(
            super::managed_test_control::frame_ingress::FrameIngress::prepare(
                self.id,
                core.clone(),
                Rc::clone(&self.navigation),
                close,
                self.proxy.clone(),
            ),
        );
        Ok(())
    }

    /// `NativeBuild` calls this only after the same View's `GuardInstalled` proof.
    /// Returning from `AddScript` is not readiness: its real completion is required.
    pub(super) fn install_on_ui(&self) -> Result<(), WvError> {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(observer) = &self.frame_ingress {
            observer
                .install_on_ui()
                .map_err(|_| failure("frame observer installation refused"))?;
        }
        self.resource
            .as_ref()
            .ok_or_else(|| failure("original resource owner absent"))?
            .install_on_ui()?;
        let events = self.events.clone();
        let session = Rc::clone(&self.session);
        let proxy = self.proxy.clone();
        let id = self.id;
        let observe = Rc::new(move |context, fact| {
            if let Some(context) = context
                && events
                    .send(AppWindowEvent::OriginalNavigation { context, fact })
                    .is_err()
            {
                session.latch_failure("original navigation fact receiver ended", id);
            }
            if proxy.send_event(WindowsLoopEvent::OriginalWake).is_err() {
                session.latch_failure("original navigation service wake failed", id);
            }
        });
        self.callbacks
            .as_ref()
            .ok_or_else(|| failure("original callbacks absent"))?
            .install_on_ui(observe)
            .map_err(|_| failure("original navigation subscription failed"))?;
        self.bridge
            .as_ref()
            .ok_or_else(|| failure("original bridge absent"))?
            .install_on_ui()?;
        match self
            .installer
            .as_ref()
            .ok_or_else(|| failure("original preload owner absent"))?
            .begin_on_ui()
        {
            InstallStart::Returned(Ok(())) => Ok(()),
            InstallStart::OutcomeUnknown => {
                self.session
                    .latch_failure("original preload native return is unproved", self.id);
                Err(failure("original preload native return is unproved"))
            }
            InstallStart::NotAdmitted
            | InstallStart::AlreadyStarted
            | InstallStart::Returned(Err(_)) => {
                Err(failure("original preload installation refused"))
            }
        }
    }

    pub(super) fn build_status(&self, now: Instant) -> InstallStatus {
        if !self.inputs.role.is_live() {
            return InstallStatus::Refused;
        }
        self.status(now)
    }

    pub(super) fn is_retired(&self) -> bool {
        self.retired.get()
    }

    pub(super) fn status(&self, now: Instant) -> InstallStatus {
        if self.retired.get() {
            return InstallStatus::Refused;
        }
        self.installer
            .as_ref()
            .map_or(InstallStatus::Refused, |owner| owner.status(now))
    }

    pub(super) fn deliver_on_ui(
        &self,
        pending: &mut Option<RendererBridgeOutcome>,
    ) -> Result<bool, WvError> {
        self.bridge
            .as_ref()
            .ok_or_else(|| failure("original bridge delivery owner absent"))?
            .deliver_on_ui(pending)
    }

    pub(super) fn drive_on_ui(&self) -> Result<(), WvError> {
        let expired = {
            let mut navigation = self
                .navigation
                .try_borrow_mut()
                .map_err(|_| failure("original deadline met a held document borrow"))?;
            let observed = (
                navigation.host_context(),
                navigation.active_attempt(),
                navigation.current_document().map(|lease| lease.stamp()),
            );
            navigation
                .expire_uncommitted(Instant::now())
                .then_some(observed)
        };
        if let Some((Some(context), Some(attempt), revoked)) = expired {
            self.observe(
                context,
                OriginalNavigationFact::Failed {
                    attempt: crate::renderer_contract::NativeAttemptObservation(attempt),
                    revoked,
                },
            )?;
        }
        if let Some(bridge) = &self.bridge {
            bridge.drive_on_ui()?;
        }
        Ok(())
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        let navigation = self
            .navigation
            .try_borrow()
            .ok()
            .and_then(|navigation| navigation.next_deadline());
        navigation
            .into_iter()
            .chain(
                self.bridge
                    .as_ref()
                    .and_then(|bridge| bridge.next_deadline()),
            )
            .min()
    }

    fn observe(
        &self,
        context: NativeNavigationContext,
        fact: OriginalNavigationFact,
    ) -> Result<(), WvError> {
        self.events
            .send(AppWindowEvent::OriginalNavigation { context, fact })
            .map_err(|_| {
                self.session
                    .latch_failure("original navigation fact receiver ended", self.id);
                failure("original navigation fact receiver ended")
            })
    }
}
