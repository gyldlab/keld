//! Same prepaid original-call transfer into the canonical shared queue.
use super::{
    CallPayload, CapturedInput, OriginalBridge, OriginalCredentialSlot, State, WvError, failure,
};
use crate::original_call::original_call_channel;
use crate::original_call::queue::{OriginalCallInput, OriginalCallQueue};
use crate::wv_link::AppPageCall;

impl OriginalBridge {
    pub(super) fn intake_call(
        &self,
        input: &mut CapturedInput,
        call: AppPageCall,
    ) -> Result<(), WvError> {
        let (lease, _, _) =
            self.input_context(input, &call.document, OriginalCredentialSlot::App, true)?;
        // The derived literal and second retained credential are prepaid before copy.
        self.pay_input(input, 256 + 32)?;
        let channel = original_call_channel(&call)?;
        let role = if channel.is_some() {
            Some(
                self.state
                    .try_borrow()
                    .map_err(|_| failure("effect role borrowed"))?
                    .binding
                    .route_role()
                    .cloned()
                    .ok_or_else(|| failure("effect has no captured original route"))?,
            )
        } else {
            None
        };
        let facts = input.facts(self)?;
        let (encoded, payload_bytes) = CallPayload::layout(&call, input.value_bytes)?;
        self.pay_input(
            input,
            payload_bytes
                .checked_sub(input.value_bytes)
                .ok_or_else(|| failure("call carrier prepayment underflow"))?,
        )?;
        let credential = call.document.clone();
        let mut queued = Some(OriginalCallInput {
            id: call.call,
            lease,
            payload: CallPayload::new(call, encoded, input.value_bytes),
            payload_bytes,
            credential,
            channel,
            role_witness: role,
            facts: Some(facts),
        });
        let required = OriginalCallQueue::prepaid_input_bytes(
            queued
                .as_ref()
                .ok_or_else(|| failure("prepared local input disappeared"))?,
        )?;
        let paid = input
            .charge
            .as_ref()
            .ok_or_else(|| failure("local input token absent"))?
            .retained_bytes();
        if required > paid {
            self.pay_input(input, required - paid)?;
        }
        let result = {
            let nav = self
                .navigation
                .try_borrow()
                .map_err(|_| failure("local queue native owner borrowed"))?;
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| failure("local queue state borrowed"))?;
            let State {
                binding,
                calls,
                pending_send,
                fault,
                ..
            } = &mut *state;
            calls.enqueue_prepaid(
                &mut queued,
                &mut input.charge,
                binding,
                nav.document(),
                &self.endpoint.requests,
                pending_send.as_ref().map(|v| &v.record),
                fault,
            )
        };
        if result.is_err() && input.charge.is_none() {
            self.fail("admitted original call failed after owned transfer");
        }
        let progress = result?;
        if progress.needs_service_wake {
            self.wake();
        }
        self.wake();
        Ok(())
    }
}
