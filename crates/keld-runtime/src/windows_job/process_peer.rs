//! Retained identity pin for a process observed on a connected Windows pipe.
//! The pin is one exact kernel object, never a reopened PID.

use std::io;
use std::os::windows::ffi::OsStringExt as _;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::PathBuf;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    DUPLICATE_SAME_ACCESS, DuplicateHandle, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, WaitForSingleObject,
};

use super::{WindowsHostJobError, finite_wait_millis, process_creation_time, process_token_facts};

/// Retained process-object identity observed from a connected Windows pipe peer.
///
/// The retained handle prevents PID reuse from changing this observation. The
/// caller still compares `image_path` against a trusted installed-image digest;
/// a PID or pipe DACL alone is not executable authentication.
#[derive(Debug)]
pub struct WindowsProcessPeer {
    pub(super) process: OwnedHandle,
    pub(super) process_id: u32,
    session_id: u32,
    pub(super) token_facts: keld_ipc::WindowsPeerTokenFacts,
    pub(super) creation_time: u64,
    image_path: PathBuf,
}

impl WindowsProcessPeer {
    /// Opens and pins the exact PID and session reported by the connected pipe.
    ///
    /// # Errors
    ///
    /// Returns an error if the process exits before observation, its PID/session
    /// differs from the pipe report, or its image/creation time cannot be read.
    pub fn open(process_id: u32, pipe_session_id: u32) -> Result<Self, WindowsHostJobError> {
        // SAFETY: the PID came from the OS pipe endpoint query; only identity
        // query and synchronization rights are requested.
        let raw_process = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                process_id,
            )
        };
        if raw_process.is_null() {
            return Err(WindowsHostJobError::new("keeper peer process open"));
        }
        // SAFETY: OpenProcess returned one fresh non-null owning process handle.
        let process = unsafe { OwnedHandle::from_raw_handle(raw_process.cast()) };
        // SAFETY: the retained process handle is live; GetProcessId is read-only.
        let observed_id = unsafe { GetProcessId(process.as_raw_handle().cast()) };
        if observed_id != process_id {
            return Err(WindowsHostJobError::contract(
                "keeper peer process identity",
                "opened process ID differs from the connected pipe peer ID",
            ));
        }
        let token_facts = process_token_facts(process.as_raw_handle().cast())?;
        let observed_session = token_facts.session_id;
        if observed_session != pipe_session_id {
            return Err(WindowsHostJobError::contract(
                "keeper peer session identity",
                "process session differs from the connected pipe session",
            ));
        }
        let mut image = vec![0_u16; 32_768];
        let mut image_units = u32::try_from(image.len()).map_err(|_| {
            WindowsHostJobError::contract(
                "keeper peer image path",
                "image buffer exceeds the Win32 size bound",
            )
        })?;
        // SAFETY: process is retained, image is writable for image_units UTF-16
        // code units, and image_units is writable length storage.
        if unsafe {
            QueryFullProcessImageNameW(
                process.as_raw_handle().cast(),
                0,
                image.as_mut_ptr(),
                &raw mut image_units,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new("keeper peer image query"));
        }
        let image_units = usize::try_from(image_units).map_err(|_| {
            WindowsHostJobError::contract(
                "keeper peer image path",
                "returned image path length exceeds usize",
            )
        })?;
        if image_units == 0 || image_units > image.len() {
            return Err(WindowsHostJobError::contract(
                "keeper peer image path",
                "returned image path length is invalid",
            ));
        }
        let image_path = PathBuf::from(std::ffi::OsString::from_wide(&image[..image_units]));
        let creation_time = process_creation_time(
            process.as_raw_handle().cast(),
            "keeper peer creation-time query",
        )?;
        // A process can exit after the pipe reports its PID. Refuse a stale
        // process object before returning this connected-peer observation.
        // The protocol challenge/ack must still bind the live connection.
        // SAFETY: `process` owns the exact process handle from `OpenProcess`
        // throughout this zero-time, read-only signal query.
        match unsafe { WaitForSingleObject(process.as_raw_handle().cast(), 0) } {
            WAIT_TIMEOUT => {}
            WAIT_OBJECT_0 => {
                return Err(WindowsHostJobError::contract(
                    "keeper peer process state query",
                    "pipe peer exited before its identity was fully observed",
                ));
            }
            _ => return Err(WindowsHostJobError::new("keeper peer process state query")),
        }
        Ok(Self {
            process,
            process_id,
            session_id: observed_session,
            token_facts,
            creation_time,
            image_path,
        })
    }

    /// Duplicates this exact retained process-object observation for an independent
    /// keeper owner; it does not reopen by PID or mint a new peer identity.
    ///
    /// # Errors
    ///
    /// Returns an error if Windows cannot duplicate the retained process handle.
    pub fn try_clone(&self) -> Result<Self, WindowsHostJobError> {
        let mut duplicate = std::ptr::null_mut();
        // SAFETY: `self.process` is an owned live process handle; SAME_ACCESS
        // creates a second handle to that exact kernel object in this process.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.process.as_raw_handle().cast(),
                GetCurrentProcess(),
                &raw mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(WindowsHostJobError::new(
                "keeper peer process pin duplication",
            ));
        }
        // SAFETY: successful DuplicateHandle returned one fresh owning handle.
        let process = unsafe { OwnedHandle::from_raw_handle(duplicate.cast()) };
        Ok(Self {
            process,
            process_id: self.process_id,
            session_id: self.session_id,
            token_facts: self.token_facts.clone(),
            creation_time: self.creation_time,
            image_path: self.image_path.clone(),
        })
    }

    /// PID reported by the connected pipe and pinned by the process handle.
    #[must_use]
    pub const fn process_id(&self) -> u32 {
        self.process_id
    }

    /// Session reported by the connected pipe and re-read from the process.
    #[must_use]
    pub const fn session_id(&self) -> u32 {
        self.session_id
    }

    /// User SID, session and integrity facts queried from this exact process token.
    #[must_use]
    pub const fn token_facts(&self) -> &keld_ipc::WindowsPeerTokenFacts {
        &self.token_facts
    }

    /// Process creation time in Windows FILETIME units.
    #[must_use]
    pub const fn creation_time(&self) -> u64 {
        self.creation_time
    }

    /// Full image path reported by Windows for the retained process object.
    #[must_use]
    pub fn image_path(&self) -> &std::path::Path {
        &self.image_path
    }

    /// Reports whether this exact process object has exited.
    ///
    /// # Errors
    ///
    /// Returns an error if Windows cannot query the retained process handle.
    pub fn has_exited(&self) -> Result<bool, WindowsHostJobError> {
        // SAFETY: this owner retains the exact process object during the query.
        match unsafe { WaitForSingleObject(self.process.as_raw_handle().cast(), 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(WindowsHostJobError::new("keeper peer process state query")),
        }
    }

    /// Waits on this retained exact process object for a bounded duration.
    ///
    /// # Errors
    ///
    /// Returns an error if Windows cannot wait on the retained process handle.
    pub fn wait_until_exited(&self, timeout: Duration) -> Result<bool, WindowsHostJobError> {
        let millis = finite_wait_millis(timeout);
        // SAFETY: `self.process` is a retained process handle with synchronize
        // rights; the timeout is finite and never uses INFINITE.
        match unsafe { WaitForSingleObject(self.process.as_raw_handle().cast(), millis) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(WindowsHostJobError::new("keeper peer process wait")),
        }
    }
}

impl keld_ipc::WindowsLifecyclePeerPin for WindowsProcessPeer {
    fn process_id(&self) -> u32 {
        self.process_id
    }

    fn session_id(&self) -> u32 {
        self.session_id
    }

    fn has_exited(&self) -> io::Result<bool> {
        WindowsProcessPeer::has_exited(self).map_err(|error| io::Error::other(error.to_string()))
    }
}
