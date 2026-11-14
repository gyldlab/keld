//! Original-call completion custody shared by native adapters.
//! These are the same retained correlations, not proof of native execution.

use crate::native_document::NativeDocumentOwner;
use crate::renderer_bridge::RendererRoleWitness;
use crate::renderer_contract::CheckedNativeDocumentLease;

pub(crate) struct ActiveOriginalCall {
    id: u32,
    lease: CheckedNativeDocumentLease,
    credential: String,
    channel: Option<String>,
    send_forwarded: bool,
    function_finished: bool,
    native_finished: bool,
    local_bytes: usize,
    role_witness: Option<RendererRoleWitness>,
}

impl ActiveOriginalCall {
    /// Retain the invocation selected by the existing scheduling owner after
    /// its same-turn living document/source/role checks. This creates no lease
    /// or authority and does not mean the original Function has executed.
    pub(crate) fn entered(
        id: u32,
        lease: CheckedNativeDocumentLease,
        credential: String,
        channel: Option<String>,
        local_bytes: usize,
        role_witness: Option<RendererRoleWitness>,
    ) -> Self {
        Self {
            id,
            lease,
            credential,
            channel,
            send_forwarded: false,
            function_finished: false,
            native_finished: false,
            local_bytes,
            role_witness,
        }
    }

    /// The original Function will not enter. Its private refusal carrier still
    /// retains native custody and the same local charge until actual completion.
    pub(crate) fn refused_before_entry(
        id: u32,
        lease: CheckedNativeDocumentLease,
        credential: String,
        local_bytes: usize,
    ) -> Self {
        let mut call = Self::entered(id, lease, credential, None, local_bytes, None);
        call.function_finished = true;
        call
    }

    pub(crate) const fn id(&self) -> u32 {
        self.id
    }
    pub(crate) const fn lease(&self) -> &CheckedNativeDocumentLease {
        &self.lease
    }
    pub(crate) fn channel(&self) -> Option<&str> {
        self.channel.as_deref()
    }
    pub(crate) const fn local_bytes(&self) -> usize {
        self.local_bytes
    }
    #[cfg(any(target_os = "macos", test))]
    pub(crate) const fn send_forwarded(&self) -> bool {
        self.send_forwarded
    }
    #[cfg(any(
        test,
        all(target_os = "macos", feature = "profile-test-hooks", debug_assertions)
    ))]
    pub(crate) const fn function_finished(&self) -> bool {
        self.function_finished
    }
    pub(crate) const fn native_finished(&self) -> bool {
        self.native_finished
    }

    /// Cleanup remains correlated to the original retained invocation after
    /// logical revocation. Repeated matching completion is still idempotent.
    pub(crate) fn observe_function_finished(
        &mut self,
        id: u32,
        credential: &str,
    ) -> Result<(), &'static str> {
        if self.id != id || self.credential != credential {
            return Err("original invocation completion does not match retained physical custody");
        }
        self.function_finished = true;
        Ok(())
    }

    /// The platform must supply an actual native completion receipt. Native API
    /// acceptance or a posted consumer message is not such a receipt.
    pub(crate) fn observe_native_quiescence(
        &mut self,
        id: u32,
        lease: &CheckedNativeDocumentLease,
        completed: bool,
    ) -> bool {
        if self.id != id || !self.lease.same_binding(lease) {
            return false;
        }
        self.native_finished = true;
        if !completed || !lease.is_current() {
            self.function_finished = true;
        }
        true
    }

    /// Actual Windows Post return settles only the carrier half. Revocation
    /// and `S_OK` cannot manufacture the original Function's completion.
    #[cfg(windows)]
    pub(crate) fn observe_native_return(
        &mut self,
        id: u32,
        lease: &CheckedNativeDocumentLease,
    ) -> bool {
        if self.id != id || !self.lease.same_binding(lease) {
            return false;
        }
        self.native_finished = true;
        true
    }

    /// Both completion halves are present. Removal still requires any
    /// [`Self::needs_preflight_abort`] handoff through the existing endpoint;
    /// this observation cannot release that correlation or its local charge.
    pub(crate) const fn ready_to_release(&self) -> bool {
        self.function_finished && self.native_finished
    }
    pub(crate) fn needs_preflight_abort(&self) -> bool {
        self.ready_to_release() && self.channel.is_some() && !self.send_forwarded
    }

    /// Same-turn eligibility only. The caller also checks registration, retire
    /// state and message order; Windows native publication still requires the
    /// canonical Core role fence. Never retain this observation as authority.
    pub(crate) fn send_eligible(
        &self,
        owner: &NativeDocumentOwner,
        current_role: Option<&RendererRoleWitness>,
        credential: &str,
        channel: &str,
    ) -> bool {
        self.credential == credential
            && self.channel.as_deref() == Some(channel)
            && !self.send_forwarded
            && self.lease.belongs_to_owner(owner)
            && self.role_witness.as_ref().is_some_and(|captured| {
                captured.is_live()
                    && current_role
                        .is_some_and(|current| current.is_live() && current.same_role(captured))
            })
    }

    /// Only after this original Send was accepted by the existing endpoint.
    pub(crate) fn send_handed_off(&mut self) -> Result<(), &'static str> {
        if self.send_forwarded || self.channel.is_none() || self.role_witness.is_none() {
            return Err("original Send handoff has no unused admitted effect slot");
        }
        self.send_forwarded = true;
        Ok(())
    }
}

pub(crate) struct PendingOriginalSend {
    id: u32,
    lease: CheckedNativeDocumentLease,
}
impl PendingOriginalSend {
    /// Construct only after the existing endpoint accepted the exact request.
    pub(crate) const fn after_handoff(id: u32, lease: CheckedNativeDocumentLease) -> Self {
        Self { id, lease }
    }
    #[cfg(any(
        test,
        all(target_os = "macos", feature = "profile-test-hooks", debug_assertions)
    ))]
    pub(crate) const fn id(&self) -> u32 {
        self.id
    }
    pub(crate) const fn lease(&self) -> &CheckedNativeDocumentLease {
        &self.lease
    }
    pub(crate) fn matches_result(&self, navigation: u64, id: u32) -> bool {
        self.id == id && u64::from(self.lease.stamp().observed_generation()) == navigation
    }
}

#[cfg(all(test, windows))]
#[path = "active_tests.rs"]
mod tests;
