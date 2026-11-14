//! Forward to the same physical Registry Entry; this owns no state or quota.
use super::{PrimaryCallerContext, WindowSession};
use keld_ipc::window::WindowRef;
impl WindowSession {
    pub(in crate::app_session) fn native_view_scope_status(
        &self,
        window: WindowRef,
    ) -> Option<bool> {
        self.registry.native_view_scope_status(window)
    }
    pub(in crate::app_session) fn close_native_view_offers(&mut self, window: WindowRef) -> bool {
        self.registry.close_native_view_offers(window)
    }
    pub(in crate::app_session) fn native_offers_open_for(
        &self,
        caller: &PrimaryCallerContext,
        window: WindowRef,
    ) -> bool {
        self.registry.native_offers_open_for(caller, window)
    }
}
