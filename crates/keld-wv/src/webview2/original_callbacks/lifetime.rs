//! The complete `OriginalView` attaches this owner before any subscription.
#![forbid(unsafe_code)]

use super::super::guarded_navigation::canonical_webview_identity;
use super::super::original_navigation::OriginalNavigation;
use super::super::original_resource::ResourceSession;
use super::boundary::{FailureSink, boundary};
use super::registration::Registration;
use super::{
    NavigationCallbacks, Observation, install_completed, install_loading, install_starting,
};
use crate::WebviewId;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;
use windows::Win32::Foundation::E_UNEXPECTED;

impl NavigationCallbacks {
    /// No subscription, native getter, navigation, or Close occurs here. Root
    /// binds `ResourceView` and attaches this exact owner before `install_on_ui`.
    pub(in crate::webview2) fn prepare(
        view: ICoreWebView2,
        navigation: Rc<RefCell<OriginalNavigation>>,
        session: Rc<ResourceSession>,
        id: WebviewId,
    ) -> Self {
        Self {
            view,
            navigation,
            failure: FailureSink::new(&session, id),
            _session: session,
            starting: Registration::new(),
            loading: Registration::new(),
            completed: Registration::new(),
            install_attempted: Cell::new(false),
            installing: Cell::new(false),
            retired: Cell::new(false),
            controller_closed: Cell::new(false),
        }
    }

    pub(in crate::webview2) fn install_on_ui(
        &self,
        observe: Observation,
    ) -> windows::core::Result<()> {
        if self.install_attempted.replace(true) {
            return Err(E_UNEXPECTED.into());
        }
        self.require_live()?;
        self.installing.set(true);
        let result = boundary(&|detail| self.failure.latch(detail), || {
            let identity = canonical_webview_identity(&self.view)?;
            install_starting(self, identity, Rc::clone(&observe))?;
            install_loading(self, identity, Rc::clone(&observe))?;
            install_completed(self, identity, observe)?;
            self.require_live()
        });
        self.installing.set(false);
        self.clear_closed_tokens();
        result
    }

    pub(super) fn require_live(&self) -> windows::core::Result<()> {
        if self.retired.get() || self.controller_closed.get() {
            Err(E_UNEXPECTED.into())
        } else {
            Ok(())
        }
    }

    /// Keep every navigation/security handler installed until actual Close.
    /// There is no interval in which a live view loses its denial callback.
    pub(in crate::webview2) fn retire_on_ui(&self) -> windows::core::Result<()> {
        self.retired.set(true);
        boundary(&|detail| self.failure.latch(detail), || {
            let Ok(mut navigation) = self.navigation.try_borrow_mut() else {
                self.failure
                    .latch("navigation callback retirement met a held document-owner borrow");
                return Err(E_UNEXPECTED.into());
            };
            navigation.destroy();
            Ok(())
        })
    }

    /// Root observed successful Close of this View's SAME retained controller.
    /// That is not permission to release an active add call or callback stack.
    pub(in crate::webview2) fn controller_closed_on_ui(&self) {
        self.controller_closed.set(true);
        let _ = self.retire_on_ui(); // A refusal already latched the same permanent session fault.
        self.clear_closed_tokens();
    }

    fn calls_quiescent(&self) -> bool {
        !self.installing.get()
            && !self.starting.in_call()
            && !self.loading.in_call()
            && !self.completed.in_call()
            && self
                .navigation
                .try_borrow()
                .is_ok_and(|state| state.callbacks_inactive())
    }

    fn clear_closed_tokens(&self) {
        if self.controller_closed.get() && self.calls_quiescent() {
            self.starting.clear_after_close(true, true);
            self.loading.clear_after_close(true, true);
            self.completed.clear_after_close(true, true);
        }
    }

    /// Root additionally requires the retained `ResourceSession`'s successful
    /// retire/drain Result. Its permanent fault is never a release receipt.
    pub(in crate::webview2) fn is_drained(&self) -> bool {
        self.retired.get() && self.controller_closed.get() && self.calls_quiescent()
    }
}
