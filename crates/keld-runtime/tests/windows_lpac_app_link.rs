//! Real Windows composition evidence for KEL-210's LPAC/app-link contract.
//!
//! This preserves the existing KEL-101 current-user-only pipe policy. It is a
//! compatibility prerequisite test, not an implementation of LPAC product startup.
//! `AppContainer` and capability count are observed; LPAC opt-out is configuration
//! provenance here, not independent token or containment qualification.

#![cfg(windows)]
#![allow(unsafe_code)] // test-owned Win32 fixture and independent descriptor/token observation
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(clippy::expect_used)] // independent process, token, and byte assertions

use std::env;
use std::ffi::{OsStr, OsString, c_void};
use std::fs::{self, File};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::os::windows::io::{AsHandle as _, AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use keld_ipc::bootstrap::{
    WindowsNamedPipeBootstrapAdmission, WindowsNamedPipeBootstrapCancellation,
    WindowsNamedPipeBootstrapListener, WindowsNamedPipeBootstrapStream,
};
use keld_ipc::link::handshake_client;
use keld_ipc::token::parse_app_link;
use keld_ipc::{BootstrapRejection, BootstrapRejectionObserver};
use keld_runtime::windows_lpac::{
    WindowsLpacChild, WindowsLpacPathAccess, WindowsLpacProfile, WindowsLpacStdio,
};
use sha2::{Digest as _, Sha256};
use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SE_KERNEL_OBJECT,
    SetSecurityInfo,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetTokenInformation,
    LABEL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SECURITY_ATTRIBUTES,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
    PIPE_ACCESS_DUPLEX, READ_CONTROL, WRITE_DAC,
};
use windows_sys::Win32::System::Pipes::{CreateNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

const ENDPOINT_ENV: &str = "KELD_210_TEST_PIPE_ENDPOINT";
const CHILD_TEST: &str = "lpac_pipe_open_child";

#[derive(Default)]
struct Rejections(Mutex<Vec<BootstrapRejection>>);

impl BootstrapRejectionObserver for Rejections {
    fn rejected(&self, rejection: BootstrapRejection) {
        self.0.lock().expect("rejection recorder").push(rejection);
    }
}

#[test]
#[ignore = "subprocess entry; exercised by the non-ignored parent with real OS oracles"]
fn lpac_pipe_open_child() {
    let endpoint = env::var(ENDPOINT_ENV).expect("parent supplies an owned endpoint");
    match WindowsNamedPipeBootstrapStream::connect(&endpoint) {
        Ok(stream) => {
            drop(stream);
            println!("KEL210_PIPE_OPEN allowed=true");
        }
        Err(error) => {
            println!(
                "KEL210_PIPE_OPEN allowed=false kind={:?} raw={:?}",
                error.kind(),
                error.raw_os_error()
            );
        }
    }
}

#[test]
fn current_user_only_pipe_denies_configured_lpac_client_and_preserves_owner_authentication() {
    // Two fresh instances also prove the first observation does not poison a
    // subsequent listener. No change to the production pipe's security descriptor.
    for _ in 0..2 {
        observe_current_policy();
    }
}

fn observe_current_policy() {
    let root = tempfile::tempdir().expect("owned test root");
    println!("KEL210_FIXTURE root={}", root.path().display());
    let runtime = root.path().join("runtime");
    let private = root.path().join("private");
    fs::create_dir(&runtime).expect("runtime directory");
    fs::create_dir(&private).expect("private directory");
    let executable = runtime.join("pipe-client.exe");
    fs::copy(env::current_exe().expect("current test image"), &executable)
        .expect("copy reviewed test image");

    let listener =
        Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("current pipe policy"));
    let app_link = listener.app_link();
    let endpoint = listener.endpoint().to_owned();
    let (server, finished_tx, seen) = start_bootstrap_worker(&listener);
    let (exit, lpac_output) =
        run_lpac_client(root.path(), &runtime, &private, &executable, &endpoint);

    // The same image/API outside LPAC must actually open the very same pipe;
    // an always-failing client or an absent endpoint cannot pass this control.
    let normal_output = run_normal_client(&executable, &endpoint);
    let (name, token) = parse_app_link(&app_link).expect("host-minted link");
    let mut normal = WindowsNamedPipeBootstrapStream::connect(name).expect("owner connects");
    handshake_client(&mut normal, &token).expect("owner consumes authenticated HELLO");
    drop(normal);
    finished_tx.send(()).expect("client completion witness");
    let server_completed = server.finish();

    assert_eq!(exit, 0, "LPAC fixture execution failed: {lpac_output}");
    assert!(
        lpac_output.contains("allowed=false kind=PermissionDenied raw=Some(5)"),
        "current LPAC/pipe policy premise was refuted: {lpac_output}"
    );
    assert!(
        normal_output.contains("KEL210_PIPE_OPEN allowed=true"),
        "same-image normal-token positive control failed: {normal_output}"
    );
    assert!(server_completed, "intended client was not authenticated");
    assert!(
        seen.0
            .lock()
            .expect("rejections")
            .iter()
            .all(|item| *item != BootstrapRejection::HelloAuth),
        "OS-open denial must not be misreported as wrong-token HELLO"
    );
    root.close()
        .expect("all owned fixture resources removed after child reap");
    println!(
        "KEL210_COMPOSITION configured_lpac_open=ERROR_ACCESS_DENIED normal_open=allowed normal_hello=authenticated"
    );
}

fn start_bootstrap_worker(
    listener: &Arc<WindowsNamedPipeBootstrapListener>,
) -> (BootstrapWorker, mpsc::Sender<()>, Arc<Rejections>) {
    let cancellations = listener.cancellation();
    let seen = Arc::new(Rejections::default());
    let (finished_tx, finished_rx) = mpsc::channel();
    let server_listener = Arc::clone(listener);
    let server_seen = Arc::clone(&seen);
    let server = thread::spawn(move || {
        let admitted = server_listener
            .accept_authenticated_until(Instant::now() + Duration::from_secs(20), &*server_seen)
            .expect("bounded bootstrap admission");
        let WindowsNamedPipeBootstrapAdmission::Authenticated(mut stream) = admitted else {
            return false;
        };
        keld_ipc::serve_echo_requests(&mut stream).expect("existing echo session until peer close");
        // Retain the server until the real client consumed HELLO and closed;
        // dropping it immediately could discard a pending reply on Windows.
        let consumed = finished_rx.recv_timeout(Duration::from_secs(5)).is_ok();
        drop(stream);
        consumed
    });

    let server = BootstrapWorker {
        worker: Some(server),
        cancellation: cancellations,
    };
    (server, finished_tx, seen)
}

fn run_lpac_client(
    root: &Path,
    runtime: &Path,
    private: &Path,
    executable: &Path,
    endpoint: &str,
) -> (u32, String) {
    let profile_name = format!(
        "keld-210-{}",
        root.file_name()
            .expect("unique root basename")
            .to_string_lossy()
    );
    let profile =
        WindowsLpacProfile::create(OsStr::new(&profile_name)).expect("fresh LPAC profile");
    run_profile_client(&profile, root, runtime, private, executable, endpoint)
}

fn run_profile_client(
    profile: &WindowsLpacProfile,
    root: &Path,
    runtime: &Path,
    private: &Path,
    executable: &Path,
    endpoint: &str,
) -> (u32, String) {
    let arguments: Vec<OsString> = ["--exact", CHILD_TEST, "--ignored", "--nocapture"]
        .into_iter()
        .map(OsString::from)
        .collect();
    run_profile_program(
        profile, root, runtime, private, executable, endpoint, &arguments,
    )
}

fn run_profile_program(
    profile: &WindowsLpacProfile,
    root: &Path,
    runtime: &Path,
    private: &Path,
    executable: &Path,
    endpoint: &str,
    arguments: &[OsString],
) -> (u32, String) {
    profile
        .grant_path(root, WindowsLpacPathAccess::Traverse)
        .expect("root traverse");
    profile
        .grant_path(runtime, WindowsLpacPathAccess::ReadExecute)
        .expect("runtime ACL");
    profile
        .grant_path(private, WindowsLpacPathAccess::RolePrivate)
        .expect("private output ACL");
    let mut output = tempfile::tempfile_in(private).expect("owned output sink");
    let input = File::open("NUL").expect("null stdin");
    let mut environment = vec![(OsString::from(ENDPOINT_ENV), OsString::from(endpoint))];
    for key in ["SystemRoot", "WINDIR", "USERPROFILE", "LOCALAPPDATA"] {
        if let Some(value) = env::var_os(key) {
            environment.push((OsString::from(key), value));
        }
    }
    environment.push((OsString::from("TEMP"), private.as_os_str().to_owned()));
    environment.push((OsString::from("TMP"), private.as_os_str().to_owned()));
    let child = profile
        .spawn_suspended(
            executable,
            arguments,
            &environment,
            Some(private),
            Some(WindowsLpacStdio {
                stdin: input.as_handle(),
                stdout: output.as_handle(),
                stderr: output.as_handle(),
            }),
            &[],
        )
        .expect("suspended LPAC client");
    let mut child = ReapedChild(child);
    let token = child.0.observe_token().expect("actual AppContainer token");
    assert!(token.is_app_container);
    // Configuration provenance only: observe_token does not query the LPAC flag.
    // This transport prerequisite does not certify All Application Packages opt-out.
    assert!(token.all_application_packages_opt_out_configured);
    assert_eq!(token.capability_count, 0);
    child.0.resume().expect("resume inspected client");
    let exit = child.0.wait(10_000).expect("bounded LPAC client exit");
    output.seek(SeekFrom::Start(0)).expect("rewind output");
    let mut lpac_output = String::new();
    output
        .read_to_string(&mut lpac_output)
        .expect("read LPAC observation");

    (exit, lpac_output)
}

// These guards join/reap on assertion failures as well as the successful path.
struct ReapedChild(WindowsLpacChild);

impl Drop for ReapedChild {
    fn drop(&mut self) {
        if self.0.wait(0).is_ok() {
            return;
        }
        let termination = self.0.terminate(1);
        let reaped = self.0.wait(5_000);
        if let Ok(exit) = &reaped {
            eprintln!("KEL210_CLEANUP child_reaped=true exit={exit}");
        }
        if let Err(error) = &termination {
            eprintln!("owned child termination failed: {error}");
        }
        if let Err(error) = &reaped {
            eprintln!("owned child reap failed: {error}");
        }
        if !thread::panicking() {
            reaped.expect("owned child must be reaped before releasing profile/resources");
        }
    }
}

struct BootstrapWorker {
    worker: Option<thread::JoinHandle<bool>>,
    cancellation: WindowsNamedPipeBootstrapCancellation,
}

impl BootstrapWorker {
    fn finish(mut self) -> bool {
        self.worker
            .take()
            .expect("owned worker")
            .join()
            .expect("bootstrap worker joins")
    }
}

impl Drop for BootstrapWorker {
    fn drop(&mut self) {
        let cancellation = self.cancellation.cancel();
        if let Err(error) = &cancellation {
            eprintln!("owned listener cancellation failed: {error}");
        }
        if let Some(worker) = self.worker.take() {
            let joined = worker.join();
            eprintln!("KEL210_CLEANUP worker_joined=true");
            if joined.is_err() {
                eprintln!("owned bootstrap worker panicked");
            }
            if !thread::panicking() {
                cancellation.expect("listener cancellation");
                joined.expect("cancelled worker joins");
            }
        } else if !thread::panicking() {
            cancellation.expect("idempotent listener cancellation");
        }
    }
}

fn run_normal_client(executable: &Path, endpoint: &str) -> String {
    let mut child = Command::new(executable)
        .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
        .env(ENDPOINT_ENV, endpoint)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("same-image normal-token control");
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().expect("control status").is_none() {
        if Instant::now() >= deadline {
            child.kill().expect("kill only the owned timed-out control");
            child.wait().expect("reap timed-out control");
            break;
        }
        thread::yield_now();
    }
    let output = child.wait_with_output().expect("control output");
    assert!(output.status.success(), "normal control failed");
    String::from_utf8(output.stdout).expect("UTF-8 control observation")
}

#[test]
fn candidate_package_acl_and_integrity_labels_are_independently_qualified() {
    let user = current_user_sid();
    for repetition in 0..2 {
        let root = tempfile::tempdir().expect("owned matrix root");
        let runtime = root.path().join("runtime");
        let private = root.path().join("private");
        fs::create_dir(&runtime).expect("runtime directory");
        fs::create_dir(&private).expect("private directory");
        let executable = runtime.join("pipe-client.exe");
        fs::copy(env::current_exe().expect("current image"), &executable).expect("fixture image");
        let suffix = root
            .path()
            .file_name()
            .expect("unique root")
            .to_string_lossy();
        let intended =
            WindowsLpacProfile::create(OsStr::new(&format!("keld210-intended-{suffix}")))
                .expect("intended profile");
        let wrong = WindowsLpacProfile::create(OsStr::new(&format!("keld210-wrong-{suffix}")))
            .expect("wrong profile");
        let sid = intended.sid_string().expect("intended package SID");
        assert_ne!(sid, wrong.sid_string().expect("wrong package SID"));
        for label in ["ME", "LW"] {
            for grant in [false, true] {
                let package_ace = if grant {
                    format!("(A;;0x12019b;;;{sid})")
                } else {
                    String::new()
                };
                let sddl = format!("D:P(A;;0x12019b;;;{user}){package_ace}S:AI(ML;;NW;;;{label})");
                for client in ["intended", "wrong", "ordinary"] {
                    let (pipe, endpoint) = candidate_pipe(&sddl);
                    let readback = pipe_descriptor(&pipe);
                    assert_eq!(
                        readback, sddl,
                        "actual descriptor must equal this matrix cell"
                    );
                    let output = if client == "ordinary" {
                        run_normal_client(&executable, &endpoint)
                    } else {
                        let profile = if client == "intended" {
                            &intended
                        } else {
                            &wrong
                        };
                        let (exit, output) = run_profile_client(
                            profile,
                            root.path(),
                            &runtime,
                            &private,
                            &executable,
                            &endpoint,
                        );
                        assert_eq!(exit, 0, "profile client failed: {output}");
                        output
                    };
                    let allowed = output.contains("KEL210_PIPE_OPEN allowed=true");
                    let denied = output.contains("allowed=false kind=PermissionDenied raw=Some(5)");
                    println!(
                        "KEL210_MATRIX repetition={repetition} label={label} package_ace={grant} client={client} allowed={allowed} denied={denied} descriptor={readback}"
                    );
                    assert!(allowed || denied, "unexpected client outcome: {output}");
                    // Freeze the measured contract: an exact package grant works
                    // at both labels; an ungranted profile never inherits that grant.
                    if client == "ordinary" || (client == "intended" && grant) {
                        assert!(allowed, "intended or owner positive control");
                    }
                    if client == "wrong" || (client == "intended" && !grant) {
                        assert!(denied, "ungranted package must be denied");
                    }
                    drop(pipe);
                }
            }
        }
        drop(wrong);
        drop(intended);
        root.close()
            .expect("all matrix resources removed after child reap");
    }
}

// This fixture only creates an OS pipe and observes client open; it implements no
// transport driver, handshake, permission parser, or product policy constructor.
fn candidate_pipe(sddl: &str) -> (OwnedHandle, String) {
    let name_owner = WindowsNamedPipeBootstrapListener::bind().expect("host-minted endpoint");
    let endpoint = name_owner.endpoint().to_owned();
    drop(name_owner);
    let descriptor = descriptor_from_sddl(sddl);
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).expect("ABI size"),
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let name: Vec<u16> = endpoint.encode_utf16().chain([0]).collect();
    // SAFETY: live NUL-terminated name and descriptor; correct attributes length;
    // inheritance disabled. No pending I/O is submitted on this fixture handle.
    let raw = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
            PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65_536,
            65_536,
            0,
            &raw const attributes,
        )
    };
    assert_ne!(
        raw,
        INVALID_HANDLE_VALUE,
        "candidate pipe: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: successful CreateNamedPipe returned a fresh owning handle.
    (
        unsafe { OwnedHandle::from_raw_handle(raw.cast()) },
        endpoint,
    )
}

struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: this owner only receives successful LocalAlloc-family API results.
        let remaining = unsafe { LocalFree(self.0) };
        assert!(remaining.is_null(), "release native descriptor allocation");
    }
}

fn descriptor_from_sddl(sddl: &str) -> LocalAllocation {
    let text: Vec<u16> = sddl.encode_utf16().chain([0]).collect();
    let mut descriptor = std::ptr::null_mut();
    // SAFETY: valid NUL-terminated SDDL and writable output; revision 1 is documented.
    assert_ne!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                1,
                &raw mut descriptor,
                std::ptr::null_mut(),
            )
        },
        0,
        "parse candidate SDDL: {}",
        std::io::Error::last_os_error()
    );
    LocalAllocation(descriptor)
}

fn pipe_descriptor(pipe: &OwnedHandle) -> String {
    let mut descriptor = std::ptr::null_mut();
    let information = DACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION;
    // SAFETY: pipe is live; only descriptor output is requested and it is writable.
    assert_eq!(
        unsafe {
            GetSecurityInfo(
                pipe.as_raw_handle().cast(),
                SE_KERNEL_OBJECT,
                information,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut descriptor,
            )
        },
        0,
        "read actual pipe descriptor"
    );
    let descriptor = LocalAllocation(descriptor);
    let mut text = std::ptr::null_mut();
    // SAFETY: valid returned descriptor, writable string output; same information set.
    assert_ne!(
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor.0,
                1,
                information,
                &raw mut text,
                std::ptr::null_mut(),
            )
        },
        0,
        "descriptor text"
    );
    local_utf16_string(&LocalAllocation(text.cast()))
}

fn current_user_sid() -> String {
    let mut raw = std::ptr::null_mut();
    // SAFETY: current-process pseudo-handle is valid; token output is writable.
    assert_ne!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut raw) },
        0,
        "open current user token"
    );
    // SAFETY: the successful call returned a fresh owning token handle.
    let token = unsafe { OwnedHandle::from_raw_handle(raw.cast()) };
    let mut bytes = 0_u32;
    // SAFETY: documented sizing call with null buffer and writable size.
    let _ = unsafe {
        GetTokenInformation(
            token.as_raw_handle().cast(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &raw mut bytes,
        )
    };
    assert!(bytes >= u32::try_from(std::mem::size_of::<TOKEN_USER>()).expect("ABI size"));
    let mut buffer = vec![
        0_usize;
        usize::try_from(bytes)
            .expect("token size")
            .div_ceil(std::mem::size_of::<usize>())
    ];
    // SAFETY: aligned buffer has at least the queried writable size.
    assert_ne!(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle().cast(),
                TokenUser,
                buffer.as_mut_ptr().cast(),
                bytes,
                &raw mut bytes,
            )
        },
        0,
        "read actual user token"
    );
    // SAFETY: successful TokenUser query returned a live aligned TOKEN_USER and SID.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text = std::ptr::null_mut();
    // SAFETY: returned user SID remains live; output receives a new local allocation.
    assert_ne!(
        unsafe { ConvertSidToStringSidW(user.User.Sid, &raw mut text) },
        0,
        "user SID text"
    );
    local_utf16_string(&LocalAllocation(text.cast()))
}

fn local_utf16_string(text: &LocalAllocation) -> String {
    let pointer = text.0.cast::<u16>();
    let mut length = 0;
    // SAFETY: callers pass successful SDK string-conversion results, guaranteed
    // NUL-terminated UTF-16 allocations; ownership outlives scan and conversion.
    unsafe {
        while *pointer.add(length) != 0 {
            length += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(pointer, length)).expect("SDK UTF-16")
    }
}

#[test]
fn exact_package_grant_preserves_bun_hello_and_rejects_wrong_token() {
    let bun_source = env::split_paths(&env::var_os("PATH").expect("Bun PATH"))
        .map(|directory| directory.join("bun.exe"))
        .find(|candidate| candidate.is_file())
        .expect("Bun on PATH");
    let digest = format!(
        "{:x}",
        Sha256::digest(fs::read(&bun_source).expect("Bun artifact bytes"))
    );
    for repetition in 0..2 {
        observe_bun_hello(&bun_source, &digest, repetition);
    }
}

fn observe_bun_hello(bun_source: &Path, digest: &str, repetition: u32) {
    let root = tempfile::tempdir().expect("owned Bun fixture root");
    let runtime = root.path().join("runtime");
    let private = root.path().join("private");
    fs::create_dir(&runtime).expect("runtime directory");
    fs::create_dir(&private).expect("private directory");
    let bun = runtime.join("bun.exe");
    fs::copy(bun_source, &bun).expect("copy actual Bun image");
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(fs::read(&bun).expect("copied Bun bytes"))
        ),
        digest,
        "executed copy must match the recorded Bun artifact"
    );
    let entry = write_bun_client(&runtime);
    let suffix = root
        .path()
        .file_name()
        .expect("unique root")
        .to_string_lossy();
    let profile = WindowsLpacProfile::create(OsStr::new(&format!("keld210-bun-{suffix}")))
        .expect("fresh Bun profile");
    prove_candidate_pre_cancellation(&profile.sid_string().expect("package SID"));
    let listener = Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("real bootstrap"));
    let link = listener.app_link();
    let descriptor = grant_fixture_pipe(&listener, &profile.sid_string().expect("package SID"));
    let (server, finished, seen) = start_bootstrap_worker(&listener);
    let mut wrong = link.clone().into_bytes();
    let last = wrong.last_mut().expect("nonempty link");
    *last = if *last == b'0' { b'1' } else { b'0' };
    let wrong = String::from_utf8(wrong).expect("ASCII link");
    let arguments = [entry.into_os_string()];
    let (wrong_exit, wrong_output) = run_profile_program(
        &profile,
        root.path(),
        &runtime,
        &private,
        &bun,
        &wrong,
        &arguments,
    );
    assert_eq!(
        wrong_exit, 0,
        "wrong-token client execution: {wrong_output}"
    );
    assert!(wrong_output.contains("KEL210_BUN_HELLO authenticated=false"));
    assert!(
        !wrong_output.contains("authenticated=true"),
        "foreign token authenticated"
    );
    assert!(
        seen.0
            .lock()
            .expect("rejections")
            .contains(&BootstrapRejection::HelloAuth),
        "wrong token must reach and fail the existing HELLO verifier"
    );
    let (exit, output) = run_profile_program(
        &profile,
        root.path(),
        &runtime,
        &private,
        &bun,
        &link,
        &arguments,
    );
    assert_eq!(exit, 0, "intended Bun execution: {output}");
    assert!(
        output.contains("KEL210_BUN_HELLO authenticated=true echoes=2"),
        "Bun failed: {output}"
    );
    finished.send(()).expect("Bun consumed HELLO and exited");
    assert!(server.finish(), "bootstrap completed after consumed HELLO");
    drop(listener);
    drop(profile);
    root.close().expect("Bun resource cleanup after reap");
    println!(
        "KEL210_BUN repetition={repetition} sha256={digest} wrong_token=rejected intended=authenticated echoes=2 descriptor={descriptor}"
    );
}

fn write_bun_client(runtime: &Path) -> PathBuf {
    fs::write(
        runtime.join("kipc.ts"),
        include_str!("../../keld-cli/templates/hello/src/kipc.ts"),
    )
    .expect("copy unchanged product client");
    let entry = runtime.join("client.ts");
    fs::write(
        &entry,
        r"import { AppLinkSession } from './kipc.ts';
try {
  const session = await AppLinkSession.connect(process.env.KELD_210_TEST_PIPE_ENDPOINT!);
  try {
    for (const request of [{ message: 'keld-lpac-first', count: 1 }, { message: 'keld-lpac-second', count: 2 }]) {
      const reply = await session.echo(request);
      if (reply.message !== request.message || reply.count !== request.count) throw new Error('echo mismatch');
    }
    console.log(`KEL210_BUN_HELLO authenticated=true echoes=2 version=${Bun.version}`);
  } finally { session.close(); }
} catch (error) {
  console.log('KEL210_BUN_HELLO authenticated=false');
}
",
    )
    .expect("write bounded client observation");
    entry
}

fn grant_fixture_pipe(listener: &WindowsNamedPipeBootstrapListener, package: &str) -> String {
    let name: Vec<u16> = listener.endpoint().encode_utf16().chain([0]).collect();
    // SAFETY: live owned endpoint with NUL terminator; open only owner ACL rights.
    // This setup connection is closed before the real bootstrap worker starts.
    let raw = unsafe {
        CreateFileW(
            name.as_ptr(),
            READ_CONTROL | WRITE_DAC,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(
        raw,
        INVALID_HANDLE_VALUE,
        "owned ACL handle: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: successful CreateFile returns one new owning handle.
    let pipe = unsafe { OwnedHandle::from_raw_handle(raw.cast()) };
    let before = pipe_descriptor(&pipe);
    let current = format!("D:P(A;;0x12019b;;;{})", current_user_sid());
    assert!(before.starts_with(&current));
    assert_eq!(
        before.matches("(A;").count(),
        1,
        "default still has one user ACE"
    );
    let expected = before.replacen(&current, &format!("{current}(A;;0x12019b;;;{package})"), 1);
    let descriptor = descriptor_from_sddl(&expected);
    let mut acl = std::ptr::null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    // SAFETY: descriptor is live and all ACL output slots are writable.
    assert_ne!(
        unsafe {
            GetSecurityDescriptorDacl(
                descriptor.0,
                &raw mut present,
                &raw mut acl,
                &raw mut defaulted,
            )
        },
        0,
        "candidate DACL extraction"
    );
    assert_eq!(present, 1);
    assert!(!acl.is_null(), "never apply a null DACL");
    // SAFETY: live owner handle and non-null ACL from the live descriptor; only
    // this test-owned pipe's DACL changes. Label and other security fields are untouched.
    assert_eq!(
        unsafe {
            SetSecurityInfo(
                pipe.as_raw_handle().cast(),
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                acl,
                std::ptr::null(),
            )
        },
        0,
        "set test-only exact package policy"
    );
    let actual = pipe_descriptor(&pipe);
    assert_eq!(actual, expected, "DACL exact and original label unchanged");
    actual
}

fn prove_candidate_pre_cancellation(package: &str) {
    let listener =
        Arc::new(WindowsNamedPipeBootstrapListener::bind().expect("cancellation fixture"));
    grant_fixture_pipe(&listener, package);
    listener
        .cancellation()
        .cancel()
        .expect("pre-cancel candidate admission");
    let started = Instant::now();
    let (server, _finished, _seen) = start_bootstrap_worker(&listener);
    assert!(
        !server.finish(),
        "cancelled admission must not authenticate"
    );
    // The admission deadline is 20 seconds. A no-op cancellation must fail this
    // kill-switch oracle rather than pass by eventually timing out.
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "cancellation waited for admission timeout"
    );
    assert!(
        WindowsNamedPipeBootstrapStream::connect(listener.endpoint()).is_err(),
        "cancelled endpoint must be closed"
    );
}
