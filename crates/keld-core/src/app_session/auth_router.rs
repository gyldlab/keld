//! Host-session AUTH caller admission and independent serial Native owner.
//! No issuer, grant, provider, schema, or Native timeout policy lives here.
use std::collections::hash_map::Entry;
use std::sync::atomic::AtomicUsize;

use keld_native::auth::{
    AsWorkerPort, AuthBroker, AuthRequest, HandleState, HostFault, OperationId, Progress,
    ReadyResponse, ResponseEvent,
};

use super::{
    APP_LINK_READER_POLL, AppWindowCommand, Arc, AuthConfiguration, CorrelationId, FrameKind,
    HashMap, HostAppError, Mutex, Ordering, PrimaryRouterHandle, Principal, Receiver,
    RecvTimeoutError, TrySendError, VerifiedManifest, Weak, app_detail, app_io, app_ipc, encode,
    mpsc, thread, write_frame,
};

// This is a caller/physical-leader budget, not a provider concurrency policy.
// A completed leader occupies its original reservation until Native Drained.
const AUTH_CALL_CAPACITY: usize = 32;
type CallKey = (u32, CorrelationId);

pub(super) struct AuthSession {
    pending: Mutex<HashMap<CallKey, Arc<CallerTicket>>>,
    used: AtomicUsize,
    accepting: super::AtomicBool,
    tail: super::AtomicBool,
    ownership_fault: super::AtomicBool,
    completion: Mutex<(bool, bool)>, // (physically drained, Quit requested)
    #[cfg(test)]
    after_ready_publication: Mutex<Option<ReadyPublicationHold>>,
}

// A test-owned gate, per auth session. It models a worker that has already
// written the reply but has not yet released its last physical caller lease.
#[cfg(test)]
struct ReadyPublicationHold {
    correlation: CorrelationId,
    reached: std::sync::mpsc::SyncSender<()>,
    release: Receiver<()>,
}

impl AuthSession {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(HashMap::new()),
            used: AtomicUsize::new(0),
            accepting: super::AtomicBool::new(true),
            tail: super::AtomicBool::new(false),
            ownership_fault: super::AtomicBool::new(false),
            completion: Mutex::new((false, false)),
            #[cfg(test)]
            after_ready_publication: Mutex::new(None),
        })
    }

    #[cfg(test)]
    fn hold_after_ready_publication(&self, correlation: CorrelationId) {
        let hold = {
            let mut armed = self
                .after_ready_publication
                .lock()
                .expect("test publication hold lock poisoned");
            if armed
                .as_ref()
                .is_some_and(|hold| hold.correlation == correlation)
            {
                armed.take()
            } else {
                None
            }
        };
        if let Some(hold) = hold {
            // The reply is on the real app link already. Keep the actual
            // worker-local CallerTicket alive until the test releases it.
            hold.reached.send(()).expect("test publication observer");
            hold.release
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("test publication gate release");
        }
    }

    pub(super) fn admission(
        &self,
        attempt: u32,
        corr: CorrelationId,
    ) -> super::GuardedCallAdmission {
        use super::GuardedCallAdmission;
        let Ok(pending) = self.pending.lock() else {
            return GuardedCallAdmission::RejectSession;
        };
        if !self.accepting.load(Ordering::Acquire) || pending.contains_key(&(attempt, corr)) {
            return GuardedCallAdmission::RejectSession;
        }
        if self.used.load(Ordering::Acquire) >= AUTH_CALL_CAPACITY {
            GuardedCallAdmission::AtCapacity
        } else {
            GuardedCallAdmission::Admit
        }
    }

    fn reserve(
        self: &Arc<Self>,
        attempt: u32,
        corr: CorrelationId,
    ) -> Result<Arc<CallerTicket>, HostAppError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| app_detail("auth admission", "caller lock poisoned"))?;
        if !self.accepting.load(Ordering::Acquire) || pending.contains_key(&(attempt, corr)) {
            return Err(app_detail("auth admission", "closed or duplicate caller"));
        }
        self.used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                (used < AUTH_CALL_CAPACITY).then(|| used + 1)
            })
            .map_err(|_| app_detail("auth admission", "bounded caller capacity exhausted"))?;
        let ticket = Arc::new(CallerTicket {
            session: Arc::downgrade(self),
            key: (attempt, corr),
        });
        pending.insert(ticket.key, Arc::clone(&ticket));
        Ok(ticket)
    }

    fn take(&self, ticket: &Arc<CallerTicket>) -> Result<bool, HostAppError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| app_detail("auth publication", "caller lock poisoned"))?;
        if pending
            .get(&ticket.key)
            .is_some_and(|stored| Arc::ptr_eq(stored, ticket))
        {
            pending.remove(&ticket.key);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn live(&self, ticket: &Arc<CallerTicket>) -> bool {
        self.pending.lock().is_ok_and(|pending| {
            pending
                .get(&ticket.key)
                .is_some_and(|stored| Arc::ptr_eq(stored, ticket))
        })
    }

    pub(super) fn retire(&self, attempt: u32) -> Result<(), HostAppError> {
        self.pending
            .lock()
            .map_err(|_| app_detail("auth retirement", "caller lock poisoned"))?
            .retain(|(owner, _), _| *owner != attempt);
        Ok(())
    }

    pub(super) fn pending_for(
        &self,
        attempt: u32,
    ) -> Result<Vec<(keld_ipc::ChannelId, CorrelationId)>, HostAppError> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| app_detail("auth terminal answers", "caller lock poisoned"))?;
        Ok(pending
            .keys()
            .filter(|(owner, _)| *owner == attempt)
            .map(|(_, corr)| (keld_ipc::channel_table::AUTH.id(), *corr))
            .collect())
    }

    pub(super) fn quit_after_drain(
        &self,
        handle: &PrimaryRouterHandle,
    ) -> Result<(), HostAppError> {
        self.stop_tail();
        let mut completion = self
            .completion
            .lock()
            .map_err(|_| app_detail("auth tail", "completion lock poisoned"))?;
        if completion.1 {
            return Ok(());
        }
        completion.1 = true;
        if completion.0 {
            handle
                .window_commands
                .send(AppWindowCommand::Quit)
                .map_err(|_| app_detail("auth tail", "UI event loop unavailable"))?;
        }
        Ok(())
    }

    fn physically_drained(&self, handle: &PrimaryRouterHandle) -> Result<(), HostAppError> {
        let mut completion = self
            .completion
            .lock()
            .map_err(|_| app_detail("auth tail", "completion lock poisoned"))?;
        completion.0 = true;
        if completion.1 {
            handle
                .window_commands
                .send(AppWindowCommand::Quit)
                .map_err(|_| app_detail("auth tail", "UI event loop unavailable"))?;
        }
        Ok(())
    }

    pub(super) fn stop_tail(&self) {
        self.accepting.store(false, Ordering::Release);
        self.tail.store(true, Ordering::Release);
    }
}

struct CallerTicket {
    session: Weak<AuthSession>,
    key: CallKey,
}
impl Drop for CallerTicket {
    fn drop(&mut self) {
        if let Some(session) = self.session.upgrade()
            && session
                .used
                .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                    used.checked_sub(1)
                })
                .is_err()
        {
            // Impossible ownership underflow closes admission; never wrap capacity.
            session.ownership_fault.store(true, Ordering::Release);
            session.stop_tail();
        }
    }
}

pub(super) struct AuthWork {
    ticket: Arc<CallerTicket>,
    request: AuthRequest,
    cancellation: Arc<super::AtomicBool>,
}
struct Operation {
    // Exactly the Some(admission) returned to the physical leader. This survives
    // every logical answer and caller detachment, and drops only on Drained.
    _lease: Arc<CallerTicket>,
    invariant_leases: Vec<Arc<CallerTicket>>,
    callers: Vec<Arc<CallerTicket>>,
}

pub(super) struct AuthWorker {
    join: Option<super::JoinHandle<Result<(), HostAppError>>>,
    session: Arc<AuthSession>,
}

impl AuthWorker {
    pub(super) fn start(
        handle: &mut PrimaryRouterHandle,
        verified: VerifiedManifest,
        mut configuration: AuthConfiguration,
        presentation: Option<AsWorkerPort>,
    ) -> Result<Self, HostAppError> {
        // Sole closed-table filter owned by Native. No validation of known
        // clientId/fixedRedirect here: that remains after the actual Allow.
        keld_native::auth::retain_known_profiles(&mut configuration, |_| {
            eprintln!("keld: ignored unknown auth profile in operator configuration");
        });
        let session = AuthSession::new();
        let (commands, receiver) = mpsc::sync_channel(AUTH_CALL_CAPACITY);
        handle.auth = Some(Arc::clone(&session));
        handle.auth_commands = Some(commands);
        let owner = handle.clone();
        let worker_session = Arc::clone(&session);
        let join = thread::Builder::new()
            .name(String::from("keld-core-auth-worker"))
            .spawn(move || {
                run_worker(
                    &receiver,
                    &owner,
                    &worker_session,
                    &verified,
                    configuration,
                    presentation,
                )
            })
            .map_err(|source| app_io("auth worker startup", &source))?;
        Ok(Self {
            join: Some(join),
            session,
        })
    }

    // Called only by router terminal owner, outside its transition/current
    // locks. MainThreadActor polls merely request stop_tail and never Join.
    pub(super) fn stop_and_join(&mut self) -> Result<(), HostAppError> {
        self.session.stop_tail();
        self.join.take().map_or(Ok(()), |join| {
            join.join()
                .map_err(|_| app_detail("auth worker", "thread panicked"))?
        })
    }
}

impl Drop for AuthWorker {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

// A recovered write loss is an observed typed incident, not successful delivery.
enum AuthPublication {
    NoIncident,
    GenerationRetired(HostAppError),
}

impl PrimaryRouterHandle {
    pub(super) fn enqueue_auth(
        &self,
        attempt: u32,
        corr: CorrelationId,
        payload: &[u8],
        cancellation: &Arc<super::AtomicBool>,
    ) -> Result<(), HostAppError> {
        // Header rejects before payload; recheck at ownership admission BEFORE
        // decoding. Retirement and enqueue share this existing transition.
        let _transition = self.shutdown.transition_guard();
        let current = self
            .current
            .lock()
            .map_err(|_| app_detail("auth admission", "generation lock poisoned"))?;
        if !self.shutdown.is_running()
            || current.as_ref().is_none_or(|a| a.attempt != attempt)
            || cancellation.load(Ordering::Acquire)
        {
            return Err(app_detail("auth admission", "caller generation retired"));
        }
        let session = self
            .auth
            .as_ref()
            .ok_or_else(|| app_detail("auth admission", "guarded auth owner unavailable"))?;
        let ticket = session.reserve(attempt, corr)?;
        let request = match super::decode(payload) {
            Ok(request) => request,
            Err(source) => {
                session.take(&ticket)?;
                return Err(app_ipc("auth request", &source));
            }
        };
        let commands = self
            .auth_commands
            .as_ref()
            .ok_or_else(|| app_detail("auth submission", "worker queue unavailable"))?;
        match commands.try_send(AuthWork {
            ticket: Arc::clone(&ticket),
            request,
            cancellation: Arc::clone(cancellation),
        }) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(work) | TrySendError::Disconnected(work)) => {
                session.take(&work.ticket)?;
                Err(app_detail(
                    "auth submission",
                    "bounded auth worker queue unavailable",
                ))
            }
        }
    }

    fn publish_auth(
        &self,
        session: &AuthSession,
        ticket: &Arc<CallerTicket>,
        response: ReadyResponse,
    ) -> Result<AuthPublication, HostAppError> {
        let transition = self.shutdown.transition_guard();
        let mut current = self
            .current
            .lock()
            .map_err(|_| app_detail("auth publication", "generation lock poisoned"))?;
        let Some(active) = current.as_mut().filter(|a| a.attempt == ticket.key.0) else {
            return Ok(AuthPublication::NoIncident);
        };
        if !self.shutdown.is_running() || active.reader_stop.load(Ordering::Acquire) {
            return Ok(AuthPublication::NoIncident);
        }
        if !session.take(ticket)? {
            return Ok(AuthPublication::NoIncident);
        }
        // Freshness conversion occurs per live follower AFTER final Core checks.
        let result = response.into_response();
        let channel = keld_ipc::channel_table::AUTH.id();
        let written = match result {
            Ok(response) => {
                let payload =
                    encode(&response).map_err(|source| app_ipc("auth response", &source))?;
                write_frame(
                    &mut active.writer,
                    FrameKind::Reply,
                    0,
                    channel,
                    ticket.key.1,
                    &payload,
                )
                .map_err(|source| app_ipc("auth reply", &source))
            }
            Err(error) => keld_ipc::write_call_error(
                &mut active.writer,
                channel,
                ticket.key.1,
                error.as_ref(),
            )
            .map_err(|source| app_ipc("auth call error", &source)),
        };
        if let Err(primary) = written {
            let cleanup = self.retire_current_generation_locked(
                &mut current,
                ticket.key.0,
                "auth terminal write lost",
                "auth terminal write link close",
            );
            // Existing guardian link-loss owner chooses successor recovery.
            drop(current);
            drop(transition);
            let recovery = self.link_failed(ticket.key.0);
            return match (cleanup, recovery) {
                (Ok(()), Ok(())) => Ok(AuthPublication::GenerationRetired(primary)),
                (cleanup, recovery) => Err(super::append_app_cleanup(primary, [cleanup, recovery])),
            };
        }
        Ok(AuthPublication::NoIncident)
    }
}

fn host_fault(fault: &HostFault) -> HostAppError {
    match fault {
        HostFault::BrowserCleanupUnproven(error) | HostFault::HttpCleanupUnproven(error) => {
            // Preserve the original resource-redacted component error, host only.
            app_detail("auth physical cleanup", error.to_string())
        }
        _ => app_detail("auth native invariant", format!("{fault:?}")),
    }
}

fn record_failure(
    handle: &PrimaryRouterHandle,
    session: &AuthSession,
    failure: &mut Option<HostAppError>,
    error: HostAppError,
) {
    session.stop_tail();
    if failure.is_none() {
        eprintln!("{error}"); // only already-redacted component/Core diagnostics
        *failure = Some(error);
        let _ = handle.window_commands.send(AppWindowCommand::Fatal);
    }
}

// All publication callers share this classification. The original resource-redacted
// typed incident is reported only after retirement and the existing owner ACK succeed.
fn observe_auth_publication(
    handle: &PrimaryRouterHandle,
    session: &AuthSession,
    failure: &mut Option<HostAppError>,
    publication: Result<AuthPublication, HostAppError>,
) {
    match publication {
        Ok(AuthPublication::NoIncident) => {}
        Ok(AuthPublication::GenerationRetired(primary)) => eprintln!("{primary}"),
        Err(error) => record_failure(handle, session, failure, error),
    }
}

fn run_worker(
    receiver: &Receiver<AuthWork>,
    handle: &PrimaryRouterHandle,
    session: &AuthSession,
    verified: &VerifiedManifest,
    configuration: AuthConfiguration,
    presentation: Option<AsWorkerPort>,
) -> Result<(), HostAppError> {
    // Construct/use/drop on one owner thread. This broker is never replaced by
    // a Bun generation or an FS retirement command.
    let mut broker = match presentation {
        Some(port) => AuthBroker::prepare_with_as(verified, configuration, port),
        None => AuthBroker::prepare(verified, configuration),
    };
    let mut operations: HashMap<OperationId, Operation> =
        HashMap::with_capacity(AUTH_CALL_CAPACITY);
    let mut events = Vec::with_capacity(AUTH_CALL_CAPACITY);
    let mut ids = Vec::with_capacity(AUTH_CALL_CAPACITY);
    let mut tail_started = false;
    let mut failure: Option<HostAppError> = None;
    loop {
        events.clear();
        if session.ownership_fault.load(Ordering::Acquire) && failure.is_none() {
            record_failure(
                handle,
                session,
                &mut failure,
                app_detail("auth ownership", "caller lease counter underflow"),
            );
        }
        if session.tail.load(Ordering::Acquire) && !tail_started {
            broker.stop_for_host_tail(|event| events.push(event));
            tail_started = true;
        }
        ids.clear();
        ids.extend(operations.keys().cloned());
        for id in ids.drain(..) {
            let progress = broker.poll(&id, |event| events.push(event));
            if let Progress::HostFault(fault) = &progress {
                record_failure(handle, session, &mut failure, host_fault(fault));
            }
            publish_events(&mut events, &mut operations, handle, session, &mut failure);
            if progress == Progress::Drained {
                operations.remove(&id); // sole physical lease release boundary
            } else if let Some(operation) = operations.get_mut(&id) {
                operation.callers.retain(|ticket| session.live(ticket));
            }
        }
        // stop_for_host_tail may emit even when no operation needed polling.
        publish_events(&mut events, &mut operations, handle, session, &mut failure);
        if tail_started && operations.is_empty() {
            return finish_worker_tail(receiver, handle, session, failure);
        }
        let incoming = receiver.recv_timeout(APP_LINK_READER_POLL);
        let work = match incoming {
            Ok(work) => work,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                session.stop_tail();
                continue;
            }
        };
        let entered =
            enter_auth_request(&mut broker, handle, session, verified, work, tail_started);
        let (state, ticket) = match entered {
            Ok(Some(entered)) => entered,
            Ok(None) => continue,
            Err(error) => {
                record_failure(handle, session, &mut failure, error);
                continue; // retain broker and existing physical operations
            }
        };
        dispatch_result(
            state,
            ticket,
            &mut operations,
            handle,
            session,
            &mut failure,
        );
    }
}

fn publish_events(
    events: &mut Vec<ResponseEvent>,
    operations: &mut HashMap<OperationId, Operation>,
    handle: &PrimaryRouterHandle,
    session: &AuthSession,
    failure: &mut Option<HostAppError>,
) {
    for ResponseEvent {
        operation,
        response,
    } in events.drain(..)
    {
        if let Some(operation) = operations.get_mut(&operation) {
            for ticket in operation.callers.drain(..) {
                observe_auth_publication(
                    handle,
                    session,
                    failure,
                    handle.publish_auth(session, &ticket, response.clone()),
                );
            }
        }
    }
}

fn finish_worker_tail(
    receiver: &Receiver<AuthWork>,
    handle: &PrimaryRouterHandle,
    session: &AuthSession,
    failure: Option<HostAppError>,
) -> Result<(), HostAppError> {
    // Pending queued work has no native effects, and must not reenter.
    while let Ok(work) = receiver.try_recv() {
        session.take(&work.ticket)?;
    }
    session
        .pending
        .lock()
        .map_err(|_| app_detail("auth tail", "caller lock poisoned"))?
        .clear();
    if session.used.load(Ordering::Acquire) != 0 {
        return Err(app_detail(
            "auth tail",
            "caller leases remain after physical drain",
        ));
    }
    let notify = if failure.is_none() {
        session.physically_drained(handle)
    } else {
        Ok(())
    };
    super::collapse_app_results([failure.map_or(Ok(()), Err), notify])
}

type EnteredAuthRequest = (HandleState<Arc<CallerTicket>>, Arc<CallerTicket>);

fn enter_auth_request(
    broker: &mut AuthBroker,
    handle: &PrimaryRouterHandle,
    session: &AuthSession,
    verified: &VerifiedManifest,
    work: AuthWork,
    tail_started: bool,
) -> Result<Option<EnteredAuthRequest>, HostAppError> {
    let AuthWork {
        ticket,
        request,
        cancellation,
    } = work;
    let _transition = handle.shutdown.transition_guard();
    let current = handle
        .current
        .lock()
        .map_err(|_| app_detail("auth entry", "generation lock poisoned"))?;
    if tail_started
        || !session.accepting.load(Ordering::Acquire)
        || !handle.shutdown.is_running()
        || current.as_ref().is_none_or(|a| a.attempt != ticket.key.0)
        || cancellation.load(Ordering::Acquire)
        || !session.live(&ticket)
    {
        session.take(&ticket)?;
        return Ok(None);
    }
    // Native's single guard precedes effects. Retirement cannot cross
    // the existing Core transition/current barrier during entry.
    let state = broker.handle_request(verified, Principal::AppProcess, request, || {
        Some(Arc::clone(&ticket))
    });
    Ok(Some((state, ticket)))
}

fn dispatch_result(
    state: HandleState<Arc<CallerTicket>>,
    ticket: Arc<CallerTicket>,
    operations: &mut HashMap<OperationId, Operation>,
    handle: &PrimaryRouterHandle,
    session: &AuthSession,
    failure: &mut Option<HostAppError>,
) {
    match state {
        HandleState::Ready(response) => {
            observe_auth_publication(
                handle,
                session,
                failure,
                handle.publish_auth(session, &ticket, response),
            );
            #[cfg(test)]
            session.hold_after_ready_publication(ticket.key.1);
        }
        HandleState::Pending {
            operation,
            admission,
        } => {
            match (operations.entry(operation), admission) {
                (Entry::Vacant(entry), Some(lease)) => {
                    entry.insert(Operation {
                        _lease: lease,
                        invariant_leases: Vec::new(),
                        callers: vec![ticket],
                    });
                }
                (Entry::Occupied(mut entry), None) => {
                    entry.get_mut().callers.push(ticket);
                }
                // Native single-flight invariant failed: retain any returned
                // physical lease; close admission and poll every known owner.
                (Entry::Occupied(mut entry), Some(lease)) => {
                    entry.get_mut().invariant_leases.push(lease);
                    record_failure(
                        handle,
                        session,
                        failure,
                        app_detail("auth owner", "duplicate native leader"),
                    );
                }
                (Entry::Vacant(entry), None) => {
                    entry.insert(Operation {
                        _lease: Arc::clone(&ticket),
                        invariant_leases: Vec::new(),
                        callers: vec![ticket],
                    });
                    record_failure(
                        handle,
                        session,
                        failure,
                        app_detail("auth owner", "coalesced operation owner missing"),
                    );
                }
            }
        }
        HandleState::DeniedInvariant { response, fault } => {
            observe_auth_publication(
                handle,
                session,
                failure,
                handle.publish_auth(session, &ticket, response),
            );
            record_failure(handle, session, failure, host_fault(&fault));
        }
        HandleState::HostFault(fault) => {
            record_failure(handle, session, failure, host_fault(&fault));
        }
        HandleState::AtCapacity => {
            record_failure(
                handle,
                session,
                failure,
                app_detail("auth owner", "reserved admission unavailable"),
            );
        }
    }
}

#[cfg(all(test, not(keld_core_native_renderer_main)))]
#[path = "auth_router_tests.rs"]
mod tests;
