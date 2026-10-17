//! Supervised child execution and authenticated private group registration.
use super::generation_host::runtime_io_kind;
use super::host_liveness::{
    observe_failed_initial_preparation_shutdown, observe_pre_generation_shutdown,
};
use super::{
    AsFd, CapturedOutput, Command, CrashLedger, Duration, GENERATION_CONTROL_POLL,
    GenerationControlPeer, GenerationControlReader, GuardedPrimaryPreparer, REGISTRATION_ENV,
    REGISTRATION_LEN, REGISTRATION_MAGIC, Read, RestartPolicy, RuntimeError,
    SupervisedGuardianPreparer, Supervisor, SupervisorOutcome, Write,
    await_guarded_primary_liveness, collapse_failures, handshake_client, io, lifecycle_error,
    observe_supervised_liveness, parse_app_link, set_liveness_nonblocking,
    validate_liveness_bootstrap,
};
#[cfg(test)]
use super::{
    CommandExt, GuardianReport, Stdio, await_host_death, terminate_registered_group,
    validate_group_pid,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub(super) fn run_supervised_with_registration<R, W, C, A, S, F>(
    mut liveness: R,
    registration: W,
    mut quit_ack: A,
    command_factory: C,
    child_spawned: S,
    revoke_registered_resources: F,
) -> Result<(u32, CrashLedger, CapturedOutput), RuntimeError>
where
    R: Read + AsFd,
    W: Write + Send + 'static,
    C: FnOnce() -> Result<Command, RuntimeError> + Send + 'static,
    A: Write,
    S: FnOnce(u32) -> io::Result<()> + Send + 'static,
    F: FnOnce() -> io::Result<()> + Send + 'static,
{
    if let Err(error) = validate_liveness_bootstrap(&mut liveness) {
        return Err(fail_before_child(error, revoke_registered_resources));
    }
    let (pid_tx, pid_rx) = std::sync::mpsc::sync_channel(1);
    let preparer = SupervisedGuardianPreparer {
        command_factory: Some(command_factory),
        registration: Some(registration),
        child_spawned: Some(child_spawned),
        revoke_registered_resources: Some(revoke_registered_resources),
        pid_tx,
    };
    let supervisor = Supervisor::start_prepared(
        RestartPolicy {
            max_crashes: 1,
            window_secs: 30,
        },
        preparer,
    )?;
    let leader_pid = pid_rx.recv().map_err(|_| {
        lifecycle_error(
            "macOS guardian supervised-child registration",
            io::Error::other("supervisor ended before registering its initial child"),
        )
    })?;

    set_liveness_nonblocking(&liveness, true)?;
    let mut liveness_result = Ok(());
    let mut quit_accepted = false;
    loop {
        match observe_supervised_liveness(
            &mut liveness,
            &supervisor,
            &mut quit_accepted,
            &mut quit_ack,
        ) {
            Ok(true) => break,
            Ok(false) => {}
            Err(error) => {
                liveness_result = Err(error);
                supervisor.shutdown();
                break;
            }
        }
        if let Some(event) = supervisor.recv_event(Duration::from_millis(20))
            && matches!(
                event,
                crate::SupervisorEvent::Exited { .. }
                    | crate::SupervisorEvent::RespawnFailed
                    | crate::SupervisorEvent::Failed { .. }
                    | crate::SupervisorEvent::CrashLoopTripped
                    | crate::SupervisorEvent::Stopped
            )
        {
            if let Err(error) = observe_supervised_liveness(
                &mut liveness,
                &supervisor,
                &mut quit_accepted,
                &mut quit_ack,
            ) {
                liveness_result = Err(error);
                supervisor.shutdown();
            }
            break;
        }
    }
    let outcome = supervisor.wait_for_outcome();
    let crash_ledger = supervisor.crash_ledger();
    let output = supervisor.output();
    drop(supervisor);

    let mut failures = Vec::new();
    if let Err(error) = liveness_result {
        failures.push(error);
    }
    match outcome {
        SupervisorOutcome::CrashLoop(error) | SupervisorOutcome::Failed(error) => {
            failures.push(error);
        }
        SupervisorOutcome::Stopped => {
            if let Some(termination) = crash_ledger.last_self_termination {
                failures.push(RuntimeError::ChildCrashed {
                    pid: termination.pid,
                    exit_code: termination.exit_code,
                    stderr_tail: output.stderr_tail(2_000),
                });
            }
        }
    }
    if !failures.is_empty() {
        return Err(collapse_failures(failures));
    }
    Ok((leader_pid, crash_ledger, output))
}

#[cfg(test)]
pub(super) fn run_with_registration<R, W, S, F>(
    mut command: Command,
    mut liveness: R,
    mut registration: W,
    child_spawned: S,
    revoke_registered_resources: F,
) -> Result<GuardianReport, RuntimeError>
where
    R: Read + AsFd,
    W: Write,
    S: FnOnce(u32) -> io::Result<()>,
    F: FnOnce() -> io::Result<()>,
{
    if command.get_program().is_empty() {
        let error = lifecycle_error(
            "macOS guardian bootstrap",
            io::Error::new(io::ErrorKind::InvalidInput, "child program is empty"),
        );
        return Err(fail_before_child(error, revoke_registered_resources));
    }

    if let Err(error) = validate_liveness_bootstrap(&mut liveness) {
        return Err(fail_before_child(error, revoke_registered_resources));
    }

    command
        .env_remove(REGISTRATION_ENV)
        .process_group(0)
        .stdin(Stdio::null());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(source) => {
            return Err(fail_before_child(
                RuntimeError::Spawn(source),
                revoke_registered_resources,
            ));
        }
    };
    let leader_pid = child.id();

    let registration_result = validate_group_pid(leader_pid)
        .map(|_| ())
        .and_then(|()| write_group_registration(&mut registration, leader_pid));
    if let Err(error) = registration_result {
        let mut failures = vec![error];
        if let Err(source) = revoke_registered_resources() {
            failures.push(lifecycle_error(
                "macOS guardian registered-resource revocation",
                source,
            ));
        }
        if let Err(error) = terminate_registered_group(leader_pid) {
            failures.push(error);
        }
        if let Err(source) = child.wait() {
            failures.push(lifecycle_error("macOS guardian direct-child wait", source));
        }
        return Err(collapse_failures(failures));
    }

    if let Err(source) = child_spawned(leader_pid) {
        let mut failures = vec![lifecycle_error(
            "macOS guardian local registration observer",
            source,
        )];
        if let Err(source) = revoke_registered_resources() {
            failures.push(lifecycle_error(
                "macOS guardian registered-resource revocation",
                source,
            ));
        }
        if let Err(error) = terminate_registered_group(leader_pid) {
            failures.push(error);
        }
        if let Err(source) = child.wait() {
            failures.push(lifecycle_error("macOS guardian direct-child wait", source));
        }
        return Err(collapse_failures(failures));
    }

    let liveness_result = await_host_death(&mut liveness);
    let revocation_result = revoke_registered_resources()
        .map_err(|source| lifecycle_error("macOS guardian registered-resource revocation", source));
    let signal_result = terminate_registered_group(leader_pid);
    let wait_result = child.wait();
    let mut failures = Vec::new();
    if let Err(error) = liveness_result {
        failures.push(error);
    }
    if let Err(error) = revocation_result {
        failures.push(error);
    }
    if let Err(error) = signal_result {
        failures.push(error);
    }
    let leader_status = match wait_result {
        Ok(status) => status,
        Err(source) => {
            failures.push(lifecycle_error("macOS guardian direct-child wait", source));
            return Err(collapse_failures(failures));
        }
    };
    if !failures.is_empty() {
        return Err(collapse_failures(failures));
    }
    Ok(GuardianReport {
        leader_pid,
        leader_status,
    })
}

pub(super) fn connect_guardian_registration() -> Result<std::os::unix::net::UnixStream, RuntimeError>
{
    let link = std::env::var(REGISTRATION_ENV).map_err(|source| {
        lifecycle_error(
            "macOS guardian private registration bootstrap",
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{REGISTRATION_ENV} is unavailable: {source}"),
            ),
        )
    })?;
    let (endpoint, token) = parse_app_link(&link).map_err(|source| {
        lifecycle_error(
            "macOS guardian private registration bootstrap",
            io::Error::new(io::ErrorKind::InvalidInput, source.to_string()),
        )
    })?;
    let mut stream = std::os::unix::net::UnixStream::connect(endpoint)
        .map_err(|source| lifecycle_error("macOS guardian private registration connect", source))?;
    handshake_client(&mut stream, &token).map_err(|source| {
        lifecycle_error(
            "macOS guardian private registration authentication",
            io::Error::other(source.to_string()),
        )
    })?;
    Ok(stream)
}

pub(super) fn write_group_registration(
    writer: &mut impl Write,
    group_pid: u32,
) -> Result<(), RuntimeError> {
    let mut record = [0_u8; REGISTRATION_LEN];
    record[..4].copy_from_slice(&REGISTRATION_MAGIC);
    record[4..].copy_from_slice(&group_pid.to_be_bytes());
    writer.write_all(&record).map_err(|source| {
        lifecycle_error("macOS guardian authenticated group registration", source)
    })?;
    writer.flush().map_err(|source| {
        lifecycle_error(
            "macOS guardian authenticated group registration flush",
            source,
        )
    })
}

/// Runs a persistent macOS guardian whose single [`Supervisor`] requests a
/// fresh host-owned primary generation before every Bun spawn.
///
/// The authenticated guardian-registration stream becomes a bounded private
/// generation-control channel. The host remains the endpoint/token/stream
/// owner; the guardian remains the only Bun child, process-group, restart and
/// reap owner. Public kipc frame bytes are unchanged.
///
/// # Errors
///
/// Returns [`RuntimeError`] when either private bootstrap is invalid, the host
/// rejects a generation transition, the supervisor reaches a terminal
/// failure, or an unrequested status-zero termination stops supervision.
pub fn run_guarded_primary<R, C, A>(
    mut liveness: R,
    command_factory: C,
    mut quit_ack: A,
) -> Result<CapturedOutput, RuntimeError>
where
    R: Read + AsFd,
    C: FnMut(&str) -> Result<Command, RuntimeError> + Send + 'static,
    A: Write,
{
    // Validate the host-exclusive liveness pipe BEFORE authenticating the
    // Guardian registration stream. Once authentication completes, the host
    // is allowed to send Q/S and close its writer at any moment. The old
    // ordering treated a legitimate early Q/S as malformed bootstrap data
    // and could turn concurrent startup/shutdown into a spurious failure.
    validate_liveness_bootstrap(&mut liveness)?;
    let control = connect_guardian_registration()?;
    #[cfg(test)]
    super::tests::bootstrap_shutdown_race::hold_guardian_after_authenticated_control()?;
    control
        .set_read_timeout(Some(GENERATION_CONTROL_POLL))
        .map_err(|source| lifecycle_error("macOS guardian generation control deadline", source))?;
    control
        .set_write_timeout(Some(GENERATION_CONTROL_POLL))
        .map_err(|source| lifecycle_error("macOS guardian generation control deadline", source))?;
    if observe_pre_generation_shutdown(&mut liveness, &mut quit_ack)? {
        // The authenticated Host requested shutdown before any generation
        // could execute Bun. KSA and actual host EOF are already observed.
        return Ok(CapturedOutput::default());
    }
    #[cfg(test)]
    super::tests::bootstrap_shutdown_race::hold_guardian_after_idle_probe()?;
    let host_unavailable = Arc::new(AtomicBool::new(false));
    let preparation_cancelled = Arc::new(AtomicBool::new(false));
    let initial_prepared = Arc::new(AtomicBool::new(false));
    let supervisor = match Supervisor::start_prepared(
        RestartPolicy::default(),
        GuardedPrimaryPreparer {
            command_factory,
            control: Arc::new(Mutex::new(GenerationControlPeer {
                stream: control,
                reader: GenerationControlReader::new(),
            })),
            host_unavailable: Arc::clone(&host_unavailable),
            preparation_cancelled: Arc::clone(&preparation_cancelled),
            initial_prepared: Arc::clone(&initial_prepared),
        },
    ) {
        Ok(supervisor) => supervisor,
        Err(startup_error) => {
            // The first startup result is synchronous. On Err, no initial
            // child was handed to the supervisor and Bun has not executed.
            // A concurrent real host S wins its pre-generation shutdown
            // attribution; a bare host EOF or unrelated startup failure
            // must not forge KSA or turn a genuine error into success.
            // Only host-rejected first PREPARE has this EOF shape before a
            // genuine PREPARED response. Once PREPARED was admitted, a local
            // factory/codec/spawn failure must remain fatal even if the host
            // concurrently sent S; a KSA there would mask real startup loss.
            let host_rejected_first_prepare = !initial_prepared.load(Ordering::Acquire)
                && matches!(
                    runtime_io_kind(&startup_error),
                    Some(io::ErrorKind::UnexpectedEof)
                );
            if host_rejected_first_prepare {
                match observe_failed_initial_preparation_shutdown(&mut liveness, &mut quit_ack) {
                    Ok(true) => return Ok(CapturedOutput::default()),
                    Ok(false) => {}
                    Err(control_error) => {
                        // Malformed Q, duplicate S, acknowledgment I/O failure
                        // or an unclosed writer is an independent fault.
                        // Preserve the original authenticated KGC1 failure;
                        // never overwrite its security/protocol provenance.
                        return Err(collapse_failures(vec![startup_error, control_error]));
                    }
                }
            }
            return Err(startup_error);
        }
    };

    set_liveness_nonblocking(&liveness, true)?;
    let (liveness_result, quit_accepted, host_dead) = await_guarded_primary_liveness(
        &mut liveness,
        &supervisor,
        &mut quit_ack,
        &host_unavailable,
        &preparation_cancelled,
    );

    let outcome = supervisor.wait_for_outcome();
    let crash_ledger = supervisor.crash_ledger();
    let output = supervisor.output();
    let mut failures = Vec::new();
    if let Err(error) = liveness_result {
        failures.push(error);
    }
    match outcome {
        SupervisorOutcome::CrashLoop(error) | SupervisorOutcome::Failed(error) => {
            failures.push(error);
        }
        SupervisorOutcome::Stopped if !quit_accepted && !host_dead => {
            if let Some(termination) = crash_ledger.last_self_termination {
                failures.push(RuntimeError::ChildCrashed {
                    pid: termination.pid,
                    exit_code: termination.exit_code,
                    stderr_tail: output.stderr_tail(2_000),
                });
            } else {
                failures.push(lifecycle_error(
                    "macOS guardian generation supervisor stop",
                    io::Error::other("supervisor stopped without owned shutdown attribution"),
                ));
            }
        }
        SupervisorOutcome::Stopped => {}
    }
    if failures.is_empty() {
        Ok(output)
    } else {
        Err(collapse_failures(failures))
    }
}

pub(super) fn fail_before_child(
    error: RuntimeError,
    revoke_registered_resources: impl FnOnce() -> io::Result<()>,
) -> RuntimeError {
    let mut failures = vec![error];
    if let Err(source) = revoke_registered_resources() {
        failures.push(lifecycle_error(
            "macOS guardian pre-child registered-resource revocation",
            source,
        ));
    }
    collapse_failures(failures)
}
