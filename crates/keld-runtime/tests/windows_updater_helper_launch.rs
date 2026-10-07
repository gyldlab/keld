//! The updater helper's elevated launch primitive (KEL-53 §4 "Helper launch and
//! self-anchor"; §7 row "17 (helper launch and self-anchor)", launch cells).
//!
//! CI covers what is decided before the shell is called: the closed helper
//! path and argument types. The `ShellExecuteExW` block itself runs in CI
//! through the `open` verb, in `windows_job.rs`'s unit test
//! `shell_launch_owns_the_process_handle_and_passes_the_argument_and_directory`.
//! A `runas` launch shows the UAC prompt, so the consent and decline cells are
//! `#[ignore]`d operator rows; each names its exact command and expected
//! observation.

#![cfg(windows)]
#![allow(unsafe_code)] // isolated test-only process observation with local ABI proofs
#![allow(clippy::expect_used, clippy::panic)] // fixture invariants must abort the proof loudly
#![deny(unsafe_op_in_unsafe_fn)]

use std::env;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt as _;
use std::os::windows::io::{AsHandle as _, AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use keld_runtime::windows_job::{
    WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR, WindowsUpdaterHelperArgument,
    WindowsUpdaterHelperLaunchError, WindowsUpdaterHelperPath, launch_elevated_updater_helper,
};
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, GetProcessId, OpenProcess, OpenProcessToken,
    PROCESS_NAME_WIN32, PROCESS_VM_READ, QueryFullProcessImageNameW, WaitForSingleObject,
};

/// 64 lowercase hex digits, written out here rather than derived by the code
/// under test.
const LOCATOR: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn rendezvous(locator: &str) -> String {
    format!(r"\\.\pipe\keld-attempt-{locator}")
}

#[test]
fn helper_path_admits_only_a_win32_canonical_drive_letter_exe_path() {
    const NO_DRIVE: &str = "it does not start with a drive letter";
    const PREFIX: &str = "its prefix is not a plain drive letter";
    const NO_ROOT: &str = "it is not absolute from the drive root";
    const PARENT: &str = "it has a `..` component";
    const STREAM: &str = "it names an alternate data stream";
    const FORBIDDEN: &str = "it holds a character that Win32 forbids in a name";
    const DEVICE: &str = "a component is a reserved device name";
    const NO_FILE: &str = "it names no file";
    const NOT_EXE: &str = "it does not name an `.exe` image";
    const SPELLING: &str =
        "it is not canonical: a `.` component or a `/`, doubled or trailing separator";
    const REWRITTEN: &str = "Win32 rewrites it to a different full path";
    const TRAILING: &str = "a component ends in `.` or a space";

    for accepted in [
        r"C:\Program Files\Keld App\versions\1.2.3\keld-updater-helper.exe",
        r"c:\x.EXE",
        r"D:\keld-updater-helper.exe",
        r"C:\keld\.hidden\keld-updater-helper.exe",
        r"C:\keld\CONSOLE\COM10.exe",
        r"C:\keld\nul-helper\LPT10.exe",
    ] {
        let path = WindowsUpdaterHelperPath::new(Path::new(accepted))
            .unwrap_or_else(|error| panic!("{accepted} must be admitted: {error}"));
        assert_eq!(path.as_path(), Path::new(accepted));
    }

    let refused: &[(&str, &str)] = &[
        ("", NO_DRIVE),
        ("keld-updater-helper.exe", NO_DRIVE),
        (r".\keld-updater-helper.exe", NO_DRIVE),
        (r"\keld\keld-updater-helper.exe", NO_DRIVE),
        (r"\\server\share\keld-updater-helper.exe", PREFIX),
        (r"\\?\C:\keld\keld-updater-helper.exe", PREFIX),
        (r"\\.\C:\keld\keld-updater-helper.exe", PREFIX),
        (r"C:keld-updater-helper.exe", NO_ROOT),
        (r"C:\keld\..\keld-updater-helper.exe", PARENT),
        (r"C:\keld\helper.exe:stream", STREAM),
        (r"C:\keld\file:keld-updater-helper.exe", STREAM),
        (r#"C:\keld\a" --recovery-role ".exe"#, FORBIDDEN),
        (r"C:\keld\*.exe", FORBIDDEN),
        (r"C:\keld\?.exe", FORBIDDEN),
        (r"C:\keld\a|b.exe", FORBIDDEN),
        (r"C:\keld\a<b.exe", FORBIDDEN),
        (r"C:\keld\a>b.exe", FORBIDDEN),
        ("C:\\keld\\a\u{1}b.exe", FORBIDDEN),
        ("C:\\keld\\a\u{1f}b.exe", FORBIDDEN),
        ("C:\\keld\\keld\0updater-helper.exe", FORBIDDEN),
        (r"C:\keld\CON.exe", DEVICE),
        (r"C:\keld\NUL.exe", DEVICE),
        (r"C:\CON\keld-updater-helper.exe", DEVICE),
        (r"C:\keld\COM1\keld-updater-helper.exe", DEVICE),
        (r"C:\keld\lpt9.exe", DEVICE),
        ("C:\\keld\\COM\u{b9}\\keld-updater-helper.exe", DEVICE),
        (r"C:\keld\aux.tar.exe", DEVICE),
        (r"C:\keld\prn .exe", DEVICE),
        (r"C:\", NO_FILE),
        (r"C:\keld\keld-updater-helper.txt", NOT_EXE),
        (r"C:\keld\keld-updater-helper", NOT_EXE),
        (r"C:\keld\keld-updater-helper.exe.", NOT_EXE),
        (r"C:\keld\keld-updater-helper.exe ", NOT_EXE),
        (r"C:\keld\.\keld-updater-helper.exe", SPELLING),
        ("C:/keld/keld-updater-helper.exe", SPELLING),
        (r"C:\keld\\keld-updater-helper.exe", SPELLING),
        (r"C:\keld\keld-updater-helper.exe\", SPELLING),
        // GetFullPathNameW removes a single trailing period from an inner
        // component, but keeps an inner trailing space and a run of periods.
        (r"C:\keld.\keld-updater-helper.exe", REWRITTEN),
        (r"C:\keld \keld-updater-helper.exe", TRAILING),
        (r"C:\keld\...\keld-updater-helper.exe", TRAILING),
    ];
    for (path, rule) in refused {
        match WindowsUpdaterHelperPath::new(Path::new(path)) {
            Err(WindowsUpdaterHelperLaunchError::HelperPath { rule: actual }) => {
                assert_eq!(actual, *rule, "{path:?}");
            }
            other => panic!("{path:?} must be refused with {rule:?}, got {other:?}"),
        }
    }
}

#[test]
fn helper_argument_admits_only_an_exact_rendezvous_or_the_recovery_selector() {
    assert_eq!(WINDOWS_UPDATER_HELPER_RECOVERY_SELECTOR, "--recovery-role");
    let recovery = WindowsUpdaterHelperArgument::recovery();
    assert_eq!(recovery.as_str(), "--recovery-role");

    let name = rendezvous(LOCATOR);
    let activation =
        WindowsUpdaterHelperArgument::activation(&name).expect("an exact rendezvous name");
    assert_eq!(activation.as_str(), name);

    // Each admitted argument is one command-line token.
    for argument in [&recovery, &activation] {
        assert!(
            !argument
                .as_str()
                .chars()
                .any(|character| character.is_whitespace() || character == '"'),
            "{argument:?}"
        );
    }

    let upper = LOCATOR.to_ascii_uppercase();
    let refused = [
        String::new(),
        "--recovery-role".to_owned(),
        "--RECOVERY-ROLE".to_owned(),
        format!(r"\\server\pipe\keld-attempt-{LOCATOR}"),
        format!(r"\\?\pipe\keld-attempt-{LOCATOR}"),
        format!(r"\\.\pipe\keld-lifecycle-{LOCATOR}"),
        format!(r"\\.\pipe\keld-{LOCATOR}"),
        rendezvous(&upper),
        rendezvous(&LOCATOR[..63]),
        rendezvous(&format!("{LOCATOR}0")),
        format!("{name} "),
        format!(" {name}"),
        format!("{name} --recovery-role"),
    ];
    for argument in refused {
        assert!(
            matches!(
                WindowsUpdaterHelperArgument::activation(&argument),
                Err(WindowsUpdaterHelperLaunchError::ArgumentShape)
            ),
            "{argument:?} must be refused"
        );
    }
}

#[test]
fn launch_refusals_name_their_code_and_fix() {
    let path = WindowsUpdaterHelperPath::new(Path::new("helper.exe"))
        .expect_err("a bare name is refused")
        .to_string();
    assert!(
        path.starts_with(
            "KELD-RUNTIME-019: elevated updater-helper launch refused: the helper path is not a \
             Win32-canonical drive-letter `.exe` path: it does not start with a drive letter."
        ),
        "{path}"
    );
    let argument = WindowsUpdaterHelperArgument::activation("x")
        .expect_err("a malformed rendezvous is refused")
        .to_string();
    assert!(
        argument.starts_with("KELD-RUNTIME-019: ") && argument.contains("keld-attempt-<64"),
        "{argument}"
    );
    let declined = WindowsUpdaterHelperLaunchError::Declined.to_string();
    assert!(
        declined.starts_with("KELD-RUNTIME-019: ")
            && declined.contains("Nothing was launched and no protected state changed"),
        "{declined}"
    );
    // A helper may be running without a bound handle: the guidance says how it
    // is made to exit.
    for unbound in [
        WindowsUpdaterHelperLaunchError::NoProcessHandle,
        WindowsUpdaterHelperLaunchError::ProcessIdentity {
            source: std::io::Error::from_raw_os_error(6),
        },
    ] {
        let rendered = unbound.to_string();
        assert!(
            rendered.starts_with("KELD-RUNTIME-019: ")
                && rendered.contains("A helper may already be running: close the bootstrap"),
            "{rendered}"
        );
    }
}

#[test]
#[ignore = "operator row: from a non-elevated prompt with UAC at its default level, run \
            `cargo test -p keld-runtime --test windows_updater_helper_launch -- --ignored \
            --exact --nocapture uac_consent_starts_the_exact_image_elevated_and_retains_its_handle` \
            and choose Yes"]
fn uac_consent_starts_the_exact_image_elevated_and_retains_its_handle() {
    assert!(
        !process_is_elevated(current_process()),
        "run this row from a non-elevated prompt"
    );
    // The launched image is this test executable: libtest reads the one
    // rendezvous argument as a test-name filter, runs no test and exits 0.
    let image = env::current_exe().expect("this test executable");
    let path = WindowsUpdaterHelperPath::new(&image).expect("the test executable path");
    let argument =
        WindowsUpdaterHelperArgument::activation(&rendezvous(LOCATOR)).expect("rendezvous");
    let helper =
        launch_elevated_updater_helper(&path, &argument).expect("choose Yes on the UAC prompt");
    let handle: HANDLE = helper.as_handle().as_raw_handle().cast();

    // SAFETY: `helper` retains `handle` for every call below.
    assert_eq!(unsafe { GetProcessId(handle) }, helper.id());
    assert_eq!(canonical(&image_path(handle)), canonical(&image));
    // SAFETY: as above; the wait is bounded.
    assert_eq!(
        unsafe { WaitForSingleObject(handle, 120_000) },
        WAIT_OBJECT_0
    );
    let mut exit_code = u32::MAX;
    // SAFETY: as above; `exit_code` is writable.
    assert_ne!(unsafe { GetExitCodeProcess(handle, &raw mut exit_code) }, 0);
    assert_eq!(
        exit_code, 0,
        "the elevated test executable ran with its one argument"
    );

    // An elevated process refuses this Medium process a read right that a
    // same-user, non-elevated process grants (negative control below).
    assert_eq!(
        open_for_memory_read(helper.id()).map(drop),
        Err(ERROR_ACCESS_DENIED.cast_signed()),
        "the launched process must be elevated"
    );
    let mut ordinary = Command::new(&image)
        .arg(rendezvous(LOCATOR))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the same image without elevation");
    let ordinary_read = open_for_memory_read(ordinary.id());
    assert!(ordinary.wait().expect("ordinary child").success());
    assert!(
        ordinary_read.is_ok(),
        "negative control: a non-elevated process grants PROCESS_VM_READ, got {ordinary_read:?}"
    );
    println!("UAC_CONSENT pid={} exit={exit_code}", helper.id());
}

#[test]
#[ignore = "operator row: from a non-elevated prompt with UAC at its default level, run \
            `cargo test -p keld-runtime --test windows_updater_helper_launch -- --ignored \
            --exact --nocapture uac_decline_is_the_typed_declined_refusal` and choose No"]
fn uac_decline_is_the_typed_declined_refusal() {
    assert!(
        !process_is_elevated(current_process()),
        "run this row from a non-elevated prompt"
    );
    let image = env::current_exe().expect("this test executable");
    let path = WindowsUpdaterHelperPath::new(&image).expect("the test executable path");
    let error = launch_elevated_updater_helper(&path, &WindowsUpdaterHelperArgument::recovery())
        .expect_err("choose No on the UAC prompt");
    assert!(
        matches!(error, WindowsUpdaterHelperLaunchError::Declined),
        "{error}"
    );
    println!("UAC_DECLINE {error}");
}

fn current_process() -> HANDLE {
    // SAFETY: GetCurrentProcess returns this process's non-owning pseudo-handle.
    unsafe { GetCurrentProcess() }
}

fn process_is_elevated(process: HANDLE) -> bool {
    let mut raw_token: HANDLE = std::ptr::null_mut();
    // SAFETY: `process` is live and `raw_token` is writable.
    assert_ne!(
        unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut raw_token) },
        0,
        "open the process token"
    );
    // SAFETY: OpenProcessToken succeeded, so this is the token's sole owner.
    let token = unsafe { OwnedHandle::from_raw_handle(raw_token.cast()) };
    let mut elevation = TOKEN_ELEVATION::default();
    let mut written = 0_u32;
    let size = u32::try_from(size_of::<TOKEN_ELEVATION>()).expect("TOKEN_ELEVATION size");
    // SAFETY: `token` is live and the output buffer is a writable TOKEN_ELEVATION.
    assert_ne!(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                TokenElevation,
                (&raw mut elevation).cast(),
                size,
                &raw mut written,
            )
        },
        0,
        "query TokenElevation"
    );
    elevation.TokenIsElevated != 0
}

fn image_path(process: HANDLE) -> PathBuf {
    let mut buffer = vec![0_u16; 32_768];
    let mut length = u32::try_from(buffer.len()).expect("buffer length");
    // SAFETY: `process` is live and `buffer` is writable for `length` units.
    assert_ne!(
        unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &raw mut length,
            )
        },
        0,
        "query the launched image path"
    );
    let length = usize::try_from(length).expect("length fits usize");
    PathBuf::from(OsString::from_wide(&buffer[..length]))
}

/// Opens `process_id` for `PROCESS_VM_READ`, or returns the Windows error.
fn open_for_memory_read(process_id: u32) -> Result<OwnedHandle, i32> {
    // SAFETY: OpenProcess dereferences no caller memory.
    let raw = unsafe { OpenProcess(PROCESS_VM_READ, 0, process_id) };
    if raw.is_null() {
        return Err(std::io::Error::last_os_error()
            .raw_os_error()
            .expect("OpenProcess sets a Windows error"));
    }
    // SAFETY: OpenProcess succeeded, so this is the handle's sole owner.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.cast()) })
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", path.display()))
}
