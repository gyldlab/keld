//! Closed canonical app envelope intake. Expected pre-entry refusal disposes
//! its actual input before release; an admitted record never loses its charge.
use super::capture::CapturedInput;
use super::control::{ControlKind, ControlWork};
use super::current::{Action, CurrentOperation};
use super::invocation::CallPayload;
use super::state::{RetainedSend, State};
use super::{OriginalBridge, WvError, failure};
use crate::original_binding::OriginalCredentialSlot;
use crate::original_call::active::PendingOriginalSend;
use crate::original_call::queue::OriginalInputCharge;
use crate::original_call::request_for_lease;
use crate::renderer_bridge::{RendererBridgeOperation, RendererBridgeRequest, RendererRoleWitness};
use crate::renderer_contract::CheckedNativeDocumentLease;
use crate::wv_link::{AppWvLinkEnvelope, WvLinkEnvelope};
use std::sync::mpsc::TrySendError;

mod call;

pub(super) struct PendingSendInput {
    request: Option<RendererBridgeRequest>,
    credential: String,
    role: RendererRoleWitness,
    input: Option<OriginalInputCharge<CallPayload>>, // Request/credential drop first.
}
impl PendingSendInput {
    pub(super) fn drive(&mut self, bridge: &OriginalBridge) -> Result<Action, WvError> {
        let nav = bridge
            .navigation
            .try_borrow()
            .map_err(|_| failure("Send handoff owner borrowed"))?;
        let mut state = bridge
            .state
            .try_borrow_mut()
            .map_err(|_| failure("Send handoff state borrowed"))?;
        let request = self
            .request
            .as_ref()
            .ok_or_else(|| failure("Send handoff lost its request"))?;
        let lease = request
            .document_lease
            .as_ref()
            .ok_or_else(|| failure("Send request lost its checked lease"))?;
        if bridge.retired.get() || !lease.belongs_to_owner(nav.document()) || !self.role.is_live() {
            drop(state);
            drop(nav);
            drop(self.request.take());
            drop(std::mem::take(&mut self.credential));
            bridge
                .state
                .try_borrow_mut()
                .map_err(|_| failure("cancelled Send storage owner borrowed"))?
                .calls
                .release_input(&mut self.input)?;
            return Ok(Action::Dispose);
        }
        let RendererBridgeOperation::Send {
            channel,
            original_call,
        } = &request.operation
        else {
            return Err(failure("pending Send changed operation"));
        };
        if state.pending_send.is_some()
            || !state.binding.registered()
            || !state.binding.authentic(
                nav.document(),
                &self.credential,
                OriginalCredentialSlot::App,
            )
            || !state.calls.active().is_some_and(|active| {
                active.id() == *original_call
                    && active.send_eligible(
                        nav.document(),
                        state.binding.route_role(),
                        &self.credential,
                        channel,
                    )
            })
        {
            return Err(failure(
                "retained Send lost exact original-function admission",
            ));
        }
        let id = request.request;
        let lease = lease.clone();
        let request = self
            .request
            .take()
            .ok_or_else(|| failure("Send request disappeared"))?;
        match bridge.endpoint.requests.try_send(request) {
            Ok(()) => {
                // Same pure channel cut as the shared queue. Attach before any
                // later fallible transition, even when an invariant fails.
                state.pending_send = Some(RetainedSend {
                    record: PendingOriginalSend::after_handoff(id, lease),
                    role: self.role.clone(),
                    input: self.input.take(),
                });
                let active = state
                    .calls
                    .active_mut()
                    .ok_or_else(|| failure("accepted Send lost original call"))?;
                active.send_handed_off().map_err(failure)?;
                Ok(Action::Dispose)
            }
            Err(TrySendError::Full(request)) => {
                self.request = Some(request);
                Ok(Action::Keep)
            }
            Err(TrySendError::Disconnected(request)) => {
                self.request = Some(request);
                Err(failure("Send endpoint disconnected with custody retained"))
            }
        }
    }
}

impl OriginalBridge {
    fn input_context(
        &self,
        input: &CapturedInput,
        credential: &str,
        slot: OriginalCredentialSlot,
        registered: bool,
    ) -> Result<(CheckedNativeDocumentLease, RendererRoleWitness, [u8; 32]), WvError> {
        if self.retired.get() {
            return Err(failure("retired bridge refuses new input"));
        }
        let nav = self
            .navigation
            .try_borrow()
            .map_err(|_| failure("input native owner borrowed"))?;
        let state = self
            .state
            .try_borrow()
            .map_err(|_| failure("input binding borrowed"))?;
        let lease = input
            .document
            .as_ref()
            .filter(|lease| lease.belongs_to_owner(nav.document()))
            .ok_or_else(|| failure("input does not retain the same current native lease"))?;
        if state.fault.is_faulted()
            || !state.binding.authentic(nav.document(), credential, slot)
            || (registered && !state.binding.registered())
        {
            return Err(failure("early, stale or unregistered original input"));
        }
        let role = state
            .binding
            .route_role()
            .cloned()
            .or_else(|| nav.host_role().cloned())
            .ok_or_else(|| failure("input has no actual Core caller"))?;
        Ok((lease.clone(), role, state.epoch()?))
    }
    fn pay_input(&self, input: &mut CapturedInput, additional: usize) -> Result<(), WvError> {
        let token = input
            .charge
            .as_mut()
            .ok_or_else(|| failure("input prepayment disappeared"))?;
        self.state
            .try_borrow_mut()
            .map_err(|_| failure("input storage owner borrowed"))?
            .calls
            .grow_input(token, additional)
    }
    #[expect(
        clippy::too_many_lines,
        reason = "One closed envelope dispatch preserves exact input-charge transfer and refused-input disposal."
    )]
    pub(super) fn intake(&self, input: &mut CapturedInput) -> Result<(), WvError> {
        let envelope = input
            .envelope
            .take()
            .ok_or_else(|| failure("native input already consumed"))?;
        match envelope {
            AppWvLinkEnvelope::Transport(WvLinkEnvelope::Bind) => {
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                self.observe_startup("bind_intake", input.document.as_ref(), None);
                self.wake();
                Ok(())
            }
            AppWvLinkEnvelope::Registered {
                document,
                methods: _,
            } => {
                let _ = self.input_context(input, &document, OriginalCredentialSlot::App, false)?;
                let nav = self
                    .navigation
                    .try_borrow()
                    .map_err(|_| failure("registration owner borrowed"))?;
                let mut state = self
                    .state
                    .try_borrow_mut()
                    .map_err(|_| failure("registration state borrowed"))?;
                state
                    .binding
                    .register(nav.document(), &document)
                    .map_err(failure)?;
                state.registration_ack_pending = true;
                drop(state);
                drop(nav);
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                self.observe_startup("registered", input.document.as_ref(), None);
                self.wake();
                Ok(())
            }
            AppWvLinkEnvelope::CallFinished { document, call } => {
                let mut state = self
                    .state
                    .try_borrow_mut()
                    .map_err(|_| failure("completion state borrowed"))?;
                state
                    .calls
                    .active_mut()
                    .ok_or_else(|| failure("unsolicited original completion"))?
                    .observe_function_finished(call, &document)
                    .map_err(failure)?;
                drop(state);
                self.wake();
                Ok(())
            }
            AppWvLinkEnvelope::ReleaseCall { document, call } => {
                let (lease, role, epoch) =
                    self.input_context(input, &document, OriginalCredentialSlot::App, true)?;
                if !self.current.is_empty() {
                    return Err(failure("bounded native control position is occupied"));
                }
                let mut work = ControlWork::local(ControlKind::Refused(call), lease, role, epoch)?
                    .map(CurrentOperation::Control);
                if work.is_none() || !self.current.attach(&mut work, |v| v) {
                    return Err(failure("bounded native refusal control unavailable"));
                }
                self.wake();
                Ok(())
            }
            AppWvLinkEnvelope::Call(call) => self.intake_call(input, call),
            AppWvLinkEnvelope::Transport(WvLinkEnvelope::Consumed {
                document,
                delivery,
                disposition,
            }) => {
                // flush_controls clones this exact diagnostic into the existing
                // endpoint. Prepay that simultaneous copy before retaining it.
                let copied = match &disposition {
                    crate::wv_link::WvConsumerDisposition::Refused { code, message } => code
                        .capacity()
                        .checked_add(message.capacity())
                        .ok_or_else(|| failure("consumer diagnostic storage overflow"))?,
                    _ => 0,
                };
                self.pay_input(input, copied)?;
                let mut protocol = self
                    .event
                    .protocol
                    .try_borrow_mut()
                    .map_err(|_| failure("event consumer record borrowed"))?;
                if protocol.consumed_input.is_some() || input.charge.is_none() {
                    return Err(failure("duplicate event consumer storage"));
                }
                protocol
                    .record
                    .as_mut()
                    .ok_or_else(|| failure("unsolicited event consumer"))?
                    .observe_consumed(delivery.get(), &document, disposition)
                    .map_err(failure)?;
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                let consumed_stamp = protocol
                    .record
                    .as_ref()
                    .map(|record| record.lease().stamp().0);
                // All checks ended; same owned disposition and token stay together.
                protocol.consumed_input = input.charge.take();
                drop(protocol);
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                if let Some(stamp) = consumed_stamp {
                    super::super::managed_test_control::navigation_post::consumed(
                        &self.navigation,
                        stamp,
                        delivery.get(),
                    );
                }
                self.wake();
                Ok(())
            }
            AppWvLinkEnvelope::Transport(WvLinkEnvelope::Send {
                document,
                message,
                channel,
                payload,
            }) => {
                let (lease, role, _) =
                    self.input_context(input, &document, OriginalCredentialSlot::App, true)?;
                let facts = input.facts(self)?; // Sole native URL parser, outside interior guards.
                let original_call = {
                    let nav = self
                        .navigation
                        .try_borrow()
                        .map_err(|_| failure("Send ingress owner borrowed"))?;
                    let mut state = self
                        .state
                        .try_borrow_mut()
                        .map_err(|_| failure("Send ingress state borrowed"))?;
                    state
                        .binding
                        .accept_message_sequence(message.get())
                        .map_err(failure)?;
                    if state.pending_send.is_some() || !self.current.is_empty() {
                        return Err(failure(
                            "Send lacks its exact unused original-function slot",
                        ));
                    }
                    state
                        .calls
                        .active()
                        .filter(|active| {
                            active.send_eligible(
                                nav.document(),
                                state.binding.route_role(),
                                &document,
                                &channel,
                            )
                        })
                        .map(crate::original_call::active::ActiveOriginalCall::id)
                        .ok_or_else(|| {
                            failure("Send lacks its exact unused original-function slot")
                        })?
                };
                let mut request = request_for_lease(
                    lease,
                    message.get(),
                    RendererBridgeOperation::Send {
                        channel,
                        original_call,
                    },
                    payload,
                );
                request.native_facts = Some(facts);
                request.role_witness = Some(role.clone());
                let mut pending = Some(PendingSendInput {
                    request: Some(request),
                    credential: document,
                    role,
                    input: input.charge.take(),
                });
                if !self.current.attach(&mut pending, CurrentOperation::Send) {
                    if let Some(mut retained) = pending {
                        drop(retained.request.take());
                        input.charge = retained.input.take();
                    }
                    return Err(failure("native Send position became occupied"));
                }
                self.wake();
                Ok(())
            }
            AppWvLinkEnvelope::Transport(WvLinkEnvelope::Invoke {
                document,
                request,
                channel,
                payload,
            }) => {
                let (lease, role, _) =
                    self.input_context(input, &document, OriginalCredentialSlot::Trusted, false)?;
                if channel.get() != self.endpoint.admitted_channel {
                    return Err(failure("numeric channel is not admitted by this endpoint"));
                }
                let facts = input.facts(self)?;
                let mut call = request_for_lease(
                    lease,
                    request.get(),
                    RendererBridgeOperation::Invoke,
                    payload,
                );
                call.channel = channel.get();
                call.native_facts = Some(facts);
                call.role_witness = Some(role);
                let mut pending = Some(call);
                let result = {
                    let nav = self
                        .navigation
                        .try_borrow()
                        .map_err(|_| failure("numeric input owner borrowed"))?;
                    let mut state = self
                        .state
                        .try_borrow_mut()
                        .map_err(|_| failure("numeric input state borrowed"))?;
                    let State {
                        calls,
                        pending_send,
                        ..
                    } = &mut *state;
                    self.event
                        .with_record(|event| {
                            calls.begin_numeric_result(
                                &mut pending,
                                nav.document(),
                                pending_send.as_ref().map(|v| &v.record),
                                event,
                            )
                        })?
                        .ok_or_else(|| {
                            failure("numeric input waits for retained event preparation")
                        })?
                };
                if result.is_err() && pending.is_none() {
                    self.fail("numeric admission lost its checked input cut");
                }
                result?;
                self.wake();
                Ok(())
            }
        }
    }
}
