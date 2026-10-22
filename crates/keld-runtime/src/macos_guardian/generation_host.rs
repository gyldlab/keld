//! One owner for host-side primary generation control and recovery transitions.
use super::generation_wire::initial_generation_record_timeout;
use super::process_group::{
    require_attested_group_absence, terminate_attested_group, validate_guardian_child_group,
};
use super::{
    BoundRoleGeneration, CONTROL_CLEAR, CONTROL_CLEARED, CONTROL_PREPARE, CONTROL_PREPARED,
    CONTROL_REGISTERED, CONTROL_REVOKE, CONTROL_REVOKED, CONTROL_SPAWNED,
    DEFAULT_ADMISSION_TIMEOUT, GENERATION_CONTROL_POLL, GUARDIAN_SHUTDOWN_DEADLINE,
    GenerationControlReader, GenerationControlRecord, GenerationLease, GuardedPrimaryUpdate,
    Instant, Mutex, Receiver, RecoveryState, RegisteredPrimaryGeneration, RevocationCause,
    RoleEvent, RoleGenerationLease, RoleGenerationOwner, RoleOwner, RuntimeError,
    SUPERVISED_QUIT_ACK_DEADLINE, Sender, collapse_failures, decode_revocation_cause, io,
    lifecycle_error, lock_generation, lock_recovery, mpsc, thread, write_generation_control,
};

#[cfg(test)]
pub(super) struct GenerationProvisionFence {
    pub(super) attempt: u32,
    pub(super) entered: mpsc::SyncSender<()>,
    pub(super) release: Receiver<()>,
}

#[cfg(test)]
pub(super) struct GenerationRegistrationFence {
    pub(super) attempt: u32,
    pub(super) entered: mpsc::SyncSender<u32>,
    pub(super) release: Receiver<()>,
    // A complete fixed PREPARE has arrived on the actual authenticated
    // generation-control stream, before the worker acquires recovery state.
    pub(super) received_prepare: Option<mpsc::SyncSender<()>>,
    // Independent test-only receipt after a complete authentic SPAWNED
    // record was decoded and its real OS parent/birth validated, but before
    // the host decides whether shutdown permits REGISTERED.
    pub(super) received_spawned: Option<mpsc::SyncSender<u32>>,
}

/// The retained Guardian PID and the original authenticated-bootstrap deadline
/// belong to one generation-control host admission, not separate callers.
#[derive(Debug, Clone, Copy)]
pub(super) struct GenerationHostAdmission {
    guardian_pid: u32,
    first_record_deadline: Instant,
}

impl GenerationHostAdmission {
    pub(super) const fn new(guardian_pid: u32, first_record_deadline: Instant) -> Self {
        Self {
            guardian_pid,
            first_record_deadline,
        }
    }
}

#[derive(Default)]
pub(super) struct HostGenerationState {
    pub(super) lease: Option<RoleGenerationLease>,
    pub(super) current_attempt: Option<u32>,
    pub(super) revoked_attempt: Option<u32>,
    pub(super) pending_prepare: Option<GenerationControlRecord>,
    pub(super) initial_decision_deadline: Option<Instant>,
}

#[allow(clippy::too_many_lines)] // one owner keeps every private protocol transition contiguous
pub(super) fn run_generation_host(
    admission: GenerationHostAdmission,
    mut control: std::os::unix::net::UnixStream,
    updates_tx: &Sender<GuardedPrimaryUpdate>,
    current_generation: &Mutex<Option<RegisteredPrimaryGeneration>>,
    recovery_decision: &Mutex<RecoveryState>,
    #[cfg(test)] provision_fence: Option<&GenerationProvisionFence>,
    #[cfg(test)] registration_fence: Option<&GenerationRegistrationFence>,
) -> Result<(), RuntimeError> {
    control
        .set_read_timeout(Some(GENERATION_CONTROL_POLL))
        .map_err(|source| lifecycle_error("macOS guardian generation host deadline", source))?;
    control
        .set_write_timeout(Some(GENERATION_CONTROL_POLL))
        .map_err(|source| lifecycle_error("macOS guardian generation host deadline", source))?;
    let (events_tx, events_rx) = mpsc::channel();
    let (bound_tx, bound_rx) = mpsc::channel();
    let mut owner = RoleGenerationOwner::new(
        RoleOwner::Primary,
        DEFAULT_ADMISSION_TIMEOUT,
        events_tx,
        Some(bound_tx),
        #[cfg(test)]
        #[cfg(unix)]
        None,
    );
    let mut state = HostGenerationState::default();
    let mut control_reader = GenerationControlReader::new();
    let mut first_record_pending = true;
    loop {
        if let Some(lease) = state.lease.as_mut() {
            lease.poll()?;
        }
        forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
        // Authentication is not permission to hold the host's generation
        // worker forever with an open zero-byte KGC1 stream. The first fixed
        // control record shares the ORIGINAL authenticated-bootstrap deadline;
        // unlike the existing per-record timer, this includes zero bytes.
        if first_record_pending && Instant::now() >= admission.first_record_deadline {
            return Err(initial_generation_record_timeout());
        }
        let record = match control_reader.poll_until(
            &mut control,
            first_record_pending.then_some(admission.first_record_deadline),
        ) {
            Ok(record) => record,
            Err(error) if runtime_io_kind(&error) == Some(io::ErrorKind::UnexpectedEof) => {
                let result = finish_generation_control_eof(
                    error,
                    control_reader.filled,
                    &mut state,
                    current_generation,
                    recovery_decision,
                );
                forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
                return result;
            }
            Err(error) => return Err(error),
        };
        if first_record_pending {
            if Instant::now() >= admission.first_record_deadline {
                // A fully buffered late frame is not accepted after the
                // original deadline merely because a read completed.
                return Err(initial_generation_record_timeout());
            }
            if record.is_some() {
                first_record_pending = false;
            }
        }
        #[cfg(test)]
        if record
            .as_ref()
            .is_some_and(|record| record.kind == CONTROL_PREPARE)
            && let Some(received) = registration_fence
                .filter(|fence| {
                    record
                        .as_ref()
                        .is_some_and(|record| record.attempt == fence.attempt)
                })
                .and_then(|fence| fence.received_prepare.as_ref())
        {
            received.send(()).map_err(|_| {
                lifecycle_error(
                    "test verified KGC1 PREPARE receipt",
                    io::Error::other("native PREPARE observer ended"),
                )
            })?;
        }
        let expected_attempt = lock_recovery(recovery_decision).expected_attempt;
        if let Some(record) = record {
            if state.pending_prepare.is_some() {
                return Err(invalid_control_transition(
                    "pending prepare",
                    record.attempt,
                    expected_attempt,
                ));
            }
            match record.kind {
                CONTROL_PREPARE => {
                    if record.attempt != expected_attempt
                        || state.current_attempt.is_some()
                        || state.lease.is_some()
                        || state.revoked_attempt.is_some()
                        || lock_generation(current_generation).is_some()
                        || record.pid != 0
                        || record.cause != 0
                        || !record.payload.is_empty()
                    {
                        return Err(invalid_control_transition(
                            "prepare",
                            record.attempt,
                            expected_attempt,
                        ));
                    }
                    if record.attempt > 1 && !lock_recovery(recovery_decision).initial_ready {
                        state.initial_decision_deadline =
                            Some(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE);
                    }
                    state.pending_prepare = Some(record);
                }
                CONTROL_SPAWNED => {
                    if state.current_attempt != Some(record.attempt)
                        || record.pid == 0
                        || record.cause != 0
                        || !record.payload.is_empty()
                        || lock_generation(current_generation).is_some()
                    {
                        return Err(invalid_control_transition(
                            "spawned",
                            record.attempt,
                            expected_attempt,
                        ));
                    }
                    // A PREPARED locator is not authority to execute a
                    // not-yet-registered process. Serialize the *actual*
                    // REGISTERED ACK (the Guardian's pre-exec permit) with
                    // the host's permanent denial and S write. Otherwise a
                    // delayed SPAWNED could authorize Bun after S had already
                    // been flushed, despite the denied recovery decision.
                    let recovery = lock_recovery(recovery_decision);
                    if state.lease.is_none() {
                        return Err(invalid_control_transition(
                            "spawned",
                            record.attempt,
                            expected_attempt,
                        ));
                    }
                    let group_identity =
                        validate_guardian_child_group(record.pid, admission.guardian_pid)?;
                    #[cfg(test)]
                    if let Some(received) = registration_fence
                        .filter(|fence| fence.attempt == record.attempt)
                        .and_then(|fence| fence.received_spawned.as_ref())
                    {
                        received.send(record.pid).map_err(|_| {
                            lifecycle_error(
                                "test verified KGC1 SPAWNED receipt",
                                io::Error::other("native SPAWNED observer ended"),
                            )
                        })?;
                    }
                    if recovery.denied {
                        // This is still a real, host-attested Guardian child.
                        // Take temporary host custody BEFORE signaling so a
                        // fail-safe retains its original birth on an OS fault.
                        // Never trust the caller's raw numeric group alone.
                        let mut retained = lock_generation(current_generation);
                        *retained = Some(RegisteredPrimaryGeneration::authenticated(
                            record.attempt,
                            group_identity,
                        ));
                        let native_cleanup = terminate_attested_group(group_identity);
                        if native_cleanup.is_ok() {
                            retained.take();
                        }
                        drop(retained);
                        drop(recovery);
                        let lease_cleanup = state
                            .lease
                            .take()
                            .map(|lease| lease.revoke(RevocationCause::Shutdown))
                            .transpose();
                        if let Err(cleanup) = native_cleanup {
                            return Err(match lease_cleanup {
                                Ok(_) => cleanup,
                                Err(other) => collapse_failures(vec![cleanup, other]),
                            });
                        }
                        lease_cleanup?;
                        return Err(refused_post_shutdown_spawn());
                    }
                    let current = state.lease.as_mut().ok_or_else(|| {
                        invalid_control_transition("spawned", record.attempt, expected_attempt)
                    })?;
                    // The authenticated Guardian is still untrusted input.
                    // Before granting an app link or recording a killable PGID,
                    // prove this leader is its actual direct OS child.
                    current.child_spawned(record.pid, record.attempt)?;
                    *lock_generation(current_generation) = Some(
                        RegisteredPrimaryGeneration::authenticated(record.attempt, group_identity),
                    );
                    forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
                    #[cfg(test)]
                    if let Some(fence) =
                        registration_fence.filter(|fence| fence.attempt == record.attempt)
                    {
                        // The actual host retains the group, but must not yet
                        // emit REGISTERED or authorize child exec. No shipping
                        // control records, deadlines or extra owners change.
                        fence.entered.send(record.pid).map_err(|_| {
                            lifecycle_error(
                                "test generation registration receipt",
                                io::Error::other("test ACK controller ended"),
                            )
                        })?;
                        fence
                            .release
                            .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
                            .map_err(|source| {
                                lifecycle_error(
                                    "test generation REGISTERED ACK release",
                                    io::Error::other(source),
                                )
                            })?;
                    }
                    write_generation_control(
                        &mut control,
                        &GenerationControlRecord::new(CONTROL_REGISTERED, record.attempt)
                            .with_pid(record.pid),
                    )?;
                    // Release only AFTER the complete fixed KGC1 REGISTERED
                    // record was published. Any later S is ordered after
                    // native group retention and authorization.
                    drop(recovery);
                }
                CONTROL_REVOKE => {
                    if state.current_attempt != Some(record.attempt)
                        || state.revoked_attempt.is_some()
                        || record.pid != 0
                        || !record.payload.is_empty()
                    {
                        return Err(invalid_control_transition(
                            "revoke",
                            record.attempt,
                            expected_attempt,
                        ));
                    }
                    let cause = decode_revocation_cause(record.cause)?;
                    let current = state.lease.take().ok_or_else(|| {
                        invalid_control_transition("revoke", record.attempt, expected_attempt)
                    })?;
                    // Close native release before the lease emits Revoked.
                    lock_recovery(recovery_decision).retire(record.attempt);
                    current.revoke(cause)?;
                    state.revoked_attempt = Some(record.attempt);
                    forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
                    write_generation_control(
                        &mut control,
                        &GenerationControlRecord::new(CONTROL_REVOKED, record.attempt),
                    )?;
                }
                CONTROL_CLEAR => {
                    // Serialize the native CLEAR fail-safe with host-triggered
                    // fail_current_generation. Without this one lock owner,
                    // an old failure callback could retain a copy of the
                    // retired PGID and signal it after a successor was minted.
                    let mut current_generation_owner = lock_generation(current_generation);
                    let registered_generation = *current_generation_owner;
                    let registered =
                        registered_generation.map_or(0, |generation| generation.group_pid);
                    if state.current_attempt != Some(record.attempt)
                        || state.revoked_attempt != Some(record.attempt)
                        || record.cause != 0
                        || !record.payload.is_empty()
                        || record.pid != registered
                    {
                        return Err(invalid_control_transition(
                            "clear",
                            record.attempt,
                            expected_attempt,
                        ));
                    }
                    // Reuse one existing shutdown budget for both the native
                    // signal and the read-only group-absence witness. Keep
                    // current_generation_owner locked until that proof passes.
                    let absence_deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
                    // CONTROL_REVOKED only proves that app-link capability was
                    // revoked. The authenticated Guardian's CLEAR record is
                    // not evidence that it actually killed its Bun group:
                    // independently signal the same attested native group
                    // before relinquishing the host's last fail-safe owner or
                    // admitting a successor. A forged/early CLEAR must never
                    // make a still-running descendant invisible to the host.
                    // SIGKILL success is not sufficient: a held zombie still
                    // has a live PGID. Only dual ESRCH permits CLEARED.
                    if let Some(generation) = registered_generation {
                        match generation.group_identity {
                            Some(group) => {
                                terminate_attested_group(group)?;
                                require_attested_group_absence(group, absence_deadline)?;
                            }
                            #[cfg(test)]
                            None => {} // Synthetic EOF/transition-only fixtures.
                            #[cfg(not(test))]
                            None => {
                                return Err(lifecycle_error(
                                    "macOS guardian generation clear authority",
                                    io::Error::new(
                                        io::ErrorKind::PermissionDenied,
                                        "registered Bun group has no native birth witness",
                                    ),
                                ));
                            }
                        }
                    }
                    current_generation_owner.take();
                    drop(current_generation_owner);
                    write_generation_control(
                        &mut control,
                        &GenerationControlRecord::new(CONTROL_CLEARED, record.attempt)
                            .with_pid(record.pid),
                    )?;
                    state.current_attempt = None;
                    state.revoked_attempt = None;
                    // None alone is not Clear: only the completed acknowledgment
                    // advances the attempt and permits a pending release request.
                    lock_recovery(recovery_decision).cleared(record.attempt)?;
                }
                _ => {
                    return Err(lifecycle_error(
                        "macOS guardian generation host transition",
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("unexpected control kind {}", record.kind),
                        ),
                    ));
                }
            }
        }
        if control_reader.filled != 0 {
            continue;
        }
        let Some(pending) = state.pending_prepare.as_ref() else {
            continue;
        };
        let provisioned = {
            let mut recovery = lock_recovery(recovery_decision);
            check_initial_recovery_decision(&recovery, state.initial_decision_deadline)?;
            if pending.attempt == 1 && recovery.denied {
                return Err(rejected_recovery());
            }
            if !recovery.permits(pending.attempt) {
                continue;
            }
            recovery.release_attempt = None;
            // This actual bind/token mint is ordered against permanent denial.
            // Do not release the recovery guard between the check and provision.
            #[cfg(test)]
            if let Some(fence) = provision_fence.filter(|fence| fence.attempt == pending.attempt) {
                // Scoped unit-test custody only: shipping code has no fence,
                // callback, global, extra owner, or branch here. The real host
                // owner retains its recovery guard through the actual mint.
                fence.entered.send(()).map_err(|_| {
                    lifecycle_error(
                        "test generation pre-mint receipt",
                        io::Error::other("test custodian ended"),
                    )
                })?;
                fence
                    .release
                    .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
                    .map_err(|source| {
                        lifecycle_error(
                            "test generation pre-mint release",
                            io::Error::other(source),
                        )
                    })?;
            }
            owner.provision(pending.attempt)?
        };
        let response = GenerationControlRecord::new(CONTROL_PREPARED, pending.attempt)
            .with_payload(provisioned.app_link.as_bytes().to_vec());
        state.current_attempt = Some(pending.attempt);
        state.lease = Some(provisioned.lease);
        state.pending_prepare = None;
        state.initial_decision_deadline = None;
        forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
        if let Err(error) = write_generation_control(&mut control, &response) {
            if matches!(
                runtime_io_kind(&error),
                Some(io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset)
            ) {
                let result = finish_generation_control_eof(
                    error,
                    control_reader.filled,
                    &mut state,
                    current_generation,
                    recovery_decision,
                );
                forward_generation_updates(updates_tx, &bound_rx, &events_rx)?;
                return result;
            }
            return Err(error);
        }
    }
}

/// Typed, private evidence that initial preparation was rejected by the
/// host's own permanent recovery denial before a generation existed. It is
/// not a general KGC1 I/O, parser or OS-cleanup error.
#[derive(Debug)]
struct InitialRecoveryDenied;

impl std::fmt::Display for InitialRecoveryDenied {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("successor preparation was rejected before initial Ready")
    }
}

impl std::error::Error for InitialRecoveryDenied {}

fn rejected_recovery() -> RuntimeError {
    lifecycle_error(
        "macOS guardian generation host transition",
        io::Error::new(io::ErrorKind::InvalidData, InitialRecoveryDenied),
    )
}

/// Typed, private evidence that the host refused an in-flight SPAWNED
/// because its own permanent denial had already won, AFTER it took custody
/// of the attested child group, signalled it and released custody, and
/// revoked the prepared role lease. A failed cleanup never carries it.
#[derive(Debug)]
struct PostShutdownSpawnRefused;

impl std::fmt::Display for PostShutdownSpawnRefused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .write_str("SPAWNED refused after host S; no CONTROL_REGISTERED permit was granted")
    }
}

impl std::error::Error for PostShutdownSpawnRefused {}

fn refused_post_shutdown_spawn() -> RuntimeError {
    lifecycle_error(
        "macOS guardian post-shutdown registration refusal",
        io::Error::new(io::ErrorKind::PermissionDenied, PostShutdownSpawnRefused),
    )
}

pub(super) fn is_post_shutdown_spawn_refused(error: &RuntimeError) -> bool {
    matches!(error, RuntimeError::Lifecycle { source, .. }
        if source.get_ref()
            .and_then(|source| source.downcast_ref::<PostShutdownSpawnRefused>())
            .is_some())
}

pub(super) fn is_initial_recovery_denied(error: &RuntimeError) -> bool {
    matches!(error, RuntimeError::Lifecycle { source, .. }
        if source.get_ref()
            .and_then(|source| source.downcast_ref::<InitialRecoveryDenied>())
            .is_some())
}

pub(super) fn check_initial_recovery_decision(
    recovery: &RecoveryState,
    deadline: Option<Instant>,
) -> Result<(), RuntimeError> {
    if recovery.initial_ready || deadline.is_none() {
        return Ok(());
    }
    if recovery.denied {
        return Err(rejected_recovery());
    }
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(lifecycle_error(
            "macOS guardian generation host transition",
            io::Error::new(
                io::ErrorKind::TimedOut,
                "initial Ready did not decide recovery before the deadline",
            ),
        ));
    }
    Ok(())
}

pub(super) fn finish_generation_control_eof(
    primary: RuntimeError,
    partial_bytes: usize,
    state: &mut HostGenerationState,
    current_generation: &Mutex<Option<RegisteredPrimaryGeneration>>,
    recovery: &Mutex<RecoveryState>,
) -> Result<(), RuntimeError> {
    if partial_bytes != 0 || lock_generation(current_generation).is_some() {
        return Err(primary);
    }
    if let Some(lease) = state.lease.take() {
        let Some(attempt) = state.current_attempt.take() else {
            return Err(primary);
        };
        if state.revoked_attempt.is_some() {
            return Err(primary);
        }
        lock_recovery(recovery).retire(attempt);
        if let Err(cleanup) = lease.revoke(RevocationCause::Shutdown) {
            return Err(collapse_failures(vec![primary, cleanup]));
        }
    }
    if state.current_attempt.is_some() || state.revoked_attempt.is_some() {
        return Err(primary);
    }
    // EOF can precede the host's Q/S ACK read by one scheduling turn. Retain
    // the EOF fact and its one existing attribution budget; never reread EOF.
    let deadline = Instant::now() + SUPERVISED_QUIT_ACK_DEADLINE;
    loop {
        let recovery = lock_recovery(recovery);
        if !recovery.denied {
            return Err(primary);
        }
        if recovery.shutdown_acknowledged {
            return Ok(());
        }
        drop(recovery);
        if Instant::now() >= deadline {
            return Err(primary);
        }
        thread::yield_now();
    }
}

fn forward_generation_updates(
    updates_tx: &Sender<GuardedPrimaryUpdate>,
    bound_rx: &Receiver<BoundRoleGeneration>,
    events_rx: &Receiver<RoleEvent>,
) -> Result<(), RuntimeError> {
    while let Ok(bound) = bound_rx.try_recv() {
        updates_tx
            .send(GuardedPrimaryUpdate::Bound(bound))
            .map_err(|_| generation_consumer_gone())?;
    }
    while let Ok(event) = events_rx.try_recv() {
        updates_tx
            .send(GuardedPrimaryUpdate::Role(event))
            .map_err(|_| generation_consumer_gone())?;
    }
    Ok(())
}

fn generation_consumer_gone() -> RuntimeError {
    lifecycle_error(
        "macOS guardian generation update delivery",
        io::Error::new(
            io::ErrorKind::BrokenPipe,
            "host generation consumer ended before shutdown",
        ),
    )
}

fn invalid_control_transition(phase: &str, attempt: u32, expected: u32) -> RuntimeError {
    lifecycle_error(
        "macOS guardian generation host transition",
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid {phase} transition for attempt {attempt}; expected {expected}"),
        ),
    )
}

pub(super) fn runtime_io_kind(error: &RuntimeError) -> Option<io::ErrorKind> {
    match error {
        RuntimeError::Spawn(source) | RuntimeError::Lifecycle { source, .. } => Some(source.kind()),
        RuntimeError::CrashLoop { .. }
        | RuntimeError::ChildCrashed { .. }
        | RuntimeError::GuardianExited { .. } => None,
    }
}
