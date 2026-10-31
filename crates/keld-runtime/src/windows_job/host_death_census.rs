//! Native Windows host-death census: retained member identity and bounded release proof.
//!
//! The host-death Job owner retains both Job handles and alone clears kill-on-close
//! after a settled census. This module observes and terminates eligible members.

use std::io;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{ERROR_INVALID_PARAMETER, HANDLE};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    TerminateProcess,
};

use super::{
    WindowsHostJobError, WindowsJobProcessIdSnapshot, process_in_job, process_signaled,
    wait_handle_bounded, wait_handle_until,
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

/// The rights the census needs on one listed member: classification through
/// `IsProcessInJob`, the exit wait and, outside the family, termination.
const CENSUS_MEMBER_RIGHTS: u32 =
    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE;
/// Bound on one wait for a member whose termination was denied, before the
/// next snapshot. The deadline refusal covers a member that never exits.
const CENSUS_DENIED_MEMBER_WAIT: Duration = Duration::from_millis(25);

/// Opens one member of the host-death Job by the ID a snapshot listed, with
/// exactly the census rights. This is the census's only lookup of that reusable
/// ID: it classifies, waits and terminates through the handle returned here.
///
/// An ID that names no process object fails with `ERROR_INVALID_PARAMETER`, the
/// status the census reads as "exited after the snapshot". An exited member
/// whose object another open handle still keeps alive opens like a live one;
/// its handle is signaled, and the census classifies it as any other member.
/// Which of the two an exited ID shows depends on handles other processes
/// hold, so the census relies on both.
pub(super) fn open_census_member(process_id: u32) -> io::Result<OwnedHandle> {
    // SAFETY: OpenProcess reads no caller memory; a numeric ID that names no
    // process object makes it fail. A non-null result is one fresh owning
    // handle, converted exactly once below.
    let raw = unsafe { OpenProcess(CENSUS_MEMBER_RIGHTS, 0, process_id) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is the fresh non-null owning handle returned above.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
}

/// The host-death census of [`super::WindowsHostDeathJob::release_for_exit`]. The
/// Jobs' owners retain both handles through the call.
pub(super) fn exit_census(
    host_death_job: HANDLE,
    attempt_job: HANDLE,
    deadline: Instant,
    mut snapshot_ids: impl FnMut() -> Result<WindowsJobProcessIdSnapshot, WindowsHostJobError>,
    mut open_member: impl FnMut(u32) -> io::Result<OwnedHandle>,
) -> Result<WindowsExitCensus, WindowsHostJobError> {
    let own_process_id = std::process::id();
    let mut terminated = 0_u32;
    let mut snapshots = 0_u32;
    loop {
        snapshots = snapshots.saturating_add(1);
        let snapshot = snapshot_ids()?;
        // The kernel's count and list agree except while a membership change is in
        // flight (KEL-270 F54); such a snapshot is retaken under the deadline, like
        // one with an exited or reused ID, and is never a refusal.
        let mut clean = snapshot.is_settled();
        let mut family = 0_u32;
        let mut terminating = Vec::new();
        let mut denied = Vec::new();
        for process_id in snapshot.ids {
            if process_id == own_process_id {
                continue;
            }
            let member = match open_member(process_id) {
                Ok(member) => member,
                Err(source)
                    if source.raw_os_error() == Some(ERROR_INVALID_PARAMETER.cast_signed()) =>
                {
                    // The member exited after the snapshot; the next one omits it.
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
                // The listed ID was reused by a process outside the Job.
                clean = false;
                continue;
            }
            if process_in_job(raw, attempt_job, "host-death census family membership")? {
                family = family.saturating_add(1);
                continue;
            }
            clean = false;
            if process_signaled(raw).map_err(|source| WindowsHostJobError {
                phase: "host-death census member state query",
                source,
            })? {
                // Already terminated; the list removes it on its own.
                continue;
            }
            // SAFETY: `member` is the live handle this census opened with
            // PROCESS_TERMINATE; the call reads no caller memory. The member is a
            // process of the host's own Job outside the released family, which
            // closing that Job at exit would terminate in the same way.
            if unsafe { TerminateProcess(raw, 1) } == 0 {
                // Denied, as for a protected process: it stays in the next
                // snapshot and the deadline refusal covers it.
                denied.push(member);
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
        if clean {
            return Ok(WindowsExitCensus {
                family,
                terminated,
                snapshots,
            });
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(WindowsHostJobError::contract(
                "host-death census",
                format!(
                    "the deadline passed with a member outside the released family or in \
                     transition still in the host-death Job after {snapshots} snapshot(s); \
                     not released"
                ),
            ));
        }
        if let Some(member) = denied.first() {
            // A member the census could not terminate paces the next snapshot
            // through a bounded wait on its own handle, in case it exits on its
            // own; nothing here waits for a member that is already signaled.
            let bound = now
                .checked_add(CENSUS_DENIED_MEMBER_WAIT)
                .map_or(deadline, |bound| bound.min(deadline));
            wait_handle_bounded(
                member.as_raw_handle().cast(),
                bound,
                "host-death census denied-member wait",
            )?;
        }
    }
}
