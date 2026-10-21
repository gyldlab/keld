//! Single-group supervised preparation and resource revocation.
use super::process_group::{LocalGroupWitness, terminate_attested_group};
use super::{
    ChildPreparer, Command, CommandExt, GenerationLease, PreparedChild, REGISTRATION_ENV,
    RevocationCause, RuntimeError, SpawnAdmission, Stdio, Write, collapse_failures, io,
    lifecycle_error, write_group_registration,
};

pub(super) struct SupervisedGuardianPreparer<C, W, S, F: FnOnce() -> io::Result<()>> {
    pub(super) command_factory: Option<C>,
    pub(super) registration: Option<W>,
    pub(super) child_spawned: Option<S>,
    pub(super) revoke_registered_resources: Option<F>,
    pub(super) pid_tx: std::sync::mpsc::SyncSender<u32>,
}

impl<C, W, S, F> Drop for SupervisedGuardianPreparer<C, W, S, F>
where
    F: FnOnce() -> io::Result<()>,
{
    fn drop(&mut self) {
        if let Some(revoke) = self.revoke_registered_resources.take() {
            let _ = revoke();
        }
    }
}

pub(super) struct SupervisedGuardianLease<S, F> {
    admission: Option<SpawnAdmission>,
    child_spawned: Option<S>,
    revoke_registered_resources: Option<F>,
    pid_tx: std::sync::mpsc::SyncSender<u32>,
    group_pid: Option<u32>,
    group_witness: LocalGroupWitness,
}

impl<C, W, S, F> ChildPreparer for SupervisedGuardianPreparer<C, W, S, F>
where
    C: FnOnce() -> Result<Command, RuntimeError> + Send + 'static,
    W: Write + Send + 'static,
    S: FnOnce(u32) -> io::Result<()> + Send + 'static,
    F: FnOnce() -> io::Result<()> + Send + 'static,
{
    type Lease = SupervisedGuardianLease<S, F>;

    fn prepare(&mut self, attempt: u32) -> Result<PreparedChild<Self::Lease>, RuntimeError> {
        if attempt != 1 {
            return Err(lifecycle_error(
                "macOS guardian supervisor policy",
                io::Error::other("KEL-96/T1b forbids a successor before KEL-96/T3"),
            ));
        }
        let factory = self.command_factory.take().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian supervisor preparation",
                io::Error::other("Bun command factory was already consumed"),
            )
        })?;
        let mut command = match factory() {
            Ok(command) => command,
            Err(error) => {
                let mut failures = vec![error];
                if let Some(revoke) = self.revoke_registered_resources.take()
                    && let Err(source) = revoke()
                {
                    failures.push(lifecycle_error(
                        "macOS guardian pre-child registered-resource revocation",
                        source,
                    ));
                }
                return Err(collapse_failures(failures));
            }
        };
        command
            .env_remove(REGISTRATION_ENV)
            .process_group(0)
            .stdin(Stdio::null());
        let mut registration = self.registration.take().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian supervisor preparation",
                io::Error::other("authenticated registration writer is missing"),
            )
        })?;
        let group_witness = LocalGroupWitness::default();
        let reporter_group_witness = group_witness.clone();
        let admission = SpawnAdmission::install(&mut command, move |pid| {
            reporter_group_witness.register_child(pid)?;
            write_group_registration(&mut registration, pid)
        })?;
        Ok(PreparedChild {
            command: command.into(),
            lease: SupervisedGuardianLease {
                admission: Some(admission),
                child_spawned: self.child_spawned.take(),
                revoke_registered_resources: self.revoke_registered_resources.take(),
                pid_tx: self.pid_tx.clone(),
                group_pid: None,
                group_witness,
            },
        })
    }
}

impl<S, F> GenerationLease for SupervisedGuardianLease<S, F>
where
    S: FnOnce(u32) -> io::Result<()> + Send + 'static,
    F: FnOnce() -> io::Result<()> + Send + 'static,
{
    fn child_spawned(&mut self, pid: u32, _attempt: u32) -> Result<(), RuntimeError> {
        let admission = self.admission.take().ok_or_else(|| {
            lifecycle_error(
                "macOS guardian supervised-child registration",
                io::Error::other("pre-exec registration gate is missing"),
            )
        })?;
        let (group_pid, registered) = admission.finish();
        self.group_pid = group_pid;
        if registered? != pid {
            return Err(lifecycle_error(
                "macOS guardian supervised-child registration",
                io::Error::new(io::ErrorKind::InvalidData, "pre-exec PID changed at spawn"),
            ));
        }
        if let Some(observer) = self.child_spawned.take() {
            observer(pid).map_err(|source| {
                lifecycle_error("macOS guardian supervised-child observer", source)
            })?;
        }
        self.pid_tx.send(pid).map_err(|_| {
            lifecycle_error(
                "macOS guardian supervised-child registration",
                io::Error::other("guardian owner stopped before child registration"),
            )
        })
    }

    fn revoke(mut self, _cause: RevocationCause) -> Result<(), RuntimeError> {
        if let Some(admission) = self.admission.take() {
            let (group_pid, result) = admission.finish();
            self.group_pid = group_pid;
            // A failed admission does not grant execution; its typed failure
            // is already owned by the supervisor's spawn/registration path.
            let _ = result;
        }
        let mut failures = Vec::new();
        if let Some(revoke) = self.revoke_registered_resources.take()
            && let Err(source) = revoke()
        {
            failures.push(lifecycle_error(
                "macOS guardian registered-resource revocation",
                source,
            ));
        }
        let verified_group = self.group_witness.take();
        if let Some(group) = verified_group
            && let Err(error) = terminate_attested_group(group)
        {
            failures.push(error);
        }
        if verified_group.is_none() && self.group_pid.is_some() {
            failures.push(lifecycle_error(
                "macOS Guardian supervised local group custody",
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "discovered process group has no verified original kernel child birth",
                ),
            ));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(collapse_failures(failures))
        }
    }
}
