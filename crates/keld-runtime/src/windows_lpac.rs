//! Zero-capability Less Privileged `AppContainer` process admission for Windows,
//! and the one suspended-child type that every Keld suspended launch shares.
//!
//! The profile SID, ACL grants, creation attributes, admitted environment and
//! inherited handles are constructed explicitly. A child starts suspended so
//! its token and raw handle table can be audited before untrusted code runs.
//! The `PerUserDirect` candidate's same-token launch uses the same creation call
//! and the same suspended-child type, without the LPAC attributes and without
//! any inherited handle (KEL-53 §5). Every launch names the child's standard
//! handles explicitly, the admitted copies or null, because process creation
//! otherwise hands the child this process's own standard handles.

#![allow(unsafe_code)] // isolated Win32 security/process ABI; every call has a local proof
#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString, c_void};
use std::fs::OpenOptions;
use std::io;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::os::windows::io::{AsRawHandle as _, BorrowedHandle, FromRawHandle as _, OwnedHandle};
use std::path::Path;

use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, LocalFree, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, EXPLICIT_ACCESS_W, GetSecurityInfo, NO_MULTIPLE_TRUSTEE,
    SE_FILE_OBJECT, SET_ACCESS, SetEntriesInAclW, SetSecurityInfo, TRUSTEE_IS_SID, TRUSTEE_IS_USER,
    TRUSTEE_W,
};
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, FreeSid, GetTokenInformation, PSID, SECURITY_CAPABILITIES,
    SID_AND_ATTRIBUTES, SUB_CONTAINERS_AND_OBJECTS_INHERIT, TOKEN_GROUPS, TOKEN_QUERY,
    TokenCapabilities, TokenIsAppContainer,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, READ_CONTROL, WRITE_DAC,
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

mod launch_encoding;
use launch_encoding::{encode_command_line, encode_environment};

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
    /// The DACL is read, merged and written through one retained handle to the
    /// object the path named when it was opened, so renaming or replacing the
    /// path afterwards cannot redirect the write.
    ///
    /// Only the final path component (the leaf) is checked for a reparse point.
    /// Reparse points in ancestor components (junctions, directory symbolic
    /// links, mount points) are followed by the open like any Win32 path, so
    /// the grant lands on whatever object the resolved path named at open
    /// time. The caller must pass a path whose ancestors a less-trusted
    /// principal cannot replace.
    ///
    /// # Errors
    ///
    /// Fails if the path does not exist, the opened leaf is a reparse point,
    /// or that exact object's DACL cannot be read, merged, and written.
    pub fn grant_path(
        &self,
        path: &Path,
        access: WindowsLpacPathAccess,
    ) -> Result<(), WindowsLpacError> {
        self.prepare_grant(path, access)?.publish()
    }

    /// Opens the grant target, refuses a reparse-point leaf, and reads and
    /// merges that exact object's DACL. [`PreparedGrant::publish`] writes the
    /// merged DACL through the same retained handle.
    fn prepare_grant(
        &self,
        path: &Path,
        access: WindowsLpacPathAccess,
    ) -> Result<PreparedGrant, WindowsLpacError> {
        // Preserve the API's explicit embedded-NUL refusal before opening.
        let _ = wide_nul(path.as_os_str(), "ACL path")?;
        // This handle owns both the identity and the authority to read/write
        // its DACL. The share mode does not stop a rename: this handle asks
        // for no data or DELETE access, so Windows does not check sharing
        // against it; on Windows 11 a rename succeeds while it is open.
        // Identity rests on this retained HANDLE alone.
        // OPEN_REPARSE_POINT permits fail-closed inspection of the final leaf.
        let original = OpenOptions::new()
            .access_mode(READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|error| WindowsLpacError::contract("ACL object open", error.to_string()))?;
        let metadata = original.metadata().map_err(|error| {
            WindowsLpacError::contract("ACL object metadata", error.to_string())
        })?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(WindowsLpacError::contract(
                "ACL object admission",
                "refuse a reparse-point grant target",
            ));
        }

        let mut old_acl = std::ptr::null_mut();
        let mut security_descriptor = std::ptr::null_mut();
        // SAFETY: `original` is a live READ_CONTROL handle to the admitted
        // object. The output slots are writable, and the returned LocalAlloc
        // descriptor owns the borrowed old ACL.
        let status = unsafe {
            GetSecurityInfo(
                original.as_raw_handle().cast(),
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
        // The merged ACL is its own allocation; the read descriptor that owns
        // the old ACL is no longer referenced.
        drop(descriptor);
        Ok(PreparedGrant { original, new_acl })
    }

    /// Creates a suspended LPAC child with an explicit environment and exact
    /// inherited-handle list.
    ///
    /// `standard_handles`, when present, are added to the allowlist and wired
    /// as stdin/stdout/stderr. Without them the child's three standard handles
    /// are null, so its `GetStdHandle` reports a process without associated
    /// standard handles; it never receives this process's own standard handles.
    /// `extra_handles` is reserved for authenticated app-link transport. No
    /// other handle is admitted.
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
        // The child's standard handles are exactly these three fields: the
        // admitted private copies, or the null default. The flag is set for
        // every launch; `create_suspended_process` owns that rule.
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        if let Some(stdio) = standard_handles {
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
///
/// Every launch sets `STARTF_USESTDHANDLES`, so the child's standard handles
/// are exactly the startup structure's `hStdInput`, `hStdOutput` and
/// `hStdError`: the LPAC launch's admitted private copies, or null, which the
/// child's `GetStdHandle` reports as a process without associated standard
/// handles. Without the flag, process creation gives the child this process's
/// own standard handles, and when they are not console handles, such as the
/// pipes of `keld dev` or a test runner, it duplicates them into the child even
/// with handle inheritance off, outside any handle-list attribute and whether
/// or not the handle is inheritable. A startup structure without the flag is
/// refused before any process exists.
fn create_suspended_process(
    application: &[u16],
    command_line: &mut [u16],
    environment: &[u16],
    current_dir: Option<&[u16]>,
    startup: SuspendedStartup<'_>,
) -> Result<WindowsSuspendedChild, WindowsLpacError> {
    // The pointer is derived from the whole startup object of each arm: the
    // LPAC call reads the STARTUPINFOEXW beyond its leading STARTUPINFOW.
    let (launch, startup_info, flags, inherit_handles, extended, phase): (
        _,
        *const STARTUPINFOW,
        _,
        _,
        _,
        _,
    ) = match startup {
        SuspendedStartup::Lpac {
            startup,
            inherit_handles,
        } => (
            SuspendedLaunch::Lpac,
            std::ptr::from_ref(startup).cast::<STARTUPINFOW>(),
            startup.StartupInfo.dwFlags,
            inherit_handles,
            EXTENDED_STARTUPINFO_PRESENT,
            "CreateProcessW LPAC launch",
        ),
        SuspendedStartup::SameToken(startup) => (
            SuspendedLaunch::SameToken,
            std::ptr::from_ref(startup),
            startup.dwFlags,
            false,
            0,
            "CreateProcessW same-token launch",
        ),
    };
    if flags & STARTF_USESTDHANDLES == 0 {
        return Err(WindowsLpacError::contract(
            phase,
            "the startup structure does not name the child's standard handles: without \
             STARTF_USESTDHANDLES the child would receive this process's own",
        )
        .in_launch(launch));
    }
    let mut process = PROCESS_INFORMATION::default();
    let creation_flags = CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | extended;
    // SAFETY: every pointer refers to live storage through this synchronous
    // call; command line is mutable as required; `startup_info` points at the
    // borrowed startup object of its arm: a STARTUPINFOW for the same-token
    // launch, and for the LPAC launch a STARTUPINFOEXW whose attribute list
    // retains its backing values, where the pointer is derived from the whole
    // STARTUPINFOEXW, so the extended read of `lpAttributeList` that
    // EXTENDED_STARTUPINFO_PRESENT (set only then) requests is within its
    // provenance; its standard-handle fields are null or the LPAC launch's
    // retained private copies, and the flag that makes them the child's is
    // checked above; process/thread outputs are writable. TRUE inheritance is
    // used only by the LPAC launch, together with its explicit handle-list
    // attribute; the same-token launch passes FALSE and no token, so the child
    // runs under a copy of this process's primary token.
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
        detached: false,
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
    /// Set by the one detach, [`Self::detach`]: the child is a released
    /// candidate that this owner keeps running when it drops.
    detached: bool,
    launch: SuspendedLaunch,
}

impl WindowsSuspendedChild {
    /// Creates the `PerUserDirect` candidate suspended, under the caller's own
    /// token (KEL-53 §5, §6 S6b).
    ///
    /// The one suspended creation call runs with `CREATE_SUSPENDED`, without the
    /// LPAC attribute list, with handle inheritance off and with the child's
    /// three standard handles named null, so the child holds no handle of the
    /// caller: no attempt endpoint, no start-gate pipe and no standard handle.
    /// Its `GetStdHandle` reports NULL for all three, as for a process without
    /// associated standard handles; inheritance off alone would not do this,
    /// because process creation duplicates a caller's non-console standard
    /// handles into a child whose startup structure does not name its own. It
    /// runs under a copy of the caller's primary token.
    /// `environment` is its complete environment block, as for the LPAC launch,
    /// and `current_dir` its working directory. `program` and `current_dir` must
    /// be absolute: `program` names the image exactly, and Windows neither
    /// searches for it nor resolves it against a current directory.
    ///
    /// The caller records the child with
    /// [`crate::windows_job::WindowsLaunchedProcess::record`] and assigns it to
    /// its attempt Job with
    /// [`crate::windows_job::WindowsProcessJob::assign_child`]. The record's one
    /// resume requires the membership proof that call returns, so the child is a
    /// Job member before its first instruction. The stdin start gate is not used.
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
            // The child's standard handles are exactly these three, null: the
            // flag is required by `create_suspended_process`, which owns the
            // rule.
            dwFlags: STARTF_USESTDHANDLES,
            hStdInput: std::ptr::null_mut(),
            hStdOutput: std::ptr::null_mut(),
            hStdError: std::ptr::null_mut(),
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

    /// Whether the one resume was already spent.
    pub(crate) const fn is_resumed(&self) -> bool {
        self.thread.is_none()
    }

    /// Keeps the child running and closes this owner's handles to it: the
    /// detach of a released candidate (KEL-53 §4 "Candidate release after
    /// commit", *Launch-record detach*). The one caller is
    /// [`crate::windows_job::WindowsLaunchedProcess::detach`], which admits only
    /// a resumed child bound to a host-death census; that check is the record's,
    /// so it is made once. Closing the process handle ends nothing: the child
    /// continues by identity only, and the kernel keeps its object alive.
    pub(crate) fn detach(mut self) {
        self.detached = true;
    }

    /// A refusal of an operation on this child, coded for the launch that made it.
    pub(crate) fn refusal(&self, phase: &'static str, detail: &str) -> WindowsLpacError {
        WindowsLpacError::contract(phase, detail).in_launch(self.launch)
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
        // The reported byte count and declared group entries must both fit the
        // live token buffer before any capability count can be admitted.
        let capability_count = token_group_count(&token, TokenCapabilities, "TokenCapabilities")
            .map_err(|error| error.in_launch(self.launch))?;
        Ok(WindowsLpacTokenObservation {
            is_app_container: is_app_container != 0,
            // Only the LPAC launch supplies the creation attribute.
            all_application_packages_opt_out_configured: self.launch == SuspendedLaunch::Lpac,
            capability_count,
        })
    }

    /// Resumes the initially suspended primary thread exactly once.
    ///
    /// A same-token child refuses here: it runs only through its launch record,
    /// whose resume requires the child's attempt-Job membership proof
    /// ([`crate::windows_job::WindowsLaunchedProcess::resume`]). The refusal
    /// spends nothing, so that record can still resume it.
    ///
    /// The thread's previous suspend count must be exactly 1, the one suspension
    /// of `CREATE_SUSPENDED`: a count other than 1 shows a resume or suspend of
    /// the thread since creation that was not balanced, and refuses. A balanced
    /// resume-and-suspend pair by another process that holds a thread handle is
    /// not visible here; that is same-user code, which is outside the
    /// `PerUserDirect` boundary (KEL-53 criterion 12).
    ///
    /// # Errors
    ///
    /// Fails for a same-token child, if the child was already resumed, the kernel
    /// rejects resume, or the previous suspend count was not 1. In that last case
    /// the resume is spent and the caller must terminate the child.
    pub fn resume(&mut self) -> Result<(), WindowsLpacError> {
        if self.launch == SuspendedLaunch::SameToken {
            return Err(self.refusal(
                "child resume",
                "a same-token candidate resumes only through its launch record and attempt-Job membership proof",
            ));
        }
        self.resume_held()
    }

    /// The one resume of the primary thread, for any launch, with the suspend-count
    /// check of [`Self::resume`]. The launch record calls it after checking the
    /// child's attempt-Job membership proof.
    pub(crate) fn resume_held(&mut self) -> Result<(), WindowsLpacError> {
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
        if self.terminated || self.detached {
            return;
        }
        // SAFETY: the process handle is live and owned here until this body
        // returns, and the call reads no caller memory. The child was neither
        // reaped nor terminated through this owner (`terminated`), nor released
        // by the one detach there is, `WindowsLaunchedProcess::detach` of a
        // resumed child bound to a host-death census (`detached`), so it is
        // still this owner's suspended or failed-evidence child, and terminating
        // it keeps it from escaping when callers unwind.
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

/// A merged DACL ready to be written to the exact object
/// [`WindowsLpacProfile::prepare_grant`] opened and read.
struct PreparedGrant {
    /// The retained `READ_CONTROL | WRITE_DAC` handle the DACL was read from.
    original: std::fs::File,
    /// The merged DACL, one `LocalAlloc` allocation from `SetEntriesInAclW`.
    new_acl: LocalAllocation,
}

impl PreparedGrant {
    /// Writes the merged DACL through the retained handle and closes it.
    fn publish(self) -> Result<(), WindowsLpacError> {
        // SAFETY: `original` is still the exact WRITE_DAC handle from the
        // preceding GetSecurityInfo; renaming the path cannot redirect this
        // write. The merged DACL is live for the synchronous SetSecurityInfo.
        let status = unsafe {
            SetSecurityInfo(
                self.original.as_raw_handle().cast(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                self.new_acl.0.cast(),
                std::ptr::null(),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(WindowsLpacError::win32("filesystem DACL write", status));
        }
        Ok(())
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

/// Allocates the second query's buffer through keld-guard's one Windows
/// token-information bound, which refuses the untrusted first size report
/// before any allocation.
fn token_storage(
    required: u32,
    phase: &'static str,
) -> Result<(Vec<usize>, u32), WindowsLpacError> {
    keld_guard::bounded_token_information_buffer(required)
        .map_err(|error| WindowsLpacError::contract(phase, error.to_string()))
}

/// Reads the `TOKEN_GROUPS` entry count of one token class (`TokenCapabilities`
/// for [`WindowsSuspendedChild::observe_token`]) through the bounded read.
fn token_group_count(
    token: &OwnedHandle,
    class: i32,
    phase: &'static str,
) -> Result<u32, WindowsLpacError> {
    let (storage, returned) = query_token_buffer(token, class, phase)?;
    token_groups_count(&storage, returned, phase)
}

/// Reads only the group count after bounding its entire declared record
/// against Win32's actual returned byte length (not the rounded allocation).
fn token_groups_count(
    storage: &[usize],
    returned: usize,
    phase: &'static str,
) -> Result<u32, WindowsLpacError> {
    let header = std::mem::offset_of!(TOKEN_GROUPS, Groups);
    if returned < header || returned > std::mem::size_of_val(storage) {
        return Err(WindowsLpacError::contract(
            phase,
            "token group header or returned length is invalid",
        ));
    }
    let first = storage
        .first()
        .ok_or_else(|| WindowsLpacError::contract(phase, "token group buffer is empty"))?;
    let word = first.to_ne_bytes();
    let count = u32::from_ne_bytes([word[0], word[1], word[2], word[3]]);
    let members = usize::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(std::mem::size_of::<SID_AND_ATTRIBUTES>()))
        .and_then(|size| header.checked_add(size))
        .ok_or_else(|| WindowsLpacError::contract(phase, "token group count overflow"))?;
    if members > returned {
        return Err(WindowsLpacError::contract(
            phase,
            "declared token group entries exceed returned bytes",
        ));
    }
    Ok(count)
}

fn query_token_buffer(
    token: &OwnedHandle,
    class: i32,
    phase: &'static str,
) -> Result<(Vec<usize>, usize), WindowsLpacError> {
    let mut required = 0_u32;
    // SAFETY: null sizing call with writable returned length.
    let _ = unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            class,
            std::ptr::null_mut(),
            0,
            &raw mut required,
        )
    };
    if required == 0 {
        return Err(WindowsLpacError::last_os(phase));
    }
    let (mut storage, capacity) = token_storage(required, phase)?;
    let mut returned = 0_u32;
    // SAFETY: aligned storage holds `capacity` writable bytes, at least
    // `required`; the caller validates `returned` before interpreting the
    // token structure.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            class,
            storage.as_mut_ptr().cast(),
            capacity,
            &raw mut returned,
        )
    } == 0
    {
        return Err(WindowsLpacError::last_os(phase));
    }
    let returned = usize::try_from(returned)
        .map_err(|_| WindowsLpacError::contract(phase, "returned token length exceeds usize"))?;
    if returned == 0 || returned > usize::try_from(required).unwrap_or(usize::MAX) {
        return Err(WindowsLpacError::contract(
            phase,
            "returned token information length exceeds its request",
        ));
    }
    Ok((storage, returned))
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
    fn lpac_token_size_report_is_bounded_by_the_guard_owner_before_allocation() {
        let phase = "test TokenCapabilities";
        let word = std::mem::size_of::<usize>();
        let max = u32::try_from(keld_guard::MAX_TOKEN_INFORMATION_BYTES).expect("bound fits u32");
        assert_eq!(
            max,
            1024 * 1024,
            "the one owner keeps the reviewed 1 MiB bound"
        );
        let (storage, capacity) = super::token_storage(1, phase).expect("one byte");
        assert_eq!(
            (storage.len(), usize::try_from(capacity).expect("fits")),
            (1, word)
        );
        let (storage, capacity) = super::token_storage(max, phase).expect("one MiB cap admitted");
        assert_eq!(
            storage.len(),
            usize::try_from(max).expect("u32 fits usize") / word
        );
        assert_eq!(capacity, max);
        for required in [max + 1, u32::MAX] {
            assert_eq!(
                super::token_storage(required, phase)
                    .expect_err("untrusted oversized token count must be refused before allocation")
                    .to_string(),
                "KELD-RUNTIME-015: Windows LPAC admission failed during test TokenCapabilities: \
                 token information request exceeds 1 MiB. Do not start an unconfined \
                 replacement; repair the profile, ACL, or handle list.",
                "required {required}"
            );
        }
    }

    #[test]
    fn lpac_capability_count_checks_returned_header_and_declared_group_members() {
        use windows_sys::Win32::Security::{SID_AND_ATTRIBUTES, TOKEN_GROUPS};

        let phase = "TokenCapabilities";
        let header = std::mem::offset_of!(TOKEN_GROUPS, Groups);
        let group_bytes = std::mem::size_of::<SID_AND_ATTRIBUTES>();
        let mut backing = [0_usize; 8];
        assert_eq!(
            super::token_groups_count(&backing, header, phase).expect("zero capabilities"),
            0
        );
        backing[0] = 2;
        assert_eq!(
            super::token_groups_count(&backing, header + 2 * group_bytes, phase)
                .expect("complete two-member returned span"),
            2
        );
        for returned in [header, header + group_bytes, header + 2 * group_bytes - 1] {
            let refusal = super::token_groups_count(&backing, returned, phase)
                .expect_err("a declared capability outside returned bytes must refuse");
            assert!(
                refusal.to_string().contains(
                    "TokenCapabilities: declared token group entries exceed returned bytes"
                ),
                "{refusal}"
            );
        }
        backing[0] = 0;
        for returned in [0, header - 1, std::mem::size_of_val(&backing) + 1] {
            let refusal = super::token_groups_count(&backing, returned, phase)
                .expect_err("invalid returned count or incomplete TOKEN_GROUPS header");
            assert!(
                refusal.to_string().contains(
                    "TokenCapabilities: token group header or returned length is invalid"
                ),
                "{refusal}"
            );
        }
    }

    /// The native bounded read that `observe_token` uses, against real kernel
    /// `TOKEN_GROUPS` answers: this process's `TokenGroups` (many entries) and
    /// `TokenCapabilities` (none, outside an `AppContainer`) match an
    /// independent fixed-buffer read, and a returned span cut inside the real
    /// declared entries is refused.
    #[test]
    fn native_token_group_read_matches_an_independent_kernel_read() {
        use std::os::windows::io::{FromRawHandle as _, OwnedHandle};
        use windows_sys::Win32::Security::{
            GetTokenInformation, SID_AND_ATTRIBUTES, TOKEN_GROUPS, TOKEN_QUERY, TokenCapabilities,
            TokenGroups,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        let mut raw_token = std::ptr::null_mut();
        // SAFETY: the current-process pseudo-handle needs no close; the output
        // receives one owned token handle on success.
        let opened =
            unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut raw_token) };
        assert!(opened != 0 && !raw_token.is_null(), "open own token");
        // SAFETY: fresh non-null owning token handle converted once.
        let token = unsafe { OwnedHandle::from_raw_handle(raw_token.cast()) };
        let oracle = |class: i32| -> u32 {
            let mut storage = vec![0_usize; 64 * 1024 / std::mem::size_of::<usize>()];
            let capacity = u32::try_from(std::mem::size_of_val(storage.as_slice())).expect("fits");
            let mut returned = 0_u32;
            // SAFETY: live token; aligned writable storage of `capacity` bytes.
            let ok = unsafe {
                GetTokenInformation(
                    token.as_raw_handle().cast(),
                    class,
                    storage.as_mut_ptr().cast(),
                    capacity,
                    &raw mut returned,
                )
            };
            assert_ne!(ok, 0, "independent fixed-buffer token read");
            // SAFETY: the kernel filled an aligned TOKEN_GROUPS header.
            unsafe { (*storage.as_ptr().cast::<TOKEN_GROUPS>()).GroupCount }
        };

        let groups = super::token_group_count(&token, TokenGroups, "TokenGroups")
            .expect("bounded native TokenGroups read");
        assert!(groups > 0, "a process token belongs to groups");
        assert_eq!(groups, oracle(TokenGroups));
        assert_eq!(
            super::token_group_count(&token, TokenCapabilities, "TokenCapabilities")
                .expect("bounded native TokenCapabilities read"),
            oracle(TokenCapabilities)
        );

        let (storage, returned) = super::query_token_buffer(&token, TokenGroups, "TokenGroups")
            .expect("bounded native TokenGroups buffer");
        let declared = std::mem::offset_of!(TOKEN_GROUPS, Groups)
            + usize::try_from(groups).expect("fits") * std::mem::size_of::<SID_AND_ATTRIBUTES>();
        assert!(
            declared <= returned,
            "the kernel's declared entries fit its returned span"
        );
        assert_eq!(
            super::token_groups_count(&storage, declared, "TokenGroups").expect("exact span"),
            groups
        );
        assert!(
            super::token_groups_count(&storage, declared - 1, "TokenGroups")
                .expect_err("a span cut inside the real entries must refuse")
                .to_string()
                .contains("TokenGroups: declared token group entries exceed returned bytes")
        );
    }

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
            .resume_held()
            .expect_err("a count other than 1 refuses the resume");
        assert!(
            refusal
                .to_string()
                .contains("previous suspend count was 2, not 1"),
            "{refusal}"
        );
        assert!(child.resume_held().is_err(), "the refused resume is spent");
        child.terminate(1).expect("terminate the refused child");
    }

    #[test]
    fn a_startup_structure_that_does_not_name_the_childs_standard_handles_is_refused() {
        // The one creation call owns the rule that every launch names the child's
        // standard handles; a structure without STARTF_USESTDHANDLES refuses before
        // any process exists. Removing the check creates a suspended cmd.exe instead.
        let program =
            std::path::PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set"))
                .join(r"System32\cmd.exe");
        let application =
            super::wide_nul(program.as_os_str(), "application path").expect("encode the path");
        let mut command_line =
            super::encode_command_line(program.as_os_str(), &[std::ffi::OsString::from("/d")])
                .expect("encode the command line");
        let environment = super::encode_environment(&[]).expect("encode the environment");
        let startup = windows_sys::Win32::System::Threading::STARTUPINFOW {
            cb: u32::try_from(std::mem::size_of::<
                windows_sys::Win32::System::Threading::STARTUPINFOW,
            >())
            .expect("STARTUPINFOW size fits u32"),
            ..Default::default()
        };
        let refusal = super::create_suspended_process(
            &application,
            &mut command_line,
            &environment,
            None,
            super::SuspendedStartup::SameToken(&startup),
        )
        .expect_err("a startup structure without STARTF_USESTDHANDLES refuses");
        let text = refusal.to_string();
        assert!(
            text.starts_with("KELD-RUNTIME-020: ")
                && text.contains(
                    "CreateProcessW same-token launch: the startup structure does not name the \
                     child's standard handles: without STARTF_USESTDHANDLES the child would \
                     receive this process's own"
                ),
            "{text}"
        );
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

    #[test]
    fn grant_path_reparse_alias_does_not_mutate_an_unrelated_directory() {
        use std::ffi::OsString;
        use std::os::windows::fs::symlink_dir;
        use std::path::Path;
        use std::process::Command;
        use std::time::{SystemTime, UNIX_EPOCH};

        let fixture = tempfile::tempdir().expect("own an isolated LPAC grant fixture");
        let ordinary = fixture.path().join("ordinary");
        let outsider = fixture.path().join("outsider");
        let alias = fixture.path().join("reparse-alias");
        std::fs::create_dir(&ordinary).expect("own ordinary grant directory");
        std::fs::create_dir(&outsider).expect("own unrelated directory");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("monotonic positive wall-clock generation")
            .as_nanos();
        let profile_name =
            OsString::from(format!("keld-grant-alias-{}-{nonce}", std::process::id()));
        let profile = super::WindowsLpacProfile::create(&profile_name)
            .expect("fresh profile is under the native test owner");
        profile
            .grant_path(&ordinary, super::WindowsLpacPathAccess::ReadExecute)
            .expect("ordinary owner-controlled directory admits a positive grant");

        // Junction fallback requires no elevated symlink privilege on Windows.
        // Both the alias and its target are test-owned paths under one TempDir.
        if let Err(link_error) = symlink_dir(&outsider, &alias) {
            let result = Command::new("cmd.exe")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(&alias)
                .arg(&outsider)
                .status()
                .expect("invoke built-in junction creator");
            assert!(
                result.success(),
                "test-owned reparse alias unavailable: symlink {link_error}; junction {result}"
            );
        }

        let before = observed_dacl(&outsider);
        let granted = profile.grant_path(&alias, super::WindowsLpacPathAccess::RolePrivate);
        let after = observed_dacl(&outsider);
        // Remove exactly this test-owned alias, never recursively remove its target.
        std::fs::remove_dir(&alias).expect("unlink only test-owned alias");
        assert_eq!(
            after, before,
            "granting a reparse alias must never change the unrelated target's actual DACL"
        );
        // The exact admission refusal, not any open or metadata failure.
        assert_eq!(
            granted
                .expect_err("a reparse alias must not be admitted as a grant target")
                .to_string(),
            "KELD-RUNTIME-015: Windows LPAC admission failed during ACL object admission: \
             refuse a reparse-point grant target. Do not start an unconfined replacement; \
             repair the profile, ACL, or handle list."
        );
        assert!(
            Path::new(&outsider).is_dir(),
            "unrelated target was not removed"
        );
    }

    #[test]
    fn grant_path_retained_directory_refuses_post_read_substitution() {
        use std::ffi::OsString;
        use std::time::{SystemTime, UNIX_EPOCH};

        let fixture = tempfile::tempdir().expect("own an isolated pathname-swap fixture");
        let claimed = fixture.path().join("claimed");
        let outsider = fixture.path().join("outsider");
        let moved = fixture.path().join("original-moved");
        std::fs::create_dir(&claimed).expect("test owns the claimed original");
        std::fs::create_dir(&outsider).expect("test owns the independent outsider");
        // An unlocked fixture must support renaming. Without this preflight,
        // an unrelated path/ACL failure could make the later race test green.
        std::fs::rename(&claimed, &moved).expect("unlocked original can be renamed");
        std::fs::rename(&moved, &claimed).expect("restore original after rename preflight");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("positive fixture generation")
            .as_nanos();
        let name = OsString::from(format!("keld-grant-race-{}-{nonce}", std::process::id()));
        let profile = super::WindowsLpacProfile::create(&name).expect("fresh test-owned LPAC SID");
        let outsider_before = observed_dacl(&outsider);
        let original_before = observed_dacl(&claimed);

        // The substitution runs between the shipping ACL read and merge and
        // the publish, in this test body rather than inside the code under
        // test. The same-user rename may succeed on this Windows build: only
        // original HANDLE identity can guarantee that the outsider never
        // receives this ACL grant.
        let prepared = profile
            .prepare_grant(&claimed, super::WindowsLpacPathAccess::RolePrivate)
            .expect("an ordinary retained directory admits its DACL read and merge");
        let mut substituted = false;
        match std::fs::rename(&claimed, &moved) {
            Ok(()) => {
                if let Err(error) = std::fs::rename(&outsider, &claimed) {
                    std::fs::rename(&moved, &claimed)
                        .expect("rescue original after failed outsider substitution");
                    panic!("test-owned outsider substitution failed: {error}");
                }
                substituted = true;
            }
            Err(error) => {
                // The grant handle asks for no data or DELETE access, so
                // Windows 11 does not check sharing against it and the rename
                // succeeds. A build or filesystem that does refuse it may only
                // do so with an actual sharing/permission refusal; a missing
                // path or another fixture error must fail.
                assert!(
                    matches!(error.raw_os_error(), Some(5 | 32)),
                    "unexpected original-rename failure instead of substitution or OS refusal: {error}"
                );
                assert!(
                    claimed.is_dir() && outsider.is_dir() && !moved.exists(),
                    "a refused rename must leave both test-owned identities intact"
                );
            }
        }
        let grant = prepared.publish();
        let outsider_after = observed_dacl(if substituted { &claimed } else { &outsider });
        let original_after = observed_dacl(if substituted { &moved } else { &claimed });
        if substituted {
            // Restore only test-owned entries before any assertion panics. Do
            // not follow links or remove unrelated files in this cleanup.
            std::fs::rename(&claimed, &outsider).expect("rescue owned outsider name");
            std::fs::rename(&moved, &claimed).expect("restore original owner path");
        }
        assert_eq!(
            outsider_after, outsider_before,
            "path substitution must not publish the original ACL grant onto the outsider"
        );
        assert_ne!(
            original_after, original_before,
            "the grant must apply to the original retained object even after rename"
        );
        grant.expect("an ordinary retained directory receives its intended package grant");
    }

    #[test]
    fn grant_path_retained_handle_preserves_original_after_forced_directory_swap() {
        use std::ffi::OsString;
        use std::time::{SystemTime, UNIX_EPOCH};

        let fixture = tempfile::tempdir().expect("own isolated forced-swap fixture");
        let claimed = fixture.path().join("claimed");
        let outsider = fixture.path().join("outsider");
        let moved = fixture.path().join("original-moved");
        std::fs::create_dir(&claimed).expect("create owned original");
        std::fs::create_dir(&outsider).expect("create independent outsider");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("positive fixture generation")
            .as_nanos();
        let name = OsString::from(format!("keld-forced-swap-{}-{nonce}", std::process::id()));
        let profile = super::WindowsLpacProfile::create(&name).expect("test-owned profile SID");
        let outsider_before = observed_dacl(&outsider);
        let original_before = observed_dacl(&claimed);

        // This native oracle MUST replace the pathname while the shipping
        // grant handle stays open: that handle asks for no data or DELETE
        // access, so its share mode does not refuse the rename. Refusing
        // either rename is a failure, not an acceptable green path. The swap
        // runs in this test body, between the shipping read and merge and the
        // publish, with the shipping open configuration.
        let prepared = profile
            .prepare_grant(&claimed, super::WindowsLpacPathAccess::RolePrivate)
            .expect("an ordinary retained directory admits its DACL read and merge");
        std::fs::rename(&claimed, &moved)
            .expect("the shipping grant handle must permit the forced rename");
        if let Err(error) = std::fs::rename(&outsider, &claimed) {
            std::fs::rename(&moved, &claimed)
                .expect("restore owned original if outsider substitution fails");
            panic!("forced outsider substitution failed: {error}");
        }
        let grant = prepared.publish();
        let outsider_after = observed_dacl(&claimed);
        let original_after = observed_dacl(&moved);
        std::fs::rename(&claimed, &outsider).expect("restore owned outsider");
        std::fs::rename(&moved, &claimed).expect("restore owned original");

        assert_eq!(
            outsider_after, outsider_before,
            "forced pathname swap must never grant the unrelated outsider's DACL"
        );
        assert_ne!(
            original_after, original_before,
            "retained HANDLE must publish the intended ACL to the renamed original"
        );
        grant.expect("the forced rename does not prevent a grant on the retained HANDLE");
    }

    fn observed_dacl(path: &std::path::Path) -> String {
        use std::os::windows::ffi::OsStrExt as _;
        use windows_sys::Win32::Security::Authorization::{
            ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
            SE_FILE_OBJECT,
        };
        use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;

        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: live NUL-terminated path, writable descriptor output.
        let status = unsafe {
            GetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut descriptor,
            )
        };
        assert_eq!(status, 0, "read independently observed test-owned DACL");
        let _descriptor = super::LocalAllocation(descriptor);
        let mut sddl = std::ptr::null_mut();
        // SAFETY: live security descriptor, writable string output.
        assert_ne!(
            unsafe {
                ConvertSecurityDescriptorToStringSecurityDescriptorW(
                    descriptor,
                    1,
                    DACL_SECURITY_INFORMATION,
                    &raw mut sddl,
                    std::ptr::null_mut(),
                )
            },
            0,
            "convert actual kernel DACL to independent SDDL"
        );
        let _sddl = super::LocalAllocation(sddl.cast());
        let mut len = 0;
        while len < 4096 {
            // SAFETY: conversion guarantees a terminated UTF-16 LocalAlloc string.
            if unsafe { *sddl.add(len) } == 0 {
                break;
            }
            len += 1;
        }
        assert!(len < 4096, "bounded SDDL conversion");
        // SAFETY: LocalAlloc string is live and terminated at this bounded offset.
        String::from_utf16(unsafe { std::slice::from_raw_parts(sddl, len) })
            .expect("OS descriptor has valid UTF-16")
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
