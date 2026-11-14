//! Native Echo reply storage uses the original pending-call prepayment only.
use super::current::Action;
use super::post::{PostAuthority, PostPhase, PostResult, PostTarget, post_prepared_with_entry};
use super::{OriginalBridge, WebviewId, WvError, failure};
use crate::renderer_bridge::{
    NativeNumericResultReservation, RendererBridgeOutcome, RendererRoleWitness,
};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::wv_link::HostRendererMessage;

pub(super) struct NumericWork {
    wide: Vec<u16>, // Must drop before the outcome's actual result storage token.
    document: CheckedNativeDocumentLease,
    role: RendererRoleWitness,
    epoch: Option<[u8; 32]>,
    phase: PostPhase,
    outcome: RendererBridgeOutcome,
}
impl NumericWork {
    pub(super) fn received(
        outcome: RendererBridgeOutcome,
        document: CheckedNativeDocumentLease,
        role: RendererRoleWitness,
        epoch: Option<[u8; 32]>,
    ) -> Self {
        Self {
            wide: Vec::new(),
            document,
            role,
            epoch,
            phase: PostPhase::Prepared,
            outcome,
        }
    }
    pub(super) fn token(&self) -> Option<&NativeNumericResultReservation> {
        token(&self.outcome)
    }
    pub(super) fn correlation(&self) -> (WebviewId, u64, u32) {
        (
            self.document.stamp().view(),
            u64::from(self.document.stamp().observed_generation()),
            self.outcome.request(),
        )
    }
    fn prepare(&mut self) -> Result<(), WvError> {
        if !self.wide.is_empty() {
            return Ok(());
        }
        let token =
            token(&self.outcome).ok_or_else(|| failure("numeric result lost original storage"))?;
        let (carrier, dto) = match &self.outcome {
            RendererBridgeOutcome::Reply {
                request, payload, ..
            } => (
                HostRendererMessage::Reply {
                    request: *request,
                    ok: true,
                    payload,
                },
                payload.capacity(),
            ),
            RendererBridgeOutcome::Error {
                request,
                code,
                detail,
                ..
            } => (
                HostRendererMessage::Error {
                    request: *request,
                    ok: false,
                    code,
                    detail,
                },
                code.capacity()
                    .checked_add(detail.capacity())
                    .ok_or_else(|| failure("numeric DTO overflow"))?,
            ),
            _ => return Err(failure("non-numeric result reached numeric position")),
        };
        let text = std::str::from_utf8(
            self.epoch
                .as_ref()
                .ok_or_else(|| failure("numeric result lost original epoch"))?,
        )
        .map_err(|_| failure("numeric epoch is not UTF8"))?;
        let (wide, _) = super::post::prepare_encoded_bytes(&carrier, text, |_, backing| {
            dto.checked_add(backing)
                .and_then(|v| v.checked_add(std::mem::size_of::<Self>()))
                .filter(|bytes| token.covers_storage(*bytes))
                .map(|_| ())
                .ok_or("numeric native representations exceed original prepayment")
        })
        .map_err(failure)?;
        self.wide = wide;
        Ok(())
    }
    pub(super) fn drive(&mut self, bridge: &OriginalBridge) -> Result<Action, WvError> {
        if bridge.retired.get() || !self.document.is_current() || !self.role.is_live() {
            self.phase.cancel_unentered();
            return Ok(
                if self
                    .phase
                    .physically_quiescent(bridge.close.actual_closed())
                {
                    Action::Dispose
                } else {
                    Action::Keep
                },
            );
        }
        self.prepare()?;
        let result = post_prepared_with_entry(
            &mut self.phase,
            &PostAuthority::Current(self.role.clone()),
            PostTarget {
                view: &bridge.view,
                expected_identity: bridge.identity,
                navigation: &bridge.navigation,
                wide: &self.wide,
                document: &self.document,
                epoch: self
                    .epoch
                    .as_ref()
                    .ok_or_else(|| failure("numeric epoch absent before native entry"))?,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                event_storage: None,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                carrier: super::super::managed_test_control::navigation_post::PostCarrier::Numeric,
            },
            &mut |lease, epoch, owner| {
                if bridge.retired.get() {
                    return false;
                }
                bridge.state.try_borrow().is_ok_and(|state| {
                    !state.fault.is_faulted() && state.matches(owner, lease, epoch, false)
                })
            },
        );
        match result {
            PostResult::Returned(Ok(())) | PostResult::AlreadyReturned => Ok(Action::Dispose),
            PostResult::NotAdmitted => Ok(Action::Keep),
            PostResult::Returned(Err(_)) => Err(failure("numeric native Post returned failure")),
            PostResult::OutcomeUnknown => Err(failure("numeric native Post outcome is unproved")),
        }
    }
}
pub(super) fn token(outcome: &RendererBridgeOutcome) -> Option<&NativeNumericResultReservation> {
    match outcome {
        RendererBridgeOutcome::Reply { numeric_result, .. }
        | RendererBridgeOutcome::Error { numeric_result, .. } => numeric_result.as_ref(),
        _ => None,
    }
}
