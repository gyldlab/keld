//! Windows host-death descendant ownership for KEL-78/T3.
//!
//! This is supervisor cleanup, not LPAC containment. The host installs one
//! unnamed, non-inheritable Job before any Bun role exists. The host is a Job
//! member, so later children inherit membership without a spawn/assignment
//! race. The sole Job handle intentionally lives until process termination;
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` then terminates the enrolled tree even
//! when host destructors cannot run.

#![allow(unsafe_code)] // isolated Win32 Job ABI; every call has a local handle/pointer proof
#![deny(unsafe_op_in_unsafe_fn)]

use std::io;
use std::os::windows::io::{FromRawHandle as _, OwnedHandle};

use windows_sys::Win32::Foundation::{
    GetHandleInformation, HANDLE_FLAG_INHERIT, SetHandleInformation,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// Verified facts about the installed host-death Job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsHostJobObservation {
    /// The exact configured limit flags.
    pub limit_flags: u32,
    /// Whether the host was already inside an outer Job before nesting Keld's
    /// own unnamed Job.
    pub nested_under_existing_job: bool,
    /// Assignment of the current host process to the Keld Job succeeded.
    pub current_process_assigned: bool,
    /// The sole Job handle was verified non-inheritable before assignment.
    pub handle_inheritable: bool,
}

/// Failure to install the Windows host-death Job before child creation.
#[derive(Debug)]
pub struct WindowsHostJobError {
    phase: &'static str,
    source: io::Error,
}

impl WindowsHostJobError {
    fn new(phase: &'static str) -> Self {
        Self {
            phase,
            source: io::Error::last_os_error(),
        }
    }
}

impl std::fmt::Display for WindowsHostJobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "KELD-RUNTIME-014: Windows host-death Job installation failed during {}: {}. \
             Stop before spawning Bun; verify nested Job support and retry.",
            self.phase, self.source
        )
    }
}

impl std::error::Error for WindowsHostJobError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Installs the one process-lifetime Windows Job that reaps Bun descendants
/// when the host dies abnormally.
///
/// Call exactly once, before the first supervised child or listener is
/// created. The unnamed Job grants no open-by-name path. Its handle is
/// explicitly non-inheritable and intentionally retained by the OS until host
/// termination; returning it would let a caller leak lifecycle ownership.
///
/// # Errors
///
/// Fails closed when Job creation/configuration, nested assignment, or exact
/// flag/handle verification fails. No child may be spawned after an error.
pub fn install_host_death_job() -> Result<WindowsHostJobObservation, WindowsHostJobError> {
    let mut outer_job = 0;
    // SAFETY: GetCurrentProcess returns a non-owning pseudo-handle valid for
    // this process lifetime; `outer_job` is live writable BOOL storage. A null
    // Job handle asks whether the process belongs to any Job.
    if unsafe {
        IsProcessInJob(
            GetCurrentProcess(),
            std::ptr::null_mut(),
            &raw mut outer_job,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new("outer Job observation"));
    }

    // SAFETY: both optional inputs are null, requesting an unnamed Job with
    // default non-inheritable security attributes. The returned owning handle
    // is checked before conversion.
    let raw_job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if raw_job.is_null() {
        return Err(WindowsHostJobError::new("CreateJobObjectW"));
    }
    // SAFETY: `raw_job` is a fresh non-null owning handle returned above and
    // is converted exactly once.
    let job = unsafe { OwnedHandle::from_raw_handle(raw_job.cast()) };

    // Defense in depth: default SECURITY_ATTRIBUTES already makes the handle
    // non-inheritable. Clear the bit explicitly before any child exists.
    // SAFETY: the Job handle is live and owned by `job`; this call changes one
    // flag and retains no pointer.
    if unsafe { SetHandleInformation(raw_job, HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(WindowsHostJobError::new("Job handle inheritance clear"));
    }

    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let limits_size =
        u32::try_from(std::mem::size_of_val(&limits)).map_err(|_| WindowsHostJobError {
            phase: "Job limit structure size",
            source: io::Error::other("JOBOBJECT_EXTENDED_LIMIT_INFORMATION exceeds u32"),
        })?;
    // SAFETY: `limits` is a live initialized structure of `limits_size` bytes;
    // SetInformationJobObject copies it synchronously and retains no pointer.
    if unsafe {
        SetInformationJobObject(
            raw_job,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            limits_size,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new("Job limit configuration"));
    }

    let mut observed_limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    // SAFETY: `observed_limits` is live writable storage of exactly
    // `limits_size`; the optional returned-length pointer is not needed.
    if unsafe {
        QueryInformationJobObject(
            raw_job,
            JobObjectExtendedLimitInformation,
            (&raw mut observed_limits).cast(),
            limits_size,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(WindowsHostJobError::new("Job limit readback"));
    }
    let observed_flags = observed_limits.BasicLimitInformation.LimitFlags;
    let forbidden = JOB_OBJECT_LIMIT_BREAKAWAY_OK | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
    if observed_flags != JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE || observed_flags & forbidden != 0 {
        return Err(WindowsHostJobError {
            phase: "Job limit readback",
            source: io::Error::other(format!(
                "expected only KILL_ON_JOB_CLOSE, observed 0x{observed_flags:08x}"
            )),
        });
    }

    let mut handle_flags = 0_u32;
    // SAFETY: `raw_job` remains live and `handle_flags` is writable storage.
    if unsafe { GetHandleInformation(raw_job, &raw mut handle_flags) } == 0 {
        return Err(WindowsHostJobError::new("Job handle inheritance readback"));
    }
    if handle_flags & HANDLE_FLAG_INHERIT != 0 {
        return Err(WindowsHostJobError {
            phase: "Job handle inheritance readback",
            source: io::Error::other("Job handle remains inheritable"),
        });
    }

    // Assignment is deliberately last: every prior failure can close an empty
    // Job harmlessly. The current process may already belong to a CI/launcher
    // Job; Windows 8+ nested Job semantics make this Keld Job the immediate
    // child owner when the outer Job permits nesting.
    // SAFETY: both handles are live for the call; GetCurrentProcess is a
    // non-owning pseudo-handle and `raw_job` is owned by `job`.
    if unsafe { AssignProcessToJobObject(raw_job, GetCurrentProcess()) } == 0 {
        return Err(WindowsHostJobError::new("current-process Job assignment"));
    }

    let observation = WindowsHostJobObservation {
        limit_flags: observed_flags,
        nested_under_existing_job: outer_job != 0,
        current_process_assigned: true,
        handle_inheritable: false,
    };

    // This is an intentional process-lifetime handle, not a recoverable leak:
    // closing it while the host is alive would synchronously terminate the
    // host and every enrolled child. The kernel closes it on every abnormal or
    // orderly process-termination path, which is the mechanism being proved.
    std::mem::forget(job);
    Ok(observation)
}
