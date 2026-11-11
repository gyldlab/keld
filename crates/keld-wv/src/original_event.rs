//! One retained original event and its independent completion facts.
//! Native adapters supply actual physical receipts; this owner does not infer
//! execution, consumption or Core release from native dispatch acceptance.

use crate::native_document::NativeDocumentOwner;
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::wv_link::WvConsumerDisposition;

pub(crate) struct PendingOriginalEvent {
    id: u32,
    lease: CheckedNativeDocumentLease,
    credential: String,
    disposition: Option<WvConsumerDisposition>,
    consumed_sent: bool,
    quiesced: Option<bool>,
    quiesced_sent: bool,
}

impl PendingOriginalEvent {
    /// Called in the admitting UI turn. The living owner, not a remembered
    /// current flag or numeric stamp, must still own this exact source/lease.
    pub(crate) fn begin(
        owner: &NativeDocumentOwner,
        id: u32,
        lease: CheckedNativeDocumentLease,
        credential: String,
    ) -> Result<Self, &'static str> {
        if id == 0
            || !owner
                .lease_for(lease.0.stamp())
                .is_some_and(|current| current.same_binding(&lease.0))
            || !owner
                .source_binding()
                .is_some_and(|source| lease.belongs_to_source(&source))
        {
            return Err("addressed event native lease revoked or mismatched");
        }
        Ok(Self {
            id,
            lease,
            credential,
            disposition: None,
            consumed_sent: false,
            quiesced: None,
            quiesced_sent: false,
        })
    }

    pub(crate) const fn id(&self) -> u32 {
        self.id
    }
    pub(crate) const fn lease(&self) -> &CheckedNativeDocumentLease {
        &self.lease
    }

    /// Terminal cleanup uses the original retained correlation even after
    /// logical revocation. It grants no new delivery or document authority.
    pub(crate) fn observe_consumed(
        &mut self,
        id: u32,
        credential: &str,
        disposition: WvConsumerDisposition,
    ) -> Result<(), &'static str> {
        if self.id != id
            || self.credential != credential
            || self.consumed_sent
            || self.disposition.is_some()
        {
            return Err("stale, unsolicited or duplicate consumer completion");
        }
        self.disposition = Some(disposition);
        Ok(())
    }

    /// Only the platform's actual completion owner may supply this receipt.
    /// A current successor cannot settle the original event's native custody.
    pub(crate) fn observe_native_quiescence(
        &mut self,
        id: u32,
        lease: &CheckedNativeDocumentLease,
        completed: bool,
    ) -> bool {
        if self.id != id || !self.lease.same_binding(lease) {
            return false;
        }
        self.quiesced = Some(completed);
        true
    }

    pub(crate) fn pending_consumed(&self) -> Option<&WvConsumerDisposition> {
        (!self.consumed_sent)
            .then_some(self.disposition.as_ref())
            .flatten()
    }
    pub(crate) fn pending_quiescence(&self) -> Option<bool> {
        (!self.quiesced_sent).then_some(self.quiesced).flatten()
    }

    /// Call only after the same bounded endpoint accepted this record's
    /// Consumed control. Full leaves this record and its pending fact intact.
    pub(crate) fn consumed_handed_off(&mut self) -> Result<(), &'static str> {
        if self.pending_consumed().is_none() {
            return Err("consumer handoff has no pending original fact");
        }
        self.consumed_sent = true;
        Ok(())
    }
    /// Call only after the existing endpoint accepted `EventQuiesced` for this
    /// record. This changes no consumer or Core release fact.
    pub(crate) fn quiescence_handed_off(&mut self) -> Result<(), &'static str> {
        if self.pending_quiescence().is_none() {
            return Err("native quiescence handoff has no pending original fact");
        }
        self.quiesced_sent = true;
        Ok(())
    }

    /// Validate an actual Core `EventReleased` outcome. Consumer completion is
    /// Core's separate release prerequisite; it is never manufactured here.
    pub(crate) fn can_release(&self, navigation: u64, id: u32) -> Result<bool, &'static str> {
        if self.id != id || u64::from(self.lease.stamp().observed_generation()) != navigation {
            return Ok(false);
        }
        if self.quiesced.is_none() || !self.quiesced_sent {
            return Err("Core attempted event release before actual native quiescence");
        }
        Ok(true)
    }

    pub(crate) fn retirement_controls_quiesced(&self) -> bool {
        self.quiesced_sent && (self.consumed_sent || !self.lease.is_current())
    }

    pub(crate) const fn consumed_sent(&self) -> bool {
        self.consumed_sent
    }
    pub(crate) const fn native_quiescence(&self) -> Option<bool> {
        self.quiesced
    }
    pub(crate) const fn quiescence_sent(&self) -> bool {
        self.quiesced_sent
    }
}

#[cfg(all(test, windows))]
#[path = "original_event_tests.rs"]
mod tests;
