//! One Host-wide verified resource session; per-view native policy bindings.
//! The shared `TaskLedger` owns task incarnation/accounting, `OriginalNavigation`
//! owns document authority, and W2 alone owns actual controller Close.
#![deny(unsafe_code)]

mod admission;
mod delivery;
mod filters;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod fixture_policy;
mod lifecycle;
mod native;
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
mod observation;
mod reader;

pub(super) use native::require_get_without_body;

use super::WindowsLoopEvent;
use super::controller_custody::AdoptedControllerClose;
use super::guarded_navigation::canonical_webview_identity;
use super::original_navigation::OriginalNavigation;
use crate::native_document::ResourceLease;
use crate::resource_intake::{ORIGINAL_WEBAPP_LIMITS, RetainedByteCredit, TaskLedger, TaskTicket};
use crate::resource_source::{AdmittedWebAppSource as Source, SelectedRead};
use crate::{WebviewId, WvError};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};
use std::sync::{Arc, atomic::AtomicBool};
use tao::event_loop::EventLoopProxy;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2Deferral, ICoreWebView2Environment,
    ICoreWebView2WebResourceRequestedEventArgs, ICoreWebView2WebResourceResponse,
};
use windows::core::{IUnknown, Interface};

struct NativePending {
    args: ICoreWebView2WebResourceRequestedEventArgs,
    deferral: Option<ICoreWebView2Deferral>,
    environment: ICoreWebView2Environment,
    response: Option<ICoreWebView2WebResourceResponse>,
}

struct Pending {
    ticket: TaskTicket,
    view: Weak<ResourceView>,
    view_id: WebviewId,
    lease: ResourceLease,
    source: Arc<Source>,
    selected: SelectedRead,
    document: bool,
    claimed_document: bool,
    native: Option<NativePending>,
    cancelled: Arc<AtomicBool>,
    _metadata_credit: RetainedByteCredit,
    body_credit: Option<RetainedByteCredit>,
    completed: Option<reader::Completed>,
}
const _: () = assert!(std::mem::size_of::<Pending>() <= 1024);

struct State {
    ledger: TaskLedger,
    pending: BTreeMap<usize, Pending>,
    admitting: BTreeMap<usize, WebviewId>,
    views: BTreeMap<u32, Weak<ResourceView>>,
    worker_filter_owner: BTreeMap<usize, WebviewId>,
    filter_transition: BTreeSet<usize>,
    closing_filter_owner: BTreeMap<usize, WebviewId>,
}

pub(super) struct ResourceSession {
    custody: Arc<Source>,
    proxy: EventLoopProxy<WindowsLoopEvent>,
    state: RefCell<State>,
    reader: RefCell<reader::ReaderCustody>,
    retiring: Cell<bool>,
    fault: Cell<Option<&'static str>>,
    _metadata_credit: RetainedByteCredit,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    test_resource_observation: bool,
}

pub(super) struct ResourceView {
    id: WebviewId,
    core: ICoreWebView2,
    core_identity: usize,
    environment: ICoreWebView2Environment,
    environment_identity: usize,
    navigation: Rc<RefCell<OriginalNavigation>>,
    source: Arc<Source>,
    session: Rc<ResourceSession>,
    close: Rc<AdoptedControllerClose>,
    retired: Cell<bool>,
    controller_closed: Cell<bool>,
    handler_token: Cell<Option<i64>>,
    install_attempted: Cell<bool>,
    install_in_progress: Cell<bool>,
    emergency: RefCell<Option<NativePending>>,
    emergency_acquiring: Cell<bool>,
    _metadata_credit: RetainedByteCredit,
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    fixture_inline: fixture_policy::InlineFixture,
}
#[cfg(all(feature = "profile-test-hooks", debug_assertions))]
const _: () =
    assert!(std::mem::size_of::<ResourceView>() + 2 * std::mem::size_of::<usize>() <= 2048);

fn failure(detail: &'static str) -> WvError {
    WvError::EventLoop(detail.to_owned())
}

impl ResourceSession {
    pub(super) fn new(
        source: Arc<Source>,
        proxy: EventLoopProxy<WindowsLoopEvent>,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))] test_hold_directory: Option<
            std::path::PathBuf,
        >,
    ) -> Result<Rc<Self>, WvError> {
        let credit = RetainedByteCredit::reserve(&source.retained_credit_account(), 4096)
            .map_err(|_| failure("resource session metadata budget exhausted"))?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let test_resource_observation = test_hold_directory.is_some();
        let reader = RefCell::new(reader::ReaderCustody::new());
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        reader
            .borrow_mut()
            .configure_test_hold(test_hold_directory)?;
        Ok(Rc::new(Self {
            custody: source,
            proxy,
            state: RefCell::new(State {
                ledger: TaskLedger::new(ORIGINAL_WEBAPP_LIMITS),
                pending: BTreeMap::new(),
                admitting: BTreeMap::new(),
                views: BTreeMap::new(),
                worker_filter_owner: BTreeMap::new(),
                filter_transition: BTreeSet::new(),
                closing_filter_owner: BTreeMap::new(),
            }),
            reader,
            retiring: Cell::new(false),
            fault: Cell::new(None),
            _metadata_credit: credit,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            test_resource_observation,
        }))
    }

    // Fault and notification are Host-wide; callers retain exact view custody.
    pub(super) fn latch_failure(&self, detail: &'static str, _view: WebviewId) {
        if self.fault.get().is_some() {
            return;
        }
        self.fault.set(Some(detail));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = self.proxy.send_event(WindowsLoopEvent::OriginalWake);
        }));
    }

    // Bounded cursor iteration keeps retirement allocation-free, even with a
    // full body account and reentrant callbacks. A stale Weak is not a receipt.
    fn next_view_after(
        &self,
        after: Option<WebviewId>,
    ) -> Option<(WebviewId, Option<Rc<ResourceView>>)> {
        use std::ops::Bound::{Excluded, Unbounded};
        self.state
            .borrow()
            .views
            .range((after.map_or(Unbounded, |id| Excluded(id.0)), Unbounded))
            .next()
            .map(|(id, view)| (WebviewId(*id), view.upgrade()))
    }

    /// Root drives this singleton on every loop turn, including unscoped wakes.
    pub(super) fn drain_ui(&self) -> Result<(), WvError> {
        let completed = self.reader.borrow_mut().take_result()?;
        if let Some(completed) = completed {
            self.deliver(completed);
        }
        reader::with_reader_on_ui(&self.reader, reader::Reader::poll_termination)?;
        if let Some(detail) = self.fault.get() {
            return Err(failure(detail));
        }
        self.dispatch_waiting();
        if let Some(detail) = self.fault.get() {
            return Err(failure(detail));
        }
        Ok(())
    }

    pub(super) fn is_drained(&self) -> bool {
        self.retiring.get()
            && self.state.borrow().views.is_empty()
            && self.state.borrow().filter_transition.is_empty()
            && self.state.borrow().closing_filter_owner.is_empty()
            && self.state.borrow().pending.is_empty()
            && self.state.borrow().admitting.is_empty()
            && self.state.borrow().ledger.is_empty()
            && !self.state.borrow().ledger.has_active_delivery()
            && self.reader.borrow().is_drained()
    }
}

impl ResourceView {
    /// Bind only. The caller attaches this exact `Rc` to its complete View before
    /// invoking `install_on_ui` or any other fallible native installation. That
    /// owner must be parked on error until actual Close and resource drain.
    pub(super) fn bind(
        core: &ICoreWebView2,
        environment: &ICoreWebView2Environment,
        navigation: Rc<RefCell<OriginalNavigation>>,
        session: Rc<ResourceSession>,
        id: WebviewId,
        close: Rc<AdoptedControllerClose>,
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        image: &crate::preload_image::AppPreloadImage,
    ) -> Result<Rc<Self>, WvError> {
        if session.retiring.get() || session.fault.get().is_some() {
            return Err(failure("resource session is unavailable"));
        }
        let source = navigation
            .try_borrow()
            .map_err(|_| failure("source binding is busy"))?
            .source()
            .clone();
        if !session.custody.shares_windows_resource_custody(&source) {
            return Err(failure(
                "window source does not share the Host's actual resource custody",
            ));
        }
        let credit = RetainedByteCredit::reserve(&source.retained_credit_account(), 2048)
            .map_err(|_| failure("resource window metadata budget exhausted"))?;
        let core_identity = canonical_webview_identity(core)
            .map_err(|_| failure("cannot bind exact native resource view"))?;
        let environment_identity = environment
            .cast::<IUnknown>()
            .map_err(|_| failure("cannot bind exact resource environment"))?
            .as_raw() as usize;
        if session.state.borrow().views.contains_key(&id.0) {
            return Err(failure("resource view lifetime is already bound"));
        }
        let view = Rc::new(Self {
            id,
            core: core.clone(),
            core_identity,
            environment: environment.clone(),
            environment_identity,
            navigation,
            source,
            session,
            close,
            retired: Cell::new(false),
            controller_closed: Cell::new(false),
            handler_token: Cell::new(None),
            install_attempted: Cell::new(false),
            install_in_progress: Cell::new(false),
            emergency: RefCell::new(None),
            emergency_acquiring: Cell::new(false),
            _metadata_credit: credit,
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            fixture_inline: fixture_policy::InlineFixture::capture(image),
        });
        view.session
            .state
            .borrow_mut()
            .views
            .insert(id.0, Rc::downgrade(&view));
        Ok(view)
    }

    /// The complete View already retains this exact `Rc`. Failure cannot discard
    /// its guard, emergency deferral, Close owner, or shared resource credits.
    pub(super) fn install_on_ui(self: &Rc<Self>) -> Result<(), WvError> {
        if self.install_attempted.replace(true) {
            return Err(failure(
                "native resource installation was already attempted",
            ));
        }
        if self.retired.get()
            || self.controller_closed.get()
            || self.session.retiring.get()
            || self.session.fault.get().is_some()
        {
            return Err(failure("native resource owner is already retiring"));
        }
        self.install_in_progress.set(true);
        let installed = self.install_callback_and_filters();
        if installed.is_err() {
            self.emergency_close("resource guard installation failed");
        }
        self.install_in_progress.set(false);
        self.release_closed_registration();
        installed
    }

    pub(super) fn is_drained(&self) -> bool {
        self.retired.get()
            && self.controller_closed.get()
            && !self.emergency_acquiring.get()
            && !self.install_in_progress.get()
            && self.emergency.borrow().is_none()
            && !self
                .session
                .state
                .borrow()
                .pending
                .values()
                .any(|pending| pending.view_id == self.id)
            && !self
                .session
                .state
                .borrow()
                .admitting
                .values()
                .any(|id| *id == self.id)
            && !self.session.state.borrow().ledger.has_active_delivery()
            && !self.session.state.borrow().views.contains_key(&self.id.0)
            && self
                .session
                .state
                .borrow()
                .worker_filter_owner
                .get(&self.environment_identity)
                != Some(&self.id)
            && self
                .session
                .state
                .borrow()
                .closing_filter_owner
                .get(&self.environment_identity)
                != Some(&self.id)
            && !self
                .session
                .state
                .borrow()
                .filter_transition
                .contains(&self.environment_identity)
    }

    fn is_live_resource(&self, lease: ResourceLease) -> bool {
        !self.retired.get()
            && !self.controller_closed.get()
            && !self.session.retiring.get()
            && self.session.fault.get().is_none()
            && self
                .navigation
                .try_borrow()
                .is_ok_and(|nav| nav.is_current_resource(lease))
    }
}
