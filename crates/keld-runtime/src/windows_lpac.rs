//! Zero-capability Less Privileged `AppContainer` process admission for Windows,
//! and the one suspended-child type that every Keld suspended launch shares.
//!
//! The profile SID, ACL grants, creation attributes, admitted environment and
//! inherited handles are constructed explicitly. A child starts suspended so
//! its token and raw handle table can be audited before untrusted code runs.
//! The `PerUserDirect` candidate's same-token launch uses the same creation call
//! and the same suspended-child type, without the LPAC attributes and without
//! any inherited handle (KEL-53 §5).

#![allow(unsafe_code)] // isolated Win32 security/process ABI; every call has a local proof
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString, c_void};
use std::io;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::{AsRawHandle as _, BorrowedHandle, FromRawHandle as _, OwnedHandle};
use std::path::Path;

use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, LocalFree, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE,
    SE_FILE_OBJECT, SET_ACCESS, SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID,
    TRUSTEE_IS_USER, TRUSTEE_W,
};
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, FreeSid, GetTokenInformation, PSID, SECURITY_CAPABILITIES,
    SUB_CONTAINERS_AND_OBJECTS_INHERIT, TOKEN_GROUPS, TOKEN_QUERY, TokenCapabilities,
    TokenIsAppContainer,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_TRAVERSE,
};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess, GetExitCodeProcess,
    InitializeProcThreadAttributeList, OpenProcessToken,
    PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, PROCESS_INFORMATION, ResumeThread,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};
use windows_sys::Win32::System::WindowsProgramming::PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT;

const ERROR_SUCCESS: u32 = 0;
const STILL_ACTIVE: u32 = 259;

/// Access granted to the LPAC package SID for one reviewed filesystem path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsLpacPathAccess {
    /// Traverse this directory itself without listing or inherited access.
    Traverse,
    /// Read and execute one runtime or fixture artifact.
    ReadExecute,
    /// Read, write, and traverse a role-private directory and its descendants.
    RolePrivate,
}

/// Kernel token facts observed from a created LPAC process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsLpacTokenObservation {
    /// The token has `AppContainer` identity.
    pub is_app_container: bool,
    /// The required All Application Packages opt-out was supplied to the
    /// process-creation attribute list. Its access effect is a separate
    /// hostile oracle because some Windows builds reject the token info class.
    pub all_application_packages_opt_out_configured: bool,
    /// Number of capability SIDs in the actual child token.
    pub capability_count: u32,
}

/// Typed failure while constructing or using a suspended launch: the Windows
/// strict boundary (`KELD-RUNTIME-015`) or the `PerUserDirect` candidate's
/// same-token launch (`KELD-RUNTIME-020`).
#[derive(Debug)]
pub struct WindowsLpacError {
    phase: &'static str,
    detail: String,
    launch: SuspendedLaunch,
}

/// Which suspended launch a child or an error belongs to. It selects the error's
/// code and fix guidance, and whether the LPAC creation attributes were supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SuspendedLaunch {
    /// The zero-capability LPAC launch.
    Lpac,
    /// The `PerUserDirect` candidate's launch under the caller's own token.
    SameToken,
}

impl WindowsLpacError {
    fn last_os(phase: &'static str) -> Self {
        Self {
            phase,
            detail: io::Error::last_os_error().to_string(),
            launch: SuspendedLaunch::Lpac,
        }
    }

    fn status(phase: &'static str, status: i32) -> Self {
        Self {
            phase,
            detail: format!("HRESULT/NT status 0x{:08x}", status.cast_unsigned()),
            launch: SuspendedLaunch::Lpac,
        }
    }

    fn win32(phase: &'static str, status: u32) -> Self {
        Self {
            phase,
            detail: io::Error::from_raw_os_error(status.cast_signed()).to_string(),
            launch: SuspendedLaunch::Lpac,
        }
    }

    fn contract(phase: &'static str, detail: impl Into<String>) -> Self {
        Self {
            phase,
            detail: detail.into(),
            launch: SuspendedLaunch::Lpac,
        }
    }

    /// The same failure, attributed to `launch`.
    const fn in_launch(mut self, launch: SuspendedLaunch) -> Self {
        self.launch = launch;
        self
    }
}

impl std::fmt::Display for WindowsLpacError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.launch {
            SuspendedLaunch::Lpac => write!(
                f,
                "KELD-RUNTIME-015: Windows LPAC admission failed during {}: {}. \
                 Do not start an unconfined replacement; repair the profile, ACL, or handle list.",
                self.phase, self.detail
            ),
            SuspendedLaunch::SameToken => write!(
                f,
                "KELD-RUNTIME-020: the same-token suspended candidate launch failed during {}: {}. \
                 Start no candidate in its place: end the attempt Job and roll the attempt back; \
                 never resume a child whose creation, Job membership or launch record was not proved.",
                self.phase, self.detail
            ),
        }
    }
}

impl std::error::Error for WindowsLpacError {}

/// One zero-capability `AppContainer` profile and its owning SID allocation.
#[derive(Debug)]
pub struct WindowsLpacProfile {
    name: Vec<u16>,
    sid: PSID,
}

impl WindowsLpacProfile {
    /// Creates a new, uniquely named profile with no capability SIDs.
    ///
    /// # Errors
    ///
    /// Fails rather than opening an existing profile, because an existing
    /// identity would make ACL/profile-generation provenance ambiguous.
    pub fn create(name: &OsStr) -> Result<Self, WindowsLpacError> {
        let name = wide_nul(name, "AppContainer profile name")?;
        let mut sid = std::ptr::null_mut();
        // SAFETY: all three strings are live NUL-terminated UTF-16 buffers;
        // capability pointer/count are both empty; `sid` is writable storage.
        let status = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                name.as_ptr(),
                name.as_ptr(),
                std::ptr::null(),
                0,
                &raw mut sid,
            )
        };
        if status < 0 {
            return Err(WindowsLpacError::status(
                "CreateAppContainerProfile",
                status,
            ));
        }
        if sid.is_null() {
            return Err(WindowsLpacError::contract(
                "CreateAppContainerProfile",
                "success returned a null package SID",
            ));
        }
        Ok(Self { name, sid })
    }

    /// Returns the canonical package SID string for evidence identity.
    ///
    /// # Errors
    ///
    /// Fails if Windows cannot serialize the profile SID or returns malformed
    /// non-terminated output.
    pub fn sid_string(&self) -> Result<String, WindowsLpacError> {
        let mut raw = std::ptr::null_mut();
        // SAFETY: `sid` is live and valid; `raw` receives one LocalAlloc string.
        if unsafe { ConvertSidToStringSidW(self.sid, &raw mut raw) } == 0 {
            return Err(WindowsLpacError::last_os("AppContainer SID serialization"));
        }
        if raw.is_null() {
            return Err(WindowsLpacError::contract(
                "AppContainer SID serialization",
                "success returned a null string",
            ));
        }
        let allocation = LocalAllocation(raw.cast());
        let mut length = 0_usize;
        while length < 256 {
            // SAFETY: ConvertSidToStringSidW guarantees NUL-terminated output;
            // the conservative SID-string bound prevents an unbounded scan.
            if unsafe { *raw.add(length) } == 0 {
                // SAFETY: `raw` is live UTF-16 storage of `length` units.
                let units = unsafe { std::slice::from_raw_parts(raw, length) };
                let value = String::from_utf16(units).map_err(|error| {
                    WindowsLpacError::contract("AppContainer SID serialization", error.to_string())
                })?;
                drop(allocation);
                return Ok(value);
            }
            length += 1;
        }
        Err(WindowsLpacError::contract(
            "AppContainer SID serialization",
            "SID string exceeded 255 UTF-16 units without a terminator",
        ))
    }

    /// Adds the package SID to an existing filesystem DACL without replacing
    /// its other access entries.
    ///
    /// # Errors
    ///
    /// Fails if the path does not exist or its DACL cannot be read, merged,
    /// and written exactly.
    pub fn grant_path(
        &self,
        path: &Path,
        access: WindowsLpacPathAccess,
    ) -> Result<(), WindowsLpacError> {
        let path_wide = wide_nul(path.as_os_str(), "ACL path")?;
        let metadata = std::fs::metadata(path)
            .map_err(|error| WindowsLpacError::contract("ACL path metadata", error.to_string()))?;

        let mut old_acl = std::ptr::null_mut();
        let mut security_descriptor = std::ptr::null_mut();
        // SAFETY: `path_wide` is NUL terminated; the requested output slots
        // are live. The returned descriptor owns the borrowed old ACL.
        let status = unsafe {
            GetNamedSecurityInfoW(
                path_wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut old_acl,
                std::ptr::null_mut(),
                &raw mut security_descriptor,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(WindowsLpacError::win32("filesystem DACL read", status));
        }
        let descriptor = LocalAllocation(security_descriptor);

        let permissions = match access {
            WindowsLpacPathAccess::Traverse => FILE_TRAVERSE,
            WindowsLpacPathAccess::ReadExecute => FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
            WindowsLpacPathAccess::RolePrivate => {
                FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE
            }
        };
        let inheritance = if metadata.is_dir() && access != WindowsLpacPathAccess::Traverse {
            SUB_CONTAINERS_AND_OBJECTS_INHERIT
        } else {
            0
        };
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: permissions,
            grfAccessMode: SET_ACCESS,
            grfInheritance: inheritance,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_USER,
                ptstrName: self.sid.cast(),
            },
        };
        let mut new_acl = std::ptr::null_mut();
        // SAFETY: `entry` and old ACL remain live for the synchronous merge;
        // `new_acl` receives one LocalAlloc allocation on success.
        let status = unsafe { SetEntriesInAclW(1, &raw const entry, old_acl, &raw mut new_acl) };
        if status != ERROR_SUCCESS {
            return Err(WindowsLpacError::win32("filesystem DACL merge", status));
        }
        let new_acl = LocalAllocation(new_acl.cast());
        // SAFETY: the path is live and `new_acl` contains the merged DACL;
        // SetNamedSecurityInfoW copies it before returning.
        let status = unsafe {
            SetNamedSecurityInfoW(
                path_wide.as_ptr().cast_mut(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                new_acl.0.cast(),
                std::ptr::null(),
            )
        };
        drop(descriptor);
        if status != ERROR_SUCCESS {
            return Err(WindowsLpacError::win32("filesystem DACL write", status));
        }
        Ok(())
    }

    /// Creates a suspended LPAC child with an explicit environment and exact
    /// inherited-handle list.
    ///
    /// `standard_handles`, when present, are added to the allowlist and wired
    /// as stdin/stdout/stderr. `extra_handles` is reserved for authenticated
    /// app-link transport. No other handle is admitted.
    ///
    /// # Errors
    ///
    /// Fails closed on malformed input, attribute construction, inheritance
    /// preparation, or process creation. The child never runs on success until
    /// [`WindowsSuspendedChild::resume`] is called.
    pub fn spawn_suspended(
        &self,
        program: &Path,
        args: &[OsString],
        environment: &[(OsString, OsString)],
        current_dir: Option<&Path>,
        standard_handles: Option<WindowsLpacStdio<'_>>,
        extra_handles: &[BorrowedHandle<'_>],
    ) -> Result<WindowsSuspendedChild, WindowsLpacError> {
        let application = wide_nul(program.as_os_str(), "application path")?;
        let mut command_line = encode_command_line(program.as_os_str(), args)?;
        let environment = encode_environment(environment)?;
        let current_dir = current_dir
            .map(|path| wide_nul(path.as_os_str(), "current directory"))
            .transpose()?;

        let mut raw_handles = Vec::with_capacity(extra_handles.len() + 3);
        if let Some(stdio) = standard_handles {
            raw_handles.extend([
                stdio.stdin.as_raw_handle().cast(),
                stdio.stdout.as_raw_handle().cast(),
                stdio.stderr.as_raw_handle().cast(),
            ]);
        }
        raw_handles.extend(
            extra_handles
                .iter()
                .map(|handle| handle.as_raw_handle().cast()),
        );
        deduplicate_handles(&mut raw_handles)?;
        let inherited = InheritedHandleCopies::duplicate(&raw_handles)?;
        let inherited_handles = inherited.raw_handles();

        let attribute_count = if inherited_handles.is_empty() { 2 } else { 3 };
        let mut attributes = AttributeList::new(attribute_count)?;
        let capabilities = SECURITY_CAPABILITIES {
            AppContainerSid: self.sid,
            Capabilities: std::ptr::null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        };
        attributes.update(
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
            (&raw const capabilities).cast(),
            std::mem::size_of_val(&capabilities),
            "zero-capability SECURITY_CAPABILITIES",
        )?;
        let all_packages_policy = PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT;
        attributes.update(
            PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY as usize,
            (&raw const all_packages_policy).cast(),
            std::mem::size_of_val(&all_packages_policy),
            "All Application Packages opt-out",
        )?;
        if !inherited_handles.is_empty() {
            attributes.update(
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                inherited_handles.as_ptr().cast(),
                std::mem::size_of_val(inherited_handles.as_slice()),
                "explicit inherited-handle list",
            )?;
        }

        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb =
            u32::try_from(std::mem::size_of::<STARTUPINFOEXW>()).map_err(|_| {
                WindowsLpacError::contract("STARTUPINFOEXW size", "structure exceeds u32")
            })?;
        startup.lpAttributeList = attributes.pointer;
        if let Some(stdio) = standard_handles {
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.StartupInfo.hStdInput =
                inherited.copy_of(stdio.stdin.as_raw_handle().cast())?;
            startup.StartupInfo.hStdOutput =
                inherited.copy_of(stdio.stdout.as_raw_handle().cast())?;
            startup.StartupInfo.hStdError =
                inherited.copy_of(stdio.stderr.as_raw_handle().cast())?;
        }

        create_suspended_process(
            &application,
            &mut command_line,
            &environment,
            current_dir.as_deref(),
            SuspendedStartup::Lpac {
                startup: &startup,
                inherit_handles: !inherited_handles.is_empty(),
            },
        )
    }
}

impl Drop for WindowsLpacProfile {
    fn drop(&mut self) {
        // SAFETY: the name remains NUL terminated and identifies only this
        // profile. Deletion revokes the persistent profile registration; live
        // process tokens remain kernel-owned.
        let _ = unsafe { DeleteAppContainerProfile(self.name.as_ptr()) };
        // SAFETY: `sid` is the one allocation returned by profile creation and
        // is released exactly once here.
        let _ = unsafe { FreeSid(self.sid) };
    }
}

/// The startup information of one suspended creation.
#[derive(Clone, Copy)]
enum SuspendedStartup<'a> {
    /// The LPAC launch: its attribute list, and handle inheritance only together
    /// with its explicit handle-list attribute.
    Lpac {
        startup: &'a STARTUPINFOEXW,
        inherit_handles: bool,
    },
    /// The same-token candidate launch: no attribute list and no inheritance.
    SameToken(&'a STARTUPINFOW),
}

/// The one `CreateProcessW` call of every Keld suspended launch.
fn create_suspended_process(
    application: &[u16],
    command_line: &mut [u16],
    environment: &[u16],
    current_dir: Option<&[u16]>,
    startup: SuspendedStartup<'_>,
) -> Result<WindowsSuspendedChild, WindowsLpacError> {
    let (launch, startup_info, inherit_handles, extended, phase): (
        _,
        *const STARTUPINFOW,
        _,
        _,
        _,
    ) = match startup {
        SuspendedStartup::Lpac {
            startup,
            inherit_handles,
        } => (
            SuspendedLaunch::Lpac,
            (&raw const startup.StartupInfo).cast(),
            inherit_handles,
            EXTENDED_STARTUPINFO_PRESENT,
            "CreateProcessW LPAC launch",
        ),
        SuspendedStartup::SameToken(startup) => (
            SuspendedLaunch::SameToken,
            std::ptr::from_ref(startup),
            false,
            0,
            "CreateProcessW same-token launch",
        ),
    };
    let mut process = PROCESS_INFORMATION::default();
    let creation_flags = CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | extended;
    // SAFETY: every pointer refers to live storage through this synchronous
    // call; command line is mutable as required; `startup_info` points at a
    // borrowed STARTUPINFOW, which for the LPAC launch heads a STARTUPINFOEXW
    // whose attribute list retains its backing values, and
    // EXTENDED_STARTUPINFO_PRESENT is set only then; process/thread outputs are
    // writable. TRUE inheritance is used only by the LPAC launch, together with
    // its explicit handle-list attribute; the same-token launch passes FALSE and
    // no token, so the child runs under a copy of this process's primary token.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            i32::from(inherit_handles),
            creation_flags,
            environment.as_ptr().cast(),
            current_dir.map_or(std::ptr::null(), <[u16]>::as_ptr),
            startup_info,
            &raw mut process,
        )
    };
    if created == 0 {
        return Err(WindowsLpacError::last_os(phase).in_launch(launch));
    }
    if process.hProcess.is_null() || process.hThread.is_null() {
        // SAFETY: any non-null output belongs to this failed construction.
        unsafe {
            if !process.hProcess.is_null() {
                let _ = CloseHandle(process.hProcess);
            }
            if !process.hThread.is_null() {
                let _ = CloseHandle(process.hThread);
            }
        }
        return Err(WindowsLpacError::contract(
            phase,
            "success returned a null process or thread handle",
        )
        .in_launch(launch));
    }

    Ok(WindowsSuspendedChild {
        // SAFETY: both handles are fresh, non-null owning handles returned by
        // CreateProcessW and each is converted exactly once.
        process: unsafe { OwnedHandle::from_raw_handle(process.hProcess.cast()) },
        thread: Some(unsafe { OwnedHandle::from_raw_handle(process.hThread.cast()) }),
        pid: process.dwProcessId,
        terminated: false,
        launch,
    })
}

/// The exact standard handles admitted to one LPAC child.
#[derive(Debug, Clone, Copy)]
pub struct WindowsLpacStdio<'a> {
    /// Child standard input.
    pub stdin: BorrowedHandle<'a>,
    /// Child standard output.
    pub stdout: BorrowedHandle<'a>,
    /// Child standard error.
    pub stderr: BorrowedHandle<'a>,
}

/// Owning handle pair for one process that a Keld launch path created suspended.
///
/// This is the single suspended-child type (KEL-53 §5 reuse decisions). The LPAC
/// launch and the `PerUserDirect` candidate's same-token launch
/// ([`Self::spawn_same_token`]) create it; any other suspended launch, such as
/// the attempt owner's token launch, must adopt its created process into this
/// same type, so the resume-once state and its one primary-thread resume are
/// never duplicated. The process handle stays retained after resume; it is the
/// launch handle that [`crate::windows_job::WindowsLaunchedProcess`] binds a
/// connect-back claimant to.
#[derive(Debug)]
pub struct WindowsSuspendedChild {
    process: OwnedHandle,
    thread: Option<OwnedHandle>,
    pid: u32,
    terminated: bool,
    launch: SuspendedLaunch,
}

impl WindowsSuspendedChild {
    /// Creates the `PerUserDirect` candidate suspended, under the caller's own
    /// token (KEL-53 §5, §6 S6b).
    ///
    /// The one suspended creation call runs with `CREATE_SUSPENDED`, without the
    /// LPAC attribute list and with handle inheritance off, so the child inherits
    /// no handle of the caller: no attempt endpoint, no start-gate pipe and no
    /// standard handle. It runs under a copy of the caller's primary token.
    /// `environment` is its complete environment block, as for the LPAC launch,
    /// and `current_dir` its working directory. `program` and `current_dir` must
    /// be absolute: `program` names the image exactly, and Windows neither
    /// searches for it nor resolves it against a current directory.
    ///
    /// The caller assigns the child to its attempt Job with
    /// [`crate::windows_job::WindowsProcessJob::assign_child`] and records it
    /// with [`crate::windows_job::WindowsLaunchedProcess::record`] before the one
    /// [`Self::resume`], so the child is a Job member before its first
    /// instruction. The stdin start gate is not used.
    ///
    /// # Errors
    ///
    /// `KELD-RUNTIME-020` for a relative path, a NUL in any input, a malformed
    /// environment, or a refused creation; no child exists then.
    pub fn spawn_same_token(
        program: &Path,
        args: &[OsString],
        environment: &[(OsString, OsString)],
        current_dir: &Path,
    ) -> Result<Self, WindowsLpacError> {
        let same_token = |error: WindowsLpacError| error.in_launch(SuspendedLaunch::SameToken);
        for (path, phase) in [
            (program, "application path"),
            (current_dir, "current directory"),
        ] {
            if !path.is_absolute() {
                return Err(same_token(WindowsLpacError::contract(
                    phase,
                    "path is not absolute",
                )));
            }
        }
        let application = wide_nul(program.as_os_str(), "application path").map_err(same_token)?;
        let mut command_line =
            encode_command_line(program.as_os_str(), args).map_err(same_token)?;
        let environment = encode_environment(environment).map_err(same_token)?;
        let current_dir =
            wide_nul(current_dir.as_os_str(), "current directory").map_err(same_token)?;
        let startup = STARTUPINFOW {
            cb: u32::try_from(std::mem::size_of::<STARTUPINFOW>()).map_err(|_| {
                same_token(WindowsLpacError::contract(
                    "STARTUPINFOW size",
                    "structure exceeds u32",
                ))
            })?,
            ..STARTUPINFOW::default()
        };
        create_suspended_process(
            &application,
            &mut command_line,
            &environment,
            Some(&current_dir),
            SuspendedStartup::SameToken(&startup),
        )
    }

    /// Returns the process ID that process creation reported for this child.
    ///
    /// The ID is evidence correlation only, not an identity on its own: an ID
    /// names a process only until that process exits. Bind a process by
    /// comparing its process object, as
    /// [`crate::windows_job::WindowsLaunchedProcess::bind_claimant`] does.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.pid
    }

    /// Borrows the process handle for token and cross-process handle-table
    /// observation before resume.
    #[must_use]
    pub fn process_handle(&self) -> BorrowedHandle<'_> {
        // SAFETY: the returned borrow cannot outlive `self.process`.
        unsafe { BorrowedHandle::borrow_raw(self.process.as_raw_handle()) }
    }

    /// Reads the kernel token's `AppContainer`, LPAC, and capability facts.
    ///
    /// # Errors
    ///
    /// Fails if the token cannot be opened or any fact cannot be queried.
    pub fn observe_token(&self) -> Result<WindowsLpacTokenObservation, WindowsLpacError> {
        let mut raw_token = std::ptr::null_mut();
        // SAFETY: process handle is live; output receives one token handle.
        if unsafe {
            OpenProcessToken(
                self.process.as_raw_handle().cast(),
                TOKEN_QUERY,
                &raw mut raw_token,
            )
        } == 0
        {
            return Err(WindowsLpacError::last_os("child token open").in_launch(self.launch));
        }
        if raw_token.is_null() {
            return Err(WindowsLpacError::contract(
                "child token open",
                "success returned a null token handle",
            )
            .in_launch(self.launch));
        }
        // SAFETY: fresh non-null owning token handle converted once.
        let token = unsafe { OwnedHandle::from_raw_handle(raw_token.cast()) };
        let is_app_container = query_token_u32(&token, TokenIsAppContainer, "TokenIsAppContainer")
            .map_err(|error| error.in_launch(self.launch))?;
        let capabilities = query_token_buffer(&token, TokenCapabilities, "TokenCapabilities")
            .map_err(|error| error.in_launch(self.launch))?;
        // SAFETY: query_token_buffer returns aligned storage filled for the
        // requested TOKEN_GROUPS class and keeps it live for this read.
        let capability_count =
            unsafe { (*(capabilities.as_ptr().cast::<TOKEN_GROUPS>())).GroupCount };
        Ok(WindowsLpacTokenObservation {
            is_app_container: is_app_container != 0,
            // Only the LPAC launch supplies the creation attribute.
            all_application_packages_opt_out_configured: self.launch == SuspendedLaunch::Lpac,
            capability_count,
        })
    }

    /// Resumes the initially suspended primary thread exactly once.
    ///
    /// The thread's previous suspend count must be exactly 1: the one suspension
    /// of `CREATE_SUSPENDED`. So a successful resume proves that the primary
    /// thread ran no instruction between creation and this call, and that
    /// anything the caller did first, such as attempt-Job assignment, preceded
    /// the child's first instruction.
    ///
    /// # Errors
    ///
    /// Fails if the child was already resumed, the kernel rejects resume, or the
    /// previous suspend count was not 1. In that last case the resume is spent
    /// and the caller must terminate the child.
    pub fn resume(&mut self) -> Result<(), WindowsLpacError> {
        let thread = self.thread.as_ref().ok_or_else(|| {
            WindowsLpacError::contract("child resume", "primary thread already resumed")
                .in_launch(self.launch)
        })?;
        // SAFETY: the primary-thread handle is live and still owned here.
        let previous = unsafe { ResumeThread(thread.as_raw_handle().cast()) };
        if previous == u32::MAX {
            return Err(WindowsLpacError::last_os("child resume").in_launch(self.launch));
        }
        self.thread = None;
        if previous != 1 {
            return Err(WindowsLpacError::contract(
                "child resume",
                format!(
                    "the primary thread's previous suspend count was {previous}, not 1: it was \
                     not held suspended from creation"
                ),
            )
            .in_launch(self.launch));
        }
        Ok(())
    }

    /// Waits for termination and returns the exact Windows exit code.
    ///
    /// # Errors
    ///
    /// Fails on timeout or process-status query failure.
    pub fn wait(&mut self, timeout_ms: u32) -> Result<u32, WindowsLpacError> {
        // SAFETY: process handle remains live for the wait.
        match unsafe { WaitForSingleObject(self.process.as_raw_handle().cast(), timeout_ms) } {
            WAIT_OBJECT_0 => {}
            WAIT_TIMEOUT => {
                return Err(WindowsLpacError::contract(
                    "child wait",
                    format!("process {} exceeded {timeout_ms} ms", self.pid),
                )
                .in_launch(self.launch));
            }
            _ => return Err(WindowsLpacError::last_os("child wait").in_launch(self.launch)),
        }
        let mut exit_code = STILL_ACTIVE;
        // SAFETY: live process handle and writable exit-code storage.
        if unsafe { GetExitCodeProcess(self.process.as_raw_handle().cast(), &raw mut exit_code) }
            == 0
        {
            return Err(WindowsLpacError::last_os("child exit code").in_launch(self.launch));
        }
        self.terminated = true;
        Ok(exit_code)
    }

    /// Terminates only this test/supervisor-owned child.
    ///
    /// # Errors
    ///
    /// Fails if Windows rejects termination.
    pub fn terminate(&mut self, exit_code: u32) -> Result<(), WindowsLpacError> {
        // SAFETY: live owning process handle; the caller owns this child.
        if unsafe { TerminateProcess(self.process.as_raw_handle().cast(), exit_code) } == 0 {
            return Err(WindowsLpacError::last_os("child terminate").in_launch(self.launch));
        }
        self.terminated = true;
        Ok(())
    }
}

impl Drop for WindowsSuspendedChild {
    fn drop(&mut self) {
        if self.terminated {
            return;
        }
        // SAFETY: this owner never detaches. Termination prevents a suspended
        // or failed-evidence child from escaping when callers unwind.
        let _ = unsafe { TerminateProcess(self.process.as_raw_handle().cast(), 1) };
    }
}

struct AttributeList {
    _storage: Vec<usize>,
    pointer: *mut c_void,
}

impl AttributeList {
    fn new(count: u32) -> Result<Self, WindowsLpacError> {
        let mut bytes = 0_usize;
        // SAFETY: null is the documented sizing call; `bytes` is writable.
        let _ = unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), count, 0, &raw mut bytes)
        };
        if bytes == 0 {
            return Err(WindowsLpacError::last_os("process attribute-list sizing"));
        }
        let words = bytes.div_ceil(std::mem::size_of::<usize>());
        let mut storage = vec![0_usize; words];
        let pointer = storage.as_mut_ptr().cast();
        // SAFETY: storage is aligned and at least the sized byte count; it
        // remains owned by this structure until DeleteProcThreadAttributeList.
        if unsafe { InitializeProcThreadAttributeList(pointer, count, 0, &raw mut bytes) } == 0 {
            return Err(WindowsLpacError::last_os(
                "process attribute-list initialization",
            ));
        }
        Ok(Self {
            _storage: storage,
            pointer,
        })
    }

    fn update(
        &mut self,
        attribute: usize,
        value: *const c_void,
        bytes: usize,
        phase: &'static str,
    ) -> Result<(), WindowsLpacError> {
        // SAFETY: list is initialized and live; each value is retained by the
        // caller through CreateProcessW; optional output pointers are null.
        if unsafe {
            UpdateProcThreadAttribute(
                self.pointer,
                0,
                attribute,
                value,
                bytes,
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        } == 0
        {
            return Err(WindowsLpacError::last_os(phase));
        }
        Ok(())
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: pointer was initialized once and storage remains live.
        unsafe { DeleteProcThreadAttributeList(self.pointer) };
    }
}

struct InheritedHandleCopies {
    originals: Vec<HANDLE>,
    copies: Vec<OwnedHandle>,
}

impl InheritedHandleCopies {
    fn duplicate(handles: &[HANDLE]) -> Result<Self, WindowsLpacError> {
        let mut copies = Vec::with_capacity(handles.len());
        for &handle in handles {
            if handle.is_null() || handle == (-1_isize as HANDLE) {
                return Err(WindowsLpacError::contract(
                    "inherited-handle validation",
                    "null or invalid handle in allowlist",
                ));
            }
            let mut duplicate = std::ptr::null_mut();
            // SAFETY: the source handle and current-process pseudo-handles are
            // live. `duplicate` receives one owning, inheritable copy with the
            // same access. No caller-owned handle flag is changed.
            if unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    handle,
                    GetCurrentProcess(),
                    &raw mut duplicate,
                    0,
                    1,
                    DUPLICATE_SAME_ACCESS,
                )
            } == 0
            {
                return Err(WindowsLpacError::last_os(
                    "inherited-handle private duplication",
                ));
            }
            if duplicate.is_null() {
                return Err(WindowsLpacError::contract(
                    "inherited-handle private duplication",
                    "success returned a null duplicate",
                ));
            }
            // SAFETY: duplicate is one fresh non-null owning handle.
            copies.push(unsafe { OwnedHandle::from_raw_handle(duplicate.cast()) });
        }
        Ok(Self {
            originals: handles.to_vec(),
            copies,
        })
    }

    fn raw_handles(&self) -> Vec<HANDLE> {
        self.copies
            .iter()
            .map(|handle| handle.as_raw_handle().cast())
            .collect()
    }

    fn copy_of(&self, original: HANDLE) -> Result<HANDLE, WindowsLpacError> {
        let index = self
            .originals
            .iter()
            .position(|candidate| *candidate == original)
            .ok_or_else(|| {
                WindowsLpacError::contract(
                    "standard-handle private duplication",
                    "standard handle missing from admitted list",
                )
            })?;
        Ok(self.copies[index].as_raw_handle().cast())
    }
}

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        if self.0.is_null() {
            return;
        }
        // SAFETY: pointer is one LocalAlloc-family allocation returned by the
        // security API and is released once here.
        let _ = unsafe { LocalFree(self.0) };
    }
}

fn query_token_u32(
    token: &OwnedHandle,
    class: i32,
    phase: &'static str,
) -> Result<u32, WindowsLpacError> {
    let mut value = 0_u32;
    let mut returned = 0_u32;
    // SAFETY: token is live; value and returned length are writable.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            class,
            (&raw mut value).cast(),
            u32::try_from(std::mem::size_of_val(&value)).unwrap_or(u32::MAX),
            &raw mut returned,
        )
    } == 0
    {
        return Err(WindowsLpacError::last_os(phase));
    }
    Ok(value)
}

fn query_token_buffer(
    token: &OwnedHandle,
    class: i32,
    phase: &'static str,
) -> Result<Vec<usize>, WindowsLpacError> {
    let mut bytes = 0_u32;
    // SAFETY: null sizing call with writable returned length.
    let _ = unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            class,
            std::ptr::null_mut(),
            0,
            &raw mut bytes,
        )
    };
    if bytes == 0 {
        return Err(WindowsLpacError::last_os(phase));
    }
    let byte_count = usize::try_from(bytes)
        .map_err(|_| WindowsLpacError::contract(phase, "token information length exceeds usize"))?;
    let mut storage = vec![0_usize; byte_count.div_ceil(std::mem::size_of::<usize>())];
    // SAFETY: aligned storage has at least `bytes` writable bytes.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            class,
            storage.as_mut_ptr().cast(),
            bytes,
            &raw mut bytes,
        )
    } == 0
    {
        return Err(WindowsLpacError::last_os(phase));
    }
    Ok(storage)
}

fn deduplicate_handles(handles: &mut Vec<HANDLE>) -> Result<(), WindowsLpacError> {
    let mut seen = BTreeSet::new();
    handles.retain(|handle| seen.insert(*handle as usize));
    if handles.len() > u32::MAX as usize {
        return Err(WindowsLpacError::contract(
            "inherited-handle list",
            "handle count exceeds u32",
        ));
    }
    Ok(())
}

fn encode_command_line(program: &OsStr, args: &[OsString]) -> Result<Vec<u16>, WindowsLpacError> {
    let mut encoded = Vec::new();
    append_quoted_arg(&mut encoded, program)?;
    for arg in args {
        encoded.push(u16::from(b' '));
        append_quoted_arg(&mut encoded, arg)?;
    }
    encoded.push(0);
    Ok(encoded)
}

fn append_quoted_arg(output: &mut Vec<u16>, arg: &OsStr) -> Result<(), WindowsLpacError> {
    let units: Vec<u16> = arg.encode_wide().collect();
    if units.contains(&0) {
        return Err(WindowsLpacError::contract(
            "command-line encoding",
            "argument contains NUL",
        ));
    }
    let needs_quotes =
        units.is_empty() || units.iter().any(|unit| matches!(*unit, 0x20 | 0x09 | 0x22));
    if !needs_quotes {
        output.extend(units);
        return Ok(());
    }

    output.push(u16::from(b'"'));
    let mut slashes = 0_usize;
    for unit in units {
        if unit == u16::from(b'\\') {
            slashes += 1;
            continue;
        }
        if unit == u16::from(b'"') {
            output.extend(std::iter::repeat_n(u16::from(b'\\'), slashes * 2 + 1));
        } else {
            output.extend(std::iter::repeat_n(u16::from(b'\\'), slashes));
        }
        slashes = 0;
        output.push(unit);
    }
    output.extend(std::iter::repeat_n(u16::from(b'\\'), slashes * 2));
    output.push(u16::from(b'"'));
    Ok(())
}

fn encode_environment(environment: &[(OsString, OsString)]) -> Result<Vec<u16>, WindowsLpacError> {
    let mut ordered = Vec::with_capacity(environment.len());
    let mut keys = BTreeSet::new();
    for (key, value) in environment {
        let key_units: Vec<u16> = key.encode_wide().collect();
        let value_units: Vec<u16> = value.encode_wide().collect();
        if key_units.is_empty()
            || key_units.contains(&0)
            || key_units.contains(&u16::from(b'='))
            || value_units.contains(&0)
        {
            return Err(WindowsLpacError::contract(
                "environment encoding",
                "key is empty/contains '=' or key/value contains NUL",
            ));
        }
        let folded = key.to_string_lossy().to_uppercase();
        if !keys.insert(folded.clone()) {
            return Err(WindowsLpacError::contract(
                "environment encoding",
                format!("duplicate case-insensitive key {folded}"),
            ));
        }
        ordered.push((folded, key_units, value_units));
    }
    ordered.sort_by(|left, right| left.0.cmp(&right.0));

    let mut encoded = Vec::new();
    for (_, key, value) in ordered {
        encoded.extend(key);
        encoded.push(u16::from(b'='));
        encoded.extend(value);
        encoded.push(0);
    }
    encoded.push(0);
    if environment.is_empty() {
        encoded.push(0);
    }
    Ok(encoded)
}

fn wide_nul(value: &OsStr, phase: &'static str) -> Result<Vec<u16>, WindowsLpacError> {
    nul_terminated_wide(value)
        .ok_or_else(|| WindowsLpacError::contract(phase, "value contains NUL"))
}

/// Encodes `value` as NUL-terminated UTF-16 for a Win32 string parameter, or
/// returns `None` when it contains a NUL, which Windows would read as an
/// earlier end of the string.
pub(crate) fn nul_terminated_wide(value: &OsStr) -> Option<Vec<u16>> {
    let mut encoded: Vec<u16> = value.encode_wide().collect();
    if encoded.contains(&0) {
        return None;
    }
    encoded.push(0);
    Some(encoded)
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::windows::io::AsRawHandle as _;

    use windows_sys::Win32::Foundation::{GetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT};

    use super::{InheritedHandleCopies, WindowsSuspendedChild};

    #[test]
    fn resume_refuses_a_primary_thread_not_held_suspended_exactly_once() {
        // Seam-injected: a second suspension stands in for any count but the one of
        // CREATE_SUSPENDED. A thread that already ran shows count 0 and refuses the
        // same way; that case cannot be built without resuming the thread here.
        let system32 =
            std::path::PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set"))
                .join("System32");
        let mut child = WindowsSuspendedChild::spawn_same_token(
            &system32.join("cmd.exe"),
            &[std::ffi::OsString::from("/d")],
            &[],
            &system32,
        )
        .expect("create a suspended child");
        let thread = child.thread.as_ref().expect("the primary thread is held");
        // SAFETY: the primary-thread handle is live and owned by `child`.
        let previous = unsafe {
            windows_sys::Win32::System::Threading::SuspendThread(thread.as_raw_handle().cast())
        };
        assert_eq!(previous, 1, "CREATE_SUSPENDED leaves one suspension");
        let refusal = child
            .resume()
            .expect_err("a count other than 1 refuses the resume");
        assert!(
            refusal
                .to_string()
                .contains("previous suspend count was 2, not 1"),
            "{refusal}"
        );
        assert!(child.resume().is_err(), "the refused resume is spent");
        child.terminate(1).expect("terminate the refused child");
    }

    #[test]
    fn admitted_handle_uses_private_inheritable_copy_without_mutating_caller() {
        let original = File::open("NUL").expect("open test handle");
        let original_raw = original.as_raw_handle().cast();
        assert!(!is_inheritable(original_raw));

        let copies = InheritedHandleCopies::duplicate(&[original_raw])
            .expect("duplicate admitted handle privately");
        assert!(!is_inheritable(original_raw));
        assert!(is_inheritable(copies.raw_handles()[0]));
        assert_eq!(copies.copies.len(), 1);
    }

    fn is_inheritable(handle: HANDLE) -> bool {
        let mut flags = 0_u32;
        // SAFETY: the test supplies a live borrowed handle and writable flags.
        assert_ne!(
            unsafe { GetHandleInformation(handle, &raw mut flags) },
            0,
            "query handle flags: {}",
            std::io::Error::last_os_error()
        );
        flags & HANDLE_FLAG_INHERIT != 0
    }

    #[test]
    fn nul_terminated_wide_appends_one_terminator_and_refuses_an_interior_nul() {
        use std::ffi::OsStr;
        assert_eq!(
            super::nul_terminated_wide(OsStr::new("ab")),
            Some(vec![u16::from(b'a'), u16::from(b'b'), 0])
        );
        assert_eq!(super::nul_terminated_wide(OsStr::new("")), Some(vec![0]));
        assert_eq!(super::nul_terminated_wide(OsStr::new("a\0b")), None);
    }
}
