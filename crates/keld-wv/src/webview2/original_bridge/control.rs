//! CONTROL storage stays on the existing Core reservation and one current slot.
use super::current::Action;
use super::post::{PostAuthority, PostResult, PreparedControl};
use super::startup::BoundEntry;
use super::state::{RetainedSend, State};
use super::{Instant, OriginalBridge, WvError, failure};
use crate::renderer_bridge::{
    NativeControlReservation, RendererBridgeOutcome, RendererRoleWitness,
};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::wv_link::HostRendererMessage;

#[derive(Clone, Copy)]
pub(super) enum ControlKind {
    Bound(BoundEntry),
    Registered,
    Refused(u32),
    Outcome,
}
pub(super) struct ControlWork {
    kind: ControlKind,
    document: CheckedNativeDocumentLease,
    role: RendererRoleWitness,
    epoch: Option<[u8; 32]>,
    outcome: Option<RendererBridgeOutcome>,
    send_input: Option<RetainedSend>,
    prepared: Option<PreparedControl>,
    charge: Option<NativeControlReservation>,
}
impl ControlWork {
    pub(super) fn local(
        kind: ControlKind,
        document: CheckedNativeDocumentLease,
        role: RendererRoleWitness,
        epoch: [u8; 32],
    ) -> Result<Option<Self>, WvError> {
        let carrier = carrier(kind, None)?;
        let text =
            std::str::from_utf8(&epoch).map_err(|_| failure("invalid fixed control epoch"))?;
        let encoded = carrier
            .encoded_len(text)
            .map_err(|_| failure("invalid canonical local control"))?;
        let Some(charge) =
            role.try_reserve_native_control(encoded, 1024 + std::mem::size_of::<Self>())
        else {
            return Ok(None);
        };
        Ok(Some(Self {
            kind,
            document,
            role,
            epoch: Some(epoch),
            outcome: None,
            send_input: None,
            prepared: None,
            charge: Some(charge),
        }))
    }
    /// Borrowed outcome/target validation already ran. This moves the same DTO
    /// and token without a fallible step or a second reservation.
    pub(super) fn received(
        outcome: RendererBridgeOutcome,
        document: CheckedNativeDocumentLease,
        role: RendererRoleWitness,
        epoch: Option<[u8; 32]>,
        send_input: Option<RetainedSend>,
    ) -> Self {
        Self {
            kind: ControlKind::Outcome,
            document,
            role,
            epoch,
            outcome: Some(outcome),
            send_input,
            prepared: None,
            charge: None,
        }
    }
    fn may_post(&self, bridge: &OriginalBridge) -> bool {
        if bridge.retired.get() || !self.document.is_current() {
            return false;
        }
        matches!(
            &self.outcome,
            Some(RendererBridgeOutcome::RouteState {
                ready: false,
                retirement: Some(_),
                ..
            })
        ) || self.role.is_live()
    }
    fn prepare(&mut self) -> Result<(), WvError> {
        if self.prepared.is_some() {
            return Ok(());
        }
        if self.charge.is_none() {
            self.charge = match self.outcome.as_mut() {
                Some(
                    RendererBridgeOutcome::SendResult { control, .. }
                    | RendererBridgeOutcome::RouteState { control, .. },
                ) => control.take(),
                _ => None,
            };
        }
        let authority = match self.outcome.as_ref() {
            Some(RendererBridgeOutcome::RouteState {
                ready: false,
                retirement: Some(permit),
                ..
            }) => PostAuthority::RetiredRoute(permit.clone()),
            _ => PostAuthority::Current(self.role.clone()),
        };
        let message = carrier(self.kind, self.outcome.as_ref())?;
        let epoch = std::str::from_utf8(
            self.epoch
                .as_ref()
                .ok_or_else(|| failure("control lost original epoch"))?,
        )
        .map_err(|_| failure("control epoch is not UTF8"))?;
        let charge = self
            .charge
            .take()
            .ok_or_else(|| failure("actual Core CONTROL token is absent"))?;
        match PreparedControl::prepare(&message, epoch, self.document.clone(), authority, charge) {
            Ok(value) => {
                self.prepared = Some(value);
                Ok(())
            }
            Err(error) => {
                self.charge = Some(error.charge);
                Err(failure(error.detail))
            }
        }
    }
    fn dispose_if_quiescent(&mut self, bridge: &OriginalBridge) -> Result<Action, WvError> {
        if let Some(prepared) = self.prepared.take() {
            match prepared.into_charge_after_close(&bridge.close) {
                Ok(charge) => self.charge = Some(charge),
                Err(retained) => {
                    self.prepared = Some(retained);
                    return Ok(Action::Keep);
                }
            }
        }
        // Actual original DTO and inbound Send charge are gone before CONTROL.
        drop(self.outcome.take());
        if let Some(send) = self.send_input.as_mut() {
            bridge
                .state
                .try_borrow_mut()
                .map_err(|_| failure("Send storage release owner borrowed"))?
                .calls
                .release_input(&mut send.input)?;
        }
        drop(self.send_input.take());
        Ok(Action::Dispose)
    }
    pub(super) fn drive(&mut self, bridge: &OriginalBridge) -> Result<Action, WvError> {
        if !self.may_post(bridge) {
            return self.dispose_if_quiescent(bridge);
        }
        self.prepare()?;
        let kind = self.kind;
        let role = &self.role;
        let ready = match &self.outcome {
            Some(RendererBridgeOutcome::RouteState { ready, .. }) => Some(*ready),
            _ => None,
        };
        let Some(prepared) = self.prepared.as_mut() else {
            return Err(failure("prepared control disappeared"));
        };
        let result = prepared.post_with_entry(
            &bridge.view,
            bridge.identity,
            &bridge.navigation,
            &mut |lease, epoch, owner| {
                if bridge.retired.get() {
                    return false;
                }
                let Ok(mut state) = bridge.state.try_borrow_mut() else {
                    return false;
                };
                let registered = !matches!(kind, ControlKind::Bound(_));
                if state.fault.is_faulted() || !state.matches(owner, lease, epoch, registered) {
                    return false;
                }
                if ready == Some(true) && !state.binding.role_current(role) {
                    return false;
                }
                match kind {
                    ControlKind::Bound(entry) => {
                        let State {
                            binding, startup, ..
                        } = &mut *state;
                        startup
                            .as_mut()
                            .is_some_and(|s| s.begin_bound(entry, binding, owner, Instant::now()))
                    }
                    ControlKind::Registered => {
                        if !state.registration_ack_pending {
                            return false;
                        }
                        state.registration_ack_pending = false;
                        true
                    }
                    ControlKind::Refused(_) | ControlKind::Outcome => true,
                }
            },
        );
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if matches!(&result, PostResult::Returned(Ok(()))) {
            match kind {
                ControlKind::Bound(BoundEntry::Initial) => {
                    bridge.observe_startup("initial_returned", None, None);
                }
                ControlKind::Bound(BoundEntry::DomRetry) => {
                    bridge.observe_startup("dom_retry_returned", None, None);
                }
                ControlKind::Registered | ControlKind::Refused(_) | ControlKind::Outcome => {}
            }
        }
        match result {
            PostResult::Returned(Ok(())) | PostResult::AlreadyReturned => {
                self.dispose_if_quiescent(bridge)
            }
            PostResult::NotAdmitted => {
                if self.may_post(bridge) {
                    Ok(Action::Keep)
                } else {
                    self.dispose_if_quiescent(bridge)
                }
            }
            PostResult::Returned(Err(_)) => Err(failure("CONTROL native Post returned failure")),
            PostResult::OutcomeUnknown => Err(failure("CONTROL native Post outcome is unproved")),
        }
    }
}
fn carrier(
    kind: ControlKind,
    outcome: Option<&RendererBridgeOutcome>,
) -> Result<HostRendererMessage<'_>, WvError> {
    Ok(match kind {
        ControlKind::Bound(_) => HostRendererMessage::Bound { ready: false },
        ControlKind::Registered => HostRendererMessage::AppRegistered,
        ControlKind::Refused(call) => HostRendererMessage::AppRefused { call },
        ControlKind::Outcome => match outcome {
            Some(RendererBridgeOutcome::SendResult {
                request,
                disposition,
                ..
            }) => HostRendererMessage::SendResult {
                message: *request,
                disposition,
            },
            Some(RendererBridgeOutcome::RouteState { ready, .. }) => {
                HostRendererMessage::RouteState { ready: *ready }
            }
            _ => return Err(failure("non-CONTROL outcome reached the control position")),
        },
    })
}
const _: () = assert!(std::mem::size_of::<ControlWork>() <= 1024);
