//! Original-document binding metadata, shared without a second document owner.
//! Credentials correlate protocol records; native source/lease and Core role
//! admission remain authority. No script or native operation lives here.

use crate::native_document::{NativeCommittedStamp, NativeDocumentOwner};
use crate::renderer_bridge::RendererRoleWitness;
use crate::renderer_contract::{CheckedNativeDocumentLease, NativeAttemptObservation};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OriginalCredentialSlot {
    Trusted,
    App,
}

#[derive(Default)]
pub(crate) struct OriginalBinding {
    committed: Option<CheckedNativeDocumentLease>,
    prepared: Option<PreparedOriginalBinding>,
    app_credential: Option<String>,
    trusted_credential: Option<String>,
    registered: bool,
    registration_notice: Option<CheckedNativeDocumentLease>,
    route_role: Option<RendererRoleWitness>,
    last_message: u32,
}
struct PreparedOriginalBinding {
    attempt: NativeAttemptObservation,
    app: String,
    trusted: String,
}

/// The same prepared credentials in transit across existing queue cancellation.
/// No native call occurs here; finish must recheck the exact living lease/source.
/// This value is not cloneable and is not a new pending queue or authority token.
pub(crate) struct PendingBindingCommit {
    prepared: PreparedOriginalBinding,
    lease: CheckedNativeDocumentLease,
}

impl OriginalBinding {
    /// Stage only the actual owner's existing attempt. Native bootstrap
    /// installation, if required by that platform, has its own real receipt.
    pub(crate) fn prepare_attempt(
        &mut self,
        owner: &NativeDocumentOwner,
        attempt: NativeAttemptObservation,
        app: String,
        trusted: String,
    ) -> Result<(), &'static str> {
        let source = owner
            .source_binding()
            .ok_or("pending bootstrap has no native bound source")?;
        if owner
            .resource_lease(attempt.0, source.source_id())
            .is_none()
        {
            return Err("bootstrap does not match actual pending NativeAttempt");
        }
        self.prepared = Some(PreparedOriginalBinding {
            attempt,
            app,
            trusted,
        });
        Ok(())
    }

    /// The adapter observed an actual native change, but cannot prove that its
    /// prepared metadata still describes it. An ordinary rejected pre-native
    /// request must not discard an otherwise valid prepared attempt.
    pub(crate) fn invalidate_prepared(&mut self) {
        self.prepared = None;
    }

    /// Keep this after the adapter replaces its retained native script owners,
    /// and before its existing queue cancellation. No native object is dropped.
    pub(crate) fn clear_for_attempt(&mut self) {
        self.committed = None;
        self.clear_credentials();
    }
    pub(crate) fn clear_credentials(&mut self) {
        self.app_credential = None;
        self.trusted_credential = None;
        self.registered = false;
    }

    pub(crate) fn current(&self, owner: &NativeDocumentOwner) -> Option<NativeCommittedStamp> {
        let lease = self.committed.as_ref()?;
        lease.belongs_to_owner(owner).then_some(lease.0.stamp())
    }

    /// Take the existing prepared record before cancelling old queued work.
    /// A repeated notification for this exact commit does not reset metadata.
    pub(crate) fn take_matching_commit(
        &mut self,
        owner: &NativeDocumentOwner,
    ) -> Result<Option<PendingBindingCommit>, &'static str> {
        let stamp = owner
            .current_stamp()
            .ok_or("no qualified committed application document")?;
        if self.current(owner) == Some(stamp) {
            return Ok(None);
        }
        let prepared = self
            .prepared
            .take()
            .filter(|prepared| prepared.attempt.0 == stamp.attempt)
            .ok_or("actual commit has no matching native-prepared bootstrap")?;
        let lease = owner
            .lease_for(stamp)
            .map(CheckedNativeDocumentLease)
            .filter(|lease| lease.belongs_to_owner(owner))
            .ok_or("actual commit lost its exact native source binding")?;
        Ok(Some(PendingBindingCommit { prepared, lease }))
    }

    /// Call only after existing queued originals have been cancelled. A reentry
    /// or retirement during cancellation cannot redirect this prepared record.
    pub(crate) fn finish_commit(
        &mut self,
        owner: &NativeDocumentOwner,
        candidate: PendingBindingCommit,
    ) -> Result<(), &'static str> {
        if !candidate.lease.belongs_to_owner(owner)
            || candidate.prepared.attempt.0 != candidate.lease.0.stamp().attempt
        {
            return Err("prepared binding lost its actual committed native lease");
        }
        self.committed = Some(candidate.lease);
        self.app_credential = Some(candidate.prepared.app);
        self.trusted_credential = Some(candidate.prepared.trusted);
        self.registered = false;
        self.registration_notice = None;
        self.route_role = None;
        self.last_message = 0;
        Ok(())
    }

    pub(crate) fn credential(&self, slot: OriginalCredentialSlot) -> Option<&str> {
        match slot {
            OriginalCredentialSlot::App => self.app_credential.as_deref(),
            OriginalCredentialSlot::Trusted => self.trusted_credential.as_deref(),
        }
    }
    pub(crate) const fn has_prepared_attempt(&self) -> bool {
        self.prepared.is_some()
    }
    pub(crate) const fn registered(&self) -> bool {
        self.registered
    }
    pub(crate) fn authentic(
        &self,
        owner: &NativeDocumentOwner,
        supplied: &str,
        slot: OriginalCredentialSlot,
    ) -> bool {
        self.current(owner).is_some() && self.credential(slot) == Some(supplied)
    }
    pub(crate) fn original_current(
        &self,
        owner: &NativeDocumentOwner,
        lease: &CheckedNativeDocumentLease,
        credential: &str,
    ) -> bool {
        self.registered
            && self.authentic(owner, credential, OriginalCredentialSlot::App)
            && self
                .committed
                .as_ref()
                .is_some_and(|bound| bound.same_binding(lease))
            && lease.belongs_to_owner(owner)
    }

    pub(crate) fn register(
        &mut self,
        owner: &NativeDocumentOwner,
        credential: &str,
    ) -> Result<(), &'static str> {
        if self.registered || !self.authentic(owner, credential, OriginalCredentialSlot::App) {
            return Err("private registry binding is stale or duplicate");
        }
        let lease = self
            .committed
            .as_ref()
            .ok_or("registry notice lost actual native commit")?;
        self.registration_notice = Some(lease.clone());
        self.registered = true;
        Ok(())
    }
    pub(crate) fn registration_notice(&self) -> Option<&CheckedNativeDocumentLease> {
        self.registration_notice.as_ref()
    }
    /// Only after the existing endpoint accepted the exact registration notice.
    pub(crate) fn registration_handed_off(
        &mut self,
        lease: &CheckedNativeDocumentLease,
    ) -> Result<(), &'static str> {
        if !self
            .registration_notice
            .as_ref()
            .is_some_and(|pending| pending.same_binding(lease))
        {
            return Err("registration handoff differs from retained notice");
        }
        self.registration_notice = None;
        Ok(())
    }
    pub(crate) fn discard_revoked_registration(&mut self) {
        if self
            .registration_notice
            .as_ref()
            .is_some_and(|lease| !lease.is_current())
        {
            self.registration_notice = None;
        }
    }

    pub(crate) fn route_role(&self) -> Option<&RendererRoleWitness> {
        self.route_role.as_ref()
    }
    pub(crate) fn role_current(&self, role: &RendererRoleWitness) -> bool {
        role.is_live()
            && self
                .route_role
                .as_ref()
                .is_some_and(|current| current.is_live() && current.same_role(role))
    }
    pub(crate) fn observe_route(
        &mut self,
        owner: &NativeDocumentOwner,
        navigation: u64,
        ready: bool,
        role: RendererRoleWitness,
    ) -> bool {
        if self
            .current(owner)
            .is_none_or(|stamp| u64::from(stamp.generation.get()) != navigation)
        {
            return false;
        }
        if ready {
            if !role.is_live() {
                return false;
            }
            self.route_role = Some(role);
        } else {
            if !self
                .route_role
                .as_ref()
                .is_some_and(|current| current.same_role(&role))
            {
                return false;
            }
            self.route_role = None;
        }
        true
    }

    /// Preserve the original +1 check and its position before active-Send
    /// validation. Refused later dispatch does not rewind this message sequence.
    pub(crate) fn accept_message_sequence(&mut self, message: u32) -> Result<(), &'static str> {
        if message
            != self
                .last_message
                .checked_add(1)
                .ok_or("app send sequence exhausted")?
        {
            return Err("duplicate, skipped or replayed app message");
        }
        self.last_message = message;
        Ok(())
    }
}

#[cfg(all(test, windows))]
#[path = "original_binding_tests.rs"]
mod tests;
