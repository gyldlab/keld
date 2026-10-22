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
    /// After the kernel authorized the one signal, before `killpg`.
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
/// decision; one signal per *generation* is enforced by the retained owner
/// (`GuardedPrimary`'s spent authority), not here.
pub(super) fn terminate_attested_group(group: GroupIdentity) -> Result<(), RuntimeError> {
    terminate_attested_group_until(group, Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE / 4)
}

/// [`terminate_attested_group`] with a caller-owned absolute reap deadline,
/// so a retained owner can bound every attempt by one shared budget.
pub(super) fn terminate_attested_group_until(
    group: GroupIdentity,
    reap_deadline: Instant,
) -> Result<(), RuntimeError> {
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
            // Only the FIRST read-only observation may still authorize the
            // retained group-wide cleanup: original leader reaped, group
            // signallable through a same-group descendant. Every other state,
            // including Darwin's ambiguous zombie-only EPERM, goes to the one
            // absence owner, which never signals and accepts only dual ESRCH
            // within the caller's reap deadline (a quarter of the existing
            // shutdown deadline unless a retained owner shares one budget).
            if probe_group_absence(raw) == (Err(Errno::ESRCH), Ok(())) {
                return signal_attested_group_once(group, reap_deadline);
            }
            // Keep this fallback's existing typed failure: callers and
            // their native oracles distinguish it from a later probe-only
            // absence timeout of a retained generation.
            require_attested_group_absence(group, reap_deadline)
                .map_err(|_| registered_identity_unavailable(observation))
        }
    }
}

// The fallback's typed failure when the original leader's birth could not be
// observed and the group was not proven absent. Shared by the host's in-call
// cleanup and the own-parent retirement, so callers and their native oracles
// see one shape.
fn registered_identity_unavailable(observation: RuntimeError) -> RuntimeError {
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
}

// The one group SIGKILL of an attested cleanup call, from either the
// birth-matched arm or the first-probe arm. XNU `killpg1` counts an exiting
// member as found but skips SZOMB members, and returns EPERM when it found
// none, so the last member finishing its exit just before this signal leaves
// a zombie-only group: ambiguous, neither a refused signal nor absence. The
// denied signal is never repeated in this call; the one signal-phase EPERM
// classifier hands it to the read-only absence owner inside this call's
// budget, and an unproven group keeps the typed signal-phase EPERM.
fn signal_attested_group_once(
    group: GroupIdentity,
    reap_deadline: Instant,
) -> Result<(), RuntimeError> {
    #[cfg(test)]
    run_test_group_signal_hook(group.pid, TestGroupSignalPoint::BeforeSignal);
    terminate_registered_group(group.pid).or_else(|denied| {
        #[cfg(test)]
        run_test_group_signal_hook(group.pid, TestGroupSignalPoint::AfterDeniedSignal);
        reconcile_denied_signal_after_reap(group, denied, reap_deadline)
    })
}

// One read-only kernel observation of the original leader PID and its whole
// process group. It carries no signal authority.
fn probe_group_absence(raw: i32) -> (Result<Pid, Errno>, Result<(), Errno>) {
    let leader = getpgid(Some(Pid::from_raw(raw)));
    let whole_group = kill(Pid::from_raw(-raw), None);
    #[cfg(test)]
    if matches!(
        (&leader, &whole_group),
        (Err(Errno::ESRCH), Err(Errno::EPERM))
    ) && let Ok(group) = u32::try_from(raw)
    {
        report_test_ambiguous_probe(group);
    }
    (leader, whole_group)
}

fn absence_timeout() -> RuntimeError {
    lifecycle_error(
        "macOS guardian generation clear group absence",
        io::Error::new(
            io::ErrorKind::TimedOut,
            "original group absence remains unproven after the shutdown budget",
        ),
    )
}

/// The single owner of post-signal group-absence proof. Host CLEAR, the
/// host fallback cleanup and the Guardian's post-reap EPERM reconciliation all
/// delegate here. A successful SIGKILL only proves signal delivery: the exact
/// leader or an inherited descendant can remain as a zombie until its parent
/// reaps it. Absence is proven only when both *read-only* kernel observations
/// report ESRCH and the decision itself still falls inside `deadline`.
///
/// This function never signals a raw PGID. A reused live leader is an error,
/// not new authority, and all inaccessible or persistent groups fail closed.
/// The two reads remain a point-in-time witness, not a kernel-pinned handle.
pub(super) fn require_attested_group_absence(
    group: GroupIdentity,
    deadline: Instant,
) -> Result<(), RuntimeError> {
    require_attested_group_absence_with_observers(
        group,
        deadline,
        Instant::now,
        probe_group_absence,
    )
}

// Keep time-of-observation and the read-only kernel witnesses explicit, so
// native tests can independently falsify both deadline guards. Shipping
// callers always supply the real monotonic clock and exact kernel probes;
// neither observer carries process-group signal authority. This injected
// seam stays private to the module, not another production absence API.
fn require_attested_group_absence_with_observers(
    group: GroupIdentity,
    deadline: Instant,
    mut now: impl FnMut() -> Instant,
    mut probe: impl FnMut(i32) -> (Result<Pid, Errno>, Result<(), Errno>),
) -> Result<(), RuntimeError> {
    let raw = validate_group_pid(group.pid)?;
    loop {
        if now() >= deadline {
            return Err(absence_timeout());
        }
        let (leader, whole_group) = probe(raw);
        // Both reads and the decision must fall inside the original budget;
        // dual ESRCH observed only after expiration cannot authorize success.
        let in_budget = now() < deadline;
        if leader == Err(Errno::ESRCH) && whole_group == Err(Errno::ESRCH) {
            return if in_budget {
                Ok(())
            } else {
                Err(absence_timeout())
            };
        }
        match leader {
            Ok(_) => {
                if let Ok(observed) = observe_group_identity(group.pid)
                    && observed != group
                {
                    return Err(lifecycle_error(
                        "macOS guardian generation clear group absence",
                        io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "group leader birth changed before confirmed native absence",
                        ),
                    ));
                }
            }
            Err(Errno::ESRCH) => {}
            Err(error) => {
                return Err(lifecycle_error(
                    "macOS guardian generation clear group absence",
                    nix_io_error(error),
                ));
            }
        }
        if let Err(error) = whole_group
            && !matches!(error, Errno::EPERM | Errno::ESRCH)
        {
            return Err(lifecycle_error(
                "macOS guardian generation clear group absence",
                nix_io_error(error),
            ));
        }
        if !in_budget {
            return Err(absence_timeout());
        }
        std::thread::sleep(GENERATION_CONTROL_POLL);
    }
}

// Native tests falsify each deadline guard of the one absence owner through
// the same private observer seam; shipping callers never reach it.
#[cfg(test)]
pub(super) fn require_attested_group_absence_for_test(
    group: GroupIdentity,
    deadline: Instant,
    now: impl FnMut() -> Instant,
    probe: impl FnMut(i32) -> (Result<Pid, Errno>, Result<(), Errno>),
) -> Result<(), RuntimeError> {
    require_attested_group_absence_with_observers(group, deadline, now, probe)
}

const GROUP_SIGNAL_PHASE: &str = "macOS guardian process-group signal";

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
            GROUP_SIGNAL_PHASE,
            io::Error::from_raw_os_error(error as i32),
        )),
    }
}

// The ONE classifier of a denied group signal. A group signal gets EPERM
// while every remaining member is a zombie, e.g. a killed direct child or
// same-group descendant not yet reaped, or the last member finishing its exit
// just before the signal. Two callers reconsider it: the attested cleanup
// right after its one signal (`signal_attested_group_once`), and the Guardian
// after it reaps its original direct Child. Neither a stale PID nor EPERM
// grants new kill authority. This function only classifies the exact
// signal-phase EPERM; the absence decision belongs to
// `require_attested_group_absence`, inside the caller's original budget. Any
// other failure is returned unchanged; an unproven absence keeps the typed
// signal-phase EPERM and adds the absence owner's reason as context.
pub(super) fn reconcile_denied_signal_after_reap(
    group: GroupIdentity,
    denied: RuntimeError,
    deadline: Instant,
) -> Result<(), RuntimeError> {
    match denied {
        RuntimeError::Lifecycle {
            phase: phase @ GROUP_SIGNAL_PHASE,
            source,
        } if is_denied_signal_source(&source) => require_attested_group_absence(group, deadline)
            .map_err(|absence| {
                lifecycle_error(
                    phase,
                    io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        UnprovenDeniedSignal {
                            denied: source,
                            absence: absence_reason(&absence),
                        },
                    ),
                )
            }),
        other => Err(other),
    }
}

// A denied group signal whose read-only absence check did not prove the group
// gone. Its source stays the kernel's raw signal-phase EPERM, so the one
// classifier still recognises exactly that denial (also after the Guardian's
// reap), and the absence owner's own reason, e.g. a changed leader birth or
// the spent budget, is kept as context instead of being discarded.
#[derive(Debug)]
struct UnprovenDeniedSignal {
    denied: io::Error,
    absence: String,
}

impl std::fmt::Display for UnprovenDeniedSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}; group absence after the denied signal is unproven: {}",
            self.denied, self.absence
        )
    }
}

impl std::error::Error for UnprovenDeniedSignal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.denied)
    }
}

// The exact signal-phase EPERM: the kernel's raw error, or that same raw error
// carried inside an unproven-absence context. Nothing else is a denial.
fn is_denied_signal_source(source: &io::Error) -> bool {
    source.raw_os_error() == Some(libc::EPERM)
        || source
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<UnprovenDeniedSignal>())
            .is_some_and(|context| is_denied_signal_source(&context.denied))
}

fn absence_reason(absence: &RuntimeError) -> String {
    match absence {
        RuntimeError::Lifecycle { phase, source } => format!("{phase}: {source}"),
        other => other.to_string(),
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
///   and, unproven, fails with the typed signal-phase EPERM carrying the
///   absence owner's reason;
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
    let signal_once = || {
        #[cfg(test)]
        run_test_group_signal_hook(group.pid, TestGroupSignalPoint::BeforeSignal);
        // Every signal failure waits for the reap: the one classifier,
        // `reconcile_denied_signal_after_reap`, decides afterwards and
        // returns anything but the exact signal-phase EPERM unchanged.
        match terminate_registered_group(group.pid) {
            Ok(()) => Decision::Done(Ok(())),
            Err(refused) => Decision::Denied(refused),
        }
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
            Ok(raw) => match probe_group_absence(raw) {
                (_, Err(Errno::ESRCH)) => Decision::Done(Ok(())),
                (Err(Errno::ESRCH), Ok(())) => signal_once(),
                // Our own unreaped leader (or another member) is a zombie:
                // decided only after this caller's reap.
                (Err(Errno::ESRCH), Err(Errno::EPERM)) => {
                    Decision::Ambiguous(registered_identity_unavailable(observation))
                }
                _ => Decision::Done(Err(registered_identity_unavailable(observation))),
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
