//! The Host outcome remains in its source Option until its exact native owner
//! has attached it. Busy/inconsistent delivery never drops move-only custody.
use super::control::ControlWork;
use super::current::{Action, CurrentOperation};
use super::numeric::{NumericWork, token};
use super::state::State;
use super::{OriginalBridge, WvError, failure};
use crate::renderer_bridge::RendererBridgeOutcome;
use crate::renderer_contract::CheckedNativeDocumentLease;

impl OriginalBridge {
    pub(in crate::webview2) fn deliver_on_ui(
        &self,
        pending: &mut Option<RendererBridgeOutcome>,
    ) -> Result<bool, WvError> {
        let result = self.deliver_retained(pending);
        if result.is_err() {
            self.fail("Core outcome could not complete its retained native transfer");
        }
        result
    }
    fn original_epoch(
        lease: &CheckedNativeDocumentLease,
        state: &State,
        owner: &crate::native_document::NativeDocumentOwner,
    ) -> Option<[u8; 32]> {
        (state.binding.current(owner) == Some(lease.0.stamp()))
            .then(|| state.epoch().ok())
            .flatten()
    }
    #[expect(
        clippy::too_many_lines,
        reason = "One closed outcome dispatch keeps the move-only input attached before every fallible transition."
    )]
    fn deliver_retained(
        &self,
        pending: &mut Option<RendererBridgeOutcome>,
    ) -> Result<bool, WvError> {
        let Some(outcome) = pending.as_ref() else {
            return Ok(false);
        };
        if matches!(outcome, RendererBridgeOutcome::ControlCapacity) {
            drop(pending.take());
            self.wake();
            return Ok(true);
        }
        if outcome.webview() != Some(self.id) {
            return Ok(false);
        }
        match outcome {
            RendererBridgeOutcome::Event { .. } => {
                let taken = self.event.attach(pending, self)?;
                if taken {
                    self.wake();
                }
                Ok(taken)
            }
            RendererBridgeOutcome::EventReleased {
                navigation,
                request,
                ..
            } => {
                let taken = self.event.release(*navigation, *request, pending)?;
                if taken {
                    self.wake();
                }
                Ok(taken)
            }
            RendererBridgeOutcome::Preflight {
                navigation,
                request,
                accepted,
                ..
            } => {
                let nav = self
                    .navigation
                    .try_borrow()
                    .map_err(|_| failure("preflight delivery owner borrowed"))?;
                let mut state = self
                    .state
                    .try_borrow_mut()
                    .map_err(|_| failure("preflight delivery state borrowed"))?;
                let State { binding, calls, .. } = &mut *state;
                let progress = calls.deliver_preflight(
                    *navigation,
                    *request,
                    *accepted,
                    binding,
                    nav.document(),
                )?;
                drop(pending.take());
                drop(state);
                drop(nav);
                // Actual accepted/refused outcome unblocks this retained queue.
                if progress.needs_service_wake {
                    self.wake();
                }
                self.wake();
                Ok(true)
            }
            RendererBridgeOutcome::SendResult {
                navigation,
                request,
                control,
                ..
            } => {
                if !self.current.is_empty() {
                    return Ok(false);
                }
                if control.is_none() {
                    return Err(failure("Send result lacks actual Core CONTROL storage"));
                }
                let nav = self
                    .navigation
                    .try_borrow()
                    .map_err(|_| failure("Send result native owner borrowed"))?;
                let mut state = self
                    .state
                    .try_borrow_mut()
                    .map_err(|_| failure("Send result state borrowed"))?;
                let send = state
                    .pending_send
                    .as_ref()
                    .filter(|s| s.record.matches_result(*navigation, *request))
                    .ok_or_else(|| failure("Send result has no exact original pending send"))?;
                let lease = send.record.lease().clone();
                let role = send.role.clone();
                let epoch = Self::original_epoch(&lease, &state, nav.document());
                let attached = self.current.attach(pending, |outcome| {
                    CurrentOperation::Control(ControlWork::received(
                        outcome,
                        lease,
                        role,
                        epoch,
                        state.pending_send.take(),
                    ))
                });
                drop(state);
                drop(nav);
                if attached {
                    self.wake();
                }
                Ok(attached)
            }
            RendererBridgeOutcome::RouteState {
                navigation,
                ready,
                role,
                control,
                ..
            } => {
                if !self.current.is_empty() {
                    return Ok(false);
                }
                if control.is_none() {
                    return Err(failure("route control lacks actual Core storage"));
                }
                let nav = self
                    .navigation
                    .try_borrow()
                    .map_err(|_| failure("route delivery owner borrowed"))?;
                let mut state = self
                    .state
                    .try_borrow_mut()
                    .map_err(|_| failure("route delivery state borrowed"))?;
                let current = nav.current_document().filter(|lease| {
                    u64::from(lease.stamp().observed_generation()) == *navigation
                        && state.binding.current(nav.document()) == Some(lease.0.stamp())
                });
                let Some(lease) = current else {
                    // This outcome never entered a native operation and targets
                    // an already-replaced document. Drop only its actual storage.
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    let observed = (self.id.0, *navigation, *ready);
                    drop(state);
                    drop(nav);
                    drop(pending.take());
                    #[cfg(all(feature = "profile-test-hooks", debug_assertions))]
                    super::super::managed_test_control::queued_close_observation::route_dropped(
                        observed.0, observed.1, observed.2,
                    );
                    return Ok(true);
                };
                let role = role.clone();
                let epoch = Some(state.epoch()?);
                if !state
                    .binding
                    .observe_route(nav.document(), *navigation, *ready, role.clone())
                {
                    drop(state);
                    drop(nav);
                    drop(pending.take());
                    return Ok(true);
                }
                let attached = self.current.attach(pending, |outcome| {
                    CurrentOperation::Control(ControlWork::received(
                        outcome, lease, role, epoch, None,
                    ))
                });
                drop(state);
                drop(nav);
                if attached {
                    self.wake();
                }
                Ok(attached)
            }
            RendererBridgeOutcome::Reply {
                navigation,
                request,
                ..
            }
            | RendererBridgeOutcome::Error {
                navigation,
                request,
                ..
            } => {
                if !self.current.is_empty() {
                    return Ok(false);
                }
                let (lease, role, epoch) = {
                    let nav = self
                        .navigation
                        .try_borrow()
                        .map_err(|_| failure("numeric result owner borrowed"))?;
                    let state = self
                        .state
                        .try_borrow()
                        .map_err(|_| failure("numeric result state borrowed"))?;
                    let (lease, role) = state
                        .calls
                        .numeric_original(self.id, *navigation, *request)
                        .ok_or_else(|| {
                            failure("numeric result lacks original pending correlation")
                        })?;
                    let storage = token(outcome)
                        .ok_or_else(|| failure("numeric result lacks original storage token"))?;
                    if !storage.matches_request(self.id, *navigation, *request, lease) {
                        return Err(failure(
                            "numeric result storage belongs to another original request",
                        ));
                    }
                    let role = role
                        .cloned()
                        .ok_or_else(|| failure("numeric result lacks captured original caller"))?;
                    (
                        lease.clone(),
                        role,
                        Self::original_epoch(lease, &state, nav.document()),
                    )
                };
                let attached = self.current.attach(pending, |outcome| {
                    CurrentOperation::Numeric(NumericWork::received(outcome, lease, role, epoch))
                });
                if attached {
                    // The same whole outcome is already attached before this
                    // fallible accounting observation. Failure retains it there.
                    self.current.drive(|operation| {
                        let CurrentOperation::Numeric(work) = operation else {
                            return Err(failure("attached numeric result changed operation"));
                        };
                        let (view, navigation, request) = work.correlation();
                        let storage = work
                            .token()
                            .ok_or_else(|| failure("attached numeric token disappeared"))?;
                        if !self
                            .state
                            .try_borrow_mut()
                            .map_err(|_| failure("numeric observation owner borrowed"))?
                            .calls
                            .observe_numeric_result(view, navigation, request, storage)
                        {
                            return Err(failure(
                                "numeric result differs from same pending storage witness",
                            ));
                        }
                        Ok(Action::Keep)
                    })?;
                    self.wake();
                }
                Ok(attached)
            }
            RendererBridgeOutcome::ControlCapacity => Ok(false),
        }
    }
}
