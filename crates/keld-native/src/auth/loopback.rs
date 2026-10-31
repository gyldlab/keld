//! Private host-owned loopback callback transport for approved KEL-89/T3.
//! Called only after guard Allow by the one existing Native auth worker.
//! Owns no worker, queue, provider policy, registry or refresh credential.

#![forbid(unsafe_code)]

use std::fmt;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::primitives::{
    CorrelatedAuthorization, LoopbackRedirect, MAX_OAUTH_TEXT_BYTES, OutgoingText,
    PendingAuthorization, PrimitiveError, RedirectPlan,
};
use super::{ProviderProfile, RedirectForm};

/// Fixed local classifications; AUTH-016 requires the same reviewed local contract.
enum Failure {
    Bind,
    Deadline,
    Cancelled,
    Io,
    Consumed,
    Primitive(PrimitiveError),
}
/// Redacted flow failure. A failed page write remains separately visible.
pub(super) struct LoopbackError {
    failure: Failure,
    reply_failed: bool,
    delivery_stopped: Option<Stop>,
}
#[derive(Clone, Copy, Debug)]
enum Stop {
    Cancelled,
    Deadline,
}
impl LoopbackError {
    fn new(failure: Failure) -> Self {
        Self {
            failure,
            reply_failed: false,
            delivery_stopped: None,
        }
    }
    /// Public error envelope code; no callback value is used to select it.
    pub(super) fn code(&self) -> &'static str {
        match &self.failure {
            Failure::Bind => "KELD-AUTH-004",
            Failure::Deadline => "KELD-AUTH-008",
            Failure::Cancelled => "KELD-AUTH-007",
            Failure::Io | Failure::Consumed => "KELD-AUTH-016",
            Failure::Primitive(error) => error.code(),
        }
    }
}
impl From<PrimitiveError> for LoopbackError {
    fn from(error: PrimitiveError) -> Self {
        Self::new(Failure::Primitive(error))
    }
}
impl fmt::Debug for LoopbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoopbackError")
            .field("code", &self.code())
            .field("reply_failed", &self.reply_failed)
            .field("delivery_stopped", &self.delivery_stopped)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for LoopbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.failure {
            Failure::Bind => f.write_str("KELD-AUTH-004: the host could not bind the loopback listener. Free the configured fixed port or correct the local socket configuration, then start a fresh user-initiated auth.begin.")?,
            Failure::Deadline => f.write_str("KELD-AUTH-008: authorization did not complete within the absolute host bound. Start a fresh user-initiated auth.begin; no automatic retry occurs.")?,
            Failure::Cancelled => f.write_str("KELD-AUTH-007: the owning authorization flow was cancelled. Start a fresh user-initiated auth.begin; no automatic retry occurs.")?,
            Failure::Io => f.write_str("KELD-AUTH-016: host loopback processing failed during socket I/O. Restore the local socket facility, then start a fresh user-initiated auth.begin.")?,
            Failure::Consumed => f.write_str("KELD-AUTH-016: the host consumed the loopback flow more than once. Correct the host flow lifecycle before starting a fresh sign-in.")?,
            Failure::Primitive(error) => fmt::Display::fmt(error, f)?,
        }
        match self.delivery_stopped {
            Some(Stop::Cancelled) => {
                f.write_str(" The owning flow was cancelled before completion-page delivery.")?;
            }
            Some(Stop::Deadline) => f.write_str(
                " Completion-page delivery exceeded the same absolute authorization bound.",
            )?,
            None => {}
        }
        if self.reply_failed {
            f.write_str(" The local browser completion page could not be delivered.")?;
        }
        Ok(())
    }
}
impl std::error::Error for LoopbackError {}

/// No result is exposed while the accepted reply socket is still owned.
pub(super) enum LoopbackProgress {
    Pending,
    Complete(Result<CorrelatedAuthorization, LoopbackError>),
}
struct Reply {
    bytes: Vec<u8>,
    written: usize,
    result: Option<Result<CorrelatedAuthorization, LoopbackError>>,
}
struct Accepted {
    stream: TcpStream,
    request: Vec<u8>,
    reply: Option<Reply>,
}
impl Drop for Accepted {
    fn drop(&mut self) {
        self.request.fill(0);
    }
}

/// One bound, single-use callback flow, polled by the existing auth worker.
pub(super) struct LoopbackFlow {
    listener: Option<TcpListener>,
    accepted: Option<Accepted>,
    // One additional never-started peer. Only accepted may own request bytes.
    idle: Option<Accepted>,
    pending: Option<PendingAuthorization>,
    redirect: LoopbackRedirect,
    bound_port: u16,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    consumed: bool,
}
impl fmt::Debug for LoopbackFlow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoopbackFlow")
            .field("listener_live", &self.listener.is_some())
            .field("peer_live", &self.accepted.is_some())
            .field("idle_peer_live", &self.idle.is_some())
            .field("consumed", &self.consumed)
            .finish_non_exhaustive()
    }
}
impl LoopbackFlow {
    /// Bind only the loopback address, requesting the plan's exact port.
    /// Validation of fixed mode is owned by `RedirectPlan` before this call.
    pub(super) fn bind(
        plan: RedirectPlan,
        pending: PendingAuthorization,
        deadline: Instant,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, LoopbackError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(LoopbackError::new(Failure::Cancelled));
        }
        if Instant::now() >= deadline {
            return Err(LoopbackError::new(Failure::Deadline));
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, plan.bind_port()))
            .map_err(|_| LoopbackError::new(Failure::Bind))?;
        listener
            .set_nonblocking(true)
            .map_err(|_| LoopbackError::new(Failure::Io))?;
        let bound_port = listener
            .local_addr()
            .map_err(|_| LoopbackError::new(Failure::Io))?
            .port();
        let redirect = plan.complete(bound_port)?;
        Ok(Self {
            listener: Some(listener),
            accepted: None,
            idle: None,
            pending: Some(pending),
            redirect,
            bound_port,
            deadline,
            cancelled,
            consumed: false,
        })
    }
    /// Exact raw URI shared by authorization and code-exchange form owners.
    pub(super) fn redirect(&self) -> &LoopbackRedirect {
        &self.redirect
    }
    /// Construct the existing primitive's URL without cloning pending secrets.
    pub(super) fn authorization_url(
        &self,
        profile: &ProviderProfile,
        client_id: &str,
        scopes: Option<&[String]>,
        hint: Option<&str>,
    ) -> Result<OutgoingText, LoopbackError> {
        let pending = self
            .pending
            .as_ref()
            .ok_or_else(|| LoopbackError::new(Failure::Consumed))?;
        Ok(pending.authorization_url(profile, client_id, &self.redirect, scopes, hint)?)
    }
    /// Cancellation applies to this entire pending flow, not a token waiter.
    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    fn finish(
        &mut self,
        result: Result<CorrelatedAuthorization, LoopbackError>,
    ) -> LoopbackProgress {
        self.listener.take();
        self.accepted.take();
        self.idle.take();
        self.pending.take();
        self.consumed = true;
        LoopbackProgress::Complete(result)
    }
    fn stop(&mut self, stop: Stop) -> LoopbackProgress {
        let prior = self
            .accepted
            .as_mut()
            .and_then(|accepted| accepted.reply.as_mut())
            .and_then(|reply| reply.result.take());
        let mut error = match prior {
            Some(Err(error)) => error,
            _ => LoopbackError::new(match stop {
                Stop::Cancelled => Failure::Cancelled,
                Stop::Deadline => Failure::Deadline,
            }),
        };
        error.delivery_stopped = Some(stop);
        self.finish(Err(error))
    }
    fn stop_requested(&mut self) -> Option<LoopbackProgress> {
        if self.cancelled.load(Ordering::Acquire) {
            Some(self.stop(Stop::Cancelled))
        } else if Instant::now() >= self.deadline {
            Some(self.stop(Stop::Deadline))
        } else {
            None
        }
    }
    /// At most two retained peers, one accept and three bounded reads per poll;
    /// a reply poll performs one nonblocking write. The extra peer never owns
    /// request bytes. Absolute clock/cancel checks precede every I/O phase.
    pub(super) fn poll(&mut self) -> LoopbackProgress {
        if self.consumed {
            return LoopbackProgress::Complete(Err(LoopbackError::new(Failure::Consumed)));
        }
        if let Some(progress) = self.stop_requested() {
            return progress;
        }
        let mut chunk = [0u8; 1024];
        if self.accepted.is_none() {
            return self.poll_accept(&mut chunk);
        }
        if self
            .accepted
            .as_ref()
            .is_some_and(|accepted| accepted.reply.is_some())
        {
            return self.poll_reply();
        }
        let read = match self.accepted.as_mut() {
            Some(accepted) => read_peer(accepted, &mut chunk),
            None => Err(LoopbackError::new(Failure::Io)),
        };
        match read {
            Ok(Some(count)) => {
                self.idle.take(); // First bytes commit the existing sole callback.
                return self.receive_chunk(&chunk[..count]);
            }
            Ok(None) => {}
            Err(error) => return self.finish(Err(error)),
        }
        if self
            .accepted
            .as_ref()
            .is_some_and(|peer| !peer.request.is_empty())
        {
            // Never evict or bypass a started/truncated/malformed callback.
            return LoopbackProgress::Pending;
        }
        if self.idle.is_some() {
            if let Some(progress) = self.stop_requested() {
                return progress;
            }
            let read = match self.idle.as_mut() {
                Some(idle) => read_peer(idle, &mut chunk),
                None => Err(LoopbackError::new(Failure::Io)),
            };
            match read {
                Ok(Some(count)) => {
                    self.accepted = self.idle.take();
                    return self.receive_chunk(&chunk[..count]);
                }
                Ok(None) => {}
                Err(error) => return self.finish(Err(error)),
            }
        }
        if let Some(progress) = self.stop_requested() {
            return progress;
        }
        self.poll_accept(&mut chunk)
    }
    fn receive_chunk(&mut self, chunk: &[u8]) -> LoopbackProgress {
        let Some(accepted) = self.accepted.as_mut() else {
            return self.finish(Err(LoopbackError::new(Failure::Io)));
        };
        let count = chunk.len();
        if count > MAX_OAUTH_TEXT_BYTES.saturating_sub(accepted.request.len()) {
            return self.finish(Err(PrimitiveError::InputLimit.into()));
        }
        let scan_from = accepted.request.len().saturating_sub(3);
        accepted.request.extend_from_slice(&chunk[..count]);
        let Some(end) = accepted.request[scan_from..]
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .map(|end| scan_from + end + 4)
        else {
            return LoopbackProgress::Pending;
        };
        let result = match request_query(&accepted.request, end, &self.redirect, self.bound_port) {
            Ok(query) => match self.pending.take() {
                Some(pending) => pending
                    .correlate_callback(query)
                    .map_err(LoopbackError::from),
                None => Err(LoopbackError::new(Failure::Consumed)),
            },
            Err(error) => Err(error.into()),
        };
        self.pending.take();
        self.listener.take(); // No second callback/replay admission after decision.
        self.idle.take();
        let (status, body) = if result.is_ok() {
            ("200 OK", "Sign-in received. Return to the application.\n")
        } else {
            (
                "400 Bad Request",
                "Sign-in could not be completed. Return to the application.\n",
            )
        };
        let bytes = format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes();
        accepted.request.fill(0);
        accepted.request.clear();
        accepted.reply = Some(Reply {
            bytes,
            written: 0,
            result: Some(result),
        });
        LoopbackProgress::Pending
    }
    fn poll_accept(&mut self, chunk: &mut [u8; 1024]) -> LoopbackProgress {
        let Some(listener) = &self.listener else {
            return self.finish(Err(LoopbackError::new(Failure::Io)));
        };
        match listener.accept() {
            Ok((stream, peer)) => {
                if !peer.ip().is_loopback() {
                    return self.finish(Err(PrimitiveError::Correlation.into()));
                }
                if stream.set_nonblocking(true).is_err() {
                    return self.finish(Err(LoopbackError::new(Failure::Io)));
                }
                let incoming = Accepted {
                    stream,
                    request: Vec::new(),
                    reply: None,
                };
                if self.accepted.is_none() {
                    self.accepted = Some(incoming);
                } else if self.idle.is_none() {
                    self.idle = Some(incoming);
                } else {
                    // Both retained peers were observed empty before this accept.
                    // Recheck oldest before recycling; newly arrived bytes win.
                    if let Some(progress) = self.stop_requested() {
                        return progress;
                    }
                    let read = match self.accepted.as_mut() {
                        Some(accepted) => read_peer(accepted, chunk),
                        None => Err(LoopbackError::new(Failure::Io)),
                    };
                    match read {
                        Ok(Some(count)) => {
                            self.idle.take();
                            return self.receive_chunk(&chunk[..count]);
                        }
                        Ok(None) => {
                            self.accepted = self.idle.take();
                            self.idle = Some(incoming);
                        }
                        Err(error) => return self.finish(Err(error)),
                    }
                }
                LoopbackProgress::Pending
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                LoopbackProgress::Pending
            }
            Err(_) => self.finish(Err(LoopbackError::new(Failure::Io))),
        }
    }
    fn poll_reply(&mut self) -> LoopbackProgress {
        let Some(accepted) = self.accepted.as_mut() else {
            return self.finish(Err(LoopbackError::new(Failure::Io)));
        };
        let Some(reply) = &mut accepted.reply else {
            return self.finish(Err(LoopbackError::new(Failure::Io)));
        };
        match accepted.stream.write(&reply.bytes[reply.written..]) {
            Ok(0) => {
                let result = page_failure(reply.result.take());
                return self.finish(result);
            }
            Ok(count) => reply.written += count,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                return LoopbackProgress::Pending;
            }
            Err(_) => {
                let result = page_failure(reply.result.take());
                return self.finish(result);
            }
        }
        if reply.written == reply.bytes.len() {
            let result = reply
                .result
                .take()
                .unwrap_or_else(|| Err(LoopbackError::new(Failure::Consumed)));
            return self.finish(result);
        }
        LoopbackProgress::Pending
    }
}
// A single safe nonblocking read; EOF/error classification matches the
// existing whole-flow policy, including zero-byte EOF. No request copy here.
fn read_peer(
    accepted: &mut Accepted,
    chunk: &mut [u8; 1024],
) -> Result<Option<usize>, LoopbackError> {
    match accepted.stream.read(chunk) {
        Ok(0) => Err(PrimitiveError::Correlation.into()),
        Ok(count) => Ok(Some(count)),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(None)
        }
        Err(_) => Err(LoopbackError::new(Failure::Io)),
    }
}
fn page_failure(
    result: Option<Result<CorrelatedAuthorization, LoopbackError>>,
) -> Result<CorrelatedAuthorization, LoopbackError> {
    let mut error = match result {
        Some(Err(error)) => error,
        _ => LoopbackError::new(Failure::Io),
    };
    error.reply_failed = true;
    Err(error)
}
fn request_query<'a>(
    request: &'a [u8],
    end: usize,
    redirect: &LoopbackRedirect,
    bound_port: u16,
) -> Result<&'a str, PrimitiveError> {
    if end != request.len() {
        return Err(PrimitiveError::Correlation);
    } // No body or pipelined request.
    let text = std::str::from_utf8(request).map_err(|_| PrimitiveError::Correlation)?;
    let mut lines = text[..end - 4].split("\r\n");
    let mut words = lines.next().ok_or(PrimitiveError::Correlation)?.split(' ');
    if words.next() != Some("GET") {
        return Err(PrimitiveError::Correlation);
    }
    let target = words.next().ok_or(PrimitiveError::Correlation)?;
    if words.next() != Some("HTTP/1.1")
        || words.next().is_some()
        || target.bytes().any(|byte| byte <= b' ' || byte >= 0x7f)
    {
        return Err(PrimitiveError::Correlation);
    }
    let host = match redirect.host() {
        RedirectForm::Localhost => "localhost",
        RedirectForm::Ipv4Loopback => "127.0.0.1",
    };
    let authority = format!("{host}:{bound_port}");
    let mut host_seen = false;
    let mut length_seen = false;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(PrimitiveError::Correlation)?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
            || value
                .bytes()
                .any(|byte| (byte < b' ' && byte != b'\t') || byte >= 0x7f)
        {
            return Err(PrimitiveError::Correlation);
        }
        let value = value.trim_matches([' ', '\t']);
        if name.eq_ignore_ascii_case("Host") {
            if host_seen
                || !(value.eq_ignore_ascii_case(&authority)
                    || (bound_port == 80 && value.eq_ignore_ascii_case(host)))
            {
                return Err(PrimitiveError::Correlation);
            }
            host_seen = true;
        } else if name.eq_ignore_ascii_case("Content-Length") {
            if length_seen || value != "0" {
                return Err(PrimitiveError::Correlation);
            }
            length_seen = true;
        } else if name.eq_ignore_ascii_case("Transfer-Encoding") {
            return Err(PrimitiveError::Correlation);
        }
    }
    if !host_seen {
        return Err(PrimitiveError::Correlation);
    }
    redirect.callback_query(target)
}

#[cfg(test)]
#[path = "loopback_tests.rs"]
mod tests;
