//! Final native admission uses the existing primary generation owner.
//! The native callback changes only its local posting state; actual native I/O
//! begins after the canonical transition/current guards have been released.

use std::sync::{Arc, Mutex, Weak};

use keld_wv::renderer_bridge::{RendererRoleFence, RendererRoleWitness};

use super::{
    ActivePrimaryGeneration, PrimaryCallerContext, PrimaryRouterHandle, SessionShutdownState,
    with_primary_caller_locked,
};

#[cfg(windows)]
mod native_scope;

/// Actual Event role backing, charged within the existing DATA metadata slab.
#[cfg(windows)]
pub(in crate::app_session) const NATIVE_ROLE_FENCE_BACKING_BYTES: usize =
    std::mem::size_of::<CanonicalRendererRoleFence>() + 2 * std::mem::size_of::<usize>();

struct CanonicalRendererRoleFence {
    // Weak ownership prevents a queued native observation from keeping the
    // entire primary session alive after its owner has gone away.
    current: Weak<Mutex<Option<ActivePrimaryGeneration>>>,
    shutdown: SessionShutdownState,
    caller: PrimaryCallerContext,
    #[cfg(windows)]
    compat: Option<Weak<Mutex<super::compat_router::CompatSession>>>,
    #[cfg(windows)]
    windows: Option<Weak<Mutex<super::window_router::WindowSession>>>,
    #[cfg(windows)]
    window: Option<keld_ipc::window::WindowRef>,
}

impl RendererRoleFence for CanonicalRendererRoleFence {
    fn try_admit_native(&self, admit: &mut dyn FnMut() -> bool) -> bool {
        let Some(current) = self.current.upgrade() else {
            return false;
        };
        let mut accepted = false;
        let result = with_primary_caller_locked(&current, &self.shutdown, &self.caller, |_| {
            // This is the same caller fence as Core publication/retirement,
            // including Runtime identity and the exact reader-stop allocation.
            // No Core guard escapes this call or crosses the following COM call.
            #[cfg(windows)]
            if let Some(window) = self.window {
                let Some(windows) = self.windows.as_ref().and_then(Weak::upgrade) else {
                    return Ok(());
                };
                let Ok(windows) = windows.lock() else {
                    return Ok(());
                };
                if !windows.native_offers_open_for(&self.caller, window) {
                    return Ok(());
                }
                accepted = admit();
                return Ok(());
            }
            accepted = admit();
            Ok(())
        });
        result.is_ok() && accepted
    }
}

impl PrimaryRouterHandle {
    #[cfg(any(not(windows), test))]
    pub(super) fn renderer_role_witness(
        &self,
        caller: &PrimaryCallerContext,
    ) -> RendererRoleWitness {
        self.renderer_role_witness_scoped(caller, None)
    }

    #[cfg(windows)]
    pub(super) fn renderer_role_witness_for_window(
        &self,
        caller: &PrimaryCallerContext,
        window: keld_ipc::window::WindowRef,
    ) -> RendererRoleWitness {
        self.renderer_role_witness_scoped(caller, Some(window))
    }

    fn renderer_role_witness_scoped(
        &self,
        caller: &PrimaryCallerContext,
        #[cfg(windows)] window: Option<keld_ipc::window::WindowRef>,
        #[cfg(not(windows))] _window: Option<keld_ipc::window::WindowRef>,
    ) -> RendererRoleWitness {
        let owner = Arc::new(CanonicalRendererRoleFence {
            current: Arc::downgrade(&self.current),
            shutdown: self.shutdown.clone(),
            caller: caller.clone(),
            #[cfg(windows)]
            compat: self.compat.as_ref().map(Arc::downgrade),
            #[cfg(windows)]
            windows: self.windows.as_ref().map(Arc::downgrade),
            #[cfg(windows)]
            window,
        });
        let witness =
            RendererRoleWitness::with_fence(Arc::clone(&caller.reader_stop), owner.clone());
        #[cfg(windows)]
        let witness = witness.with_controls(owner);
        witness
    }
}

#[cfg(windows)]
impl keld_wv::renderer_bridge::RendererControlOwner for CanonicalRendererRoleFence {
    fn try_reserve_native_control(
        &self,
        encoded_bytes: usize,
        accounted_bytes: usize,
    ) -> Option<keld_wv::renderer_bridge::NativeControlReservation> {
        self.reserve_control_for_captured_window(encoded_bytes, accounted_bytes)
    }
    fn has_native_view_scope(&self) -> bool {
        self.captured_view_scope_status().is_some()
    }
    fn close_native_offers(&self) -> bool {
        self.close_captured_native_offers()
    }
    fn native_storage_is_quiescent(&self) -> bool {
        self.captured_native_storage_quiescent()
    }
}

/// One operation ticket, not a replacement role or document state. All identity
/// and physical-document truth remains in the existing current/Registry owners.
#[cfg(windows)]
struct CanonicalRouteRetirementFence {
    current: Weak<Mutex<Option<ActivePrimaryGeneration>>>,
    windows: Weak<Mutex<super::window_router::WindowSession>>,
    shutdown: SessionShutdownState,
    owner: super::window_registry::WindowOwner,
    window: keld_ipc::window::WindowRef,
    caller: PrimaryCallerContext,
    compat: Option<Weak<Mutex<super::compat_router::CompatSession>>>,
    document: keld_wv::renderer_contract::CheckedNativeDocumentLease,
    // Serialized by the same transition/current fence. A failed local native
    // admission does not consume it; a successful admission cannot replay it.
    consumed: std::sync::atomic::AtomicBool,
}

#[cfg(windows)]
impl keld_wv::renderer_bridge::RendererRouteRetirementFence for CanonicalRouteRetirementFence {
    fn try_reserve_route_unavailable(
        &self,
        document: &keld_wv::renderer_contract::CheckedNativeDocumentLease,
        encoded_bytes: usize,
        accounted_bytes: usize,
    ) -> Option<keld_wv::renderer_bridge::NativeControlReservation> {
        let current = self.current.upgrade()?;
        let windows = self.windows.upgrade()?;
        let compat = self.compat.as_ref()?.upgrade()?;
        let _transition = self.shutdown.transition_guard();
        let _current = current.lock().ok()?;
        let windows = windows.lock().ok()?;
        if self.consumed.load(std::sync::atomic::Ordering::Acquire)
            || !self.document.same_binding(document)
            || !windows.retains_retiring_renderer_route(
                self.owner,
                &self.caller.reader_stop,
                self.window,
                document,
            )
        {
            return None;
        }
        compat.lock().ok()?.reserve_native_control_for_window(
            &self.caller,
            self.window,
            encoded_bytes,
            accounted_bytes,
            Arc::clone(&compat),
        )
    }

    fn try_admit_route_unavailable(
        &self,
        document: &keld_wv::renderer_contract::CheckedNativeDocumentLease,
        admit: &mut dyn FnMut() -> bool,
    ) -> bool {
        let (Some(current), Some(windows)) = (self.current.upgrade(), self.windows.upgrade())
        else {
            return false;
        };
        let _transition = self.shutdown.transition_guard();
        let Ok(_current) = current.lock() else {
            return false;
        };
        let Ok(windows) = windows.lock() else {
            return false;
        };
        if self.consumed.load(std::sync::atomic::Ordering::Acquire)
            || !self.document.same_binding(document)
            || !windows.retains_retiring_renderer_route(
                self.owner,
                &self.caller.reader_stop,
                self.window,
                document,
            )
        {
            return false;
        }
        // This permits only A's fixed ready:false while the Registry still
        // associates this exact physical document with A. It deliberately does
        // not turn the stopped A into a live caller, nor select a current B.
        if !admit() {
            return false;
        }
        self.consumed
            .store(true, std::sync::atomic::Ordering::Release);
        true
    }
}

#[cfg(windows)]
impl PrimaryRouterHandle {
    /// Caller holds the actual retirement cut and passes its qualified Entry
    /// snapshot before clearing the route. No validation is deferred into a
    /// page-derived identity; final admission rechecks the retained Registry.
    pub(super) fn renderer_route_retirement(
        &self,
        caller: &PrimaryCallerContext,
        route: &super::window_registry::RendererRoute,
    ) -> Option<keld_wv::renderer_bridge::RouteRetirementPermit> {
        let windows = self.windows.as_ref()?;
        Some(keld_wv::renderer_bridge::RouteRetirementPermit::from_owner(
            Arc::new(CanonicalRouteRetirementFence {
                current: Arc::downgrade(&self.current),
                windows: Arc::downgrade(windows),
                shutdown: self.shutdown.clone(),
                owner: super::window_registry::WindowOwner::new(caller.identity, caller.attempt),
                window: route.window,
                caller: caller.clone(),
                compat: self.compat.as_ref().map(Arc::downgrade),
                document: route.document.clone(),
                consumed: std::sync::atomic::AtomicBool::new(false),
            }),
        ))
    }
}

#[cfg(test)]
mod tests;
