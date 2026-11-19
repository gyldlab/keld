//! Retire the same selected resource and callback owners around actual Close.
#![forbid(unsafe_code)]

use std::rc::Rc;

use super::{NavigationCallbacks, OriginalView, PreloadInstaller, View, WvError, failure};

impl View {
    pub(in crate::webview2) fn retire_on_ui(&self) -> bool {
        let Some(original) = self.original.as_ref() else {
            return self.close_custody.close_on_ui();
        };
        if original.retire_before_close().is_err() || !self.close_custody.close_on_ui() {
            return false;
        }
        original.controller_closed_on_ui();
        original.is_drained()
    }
}

impl OriginalView {
    fn retire_before_close(&self) -> Result<(), WvError> {
        if !self.creator.close_native_offers() {
            self.session
                .latch_failure("selected producer retirement refused", self.id);
            return Err(failure("selected producer retirement refused"));
        }
        self.retired.set(true);
        self.navigation
            .try_borrow_mut()
            .map_err(|_| failure("selected retirement met a held document borrow"))?
            .destroy();
        if let Some(installer) = &self.installer {
            installer.retire_on_ui();
        }
        if let Some(callbacks) = &self.callbacks {
            callbacks
                .retire_on_ui()
                .map_err(|_| failure("selected callback retirement refused"))?;
        }
        if let Some(resource) = &self.resource {
            resource.retire_on_ui()?;
        }
        Ok(())
    }

    fn controller_closed_on_ui(&self) {
        if let Some(resource) = &self.resource {
            resource.controller_closed_on_ui();
        }
        if let Some(callbacks) = &self.callbacks {
            callbacks.controller_closed_on_ui();
        }
        if let Some(installer) = &self.installer {
            installer.controller_closed_on_ui();
        }
    }

    fn is_drained(&self) -> bool {
        self.retired.get()
            && self.creator.native_storage_is_quiescent()
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
    }
}
