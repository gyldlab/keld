//! Operations on the actual `NativeBuild` candidate. No helper takes a native
//! resource out before another established owner can receive it immediately.
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use super::super::controller_custody::AdoptedControllerClose;
use super::super::{
    ICoreWebView2, ICoreWebView2Controller, ICoreWebView2Environment, ProfileReleaseWait, View,
};
use super::resources::{
    ControllerCandidate, WindowCandidate, insert_view_candidate, retire_candidate,
};
use super::{BuildCompletion, BuildResources, CompletedBuild, NativeBuild};
use crate::{WebviewId, WvError};

pub(super) fn candidate_error(detail: &'static str) -> WvError {
    super::super::profile_failure_with(crate::profile::ProfileErrorKind::LifecycleUnproven, detail)
}

impl NativeBuild {
    pub(in crate::webview2) fn new(
        environment: ICoreWebView2Environment,
        next_id: u32,
        saved_permissions_reconciled: bool,
    ) -> Self {
        Self {
            environment,
            next_id,
            saved_permissions_reconciled,
            candidate: None,
            completed: None,
        }
    }

    pub(in crate::webview2) fn candidate_is_empty(&self) -> bool {
        self.candidate.is_none() && self.completed.is_none()
    }

    pub(in crate::webview2) fn candidate_window(&self) -> Option<&tao::window::Window> {
        self.candidate.as_ref().map(BuildResources::window)
    }

    pub(in crate::webview2) fn retire_candidate_on_ui(&mut self) -> bool {
        if !retire_candidate(&mut self.candidate, BuildResources::retire_on_ui) {
            return false;
        }
        self.completed = None;
        true
    }

    pub(super) fn window(&self) -> Result<&tao::window::Window, WvError> {
        self.candidate_window().ok_or_else(|| {
            candidate_error("the native construction candidate has no retained parent")
        })
    }

    pub(super) fn store_window(&mut self, window: tao::window::Window) {
        // The build admission checked empty before WindowBuilder entry. Store
        // immediately, before native identity, geometry or controller work.
        self.candidate = Some(BuildResources::Window(WindowCandidate {
            window: Rc::new(window),
        }));
    }

    pub(super) fn promote_controller(
        &mut self,
        controller: ICoreWebView2Controller,
        release: Option<Weak<ProfileReleaseWait>>,
    ) -> Result<(), WvError> {
        let Some(BuildResources::Window(parent)) = self.candidate.as_ref() else {
            // The existing one-shot owner closes a successful controller even
            // if an inconsistent stage refuses the handoff.
            AdoptedControllerClose::new(controller, release).close_on_ui();
            return Err(candidate_error(
                "controller adoption has no retained Window stage",
            ));
        };
        let prepared = BuildResources::Controller(ControllerCandidate {
            window: Rc::clone(&parent.window),
            close: Rc::new(AdoptedControllerClose::new(controller, release)),
        });
        let previous = super::resources::replace_candidate(&mut self.candidate, prepared);
        drop(previous);
        Ok(())
    }

    pub(super) fn controller(&self) -> Result<&ICoreWebView2Controller, WvError> {
        match self.candidate.as_ref() {
            Some(BuildResources::Controller(candidate)) => Ok(candidate.close.controller()),
            Some(BuildResources::View(view)) => Ok(&view.controller),
            _ => Err(candidate_error(
                "the construction candidate has no adopted controller",
            )),
        }
    }

    pub(super) fn promote_view(&mut self, webview: ICoreWebView2) -> Result<(), WvError> {
        let Some(BuildResources::Controller(controller)) = self.candidate.as_ref() else {
            return Err(candidate_error(
                "View promotion has no retained Controller stage",
            ));
        };
        // Any clone/preparation still has the original Controller candidate
        // behind it. Once View exists, the next operation stores it in place.
        let view = View {
            window: Rc::clone(&controller.window),
            controller: controller.close.controller().clone(),
            close_custody: Rc::clone(&controller.close),
            webview,
            hidden_for_minimize: std::cell::Cell::new(false),
            #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
            test_view_id: None,
        };
        let previous =
            super::resources::replace_candidate(&mut self.candidate, BuildResources::View(view));
        drop(previous);
        Ok(())
    }

    pub(super) fn view(&self) -> Result<&View, WvError> {
        match self.candidate.as_ref() {
            Some(BuildResources::View(view)) => Ok(view),
            _ => Err(candidate_error(
                "the construction candidate is not a retained View",
            )),
        }
    }

    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn view_mut(&mut self) -> Result<&mut View, WvError> {
        match self.candidate.as_mut() {
            Some(BuildResources::View(view)) => Ok(view),
            _ => Err(candidate_error(
                "the construction candidate is not a retained View",
            )),
        }
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "Consume the original completion value on the one candidate-transfer attempt"
    )]
    pub(in crate::webview2) fn transfer_completed_into(
        &mut self,
        views: &mut BTreeMap<u32, View>,
        completed: CompletedBuild,
    ) -> Result<WebviewId, WvError> {
        if self.completed
            != Some(BuildCompletion::Ready {
                id: completed.id,
                snapshot: completed.snapshot,
            })
        {
            return Err(candidate_error(
                "the completion does not match this original candidate",
            ));
        }
        let view = self.view()?;
        if views.contains_key(&completed.id.0)
            || views
                .values()
                .any(|existing| existing.window.id() == view.window.id())
        {
            return Err(WvError::Window(String::from(
                "native window identity collision",
            )));
        }
        // No resource is taken until both identities and metadata are checked.
        // The generic helper's only successful post-take step is map insertion.
        if !insert_view_candidate(&mut self.candidate, views, completed.id.0) {
            return Err(candidate_error(
                "the checked native candidate could not enter its empty map entry",
            ));
        }
        self.completed = None;
        Ok(completed.id)
    }
}

impl Drop for NativeBuild {
    fn drop(&mut self) {
        // The owning host gates normal release on a true retirement. The
        // destructor keeps the existing Close-before-HWND order on fallback.
        let _ = self.retire_candidate_on_ui();
    }
}
