//! Legacy one-group Guardian execution with crash-closed authenticated custody.
//! No Bun image executes before the host has accepted and retained its group.
use super::process_group::{
    GroupIdentity, LocalGroupWitness, retire_own_child_group, terminate_attested_group,
};
use super::{
    AwaitHostGroupRetention, Command, CommandExt, GUARDIAN_SHUTDOWN_DEADLINE, GuardianReport,
    Instant, Read, RuntimeError, SpawnAdmission, Stdio, await_host_death, collapse_failures,
    fail_before_child, io, lifecycle_error, validate_group_pid, validate_liveness_bootstrap,
    write_group_registration,
};
use std::os::fd::AsFd;
use std::process::{Child, ExitStatus};

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
    match (group, child.as_mut()) {
        // This Guardian is the leader's OS parent: decide the group only
        // after reaping its exact unreaped Child.
        (Some(group), Some(own)) => {
            let (retired, reaped) = retire_own_child_group(group, |_| {
                own.wait()
                    .map(|_| ())
                    .map_err(|source| lifecycle_error("macOS guardian direct-child wait", source))
            });
            failures.extend(retired.err());
            failures.extend(reaped.err());
        }
        (group, own) => {
            if let Some(group) = group
                && let Err(error) = terminate_attested_group(group)
            {
                failures.push(error);
            }
            if let Some(own) = own
                && let Err(source) = own.wait()
            {
                failures.push(lifecycle_error("macOS guardian direct-child wait", source));
            }
        }
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
    // Bound the native error-path fallback from the original host-death
    // observation, never add a fresh budget after resource revocation.
    let original_cleanup_deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
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
    // This Guardian is the leader's OS parent, so the group is decided (at
    // most one signal, no in-call wait) before it reaps its exact Child, and
    // proven absent only after that reap. The reap never blocks without
    // bound (#952): when no signal was delivered (denied, refused, changed
    // birth) the Child may still execute, so only this exact retained Child
    // is killed and reaped, inside the ORIGINAL host-death budget. A direct
    // Child kill never claims group absence on its own.
    let mut leader_status = None;
    // `original_cleanup_deadline` is taken before revocation and the owner's
    // post-decision deadline after it, so `min` keeps the original bound; it
    // only makes that ordering explicit.
    let (retired, reaped) = retire_own_child_group(group, |reap_deadline| {
        leader_status = Some(
            reap_owned_child_within_deadline(
                &mut child,
                reap_deadline.min(original_cleanup_deadline),
            )
            .map_err(|source| lifecycle_error("macOS guardian direct-child wait", source))?,
        );
        Ok(())
    });
    failures.extend(retired.err());
    if let Err(error) = reaped {
        failures.push(error);
        return Err(collapse_failures(failures));
    }
    let Some(leader_status) = leader_status else {
        failures.push(lifecycle_error(
            "macOS guardian direct-child wait",
            io::Error::other("direct child reap reported no status"),
        ));
        return Err(collapse_failures(failures));
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

// The direct Child is still our unreaped OS child whatever the group
// decision was (delivered, denied, refused or changed birth). try_wait()
// establishes that live ownership before Child::kill; it is never a raw-PID
// fallback from the untrusted authenticated registration, and it is not a
// second GROUP signal. A direct-child kill cannot establish whole-group
// absence, so `retire_own_child_group` alone decides the group and its typed
// failure is always kept even after this reap succeeds.
//
// Cost: the bounded reap polls `try_wait` with `yield_now`. After a delivered
// or host SIGKILL the Child is already exiting, so this lasts milliseconds;
// only a Child that never reaps keeps one core spinning until `deadline`
// (at most the rest of the 5 s Guardian shutdown budget) before `TimedOut`,
// the same shape as `reap_pending_guardian`.
fn reap_owned_child_within_deadline(
    child: &mut Child,
    deadline: Instant,
) -> Result<ExitStatus, io::Error> {
    if let Some(status) = child.try_wait()? {
        return Ok(status);
    }
    child.kill()?;
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(status),
            None if Instant::now() < deadline => std::thread::yield_now(),
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "original direct child did not reap within the existing Guardian shutdown budget",
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "authenticated_runner/tests.rs"]
mod tests;
