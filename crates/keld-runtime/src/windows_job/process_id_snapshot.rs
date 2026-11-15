//! Windows Job process-ID snapshot: bounded query, retake and settle semantics.
//! The parent retains lifecycle, handle binding, membership and release policy.

use std::io;

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    JOBOBJECT_BASIC_PROCESS_ID_LIST, JobObjectBasicProcessIdList, QueryInformationJobObject,
};

use super::WindowsHostJobError;

/// Initial capacity, in process IDs, of one Job process-ID snapshot. A host, its
/// Bun primary, its `WebView2` processes and a candidate family fit; a larger Job
/// grows the buffer.
pub(super) const JOB_PROCESS_ID_LIST_INITIAL_IDS: usize = 32;
/// IDs added beyond the reported count when a snapshot buffer grows, for members
/// that join between the report and the next query.
const JOB_PROCESS_ID_LIST_GROWTH_SLACK: usize = 16;
/// Bound on buffer growth within one snapshot; a Job whose member count outruns
/// it that often is refused rather than queried without end.
pub(super) const JOB_PROCESS_ID_LIST_GROWTH_LIMIT: u32 = 8;

/// One snapshot of a Job hierarchy's process-ID list.
#[derive(Debug)]
pub(super) struct WindowsJobProcessIdSnapshot {
    /// Every ID the kernel listed: the members it considers in the Job at the
    /// instant of the query. Complete on success; an exited member is omitted
    /// only after its handle is signaled and every thread of it has stopped.
    pub(super) ids: Vec<u32>,
    /// `NumberOfAssignedProcesses` of the same query. It exceeds `ids.len()` for
    /// one lock release while a member leaves the Job; it is never a buffer
    /// size.
    pub(super) assigned: u32,
}

impl WindowsJobProcessIdSnapshot {
    /// Whether the kernel's assigned count and its list agree.
    ///
    /// Every census caller MUST require this before it treats a snapshot as clean,
    /// the Machine-UAC census of S12 included: a disagreement names a membership
    /// change in flight (KEL-270 finding F54), so the snapshot is retaken under the
    /// caller's deadline, never refused and never accepted.
    pub(super) fn is_settled(&self) -> bool {
        usize::try_from(self.assigned).is_ok_and(|assigned| assigned == self.ids.len())
    }
}

/// Words of the process-ID list record that precede the ID array. The record is
/// two u32 counts followed by the `ULONG_PTR` ID array, so a usize buffer holds
/// it at the alignment it requires and the IDs start at a whole-word offset.
pub(super) const JOB_PROCESS_ID_LIST_HEADER_WORDS: usize =
    std::mem::offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList)
        / std::mem::size_of::<usize>();
const _: () = assert!(
    std::mem::offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList)
        % std::mem::size_of::<usize>()
        == 0
);

/// Lists the IDs of the live processes in one Job hierarchy: the process-ID
/// lister of KEL-53 §5 (slice S6b3), which the host-death census uses and the
/// Machine-UAC census of S12 reuses.
///
/// The list is a snapshot. A member can exit, and its ID be reused, after the
/// call returns, so callers bind each ID to a process handle and check that
/// process's Job membership before acting on it.
///
/// The buffer grows and the query is retaken only on the kernel's own "too
/// small" signal: a query that fails with `ERROR_MORE_DATA` (the header is
/// written, so the assigned count sizes the next buffer), or one that succeeds
/// with a full buffer. A successful list shorter than its buffer is complete.
/// `assigned > ids.len()` names a member whose exit the kernel has not finished
/// accounting; it is not a truncated list, and a larger buffer cannot change it
/// (KEL-270 finding F54).
pub(super) fn list_job_process_ids(
    raw_job: HANDLE,
    phase: &'static str,
) -> Result<WindowsJobProcessIdSnapshot, WindowsHostJobError> {
    list_job_process_ids_from(raw_job, JOB_PROCESS_ID_LIST_INITIAL_IDS, phase)
}

/// [`list_job_process_ids`] from a caller-chosen initial capacity, so the growth
/// path is reachable with a small Job.
pub(super) fn list_job_process_ids_from(
    raw_job: HANDLE,
    initial_ids: usize,
    phase: &'static str,
) -> Result<WindowsJobProcessIdSnapshot, WindowsHostJobError> {
    list_job_process_ids_with(initial_ids, phase, |buffer| {
        query_job_process_id_list(raw_job, buffer)
    })
}

/// One `QueryInformationJobObject` of the process-ID list into `buffer`, which
/// holds the record header and the ID array.
fn query_job_process_id_list(raw_job: HANDLE, buffer: &mut [usize]) -> io::Result<()> {
    let bytes = u32::try_from(std::mem::size_of_val(&*buffer)).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "process-ID list buffer exceeds u32",
        )
    })?;
    // SAFETY: the caller retains the live Job handle; `buffer` is writable,
    // usize-aligned storage of exactly `bytes`, which holds the record header
    // and the IDs; the API writes within that size and retains no pointer.
    let queried = unsafe {
        QueryInformationJobObject(
            raw_job,
            JobObjectBasicProcessIdList,
            buffer.as_mut_ptr().cast(),
            bytes,
            std::ptr::null_mut(),
        )
    };
    if queried == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// [`list_job_process_ids`] with the query as a parameter. Production passes
/// [`query_job_process_id_list`]; the tests inject the kernel's outcomes. The
/// query writes the record into the buffer it is given: `Err(ERROR_MORE_DATA)`
/// with the header written when the buffer is too small, any other error as the
/// OS reports it.
pub(super) fn list_job_process_ids_with(
    initial_ids: usize,
    phase: &'static str,
    mut query: impl FnMut(&mut [usize]) -> io::Result<()>,
) -> Result<WindowsJobProcessIdSnapshot, WindowsHostJobError> {
    let mut capacity = initial_ids.max(1);
    for _ in 0..JOB_PROCESS_ID_LIST_GROWTH_LIMIT {
        let mut buffer = vec![0_usize; JOB_PROCESS_ID_LIST_HEADER_WORDS + capacity];
        let short_buffer = match query(&mut buffer) {
            Ok(()) => false,
            Err(source) if source.raw_os_error() == Some(ERROR_MORE_DATA.cast_signed()) => true,
            Err(source) => return Err(WindowsHostJobError { phase, source }),
        };
        // SAFETY: `buffer` starts with at least the record's size in initialized
        // bytes at the alignment the record requires; the read copies the header
        // out and keeps no pointer into the buffer.
        let header = unsafe {
            buffer
                .as_ptr()
                .cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>()
                .read()
        };
        let assigned = usize::try_from(header.NumberOfAssignedProcesses).map_err(|_| {
            WindowsHostJobError::contract(phase, "assigned-process count exceeds usize")
        })?;
        let listed = usize::try_from(header.NumberOfProcessIdsInList).map_err(|_| {
            WindowsHostJobError::contract(phase, "listed-process count exceeds usize")
        })?;
        if listed > capacity {
            return Err(WindowsHostJobError::contract(
                phase,
                "the Job reported more process IDs than the snapshot buffer holds",
            ));
        }
        if short_buffer || listed == capacity {
            capacity = assigned
                .max(capacity.saturating_mul(2))
                .saturating_add(JOB_PROCESS_ID_LIST_GROWTH_SLACK);
            continue;
        }
        let ids = buffer
            .iter()
            .skip(JOB_PROCESS_ID_LIST_HEADER_WORDS)
            .take(listed)
            .map(|&id| {
                u32::try_from(id)
                    .map_err(|_| WindowsHostJobError::contract(phase, "process ID exceeds u32"))
            })
            .collect::<Result<Vec<u32>, _>>()?;
        return Ok(WindowsJobProcessIdSnapshot {
            ids,
            assigned: header.NumberOfAssignedProcesses,
        });
    }
    Err(WindowsHostJobError::contract(
        phase,
        "the Job's process list outgrew the snapshot buffer at every retake",
    ))
}
