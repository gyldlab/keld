//! One selected Windows bridge retained on its actual `ResourceView`.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod callbacks;
mod capture;
mod control;
mod current;
mod event;
mod installer;
mod post;
mod startup;
mod state;

pub use event::NativeEventRetirement;
pub(super) use installer::{InstallStart, InstallStatus, PreloadInstaller};

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use tao::event_loop::EventLoopProxy;

use super::WindowsLoopEvent;
use super::guarded_navigation::canonical_webview_identity;
use super::original_callbacks::registration::Registration;
use super::original_navigation::OriginalNavigation;
use super::original_resource::{ResourceSession, ResourceView};
use crate::original_call::LocalCallBudget;
use crate::renderer_bridge::{RendererBridgeEndpoint, RendererBridgeOutcome};
use crate::resource_intake::RetainedByteCredit;
use crate::resource_source::AdmittedWebAppSource;
use crate::{WebviewId, WvError};
use control::StartupControl;
use current::NativePosition;
use state::State;

pub(super) struct OriginalBridge {
    resource: Rc<ResourceView>,
    id: WebviewId,
    source: Arc<AdmittedWebAppSource>,
    navigation: Rc<RefCell<OriginalNavigation>>,
    session: Rc<ResourceSession>,
    endpoint: RendererBridgeEndpoint,
    proxy: EventLoopProxy<WindowsLoopEvent>,
    state: RefCell<State>,
    budget: RefCell<LocalCallBudget>,
    current: NativePosition<StartupControl>,
    message_registration: Registration,
    dom_registration: Registration,
    install_attempted: Cell<bool>,
    installing: Cell<bool>,
    callback_depth: Cell<Option<usize>>,
    capturing: Cell<bool>,
    driving: Cell<bool>,
    retired: Cell<bool>,
    faulted: Cell<bool>,
    _metadata: RetainedByteCredit,
}

impl OriginalBridge {
    /// The caller attaches this Rc to the existing `OriginalView` before any
    /// subscription. The sole outcome receiver remains with its Host owner.
    pub(super) fn prepare(
        resource: Rc<ResourceView>,
        endpoint: RendererBridgeEndpoint,
        proxy: EventLoopProxy<WindowsLoopEvent>,
    ) -> Result<Rc<Self>, WvError> {
        if endpoint.outcomes.is_some() {
            return Err(failure(
                "selected bridge cannot own the Host outcome receiver",
            ));
        }
        let (_, _, navigation) = resource.original_post_target();
        let (id, source) = {
            let nav = navigation
                .try_borrow()
                .map_err(|_| failure("bridge document owner is borrowed"))?;
            if !resource.holds_live_load_owner(&nav) {
                return Err(failure("bridge requires its live installed resource owner"));
            }
            (nav.document().view(), Arc::clone(nav.source()))
        };
        let navigation = Rc::clone(navigation);
        let session = Rc::clone(resource.original_resource_session());
        let metadata = RetainedByteCredit::reserve(&source.retained_credit_account(), 8192)
            .map_err(|_| failure("selected bridge metadata budget exhausted"))?;
        Ok(Rc::new(Self {
            resource,
            id,
            source,
            navigation,
            session,
            endpoint,
            proxy,
            state: RefCell::new(State::default()),
            budget: RefCell::new(LocalCallBudget::default()),
            current: NativePosition::new(),
            message_registration: Registration::new(),
            dom_registration: Registration::new(),
            install_attempted: Cell::new(false),
            installing: Cell::new(false),
            callback_depth: Cell::new(Some(0)),
            capturing: Cell::new(false),
            driving: Cell::new(false),
            retired: Cell::new(false),
            faulted: Cell::new(false),
            _metadata: metadata,
        }))
    }

    fn fail(&self, detail: &'static str) {
        self.faulted.set(true);
        self.session.latch_failure(detail, self.id);
    }

    fn wake(&self) {
        if !matches!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self
                .proxy
                .send_event(WindowsLoopEvent::OriginalWake))),
            Ok(Ok(()))
        ) {
            self.fail("selected bridge wake failed");
        }
    }

    pub(super) fn is_installed(&self) -> bool {
        self.message_registration.is_registered()
            && self.dom_registration.is_registered()
            && !self.installing.get()
            && !self.retired.get()
            && !self.faulted.get()
            && !self.resource.actual_post_controller_closed()
    }

    /// The Host outcome owner reports actual inbox or ledger capacity.
    /// The UI wake only requests service of the same retained startup.
    pub(super) fn observe_capacity(&self, outcome: &RendererBridgeOutcome) -> bool {
        if !matches!(outcome, RendererBridgeOutcome::ControlCapacity)
            || self.retired.get()
            || self.faulted.get()
        {
            return false;
        }
        self.wake();
        true
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        if self.retired.get() {
            return None;
        }
        let state = self.state.try_borrow().ok()?;
        let nav = self.navigation.try_borrow().ok()?;
        state
            .startup
            .as_ref()?
            .next_deadline(&state.binding, nav.document())
    }

    pub(super) fn retire_on_ui(&self) -> Result<(), WvError> {
        self.retired.set(true);
        self.state
            .try_borrow_mut()
            .map_err(|_| failure("bridge retirement met retained state borrow"))?
            .binding
            .discard_revoked_registration();
        Ok(())
    }

    pub(super) fn controller_closed_on_ui(&self) {
        if !self.resource.actual_post_controller_closed() {
            return;
        }
        self.retired.set(true);
        self.clear_closed_registrations();
        if !self.driving.get()
            && !self.current.is_empty()
            && let Err(error) = self.current.drive(StartupControl::dispose_if_quiescent)
        {
            self.fail("closed bridge could not settle its retained control");
            eprintln!("{error}");
        }
    }

    fn clear_closed_registrations(&self) {
        let quiescent =
            self.callback_depth.get() == Some(0) && !self.capturing.get() && !self.installing.get();
        let closed = self.resource.actual_post_controller_closed();
        self.message_registration
            .clear_after_close(closed, quiescent);
        self.dom_registration.clear_after_close(closed, quiescent);
    }

    pub(super) fn is_drained(&self) -> bool {
        self.retired.get()
            && self.resource.actual_post_controller_closed()
            && !self.installing.get()
            && !self.capturing.get()
            && !self.driving.get()
            && self.callback_depth.get() == Some(0)
            && self.message_registration.is_cleared()
            && self.dom_registration.is_cleared()
            && self.current.is_empty()
            && self
                .budget
                .try_borrow()
                .is_ok_and(|budget| budget.retained_records() == 0)
            && self
                .state
                .try_borrow()
                .is_ok_and(|state| state.binding.registration_notice().is_none())
    }
}

const _: () = assert!(std::mem::size_of::<OriginalBridge>() + 2048 <= 8192);

fn failure(detail: &'static str) -> WvError {
    WvError::RendererBridge {
        detail: detail.into(),
    }
}
