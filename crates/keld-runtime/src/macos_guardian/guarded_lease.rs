//! Fresh-generation child preparation and the exact revocation lease.
use super::process_group::{LocalGroupWitness, retire_own_child_group, terminate_attested_group};
#[cfg(test)]
use super::validate_group_leader;
use super::{
    Arc, AtomicBool, CONTROL_CLEAR, CONTROL_CLEARED, CONTROL_PREPARE, CONTROL_PREPARED,
    CONTROL_REGISTERED, CONTROL_REVOKE, CONTROL_REVOKED, CONTROL_SPAWNED, Child, ChildPreparer,
    Command, CommandExt, GUARDIAN_SHUTDOWN_DEADLINE, GenerationControlPeer,
    GenerationControlRecord, GenerationLease, Instant, Mutex, Ordering, PreparedChild,
    REGISTRATION_ENV, RevocationCause, RuntimeError, SpawnAdmission, Stdio, collapse_failures,
    control_exchange, control_exchange_with_cancellation, io, lifecycle_error,
};

pub(super) struct GuardedPrimaryPreparer<C> {
    pub(super) command_factory: C,
    pub(super) control: Arc<Mutex<GenerationControlPeer>>,
    pub(super) host_unavailable: Arc<AtomicBool>,
    pub(super) preparation_cancelled: Arc<AtomicBool>,
    /// First genuine KGC1 PREPARED was validated. A later local factory or
    /// admission fault must not be mistaken for host-rejected first PREPARE.
    pub(super) initial_prepared: Arc<AtomicBool>,
    /// Set only by a lease whose in-flight SPAWNED the host answered by
    /// closing control, after that lease retired its unexecuted direct Child
    /// (killed and reaped). Supervisor's cloned terminal error loses private
    /// error types, so the Guardian runner reads this exact fact instead.
    pub(super) refused_registration_retired: Arc<AtomicBool>,
}

impl<C> ChildPreparer for GuardedPrimaryPreparer<C>
where
    C: FnMut(&str) -> Result<Command, RuntimeError> + Send + 'static,
{
    type Lease = GuardedPrimaryLease;

    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        let response = control_exchange_with_cancellation(
            &self.control,
            &GenerationControlRecord::new(CONTROL_PREPARE, attempt),
            CONTROL_PREPARED,
            attempt,
            Some((&self.preparation_cancelled, &self.host_unavailable)),
        )?;
        if attempt == 1 {
            self.initial_prepared.store(true, Ordering::Release);
        }
        let app_link = match String::from_utf8(response.payload) {
            Ok(app_link) if !app_link.is_empty() => app_link,
            Ok(_) => {
                let primary = lifecycle_error(
                    "macOS guardian generation preparation",
                    io::Error::new(io::ErrorKind::InvalidData, "prepared app link is empty"),
                );
                return Err(collapse_guarded_preparation_failure(
                    &self.control,
                    attempt,
                    primary,
                ));
            }
            Err(source) => {
                let primary = lifecycle_error(
                    "macOS guardian generation preparation",
                    io::Error::new(io::ErrorKind::InvalidData, source),
                );
                return Err(collapse_guarded_preparation_failure(
                    &self.control,
                    attempt,
                    primary,
                ));
            }
        };
        // The factory MUST NOT install its own `pre_exec` hook. std runs such
        // hooks in registration order, so one added here would run BEFORE
        // the SpawnAdmission gate below, could fork, and would void the
        // "a refused child is its whole process group" fact that lets a
        // refused spawn be retired by its exact Child alone. std offers no
        // way to count installed hooks, so this is a review-enforced rule.
        let mut command = match (self.command_factory)(&app_link) {
            Ok(command) => command,
            Err(primary) => {
                return Err(collapse_guarded_preparation_failure(
                    &self.control,
                    attempt,
                    primary,
                ));
            }
        };
        command
            .env_remove(REGISTRATION_ENV)
            .process_group(0)
            .stdin(Stdio::null());
        let registration_control = Arc::clone(&self.control);
        let group_witness = LocalGroupWitness::default();
        let reporter_group_witness = group_witness.clone();
        let admission = SpawnAdmission::install(&mut command, move |pid| {
            reporter_group_witness.register_child(pid)?;
            control_exchange(
                &registration_control,
                &GenerationControlRecord::new(CONTROL_SPAWNED, attempt).with_pid(pid),
                CONTROL_REGISTERED,
                attempt,
            )
            .map(|_| ())
            .map_err(classify_registration_reply)
        })?;
        Ok(PreparedChild {
            command: command.into(),
            lease: GuardedPrimaryLease {
                attempt,
                control: Arc::clone(&self.control),
                host_unavailable: Arc::clone(&self.host_unavailable),
                group_pid: None,
                group_witness,
                admission: Some(admission),
                exec_withheld: false,
                host_closed_registration: false,
                refused_registration_retired: Arc::clone(&self.refused_registration_retired),
            },
        })
    }
}

/// The host closed KGC1 generation control in place of `CONTROL_REGISTERED`
/// for this exact `SPAWNED`. The host's own refusal of a spawn that arrived
/// after its S has this shape; so does any host fault after reading the
/// record. It is never an exec permit, and the control stream is gone.
#[derive(Debug)]
struct HostClosedRegistration(io::Error);

impl std::fmt::Display for HostClosedRegistration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "host closed generation control instead of CONTROL_REGISTERED: {}",
            self.0
        )
    }
}

impl std::error::Error for HostClosedRegistration {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

// Only the reporter's own SPAWNED exchange is classified. Its EOF is the
// generation-control reader's, never a local factory, PID-pipe or codec
// error, so the private marker cannot be forged by another failure.
fn classify_registration_reply(error: RuntimeError) -> RuntimeError {
    match error {
        RuntimeError::Lifecycle { phase, source }
            if source.kind() == io::ErrorKind::UnexpectedEof =>
        {
            lifecycle_error(
                phase,
                io::Error::new(io::ErrorKind::UnexpectedEof, HostClosedRegistration(source)),
            )
        }
        other => other,
    }
}

/// Recognizes only the reporter's typed host-closed registration result.
fn is_host_closed_registration(error: &RuntimeError) -> bool {
    matches!(error, RuntimeError::Lifecycle { source, .. }
        if source.get_ref()
            .and_then(|source| source.downcast_ref::<HostClosedRegistration>())
            .is_some())
}

fn collapse_guarded_preparation_failure(
    control: &Arc<Mutex<GenerationControlPeer>>,
    attempt: u32,
    primary: RuntimeError,
) -> RuntimeError {
    let revoke = control_exchange(
        control,
        &GenerationControlRecord::new(CONTROL_REVOKE, attempt)
            .with_cause(RevocationCause::AdmissionFailed),
        CONTROL_REVOKED,
        attempt,
    );
    let clear = revoke.and_then(|_| {
        control_exchange(
            control,
            &GenerationControlRecord::new(CONTROL_CLEAR, attempt),
            CONTROL_CLEARED,
            attempt,
        )
    });
    match clear {
        Ok(_) => primary,
        Err(cleanup) => collapse_failures(vec![primary, cleanup]),
    }
}

pub(super) struct GuardedPrimaryLease {
    attempt: u32,
    control: Arc<Mutex<GenerationControlPeer>>,
    host_unavailable: Arc<AtomicBool>,
    group_pid: Option<u32>,
    group_witness: LocalGroupWitness,
    admission: Option<SpawnAdmission>,
    /// The pre-exec reporter returned an error, so it never wrote the one
    /// exec permit: the direct child cannot have run Bun or created any
    /// process, and its process group is that child alone.
    exec_withheld: bool,
    /// The host closed generation control instead of answering SPAWNED. No
    /// REVOKE/CLEAR exchange can reach it; local retirement still runs.
    host_closed_registration: bool,
    refused_registration_retired: Arc<AtomicBool>,
}

// A successful SIGKILL can leave the Guardian's *direct* Bun leader as a
// zombie until its retained Child owner reaps it. Darwin can then return
// EPERM (not ESRCH) for killpg(group, 0); that ambiguous result must never
// turn into an unverified host CLEAR. Reap this exact Child before asking the
// host to independently prove the group gone. Do not wait before revoking
// role/IPC authority or issuing the original birth-attested group signal.
pub(super) fn reap_guarded_child_before_clear(
    child: &mut Child,
    deadline: Instant,
) -> Result<(), RuntimeError> {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) if Instant::now() < deadline => std::thread::yield_now(),
            Ok(None) => {
                return Err(lifecycle_error(
                    "macOS Guardian original leader reap before CLEAR",
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        "registered child was not reaped before the native CLEAR deadline",
                    ),
                ));
            }
            Err(source) => {
                return Err(lifecycle_error(
                    "macOS Guardian original leader reap before CLEAR",
                    source,
                ));
            }
        }
    }
}

impl GenerationLease for GuardedPrimaryLease {
    fn child_spawned(&mut self, pid: u32, attempt: u32) -> Result<(), RuntimeError> {
        if attempt != self.attempt {
            return Err(lifecycle_error(
                "macOS guardian generation registration",
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "supervisor attempt differs from prepared generation",
                ),
            ));
        }
        if let Some(admission) = self.admission.take() {
            // The pre-exec reporter has already registered the group with the
            // host before permitting Bun to run. Preserve its local PID for
            // revocation even if the host's acknowledgment was malformed.
            let (group_pid, registration) = admission.finish();
            self.group_pid = group_pid;
            let registered_pid = registration.inspect_err(|error| {
                self.exec_withheld = true;
                self.host_closed_registration = is_host_closed_registration(error);
            })?;
            if registered_pid != pid {
                return Err(lifecycle_error(
                    "macOS guardian generation registration",
                    io::Error::new(io::ErrorKind::InvalidData, "pre-exec PID changed at spawn"),
                ));
            }
            return Ok(());
        }
        #[cfg(test)]
        {
            // Existing synthetic ACK fault-injection tests exercise late
            // registration directly without spawning via the preparer.
            validate_group_leader(pid)?;
            self.group_pid = Some(pid);
            control_exchange(
                &self.control,
                &GenerationControlRecord::new(CONTROL_SPAWNED, attempt).with_pid(pid),
                CONTROL_REGISTERED,
                attempt,
            )?;
            Ok(())
        }
        #[cfg(not(test))]
        Err(lifecycle_error(
            "macOS guardian generation registration",
            io::Error::other("pre-exec registration gate is missing"),
        ))
    }

    fn revoke(self, cause: RevocationCause) -> Result<(), RuntimeError> {
        // A synthetic wire-only fixture may not own a direct Child. Shipping
        // supervisor callers use revoke_with_child below for its OS custody.
        self.revoke_with_post_signal(cause, None)
    }

    fn revoke_with_child(
        self,
        cause: RevocationCause,
        child: &mut Child,
    ) -> Result<(), RuntimeError> {
        self.revoke_with_post_signal(cause, Some(child))
    }
}

impl GuardedPrimaryLease {
    fn revoke_with_post_signal(
        mut self,
        cause: RevocationCause,
        mut child: Option<&mut Child>,
    ) -> Result<(), RuntimeError> {
        if let Some(admission) = self.admission.take() {
            let (group_pid, result) = admission.finish();
            self.group_pid = group_pid;
            // A failed spawn still closes the child-side permit before exec;
            // the reporter has a bounded PID receipt/host protocol deadline.
            let _ = result;
        }
        let group_pid = self.group_pid.take();
        let retained_child = child.is_some();
        let host_available =
            !self.host_unavailable.load(Ordering::Acquire) && !self.host_closed_registration;
        let mut failures = Vec::new();
        if host_available
            && let Err(error) = control_exchange(
                &self.control,
                &GenerationControlRecord::new(CONTROL_REVOKE, self.attempt).with_cause(cause),
                CONTROL_REVOKED,
                self.attempt,
            )
        {
            failures.push(error);
        }
        // The trusted local pre-exec reporter sampled the original kernel
        // birth before host registration. Never grant raw-PGID kill authority
        // to a stale lease after its leader has been recycled.
        let verified_group = self.group_witness.take();
        if self.exec_withheld
            && let Some(unexecuted) = child.as_deref_mut()
        {
            // No exec permit was ever written, so this retained direct Child
            // is the whole group: it ran only the pre-exec gate and cannot
            // have forked. Retire it through its exact unreaped handle. A
            // group-wide probe of its zombie is ambiguous on Darwin (EPERM)
            // until this parent reaps it, and adds no authority here.
            if let Err(source) = unexecuted.kill()
                && source.kind() != io::ErrorKind::InvalidInput
            {
                failures.push(lifecycle_error(
                    "macOS Guardian unexecuted child retirement",
                    source,
                ));
            }
        } else if let Some(identity) = verified_group {
            match child.take() {
                // The Guardian is this leader's OS parent: its unreaped zombie
                // keeps the group zombie-only until this exact reap, so a
                // denied or ambiguous group is decided only after it.
                Some(own) => {
                    let (retired, reaped) = retire_own_child_group(identity, |deadline| {
                        reap_guarded_child_before_clear(own, deadline)
                    });
                    failures.extend(retired.err());
                    failures.extend(reaped.err());
                }
                None => failures.extend(terminate_attested_group(identity).err()),
            }
        }
        #[cfg(not(test))]
        if verified_group.is_none() && group_pid.is_some() {
            failures.push(lifecycle_error(
                "macOS Guardian generation local group custody",
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "discovered process group has no verified original kernel child birth",
                ),
            ));
        }
        #[cfg(test)]
        if verified_group.is_none()
            && let Some(pid) = group_pid
        {
            // The legacy ACK-only native fixture omits SpawnAdmission.
            // Even there, never grant an arbitrary numeric PGID authority:
            // the test runner must remain the leader's actual OS parent.
            let cleanup =
                super::process_group::validate_guardian_child_group(pid, std::process::id())
                    .and_then(terminate_attested_group);
            if let Err(error) = cleanup {
                failures.push(error);
            }
        }
        if let Some(child) = child
            && let Err(error) =
                reap_guarded_child_before_clear(child, Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE)
        {
            // The retained supervisor Child has not proved retirement. Do
            // not send CLEAR and let the host forget a potentially live group.
            failures.push(error);
        }
        if host_available
            && failures.is_empty()
            && let Err(error) = control_exchange(
                &self.control,
                &GenerationControlRecord::new(CONTROL_CLEAR, self.attempt)
                    .with_pid(group_pid.unwrap_or(0)),
                CONTROL_CLEARED,
                self.attempt,
            )
        {
            failures.push(error);
        }
        if failures.is_empty() {
            if self.host_closed_registration && self.exec_withheld && retained_child {
                self.refused_registration_retired
                    .store(true, Ordering::Release);
            }
            Ok(())
        } else {
            Err(collapse_failures(failures))
        }
    }
}

#[cfg(test)]
#[path = "guarded_lease/tests.rs"]
mod tests;
