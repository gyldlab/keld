//! Host-owned persistent guardian and exact registered-generation lifecycle.
use super::generation_host::{
    GenerationHostAdmission, is_initial_recovery_denied, is_post_shutdown_spawn_refused,
};
use super::process_group::{GroupIdentity, terminate_attested_group};
#[cfg(test)]
use super::terminate_registered_group;
use super::{
    Arc, BoundRoleGeneration, Child, ChildStdin, ChildStdout, Duration, ExitStatus,
    GUARDIAN_SHUTDOWN_DEADLINE, Instant, JoinHandle, Mutex, Receiver, RecoveryState, RoleEvent,
    RuntimeError, SUPERVISED_ORDERLY_SHUTDOWN, SUPERVISED_QUIT_ACCEPTED, Write, collapse_failures,
    io, lifecycle_error, lock_recovery, mpsc, read_supervised_orderly_ack,
    read_supervised_quit_ack, reject_host_registration, run_generation_host,
    set_liveness_nonblocking, thread,
};

/// One host-visible update from the guarded primary-generation owner.
#[derive(Debug)]
pub enum GuardedPrimaryUpdate {
    /// Existing KEL-75 lifecycle event.
    Role(RoleEvent),
    /// Newly authenticated stream for one generation.
    Bound(BoundRoleGeneration),
}

/// Host-owned handle for one persistent guardian and its primary generations.
///
/// The value owns the guardian process, sole liveness writer, current-group
/// fail-safe and host half of KEL-75 generation admission. It exposes no raw
/// control records and no child/process-group mutation API.
#[derive(Debug)]
pub struct GuardedPrimary {
    pub(super) child: Child,
    pub(super) liveness_writer: Option<ChildStdin>,
    pub(super) quit_ack_reader: ChildStdout,
    pub(super) current_generation: Arc<Mutex<Option<RegisteredPrimaryGeneration>>>,
    pub(super) recovery_decision: Arc<Mutex<RecoveryState>>,
    pub(super) updates_rx: Receiver<GuardedPrimaryUpdate>,
    pub(super) worker: Option<JoinHandle<Result<(), RuntimeError>>>,
    pub(super) shutdown_attributed: bool,
    /// An earlier shutdown returned a fatal error after consuming OS/worker
    /// witnesses. Re-reading cached child status must not erase that failure.
    pub(super) shutdown_failed: bool,
    /// The shutdown acknowledgment was the Guardian's KSA for the host's
    /// orderly S, not a KQA for an accepted Quit. Only S may retire a
    /// refused spawn as an orderly cancellation.
    pub(super) orderly_acknowledged: bool,
    #[cfg(test)]
    pub(super) shutdown_sent: Option<mpsc::SyncSender<()>>,
    /// Scoped native test proof: denial lock must remain held until S is
    /// physically written. Never present in the shipping Guardian owner.
    #[cfg(test)]
    pub(super) shutdown_denial_fence: Option<(mpsc::SyncSender<()>, mpsc::Receiver<()>)>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RegisteredPrimaryGeneration {
    pub(super) attempt: u32,
    pub(super) group_pid: u32,
    pub(super) group_identity: Option<GroupIdentity>,
}

impl RegisteredPrimaryGeneration {
    pub(super) const fn authenticated(attempt: u32, group_identity: GroupIdentity) -> Self {
        Self {
            attempt,
            group_pid: group_identity.pid,
            group_identity: Some(group_identity),
        }
    }
}

fn terminate_generation(generation: RegisteredPrimaryGeneration) -> Result<(), RuntimeError> {
    if let Some(identity) = generation.group_identity {
        return terminate_attested_group(identity);
    }
    #[cfg(test)]
    {
        // Older synthetic-only control/error fixtures explicitly supply no
        // kernel witness. They never stand in for production enrollment.
        terminate_registered_group(generation.group_pid)
    }
    #[cfg(not(test))]
    Err(lifecycle_error(
        "macOS guardian missing native generation identity",
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "authenticated generation has no registered leader birth proof",
        ),
    ))
}

impl GuardedPrimary {
    pub(super) fn start(
        mut child: Child,
        liveness_writer: ChildStdin,
        quit_ack_reader: ChildStdout,
        control: std::os::unix::net::UnixStream,
        first_record_deadline: Instant,
        #[cfg(test)] registration_fence: Option<
            super::generation_host::GenerationRegistrationFence,
        >,
    ) -> Result<Self, RuntimeError> {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(RuntimeError::GuardianExited {
                    group_pid: 0,
                    exit_code: status.code(),
                    cleanup_error: None,
                });
            }
            Ok(None) => {}
            Err(source) => {
                return Err(reject_host_registration(
                    child,
                    liveness_writer,
                    lifecycle_error("macOS guardian generation process inspection", source),
                ));
            }
        }
        let current_generation = Arc::new(Mutex::new(None));
        let recovery_decision = Arc::new(Mutex::new(RecoveryState::new()));
        let (updates_tx, updates_rx) = mpsc::channel();
        let generation_for_worker = Arc::clone(&current_generation);
        let recovery_for_worker = Arc::clone(&recovery_decision);
        // Bind every later KGC1 SPAWNED record to this retained OS child.
        // A valid control token alone does not authorize an arbitrary PGID.
        let admission = GenerationHostAdmission::new(child.id(), first_record_deadline);
        let worker = match thread::Builder::new()
            .name("keld-runtime-guardian-generation-host".to_owned())
            .spawn(move || {
                run_generation_host(
                    admission,
                    control,
                    &updates_tx,
                    &generation_for_worker,
                    &recovery_for_worker,
                    #[cfg(test)]
                    None,
                    #[cfg(test)]
                    registration_fence.as_ref(),
                )
            }) {
            Ok(worker) => worker,
            Err(source) => {
                return Err(reject_host_registration(
                    child,
                    liveness_writer,
                    lifecycle_error("macOS guardian generation host worker", source),
                ));
            }
        };
        Ok(Self {
            child,
            liveness_writer: Some(liveness_writer),
            quit_ack_reader,
            current_generation,
            recovery_decision,
            updates_rx,
            worker: Some(worker),
            shutdown_attributed: false,
            shutdown_failed: false,
            orderly_acknowledged: false,
            #[cfg(test)]
            shutdown_sent: None,
            #[cfg(test)]
            shutdown_denial_fence: None,
        })
    }

    /// OS process id of the persistent guardian.
    #[must_use]
    pub fn guardian_pid(&self) -> u32 {
        self.child.id()
    }

    /// Current registered Bun process-group leader, when a generation is live.
    #[must_use]
    pub fn group_pid(&self) -> Option<u32> {
        lock_generation(&self.current_generation).map(|generation| generation.group_pid)
    }

    /// Arms initial Ready, then releases the current retired successor.
    pub fn arm_recovery(&self) {
        lock_recovery(&self.recovery_decision).arm();
    }

    /// Permanently rejects successor preparation for a failed startup.
    pub fn deny_recovery(&self) {
        lock_recovery(&self.recovery_decision).deny();
    }

    /// Reports app-link failure for the named current generation.
    ///
    /// A stale reader is ignored. For the current attempt, runtime signals the
    /// enrolled group; the existing guardian-side Supervisor observes that
    /// process failure, performs KEL-75 revocation, and applies its one restart
    /// policy. Core never signals or restarts a process itself.
    ///
    /// # Errors
    ///
    /// Returns the owning group-signal failure.
    pub fn fail_current_generation(&self, attempt: u32) -> Result<(), RuntimeError> {
        // The generation lock remains held through kernel identity inspection
        // and signaling. Otherwise concurrent KGC1 CLEAR could retire this
        // attempt and publish a successor between the host's read and SIGKILL.
        let current = lock_generation(&self.current_generation);
        let Some(generation) = *current else {
            return Ok(());
        };
        if generation.attempt != attempt {
            return Ok(());
        }
        terminate_generation(generation)
    }

    /// Waits for one generation update up to `timeout`.
    #[must_use]
    pub fn recv_update(&self, timeout: Duration) -> Option<GuardedPrimaryUpdate> {
        self.updates_rx.recv_timeout(timeout).ok()
    }

    /// Records accepted host shutdown before a correlated reply is published.
    ///
    /// Use it only for a Quit accepted from a bound generation. The Guardian
    /// refuses Q before its first generation exists; cancel an unbound
    /// startup with [`Self::shutdown`], whose S is acknowledged at every stage.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle error when the guardian cannot acknowledge
    /// attribution while the host still owns the session.
    pub fn accept_shutdown(&mut self) -> Result<(), RuntimeError> {
        // The same guard covers the actual locator/token mint. Release it
        // before Q I/O so the generation owner can finish cleanup concurrently.
        self.deny_recovery();
        let ack_reader = &mut self.quit_ack_reader;
        let writer = self.liveness_writer.as_mut().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian supervised-Quit control",
                io::Error::new(io::ErrorKind::NotConnected, "liveness writer is missing"),
            )
        })?;
        set_liveness_nonblocking(writer, true)?;
        writer
            .write_all(&[SUPERVISED_QUIT_ACCEPTED])
            .and_then(|()| writer.flush())
            .map_err(|source| lifecycle_error("macOS guardian supervised-Quit control", source))?;
        #[cfg(test)]
        if let Some(sent) = &self.shutdown_sent {
            sent.send(()).map_err(|_| {
                lifecycle_error(
                    "macOS guardian test-only shutdown receipt",
                    io::Error::other("native controller stopped before observing Q"),
                )
            })?;
        }
        read_supervised_quit_ack(ack_reader)?;
        self.shutdown_attributed = true;
        lock_recovery(&self.recovery_decision).shutdown_acknowledged = true;
        Ok(())
    }

    /// Polls guardian/control health and invokes the current-group fail-safe
    /// before returning a terminal error.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::GuardianExited`] for guardian death or the
    /// host-worker's exact typed control/admission failure.
    pub fn poll_fatal(&mut self) -> Result<(), RuntimeError> {
        match self.child.try_wait() {
            Ok(None) => {}
            Ok(Some(status)) => {
                self.liveness_writer.take();
                let generation = take_generation(&self.current_generation);
                let group_pid = generation.map_or(0, |generation| generation.group_pid);
                let cleanup_error = generation
                    .and_then(|generation| terminate_generation(generation).err())
                    .map(|error| io::Error::other(error.to_string()));
                Err(RuntimeError::GuardianExited {
                    group_pid,
                    exit_code: status.code(),
                    cleanup_error,
                })?;
            }
            Err(source) => {
                let error = lifecycle_error("macOS guardian generation process inspection", source);
                // An unobservable guardian is a loss of process authority,
                // not a reason to leave the enrolled Bun group running.
                return match self.fail_safe_current_group() {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(collapse_failures(vec![error, cleanup])),
                };
            }
        }
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            let result = self.join_worker_with_shutdown_proof();
            if let Err(error) = result {
                let cleanup = self.fail_safe_current_group().err();
                return match cleanup {
                    Some(cleanup_error) => Err(collapse_failures(vec![error, cleanup_error])),
                    None => Err(error),
                };
            }
        }
        Ok(())
    }

    /// Closes the liveness writer, waits the guardian, and joins generation
    /// control after the guardian has revoked/cleared the current generation.
    ///
    /// # Errors
    ///
    /// Returns a typed failure for timeout, unexpected guardian exit, current
    /// group cleanup failure, or generation worker failure.
    pub fn shutdown(&mut self) -> Result<ExitStatus, RuntimeError> {
        if self.shutdown_failed {
            return Err(lifecycle_error(
                "macOS guardian generation shutdown",
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "an earlier Guardian shutdown failed; terminal failure cannot be retried as success",
                ),
            ));
        }
        let result = self.shutdown_once();
        if result.is_err() {
            // Cleanup consumed the worker and group witnesses; a later call
            // cannot turn the original failed shutdown into fresh success.
            self.shutdown_failed = true;
        }
        result
    }

    /// Performs exactly one owned OS/worker teardown transaction; the public
    /// wrapper retains a failed terminal outcome across later calls.
    fn shutdown_once(&mut self) -> Result<ExitStatus, RuntimeError> {
        let mut failures = Vec::new();
        if let Err(error) = self.prepare_orderly_shutdown() {
            failures.push(error);
        }
        self.liveness_writer.take();
        let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
        let mut status = None;
        loop {
            match self.child.try_wait() {
                Ok(Some(observed)) => {
                    status = Some(observed);
                    break;
                }
                Ok(None) if Instant::now() < deadline => thread::yield_now(),
                Ok(None) => {
                    failures.push(lifecycle_error(
                        "macOS guardian generation shutdown deadline",
                        io::Error::new(io::ErrorKind::TimedOut, "guardian did not exit"),
                    ));
                    if let Err(error) = self.fail_safe_current_group() {
                        failures.push(error);
                    }
                    if let Err(source) = self.child.kill()
                        && source.kind() != io::ErrorKind::InvalidInput
                    {
                        failures.push(lifecycle_error(
                            "macOS guardian generation process kill",
                            source,
                        ));
                    }
                    match self.child.wait() {
                        Ok(observed) => status = Some(observed),
                        Err(source) => failures.push(lifecycle_error(
                            "macOS guardian generation process wait",
                            source,
                        )),
                    }
                    break;
                }
                Err(source) => {
                    failures.push(lifecycle_error(
                        "macOS guardian generation shutdown wait",
                        source,
                    ));
                    if let Err(error) = self.fail_safe_current_group() {
                        failures.push(error);
                    }
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
        let worker_failed = if let Err(error) = self.join_worker_with_shutdown_proof() {
            failures.push(error);
            true
        } else {
            false
        };
        if let Some(observed) = status
            && !observed.success()
        {
            let generation = take_generation(&self.current_generation);
            let group_pid = generation.map_or(0, |generation| generation.group_pid);
            let cleanup_error = generation
                .and_then(|generation| terminate_generation(generation).err())
                .map(|error| io::Error::other(error.to_string()));
            failures.push(RuntimeError::GuardianExited {
                group_pid,
                exit_code: observed.code(),
                cleanup_error,
            });
        }
        let group_still_registered = lock_generation(&self.current_generation).is_some();
        if group_still_registered && !worker_failed {
            failures.push(lifecycle_error(
                "macOS guardian generation shutdown",
                io::Error::other("guardian exited with a registered group"),
            ));
        }
        if worker_failed || group_still_registered {
            // Even a zero-status guardian or apparently successful worker
            // cannot leave a native group behind after the host's shutdown.
            if let Err(error) = self.fail_safe_current_group() {
                failures.push(error);
            }
        }
        if failures.is_empty() {
            status.ok_or_else(|| {
                lifecycle_error(
                    "macOS guardian generation shutdown wait",
                    io::Error::other("guardian ended without an observable status"),
                )
            })
        } else {
            Err(collapse_failures(failures))
        }
    }

    fn prepare_orderly_shutdown(&mut self) -> Result<(), RuntimeError> {
        if self.shutdown_attributed {
            self.deny_recovery();
            return Ok(());
        }
        // Denial is visible to the generation worker only after S has been
        // queued on the existing host-exclusive liveness pipe. Without this
        // one lock, a scheduled gap between deny() and write(S) lets the
        // worker reject initial PREPARE, then the Guardian exits without KSA.
        // Never retain this lock while awaiting KSA or host/Guardian reap.
        let mut recovery = lock_recovery(&self.recovery_decision);
        recovery.deny();
        #[cfg(test)]
        if let Some((arrived, release)) = &self.shutdown_denial_fence {
            arrived.send(()).map_err(|_| {
                lifecycle_error(
                    "test host recovery-denial receipt",
                    io::Error::other("native shutdown observer ended"),
                )
            })?;
            release
                .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
                .map_err(|source| {
                    lifecycle_error(
                        "test host recovery-denial release",
                        io::Error::other(source),
                    )
                })?;
        }
        let writer = self.liveness_writer.as_mut().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian orderly-shutdown control",
                io::Error::new(io::ErrorKind::NotConnected, "liveness writer is missing"),
            )
        })?;
        let ack_reader = &mut self.quit_ack_reader;
        set_liveness_nonblocking(writer, true)?;
        writer
            .write_all(&[SUPERVISED_ORDERLY_SHUTDOWN])
            .and_then(|()| writer.flush())
            .map_err(|source| lifecycle_error("macOS guardian orderly-shutdown control", source))?;
        drop(recovery);
        #[cfg(test)]
        if let Some(sent) = &self.shutdown_sent {
            sent.send(()).map_err(|_| {
                lifecycle_error(
                    "macOS guardian test-only shutdown receipt",
                    io::Error::other("native controller stopped before observing S"),
                )
            })?;
        }
        read_supervised_orderly_ack(ack_reader)?;
        self.shutdown_attributed = true;
        self.orderly_acknowledged = true;
        lock_recovery(&self.recovery_decision).shutdown_acknowledged = true;
        Ok(())
    }

    // The private generation worker may return Ok only after Q/S attribution
    // and the last native process group has been explicitly cleared.
    // Missing either fact is a fatal owner-state mismatch, not a clean EOF.
    fn join_worker_with_shutdown_proof(&mut self) -> Result<(), RuntimeError> {
        let was_running = self.worker.is_some();
        let worker_result = self.join_worker();
        if !was_running {
            return worker_result;
        }
        let orderly_acknowledged = self.orderly_acknowledged;
        let (acknowledged, denied, denied_before_initial_ready) = {
            let recovery = lock_recovery(&self.recovery_decision);
            (
                recovery.shutdown_acknowledged,
                recovery.denied,
                recovery.denied && !recovery.initial_ready && recovery.expected_attempt == 1,
            )
        };
        // The one acknowledgment check. Without an actual KSA/KQA no worker
        // outcome is orderly; a failed worker keeps its own provenance.
        if !acknowledged {
            return Err(worker_result.err().unwrap_or_else(|| {
                lifecycle_error(
                    "macOS guardian generation host worker",
                    io::Error::other("worker ended before acknowledged shutdown"),
                )
            }));
        }
        let shutdown_won =
            self.shutdown_attributed && lock_generation(&self.current_generation).is_none();
        // These are the ONLY expected worker outcomes when the host's own
        // acknowledged shutdown raced generation admission: a rejected first
        // PREPARE after the Guardian's idle probe, or an in-flight SPAWNED
        // refused after S. Each needs zero registered group and its private
        // typed result; the refusal type exists only after the attested group
        // was signalled and the lease revoked. Only an orderly S (KSA, not an
        // accepted Quit's KQA) may retire either one (architecture 06: the
        // first-PREPARE denial is accepted only after verified KSA). The
        // Guardian's own exit status still proves its local retirement.
        // Never downgrade another worker failure.
        let reconciled = |error: &RuntimeError| {
            shutdown_won
                && orderly_acknowledged
                && ((denied_before_initial_ready && is_initial_recovery_denied(error))
                    || (denied && is_post_shutdown_spawn_refused(error)))
        };
        if let Err(error) = worker_result
            && !reconciled(&error)
        {
            return Err(error);
        }
        if lock_generation(&self.current_generation).is_some() {
            return Err(lifecycle_error(
                "macOS guardian generation host worker",
                io::Error::other("worker ended with a registered group"),
            ));
        }
        Ok(())
    }

    pub(super) fn join_worker(&mut self) -> Result<(), RuntimeError> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        worker.join().map_err(|_| {
            lifecycle_error(
                "macOS guardian generation host worker",
                io::Error::other("generation host worker panicked"),
            )
        })?
    }

    fn fail_safe_current_group(&self) -> Result<(), RuntimeError> {
        take_generation(&self.current_generation).map_or(Ok(()), terminate_generation)
    }
}

pub(super) fn lock_generation(
    generation: &Mutex<Option<RegisteredPrimaryGeneration>>,
) -> std::sync::MutexGuard<'_, Option<RegisteredPrimaryGeneration>> {
    match generation.lock() {
        Ok(generation) => generation,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub(super) fn take_generation(
    generation: &Mutex<Option<RegisteredPrimaryGeneration>>,
) -> Option<RegisteredPrimaryGeneration> {
    lock_generation(generation).take()
}

impl Drop for GuardedPrimary {
    fn drop(&mut self) {
        if self.liveness_writer.is_none() {
            return;
        }
        let _ = self.shutdown();
    }
}
