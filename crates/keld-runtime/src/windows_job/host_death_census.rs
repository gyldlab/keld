//! Native Windows host-death census: retained member identity and bounded release proof.
//!
//! The host-death Job owner retains both Job handles and alone clears kill-on-close
//! after a settled census. Observe never terminates or creates an updater release proof.

use std::io;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    ERROR_INVALID_PARAMETER, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
};

use super::{
    WindowsHostJobError, WindowsJobProcessIdSnapshot, process_in_job, process_signaled,
    wait_handle_until,
};

/// Witness of one host-death census (KEL-53 §4 "Candidate release after
/// commit", *Census and policy*): what the final clean snapshot showed and what
/// the census did on the way there.
///
/// It is also the proof that [`super::WindowsLaunchedProcess::detach`] takes by value
/// (*Launch-record detach*). Only [`super::WindowsHostDeathJob::release_for_exit`]
/// constructs it, after the attempt-Job clear its argument witnesses and the
/// host-death clear it performs, and it is neither `Clone` nor `Copy`, so one
/// census admits one detach. Outside this crate it cannot be built:
///
/// ```compile_fail,E0451
/// let forged = keld_runtime::windows_job::WindowsExitCensus {
///     family: 1,
///     terminated: 0,
///     snapshots: 1,
/// };
/// ```
///
/// nor cloned:
///
/// ```compile_fail,E0277
/// fn twice(census: &keld_runtime::windows_job::WindowsExitCensus) {
///     let _copy: keld_runtime::windows_job::WindowsExitCensus = Clone::clone(census);
/// }
/// ```
///
/// nor spent twice. The example that compiles detaches one record with one
/// census; the one below it differs only in the second `detach` of the same
/// census, which pins every other part of the snippet, and rustdoc checks the
/// error code on nightly builds only:
///
/// ```no_run
/// use keld_runtime::windows_job::{WindowsExitCensus, WindowsLaunchedProcess};
/// fn detach(record: WindowsLaunchedProcess, census: WindowsExitCensus) {
///     let _ = record.detach(census);
/// }
/// ```
///
/// ```compile_fail,E0382
/// use keld_runtime::windows_job::{WindowsExitCensus, WindowsLaunchedProcess};
/// fn detach(
///     first: WindowsLaunchedProcess,
///     second: WindowsLaunchedProcess,
///     census: WindowsExitCensus,
/// ) {
///     let _ = first.detach(census);
///     let _ = second.detach(census);
/// }
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct WindowsExitCensus {
    /// Members of the released family in the final snapshot: processes in both
    /// the host-death Job and the released attempt Job, the host excluded.
    pub(super) family: u32,
    /// Members outside the family that the census terminated through the handles
    /// it opened, over every snapshot. A member found already exited is not
    /// counted; the next snapshot no longer lists it.
    pub(super) terminated: u32,
    /// Snapshots of the host-death Job's process-ID list, the final clean one
    /// included.
    pub(super) snapshots: u32,
}

impl WindowsExitCensus {
    /// Members of the released family in the final snapshot: processes in both
    /// the host-death Job and the released attempt Job, the host excluded.
    #[must_use]
    pub const fn family(&self) -> u32 {
        self.family
    }

    /// Members outside the family that the census terminated through the handles
    /// it opened, over every snapshot. A member found already exited is not
    /// counted; the next snapshot no longer lists it.
    #[must_use]
    pub const fn terminated(&self) -> u32 {
        self.terminated
    }

    /// Snapshots of the host-death Job's process-ID list, the final clean one
    /// included.
    #[must_use]
    pub const fn snapshots(&self) -> u32 {
        self.snapshots
    }
}

/// One classifier and retake loop serve observation and consuming updater exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CensusPolicy {
    Observe,
    Terminate,
}

impl CensusPolicy {
    const fn member_rights(self) -> u32 {
        let query_and_wait = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE;
        match self {
            Self::Observe => query_and_wait,
            Self::Terminate => query_and_wait | PROCESS_TERMINATE,
        }
    }
}

/// Private census facts are not an updater detach capability.
struct CensusCounts {
    family: u32,
    terminated: u32,
    snapshots: u32,
}

const CENSUS_RETAKE_WAIT: Duration = Duration::from_millis(25);

/// Opens the reusable ID exactly once; classification and waits use that handle.
/// Observe never acquires `PROCESS_TERMINATE`. `ERROR_INVALID_PARAMETER` is the
/// documented missing-process outcome; other open failures remain errors.
pub(super) fn open_member_for_policy(
    process_id: u32,
    policy: CensusPolicy,
) -> io::Result<OwnedHandle> {
    // SAFETY: OpenProcess reads no caller memory. A non-null result is one fresh
    // handle with only the policy's rights, converted to one owner below.
    let raw = unsafe { OpenProcess(policy.member_rights(), 0, process_id) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: raw is the fresh non-null owning handle returned above.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
}

pub(super) fn open_census_member(process_id: u32) -> io::Result<OwnedHandle> {
    open_member_for_policy(process_id, CensusPolicy::Terminate)
}

/// Consuming updater exit retains its one-family API and distinct proof. The
/// caller still must clear the host-death limit before exposing this proof.
pub(super) fn exit_census(
    host_death_job: HANDLE,
    attempt_job: HANDLE,
    deadline: Instant,
    snapshot_ids: impl FnMut() -> Result<WindowsJobProcessIdSnapshot, WindowsHostJobError>,
    open_member: impl FnMut(u32) -> io::Result<OwnedHandle>,
) -> Result<WindowsExitCensus, WindowsHostJobError> {
    let counts = census(
        host_death_job,
        &[attempt_job],
        CensusPolicy::Terminate,
        deadline,
        snapshot_ids,
        open_member,
    )?;
    Ok(WindowsExitCensus {
        family: counts.family,
        terminated: counts.terminated,
        snapshots: counts.snapshots,
    })
}

/// Observation borrows every retained family Job. It cannot clear a Job limit,
/// terminate a process or mint the consuming updater's detach proof.
pub(super) fn observe_census(
    host_death_job: HANDLE,
    family_jobs: &[HANDLE],
    deadline: Instant,
    snapshot_ids: impl FnMut() -> Result<WindowsJobProcessIdSnapshot, WindowsHostJobError>,
) -> Result<(), WindowsHostJobError> {
    let policy = CensusPolicy::Observe;
    census(
        host_death_job,
        family_jobs,
        policy,
        deadline,
        snapshot_ids,
        |pid| open_member_for_policy(pid, policy),
    )
    .map(|_| ())
}

fn check_deadline(deadline: Instant, snapshots: u32) -> Result<(), WindowsHostJobError> {
    if Instant::now() >= deadline {
        return Err(WindowsHostJobError::contract(
            "host-death census",
            format!(
                "the deadline passed before a clean census, with a member outside the \
                 retained family or in transition after {snapshots} snapshot(s); not released"
            ),
        ));
    }
    Ok(())
}

/// Every unclean retake has a bounded wait, even if the only discrepancy is a
/// kernel count/list transition. Round sub-millisecond remainder up to one OS
/// wait tick; never accept a clean result after the original absolute deadline.
fn wait_to_retake(
    policy: CensusPolicy,
    live_member: Option<&OwnedHandle>,
    deadline: Instant,
    snapshots: u32,
) -> Result<(), WindowsHostJobError> {
    check_deadline(deadline, snapshots)?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    // A live observed member is itself the wake source. Only transitions with
    // no live handle and updater termination retries need periodic retakes.
    let wait = if policy == CensusPolicy::Observe && live_member.is_some() {
        remaining
    } else {
        remaining.min(CENSUS_RETAKE_WAIT)
    };
    // Keep even an unusually distant caller deadline finite; u32::MAX means
    // INFINITE to Win32. The next round checks the original absolute deadline.
    let millis = u32::try_from(wait.as_nanos().div_ceil(1_000_000))
        .unwrap_or(u32::MAX - 1)
        .min(u32::MAX - 1);
    // SAFETY: GetCurrentProcess returns a process-lifetime pseudo handle. It is
    // not owned or closed and cannot signal while this caller is running.
    let current = unsafe { GetCurrentProcess() };
    let raw = live_member.map_or(current, |member| member.as_raw_handle().cast());
    // SAFETY: the caller retains a query/synchronize member handle through this
    // call, or raw is the current-process pseudo handle above. The finite wait
    // ends on that member's exit or the remaining deadline (rounded up by less
    // than one millisecond); transition/termination retries wait at most 25 ms.
    match unsafe { WaitForSingleObject(raw, millis.max(1)) } {
        WAIT_OBJECT_0 | WAIT_TIMEOUT => Ok(()),
        _ => Err(WindowsHostJobError::new("host-death census retake wait")),
    }
}

fn census(
    host_death_job: HANDLE,
    family_jobs: &[HANDLE],
    policy: CensusPolicy,
    deadline: Instant,
    mut snapshot_ids: impl FnMut() -> Result<WindowsJobProcessIdSnapshot, WindowsHostJobError>,
    mut open_member: impl FnMut(u32) -> io::Result<OwnedHandle>,
) -> Result<CensusCounts, WindowsHostJobError> {
    let own_process_id = std::process::id();
    let mut terminated = 0_u32;
    let mut snapshots = 0_u32;
    loop {
        check_deadline(deadline, snapshots)?;
        snapshots = snapshots.saturating_add(1);
        let snapshot = snapshot_ids()?;
        let mut clean = snapshot.is_settled();
        let mut family = 0_u32;
        let mut terminating = Vec::new();
        let mut waiting = None;
        for process_id in snapshot.ids {
            check_deadline(deadline, snapshots)?;
            if process_id == own_process_id {
                continue;
            }
            let member = match open_member(process_id) {
                Ok(member) => member,
                Err(source)
                    if source.raw_os_error() == Some(ERROR_INVALID_PARAMETER.cast_signed()) =>
                {
                    clean = false;
                    continue;
                }
                Err(source) => {
                    return Err(WindowsHostJobError {
                        phase: "host-death census member open",
                        source,
                    });
                }
            };
            let raw = member.as_raw_handle().cast();
            if !process_in_job(raw, host_death_job, "host-death census Job membership")? {
                clean = false;
                continue;
            }
            let mut in_family = false;
            for &job in family_jobs {
                in_family |= process_in_job(raw, job, "host-death census family membership")?;
            }
            if in_family {
                family = family.saturating_add(1);
                continue;
            }
            clean = false;
            if process_signaled(raw).map_err(|source| WindowsHostJobError {
                phase: "host-death census member state query",
                source,
            })? {
                continue;
            }
            if policy == CensusPolicy::Observe {
                waiting.get_or_insert(member);
                continue;
            }
            // SAFETY: only Terminate policy reaches this call. The retained
            // handle identifies a live member of the host's own Job outside
            // every retained family. Production opens it with PROCESS_TERMINATE.
            if unsafe { TerminateProcess(raw, 1) } == 0 {
                waiting.get_or_insert(member);
                continue;
            }
            terminated = terminated.saturating_add(1);
            terminating.push(member);
        }
        for member in &terminating {
            wait_handle_until(
                member.as_raw_handle().cast(),
                deadline,
                "host-death census member exit",
            )?;
        }
        check_deadline(deadline, snapshots)?;
        if clean {
            return Ok(CensusCounts {
                family,
                terminated,
                snapshots,
            });
        }
        wait_to_retake(policy, waiting.as_ref(), deadline, snapshots)?;
    }
}
