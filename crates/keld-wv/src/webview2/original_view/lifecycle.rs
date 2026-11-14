//! Logical revocation precedes the same resource guard and actual controller Close.
#![forbid(unsafe_code)]

use super::{
    Cell, NativeLoadCall, NativeNavigationContext, NavigationCallbacks, OriginalNavigation,
    OriginalNavigationFact, OriginalView, PreloadInstaller, Rc, RefCell, WvError, failure,
};
use crate::renderer_contract::{
    CheckedNativeDocumentLease, NativeDocumentStamp, OriginalDocumentDestroyAdmission,
};

impl OriginalView {
    /// Pure conditional cut. Cancellation and this exact document compete on the
    /// existing admission object; no native call is made while the owner is borrowed.
    pub(in crate::webview2) fn admit_destroy(
        &self,
        expected: &CheckedNativeDocumentLease,
        admission: &OriginalDocumentDestroyAdmission,
    ) -> Result<bool, WvError> {
        let observed = {
            let mut navigation = self
                .navigation
                .try_borrow_mut()
                .map_err(|_| failure("original destroy met a held document borrow"))?;
            if self.retired.get()
                || !expected.belongs_to_owner(navigation.document())
                || !expected.belongs_to_source(navigation.source())
                || !admission.try_accept_on_ui()
            {
                let _ = admission.refuse_on_ui();
                return Ok(false);
            }
            let observed = (
                navigation.host_context(),
                navigation.current_document().map(|lease| lease.stamp()),
            );
            navigation.destroy();
            self.retired.set(true);
            observed
        };
        if let (Some(context), previous) = observed {
            self.observe(context, OriginalNavigationFact::Revoked { previous })?;
        }
        Ok(true)
    }

    /// Only this resource retirement result permits the caller to enter the
    /// same controller Close. Failure may preserve the environment's last guard.
    pub(in crate::webview2) fn retire_before_close(&self) -> Result<(), WvError> {
        // Close future Core offers before any document borrow or native entry.
        // The same creation scope remains valid after role/document adoption;
        // existing queued completions continue to use the retained bridge.
        if !self.inputs.role.close_native_offers() {
            self.session
                .latch_failure("original producer retirement refused", self.id);
            return Err(failure("original producer retirement refused"));
        }
        if let Some((Some(context), previous)) = retire_navigation(&self.navigation, &self.retired)?
        {
            self.observe(context, OriginalNavigationFact::Revoked { previous })?;
        }
        if let Some(installer) = &self.installer {
            installer.retire_on_ui();
        }
        if let Some(callbacks) = &self.callbacks
            && callbacks.retire_on_ui().is_err()
        {
            self.session
                .latch_failure("original callback retirement refused", self.id);
        }
        if let Some(bridge) = &self.bridge
            && bridge.retire_on_ui().is_err()
        {
            self.session
                .latch_failure("original bridge retirement refused", self.id);
        }
        if let Some(resource) = &self.resource {
            resource.retire_on_ui()?;
        }
        Ok(())
    }

    /// Caller just observed successful Close from this View's existing custodian.
    pub(in crate::webview2) fn controller_closed_on_ui(&self) {
        if let Some(resource) = &self.resource {
            resource.controller_closed_on_ui();
        }
        if let Some(callbacks) = &self.callbacks {
            callbacks.controller_closed_on_ui();
        }
        if let Some(installer) = &self.installer {
            installer.controller_closed_on_ui();
        }
        if let Some(bridge) = &self.bridge {
            bridge.controller_closed_on_ui();
        }
        // Close notifications repeat while this same View waits for storage drain.
        // This test receipt observes only its first successful Close notification.
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if !self.controller_close_observed.replace(true) {
            let native_quiescent = self.inputs.role.native_storage_is_quiescent();
            super::super::managed_test_control::queued_close_observation::controller_closed(
                self.id.0,
                native_quiescent,
            );
        }
    }

    /// Root also requires successful Host `ResourceSession` retire/drain. The same
    /// Rc count covers the resource callback prefix before a ledger ticket exists.
    pub(in crate::webview2) fn is_drained(&self) -> bool {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if self
            .frame_ingress
            .as_ref()
            .is_some_and(|observer| !observer.is_drained())
        {
            return false;
        }
        let drained = self.retired.get()
            // Same physical Entry, producer closed, all old/current-role native
            // storage physically quiescent. No WindowReleased or queue shortcut.
            && self.inputs.role.native_storage_is_quiescent()
            && self
                .resource
                .as_ref()
                .is_none_or(|resource| Rc::strong_count(resource) == 1 && resource.is_drained())
            && self
                .callbacks
                .as_ref()
                .is_none_or(NavigationCallbacks::is_drained)
            && self
                .installer
                .as_ref()
                .is_none_or(PreloadInstaller::native_quiesced)
            && self
                .bridge
                .as_ref()
                .is_none_or(|bridge| bridge.is_drained())
            && self
                .call
                .as_ref()
                .is_none_or(NativeLoadCall::native_quiesced);
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if self.retired.get() {
            super::super::managed_test_control::queued_close_observation::view_drain(
                self.id.0, drained,
            );
        }
        drained
    }
}

type RetirementObservation = (Option<NativeNavigationContext>, Option<NativeDocumentStamp>);

/// Commit the existing retirement flag only after the SAME actual owner is
/// borrowed and revoked. A busy borrow leaves both document and retry intact.
fn retire_navigation(
    navigation: &RefCell<OriginalNavigation>,
    retired: &Cell<bool>,
) -> Result<Option<RetirementObservation>, WvError> {
    if retired.get() {
        return Ok(None);
    }
    let mut navigation = navigation
        .try_borrow_mut()
        .map_err(|_| failure("original retirement met a held document borrow"))?;
    let observed = (
        navigation.host_context(),
        navigation.current_document().map(|lease| lease.stamp()),
    );
    navigation.destroy();
    retired.set(true);
    Ok(Some(observed))
}

#[cfg(test)]
mod tests;
