//! Physical Entry lifetime observations, independent of its current role/document.
use super::{WindowRef, WindowRegistry};
impl WindowRegistry {
    pub(in crate::app_session) fn native_view_scope_status(
        &self,
        window: WindowRef,
    ) -> Option<bool> {
        let entry = self.entries.get(&window.id.0)?;
        (entry.window == window && entry.original_selection.is_some())
            .then_some(entry.native_offers_closed)
    }
    pub(in crate::app_session) fn close_native_view_offers(&mut self, window: WindowRef) -> bool {
        let Some(entry) = self.entries.get_mut(&window.id.0) else {
            return false;
        };
        if entry.window != window || entry.original_selection.is_none() {
            return false;
        }
        entry.native_offers_closed = true;
        true
    }
    pub(in crate::app_session) fn native_offers_open_for(
        &self,
        caller: &crate::app_session::PrimaryCallerContext,
        window: WindowRef,
    ) -> bool {
        self.native_view_scope_status(window) == Some(false)
            && self.retains_caller_window(caller, window)
    }
}
