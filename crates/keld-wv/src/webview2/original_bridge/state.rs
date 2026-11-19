//! The startup binding records actual navigation, caller, and endpoint observations.
use std::sync::mpsc::TrySendError;

use super::startup::StartupReceipt;
use super::{OriginalBridge, WvError, failure};
use crate::original_binding::{OriginalBinding, OriginalCredentialSlot};
use crate::renderer_bridge::RendererBridgeOperation;
use crate::renderer_contract::{CheckedNativeDocumentLease, NativeAttemptObservation};

#[derive(Default)]
pub(super) struct State {
    pub(super) binding: OriginalBinding,
    pub(super) startup: Option<StartupReceipt>,
    pub(super) registration_ack_pending: bool,
}

impl State {
    pub(super) fn epoch(&self) -> Result<[u8; 32], WvError> {
        self.binding
            .credential(OriginalCredentialSlot::App)
            .and_then(|s| s.as_bytes().try_into().ok())
            .ok_or_else(|| failure("actual selected binding has no fixed epoch"))
    }

    pub(super) fn matches(
        &self,
        owner: &crate::native_document::NativeDocumentOwner,
        lease: &CheckedNativeDocumentLease,
        epoch: &[u8; 32],
        registered: bool,
    ) -> bool {
        let Ok(epoch) = std::str::from_utf8(epoch) else {
            return false;
        };
        if registered {
            self.binding.original_current(owner, lease, epoch)
        } else {
            self.binding
                .authentic(owner, epoch, OriginalCredentialSlot::App)
                && lease.belongs_to_owner(owner)
                && self.binding.current(owner) == Some(lease.0.stamp())
        }
    }
}

impl OriginalBridge {
    /// Only the real selected `ContentLoading` commit can supply this attempt,
    /// lease and qualification timestamp. Entropy is outside interior guards.
    pub(super) fn synchronize_commit(&self) -> Result<(), WvError> {
        if self.retired.get() || self.faulted.get() {
            return Ok(());
        }
        let (lease, attempt, qualified_at) = {
            let nav = self
                .navigation
                .try_borrow()
                .map_err(|_| failure("binding owner borrowed"))?;
            let Some(lease) = nav.current_document() else {
                return Ok(());
            };
            let state = self
                .state
                .try_borrow()
                .map_err(|_| failure("binding state borrowed"))?;
            if state.binding.current(nav.document()) == Some(lease.0.stamp()) {
                return Ok(());
            }
            let attempt = nav
                .active_attempt()
                .ok_or_else(|| failure("native commit lost its HostAttempt"))?;
            let qualified_at = nav
                .qualified_at()
                .ok_or_else(|| failure("native commit lacks qualification time"))?;
            (lease, NativeAttemptObservation(attempt), qualified_at)
        };
        let app = crate::renderer_bridge::credential()?;
        let trusted = app.clone(); // Windows Legacy uses the same exact numeric bind credential.
        let nav = self
            .navigation
            .try_borrow()
            .map_err(|_| failure("binding owner reentered"))?;
        if !lease.belongs_to_owner(nav.document()) {
            return Err(failure("binding lease retired during preparation"));
        }
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| failure("binding state reentered"))?;
        state
            .binding
            .prepare_attempt(nav.document(), attempt, app, trusted)
            .map_err(failure)?;
        if let Some(candidate) = state
            .binding
            .take_matching_commit(nav.document())
            .map_err(failure)?
        {
            state
                .binding
                .finish_commit(nav.document(), candidate)
                .map_err(failure)?;
            state.startup =
                Some(StartupReceipt::new(nav.document(), lease, qualified_at).map_err(failure)?);
            state.registration_ack_pending = false;
        }
        Ok(())
    }

    pub(super) fn register_app(
        &self,
        captured: &CheckedNativeDocumentLease,
        credential: &str,
    ) -> Result<(), WvError> {
        if self.retired.get() || self.faulted.get() {
            return Err(failure("retired selected bridge refuses registration"));
        }
        let nav = self
            .navigation
            .try_borrow()
            .map_err(|_| failure("registration document owner borrowed"))?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| failure("registration state borrowed"))?;
        if !captured.belongs_to_owner(nav.document())
            || nav.host_role().is_none_or(|role| !role.is_live())
            || state.binding.current(nav.document()) != Some(captured.0.stamp())
            || state.startup.as_ref().is_none_or(|startup| {
                startup.expired(&state.binding, nav.document(), std::time::Instant::now())
            })
        {
            return Err(failure(
                "registration lacks the same current document and live creator",
            ));
        }
        state
            .binding
            .register(nav.document(), credential)
            .map_err(failure)?;
        state.registration_ack_pending = true;
        Ok(())
    }

    pub(super) fn record_registered_post_return(
        &self,
        lease: &CheckedNativeDocumentLease,
    ) -> Result<(), WvError> {
        let nav = self
            .navigation
            .try_borrow()
            .map_err(|_| failure("AppRegistered return document owner borrowed"))?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| failure("AppRegistered return state borrowed"))?;
        let State {
            startup, binding, ..
        } = &mut *state;
        if self.retired.get()
            || self.faulted.get()
            || !startup.as_mut().is_some_and(|receipt| {
                receipt.observe_registered_post_return(
                    binding,
                    nav.document(),
                    lease,
                    std::time::Instant::now(),
                )
            })
        {
            return Err(failure(
                "AppRegistered return missed its original startup cut",
            ));
        }
        Ok(())
    }

    /// Identity-only registration notice on the existing bounded endpoint.
    /// A full endpoint leaves the same native document notice pending.
    pub(super) fn flush_registration(&self) -> Result<(), WvError> {
        let nav = self
            .navigation
            .try_borrow()
            .map_err(|_| failure("registration handoff owner borrowed"))?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| failure("registration handoff state borrowed"))?;
        state.binding.discard_revoked_registration();
        let Some(lease) = state.binding.registration_notice().cloned() else {
            return Ok(());
        };
        if self.retired.get() || !lease.belongs_to_owner(nav.document()) {
            return Ok(());
        }
        let Some(role) = nav.host_role().filter(|role| role.is_live()) else {
            return Ok(());
        };
        if state.startup.as_ref().is_none_or(|startup| {
            startup.expired(&state.binding, nav.document(), std::time::Instant::now())
        }) {
            return Err(failure(
                "registration handoff reached its original deadline",
            ));
        }
        let mut request = crate::original_call::request_for_lease(
            lease.clone(),
            0,
            RendererBridgeOperation::Registered,
            Vec::new(),
        );
        request.role_witness = Some(role.clone());
        match self.endpoint.requests.try_send(request) {
            Ok(()) => state
                .binding
                .registration_handed_off(&lease)
                .map_err(failure),
            Err(TrySendError::Full(_)) => Ok(()),
            Err(TrySendError::Disconnected(_)) => Err(failure(
                "registration endpoint disconnected with notice retained",
            )),
        }
    }
}
