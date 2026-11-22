//! Supervised child execution and authenticated private group registration.
use super::generation_host::runtime_io_kind;
use super::host_liveness::{
    observe_failed_initial_preparation_shutdown, observe_pre_generation_shutdown,
};
use super::{
    AsFd, CapturedOutput, Command, CrashLedger, Duration, GENERATION_CONTROL_POLL,
    GenerationControlPeer, GenerationControlReader, GuardedPrimaryPreparer, GuardianFailureSite,
    GuardianPhase, GuardianPhaseLedger, REGISTRATION_ENV, REGISTRATION_LEN, REGISTRATION_MAGIC,
    Read, RestartPolicy, RuntimeError, SupervisedGuardianPreparer, Supervisor, SupervisorOutcome,
    Write, await_guarded_primary_liveness, collapse_failures, handshake_client, io,
    lifecycle_error, observe_supervised_liveness, parse_app_link, set_liveness_nonblocking,
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
/// reap owner. Public kipc frame bytes are unchanged. `select` runs once after
/// authenticated bootstrap and the pre-generation shutdown check. The returned
/// command factory owns its fixed selection across every generation.
///
/// # Errors
///
/// Returns [`RuntimeError`] when either private bootstrap is invalid, the host
/// rejects a generation transition, the supervisor reaches a terminal
/// failure, or an unrequested status-zero termination stops supervision.
pub fn run_guarded_primary<R, S, C, A>(
    mut liveness: R,
    select: S,
    mut quit_ack: A,
) -> Result<CapturedOutput, RuntimeError>
where
    R: Read + AsFd,
    S: FnOnce() -> Result<C, RuntimeError>,
    C: FnMut(&str) -> Result<Command, RuntimeError> + Send + 'static,
    A: Write,
{
    // Validate the host-exclusive liveness pipe BEFORE authenticating the
    // Guardian registration stream. Once authentication completes, the host
    // is allowed to send Q/S and close its writer at any moment. The old
    // ordering treated a legitimate early Q/S as malformed bootstrap data
    // and could turn concurrent startup/shutdown into a spurious failure.
    validate_liveness_bootstrap(&mut liveness)?;
    let phase_ledger = Arc::new(GuardianPhaseLedger::new());
    let control = connect_guardian_registration().inspect_err(|_| {
        report_guardian_phase_failure(&phase_ledger, GuardianFailureSite::Registration);
    })?;
    phase_ledger.record(GuardianPhase::Kgr1Authenticated);
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
    let command_factory = select()?;
    let host_unavailable = Arc::new(AtomicBool::new(false));
    let preparation_cancelled = Arc::new(AtomicBool::new(false));
    let initial_prepared = Arc::new(AtomicBool::new(false));
    let refused_registration_retired = Arc::new(AtomicBool::new(false));
    let supervisor = match Supervisor::start_prepared(
        RestartPolicy::default(),
        GuardedPrimaryPreparer {
            command_factory,
            control: Arc::new(Mutex::new(GenerationControlPeer {
                stream: control,
                reader: GenerationControlReader::new(),
            })),
            phase_ledger: Arc::clone(&phase_ledger),
            host_unavailable: Arc::clone(&host_unavailable),
            preparation_cancelled: Arc::clone(&preparation_cancelled),
            initial_prepared: Arc::clone(&initial_prepared),
            refused_registration_retired: Arc::clone(&refused_registration_retired),
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
                        report_guardian_phase_failure(
                            &phase_ledger,
                            GuardianFailureSite::InitialPreparation,
                        );
                        return Err(collapse_failures(vec![startup_error, control_error]));
                    }
                }
            }
            report_guardian_phase_failure(&phase_ledger, GuardianFailureSite::InitialPreparation);
            return Err(startup_error);
        }
    };

    set_liveness_nonblocking(&liveness, true)?;
    let (liveness_result, quit_accepted, orderly_requested, host_dead) =
        await_guarded_primary_liveness(
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
    let retired_refusal =
        liveness_result.is_ok() && refused_registration_retired.load(Ordering::Acquire);
    if let Err(error) = liveness_result {
        failures.push(error);
    }
    failures.extend(supervisor_outcome_failures(
        outcome,
        &crash_ledger,
        &output,
        HostAttribution {
            quit_accepted,
            orderly_requested,
            host_dead,
        },
        retired_refusal,
        &mut liveness,
        &mut quit_ack,
    ));
    if failures.is_empty() {
        Ok(output)
    } else {
        report_guardian_phase_failure(&phase_ledger, GuardianFailureSite::SupervisorTerminal);
        Err(collapse_failures(failures))
    }
}

fn report_guardian_phase_failure(ledger: &GuardianPhaseLedger, site: GuardianFailureSite) {
    // Purely observational, failure-only, and deliberately independent of
    // Host KELD-CORE-037 attribution or any authenticated Host link state.
    // stderr can be an unread, full pipe. A blocking write would prevent the
    // caller from returning its original Guardian failure. Keep this one
    // diagnostic shorter than POSIX PIPE_BUF and emit it opportunistically.
    const MAX_DIAGNOSTIC_BYTES: usize = 512;
    let mut report = ledger.failure_report(site);
    report.push('\n');
    if report.len() > MAX_DIAGNOSTIC_BYTES {
        return;
    }
    let stderr = io::stderr();
    let fd = stderr.as_fd();
    let Ok(raw_flags) = nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_GETFL) else {
        return;
    };
    let original = nix::fcntl::OFlag::from_bits_truncate(raw_flags);
    let must_restore = !original.contains(nix::fcntl::OFlag::O_NONBLOCK);
    if must_restore
        && nix::fcntl::fcntl(
            fd,
            nix::fcntl::FcntlArg::F_SETFL(original | nix::fcntl::OFlag::O_NONBLOCK),
        )
        .is_err()
    {
        return;
    }
    let _ = nix::unistd::write(fd, report.as_bytes());
    if must_restore {
        let _ = nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_SETFL(original));
    }
}

#[cfg(test)]
mod diagnostic_stderr_tests {
    use super::{GuardianFailureSite, GuardianPhaseLedger, report_guardian_phase_failure};
    use nix::fcntl::{FcntlArg, OFlag, fcntl};
    use nix::unistd::{pipe, write};
    use std::io::Read as _;
    use std::os::fd::AsFd as _;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const CHILD_FLAG: &str = "KELD_W1_GUARDIAN_DIAGNOSTIC_FIXTURE";
    const CHILD_RETURNED: &str = "KELD_W1_DIAGNOSTIC_RETURNED";

    #[derive(Clone, Copy, Debug)]
    enum StderrPipe {
        Full,
        Drained,
        ClosedReader,
    }

    #[test]
    fn guardian_stderr_diagnostic_fixture() {
        if std::env::var_os(CHILD_FLAG).is_none() {
            return;
        }
        let stderr = std::io::stderr();
        let original = fcntl(stderr.as_fd(), FcntlArg::F_GETFL).expect("stderr flags");
        report_guardian_phase_failure(
            &GuardianPhaseLedger::new(),
            GuardianFailureSite::Registration,
        );
        let restored = fcntl(stderr.as_fd(), FcntlArg::F_GETFL).expect("restored stderr flags");
        // Darwin can set an unrelated 0x10000 status bit after any actual
        // write. Test the mutable blocking contract instead of that side effect.
        assert_eq!(
            restored & OFlag::O_NONBLOCK.bits(),
            original & OFlag::O_NONBLOCK.bits(),
            "failure report must restore inherited stderr blocking mode"
        );
        println!("{CHILD_RETURNED}");
    }

    fn diagnostic_subprocess(case: StderrPipe) -> String {
        let (reader, writer) = pipe().expect("real guardian stderr pipe");
        if matches!(case, StderrPipe::Full) {
            let raw_flags = fcntl(writer.as_fd(), FcntlArg::F_GETFL).expect("pipe flags");
            let original = OFlag::from_bits_truncate(raw_flags);
            fcntl(
                writer.as_fd(),
                FcntlArg::F_SETFL(original | OFlag::O_NONBLOCK),
            )
            .expect("make test fill nonblocking");
            let bytes = [b'X'; 4_096];
            loop {
                match write(&writer, &bytes) {
                    Ok(n) => assert!(n > 0, "pipe fill must progress"),
                    Err(nix::errno::Errno::EAGAIN) => break,
                    Err(error) => panic!("pipe fill failed: {error}"),
                }
            }
            fcntl(writer.as_fd(), FcntlArg::F_SETFL(original)).expect("give child blocking stderr");
        }
        let reader = if matches!(case, StderrPipe::ClosedReader) {
            drop(reader);
            None
        } else {
            Some(reader)
        };

        let mut child = Command::new(std::env::current_exe().expect("libtest executable"))
            .arg("guardian_stderr_diagnostic_fixture")
            .arg("--nocapture")
            .env(CHILD_FLAG, "1")
            .stderr(Stdio::from(writer))
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn isolated guardian stderr probe");
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().expect("observe diagnostic subprocess") {
                break status;
            }
            if start.elapsed() > Duration::from_secs(3) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("guardian diagnostic blocked or failed to exit: {case:?}");
            }
            std::thread::yield_now();
        };
        assert!(
            status.success(),
            "diagnostic subprocess failed ({case:?}): {status}"
        );
        let mut stdout = String::new();
        child
            .stdout
            .take()
            .expect("owned stdout")
            .read_to_string(&mut stdout)
            .expect("read child completion marker");
        assert!(
            stdout.contains(CHILD_RETURNED),
            "report did not return: {stdout}"
        );
        let mut stderr = String::new();
        if let Some(reader) = reader {
            std::fs::File::from(reader)
                .read_to_string(&mut stderr)
                .expect("read retained stderr pipe after child exit");
        }
        stderr
    }

    #[test]
    fn full_unread_stderr_pipe_cannot_block_guardian_failure_return() {
        let stderr = diagnostic_subprocess(StderrPipe::Full);
        assert!(
            !stderr.contains("GH1404_GUARDIAN_PHASE"),
            "full stderr pipe must not block or report partial diagnostics"
        );
    }

    #[test]
    fn drained_stderr_pipe_keeps_guardian_failure_diagnostic() {
        let stderr = diagnostic_subprocess(StderrPipe::Drained);
        assert!(
            stderr.contains("GH1404_GUARDIAN_PHASE failure-only=true scope=guardian_registration"),
            "drained diagnostic disappeared: {stderr}"
        );
    }

    #[test]
    fn broken_stderr_pipe_does_not_replace_guardian_failure() {
        diagnostic_subprocess(StderrPipe::ClosedReader);
    }
}

/// Authenticated host shutdown attribution observed by the liveness loop.
#[derive(Clone, Copy)]
struct HostAttribution {
    quit_accepted: bool,
    orderly_requested: bool,
    host_dead: bool,
}

fn supervisor_outcome_failures(
    outcome: SupervisorOutcome,
    crash_ledger: &CrashLedger,
    output: &CapturedOutput,
    host: HostAttribution,
    retired_refusal: bool,
    liveness: &mut (impl Read + AsFd),
    quit_ack: &mut impl Write,
) -> Vec<RuntimeError> {
    match outcome {
        // The host refused this generation's in-flight SPAWNED because its
        // authenticated S had already denied recovery, then closed control.
        // `retired_refusal` is set only after the lease retired its
        // unexecuted direct Child (no exec permit was written; killed and
        // reaped), and Supervisor then stops with exactly that Failed result.
        // The host flushed S before closing control, so Failed can beat the
        // liveness loop to it: read it now, exactly like a failed first
        // PREPARE. Accept only an orderly S with its KSA; Q, bare EOF or any
        // other failure stays fatal.
        SupervisorOutcome::Failed(error)
            if retired_refusal
                && !host.quit_accepted
                && (host.orderly_requested || !host.host_dead) =>
        {
            let orderly = if host.orderly_requested {
                // The loop already consumed S and sent KSA. Skip the bounded
                // host-EOF wait the read-after-failure path performs: the
                // refused child never executed, was killed and reaped, and
                // left no process, so containment does not depend on it.
                // The host still owns the next step (it drops its writer
                // after KSA and reaps this Guardian).
                Ok(true)
            } else {
                observe_failed_initial_preparation_shutdown(liveness, quit_ack)
            };
            match orderly {
                Ok(true) => Vec::new(),
                Ok(false) => vec![error],
                // Keep the original KGC1 provenance first.
                Err(control_error) => vec![error, control_error],
            }
        }
        SupervisorOutcome::CrashLoop(error) | SupervisorOutcome::Failed(error) => vec![error],
        SupervisorOutcome::Stopped if !host.quit_accepted && !host.host_dead => {
            if let Some(termination) = crash_ledger.last_self_termination {
                vec![RuntimeError::ChildCrashed {
                    pid: termination.pid,
                    exit_code: termination.exit_code,
                    stderr_tail: output.stderr_tail(2_000),
                }]
            } else {
                vec![lifecycle_error(
                    "macOS guardian generation supervisor stop",
                    io::Error::other("supervisor stopped without owned shutdown attribution"),
                )]
            }
        }
        SupervisorOutcome::Stopped => Vec::new(),
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
