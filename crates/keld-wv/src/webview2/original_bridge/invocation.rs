//! One canonical app/refusal carrier in the shared queue's cold reusable backing.
use super::post::{PostAuthority, PostPhase, PostResult, PostTarget, post_prepared_with_entry};
use super::state::State;
use super::{OriginalBridge, WvError, failure};
use crate::original_call::queue::{OriginalInvocationKind, PreparingOriginalInvocation};
use crate::wv_link::{AppPageCall, HostRendererMessage};

pub(super) struct CallPayload {
    pub(super) call: AppPageCall,
    wide: Vec<u16>,
    encoded_limit: usize,
    value_bytes: usize,
}
impl CallPayload {
    /// Both possible canonical carriers are counted before the existing input
    /// record prepays either buffer; this is not a second codec or value walk.
    pub(super) fn layout(
        call: &AppPageCall,
        value_bytes: usize,
    ) -> Result<(usize, usize), WvError> {
        let original = HostRendererMessage::app_call(call)
            .encoded_len(&call.document)
            .map_err(|_| failure("invalid canonical original call"))?;
        let refusal = HostRendererMessage::AppRefused { call: call.call }
            .encoded_len(&call.document)
            .map_err(|_| failure("invalid canonical original refusal"))?;
        let encoded = original.max(refusal);
        let bytes = encoded
            .checked_add(1)
            .and_then(|v| v.checked_mul(2))
            .and_then(|v| v.checked_add(encoded))
            .and_then(|v| v.checked_add(value_bytes))
            .ok_or_else(|| failure("original call carrier storage overflow"))?;
        Ok((encoded, bytes))
    }
    pub(super) fn new(call: AppPageCall, encoded_limit: usize, value_bytes: usize) -> Self {
        Self {
            call,
            wide: Vec::new(),
            encoded_limit,
            value_bytes,
        }
    }
    fn render(
        &mut self,
        kind: OriginalInvocationKind,
        id: u32,
        epoch: &str,
        limit: usize,
    ) -> Result<usize, WvError> {
        if self.call.call != id || self.call.document != epoch {
            return Err(failure(
                "original renderer differs from retained correlation",
            ));
        }
        let message = match kind {
            OriginalInvocationKind::Original => HostRendererMessage::app_call(&self.call),
            OriginalInvocationKind::Refusal => HostRendererMessage::AppRefused { call: id },
        };
        let required = message
            .encoded_len(epoch)
            .map_err(|_| failure("invalid original carrier"))?;
        let units = self
            .encoded_limit
            .checked_add(1)
            .ok_or_else(|| failure("original wide overflow"))?;
        let paid = self
            .value_bytes
            .checked_add(self.encoded_limit)
            .and_then(|v| v.checked_add(units.checked_mul(2)?))
            .filter(|v| *v <= limit)
            .ok_or_else(|| failure("original carrier exceeds retained prepayment"))?;
        if required > self.encoded_limit {
            return Err(failure("original encoded carrier grew beyond admission"));
        }
        let bytes = message
            .encode(epoch)
            .map_err(|_| failure("original carrier serialization failed"))?;
        let text =
            std::str::from_utf8(&bytes).map_err(|_| failure("original carrier is not UTF8"))?;
        self.wide.clear();
        if self.wide.capacity() == 0 {
            self.wide
                .try_reserve_exact(units)
                .map_err(|_| failure("original wide allocation failed"))?;
        }
        if self.wide.capacity() > units
            || self.wide.capacity() < units
            || bytes.capacity() > self.encoded_limit
        {
            return Err(failure("original carrier capacity differs from admission"));
        }
        self.wide.extend(text.encode_utf16());
        self.wide.push(0);
        if self.wide.len() > units {
            return Err(failure("original wide carrier exceeded admission"));
        }
        Ok(paid)
    }
}

impl OriginalBridge {
    pub(super) fn drive_call(&self) -> Result<(), WvError> {
        if self.retired.get() || !self.current.is_empty() {
            return Ok(());
        }
        let (progress, role) = {
            let nav = self
                .navigation
                .try_borrow()
                .map_err(|_| failure("call owner borrowed"))?;
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| failure("call state borrowed"))?;
            // These actual inbound owners alias the same cold backing until
            // their owned storage is disposed; preparation cannot mutate it.
            if state
                .pending_send
                .as_ref()
                .is_some_and(|send| send.input.is_some())
                || self
                    .event
                    .protocol
                    .try_borrow()
                    .map_err(|_| failure("event input custody is borrowed"))?
                    .consumed_input
                    .is_some()
            {
                return Ok(());
            }
            let State {
                binding,
                calls,
                pending_send,
                fault,
                ..
            } = &mut *state;
            let progress = self
                .event
                .with_record(|event| {
                    calls.take_next_preparation(
                        binding,
                        nav.document(),
                        &self.endpoint.requests,
                        (pending_send.as_ref().map(|v| &v.record), event),
                        fault,
                        CallPayload::render,
                    )
                })?
                .transpose()?;
            let Some(progress) = progress else {
                return Ok(());
            };
            let role = progress
                .value
                .as_ref()
                .and_then(|call| calls.preparation_role(call))
                .or_else(|| binding.route_role().cloned())
                .or_else(|| nav.host_role().cloned());
            (progress, role)
        };
        if progress.needs_service_wake {
            self.wake();
        }
        let Some(invocation) = progress.value else {
            return Ok(());
        };
        let Some(role) = role else {
            self.defer_call(invocation)?;
            return Err(failure("original call has no actual Core caller fence"));
        };
        self.post_call(invocation, role)
    }
    fn defer_call(&self, call: PreparingOriginalInvocation<CallPayload>) -> Result<(), WvError> {
        let progress = self
            .state
            .try_borrow_mut()
            .map_err(|_| failure("call deferral owner borrowed"))?
            .calls
            .defer_preparation(call)?;
        if progress.needs_service_wake {
            self.wake();
        }
        Ok(())
    }
    fn post_call(
        &self,
        invocation: PreparingOriginalInvocation<CallPayload>,
        role: crate::renderer_bridge::RendererRoleWitness,
    ) -> Result<(), WvError> {
        let document = invocation.identity().lease().clone();
        let id = invocation.identity().id();
        let payload = invocation.payload()?;
        let epoch: [u8; 32] = payload
            .call
            .document
            .as_bytes()
            .try_into()
            .map_err(|_| failure("original call epoch lost its fixed shape"))?;
        let mut phase = PostPhase::Prepared;
        let result = post_prepared_with_entry(
            &mut phase,
            &PostAuthority::Current(role),
            PostTarget {
                view: &self.view,
                expected_identity: self.identity,
                navigation: &self.navigation,
                wide: &payload.wide,
                document: &document,
                epoch: &epoch,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                event_storage: None,
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                carrier:
                    super::super::managed_test_control::navigation_post::PostCarrier::Invocation,
            },
            &mut |actual, epoch, owner| {
                if self.retired.get() {
                    return false;
                }
                let Ok(mut state) = self.state.try_borrow_mut() else {
                    return false;
                };
                if !state.matches(owner, actual, epoch, true) {
                    return false;
                }
                let State {
                    binding,
                    calls,
                    pending_send,
                    fault,
                    ..
                } = &mut *state;
                self.event
                    .with_record(|event| {
                        calls.try_enter_prepared(
                            &invocation,
                            binding,
                            owner,
                            (pending_send.as_ref().map(|v| &v.record), event),
                            fault,
                        )
                    })
                    .ok()
                    .flatten()
                    .unwrap_or(false)
            },
        );
        match result {
            PostResult::NotAdmitted => self.defer_call(invocation),
            PostResult::Returned(native) => {
                invocation.native_returned()?;
                let mut state = self
                    .state
                    .try_borrow_mut()
                    .map_err(|_| failure("native return state borrowed"))?;
                if !state
                    .calls
                    .active_mut()
                    .is_some_and(|call| call.observe_native_return(id, &document))
                {
                    return Err(failure("native return lost its exact original record"));
                }
                drop(state);
                if native.is_err() {
                    return Err(failure("original native Post returned failure"));
                }
                self.wake();
                Ok(())
            }
            PostResult::AlreadyReturned | PostResult::OutcomeUnknown => {
                // Drop marks this SAME reusable backing Unproved, never Returned.
                drop(invocation);
                Err(failure("original invocation native outcome is unproved"))
            }
        }
    }
}
