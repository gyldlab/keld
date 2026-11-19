//! Final native admission is ordered by the existing primary and Registry owners.

use std::sync::{Arc, Mutex, Weak};

use keld_ipc::window::WindowRef;
use keld_runtime::primary::RoleOwner;
use keld_wv::renderer_bridge::{RendererRoleFence, RendererRoleWitness};

use super::window_router::WindowSession;
use super::{
    ActivePrimaryGeneration, PrimaryCallerContext, PrimaryLinkIdentity, PrimaryRouterHandle,
    SessionShutdownState, with_primary_caller_locked,
};

struct CanonicalRendererRoleFence {
    current: Weak<Mutex<Option<ActivePrimaryGeneration>>>,
    windows: Weak<Mutex<WindowSession>>,
    shutdown: SessionShutdownState,
    caller: PrimaryCallerContext,
    window: WindowRef,
}

impl RendererRoleFence for CanonicalRendererRoleFence {
    fn try_admit_native(&self, admit: &mut dyn FnMut() -> bool) -> bool {
        let (Some(current), Some(windows)) = (self.current.upgrade(), self.windows.upgrade())
        else {
            return false;
        };
        let mut accepted = false;
        let result = with_primary_caller_locked(&current, &self.shutdown, &self.caller, |_| {
            if let Ok(windows) = windows.lock()
                && windows.current_original_binding(&self.caller, self.window)
            {
                // The callback commits only native-local state. Native I/O
                // starts after this fence returns and all these guards drop.
                accepted = admit();
            }
            Ok(())
        });
        result.is_ok() && accepted
    }
}

impl PrimaryRouterHandle {
    /// Captures the Runtime-authenticated caller and its admitted original window.
    /// Native admission rechecks both because this witness may outlive either.
    pub(super) fn renderer_role_witness_for_window(
        &self,
        caller: &PrimaryCallerContext,
        window: WindowRef,
    ) -> Option<RendererRoleWitness> {
        if !matches!(caller.identity, PrimaryLinkIdentity::Role(principal)
            if principal.owner() == RoleOwner::Primary)
        {
            return None;
        }
        let owner = Arc::new(CanonicalRendererRoleFence {
            current: Arc::downgrade(&self.current),
            windows: Arc::downgrade(self.windows.as_ref()?),
            shutdown: self.shutdown.clone(),
            caller: caller.clone(),
            window,
        });
        Some(RendererRoleWitness::with_fence(
            Arc::clone(&caller.reader_stop),
            owner,
        ))
    }
}
