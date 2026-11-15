//! One retained-process claimant identity and kernel-object comparison policy.
//! The caller retains the process handles throughout each observation.

use std::io;
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};

use windows_sys::Win32::Foundation::{
    CompareObjectHandles, ERROR_NOT_SAME_OBJECT, FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::JobObjects::IsProcessInJob;
use windows_sys::Win32::System::Threading::{GetProcessTimes, WaitForSingleObject};

use super::{
    WindowsClaimantRefusal, WindowsHostJobError, WindowsLaunchedProcess, WindowsProcessPeer,
};

/// Process-object identity compared by the claimant binding. The lifetime keeps
/// the raw handle's owner alive while the observation is in use.
#[derive(Clone, Copy)]
pub(super) struct ProcessObservation<'a> {
    pub(super) handle: HANDLE,
    pub(super) process_id: u32,
    pub(super) creation_time: u64,
    pub(super) _owner: std::marker::PhantomData<&'a OwnedHandle>,
}

impl<'a> ProcessObservation<'a> {
    pub(super) fn of_peer(peer: &'a WindowsProcessPeer) -> Self {
        Self {
            handle: peer.process.as_raw_handle().cast(),
            process_id: peer.process_id,
            creation_time: peer.creation_time,
            _owner: std::marker::PhantomData,
        }
    }

    pub(super) fn of_launch(launch: &'a WindowsLaunchedProcess) -> Self {
        Self {
            handle: launch.child.process_handle().as_raw_handle().cast(),
            process_id: launch.child.id(),
            creation_time: launch.creation_time,
            _owner: std::marker::PhantomData,
        }
    }
}

/// Requires the three acceptance facts in their specified order.
pub(super) fn bind_observed_claimant(
    claimant: ProcessObservation<'_>,
    launch: ProcessObservation<'_>,
) -> Result<(), WindowsClaimantRefusal> {
    match same_kernel_object(claimant.handle, launch.handle) {
        Ok(true) => {}
        Ok(false) => return Err(WindowsClaimantRefusal::NotLaunchedProcess),
        Err(source) => {
            return Err(WindowsClaimantRefusal::Unverifiable {
                phase: "claimant process-object comparison",
                source,
            });
        }
    }
    // The launch observation borrows its owner, which retains the launch handle
    // through this query.
    match process_signaled(launch.handle) {
        Ok(false) => {}
        Ok(true) => return Err(WindowsClaimantRefusal::LaunchExited),
        Err(source) => {
            return Err(WindowsClaimantRefusal::Unverifiable {
                phase: "launch handle state query",
                source,
            });
        }
    }
    if claimant.process_id != launch.process_id || claimant.creation_time != launch.creation_time {
        return Err(WindowsClaimantRefusal::LaunchIdentityMismatch);
    }
    Ok(())
}

/// Reports whether a process handle is signaled, with a zero-time wait that
/// consumes nothing. The caller retains the handle through the call.
pub(super) fn process_signaled(process: HANDLE) -> io::Result<bool> {
    // SAFETY: WaitForSingleObject reads no caller memory and resolves `process`
    // only through this process's handle table, so no value makes the call
    // unsound. That the caller retains this handle through the call is a
    // correctness precondition: a closed value that was reused would name another
    // object and would not fail.
    match unsafe { WaitForSingleObject(process, 0) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => Err(io::Error::last_os_error()),
    }
}

/// Reports whether one process is a member of one Job hierarchy, by the handles
/// their owners retain through the call. A null `job` asks whether the process
/// belongs to any Job.
pub(super) fn process_in_job(
    process: HANDLE,
    job: HANDLE,
    phase: &'static str,
) -> Result<bool, WindowsHostJobError> {
    let mut in_job = 0;
    // SAFETY: the callers retain both handles through this synchronous call,
    // IsProcessInJob resolves them only through this process's handle table, and
    // `in_job` is writable BOOL storage.
    if unsafe { IsProcessInJob(process, job, &raw mut in_job) } == 0 {
        return Err(WindowsHostJobError::new(phase));
    }
    Ok(in_job != 0)
}

/// Reports whether two handles name one kernel object.
///
/// `CompareObjectHandles` requires no access right on either handle. Its
/// reference page documents only the return value: `TRUE` for one object,
/// otherwise `FALSE`. Its example names `ERROR_NOT_SAME_OBJECT` only for two
/// objects of different types, an event and a process. That two different
/// process objects leave `ERROR_NOT_SAME_OBJECT`, and that a null, out-of-range
/// or closed handle leaves `ERROR_INVALID_HANDLE`, is observed only on Windows 11
/// build 26300, pinned by
/// `compare_object_handles_classifies_same_different_and_invalid_handles` and
/// `compare_object_handles_refuses_a_closed_handle`. So `FALSE` with
/// `ERROR_NOT_SAME_OBJECT` is the only different-object verdict, and any other
/// `FALSE` is an error. Every non-`TRUE` result refuses the claimant: the verdict
/// as [`WindowsClaimantRefusal::NotLaunchedProcess`], an error as
/// [`WindowsClaimantRefusal::Unverifiable`].
pub(super) fn same_kernel_object(first: HANDLE, second: HANDLE) -> io::Result<bool> {
    // SAFETY: CompareObjectHandles only resolves the two handle values through
    // this process's handle table and dereferences no caller memory; an unusable
    // value makes it fail without touching any object.
    if unsafe { CompareObjectHandles(first, second) } != 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_NOT_SAME_OBJECT.cast_signed()) {
        return Ok(false);
    }
    Err(error)
}

/// Reads one retained process object's creation time in FILETIME units.
pub(super) fn process_creation_time(
    process: HANDLE,
    phase: &'static str,
) -> Result<u64, WindowsHostJobError> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: the caller retains the process handle and all four FILETIME outputs
    // are writable.
    if unsafe {
        GetProcessTimes(
            process,
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    } == 0
    {
        return Err(WindowsHostJobError::new(phase));
    }
    Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}
