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

pub(super) struct CanonicalRendererRoleScope {
    current: Weak<Mutex<Option<ActivePrimaryGeneration>>>,
    windows: Weak<Mutex<WindowSession>>,
    shutdown: SessionShutdownState,
    caller: PrimaryCallerContext,
}

impl CanonicalRendererRoleScope {
    pub(super) fn for_window(self, window: WindowRef) -> RendererRoleWitness {
        let stopped = Arc::clone(&self.caller.reader_stop);
        RendererRoleWitness::with_fence(
            stopped,
            Arc::new(CanonicalRendererRoleFence { role: self, window }),
        )
    }
}

struct CanonicalRendererRoleFence {
    role: CanonicalRendererRoleScope,
    window: WindowRef,
}

impl RendererRoleFence for CanonicalRendererRoleFence {
    fn try_admit_native(&self, admit: &mut dyn FnMut() -> bool) -> bool {
        let role = &self.role;
        let (Some(current), Some(windows)) = (role.current.upgrade(), role.windows.upgrade())
        else {
            return false;
        };
        let mut accepted = false;
        let result = with_primary_caller_locked(&current, &role.shutdown, &role.caller, |_| {
            if let Ok(windows) = windows.lock()
                && windows.current_original_binding(&role.caller, self.window)
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
    /// Capture the Runtime caller and Registry before allocating an original
    /// window identity. Native entry rechecks the finished witness binding.
    pub(super) fn renderer_role_scope(
        &self,
        caller: &PrimaryCallerContext,
    ) -> Option<CanonicalRendererRoleScope> {
        if !matches!(caller.identity, PrimaryLinkIdentity::Role(principal)
            if principal.owner() == RoleOwner::Primary)
        {
            return None;
        }
        Some(CanonicalRendererRoleScope {
            current: Arc::downgrade(&self.current),
            windows: Arc::downgrade(self.windows.as_ref()?),
            shutdown: self.shutdown.clone(),
            caller: caller.clone(),
        })
    }
}
