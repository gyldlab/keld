//! The one existing pending event: protocol facts and actual DATA/native body
//! keep distinct receipts while sharing the original delivery/lease/caller.
use super::current::{Action, NativePosition};
use super::event::NativeEvent;
use super::invocation::CallPayload;
use super::post::PostResult;
use super::{OriginalBridge, RefCell, WvError, failure};
use crate::original_call::queue::OriginalInputCharge;
use crate::original_event::PendingOriginalEvent;
use crate::renderer_bridge::RendererBridgeOutcome;
use std::sync::mpsc::TrySendError;

pub(super) struct EventProtocol {
    pub(super) record: Option<PendingOriginalEvent>,
    pub(super) consumed_input: Option<OriginalInputCharge<CallPayload>>,
}
pub(super) struct EventOwner {
    pub(super) protocol: RefCell<EventProtocol>,
    native: NativePosition<NativeEvent>,
}
impl EventOwner {
    pub(super) fn new() -> Self {
        Self {
            protocol: RefCell::new(EventProtocol {
                record: None,
                consumed_input: None,
            }),
            native: NativePosition::new(),
        }
    }
    pub(super) fn is_empty(&self) -> bool {
        self.native.is_empty()
            && self
                .protocol
                .try_borrow()
                .is_ok_and(|p| p.record.is_none() && p.consumed_input.is_none())
    }
    /// Observe the actual retained prepared/entered event, with no copied
    /// occupancy authority. An attached body not yet prepared defers admission.
    pub(super) fn with_record<R>(
        &self,
        run: impl FnOnce(Option<&PendingOriginalEvent>) -> R,
    ) -> Result<Option<R>, WvError> {
        let protocol = self
            .protocol
            .try_borrow()
            .map_err(|_| failure("event occupancy record is borrowed"))?;
        if let Some(record) = protocol.record.as_ref() {
            return Ok(Some(run(Some(record))));
        }
        self.native.observe(|event| match event {
            None => Some(run(None)),
            Some(event) => event.prepared_record().map(|record| run(Some(record))),
        })
    }
    pub(super) fn attach(
        &self,
        pending: &mut Option<RendererBridgeOutcome>,
        bridge: &OriginalBridge,
    ) -> Result<bool, WvError> {
        if !self.is_empty() {
            return Ok(false);
        }
        self.native.attach_checked(pending, |input| {
            NativeEvent::attach(input, bridge.id, &bridge.source)
        })
    }
    pub(super) fn release(
        &self,
        navigation: u64,
        delivery: u32,
        pending: &mut Option<RendererBridgeOutcome>,
    ) -> Result<bool, WvError> {
        let mut accepted = false;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let mut observed = None;
        self.native.drive(|event| {
            let protocol = self
                .protocol
                .try_borrow()
                .map_err(|_| failure("event release protocol borrowed"))?;
            if event.observe_release(navigation, delivery, protocol.record.as_ref())? {
                drop(pending.take());
                accepted = true;
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                {
                    observed = Some(event.queued_close_snapshot(None));
                }
            }
            Ok(Action::Keep)
        })?;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if let Some(observed) = observed {
            // No protocol/native-position borrow survives this observation.
            observed.emit("release-observed", None);
        }
        Ok(accepted)
    }
    #[expect(
        clippy::too_many_lines,
        reason = "One event owner preserves ordered protocol, native-return and physical-storage settlement."
    )]
    pub(super) fn drive(&self, bridge: &OriginalBridge) -> Result<(), WvError> {
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        let mut disposed_observation = None;
        let native_disposed = self.native.drive(|event| {
            if let Some(request) = event.retirement_request(&bridge.close) {
                match bridge.endpoint.requests.try_send(request) {
                    Ok(()) => {
                        event.retirement_handed_off()?;
                        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                        event
                            .queued_close_snapshot(Some(bridge.close.actual_closed()))
                            .emit("retirement-handed-off", None);
                    }
                    Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => {
                        return Err(failure(
                            "event retirement endpoint disconnected; body retained",
                        ));
                    }
                }
            }
            if !bridge.retired.get() && event.document().is_current() && event.role().is_live() {
                let (epoch, eligible) = {
                    let nav = bridge
                        .navigation
                        .try_borrow()
                        .map_err(|_| failure("event prepare owner borrowed"))?;
                    let state = bridge
                        .state
                        .try_borrow()
                        .map_err(|_| failure("event prepare state borrowed"))?;
                    let epoch = state.epoch()?;
                    (
                        epoch,
                        state.matches(nav.document(), event.document(), &epoch, true)
                            && state.binding.role_current(event.role())
                            && !state.fault.is_faulted(),
                    )
                };
                if eligible {
                    // Formatting/allocation are pure Rust and paid before copy.
                    // The navigation guard is dropped before the actual native API.
                    {
                        let nav = bridge
                            .navigation
                            .try_borrow()
                            .map_err(|_| failure("event preparation owner changed"))?;
                        if self
                            .protocol
                            .try_borrow()
                            .map_err(|_| failure("event protocol borrowed"))?
                            .record
                            .is_none()
                        {
                            event.prepare_protocol(
                                nav.document(),
                                std::str::from_utf8(&epoch)
                                    .map_err(|_| failure("event epoch is not UTF8"))?,
                            )?;
                        }
                    }
                    let role = event.role().clone();
                    let delivery = event.delivery();
                    let result = event.post(
                        &bridge.view,
                        bridge.identity,
                        &bridge.navigation,
                        &mut |lease, epoch, owner, prepared| {
                            if bridge.retired.get() {
                                return false;
                            }
                            let Ok(state) = bridge.state.try_borrow() else {
                                return false;
                            };
                            if state.fault.is_faulted()
                                || !state.matches(owner, lease, epoch, true)
                                || !state.binding.role_current(&role)
                            {
                                return false;
                            }
                            let Ok(mut protocol) = self.protocol.try_borrow_mut() else {
                                return false;
                            };
                            if protocol.record.is_some()
                                || protocol.consumed_input.is_some()
                                || prepared.as_ref().is_none_or(|record| {
                                    record.id() != delivery
                                        || !record.lease().same_binding(lease)
                                        || record
                                            .original_role()
                                            .is_none_or(|original| !original.same_role(&role))
                                })
                            {
                                return false;
                            }
                            // Every fallible check ended. Move the exact prepared
                            // record once; the sole Post entry follows immediately.
                            protocol.record = prepared.take();
                            true
                        },
                    );
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    if let PostResult::Returned(native) = &result {
                        // Actual API has returned; the final entry fence is unchanged.
                        event
                            .queued_close_snapshot(Some(bridge.close.actual_closed()))
                            .emit("post-return", Some(native.is_ok()));
                    }
                    match result {
                        PostResult::Returned(native) => {
                            let mut protocol = self
                                .protocol
                                .try_borrow_mut()
                                .map_err(|_| failure("event native return protocol borrowed"))?;
                            let record = protocol.record.as_mut().ok_or_else(|| {
                                failure("event return lacks actual entered protocol")
                            })?;
                            if !record.observe_native_quiescence(
                                event.delivery(),
                                event.document(),
                                native.is_ok(),
                            ) {
                                return Err(failure(
                                    "event native return differs from retained lease",
                                ));
                            }
                            drop(protocol);
                            bridge.wake();
                            if native.is_err() {
                                return Err(failure("event Post returned possible-effect failure"));
                            }
                        }
                        PostResult::OutcomeUnknown => {
                            return Err(failure("event Post outcome is unproved"));
                        }
                        PostResult::NotAdmitted | PostResult::AlreadyReturned => {}
                    }
                }
            }
            if event.can_dispose(&bridge.close) {
                // The entered protocol owns the credential String prepaid on
                // this SAME DATA token. Dispose it before NativeEvent's final
                // storage field can signal release on another thread.
                let record = self
                    .protocol
                    .try_borrow_mut()
                    .map_err(|_| failure("event protocol disposal is borrowed"))?
                    .record
                    .take();
                drop(record);
                #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                {
                    disposed_observation =
                        Some(event.queued_close_snapshot(Some(bridge.close.actual_closed())));
                }
                Ok(Action::Dispose)
            } else {
                Ok(Action::Keep)
            }
        })?;
        #[cfg(not(all(feature = "profile-test-hooks", debug_assertions)))]
        let _ = native_disposed;
        #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
        if native_disposed && let Some(observed) = disposed_observation {
            // NativePosition returned only after the actual NativeEvent/token
            // Drop returned and its same custody became Empty.
            observed.emit("native-storage-dropped", None);
        }
        if self.native.is_empty() {
            let mut protocol = self
                .protocol
                .try_borrow_mut()
                .map_err(|_| failure("event storage cleanup borrowed"))?;
            // Empty can arise only after the native body passed its same logical
            // release + physical disposal predicate (or before any event exists).
            if protocol.record.is_some() {
                return Err(failure("event protocol outlived its charged native body"));
            }
            if protocol.consumed_input.is_some() {
                bridge
                    .state
                    .try_borrow_mut()
                    .map_err(|_| failure("event input account borrowed"))?
                    .calls
                    .release_input(&mut protocol.consumed_input)?;
            }
        }
        Ok(())
    }
}
