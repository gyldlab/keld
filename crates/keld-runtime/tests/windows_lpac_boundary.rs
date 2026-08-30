//! Real Windows LPAC, ACL, hostile-probe, and raw handle-census proof.

#![cfg(windows)]
#![allow(unsafe_code)] // isolated test-only NT/Win32 observation with local ABI proofs
#![allow(clippy::expect_used, clippy::panic)] // process fixture invariants must abort loudly
#![deny(unsafe_op_in_unsafe_fn)]

use std::env;
use std::ffi::{OsStr, OsString, c_void};
use std::fs::{File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::{AsHandle as _, AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use keld_runtime::windows_lpac::{WindowsLpacPathAccess, WindowsLpacProfile, WindowsLpacStdio};
use tempfile::tempdir;
use windows_sys::Win32::Foundation::{
    CloseHandle, CompareObjectHandles, DUPLICATE_SAME_ACCESS, DuplicateHandle,
    GetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, LocalFree, SetHandleInformation,
};
use windows_sys::Win32::Networking::WinSock::{
    AF_INET, INVALID_SOCKET, IPPROTO_TCP, SOCK_STREAM, SOCKADDR, SOCKADDR_IN, SOCKET_ERROR,
    WSACleanup, WSADATA, WSAStartup, bind, closesocket, socket,
};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, SET_ACCESS,
    SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_WELL_KNOWN_GROUP,
    TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    CreateWellKnownSid, DACL_SECURITY_INFORMATION, GetTokenInformation, TOKEN_GROUPS, TOKEN_QUERY,
    TokenCapabilities, TokenIsAppContainer, WinBuiltinAnyPackageSid,
};
use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ;
use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegOpenKeyExW,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessHandleCount, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

const HELPER_ENV: &str = "KELD_WINDOWS_LPAC_HELPER";
const HELPER_TEST: &str = "windows_lpac_process_helper";
const SYSTEM_EXTENDED_HANDLE_INFORMATION: u32 = 64;
const STATUS_INFO_LENGTH_MISMATCH: i32 = -1_073_741_820;

unsafe extern "system" {
    fn NtQuerySystemInformation(
        system_information_class: u32,
        system_information: *mut c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct SystemHandleEntry {
    object: *mut c_void,
    unique_process_id: usize,
    handle_value: usize,
    granted_access: u32,
    creator_backtrace_index: u16,
    object_type_index: u16,
    handle_attributes: u32,
    reserved: u32,
}

#[test]
#[allow(clippy::too_many_lines)] // one crash-safe end-to-end boundary fixture and oracle
fn zero_capability_lpac_denies_host_authority_and_inherits_only_allowlisted_handles() {
    let temporary = tempdir().expect("create isolated LPAC fixture root");
    let root = temporary.path();
    let runtime_dir = root.join("runtime");
    let role_dir = root.join("role-private");
    let forbidden_dir = root.join("host-only");
    std::fs::create_dir_all(&runtime_dir).expect("create runtime directory");
    std::fs::create_dir_all(&role_dir).expect("create role-private directory");
    std::fs::create_dir_all(&forbidden_dir).expect("create host-only directory");

    let fixture = runtime_dir.join("lpac-probe.exe");
    std::fs::copy(
        env::current_exe().expect("current test executable"),
        &fixture,
    )
    .expect("copy signed-equivalent test artifact");
    let forbidden_file = forbidden_dir.join("secret.txt");
    std::fs::write(&forbidden_file, b"host secret").expect("write forbidden fixture");
    let blocked_dll = forbidden_dir.join("blocked.dll");
    let system_root = env::var_os("SystemRoot").unwrap_or_else(|| OsString::from(r"C:\Windows"));
    std::fs::copy(
        Path::new(&system_root).join(r"System32\version.dll"),
        &blocked_dll,
    )
    .expect("copy real DLL outside LPAC ACL");
    let all_packages_file = forbidden_dir.join("ordinary-appcontainer-visible.txt");
    std::fs::write(&all_packages_file, b"ordinary AppContainer grant")
        .expect("write All Application Packages fixture");
    grant_all_application_packages_read(&all_packages_file);
    let update_stage = forbidden_dir.join("update-stage");
    std::fs::create_dir(&update_stage).expect("create host-owned update stage");
    let update_payload = update_stage.join("payload.bin");
    std::fs::write(&update_payload, b"signed update payload").expect("write staged payload");

    let profile_name = unique_profile_name();
    let profile =
        WindowsLpacProfile::create(&profile_name).expect("create zero-capability profile");
    profile
        .grant_path(root, WindowsLpacPathAccess::Traverse)
        .expect("grant root traversal only");
    profile
        .grant_path(&runtime_dir, WindowsLpacPathAccess::ReadExecute)
        .expect("grant runtime read/execute");
    profile
        .grant_path(&role_dir, WindowsLpacPathAccess::RolePrivate)
        .expect("grant role-private read/write");

    let output_path = role_dir.join("probe-output.txt");
    let mut output = OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&output_path)
        .expect("open LPAC log sink");
    let input = File::open("NUL").expect("open null stdin");
    let marker_path = forbidden_dir.join("inheritable-host-marker.txt");
    let marker = OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&marker_path)
        .expect("open host-only inheritable marker");
    set_inheritable(marker.as_raw_handle().cast(), true);

    let allowed_file = role_dir.join("allowed.txt");
    let arguments = vec![
        OsString::from("--exact"),
        OsString::from(HELPER_TEST),
        OsString::from("--ignored"),
        OsString::from("--nocapture"),
    ];
    let mut environment = vec![
        (OsString::from(HELPER_ENV), OsString::from("probe")),
        (
            OsString::from("KELD_LPAC_ALLOWED"),
            allowed_file.into_os_string(),
        ),
        (
            OsString::from("KELD_LPAC_DENIED"),
            forbidden_file.into_os_string(),
        ),
        (
            OsString::from("KELD_LPAC_BLOCKED_DLL"),
            blocked_dll.into_os_string(),
        ),
        (
            OsString::from("KELD_LPAC_ALL_PACKAGES_FILE"),
            all_packages_file.into_os_string(),
        ),
        (
            OsString::from("KELD_LPAC_UPDATE_STAGE"),
            update_payload.into_os_string(),
        ),
        (
            OsString::from("KELD_LPAC_MARKER"),
            OsString::from(format!("{:x}", marker.as_raw_handle() as usize)),
        ),
        (
            OsString::from("KELD_LPAC_CONTROLLER_PID"),
            OsString::from(std::process::id().to_string()),
        ),
        (OsString::from("SystemRoot"), system_root),
        (OsString::from("TEMP"), role_dir.clone().into_os_string()),
        (OsString::from("TMP"), role_dir.clone().into_os_string()),
    ];
    for key in ["LOCALAPPDATA", "USERPROFILE", "WINDIR"] {
        if let Some(value) = env::var_os(key) {
            environment.push((OsString::from(key), value));
        }
    }
    let stdio = WindowsLpacStdio {
        stdin: input.as_handle(),
        stdout: output.as_handle(),
        stderr: output.as_handle(),
    };
    let mut child = profile
        .spawn_suspended(
            &fixture,
            &arguments,
            &environment,
            Some(&role_dir),
            Some(stdio),
            &[],
        )
        .expect("create suspended LPAC fixture");

    let token = child.observe_token().expect("observe LPAC child token");
    assert_eq!(
        token,
        keld_runtime::windows_lpac::WindowsLpacTokenObservation {
            is_app_container: true,
            all_application_packages_opt_out_configured: true,
            capability_count: 0,
        }
    );

    let census = raw_process_handle_census(child.id());
    let kernel_count = process_handle_count(child.process_handle().as_raw_handle().cast());
    assert_eq!(
        census.len(),
        usize::try_from(kernel_count).expect("handle count fits usize"),
        "raw SystemExtendedHandleInformation census must match GetProcessHandleCount"
    );
    assert!(
        child_contains_object(&child, &census, input.as_raw_handle().cast()),
        "allowlisted stdin object missing from child handle table"
    );
    assert!(
        child_contains_object(&child, &census, output.as_raw_handle().cast()),
        "allowlisted log object missing from child handle table"
    );
    assert!(
        !child_contains_object(&child, &census, marker.as_raw_handle().cast()),
        "non-allowlisted inheritable host file crossed the child boundary"
    );
    assert_no_other_inheritable_parent_object(
        &child,
        &census,
        &[input.as_raw_handle().cast(), output.as_raw_handle().cast()],
    );

    child.resume().expect("resume audited LPAC child");
    let exit = child.wait(10_000).expect("wait for hostile LPAC probe");

    output
        .seek(SeekFrom::Start(0))
        .expect("rewind probe output");
    let mut observed = String::new();
    output
        .read_to_string(&mut observed)
        .expect("read hostile probe output");
    assert_eq!(
        exit, 0,
        "LPAC probe exited unsuccessfully; output: {observed}"
    );
    assert!(
        observed.contains(
            "LPAC_PROBE private_write=true forbidden_read_denied=true all_packages_denied=true \
             network_denied=true registry_denied=true controller_open_denied=true \
             dll_load_denied=true update_stage_denied=true"
        ),
        "unexpected hostile LPAC output: {observed}"
    );
    assert!(
        observed.contains(
            "LPAC_DESCENDANT app_container=true capability_count=0 all_packages_denied=true"
        ) || observed.contains("LPAC_DESCENDANT spawn_denied=true"),
        "descendant neither inherited LPAC nor received an OS spawn deny: {observed}"
    );
    assert!(
        !handle_inheritable(input.as_raw_handle().cast()),
        "stdin inherit flag was not restored"
    );
    assert!(
        !handle_inheritable(output.as_raw_handle().cast()),
        "log inherit flag was not restored"
    );
    set_inheritable(marker.as_raw_handle().cast(), false);
}

#[test]
#[ignore = "private LPAC subprocess entry point"]
fn windows_lpac_process_helper() {
    if env::var(HELPER_ENV).as_deref() == Ok("descendant") {
        run_lpac_descendant_probe();
        return;
    }
    assert_eq!(env::var(HELPER_ENV).as_deref(), Ok("probe"));
    let allowed = PathBuf::from(env::var_os("KELD_LPAC_ALLOWED").expect("allowed path"));
    let denied = PathBuf::from(env::var_os("KELD_LPAC_DENIED").expect("denied path"));
    let blocked_dll =
        PathBuf::from(env::var_os("KELD_LPAC_BLOCKED_DLL").expect("blocked DLL path"));
    let all_packages_file = PathBuf::from(
        env::var_os("KELD_LPAC_ALL_PACKAGES_FILE").expect("All Application Packages path"),
    );
    let update_stage =
        PathBuf::from(env::var_os("KELD_LPAC_UPDATE_STAGE").expect("update stage path"));
    let controller_pid: u32 = env::var("KELD_LPAC_CONTROLLER_PID")
        .expect("controller PID")
        .parse()
        .expect("numeric controller PID");

    let private_write = std::fs::write(&allowed, b"role data").is_ok();
    let forbidden_read_denied = std::fs::read(&denied).is_err();
    let all_packages_denied = std::fs::read(&all_packages_file).is_err();
    let network_denied = direct_network_denied();
    let registry_denied = registry_open_denied();
    let controller_open_denied = process_open_denied(controller_pid);
    let dll_load_denied = library_load_denied(&blocked_dll);
    let update_stage_denied = std::fs::read(&update_stage).is_err()
        && OpenOptions::new().write(true).open(&update_stage).is_err();
    let descendant_contained = spawn_contained_descendant();
    println!(
        "LPAC_PROBE private_write={private_write} forbidden_read_denied={forbidden_read_denied} \
         all_packages_denied={all_packages_denied} network_denied={network_denied} \
         registry_denied={registry_denied} controller_open_denied={controller_open_denied} \
         dll_load_denied={dll_load_denied} update_stage_denied={update_stage_denied} \
         descendant_contained={descendant_contained}"
    );
    std::io::stdout().flush().expect("flush LPAC probe output");
    assert!(private_write);
    assert!(forbidden_read_denied);
    assert!(all_packages_denied);
    assert!(network_denied);
    assert!(registry_denied);
    assert!(controller_open_denied);
    assert!(dll_load_denied);
    assert!(update_stage_denied);
    assert!(descendant_contained);
}

fn spawn_contained_descendant() -> bool {
    match Command::new(env::current_exe().expect("current LPAC fixture executable"))
        .args(["--exact", HELPER_TEST, "--ignored", "--nocapture"])
        .env(HELPER_ENV, "descendant")
        .status()
    {
        Ok(status) => status.success(),
        Err(error) => {
            let denied = error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(5);
            println!(
                "LPAC_DESCENDANT spawn_denied={denied} raw_error={:?}",
                error.raw_os_error()
            );
            std::io::stdout()
                .flush()
                .expect("flush descendant spawn denial");
            denied
        }
    }
}

fn run_lpac_descendant_probe() {
    let (is_app_container, capability_count) = observe_current_token();
    let all_packages_file = PathBuf::from(
        env::var_os("KELD_LPAC_ALL_PACKAGES_FILE").expect("All Application Packages path"),
    );
    let all_packages_denied = std::fs::read(all_packages_file).is_err();
    println!(
        "LPAC_DESCENDANT app_container={is_app_container} capability_count={capability_count} \
         all_packages_denied={all_packages_denied}"
    );
    std::io::stdout()
        .flush()
        .expect("flush descendant LPAC observation");
    assert!(is_app_container);
    assert_eq!(capability_count, 0);
    assert!(all_packages_denied);
}

fn observe_current_token() -> (bool, u32) {
    let mut raw_token = std::ptr::null_mut();
    // SAFETY: current-process pseudo-handle is valid and token output writable.
    assert_ne!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut raw_token) },
        0,
        "open current LPAC token: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: successful open returned one fresh owning token handle.
    let token = unsafe { OwnedHandle::from_raw_handle(raw_token.cast()) };
    let mut is_app_container = 0_u32;
    let mut returned = 0_u32;
    // SAFETY: token and both output slots are live.
    assert_ne!(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                TokenIsAppContainer,
                (&raw mut is_app_container).cast(),
                u32::try_from(std::mem::size_of_val(&is_app_container)).unwrap_or(u32::MAX),
                &raw mut returned,
            )
        },
        0,
        "query descendant TokenIsAppContainer"
    );
    let mut capability_bytes = 0_u32;
    // SAFETY: null sizing call with writable returned length.
    let _ = unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            TokenCapabilities,
            std::ptr::null_mut(),
            0,
            &raw mut capability_bytes,
        )
    };
    let mut capabilities = vec![
        0_usize;
        usize::try_from(capability_bytes)
            .expect("capability size fits usize")
            .div_ceil(std::mem::size_of::<usize>())
    ];
    // SAFETY: aligned storage has capability_bytes writable bytes.
    assert_ne!(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                TokenCapabilities,
                capabilities.as_mut_ptr().cast(),
                capability_bytes,
                &raw mut capability_bytes,
            )
        },
        0,
        "query descendant TokenCapabilities"
    );
    // SAFETY: the returned buffer is a live aligned TOKEN_GROUPS instance.
    let capability_count = unsafe { (*(capabilities.as_ptr().cast::<TOKEN_GROUPS>())).GroupCount };
    (is_app_container != 0, capability_count)
}

fn grant_all_application_packages_read(path: &Path) {
    let mut sid_bytes = 0_u32;
    // SAFETY: null is the documented sizing call and sid_bytes is writable.
    let _ = unsafe {
        CreateWellKnownSid(
            WinBuiltinAnyPackageSid,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut sid_bytes,
        )
    };
    assert_ne!(sid_bytes, 0, "All Application Packages SID sizing failed");
    let mut sid_storage = vec![
        0_usize;
        usize::try_from(sid_bytes)
            .expect("SID size fits usize")
            .div_ceil(std::mem::size_of::<usize>())
    ];
    // SAFETY: aligned storage contains sid_bytes writable bytes.
    assert_ne!(
        unsafe {
            CreateWellKnownSid(
                WinBuiltinAnyPackageSid,
                std::ptr::null_mut(),
                sid_storage.as_mut_ptr().cast(),
                &raw mut sid_bytes,
            )
        },
        0,
        "create All Application Packages SID: {}",
        std::io::Error::last_os_error()
    );

    let path_wide = wide_nul(path.as_os_str());
    let mut old_acl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: path is NUL terminated and output slots are writable.
    assert_eq!(
        unsafe {
            GetNamedSecurityInfoW(
                path_wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut old_acl,
                std::ptr::null_mut(),
                &raw mut descriptor,
            )
        },
        0,
        "read All Application Packages fixture DACL"
    );
    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_GENERIC_READ,
        grfAccessMode: SET_ACCESS,
        grfInheritance: 0,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_WELL_KNOWN_GROUP,
            ptstrName: sid_storage.as_mut_ptr().cast(),
        },
    };
    let mut new_acl = std::ptr::null_mut();
    // SAFETY: entry and old ACL are live; new ACL output is writable.
    assert_eq!(
        unsafe { SetEntriesInAclW(1, &raw const entry, old_acl, &raw mut new_acl) },
        0,
        "merge All Application Packages fixture DACL"
    );
    // SAFETY: path and merged ACL are live for the synchronous update.
    let status = unsafe {
        SetNamedSecurityInfoW(
            path_wide.as_ptr().cast_mut(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            new_acl,
            std::ptr::null(),
        )
    };
    // SAFETY: both are LocalAlloc-family allocations returned above.
    let _ = unsafe { LocalFree(new_acl.cast()) };
    let _ = unsafe { LocalFree(descriptor) };
    assert_eq!(status, 0, "write All Application Packages fixture DACL");
}

fn registry_open_denied() -> bool {
    let subkey = wide_nul(OsStr::new("Software"));
    let mut key = std::ptr::null_mut();
    // SAFETY: subkey is NUL terminated and key output is writable.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_READ,
            &raw mut key,
        )
    };
    if status == 0 && !key.is_null() {
        // SAFETY: successful open returned one owning registry handle.
        let _ = unsafe { RegCloseKey(key) };
        false
    } else {
        true
    }
}

fn direct_network_denied() -> bool {
    let mut data = WSADATA::default();
    // SAFETY: WSADATA output is writable. A startup failure under the
    // zero-capability token is itself an OS-layer network denial.
    if unsafe { WSAStartup(0x0202, &raw mut data) } != 0 {
        return true;
    }
    // SAFETY: constant address family/type/protocol tuple.
    let socket = unsafe { socket(i32::from(AF_INET), SOCK_STREAM, IPPROTO_TCP) };
    if socket == INVALID_SOCKET {
        // SAFETY: balances the successful WSAStartup above.
        let _ = unsafe { WSACleanup() };
        return true;
    }
    let address = SOCKADDR_IN {
        sin_family: AF_INET,
        ..SOCKADDR_IN::default()
    };
    // Port zero and address zero ask the OS for any local endpoint; no fixed
    // resource or timing is involved.
    // SAFETY: socket is live and address has the exact SOCKADDR_IN layout.
    let result = unsafe {
        bind(
            socket,
            (&raw const address).cast::<SOCKADDR>(),
            i32::try_from(std::mem::size_of_val(&address)).unwrap_or(i32::MAX),
        )
    };
    // SAFETY: both calls release only resources created above.
    let _ = unsafe { closesocket(socket) };
    let _ = unsafe { WSACleanup() };
    result == SOCKET_ERROR
}

fn process_open_denied(pid: u32) -> bool {
    // SAFETY: numeric controller PID; no output pointer.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        true
    } else {
        // SAFETY: successful open returned one owning process handle.
        let _ = unsafe { CloseHandle(process) };
        false
    }
}

fn library_load_denied(path: &Path) -> bool {
    let path = wide_nul(path.as_os_str());
    // SAFETY: path is a live NUL-terminated UTF-16 buffer. A successful load
    // would be a failed boundary and the short-lived probe then aborts.
    unsafe { LoadLibraryW(path.as_ptr()) }.is_null()
}

fn raw_process_handle_census(pid: u32) -> Vec<SystemHandleEntry> {
    let mut bytes = 1_u32 << 20;
    loop {
        let words = usize::try_from(bytes)
            .expect("NT census size fits usize")
            .div_ceil(std::mem::size_of::<usize>());
        let mut storage = vec![0_usize; words];
        let mut returned = 0_u32;
        // SAFETY: aligned storage provides `bytes` writable bytes; returned
        // length is writable. Class 64 is the raw extended system handle table.
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_EXTENDED_HANDLE_INFORMATION,
                storage.as_mut_ptr().cast(),
                bytes,
                &raw mut returned,
            )
        };
        if status == STATUS_INFO_LENGTH_MISMATCH {
            bytes = returned.max(bytes.saturating_mul(2));
            continue;
        }
        assert_eq!(
            status,
            0,
            "NtQuerySystemInformation failed: 0x{:08x}",
            status.cast_unsigned()
        );

        let count = storage[0];
        let header_bytes = 2 * std::mem::size_of::<usize>();
        let available = usize::try_from(bytes)
            .expect("NT census size fits usize")
            .saturating_sub(header_bytes)
            / std::mem::size_of::<SystemHandleEntry>();
        assert!(count <= available, "NT handle census count exceeds buffer");
        // SAFETY: the class-64 buffer begins with two usize fields followed by
        // `count` aligned SystemHandleEntry records, bounded above by available.
        let entries = unsafe {
            std::slice::from_raw_parts(storage.as_ptr().add(2).cast::<SystemHandleEntry>(), count)
        };
        return entries
            .iter()
            .copied()
            .filter(|entry| entry.unique_process_id == pid as usize)
            .collect();
    }
}

fn process_handle_count(process: HANDLE) -> u32 {
    let mut count = 0_u32;
    // SAFETY: process is a live borrowed process handle; count is writable.
    assert_ne!(
        unsafe { GetProcessHandleCount(process, &raw mut count) },
        0,
        "GetProcessHandleCount failed: {}",
        std::io::Error::last_os_error()
    );
    count
}

fn child_contains_object(
    child: &keld_runtime::windows_lpac::WindowsLpacChild,
    census: &[SystemHandleEntry],
    parent_object: HANDLE,
) -> bool {
    census.iter().any(|entry| {
        let mut duplicate = std::ptr::null_mut();
        // SAFETY: child process handle is live; entry came from that exact PID
        // in the raw table; current-process pseudo-handle is valid; duplicate
        // output receives one handle on success.
        if unsafe {
            DuplicateHandle(
                child.process_handle().as_raw_handle().cast(),
                entry.handle_value as HANDLE,
                GetCurrentProcess(),
                &raw mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
            || duplicate.is_null()
        {
            return false;
        }
        // SAFETY: duplicate is the fresh owning handle returned above.
        let duplicate = unsafe { OwnedHandle::from_raw_handle(duplicate.cast()) };
        // SAFETY: both compared handles are live for this call.
        (unsafe { CompareObjectHandles(parent_object, duplicate.as_raw_handle().cast()) }) != 0
    })
}

fn assert_no_other_inheritable_parent_object(
    child: &keld_runtime::windows_lpac::WindowsLpacChild,
    child_census: &[SystemHandleEntry],
    allowed: &[HANDLE],
) {
    let parent_census = raw_process_handle_census(std::process::id());
    for entry in parent_census {
        let handle = entry.handle_value as HANDLE;
        if allowed.contains(&handle) || !handle_inheritable(handle) {
            continue;
        }
        assert!(
            !child_contains_object(child, child_census, handle),
            "inheritable parent handle 0x{:x} crossed outside HANDLE_LIST",
            entry.handle_value
        );
    }
}

fn set_inheritable(handle: HANDLE, inheritable: bool) {
    let value = if inheritable { HANDLE_FLAG_INHERIT } else { 0 };
    // SAFETY: handle is live; only the inherit bit is changed.
    assert_ne!(
        unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, value) },
        0,
        "SetHandleInformation failed: {}",
        std::io::Error::last_os_error()
    );
}

fn handle_inheritable(handle: HANDLE) -> bool {
    let mut flags = 0_u32;
    // SAFETY: census supplied a handle value; failure means the value is no
    // longer live and therefore cannot cross this spawn boundary.
    (unsafe { GetHandleInformation(handle, &raw mut flags) }) != 0
        && flags & HANDLE_FLAG_INHERIT != 0
}

fn unique_profile_name() -> OsString {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    OsString::from(format!("Keld.Test.{}.{nanos}", std::process::id()))
}

fn wide_nul(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}
