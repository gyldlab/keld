//! Private macOS token POST transport for KEL-89/T3.
//! Call only after the owning auth broker's guard admission. This module owns
//! HTTP handles/callback custody, not provider policy, JSON/token interpretation,
//! account/session liveness or single-flight admission.
#![allow(unsafe_code)] // Exact KEL-89 path/operation sanction: crates/keld-native/AGENTS.md.
#![deny(unsafe_op_in_unsafe_fn)]

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::time::{Instant, SystemTime};

use block2::DynBlock;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_foundation::{
    NSData, NSError, NSHTTPURLResponse, NSMutableURLRequest, NSObject, NSObjectProtocol, NSString,
    NSURL, NSURLRequest, NSURLResponse, NSURLSession, NSURLSessionConfiguration,
    NSURLSessionDataDelegate, NSURLSessionDataTask, NSURLSessionDelegate,
    NSURLSessionResponseDisposition, NSURLSessionTask, NSURLSessionTaskDelegate,
};

use super::provider_profile;

// Reuse the existing published control-plane byte ceiling. These cap each
// owned request/response, not the aggregate number of broker-admitted exchanges.
const BODY_LIMIT: usize = keld_ipc::MAX_FRAME_LEN;

/// Absolute bounds supplied once by the owning cold broker; never renewed.
#[derive(Clone, Copy, Debug)]
pub(super) struct HttpDeadlines {
    pub(super) request: Instant,
    pub(super) cleanup: Instant,
}

/// Fixed transport classifications; no endpoint, body or provider text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HttpFailure {
    Endpoint,
    RequestTooLarge,
    Deadline,
    Cancelled,
    Redirect,
    ResponseTooLarge,
    UnsupportedResponse,
    Network(isize),
    StatePoisoned,
    StateContended,
    CallbackProtocol,
    AlreadyConsumed,
}

/// Cleanup faults stay separate from the original exchange failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HttpCleanupFailure {
    CallbackProtocol,
    QuiescenceUnproven,
    ProtocolAndQuiescence,
}

/// Redacted, typed exchange/cleanup failure for the owning broker to attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HttpError {
    pub(super) primary: HttpFailure,
    pub(super) cleanup: Option<HttpCleanupFailure>,
    pub(super) platform_code: Option<isize>,
}

impl HttpError {
    fn new(primary: HttpFailure) -> Self {
        Self {
            primary,
            cleanup: None,
            platform_code: None,
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let condition = match self.primary {
            HttpFailure::Endpoint => "the host token endpoint configuration is invalid",
            HttpFailure::RequestTooLarge => "the token request exceeds the host byte bound",
            HttpFailure::Deadline => "the token exchange exceeded its absolute host deadline",
            HttpFailure::Cancelled => "the owning token exchange was cancelled",
            HttpFailure::Redirect => "the token endpoint attempted an unsupported redirect",
            HttpFailure::ResponseTooLarge => "the token response exceeds the host byte bound",
            HttpFailure::UnsupportedResponse => {
                "the token endpoint returned an unsupported response"
            }
            HttpFailure::Network(_) => "macOS networking could not complete the token exchange",
            HttpFailure::StatePoisoned => "the host token callback state is inconsistent",
            HttpFailure::StateContended => "the token callback state was unavailable at cleanup",
            HttpFailure::CallbackProtocol => "the token callback sequence is inconsistent",
            HttpFailure::AlreadyConsumed => "the host consumed the token exchange more than once",
        };
        write!(
            f,
            "KELD-AUTH-013: token exchange transport failed because {condition}. Start a new user-initiated sign-in; no automatic retry occurs."
        )?;
        if self.cleanup.is_some() {
            write!(
                f,
                " Callback cleanup is unproven; keep the owner through fatal cleanup and relaunch the host."
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for HttpError {}

/// Host-only provider response; OAuth `error/invalid_grant` interpretation is broker-owned.
pub(super) struct HttpResponse {
    status: u16,
    body: Vec<u8>,
    received_at: SystemTime,
    complete_at: Instant,
}
impl HttpResponse {
    pub(super) fn status(&self) -> u16 {
        self.status
    }
    pub(super) fn body(&self) -> &[u8] {
        &self.body
    }
    pub(super) fn received_at(&self) -> SystemTime {
        self.received_at
    }
    /// Original completion-callback monotonic receipt; cleanup never renews it.
    pub(super) fn receipt_mono(&self) -> Instant {
        self.complete_at
    }
}
impl fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .field("body_bytes", &self.body.len())
            .finish_non_exhaustive()
    }
}
impl Drop for HttpResponse {
    fn drop(&mut self) {
        self.body.fill(0);
    }
}

/// `CleanupUnproven` is NOT a completed operation: retain/poll this owner while
/// the broker closes admission and follows its fatal-cleanup path.
#[derive(Debug)]
pub(super) enum HttpProgress {
    Pending,
    Complete(Result<HttpResponse, HttpError>),
    CleanupUnproven(HttpError),
}

#[derive(Default)]
struct ResponseState {
    status: Option<u16>,
    body: Vec<u8>,
    received_at: Option<SystemTime>,
    platform_code: Option<isize>,
    complete_at: Option<Instant>,
    invalidated: bool,
    redirect_seen: bool,
    primary: Option<HttpFailure>,
    cleanup: Option<HttpCleanupFailure>,
}
impl ResponseState {
    fn note_cleanup(&mut self, cleanup: HttpCleanupFailure) {
        self.cleanup = Some(match (self.cleanup, cleanup) {
            (None, next) => next,
            (Some(previous), next) if previous == next => previous,
            _ => HttpCleanupFailure::ProtocolAndQuiescence,
        });
    }
    fn remaining_body_bytes(&self) -> usize {
        BODY_LIMIT.saturating_sub(self.body.len())
    }
    fn append_body(&mut self, bytes: &[u8]) -> bool {
        if bytes.len() > self.remaining_body_bytes() {
            self.fail(HttpFailure::ResponseTooLarge);
            false
        } else {
            self.body.extend_from_slice(bytes);
            true
        }
    }
    fn fail(&mut self, error: HttpFailure) {
        if self.primary.is_none() {
            self.primary = Some(error);
        }
        self.body.fill(0);
        self.body.clear();
    }
    fn protocol_fault(&mut self) {
        self.note_cleanup(HttpCleanupFailure::CallbackProtocol);
        self.fail(HttpFailure::CallbackProtocol);
    }
}

struct Shared {
    #[cfg(test)]
    body_copy_calls: std::sync::atomic::AtomicUsize,
    state: Mutex<ResponseState>,
    deadlines: HttpDeadlines,
    cancelled: Arc<AtomicBool>,
    delegate_dropped: AtomicBool,
    invalidating: AtomicBool,
}
impl Shared {
    fn lock(&self) -> MutexGuard<'_, ResponseState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.note_cleanup(HttpCleanupFailure::CallbackProtocol);
                state.fail(HttpFailure::StatePoisoned);
                state
            }
        }
    }
    fn try_lock(&self) -> Option<MutexGuard<'_, ResponseState>> {
        match self.state.try_lock() {
            Ok(state) => Some(state),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(poisoned)) => {
                let mut state = poisoned.into_inner();
                state.note_cleanup(HttpCleanupFailure::CallbackProtocol);
                state.fail(HttpFailure::StatePoisoned);
                Some(state)
            }
        }
    }
}
impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpCallbackState")
            .field("cancelled", &self.cancelled.load(Ordering::Acquire))
            .field(
                "delegate_dropped",
                &self.delegate_dropped.load(Ordering::Acquire),
            )
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
struct DelegateIvars {
    shared: Arc<Shared>,
}
impl Drop for DelegateIvars {
    fn drop(&mut self) {
        self.shared.delegate_dropped.store(true, Ordering::Release);
    }
}

define_class!(
    // SAFETY: Final exact NSObject subclass, initialized Rust ivars and no
    // class Drop. NSObject has no additional subclassing requirements.
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[name = "KeldAuthHttpDelegate"]
    #[ivars = DelegateIvars]
    #[derive(Debug)]
    struct AuthHttpDelegate;

    // SAFETY: Initialized NSObject subclass; protocol has no extra requirements.
    unsafe impl NSObjectProtocol for AuthHttpDelegate {}

    // SAFETY: Exact pinned protocol ABI; only synchronized Rust ivars, no
    // retained UI/foreign pointer. Auto traits checked below, no manual Send/Sync.
    unsafe impl NSURLSessionDelegate for AuthHttpDelegate {
        // SAFETY: Exact SDK selector, borrowed session and nullable NSError.
        #[unsafe(method(URLSession:didBecomeInvalidWithError:))]
        fn invalidated(&self, _session: &NSURLSession, error: Option<&NSError>) {
            autoreleasepool(|_| {
                let mut state = self.ivars().shared.lock();
                if state.invalidated || state.complete_at.is_none() || error.is_some() {
                    state.protocol_fault();
                }
                state.invalidated = true;
            });
        }
    }

    // SAFETY: Exact task-delegate ABI; SDK borrows never escape callbacks.
    unsafe impl NSURLSessionTaskDelegate for AuthHttpDelegate {
        // SAFETY: SDK block is sendable and borrowed live on its serial queue;
        // call once immediately with nil, never retain/move/forward request.
        #[unsafe(method(URLSession:task:willPerformHTTPRedirection:newRequest:completionHandler:))]
        fn redirect(
            &self,
            _session: &NSURLSession,
            _task: &NSURLSessionTask,
            _response: &NSHTTPURLResponse,
            _request: &NSURLRequest,
            completion: &DynBlock<dyn Fn(*mut NSURLRequest)>,
        ) {
            autoreleasepool(|_| {
                {
                    let mut state = self.ivars().shared.lock();
                    if state.redirect_seen || state.complete_at.is_some() {
                        state.protocol_fault();
                    }
                    state.redirect_seen = true;
                    state.fail(HttpFailure::Redirect);
                }
                completion.call((std::ptr::null_mut(),));
            });
        }

        // SAFETY: Exact SDK selector and nullable error; no raw adoption.
        #[unsafe(method(URLSession:task:didCompleteWithError:))]
        fn complete(
            &self,
            session: &NSURLSession,
            _task: &NSURLSessionTask,
            error: Option<&NSError>,
        ) {
            autoreleasepool(|_| {
                let complete_at = Instant::now();
                {
                    let mut state = self.ivars().shared.lock();
                    if state.complete_at.is_some() {
                        state.protocol_fault();
                    } else {
                        state.complete_at = Some(complete_at);
                        state.received_at = Some(SystemTime::now());
                    }
                    if self.ivars().shared.cancelled.load(Ordering::Acquire) {
                        state.fail(HttpFailure::Cancelled);
                    } else if complete_at > self.ivars().shared.deadlines.request {
                        state.fail(HttpFailure::Deadline);
                    }
                    if let Some(error) = error {
                        state.platform_code = Some(error.code());
                        state.fail(HttpFailure::Network(error.code()));
                    }
                    if state.status.is_none() && state.primary.is_none() {
                        state.fail(HttpFailure::UnsupportedResponse);
                    }
                }
                // No state/root/UI guard is held across SDK invalidation.
                if !self
                    .ivars()
                    .shared
                    .invalidating
                    .swap(true, Ordering::AcqRel)
                {
                    session.finishTasksAndInvalidate();
                }
            });
        }
    }

    // SAFETY: Exact typed data-delegate callbacks, borrowed NSData/response;
    // bounds checked before safe Rust copy, no raw pointer retain/cast.
    unsafe impl NSURLSessionDataDelegate for AuthHttpDelegate {
        // SAFETY: Exact SDK response/block ABI; completion called once and
        // never saved/moved. Default TLS handling is not overridden.
        #[unsafe(method(URLSession:dataTask:didReceiveResponse:completionHandler:))]
        fn response(
            &self,
            _session: &NSURLSession,
            _task: &NSURLSessionDataTask,
            response: &NSURLResponse,
            completion: &DynBlock<dyn Fn(NSURLSessionResponseDisposition)>,
        ) {
            autoreleasepool(|_| {
                let disposition = {
                    let mut state = self.ivars().shared.lock();
                    if state.status.is_some() || state.complete_at.is_some() {
                        state.protocol_fault();
                    }
                    let status = response
                        .downcast_ref::<NSHTTPURLResponse>()
                        .and_then(|http| u16::try_from(http.statusCode()).ok())
                        .filter(|status| (100..=599).contains(status));
                    state.status = status;
                    if status.is_none() {
                        state.fail(HttpFailure::UnsupportedResponse);
                    }
                    if status.is_some_and(|status| (300..400).contains(&status)) {
                        state.fail(HttpFailure::Redirect);
                    }
                    let advertised = response.expectedContentLength();
                    if advertised >= 0
                        && usize::try_from(advertised).map_or(true, |length| length > BODY_LIMIT)
                    {
                        state.fail(HttpFailure::ResponseTooLarge);
                    }
                    if self.ivars().shared.cancelled.load(Ordering::Acquire) {
                        state.fail(HttpFailure::Cancelled);
                    }
                    if state.primary.is_some() {
                        NSURLSessionResponseDisposition::Cancel
                    } else {
                        NSURLSessionResponseDisposition::Allow
                    }
                };
                completion.call((disposition,));
            });
        }

        // SAFETY: NSData is borrowed for callback duration; use only safe
        // length/copy API, with fixed cumulative bound before allocation.
        #[unsafe(method(URLSession:dataTask:didReceiveData:))]
        fn data(&self, _session: &NSURLSession, task: &NSURLSessionDataTask, data: &NSData) {
            autoreleasepool(|_| {
                let mut cancel = false;
                {
                    let mut state = self.ivars().shared.lock();
                    if state.complete_at.is_some() || state.invalidated || state.status.is_none() {
                        state.protocol_fault();
                    }
                    if self.ivars().shared.cancelled.load(Ordering::Acquire) {
                        state.fail(HttpFailure::Cancelled);
                    }
                    if state.primary.is_none() {
                        if data.length() > state.remaining_body_bytes() {
                            state.fail(HttpFailure::ResponseTooLarge);
                            cancel = true;
                        } else {
                            #[cfg(test)]
                            self.ivars()
                                .shared
                                .body_copy_calls
                                .fetch_add(1, Ordering::Relaxed);
                            let mut chunk = data.to_vec();
                            if !state.append_body(&chunk) {
                                cancel = true;
                            }
                            chunk.fill(0);
                        }
                    }
                }
                if cancel {
                    task.cancel();
                }
            });
        }
    }
);

impl AuthHttpDelegate {
    fn new(shared: Arc<Shared>) -> Result<Retained<Self>, HttpError> {
        fn require_send_sync<T: Send + Sync>() {}
        require_send_sync::<Self>();
        require_send_sync::<DelegateIvars>();
        let allocated = Self::alloc().set_ivars(DelegateIvars { shared });
        // SAFETY: Own newly allocated exact final subclass with initialized
        // ivars; one NSObject super-init consumes allocation. Nil is typed.
        let initialized: Option<Retained<Self>> = unsafe { msg_send![super(allocated), init] };
        initialized.ok_or_else(|| HttpError::new(HttpFailure::CallbackProtocol))
    }
}

struct Owners {
    session: Retained<NSURLSession>,
    task: Retained<NSURLSessionDataTask>,
    _delegate: Retained<AuthHttpDelegate>,
}
impl Drop for Owners {
    fn drop(&mut self) {
        autoreleasepool(|_| {
            self.task.cancel();
            self.session.invalidateAndCancel();
        });
    }
}

/// One private token exchange. Actor polls are nonblocking; no Rust worker,
/// channel, executor or permit is held here. Caller owns aggregate admission.
pub(super) struct HttpOperation {
    shared: Arc<Shared>,
    owners: Option<Owners>,
    consumed: bool,
    cleanup_reported: bool,
}
impl fmt::Debug for HttpOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpOperation")
            .field("consumed", &self.consumed)
            .field("cleanup_reported", &self.cleanup_reported)
            .finish_non_exhaustive()
    }
}

impl HttpOperation {
    /// Starts a POST at the DTO table's fixed HTTPS token endpoint.
    /// Unknown/malformed endpoint and size/deadline refuse before task creation.
    pub(super) fn start(
        profile_key: &str,
        mut form_body: Vec<u8>,
        deadlines: HttpDeadlines,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, HttpError> {
        let result = autoreleasepool(|_| {
            let profile = provider_profile(profile_key)
                .ok_or_else(|| HttpError::new(HttpFailure::Endpoint))?;
            let url = checked_https_url(profile.token_endpoint)?;
            Self::start_owned_url(&url, &form_body, deadlines, cancelled)
        });
        // Best-effort hygiene, not a zeroization/OS-containment claim; SDK owns
        // its separately copied body until real task/session cleanup.
        form_body.fill(0);
        result
    }

    fn start_owned_url(
        url: &NSURL,
        form_body: &[u8],
        deadlines: HttpDeadlines,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, HttpError> {
        if form_body.len() > BODY_LIMIT {
            return Err(HttpError::new(HttpFailure::RequestTooLarge));
        }
        let now = Instant::now();
        if deadlines.cleanup < deadlines.request || deadlines.request <= now {
            return Err(HttpError::new(HttpFailure::Deadline));
        }
        let remaining = deadlines.request.duration_since(now).as_secs_f64();
        let shared = Arc::new(Shared {
            #[cfg(test)]
            body_copy_calls: std::sync::atomic::AtomicUsize::new(0),
            state: Mutex::new(ResponseState::default()),
            deadlines,
            cancelled,
            delegate_dropped: AtomicBool::new(false),
            invalidating: AtomicBool::new(false),
        });
        let delegate = AuthHttpDelegate::new(Arc::clone(&shared))?;
        let config = NSURLSessionConfiguration::ephemeralSessionConfiguration();
        config.setTimeoutIntervalForRequest(remaining);
        config.setTimeoutIntervalForResource(remaining);
        // SAFETY: Owned ephemeral config and exact initialized typed delegate;
        // queueNone requests the documented serial SDK queue. Every SDK owner
        // remains retained through terminal/invalidation and actual ivar drop.
        let session = unsafe {
            NSURLSession::sessionWithConfiguration_delegate_delegateQueue(
                &config,
                Some(ProtocolObject::from_ref(&*delegate)),
                None,
            )
        };
        let request = NSMutableURLRequest::requestWithURL(url);
        request.setHTTPMethod(&NSString::from_str("POST"));
        request.setHTTPBody(Some(&NSData::with_bytes(form_body)));
        request.setValue_forHTTPHeaderField(
            Some(&NSString::from_str("application/x-www-form-urlencoded")),
            &NSString::from_str("Content-Type"),
        );
        request.setTimeoutInterval(remaining);
        let task = session.dataTaskWithRequest(&request);
        let owners = Owners {
            session,
            task,
            _delegate: delegate,
        };
        let operation = Self {
            shared,
            owners: Some(owners),
            consumed: false,
            cleanup_reported: false,
        };
        // All factories are inside the caller's safe pool. Retained owners are
        // +1 owned values escaping it, never pool-borrowed references.
        owners_resume(&operation);
        Ok(operation)
    }

    /// Requests cancellation of the whole owning exchange, not one waiter.
    pub(super) fn cancel(&mut self) {
        self.shared.cancelled.store(true, Ordering::Release);
        self.stop_sdk();
    }

    fn stop_sdk(&self) {
        // Stop intent is independent of callback-state access. This flag is
        // not an invalidation/completion receipt; only callbacks provide those.
        if !self.shared.invalidating.swap(true, Ordering::AcqRel)
            && let Some(owners) = &self.owners
        {
            autoreleasepool(|_| {
                owners.task.cancel();
                owners.session.invalidateAndCancel();
            });
        }
    }

    fn cleanup_unproven(&self) -> HttpError {
        self.stop_sdk();
        // Sticky owner-local incident survives contention. Merge it into the
        // sole response state once readable; never overwrite its prior fault.
        if let Some(mut state) = self.shared.try_lock() {
            state.note_cleanup(HttpCleanupFailure::QuiescenceUnproven);
            state.fail(HttpFailure::CallbackProtocol);
            HttpError {
                primary: state.primary.unwrap_or(HttpFailure::CallbackProtocol),
                cleanup: state.cleanup,
                platform_code: state.platform_code,
            }
        } else {
            // This nonterminal snapshot cannot observe the earlier primary or
            // platform code. It does not replace either in ResponseState; the
            // retained owner must still poll for final attributed cleanup.
            HttpError {
                primary: HttpFailure::StateContended,
                cleanup: Some(HttpCleanupFailure::QuiescenceUnproven),
                platform_code: None,
            }
        }
    }

    /// Returns completion only after terminal, invalidation and real delegate
    /// destruction. A late owner poll cannot renew either supplied deadline.
    pub(super) fn poll(&mut self) -> HttpProgress {
        if self.consumed {
            return HttpProgress::Complete(Err(HttpError::new(HttpFailure::AlreadyConsumed)));
        }
        let now = Instant::now();
        let canceled = self.shared.cancelled.load(Ordering::Acquire);
        let request_expired = now >= self.shared.deadlines.request;
        // SDK stop cannot wait for callback copying/getters to release state.
        // A finished task has already requested invalidation; stopping here is
        // idempotent and does not decide its earlier completion timestamp.
        if canceled || request_expired {
            self.stop_sdk();
        }
        let Some(mut state) = self.shared.try_lock() else {
            if now >= self.shared.deadlines.cleanup && !self.cleanup_reported {
                self.cleanup_reported = true;
                return HttpProgress::CleanupUnproven(self.cleanup_unproven());
            }
            return HttpProgress::Pending;
        };
        if canceled {
            state.fail(HttpFailure::Cancelled);
        } else if request_expired && state.complete_at.is_none() {
            state.fail(HttpFailure::Deadline);
        }
        if self.cleanup_reported {
            state.note_cleanup(HttpCleanupFailure::QuiescenceUnproven);
            state.fail(HttpFailure::CallbackProtocol);
        }
        let invalidated = state.invalidated;
        drop(state);
        if invalidated {
            // Safe pool drains any factory autorelease obligations before the
            // independent actual-delegate/Rust-owner quiescence predicates.
            autoreleasepool(|_| {
                drop(self.owners.take());
            });
        }
        let quiescent = self.shared.delegate_dropped.load(Ordering::Acquire)
            && Arc::strong_count(&self.shared) == 1;
        if !quiescent {
            if Instant::now() >= self.shared.deadlines.cleanup && !self.cleanup_reported {
                self.cleanup_reported = true;
                return HttpProgress::CleanupUnproven(self.cleanup_unproven());
            }
            return HttpProgress::Pending;
        }
        // No blocking reacquisition, including between the first snapshot and
        // cleanup/result processing. Contention never authorizes completion.
        let Some(mut state) = self.shared.try_lock() else {
            if Instant::now() >= self.shared.deadlines.cleanup && !self.cleanup_reported {
                self.cleanup_reported = true;
                return HttpProgress::CleanupUnproven(self.cleanup_unproven());
            }
            return HttpProgress::Pending;
        };
        self.consumed = true;
        if state.complete_at.is_none() || !state.invalidated {
            state.protocol_fault();
        }
        if self.shared.cancelled.load(Ordering::Acquire) {
            state.fail(HttpFailure::Cancelled);
        }
        if let Some(primary) = state.primary {
            return HttpProgress::Complete(Err(HttpError {
                primary,
                cleanup: state.cleanup,
                platform_code: state.platform_code,
            }));
        }
        let (Some(status), Some(received_at), Some(complete_at)) =
            (state.status, state.received_at, state.complete_at)
        else {
            return HttpProgress::Complete(Err(HttpError::new(HttpFailure::UnsupportedResponse)));
        };
        let body = std::mem::take(&mut state.body);
        HttpProgress::Complete(Ok(HttpResponse {
            status,
            body,
            received_at,
            complete_at,
        }))
    }

    #[cfg(test)]
    pub(super) fn test_owner(&self) -> TestHttpOwner {
        TestHttpOwner(Arc::downgrade(&self.shared))
    }

    #[cfg(test)]
    pub(super) fn start_test_url(
        endpoint: &str,
        body: &[u8],
        deadlines: HttpDeadlines,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, HttpError> {
        autoreleasepool(|_| {
            let url = NSURL::URLWithString(&NSString::from_str(endpoint))
                .ok_or_else(|| HttpError::new(HttpFailure::Endpoint))?;
            // Synthetic harness endpoint only; no new production provider.
            if url
                .scheme()
                .is_none_or(|scheme| scheme.to_string() != "http")
                || url
                    .host()
                    .is_none_or(|host| host.to_string() != "127.0.0.1")
            {
                return Err(HttpError::new(HttpFailure::Endpoint));
            }
            Self::start_owned_url(&url, body, deadlines, cancelled)
        })
    }
}

fn owners_resume(operation: &HttpOperation) {
    if let Some(owners) = &operation.owners {
        autoreleasepool(|_| {
            if operation.shared.cancelled.load(Ordering::Acquire)
                || Instant::now() >= operation.shared.deadlines.request
            {
                if operation.shared.cancelled.load(Ordering::Acquire) {
                    operation.shared.lock().fail(HttpFailure::Cancelled);
                } else {
                    operation.shared.lock().fail(HttpFailure::Deadline);
                }
                if !operation.shared.invalidating.swap(true, Ordering::AcqRel) {
                    owners.task.cancel();
                    owners.session.invalidateAndCancel();
                }
            }
            owners.task.resume();
        });
    }
}

/// Read-only test observer; a Weak reference never holds a callback owner alive.
#[cfg(test)]
pub(super) struct TestHttpOwner(std::sync::Weak<Shared>);
#[cfg(test)]
impl TestHttpOwner {
    pub(super) fn released(&self) -> bool {
        self.0.strong_count() == 0
    }
}

impl Drop for HttpOperation {
    fn drop(&mut self) {
        if !self.consumed {
            self.shared.cancelled.store(true, Ordering::Release);
            self.stop_sdk();
        }
        autoreleasepool(|_| {
            drop(self.owners.take());
        });
        // Implicit Drop is safety fallback, NOT successful quiescence evidence.
        // SDK retains delegate/ivars until its invalidation callback; broker
        // cancellation must continue polling this owner for explicit completion.
    }
}

fn checked_https_url(endpoint: &str) -> Result<Retained<NSURL>, HttpError> {
    let url = NSURL::URLWithString(&NSString::from_str(endpoint))
        .ok_or_else(|| HttpError::new(HttpFailure::Endpoint))?;
    if url
        .scheme()
        .is_none_or(|scheme| scheme.to_string() != "https")
        || url.host().is_none_or(|host| host.to_string().is_empty())
        || url.user().is_some()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(HttpError::new(HttpFailure::Endpoint));
    }
    Ok(url)
}

#[cfg(test)]
#[path = "macos_http_tests.rs"]
pub(super) mod tests;
