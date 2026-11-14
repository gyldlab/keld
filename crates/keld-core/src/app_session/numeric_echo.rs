//! The one existing numeric Echo record retains native result storage.
//! The existing reply channel carries only a notice for this Windows path;
//! actual bytes and the non-clone reservation stay here until the attached
//! dispatcher takes them. A stopped reader cannot erase a live body/decode.
pub(super) mod accounting;
mod admission;
mod body_scope;
mod read;
mod state;
#[cfg(test)]
mod tests;

use super::{
    APP_LINK_IO_DEADLINE, Arc, CallError, CorrelationId, ECHO_CHANNEL, FrameKind, HostAppError,
    Instant, IpcError, Ordering, PendingPrimaryEcho, PrimaryCallerContext, PrimaryEchoCall,
    PrimaryEchoReply, PrimaryRouterHandle, RendererBridgeOutcome, RendererBridgeRequest,
    SyncSender, TrySendError, app_detail, app_ipc, decode, mpsc, primary_writer,
    with_primary_caller_locked,
};
use keld_wv::renderer_bridge::NativeNumericResultReservation;
use state::State;

pub(super) struct Pending {
    caller: PrimaryCallerContext,
    state: State<NativeNumericResultReservation>,
}

/// This is the actual existing call receiver, plus its captured caller identity.
/// It is attached to the dispatcher before any admitted-write wait.
pub(super) struct Call {
    pub(super) inner: PrimaryEchoCall,
    pub(super) notice: Option<Result<PrimaryEchoReply, HostAppError>>,
    caller: PrimaryCallerContext,
}

#[derive(Clone, Copy)]
enum ProgressFailure {
    MissingWriter,
    Writer(primary_writer::WriterRefusal),
}
impl ProgressFailure {
    fn into_error(self) -> HostAppError {
        match self {
            Self::MissingWriter => {
                app_detail("numeric Echo progress", "original writer owner is absent")
            }
            Self::Writer(failure) => failure.into_host_error("numeric Echo progress"),
        }
    }
}

#[derive(Debug)]
enum Body {
    Reply(Vec<u8>),
    Error(CallError),
}

fn same_caller(a: &PrimaryCallerContext, b: &PrimaryCallerContext) -> bool {
    a.attempt == b.attempt
        && a.identity == b.identity
        && Arc::ptr_eq(&a.reader_stop, &b.reader_stop)
}

impl Pending {
    pub(super) fn retire(&mut self, detail: &'static str) {
        self.state.retire(detail);
    }

    pub(super) fn notify(&mut self, reply: &SyncSender<Result<PrimaryEchoReply, HostAppError>>) {
        self.state.send_notice(reply);
    }
}

impl PrimaryRouterHandle {
    /// Called by the existing unit `HostWriterProgress` service, outside all
    /// canonical/source turns. An aborted read has already dropped its body
    /// before its metadata guard marks `ReadEnded` and posts that existing hint.
    pub(in crate::app_session) fn service_numeric_echo_progress(&self) -> Result<(), HostAppError> {
        let mut pending = self
            .pending_echo
            .lock()
            .map_err(|_| app_detail("numeric Echo progress", "pending-call lock poisoned"))?;
        if let Some(waiter) = pending.as_mut()
            && let Some(numeric) = waiter.numeric.as_mut()
        {
            if let Some(failure) = numeric.state.progress_failure() {
                return Err(failure.into_error());
            }
            numeric.notify(&waiter.reply);
        }
        Ok(())
    }

    /// Actual pending source custody, including a failed notification, is not
    /// quiescent. This is only one term of the endpoint's aggregate drain gate.
    pub(in crate::app_session) fn numeric_echo_is_quiescent(&self) -> Result<bool, HostAppError> {
        let pending = self
            .pending_echo
            .lock()
            .map_err(|_| app_detail("numeric Echo drain", "pending-call lock poisoned"))?;
        Ok(pending
            .as_ref()
            .is_none_or(|waiter| waiter.numeric.is_none()))
    }

    /// Move only into the dispatcher's already-attached current outcome slot.
    /// Callers must drop the original call receiver before forwarding this
    /// outcome, so its per-call std channel backing no longer overlaps native
    /// JSON/UTF-16 encoding. A notice itself selects no current caller.
    #[expect(
        clippy::too_many_lines,
        reason = "One outcome transfer retains the same pending body and prepaid native token through final identity checks."
    )]
    pub(super) fn take_numeric_echo_outcome(
        &self,
        call: &Call,
        request: &RendererBridgeRequest,
        target: &mut Option<RendererBridgeOutcome>,
    ) -> Result<bool, HostAppError> {
        if target.is_some() {
            return Err(app_detail(
                "numeric Echo delivery",
                "delivery slot is occupied",
            ));
        }
        let failure = {
            let pending = self
                .pending_echo
                .lock()
                .map_err(|_| app_detail("numeric Echo delivery", "pending-call lock poisoned"))?;
            let Some(waiter) = pending.as_ref() else {
                return Ok(false);
            };
            let Some(numeric) = waiter.numeric.as_ref() else {
                return Ok(false);
            };
            if waiter.correlation != call.inner.correlation
                || !same_caller(&numeric.caller, &call.caller)
            {
                return Ok(false);
            }
            if let Some(failure) = numeric.state.progress_failure() {
                return Err(failure.into_error());
            }
            let failure = numeric.state.ready_failure();
            if let Some(detail) = failure {
                let buffers = "KELD-WV-011"
                    .len()
                    .checked_add(detail.len())
                    .ok_or_else(|| {
                        app_detail("numeric Echo delivery", "terminal accounting overflow")
                    })?;
                if !numeric
                    .state
                    .storage()
                    .is_some_and(|storage| accounting::covers(storage, buffers))
                {
                    return Err(app_detail(
                        "numeric Echo delivery",
                        "terminal is not prepaid",
                    ));
                }
            }
            failure
        };
        // These two bounded strings are allocated only after their same token
        // has prepaid them. Ready is immutable; the final lock repeats identity
        // and result-state checks. No native/current-generation lookup occurs.
        let mut fixed_error =
            failure.map(|detail| (String::from("KELD-WV-011"), String::from(detail)));
        let mut pending = self
            .pending_echo
            .lock()
            .map_err(|_| app_detail("numeric Echo delivery", "pending-call lock poisoned"))?;
        let Some(waiter) = pending.as_mut() else {
            return Ok(false);
        };
        let Some(numeric) = waiter.numeric.as_mut() else {
            return Ok(false);
        };
        if waiter.correlation != call.inner.correlation
            || !same_caller(&numeric.caller, &call.caller)
            || !numeric.state.is_ready()
        {
            return Ok(false);
        }
        if let Some(failure) = numeric.state.progress_failure() {
            return Err(failure.into_error());
        }
        if numeric.state.ready_failure() != failure {
            // A read may have settled between the two locks. Nothing has moved;
            // the next ordinary poll prepares that exact result's fixed strings.
            return Ok(false);
        }
        let Some(lease) = request.document_lease() else {
            return Err(app_detail(
                "numeric Echo delivery",
                "original document lease is absent",
            ));
        };
        if !numeric.state.storage().is_some_and(|storage| {
            storage.matches_request(
                request.webview(),
                request.navigation(),
                request.request(),
                lease,
            )
        }) {
            return Err(app_detail(
                "numeric Echo delivery",
                "original result storage does not match",
            ));
        }
        let fixed_capacity = fixed_error.as_ref().map_or(0, |(code, detail)| {
            code.capacity().saturating_add(detail.capacity())
        });
        if !numeric
            .state
            .storage()
            .is_some_and(|storage| accounting::covers(storage, fixed_capacity))
        {
            return Err(app_detail(
                "numeric Echo delivery",
                "terminal storage is not covered",
            ));
        }
        let Some((body, storage)) = numeric.state.take_ready() else {
            return Ok(false);
        };
        // Only infallible moves follow take until the exact current-delivery
        // owner holds every buffer and the same token. Native work happens later.
        let outcome = match body {
            Ok(Body::Reply(payload)) => RendererBridgeOutcome::Reply {
                webview: request.webview(),
                navigation: request.navigation(),
                request: request.request(),
                payload,
                numeric_result: Some(storage),
            },
            Ok(Body::Error(error)) => RendererBridgeOutcome::Error {
                webview: request.webview(),
                navigation: request.navigation(),
                request: request.request(),
                code: error.code,
                detail: error.message,
                control: None,
                numeric_result: Some(storage),
            },
            body @ Err(_) => {
                // The pair was checked before take and is now moved, not built.
                let Some((code, detail)) = fixed_error.take() else {
                    // Defensive invariant refusal restores the SAME source before
                    // allocating an error. It never fabricates an empty terminal.
                    numeric.state.restore_ready(body, storage);
                    return Err(app_detail(
                        "numeric Echo delivery",
                        "prepared terminal disappeared",
                    ));
                };
                RendererBridgeOutcome::Error {
                    webview: request.webview(),
                    navigation: request.navigation(),
                    request: request.request(),
                    code,
                    detail,
                    control: None,
                    numeric_result: Some(storage),
                }
            }
        };
        *target = Some(outcome);
        self.pending_echo_corr.store(0, Ordering::Release);
        self.pending_echo_attempt.store(0, Ordering::Release);
        let retired = pending.take();
        drop(pending);
        drop(retired);
        Ok(true)
    }
}
