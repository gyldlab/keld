//! Existing shared protocol owners; no copied document owner or authority bit.
use super::invocation::CallPayload;
use super::startup::StartupReceipt;
use super::{EventLoopProxy, OriginalBridge, WebviewId, WindowsLoopEvent, WvError, failure};
use crate::original_binding::{OriginalBinding, OriginalCredentialSlot};
use crate::original_call::active::PendingOriginalSend;
use crate::original_call::queue::{OriginalCallQueue, OriginalControlFault, OriginalInputCharge};
use crate::renderer_bridge::RendererRoleWitness;
use crate::renderer_contract::{CheckedNativeDocumentLease, NativeAttemptObservation};

/// The existing pending-send record retains its actual inbound storage until
/// its exact Core terminal. It adds no slot, endpoint, queue or quota.
pub(super) struct RetainedSend {
    pub(super) record: PendingOriginalSend,
    pub(super) role: RendererRoleWitness,
    pub(super) input: Option<OriginalInputCharge<CallPayload>>,
}

pub(super) struct State {
    pub(super) binding: OriginalBinding,
    pub(super) startup: Option<StartupReceipt>,
    pub(super) calls: OriginalCallQueue<CallPayload>,
    pub(super) pending_send: Option<RetainedSend>,
    pub(super) fault: OriginalControlFault,
    pub(super) registration_ack_pending: bool,
}
impl State {
    pub(super) fn new(
        proxy: EventLoopProxy<WindowsLoopEvent>,
        id: WebviewId,
    ) -> Result<Self, WvError> {
        Ok(Self {
            binding: OriginalBinding::default(),
            startup: None,
            calls: OriginalCallQueue::new_with_numeric_result_slot(proxy, id)?,
            pending_send: None,
            fault: OriginalControlFault::default(),
            registration_ack_pending: false,
        })
    }

    pub(super) fn epoch(&self) -> Result<[u8; 32], WvError> {
        self.binding
            .credential(OriginalCredentialSlot::App)
            .and_then(|s| s.as_bytes().try_into().ok())
            .ok_or_else(|| failure("actual original binding has no fixed epoch"))
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
    /// Entropy/allocation is outside every interior guard. Credentials describe
    /// the actual committed `HostAttempt`; both Windows hooks consume one bind.
    pub(super) fn synchronize_commit(&self) -> Result<(), WvError> {
        if self.retired.get() {
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
                .ok_or_else(|| failure("commit lost actual HostAttempt"))?;
            let qualified_at = nav
                .qualified_at()
                .ok_or_else(|| failure("commit lacks actual qualification time"))?;
            (lease, NativeAttemptObservation(attempt), qualified_at)
        };
        let app = crate::renderer_bridge::credential()?;
        let trusted = app.clone(); // Actual Windows runtime calls numericBind(credential).
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
        let candidate = state
            .binding
            .take_matching_commit(nav.document())
            .map_err(failure)?;
        let Some(candidate) = candidate else {
            return Ok(());
        };
        state.calls.cancel_queued()?;
        state
            .binding
            .finish_commit(nav.document(), candidate)
            .map_err(failure)?;
        state.startup =
            Some(StartupReceipt::new(nav.document(), lease, qualified_at).map_err(failure)?);
        state.registration_ack_pending = false;
        drop(state);
        drop(nav);
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        self.observe_startup("qualified", None, None);
        Ok(())
    }
}
