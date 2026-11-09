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

// Exact native fault points for the GH-824 group-signal race. They run on
// the calling test thread around the ONE group signal of a call, so a test
// can let the last live member finish exiting just before it, or make the
// group signallable again just after a denial, instead of hoping the
// scheduler does. Shipping builds have no seam.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TestGroupSignalPoint {
    /// Inside [`terminate_registered_group`], just before its `killpg`: every
    /// group signal the Guardian product sends passes here, whichever call
    /// site (attested arm, own-child decision or a direct call) reached it.
    BeforeSignal,
    /// After that one signal was denied (zombie-only EPERM), before the next
    /// read-only probe.
    AfterDeniedSignal,
}

#[cfg(test)]
type TestGroupSignalHook = (u32, Box<dyn FnMut(TestGroupSignalPoint)>);

#[cfg(test)]
thread_local! {
    static GROUP_SIGNAL_HOOK: RefCell<Option<TestGroupSignalHook>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(super) struct TestGroupSignalHookOwner;

#[cfg(test)]
impl Drop for TestGroupSignalHookOwner {
    fn drop(&mut self) {
        GROUP_SIGNAL_HOOK.with(|slot| *slot.borrow_mut() = None);
    }
}

#[cfg(test)]
pub(super) fn install_test_group_signal_hook(
    group: u32,
    hook: Box<dyn FnMut(TestGroupSignalPoint)>,
) -> TestGroupSignalHookOwner {
    GROUP_SIGNAL_HOOK.with(|slot| {
        assert!(slot.borrow().is_none(), "only one native signal hook owner");
        *slot.borrow_mut() = Some((group, hook));
    });
    TestGroupSignalHookOwner
}

#[cfg(test)]
fn run_test_group_signal_hook(group: u32, point: TestGroupSignalPoint) {
    // Take the hook out while it runs so it may block or spawn freely.
    let taken = GROUP_SIGNAL_HOOK.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot
            .as_ref()
            .is_some_and(|(expected, _)| *expected == group)
        {
            slot.take()
        } else {
            None
        }
    });
    if let Some((owner, mut hook)) = taken {
        hook(point);
        GROUP_SIGNAL_HOOK.with(|slot| *slot.borrow_mut() = Some((owner, hook)));
    }
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
///
/// One call sends at most ONE group signal ([`signal_attested_group_once`]).
/// This is per call: a later call on the same retained birth (after a failed
/// CLEAR, at shutdown/Drop or `fail_current_generation`) starts its own
/// decision, exactly as before; one signal per *generation* is not enforced
/// here.
pub(super) fn terminate_attested_group(group: GroupIdentity) -> Result<(), RuntimeError> {
    let reap_deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE / 4;
    match observe_group_identity(group.pid) {
        Ok(current) if current == group => signal_attested_group_once(group, reap_deadline),
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
                    // It is this call's one signal: the loop ends with it.
                    (Err(Errno::ESRCH), Ok(())) if !saw_ambiguous_group => {
                        break signal_attested_group_once(group, reap_deadline);
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

// The one group SIGKILL of an attested cleanup call, from either the
// birth-matched arm or the first-probe arm. XNU `killpg1` counts an exiting
// member as found but skips SZOMB members, and returns EPERM when it found
// none, so the last member finishing its exit just before this signal leaves
// a zombie-only group: ambiguous, neither a refused signal nor absence. The
// denied signal is never repeated in this call; it goes to the read-only
// reconciliation inside this call's budget, and an unproven group keeps the
// typed signal-phase EPERM (GH-824 Host hang).
fn signal_attested_group_once(
    group: GroupIdentity,
    reap_deadline: Instant,
) -> Result<(), RuntimeError> {
    terminate_registered_group(group.pid)
        .or_else(|denied| reconcile_denied_signal_after_reap(group, denied, reap_deadline))
}

/// Reconcile exactly one denied group signal: the typed signal-phase EPERM
/// that [`finish_group_termination`] returns when its own follow-up probe was
/// not `ESRCH`. Any other error is returned unchanged. The group is never
/// signalled again; only [`require_attested_group_absence`] (a read-only
/// observation of BOTH the original leader PID and the whole group as `ESRCH`
/// before `deadline`) proves retirement. A persistent zombie-only group, a
/// live or reused leader, or a group that turns signallable again returns the
/// original typed signal-phase EPERM.
///
/// The host calls this inside the signalling call. A caller that is the
/// leader's own OS parent calls it only after reaping that exact Child (see
/// [`retire_own_child_group`]). Name and signature match the #933/#945/#947
/// stack's owner of the same rule, but its error shape does NOT yet: this
/// returns the original denial unchanged (`raw_os_error() == Some(EPERM)`),
/// while the stack wraps an unproven denial (`raw_os_error()` is `None`,
/// matched recursively). The dedupe picks one form and adjusts the other
/// side's tests (#1077 merge plan, D3).
pub(super) fn reconcile_denied_signal_after_reap(
    group: GroupIdentity,
    denied: RuntimeError,
    deadline: Instant,
) -> Result<(), RuntimeError> {
    if !is_denied_group_signal(&denied) {
        return Err(denied);
    }
    #[cfg(test)]
    run_test_group_signal_hook(group.pid, TestGroupSignalPoint::AfterDeniedSignal);
    require_attested_group_absence(group, deadline).map_err(|_| denied)
}

fn is_denied_group_signal(error: &RuntimeError) -> bool {
    matches!(
        error,
        RuntimeError::Lifecycle {
            phase: GROUP_SIGNAL_PHASE,
            source,
        } if source.raw_os_error() == Some(libc::EPERM)
    )
}

/// Read-only proof that the original group is gone: BOTH the original leader
/// PID and the whole group report `ESRCH`, decided before `deadline`. A
/// zombie-only group (EPERM) is polled; anything else fails at once. This
/// never signals and never treats EPERM as absence. (Same name and signature
/// as the stack's owner, which additionally re-checks a live leader's birth.)
pub(super) fn require_attested_group_absence(
    group: GroupIdentity,
    deadline: Instant,
) -> Result<(), RuntimeError> {
    let raw = validate_group_pid(group.pid)?;
    loop {
        let leader = getpgid(Some(Pid::from_raw(raw)));
        let whole_group = kill(Pid::from_raw(-raw), None);
        #[cfg(test)]
        if matches!(
            (&leader, &whole_group),
            (Err(Errno::ESRCH), Err(Errno::EPERM))
        ) {
            report_test_ambiguous_probe(group.pid);
        }
        match (leader, whole_group) {
            (Err(Errno::ESRCH), Err(Errno::ESRCH)) if Instant::now() < deadline => return Ok(()),
            (Err(Errno::ESRCH), Err(Errno::EPERM)) if Instant::now() < deadline => {
                std::thread::sleep(GENERATION_CONTROL_POLL);
            }
            observed => {
                return Err(lifecycle_error(
                    "macOS guardian group absence",
                    io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!("original group absence is unproven: {observed:?}"),
                    ),
                ));
            }
        }
    }
}

/// Group retirement for a caller that is the attested leader's own OS parent
/// and still holds its unreaped `Child` (the Guardian's supervisor paths).
///
/// While that Child is unreaped its zombie keeps the group zombie-only
/// (`getpgid` ESRCH, `kill(-g, 0)` EPERM), so an in-call absence wait could
/// never succeed: it would only burn its budget and fail. Here the call makes
/// at most ONE group signal decision without any in-call wait, then runs
/// `reap_own_child` with the shared post-signal budget, and only after a
/// successful reap proves absence read-only within the rest of that budget:
/// - a denied signal is reconciled by [`reconcile_denied_signal_after_reap`]
///   and fails with the typed signal-phase EPERM;
/// - a zombie-only group found before any signal fails with the existing
///   "registered group identity unavailable" error.
///
/// Returns `(group result, reap result)` so callers keep their own reap
/// error. The group is never signalled after a denial or an ambiguity.
pub(super) fn retire_own_child_group(
    group: GroupIdentity,
    reap_own_child: impl FnOnce(Instant) -> Result<(), RuntimeError>,
) -> (Result<(), RuntimeError>, Result<(), RuntimeError>) {
    enum Decision {
        Done(Result<(), RuntimeError>),
        Denied(RuntimeError),
        Ambiguous(RuntimeError),
    }
    let identity_unavailable = |observation: RuntimeError| {
        collapse_failures(vec![
            observation,
            lifecycle_error(
                "macOS guardian registered group identity unavailable",
                io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "live process-group leader identity is unproven",
                ),
            ),
        ])
    };
    let signal_once = || match terminate_registered_group(group.pid) {
        Err(denied) if is_denied_group_signal(&denied) => Decision::Denied(denied),
        result => Decision::Done(result),
    };
    let decision = match observe_group_identity(group.pid) {
        Ok(current) if current == group => signal_once(),
        Ok(_) => Decision::Done(Err(lifecycle_error(
            "macOS guardian registered group identity changed",
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "process-group leader PID was reused; refusing unowned group signal",
            ),
        ))),
        Err(observation) => match validate_group_pid(group.pid) {
            Err(error) => Decision::Done(Err(error)),
            Ok(raw) => match (
                getpgid(Some(Pid::from_raw(raw))),
                kill(Pid::from_raw(-raw), None),
            ) {
                (_, Err(Errno::ESRCH)) => Decision::Done(Ok(())),
                (Err(Errno::ESRCH), Ok(())) => signal_once(),
                // Our own unreaped leader (or another member) is a zombie:
                // decided only after this caller's reap.
                (Err(Errno::ESRCH), Err(Errno::EPERM)) => {
                    Decision::Ambiguous(identity_unavailable(observation))
                }
                _ => Decision::Done(Err(identity_unavailable(observation))),
            },
        },
    };
    let reap_deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let reap = reap_own_child(reap_deadline);
    let retired = match decision {
        Decision::Done(result) => result,
        Decision::Denied(denied) if reap.is_ok() => {
            reconcile_denied_signal_after_reap(group, denied, reap_deadline)
        }
        Decision::Ambiguous(unavailable) if reap.is_ok() => {
            require_attested_group_absence(group, reap_deadline).map_err(|_| unavailable)
        }
        Decision::Denied(error) | Decision::Ambiguous(error) => Err(error),
    };
    (retired, reap)
}

const GROUP_SIGNAL_PHASE: &str = "macOS guardian process-group signal";

pub(super) fn terminate_registered_group(group: u32) -> Result<(), RuntimeError> {
    let raw = validate_group_pid(group)?;
    #[cfg(test)]
    run_test_group_signal_hook(group, TestGroupSignalPoint::BeforeSignal);
    let result = killpg(Pid::from_raw(raw), Signal::SIGKILL);
    finish_group_termination(result, || kill(Pid::from_raw(-raw), None))
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
            GROUP_SIGNAL_PHASE,
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
