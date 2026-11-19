//! Final native admission is ordered by the existing primary and Registry owners.

use std::sync::{Arc, Mutex, Weak};

use keld_ipc::window::WindowRef;
use keld_runtime::primary::RoleOwner;
use keld_wv::NativeNavigationContext;
use keld_wv::renderer_bridge::{
    NativeControlReservation, RendererControlOwner, RendererRoleFence, RendererRoleWitness,
};

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
        self.witness(window, None)
    }

    pub(super) fn for_original_load(
        self,
        window: WindowRef,
        context: NativeNavigationContext,
    ) -> RendererRoleWitness {
        self.witness(window, Some(context))
    }

    fn witness(
        self,
        window: WindowRef,
        original_load: Option<NativeNavigationContext>,
    ) -> RendererRoleWitness {
        let stopped = Arc::clone(&self.caller.reader_stop);
        let owner = Arc::new(CanonicalRendererRoleFence {
            role: self,
            window,
            original_load,
        });
        RendererRoleWitness::with_fence(stopped, owner.clone()).with_controls(owner)
    }
}

struct CanonicalRendererRoleFence {
    role: CanonicalRendererRoleScope,
    window: WindowRef,
    original_load: Option<NativeNavigationContext>,
}

impl RendererRoleFence for CanonicalRendererRoleFence {
    fn try_admit_native(&self, admit: &mut dyn FnMut() -> bool) -> bool {
        // The callback commits only native-local state. Native I/O starts
        // after this fence returns and all these guards drop.
        self.with_live_scope(|windows| {
            self.original_load.is_none_or(|context| {
                windows.original_load_can_enter_native(&self.role.caller, self.window, context)
            }) && admit()
        })
        .unwrap_or(false)
    }
}

impl CanonicalRendererRoleFence {
    fn with_live_scope<T>(&self, admit: impl FnOnce(&WindowSession) -> T) -> Option<T> {
        let role = &self.role;
        let (current, windows) = (role.current.upgrade()?, role.windows.upgrade()?);
        let mut admitted = None;
        with_primary_caller_locked(&current, &role.shutdown, &role.caller, |_| {
            if let Ok(windows) = windows.lock()
                && windows.current_original_binding(&role.caller, self.window)
            {
                admitted = Some(admit(&windows));
            }
            Ok(())
        })
        .ok()?;
        admitted
    }

    /// Serialize observations on the same physical producer even after the
    /// creating caller stops. This cut cannot authorize native effects.
    fn with_physical_scope<T>(&self, observe: impl FnOnce(&mut WindowSession) -> T) -> Option<T> {
        let current = self.role.current.upgrade()?;
        let windows = self.role.windows.upgrade()?;
        let _transition = self.role.shutdown.transition_guard();
        let _current = current.lock().ok()?;
        let mut windows = windows.lock().ok()?;
        windows.compat()?;
        Some(observe(&mut windows))
    }
}

impl RendererControlOwner for CanonicalRendererRoleFence {
    fn has_native_view_scope(&self) -> bool {
        self.with_physical_scope(|windows| windows.native_view_scope_status(self.window).is_some())
            .unwrap_or(false)
    }

    fn close_native_offers(&self) -> bool {
        self.with_physical_scope(|windows| windows.close_native_view_offers(self.window))
            .unwrap_or(false)
    }

    fn native_storage_is_quiescent(&self) -> bool {
        self.with_physical_scope(|windows| {
            windows.native_view_scope_status(self.window) == Some(true)
                && windows
                    .compat()
                    .is_some_and(|compat| compat.native_storage_is_quiescent(self.window))
        })
        .unwrap_or(false)
    }

    fn try_reserve_native_control(
        &self,
        encoded_bytes: usize,
        accounted_bytes: usize,
    ) -> Option<NativeControlReservation> {
        self.with_live_scope(|windows| {
            windows
                .compat()?
                .reserve_native_control(self.window, encoded_bytes, accounted_bytes)
        })
        .flatten()
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
