//! One actual Core DATA event retained in the bridge's current operation.
//! Native cancellation and physical backing release are independent facts.
use super::super::controller_custody::AdoptedControllerClose;
use super::post::PostPhase;
use crate::original_event::PendingOriginalEvent;
use crate::renderer_bridge::{
    NativeDataReservation, RendererBridgeOperation, RendererBridgeOutcome, RendererBridgeRequest,
    RendererRoleWitness,
};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::resource_source::AdmittedWebAppSource;
use crate::wv_link::WvDispatchCertainty;
use crate::{WebviewId, WvError};
use std::sync::Arc;

/// Host-only proof of retirement of one original DATA event. No public
/// constructor, Clone, deserializer, page field or successor lookup can mint it.
/// This logical terminal never proves disposal of the native body or consumer
/// execution; the original operation retains that physical custody separately.
#[derive(Debug)]
pub struct NativeEventRetirement {
    delivery: u32,
    document: CheckedNativeDocumentLease,
    role: RendererRoleWitness,
    certainty: WvDispatchCertainty,
}
impl NativeEventRetirement {
    /// Original DATA correlation; never a replacement id.
    #[must_use]
    pub const fn delivery(&self) -> u32 {
        self.delivery
    }
    /// The exact lease captured by the already-admitted Core DATA record.
    #[must_use]
    pub const fn document(&self) -> &CheckedNativeDocumentLease {
        &self.document
    }
    /// Retained original caller. Cleanup compares `same_role`, not `is_live`.
    #[must_use]
    pub const fn role(&self) -> &RendererRoleWitness {
        &self.role
    }
    /// Existing `RouteRetired` wire certainty selected by actual native facts.
    #[must_use]
    pub const fn certainty(&self) -> WvDispatchCertainty {
        self.certainty
    }
}

/// The native half of the existing single pending-event owner, not another
/// event queue. Actual body allocations precede their storage token.
pub(super) struct NativeEvent {
    delivery: u32,
    document: CheckedNativeDocumentLease,
    role: RendererRoleWitness,
    channel: String,
    payload: Vec<u8>,
    phase: PostPhase,
    retirement: Option<WvDispatchCertainty>,
    retirement_sent: bool,
    released: bool,
    storage: NativeDataReservation,
}

impl NativeEvent {
    /// Check borrowed metadata first. Every refusal leaves the caller's actual
    /// Outcome untouched. The following take/attach has no fallible/native step.
    pub(super) fn attach(
        pending: &mut Option<RendererBridgeOutcome>,
        view: WebviewId,
        source: &Arc<AdmittedWebAppSource>,
    ) -> Result<Option<Self>, WvError> {
        let Some(RendererBridgeOutcome::Event {
            webview,
            navigation,
            request,
            document,
            storage,
            ..
        }) = pending.as_ref()
        else {
            return Ok(None);
        };
        if *webview != view
            || *request == 0
            || document.stamp().view() != view
            || u64::from(document.stamp().observed_generation()) != *navigation
            || !document.belongs_to_source(source)
            || storage.is_none()
        {
            return Err(failure("Core event lacks its exact source/DATA custody"));
        }
        match pending.take() {
            Some(RendererBridgeOutcome::Event {
                request,
                document,
                role,
                channel,
                payload,
                storage: Some(storage),
                ..
            }) => Ok(Some(Self {
                delivery: request,
                document,
                role,
                channel,
                payload,
                phase: PostPhase::Prepared,
                retirement: None,
                retirement_sent: false,
                released: false,
                storage,
            })),
            retained => {
                *pending = retained;
                Err(failure(
                    "borrowed Core event changed before owned attachment",
                ))
            }
        }
    }

    pub(super) const fn delivery(&self) -> u32 {
        self.delivery
    }
    pub(super) const fn document(&self) -> &CheckedNativeDocumentLease {
        &self.document
    }
    pub(super) const fn role(&self) -> &RendererRoleWitness {
        &self.role
    }

    /// The only production proof entry: read the SAME actual Close owner.
    /// A lease/role stop is not a close receipt. Prepared cancellation is
    /// irreversible before a `NotDispatched` proof can exist.
    pub(super) fn retirement_request(
        &mut self,
        close: &AdoptedControllerClose,
    ) -> Option<RendererBridgeRequest> {
        let proof = self.select_retirement(close.actual_closed())?;
        Some(crate::original_call::request_for_lease(
            self.document.clone(),
            self.delivery,
            RendererBridgeOperation::EventRetired { retirement: proof },
            Vec::new(),
        ))
    }

    fn select_retirement(&mut self, actual_closed: bool) -> Option<NativeEventRetirement> {
        if self.retirement_sent
            || (self.document.is_current() && self.role.is_live() && !actual_closed)
        {
            return None;
        }
        let certainty = if let Some(certainty) = self.retirement {
            certainty
        } else if self.phase.cancel_unentered() {
            // Cancellation blocks this SAME operation immediately, but role
            // stop or a Close observation cannot stand in for revocation of
            // its exact checked lease. Keep the cancelled body until the
            // existing navigation owner has actually revoked that lease.
            if self.document.is_current() {
                return None;
            }
            self.retirement = Some(WvDispatchCertainty::NotDispatched);
            WvDispatchCertainty::NotDispatched
        } else if actual_closed {
            self.retirement = Some(WvDispatchCertainty::OutcomeUnknown);
            WvDispatchCertainty::OutcomeUnknown
        } else {
            return None;
        };
        Some(NativeEventRetirement {
            delivery: self.delivery,
            document: self.document.clone(),
            role: self.role.clone(),
            certainty,
        })
    }

    /// Only after the same bounded request endpoint accepted `EventRetired`.
    /// Full leaves the selected fact and actual body in this same operation.
    pub(super) fn retirement_handed_off(&mut self) -> Result<(), WvError> {
        if self.retirement.is_none() || self.retirement_sent {
            return Err(failure(
                "event retirement handoff lacks its original selection",
            ));
        }
        self.retirement_sent = true;
        Ok(())
    }

    /// `EventReleased` is an actual Core terminal observation only. It does not
    /// settle an in-call native borrow, manufacture Consumed, or drop storage.
    pub(super) fn observe_release(
        &mut self,
        navigation: u64,
        delivery: u32,
        pending: Option<&PendingOriginalEvent>,
    ) -> Result<bool, WvError> {
        if self.delivery != delivery
            || u64::from(self.document.stamp().observed_generation()) != navigation
            || self.released
        {
            return Ok(false);
        }
        if !self.retirement_sent {
            let Some(record) = pending.filter(|record| {
                record.id() == self.delivery && record.lease().same_binding(&self.document)
            }) else {
                return Ok(false);
            };
            // An actual consumer terminal may win while a later retirement
            // control is still Full. Preserve that first selected terminal.
            if !record.can_release(navigation, delivery).map_err(failure)? {
                return Ok(false);
            }
        }
        self.released = true;
        Ok(true)
    }

    pub(super) fn can_dispose(&self, close: &AdoptedControllerClose) -> bool {
        self.disposable(close.actual_closed())
    }

    fn disposable(&self, actual_closed: bool) -> bool {
        self.released && self.phase.physically_quiescent(actual_closed)
    }
}
fn failure(detail: &'static str) -> WvError {
    WvError::RendererBridge {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests;
