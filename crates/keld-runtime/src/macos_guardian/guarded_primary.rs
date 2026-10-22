//! Host-owned persistent guardian and exact registered-generation lifecycle.
use super::generation_host::{
    GenerationHostAdmission, is_initial_recovery_denied, is_post_shutdown_spawn_refused,
};
use super::process_group::{
    GroupIdentity, require_attested_group_absence, terminate_attested_group_until,
};
#[cfg(test)]
use super::terminate_registered_group;
use super::{
    Arc, BoundRoleGeneration, Child, ChildStdin, ChildStdout, Duration, ExitStatus,
    GENERATION_CONTROL_POLL, GUARDIAN_SHUTDOWN_DEADLINE, Instant, JoinHandle, Mutex, Receiver,
    RecoveryState, RoleEvent, RuntimeError, SUPERVISED_ORDERLY_SHUTDOWN, SUPERVISED_QUIT_ACCEPTED,
    Write, collapse_failures, io, lifecycle_error, lock_recovery, mpsc,
    read_supervised_orderly_ack, read_supervised_quit_ack, reject_host_registration,
    run_generation_host, set_liveness_nonblocking, thread,
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
    pub(super) native_signal: NativeSignalAuthority,
}

/// Host-side signal authority carried WITH one retained generation.
///
/// One native group signal per generation. A retained birth is custody, not
/// repeated signal authority: the numeric PGID may be recycled once the
/// leader is gone, and after a delivered SIGKILL a second group-wide kill
/// could only reach processes that joined later or a recycled group. So the
/// first host-side attempt spends the authority whether it was delivered or
/// failed. Later host paths keep custody (fail closed) and may only run the
/// read-only dual-ESRCH absence probe, never `killpg`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum NativeSignalAuthority {
    /// No host-side signal has been attempted for this generation.
    Armed,
    /// The one signal was delivered; the group is not yet proven absent.
    Delivered {
        /// Absolute end of the attempt's reap budget.
        reap_deadline: Instant,
    },
    /// The one attempt failed or was refused.
    ProbeOnly {
        /// Absolute end of the attempt's reap budget.
        reap_deadline: Instant,
    },
}

impl NativeSignalAuthority {
    /// Absolute deadline for a later read-only absence probe. Probes share
    /// the spent attempt's budget; once it has passed, a call still gets one
    /// observation (at most one poll interval) so a group that has since
    /// disappeared can be released, but never a fresh reap window.
    fn probe_deadline(self) -> Option<Instant> {
        match self {
            Self::Armed => None,
            Self::Delivered { reap_deadline } | Self::ProbeOnly { reap_deadline } => {
                Some(reap_deadline.max(Instant::now() + GENERATION_CONTROL_POLL))
            }
        }
    }
}

impl RegisteredPrimaryGeneration {
    pub(super) const fn authenticated(attempt: u32, group_identity: GroupIdentity) -> Self {
        Self {
            attempt,
            group_pid: group_identity.pid,
            group_identity: Some(group_identity),
            native_signal: NativeSignalAuthority::Armed,
        }
    }

    /// Makes the generation's single birth-attested signal attempt while
    /// armed, and spends the authority whatever its outcome. When the
    /// authority is already spent this sends nothing and returns `Ok`, so
    /// the caller applies its own read-only absence proof.
    pub(super) fn signal_if_armed(&mut self) -> Result<(), RuntimeError> {
        let Some(identity) = self.group_identity else {
            return terminate_unwitnessed_generation(self.group_pid);
        };
        if self.native_signal != NativeSignalAuthority::Armed {
            return Ok(());
        }
        let reap_deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE / 4;
        let result = terminate_attested_group_until(identity, reap_deadline);
        self.native_signal = if result.is_ok() {
            NativeSignalAuthority::Delivered { reap_deadline }
        } else {
            NativeSignalAuthority::ProbeOnly { reap_deadline }
        };
        result
    }

    /// Host fail-safe for this retained generation: the single armed signal
    /// attempt, or afterwards only the read-only absence probe.
    pub(super) fn terminate(&mut self) -> Result<(), RuntimeError> {
        match (self.native_signal.probe_deadline(), self.group_identity) {
            (Some(deadline), Some(identity)) => require_attested_group_absence(identity, deadline),
            _ => self.signal_if_armed(),
        }
    }

    /// Core's app-link failure report. A repeated report after a delivered
    /// signal is idempotent: the Guardian's CLEAR, not a second signal,
    /// proves absence. A failed attempt still reports its probe result.
    fn fail(&mut self) -> Result<(), RuntimeError> {
        match self.native_signal {
            NativeSignalAuthority::Delivered { .. } => Ok(()),
            _ => self.terminate(),
        }
    }

    /// CLEAR's absence deadline. After an armed or delivered signal CLEAR
    /// keeps its own `own` window: CLEAR's own `signal_if_armed` turns
    /// `Armed` into `Delivered { now + D/4 }` before this deadline is
    /// chosen, so capping at that budget would shrink CLEAR's own window
    /// from D to D/4 and fail a group that is still draining when CLEAR
    /// arrives. CLEAR runs at most once per attempt and any failure ends the
    /// worker, so the window is never stacked. After a FAILED attempt CLEAR
    /// is bounded by that attempt's already-spent deadline, plus at most one
    /// poll interval of grace.
    pub(super) fn clear_absence_deadline(&self, own: Instant) -> Instant {
        match self.native_signal {
            NativeSignalAuthority::ProbeOnly { .. } => self
                .native_signal
                .probe_deadline()
                .map_or(own, |shared| shared.min(own)),
            _ => own,
        }
    }
}

fn terminate_unwitnessed_generation(group_pid: u32) -> Result<(), RuntimeError> {
    #[cfg(test)]
    {
        // Older synthetic-only control/error fixtures explicitly supply no
        // kernel witness. They never stand in for production enrollment.
        terminate_registered_group(group_pid)
    }
    #[cfg(not(test))]
    {
        let _ = group_pid;
        Err(lifecycle_error(
            "macOS guardian missing native generation identity",
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "authenticated generation has no registered leader birth proof",
            ),
        ))
    }
}

// One native fail-safe owner for every terminal path. Hold the generation
// mutex across the birth-attested signal, just like fail_current_generation
// and KGC1 CLEAR. A failed signal cannot consume the original group witness;
// only successful termination or proven absence releases this enrollment.
fn terminate_and_release_generation(
    registered: &mut Option<RegisteredPrimaryGeneration>,
) -> Result<(), RuntimeError> {
    if let Some(generation) = registered.as_mut() {
        generation.terminate()?;
        *registered = None;
    }
    Ok(())
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
        let mut current = lock_generation(&self.current_generation);
        let Some(generation) = current.as_mut() else {
            return Ok(());
        };
        if generation.attempt != attempt {
            return Ok(());
        }
        generation.fail()
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
                let (group_pid, cleanup_error) = self.fatal_group_cleanup_status(true);
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
            // A later call cannot turn the original failed shutdown into
            // success, even when native failure retained the group witness.
            self.shutdown_failed = true;
        }
        result
    }

    /// Performs exactly one owned OS/worker teardown transaction; the public
    /// wrapper retains a failed terminal outcome across later calls.
    fn shutdown_once(&mut self) -> Result<ExitStatus, RuntimeError> {
        let mut failures = Vec::new();
        // One shutdown transaction makes at most one native fail-safe call:
        // a repeated call would re-report the same error and re-wait.
        let mut native_attempted = false;
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
                    native_attempted = true;
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
                    native_attempted = true;
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
            let (group_pid, cleanup_error) = self.fatal_group_cleanup_status(!native_attempted);
            native_attempted = true;
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
        if (worker_failed || group_still_registered) && !native_attempted {
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
        // was signalled and the lease revoked, and only an orderly S (KSA, not
        // an accepted Quit's KQA) may retire it. The Guardian's own exit
        // status still proves its local retirement. Never downgrade another
        // worker failure.
        let reconciled = |error: &RuntimeError| {
            shutdown_won
                && ((denied_before_initial_ready && is_initial_recovery_denied(error))
                    || (orderly_acknowledged && denied && is_post_shutdown_spawn_refused(error)))
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

    /// Group PID and cleanup outcome for a `GuardianExited` report. When
    /// this transaction already ran the native fail-safe, never call it
    /// again: report the retained group without re-waiting or duplicating
    /// the earlier native error.
    fn fatal_group_cleanup_status(&self, attempt: bool) -> (u32, Option<io::Error>) {
        let mut current = lock_generation(&self.current_generation);
        let group_pid = current
            .as_ref()
            .map_or(0, |generation| generation.group_pid);
        let cleanup_error = if attempt {
            terminate_and_release_generation(&mut current)
                .err()
                .map(|error| io::Error::other(error.to_string()))
        } else {
            current.is_some().then(|| {
                io::Error::other(
                    "native group cleanup failed earlier in this shutdown; the original group birth remains retained",
                )
            })
        };
        (group_pid, cleanup_error)
    }

    fn fail_safe_current_group(&self) -> Result<(), RuntimeError> {
        let mut current = lock_generation(&self.current_generation);
        terminate_and_release_generation(&mut current)
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

impl Drop for GuardedPrimary {
    fn drop(&mut self) {
        if self.liveness_writer.is_none() {
            return;
        }
        let _ = self.shutdown();
    }
}
