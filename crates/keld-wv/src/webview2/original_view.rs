//! The original document owners retained by the complete guarded View.
#![forbid(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use tao::event_loop::EventLoopProxy;

use super::controller_custody::AdoptedControllerClose;
use super::guarded_navigation::GuardInstalled;
use super::original_bridge::{InstallStart, InstallStatus, OriginalBridge, PreloadInstaller};
use super::original_callbacks::{NavigationCallbacks, Observation};
use super::original_load::NativeLoadCall;
use super::original_navigation::OriginalNavigation;
use super::original_resource::{ResourceSession, ResourceView};
use super::{ICoreWebView2Environment, View, WindowsLoopEvent};
use crate::preload_image::AppPreloadImage;
use crate::renderer_bridge::{RendererBridgeEndpoint, RendererBridgeOutcome, RendererRoleWitness};
use crate::resource_intake::RetainedByteCredit;
use crate::resource_source::AdmittedWebAppSource;
use crate::{OriginalNavigationFact, WebviewId, WvError};

mod lifecycle;
mod load;

pub(super) struct OriginalView {
    id: WebviewId,
    navigation: Rc<RefCell<OriginalNavigation>>,
    session: Rc<ResourceSession>,
    resource: Option<Rc<ResourceView>>,
    callbacks: Option<NavigationCallbacks>,
    installer: Option<PreloadInstaller>,
    call: Option<NativeLoadCall>,
    observer: Option<Observation>,
    inflight_load: Rc<Cell<Option<load::InFlightLoad>>>,
    bridge: RefCell<Option<Rc<OriginalBridge>>>,
    endpoint: Option<RendererBridgeEndpoint>,
    image: Arc<AppPreloadImage>,
    creator: RendererRoleWitness,
    retired: Cell<bool>,
    _metadata: RetainedByteCredit,
}

pub(super) struct ViewInputs {
    pub(super) source: Arc<AdmittedWebAppSource>,
    pub(super) image: Arc<AppPreloadImage>,
    pub(super) creator: RendererRoleWitness,
    pub(super) session: Rc<ResourceSession>,
    pub(super) renderer_lifetime: Option<crate::native_document::RendererLifetime>,
    pub(super) endpoint: RendererBridgeEndpoint,
}

const METADATA_BYTES: usize = 8192;
const _: () = assert!(
    std::mem::size_of::<OriginalView>()
        + std::mem::size_of::<OriginalNavigation>()
        + std::mem::size_of::<NavigationCallbacks>()
        + std::mem::size_of::<PreloadInstaller>()
        <= METADATA_BYTES
);

fn failure(detail: &'static str) -> WvError {
    WvError::EventLoop(detail.into())
}

impl OriginalView {
    /// The containing `NativeBuild` candidate stores this before any identity
    /// observation, subscription, or script registration on the actual View.
    pub(super) fn prepare(inputs: ViewInputs, id: WebviewId) -> Result<Self, WvError> {
        if !inputs.creator.is_live() || !inputs.creator.has_native_view_scope() {
            return Err(failure(
                "selected window lacks its live creator and native scope",
            ));
        }
        let metadata =
            RetainedByteCredit::reserve(&inputs.source.retained_credit_account(), METADATA_BYTES)
                .map_err(|_| failure("selected view metadata budget exhausted"))?;
        let navigation =
            OriginalNavigation::for_renderer(id, inputs.source, inputs.renderer_lifetime)
                .map_err(|_| failure("selected view source binding refused"))?;
        Ok(Self {
            id,
            navigation: Rc::new(RefCell::new(navigation)),
            session: inputs.session,
            resource: None,
            callbacks: None,
            installer: None,
            call: None,
            observer: None,
            inflight_load: Rc::new(Cell::new(None)),
            bridge: RefCell::new(None),
            endpoint: Some(inputs.endpoint),
            image: inputs.image,
            creator: inputs.creator,
            retired: Cell::new(false),
            _metadata: metadata,
        })
    }

    /// Every fallible native step sees its actual retained owner already in
    /// the View. The guard supplies the exact protected native interface.
    pub(super) fn install_on_ui(
        &mut self,
        guard: &GuardInstalled<'_>,
        environment: &ICoreWebView2Environment,
        close: Rc<AdoptedControllerClose>,
        observer: Observation,
        proxy: &EventLoopProxy<WindowsLoopEvent>,
        deadline: Instant,
    ) -> Result<(), WvError> {
        if self.resource.is_some() || self.retired.get() || !self.creator.is_live() {
            return Err(failure("selected native owners cannot be installed again"));
        }
        let core = guard.view();
        let resource = self.resource.insert(ResourceView::bind(
            core,
            environment,
            Rc::clone(&self.navigation),
            Rc::clone(&self.session),
            self.id,
            close,
        )?);
        let callbacks = self.callbacks.insert(NavigationCallbacks::prepare(
            core.clone(),
            Rc::clone(&self.navigation),
            Rc::clone(&self.session),
            self.id,
        ));
        let installer = self.installer.insert(PreloadInstaller::prepare(
            core.clone(),
            self.creator.clone(),
            &self.image,
            deadline,
            proxy.clone(),
            self.id,
        )?);

        resource.install_on_ui()?;
        let inflight_load = Rc::clone(&self.inflight_load);
        self.observer = Some(Rc::clone(&observer));
        callbacks
            .install_on_ui(Rc::new(move |context, mut fact| {
                if let Some(mut inflight) = inflight_load.get() {
                    inflight.observe_failure(context, &mut fact);
                    inflight_load.set(Some(inflight));
                }
                observer(context, fact);
            }))
            .map_err(|_| failure("selected navigation subscriptions refused"))?;
        let endpoint = self
            .endpoint
            .take()
            .ok_or_else(|| failure("selected view lacks its actual endpoint"))?;
        let bridge = OriginalBridge::prepare(Rc::clone(resource), endpoint, proxy.clone())?;
        // The complete View owns this exact bridge before either native Add.
        self.bridge.get_mut().insert(bridge).install_on_ui()?;
        match installer.begin_on_ui() {
            InstallStart::Returned(Ok(())) => Ok(()),
            InstallStart::OutcomeUnknown => {
                self.session
                    .latch_failure("selected preload native return is unproved", self.id);
                Err(failure("selected preload native return is unproved"))
            }
            InstallStart::NotAdmitted
            | InstallStart::AlreadyStarted
            | InstallStart::Returned(Err(_)) => {
                Err(failure("selected preload registration refused"))
            }
        }
    }

    pub(super) const fn id(&self) -> WebviewId {
        self.id
    }

    fn renderer_bridge(&self) -> Result<Option<Rc<OriginalBridge>>, WvError> {
        self.bridge
            .try_borrow()
            .map(|slot| slot.as_ref().map(Rc::clone))
            .map_err(|_| failure("selected renderer bridge is borrowed"))
    }

    pub(super) fn drive_renderer_on_ui(&self) -> Result<(), WvError> {
        self.renderer_bridge()?
            .map_or(Ok(()), |bridge| bridge.drive_on_ui())
    }

    pub(super) fn renderer_deadline(&self) -> Option<Instant> {
        self.renderer_bridge().ok().flatten()?.next_deadline()
    }

    pub(super) fn observe_renderer_capacity(
        &self,
        outcome: &RendererBridgeOutcome,
    ) -> Result<bool, WvError> {
        Ok(self
            .renderer_bridge()?
            .is_some_and(|bridge| bridge.observe_capacity(outcome)))
    }

    /// Acknowledging script installation creates no renderer readiness or
    /// permission to issue the first selected navigation.
    pub(super) fn preload_status(&self, now: Instant) -> InstallStatus {
        if !self.creator.is_live() {
            return InstallStatus::Refused;
        }
        self.installed_view_status(now)
    }

    fn installed_view_status(&self, now: Instant) -> InstallStatus {
        if self.retired.get()
            || self
                .renderer_bridge()
                .ok()
                .flatten()
                .is_none_or(|bridge| !bridge.is_installed())
        {
            return InstallStatus::Refused;
        }
        self.installer
            .as_ref()
            .map_or(InstallStatus::Refused, |installer| installer.status(now))
    }
}
