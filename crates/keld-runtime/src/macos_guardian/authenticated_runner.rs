//! Legacy one-group Guardian execution with crash-closed authenticated custody.
//! No Bun image executes before the host has accepted and retained its group.
use super::process_group::{GroupIdentity, LocalGroupWitness, terminate_attested_group};
use super::{
    AwaitHostGroupRetention, Command, CommandExt, GuardianReport, Read, RuntimeError,
    SpawnAdmission, Stdio, await_host_death, collapse_failures, fail_before_child, io,
    lifecycle_error, validate_group_pid, validate_liveness_bootstrap, write_group_registration,
};
use std::os::fd::AsFd;
use std::process::Child;

pub(super) fn run_with_authenticated_registration<R, F>(
    mut command: Command,
    mut liveness: R,
    registration: AwaitHostGroupRetention,
    revoke_registered_resources: F,
) -> Result<GuardianReport, RuntimeError>
where
    R: Read + AsFd,
    F: FnOnce() -> io::Result<()>,
{
    if command.get_program().is_empty() {
        return Err(fail_before_child(
            lifecycle_error(
                "macOS guardian bootstrap",
                io::Error::new(io::ErrorKind::InvalidInput, "child program is empty"),
            ),
            revoke_registered_resources,
        ));
    }
    if let Err(error) = validate_liveness_bootstrap(&mut liveness) {
        return Err(fail_before_child(error, revoke_registered_resources));
    }
    command
        .env_remove(super::REGISTRATION_ENV)
        .process_group(0)
        .stdin(Stdio::null());
    let mut registration = registration;
    let group_witness = LocalGroupWitness::default();
    let reporter_group_witness = group_witness.clone();
    let admission = match SpawnAdmission::install(&mut command, move |pid| {
        validate_group_pid(pid).map(|_| ())?;
        reporter_group_witness.register_child(pid)?;
        write_group_registration(&mut registration, pid)
    }) {
        Ok(admission) => admission,
        Err(error) => return Err(fail_before_child(error, revoke_registered_resources)),
    };
    let child = match command.spawn() {
        Ok(child) => child,
        Err(source) => {
            drop(command);
            let (_pid, admission_result) = admission.finish();
            // Command::spawn has already reaped a child that failed to exec.
            // With no retained Child handle and no executed Bun/descendants,
            // signalling a numeric PGID risks killing a recycled outsider.
            return Err(reject_spawn(
                RuntimeError::Spawn(source),
                admission_result.err(),
                None,
                None,
                revoke_registered_resources,
            ));
        }
    };
    let (group_pid, registration_result) = admission.finish();
    let verified_group = group_witness.take();
    let group_pid = group_pid.unwrap_or(child.id());
    if let Err(error) = registration_result {
        return Err(reject_spawn(
            error,
            None,
            verified_group,
            Some(child),
            revoke_registered_resources,
        ));
    }
    if child.id() != group_pid {
        return Err(reject_spawn(
            lifecycle_error(
                "macOS guardian pre-exec registration",
                io::Error::new(io::ErrorKind::InvalidData, "registered child PID mismatch"),
            ),
            None,
            verified_group,
            Some(child),
            revoke_registered_resources,
        ));
    }
    let Some(verified_group) = verified_group else {
        return Err(reject_spawn(
            lifecycle_error(
                "macOS guardian local group registration witness",
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "authenticated registration lost the original kernel child identity",
                ),
            ),
            None,
            None,
            Some(child),
            revoke_registered_resources,
        ));
    };
    run_live_group(child, verified_group, liveness, revoke_registered_resources)
}

fn reject_spawn(
    primary: RuntimeError,
    admission_error: Option<RuntimeError>,
    group: Option<GroupIdentity>,
    mut child: Option<Child>,
    revoke: impl FnOnce() -> io::Result<()>,
) -> RuntimeError {
    let mut failures = vec![primary];
    if let Some(error) = admission_error {
        failures.push(error);
    }
    if let Err(source) = revoke() {
        failures.push(lifecycle_error(
            "macOS guardian registered-resource revocation",
            source,
        ));
    }
    if let Some(group) = group
        && let Err(error) = terminate_attested_group(group)
    {
        failures.push(error);
    }
    if let Some(child) = child.as_mut()
        && let Err(source) = child.wait()
    {
        failures.push(lifecycle_error("macOS guardian direct-child wait", source));
    }
    collapse_failures(failures)
}

fn run_live_group<R: Read + AsFd>(
    mut child: Child,
    group: GroupIdentity,
    mut liveness: R,
    revoke: impl FnOnce() -> io::Result<()>,
) -> Result<GuardianReport, RuntimeError> {
    let liveness_result = await_host_death(&mut liveness);
    let mut failures = Vec::new();
    if let Err(error) = liveness_result {
        failures.push(error);
    }
    if let Err(source) = revoke() {
        failures.push(lifecycle_error(
            "macOS guardian registered-resource revocation",
            source,
        ));
    }
    if let Err(error) = terminate_attested_group(group) {
        failures.push(error);
    }
    let leader_status = match child.wait() {
        Ok(status) => status,
        Err(source) => {
            failures.push(lifecycle_error("macOS guardian direct-child wait", source));
            return Err(collapse_failures(failures));
        }
    };
    if failures.is_empty() {
        Ok(GuardianReport {
            leader_pid: group.pid,
            leader_status,
        })
    } else {
        Err(collapse_failures(failures))
    }
}
