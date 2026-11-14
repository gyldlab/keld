//! One actual Core DATA event retained in the bridge's current operation.
//! Native cancellation and physical backing release are independent facts.
use super::super::controller_custody::AdoptedControllerClose;
use super::post::{PostAuthority, PostPhase, PostResult, PostTarget};
use crate::original_event::PendingOriginalEvent;
use crate::renderer_bridge::{
    NativeDataReservation, RendererBridgeOperation, RendererBridgeOutcome, RendererBridgeRequest,
    RendererRoleWitness,
};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::resource_source::AdmittedWebAppSource;
use crate::wv_link::{HostRendererMessage, WvDispatchCertainty};
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
    wide: Vec<u16>,
    epoch: Option<[u8; 32]>,
    phase: PostPhase,
    retirement: Option<WvDispatchCertainty>,
    retirement_sent: bool,
    released: bool,
    prepared_record: Option<crate::original_event::PendingOriginalEvent>,
    storage: NativeDataReservation,
}

impl NativeEvent {
    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
    pub(super) fn queued_close_snapshot(
        &self,
        actual_closed: Option<bool>,
    ) -> super::super::managed_test_control::queued_close_observation::EventSnapshot {
        use super::super::managed_test_control::queued_close_observation::EventSnapshot;
        EventSnapshot {
            view: self.document.stamp().view().0,
            navigation: self.document.stamp().observed_generation(),
            message: self.delivery,
            post_phase: match self.phase {
                PostPhase::Prepared => "prepared",
                PostPhase::Cancelled => "cancelled",
                PostPhase::Posting => "posting",
                PostPhase::Returned => "returned",
                PostPhase::OutcomeUnknown => "outcome-unknown",
            },
            retirement: self.retirement.map(|certainty| match certainty {
                WvDispatchCertainty::NotDispatched => "not-dispatched",
                WvDispatchCertainty::OutcomeUnknown => "outcome-unknown",
            }),
            retirement_sent: self.retirement_sent,
            released: self.released,
            actual_closed,
        }
    }

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
                wide: Vec::new(),
                epoch: None,
                phase: PostPhase::Prepared,
                retirement: None,
                retirement_sent: false,
                released: false,
                prepared_record: None,
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

    pub(super) fn prepare(&mut self, epoch: &str) -> Result<(), WvError> {
        if self.phase != PostPhase::Prepared || self.retirement.is_some() {
            return Err(failure(
                "retired/entered event cannot prepare another carrier",
            ));
        }
        if let Some(prepared) = self.epoch {
            return if prepared.as_slice() == epoch.as_bytes() {
                Ok(())
            } else {
                Err(failure("event carrier cannot change original epoch"))
            };
        }
        let carrier = HostRendererMessage::Event {
            delivery: self.delivery,
            channel: &self.channel,
            payload: &self.payload,
        };
        let (wide, epoch) =
            super::post::prepare_encoded_bytes(&carrier, epoch, |encoded, additional| {
                self.storage
                    .try_extend_storage(encoded, additional)
                    .then_some(())
                    .ok_or("existing DATA storage budget unavailable")
            })
            .map_err(failure)?;
        self.wide = wide;
        self.epoch = Some(epoch);
        Ok(())
    }

    /// Prepare the SAME protocol record before the final local cut. Its String
    /// is prepaid on this DATA record. It is not visible to Consumed until the
    /// final entry moves it to the existing pending-event position.
    pub(super) fn prepared_record(&self) -> Option<&PendingOriginalEvent> {
        self.prepared_record.as_ref()
    }

    pub(super) fn prepare_protocol(
        &mut self,
        owner: &crate::native_document::NativeDocumentOwner,
        epoch: &str,
    ) -> Result<(), WvError> {
        self.prepare(epoch)?;
        if self.prepared_record.is_some() {
            return Ok(());
        }
        if !self.storage.try_extend_storage(
            1,
            32 + std::mem::size_of::<crate::original_event::PendingOriginalEvent>(),
        ) {
            return Err(failure(
                "event protocol metadata exceeds existing DATA storage",
            ));
        }
        let mut credential = String::new();
        credential
            .try_reserve_exact(32)
            .map_err(|_| failure("event credential allocation failed"))?;
        if epoch.len() != 32 || credential.capacity() > 32 {
            return Err(failure("event credential exceeds existing admission"));
        }
        credential.push_str(epoch);
        self.prepared_record = Some(
            crate::original_event::PendingOriginalEvent::begin_windows(
                owner,
                self.delivery,
                self.document.clone(),
                credential,
                self.role.clone(),
            )
            .map_err(failure)?,
        );
        Ok(())
    }

    /// The actual operation is held outside every `RefCell` during this call.
    /// The bridge's same current slot remains marked `NativeEntry`, so callbacks
    /// and actual reentrant Close cannot drop this body or its storage token.
    #[expect(
        clippy::type_complexity,
        reason = "The final callback borrows the exact lease, epoch, living document owner and same retained event record together."
    )]
    pub(super) fn post(
        &mut self,
        view: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
        identity: usize,
        navigation: &std::rc::Rc<
            std::cell::RefCell<super::super::original_navigation::OriginalNavigation>,
        >,
        enter: &mut dyn FnMut(
            &CheckedNativeDocumentLease,
            &[u8; 32],
            &crate::native_document::NativeDocumentOwner,
            &mut Option<crate::original_event::PendingOriginalEvent>,
        ) -> bool,
    ) -> PostResult {
        let Some(epoch) = self.epoch.as_ref() else {
            return PostResult::NotAdmitted;
        };
        if self.retirement.is_some() {
            return PostResult::NotAdmitted;
        }
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let event_storage = super::super::managed_test_control::navigation_post::EventStorage {
            delivery: self.delivery,
            payload_id: self.payload.as_ptr() as usize,
            payload_capacity: self.payload.capacity(),
            wide_id: self.wide.as_ptr() as usize,
            wide_capacity: self.wide.capacity(),
        };
        let authority = PostAuthority::Current(self.role.clone());
        super::post::post_prepared_with_entry(
            &mut self.phase,
            &authority,
            PostTarget {
                view,
                expected_identity: identity,
                navigation,
                wide: &self.wide,
                document: &self.document,
                epoch,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                carrier: super::super::managed_test_control::navigation_post::PostCarrier::Event,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                event_storage: Some(event_storage),
            },
            &mut |lease, epoch, owner| enter(lease, epoch, owner, &mut self.prepared_record),
        )
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
        pending: Option<&crate::original_event::PendingOriginalEvent>,
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
