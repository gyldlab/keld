//! Private macOS process-group validation and rejected-registration cleanup.
use std::io;
use std::process::{Child, ChildStdin, ExitStatus};
use std::sync::{Arc, Mutex};
use std::time::Instant;
#[cfg(test)]
use std::{cell::RefCell, sync::mpsc};

use nix::errno::Errno;
use nix::libc;
use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::{Pid, getpgid};

use super::{
    GENERATION_CONTROL_POLL, GUARDIAN_SHUTDOWN_DEADLINE, RuntimeError, collapse_failures,
    lifecycle_error, nix_io_error,
};

/// Kernel birth identity of the exact enrolled process-group leader.
///
/// A PGID is only a number, not a stable kernel handle. Keep the start time
/// observed *before* acknowledging registration, and recheck it at host-side
/// fail-safe signaling. If the original leader disappears or the number is
/// recycled, never authorize a signal from that stale enrollment alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct GroupIdentity {
    pub(super) pid: u32,
    real_uid: libc::uid_t,
    started_sec: u64,
    started_usec: u64,
}

/// One locally observed Guardian-child group, shared only with the pre-exec
/// reporter until its join. The reporter proves the kernel lineage/birth
/// *before* sending any registration record or receiving an exec permit.
/// Later revocation consumes precisely this identity, not an untrusted PID.
#[derive(Clone, Default)]
pub(super) struct LocalGroupWitness(Arc<Mutex<Option<GroupIdentity>>>);

impl LocalGroupWitness {
    pub(super) fn register_child(&self, pid: u32) -> Result<(), RuntimeError> {
        let identity = validate_guardian_child_group(pid, std::process::id())?;
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(identity);
        Ok(())
    }

    pub(super) fn take(&self) -> Option<GroupIdentity> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    #[cfg(test)]
    pub(super) fn seed_for_test(&self, identity: GroupIdentity) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(identity);
    }
}

// Exact native negative-control receipt. It is local to the calling test
// thread and fires only AFTER the real kernel reports missing leader + EPERM,
// so the external test-owned zombie cannot be reaped before the first probe.
#[cfg(test)]
thread_local! {
    static AMBIGUOUS_GROUP_PROBE_RECEIPT: RefCell<Option<(u32, mpsc::Sender<()>)>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(super) struct TestAmbiguousProbeReceipt;

#[cfg(test)]
impl Drop for TestAmbiguousProbeReceipt {
    fn drop(&mut self) {
        AMBIGUOUS_GROUP_PROBE_RECEIPT.with(|slot| *slot.borrow_mut() = None);
    }
}

#[cfg(test)]
pub(super) fn install_test_ambiguous_probe_receipt(
    group: u32,
    sender: mpsc::Sender<()>,
) -> TestAmbiguousProbeReceipt {
    AMBIGUOUS_GROUP_PROBE_RECEIPT.with(|slot| {
        assert!(
            slot.borrow().is_none(),
            "only one native probe receipt owner"
        );
        *slot.borrow_mut() = Some((group, sender));
    });
    TestAmbiguousProbeReceipt
}

#[cfg(test)]
fn report_test_ambiguous_probe(group: u32) {
    AMBIGUOUS_GROUP_PROBE_RECEIPT.with(|slot| {
        let mut receipt = slot.borrow_mut();
        if receipt
            .as_ref()
            .is_some_and(|(expected, _)| *expected == group)
            && let Some((_, sender)) = receipt.take()
        {
            let _ = sender.send(());
        }
    });
}

#[cfg(test)]
impl GroupIdentity {
    pub(super) fn altered_birth_for_test(mut self) -> Self {
        self.started_usec = self.started_usec.wrapping_add(1);
        self
    }

    pub(super) const fn unverified_for_test(pid: u32) -> Self {
        Self {
            pid,
            real_uid: 0,
            started_sec: 0,
            started_usec: 0,
        }
    }
}

pub(super) fn validate_group_pid(group: u32) -> Result<i32, RuntimeError> {
    if group == 0 {
        return Err(lifecycle_error(
            "macOS guardian host registration",
            io::Error::new(io::ErrorKind::InvalidInput, "process group is zero"),
        ));
    }
    i32::try_from(group).map_err(|_| {
        lifecycle_error(
            "macOS guardian host registration",
            io::Error::new(io::ErrorKind::InvalidInput, "process group exceeds c_int"),
        )
    })
}

pub(super) fn validate_group_leader(group: u32) -> Result<i32, RuntimeError> {
    let raw = validate_group_pid(group)?;
    let observed = getpgid(Some(Pid::from_raw(raw))).map_err(|source| {
        lifecycle_error(
            "macOS guardian group-leader validation",
            nix_io_error(source),
        )
    })?;
    if observed.as_raw() == raw {
        Ok(raw)
    } else {
        Err(lifecycle_error(
            "macOS guardian group-leader validation",
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("reported pid {group} belongs to process group {observed}"),
            ),
        ))
    }
}

/// KGR1 is authenticated, but its PID field is still untrusted input. Before
/// returning a retention ACK, verify that the *actual* process-group leader
/// is the retained Guardian's direct child rather than an unrelated group.
#[allow(unsafe_code)] // read-only, exact-size macOS PROC_PIDTBSDINFO observation
fn observe_group_leader_bsdinfo(group_pid: u32) -> Result<libc::proc_bsdinfo, RuntimeError> {
    let pid = validate_group_pid(group_pid)?;
    // SAFETY: proc_bsdinfo is an integer/fixed-array C output structure.
    // The kernel writes at most its exact size and retains no pointer.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = i32::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).map_err(|source| {
        lifecycle_error(
            "macOS guardian registered group identity",
            io::Error::new(io::ErrorKind::InvalidData, source),
        )
    })?;
    // SAFETY: initialized writable exact-size buffer for a checked c_int PID.
    let observed =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
    if observed != size || info.pbi_pid != group_pid || info.pbi_pgid != group_pid {
        return Err(lifecycle_error(
            "macOS guardian registered group identity",
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the registered group leader is missing, inaccessible or not its own PGID",
            ),
        ));
    }
    Ok(info)
}

pub(super) fn observe_group_identity(group_pid: u32) -> Result<GroupIdentity, RuntimeError> {
    let info = observe_group_leader_bsdinfo(group_pid)?;
    Ok(GroupIdentity {
        pid: group_pid,
        real_uid: info.pbi_ruid,
        started_sec: info.pbi_start_tvsec,
        started_usec: info.pbi_start_tvusec,
    })
}

/// A private authenticated KGR1/KGC1 PID cannot substitute another process:
/// require the exact Guardian's live direct child before retaining authority.
pub(super) fn validate_guardian_child_group(
    group_pid: u32,
    guardian_pid: u32,
) -> Result<GroupIdentity, RuntimeError> {
    let info = observe_group_leader_bsdinfo(group_pid)?;
    if info.pbi_ppid != guardian_pid {
        return Err(lifecycle_error(
            "macOS guardian registered child lineage",
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "reported group is not the retained Guardian's direct child",
            ),
        ));
    }
    Ok(GroupIdentity {
        pid: group_pid,
        real_uid: info.pbi_ruid,
        started_sec: info.pbi_start_tvsec,
        started_usec: info.pbi_start_tvusec,
    })
}

/// Host fallback for an *enrolled kernel birth*, not a caller-provided PGID.
/// Refuse a live leader whose birth no longer matches. If the original leader
/// has been reaped but its process group survives through descendants, retain
/// the existing group cleanup: on Darwin the PID allocator reserves a PGID
/// while that group has members. This is not a kernel-held group handle, so an
/// empty/recycled group between inspections remains a documented race.
pub(super) fn terminate_attested_group(group: GroupIdentity) -> Result<(), RuntimeError> {
    match observe_group_identity(group.pid) {
        Ok(current) if current == group => terminate_registered_group(group.pid),
        Ok(_) => Err(lifecycle_error(
            "macOS guardian registered group identity changed",
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "process-group leader PID was reused; refusing unowned group signal",
            ),
        )),
        Err(observation) => {
            let raw = validate_group_pid(group.pid)?;
            // Darwin may report EPERM for kill(-PGID, 0) when the enrolled
            // leader has been reaped but another already-killed group member
            // is still a zombie awaiting its *own* parent's wait(). A single
            // such observation does not prove either that a live member is
            // authorized to signal or that an empty group can be forgotten.
            // Retain the original fail-closed rule and retry ONLY the safe
            // absence probe, within a fraction of the existing shutdown
            // deadline. Never signal on an ambiguous EPERM or an extant PID.
            let reap_deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE / 4;
            let mut saw_ambiguous_group = false;
            loop {
                let leader = getpgid(Some(Pid::from_raw(raw)));
                let group_live = kill(Pid::from_raw(-raw), None);
                #[cfg(test)]
                if matches!(
                    (&leader, &group_live),
                    (Err(Errno::ESRCH), Err(Errno::EPERM))
                ) {
                    report_test_ambiguous_probe(group.pid);
                }
                match (leader, group_live) {
                    (_, Err(Errno::ESRCH)) => break Ok(()),
                    // A reaped leader can still have a live same-group
                    // descendant; retain the original group-wide cleanup.
                    (Err(Errno::ESRCH), Ok(())) if !saw_ambiguous_group => {
                        break terminate_registered_group(group.pid);
                    }
                    (Err(Errno::ESRCH), Err(Errno::EPERM)) if Instant::now() < reap_deadline => {
                        // EPERM may also denote an inaccessible group. If
                        // that numeric PGID is later recycled and becomes
                        // signallable, its new members are NOT ours. After
                        // one ambiguous observation only ESRCH may succeed;
                        // never start a new killpg from a delayed probe.
                        saw_ambiguous_group = true;
                        std::thread::sleep(GENERATION_CONTROL_POLL);
                    }
                    _ => {
                        break Err(collapse_failures(vec![
                            observation,
                            lifecycle_error(
                                "macOS guardian registered group identity unavailable",
                                io::Error::new(
                                    io::ErrorKind::PermissionDenied,
                                    "live process-group leader identity is unproven",
                                ),
                            ),
                        ]));
                    }
                }
            }
        }
    }
}

pub(super) fn terminate_registered_group(group: u32) -> Result<(), RuntimeError> {
    let group = validate_group_pid(group)?;
    let result = killpg(Pid::from_raw(group), Signal::SIGKILL);
    finish_group_termination(result, || kill(Pid::from_raw(-group), None))
}

// A reaped process-group *leader* does not prove that its descendants left
// that group. In particular, an EPERM from killpg must remain fail-closed
// unless a zero-signal probe of the *group* reports ESRCH.
pub(super) fn finish_group_termination(
    result: Result<(), Errno>,
    probe_group: impl FnOnce() -> Result<(), Errno>,
) -> Result<(), RuntimeError> {
    match result {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(Errno::EPERM) if matches!(probe_group(), Err(Errno::ESRCH)) => Ok(()),
        Err(error) => Err(lifecycle_error(
            "macOS guardian process-group signal",
            io::Error::from_raw_os_error(error as i32),
        )),
    }
}

// Registration has not yet transferred ownership. A rejected or dropped
// bootstrap must not block the host forever waiting for uncooperative EOF.
pub(super) fn reap_pending_guardian(child: &mut Child) -> Result<ExitStatus, RuntimeError> {
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::yield_now(),
            Ok(None) => {
                let mut failures = vec![lifecycle_error(
                    "macOS guardian rejected-registration cleanup deadline",
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        "guardian did not exit after host EOF",
                    ),
                )];
                if let Err(source) = child.kill()
                    && source.kind() != io::ErrorKind::InvalidInput
                {
                    failures.push(lifecycle_error(
                        "macOS guardian rejected-registration process kill",
                        source,
                    ));
                }
                if let Err(source) = child.wait() {
                    failures.push(lifecycle_error(
                        "macOS guardian rejected-registration process wait",
                        source,
                    ));
                }
                return Err(collapse_failures(failures));
            }
            Err(source) => {
                let mut failures = vec![lifecycle_error(
                    "macOS guardian rejected-registration process inspection",
                    source,
                )];
                if let Err(source) = child.kill()
                    && source.kind() != io::ErrorKind::InvalidInput
                {
                    failures.push(lifecycle_error(
                        "macOS guardian rejected-registration process kill",
                        source,
                    ));
                }
                if let Err(source) = child.wait() {
                    failures.push(lifecycle_error(
                        "macOS guardian rejected-registration process wait",
                        source,
                    ));
                }
                return Err(collapse_failures(failures));
            }
        }
    }
}

pub(super) fn reject_host_registration(
    mut child: Child,
    liveness_writer: ChildStdin,
    error: RuntimeError,
) -> RuntimeError {
    drop(liveness_writer);
    let mut failures = vec![error];
    match reap_pending_guardian(&mut child) {
        Ok(status) if status.success() => {}
        Ok(status) => {
            failures.push(lifecycle_error(
                "macOS guardian rejected-registration wait",
                io::Error::other(format!(
                    "guardian exited unsuccessfully during registration cleanup: {status}"
                )),
            ));
        }
        Err(cleanup) => failures.push(cleanup),
    }
    // No untrusted numeric PID belongs in this pre-registration cleanup API.
    // Post-enrollment rejection runs its separate birth-attested group cleanup.
    collapse_failures(failures)
}
