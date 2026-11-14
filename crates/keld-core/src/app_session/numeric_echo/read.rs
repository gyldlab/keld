//! A metadata-only read guard. The actual reservation remains in Pending while
//! the canonical reader and codec own their body buffers.
use super::body_scope::ReadScope;
use super::{
    Body, CallError, CorrelationId, ECHO_CHANNEL, FrameKind, HostAppError, IpcError,
    PrimaryCallerContext, PrimaryRouterHandle, ProgressFailure, State, accounting, app_detail,
    app_ipc, decode, same_caller,
};
use keld_ipc::ValidatedFrameHeader;

/// The actual final pre-allocation predicate, shared with bounded paired-stream
/// component controls. Caller numbers alone never select a pending native call.
pub(super) fn admit_record<S>(
    original: (&PrimaryCallerContext, CorrelationId),
    incoming: (&PrimaryCallerContext, &ValidatedFrameHeader),
    state: &mut State<S>,
    maximum: usize,
    covered: bool,
) -> Result<(), IpcError> {
    let (original_caller, correlation) = original;
    let (caller, header) = incoming;
    if correlation != header.corr() || !same_caller(original_caller, caller) {
        return Err(IpcError::Protocol {
            detail: "numeric body does not match the actual pending caller",
        });
    }
    if state.progress_failure().is_some() {
        return Err(IpcError::Protocol {
            detail: "numeric source progress is unproved",
        });
    }
    if header.len() as usize > maximum || !covered {
        return Err(IpcError::PayloadTooLarge);
    }
    if !state.begin_read() {
        return Err(IpcError::Protocol {
            detail: "numeric result body was already read or retired",
        });
    }
    Ok(())
}

pub(in crate::app_session) struct ReadGuard<'a> {
    handle: &'a PrimaryRouterHandle,
    caller: &'a PrimaryCallerContext,
    correlation: CorrelationId,
    active: bool,
}

impl PrimaryRouterHandle {
    pub(in crate::app_session) fn admit_numeric_echo_body<'a>(
        &'a self,
        caller: &'a PrimaryCallerContext,
        header: &ValidatedFrameHeader,
    ) -> Result<Option<ReadGuard<'a>>, IpcError> {
        if header.channel() != ECHO_CHANNEL
            || !matches!(header.kind(), FrameKind::Reply | FrameKind::Err)
        {
            return Ok(None);
        }
        let mut pending = self.pending_echo.lock().map_err(|_| IpcError::Protocol {
            detail: "numeric pending-call lock poisoned before body allocation",
        })?;
        let waiter = pending.as_mut().ok_or(IpcError::Protocol {
            detail: "numeric body has no original pending call",
        })?;
        let Some(numeric) = waiter.numeric.as_mut() else {
            return Ok(None);
        };
        let storage = numeric.state.storage().ok_or(IpcError::Protocol {
            detail: "numeric pending storage is absent",
        })?;
        let maximum = storage.maximum_payload_len();
        let covered = accounting::covers(storage, 2 * maximum);
        admit_record(
            (&numeric.caller, waiter.correlation),
            (caller, header),
            &mut numeric.state,
            maximum,
            covered,
        )?;
        Ok(Some(ReadGuard {
            handle: self,
            caller,
            correlation: header.corr(),
            active: true,
        }))
    }
}

impl ReadGuard<'_> {
    fn covers(&self, buffers: usize) -> Result<(), HostAppError> {
        let pending = self
            .handle
            .pending_echo
            .lock()
            .map_err(|_| app_detail("numeric Echo body", "pending-call lock poisoned"))?;
        let numeric = pending
            .as_ref()
            .filter(|waiter| waiter.correlation == self.correlation)
            .and_then(|waiter| waiter.numeric.as_ref())
            .filter(|numeric| same_caller(&numeric.caller, self.caller))
            .ok_or_else(|| {
                app_detail("numeric Echo body", "original pending record disappeared")
            })?;
        if let Some(failure) = numeric.state.progress_failure() {
            return Err(failure.into_error());
        }
        if numeric
            .state
            .storage()
            .is_some_and(|storage| accounting::covers(storage, buffers))
        {
            Ok(())
        } else {
            Err(app_detail(
                "numeric Echo body",
                "actual buffer capacity is not covered",
            ))
        }
    }

    pub(in crate::app_session) fn reply(self, payload: Vec<u8>) -> Result<(), HostAppError> {
        let scope = ReadScope {
            raw: payload,
            decoded: (),
            guard: self,
        };
        scope.guard.covers(scope.raw.capacity())?;
        scope.guard.complete(Body::Reply(scope.raw))
    }

    pub(in crate::app_session) fn error(self, payload: Vec<u8>) -> Result<(), HostAppError> {
        let mut scope = ReadScope {
            raw: payload,
            decoded: None::<CallError>,
            guard: self,
        };
        // This accepted numeric operation alone requires raw ERR<=4096: its two
        // strings plus postcard prefixes cannot fit the required native JSON
        // Error<=4096 if the raw body is already larger. Final JSON measurement
        // still applies; no global CallError/frame codec limit changes.
        scope.guard.covers(scope.raw.capacity().saturating_mul(2))?;
        scope.decoded =
            Some(decode(&scope.raw).map_err(|source| app_ipc("renderer Echo Err", &source))?);
        let error = scope
            .decoded
            .as_ref()
            .ok_or_else(|| app_detail("numeric Echo Err", "decoded result is absent"))?;
        let bytes = scope
            .raw
            .capacity()
            .checked_add(error.code.capacity())
            .and_then(|bytes| bytes.checked_add(error.message.capacity()))
            .ok_or_else(|| app_detail("numeric Echo Err", "buffer accounting overflow"))?;
        scope.guard.covers(bytes)?;
        let ReadScope {
            raw,
            decoded,
            guard,
        } = scope;
        drop(raw);
        let error =
            decoded.ok_or_else(|| app_detail("numeric Echo Err", "decoded result disappeared"))?;
        guard.complete(Body::Error(error))
    }

    fn complete(self, body: Body) -> Result<(), HostAppError> {
        let mut scope = ReadScope {
            raw: Some(body),
            decoded: (),
            guard: self,
        };
        let mut pending = scope
            .guard
            .handle
            .pending_echo
            .lock()
            .map_err(|_| app_detail("numeric Echo result", "pending-call lock poisoned"))?;
        let waiter = pending
            .as_mut()
            .filter(|waiter| waiter.correlation == scope.guard.correlation)
            .ok_or_else(|| app_detail("numeric Echo result", "original record disappeared"))?;
        let numeric = waiter
            .numeric
            .as_mut()
            .filter(|numeric| same_caller(&numeric.caller, scope.guard.caller))
            .ok_or_else(|| app_detail("numeric Echo result", "original caller disappeared"))?;
        numeric
            .state
            .finish(
                scope
                    .raw
                    .take()
                    .ok_or_else(|| app_detail("numeric Echo result", "read body disappeared"))?,
            )
            .map_err(|_| app_detail("numeric Echo result", "result read is not active"))?;
        scope.guard.active = false;
        numeric.notify(&waiter.reply);
        Ok(())
    }
}

impl Drop for ReadGuard<'_> {
    #[expect(
        clippy::collapsible_if,
        reason = "Preserve the explicit pending-lock scopes around actual read-end metadata and the intervening writer progress notification."
    )]
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        // ReadScope's actual raw/decoded fields drop before this guard. Record
        // only that existing read-return fact; no buffer/credit moves here.
        if let Ok(mut pending) = self.handle.pending_echo.lock() {
            if let Some(waiter) = pending
                .as_mut()
                .filter(|waiter| waiter.correlation == self.correlation)
            {
                if let Some(numeric) = waiter
                    .numeric
                    .as_mut()
                    .filter(|numeric| same_caller(&numeric.caller, self.caller))
                {
                    numeric
                        .state
                        .read_ended("application link ended before the renderer reply");
                }
            }
        }
        // Release Pending before the SAME writer's coalesced hint owner. A hint
        // grants no receipt; the dispatcher's existing100ms poll rechecks source.
        let failure = match self.handle.primary_writer.as_ref() {
            Some(writer) => writer
                .completed_source_progress()
                .err()
                .map(ProgressFailure::Writer),
            None => Some(ProgressFailure::MissingWriter),
        };
        if let Some(failure) = failure {
            if let Ok(mut pending) = self.handle.pending_echo.lock() {
                if let Some(waiter) = pending
                    .as_mut()
                    .filter(|waiter| waiter.correlation == self.correlation)
                {
                    if let Some(numeric) = waiter
                        .numeric
                        .as_mut()
                        .filter(|numeric| same_caller(&numeric.caller, self.caller))
                    {
                        numeric.state.record_progress_failure(failure);
                    }
                }
            }
        }
    }
}
