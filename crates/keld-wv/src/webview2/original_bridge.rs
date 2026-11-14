//! One retained Windows original bridge on its actual source/view/document.
//! Shared call/event/binding owners keep protocol custody; native operations
//! run only after every interior guard has ended.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

mod callbacks;
mod capture;
mod control;
mod current;
mod delivery;
mod drive;
mod event;
mod event_owner;
mod installer;
mod intake;
mod invocation;
mod numeric;
mod post;
mod startup;
mod state;

pub use event::NativeEventRetirement;
pub(super) use installer::{InstallStart, InstallStatus, PreloadInstaller};

use super::WindowsLoopEvent;
use super::controller_custody::AdoptedControllerClose;
use super::guarded_navigation::canonical_webview_identity;
use super::original_callbacks::registration::Registration;
use super::original_navigation::OriginalNavigation;
use super::original_resource::ResourceSession;
use crate::renderer_bridge::RendererBridgeEndpoint;
use crate::resource_intake::RetainedByteCredit;
use crate::resource_source::AdmittedWebAppSource;
use crate::{WebviewId, WvError};
use current::CurrentSlot;
use event_owner::EventOwner;
use state::State;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use tao::event_loop::EventLoopProxy;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;

pub(super) struct OriginalBridge {
    view: ICoreWebView2,
    identity: usize,
    id: WebviewId,
    source: Arc<AdmittedWebAppSource>,
    navigation: Rc<RefCell<OriginalNavigation>>,
    session: Rc<ResourceSession>,
    close: Rc<AdoptedControllerClose>,
    endpoint: RendererBridgeEndpoint,
    proxy: EventLoopProxy<WindowsLoopEvent>,
    state: RefCell<State>,
    current: CurrentSlot,
    event: EventOwner,
    message_registration: Registration,
    dom_registration: Registration,
    install_attempted: Cell<bool>,
    installing: Cell<bool>,
    callback_depth: Cell<Option<usize>>,
    capturing: Cell<bool>,
    driving: Cell<bool>,
    retired: Cell<bool>,
    retirement_wake_attempted: Cell<bool>,
    close_wake_attempted: Cell<bool>,
    _metadata: RetainedByteCredit,
}

impl OriginalBridge {
    /// The complete containing View attaches this Rc before `install_on_ui`.
    /// The endpoint receiver remains with the SAME Host outcome worker.
    pub(super) fn prepare(
        view: &ICoreWebView2,
        navigation: Rc<RefCell<OriginalNavigation>>,
        session: Rc<ResourceSession>,
        endpoint: RendererBridgeEndpoint,
        proxy: EventLoopProxy<WindowsLoopEvent>,
        id: WebviewId,
        close: Rc<AdoptedControllerClose>,
    ) -> Result<Rc<Self>, WvError> {
        if endpoint.outcomes.is_some() {
            return Err(failure(
                "original view cannot own the Host outcome receiver",
            ));
        }
        let source = {
            let nav = navigation
                .try_borrow()
                .map_err(|_| failure("bridge source is borrowed"))?;
            if nav.document().view() != id {
                return Err(failure("bridge view differs from its native owner"));
            }
            Arc::clone(nav.source())
        };
        // Same source-wide account; covers this retained owner, two installed
        // callback closures/positions and fixed diagnostics. Call bodies use
        // their existing separate LocalCallBudget/CONTROL/DATA charges.
        let metadata = RetainedByteCredit::reserve(&source.retained_credit_account(), 8192)
            .map_err(|_| failure("bridge metadata budget exhausted"))?;
        let identity = canonical_webview_identity(view)
            .map_err(|_| failure("bridge native view identity is unavailable"))?;
        let state = State::new(proxy.clone(), id)?;
        Ok(Rc::new(Self {
            view: view.clone(),
            identity,
            id,
            source,
            navigation,
            session,
            close,
            endpoint,
            proxy,
            state: RefCell::new(state),
            current: CurrentSlot::new(),
            event: EventOwner::new(),
            message_registration: Registration::new(),
            dom_registration: Registration::new(),
            install_attempted: Cell::new(false),
            installing: Cell::new(false),
            callback_depth: Cell::new(Some(0)),
            capturing: Cell::new(false),
            driving: Cell::new(false),
            retired: Cell::new(false),
            retirement_wake_attempted: Cell::new(false),
            close_wake_attempted: Cell::new(false),
            _metadata: metadata,
        }))
    }

    fn fail(&self, detail: &'static str) {
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.fault.latch();
        }
        self.session.latch_failure(detail, self.id);
    }
    fn custody_failure(&self, detail: &'static str) -> WvError {
        self.fail(detail);
        failure(detail)
    }
    fn wake(&self) {
        if !matches!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self
                .proxy
                .send_event(WindowsLoopEvent::OriginalWake))),
            Ok(Ok(()))
        ) {
            self.session
                .latch_failure("original bridge wake failed", self.id);
        }
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

    /// Root closes the SAME physical producer scope before invoking retirement.
    /// Never closes the controller itself; W2 owns that actual receipt.
    pub(super) fn retire_on_ui(&self) -> Result<(), WvError> {
        self.retired.set(true);
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| failure("bridge retirement met retained state borrow"))?;
        state.calls.cancel_queued()?;
        state.binding.discard_revoked_registration();
        drop(state);
        // Repeated owner service still performs cleanup without self-notifying again.
        if !self.retirement_wake_attempted.replace(true) {
            self.wake();
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            super::managed_test_control::queued_close_observation::retirement_wake_attempt(
                self.id.0,
            );
        }
        Ok(())
    }

    /// Only the SAME actual Close owner can release installed positions.
    /// An in-call post/getter or callback keeps custody even after reentrant Close.
    pub(super) fn controller_closed_on_ui(&self) {
        if !self.close.actual_closed() {
            return;
        }
        self.retired.set(true);
        self.clear_closed_registrations();
        // Callback return and actual storage progress retain their independent wakes.
        if !self.close_wake_attempted.replace(true) {
            self.wake();
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            super::managed_test_control::queued_close_observation::controller_close_wake_attempt(
                self.id.0,
            );
        }
    }

    fn clear_closed_registrations(&self) {
        let quiescent =
            self.callback_depth.get() == Some(0) && !self.capturing.get() && !self.installing.get();
        self.message_registration
            .clear_after_close(self.close.actual_closed(), quiescent);
        self.dom_registration
            .clear_after_close(self.close.actual_closed(), quiescent);
    }

    pub(super) fn is_drained(&self) -> bool {
        self.retired.get()
            && self.close.actual_closed()
            && !self.installing.get()
            && !self.capturing.get()
            && !self.driving.get()
            && self.callback_depth.get() == Some(0)
            && self.message_registration.is_cleared()
            && self.dom_registration.is_cleared()
            && self.current.is_empty()
            && self.event.is_empty()
            && self.state.try_borrow().is_ok_and(|s| {
                s.calls.is_drained() && s.pending_send.is_none() && !s.registration_ack_pending
            })
    }
}

const _: () = assert!(std::mem::size_of::<OriginalBridge>() + 2048 <= 8192);

fn failure(detail: &'static str) -> WvError {
    WvError::RendererBridge {
        detail: detail.into(),
    }
}
