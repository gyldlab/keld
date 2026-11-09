//! Signed state-run fixture, ordered host/Bun guard and per-store report owner.
//!
//! Each case installs its own renderer: Windows persistent profiles boot only from an
//! authenticated installed package (KEL-254 A3 AC2), whose tree is immutable.

use super::control::{
    accept_control_or_host_failure, parse_descendant_pid, read_control_line,
    read_control_line_or_host_failure, read_ready_or_host_failure,
};
use super::installed::InstalledApp;
use super::process::process_exists;
use super::product::ProductFixture;
use super::profile_observation::ProfileStateObservation;
use super::profile_response::state_redirect_html;
use super::profile_server::ProfileStateServer;
use super::signed_process::SignedStateProcessGuard;
use super::window::wait_for_host_window;
use crate::{PRODUCT_DEADLINE, startup_kill_switch, ui_end_kill_switch};
use serde_json::Value;
use std::fs;
use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub(crate) struct SignedProfileStateCase<'a> {
    pub(crate) name: &'a str,
    /// An operator-signed host that embeds its expected app identity.
    pub(crate) host: &'a std::ffi::OsStr,
    /// The keld-update unit-test executable holding the per-user install fixture.
    pub(crate) installer: &'a std::ffi::OsStr,
    pub(crate) before: &'a str,
    pub(crate) after: &'a str,
}

pub(crate) fn run_signed_profile_state_case(
    fixture: &ProductFixture,
    control_listener: &TcpListener,
    state_server: &ProfileStateServer,
    run_nonce: &str,
    case: &SignedProfileStateCase<'_>,
) {
    // One kill switch from launch to the renderer's state report, which the initial
    // navigation makes only after the host's WebView2 startup.
    let deadline = Instant::now() + startup_kill_switch();
    state_server.expect_case(case.name, deadline);
    let run = SignedProfileStateRun::start(
        fixture,
        control_listener,
        state_server.address(),
        run_nonce,
        case,
        deadline,
    );
    let observed = state_server.wait_for_case(case.name, deadline);
    record_profile_state_case(
        &observed,
        case,
        run_nonce,
        state_server.address(),
        run.process.host_pid(),
        run.bun_pid,
    );
    run.finish();
}

pub(crate) fn record_profile_state_case(
    observed: &ProfileStateObservation,
    case: &SignedProfileStateCase<'_>,
    run_nonce: &str,
    address: SocketAddr,
    host_pid: u32,
    bun_pid: u32,
) {
    assert_eq!(observed.nonce, run_nonce, "{} run nonce", case.name);
    observed
        .before
        .assert_value(case.before, case.name, "before");
    observed.after.assert_value(case.after, case.name, "after");
    println!(
        "KELD_KEL135_STORAGE {}",
        serde_json::json!({
            "case": case.name, "nonce": run_nonce, "origin": format!("http://{address}"),
            "before": observed.before.json(), "after": observed.after.json(),
            "host_pid": host_pid, "bun_pid": bun_pid,
        })
    );
}

pub(crate) struct SignedProfileStateRun {
    // Drop the process guard before removing the installation it runs from.
    pub(crate) process: SignedStateProcessGuard,
    reader: BufReader<TcpStream>,
    pub(crate) bun_pid: u32,
    case_name: String,
    installed: InstalledApp,
}

impl SignedProfileStateRun {
    pub(crate) fn installed_host(&self) -> &std::path::Path {
        self.installed.host()
    }

    pub(crate) fn start(
        fixture: &ProductFixture,
        control_listener: &TcpListener,
        address: SocketAddr,
        run_nonce: &str,
        case: &SignedProfileStateCase<'_>,
        deadline: Instant,
    ) -> Self {
        Self::start_with_profile_root(
            fixture,
            control_listener,
            address,
            run_nonce,
            case,
            deadline,
            None,
        )
    }

    pub(crate) fn start_with_profile_root(
        fixture: &ProductFixture,
        control_listener: &TcpListener,
        address: SocketAddr,
        run_nonce: &str,
        case: &SignedProfileStateCase<'_>,
        deadline: Instant,
        profile_root: Option<&std::path::Path>,
    ) -> Self {
        fs::write(
            fixture.project.join("index.html"),
            state_redirect_html(address, case.name, run_nonce, case.after),
        )
        .expect("write state renderer");
        let installed = InstalledApp::install(&fixture.project, case.host, case.installer);
        if let Some(root) = profile_root {
            assert_profile_test_root_feature(installed.host(), root);
        }
        let control_port = control_listener
            .local_addr()
            .expect("state control address")
            .port();
        let mut command = installed.command();
        command.env("KELD_T1B_CONTROL", control_port.to_string());
        if let Some(root) = profile_root {
            command
                .env("KELD_PROFILE_TEST_ROOT", root)
                .env("TEMP", root.parent().expect("test root parent"))
                .env("TMP", root.parent().expect("test root parent"));
        } else {
            command.env_remove("KELD_PROFILE_TEST_ROOT");
        }
        let child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch the installed state host without KELD_DEV_LEASE");
        let mut process = SignedStateProcessGuard::new(child);
        let control =
            accept_control_or_host_failure(control_listener, process.child_mut(), deadline);
        control
            .set_read_timeout(Some(PRODUCT_DEADLINE))
            .expect("state control timeout");
        let mut reader = BufReader::new(control);
        let hello = read_control_line(&mut reader);
        let mut hello_fields = hello.split_whitespace();
        assert_eq!(hello_fields.next(), Some("HELLO"), "{hello}");
        let bun_pid = hello_fields
            .next()
            .expect("state Bun PID")
            .parse::<u32>()
            .expect("state numeric Bun PID");
        let _app_link = hello_fields.next().expect("state app link");
        assert!(hello_fields.next().is_none(), "{hello}");
        process.observe_bun(bun_pid);
        assert_eq!(parse_descendant_pid(&read_control_line(&mut reader)), 0);
        assert_eq!(
            read_ready_or_host_failure(&mut reader, process.child_mut(), "state READY"),
            "READY"
        );
        assert_eq!(
            read_control_line_or_host_failure(&mut reader, process.child_mut(), "state ECHO1"),
            "ECHO1"
        );
        assert_eq!(
            read_control_line_or_host_failure(&mut reader, process.child_mut(), "state ECHO2"),
            "ECHO2"
        );

        Self {
            process,
            reader,
            bun_pid,
            case_name: case.name.to_owned(),
            installed,
        }
    }

    /// Quits the run and reaps its host without censoring the browser release barrier.
    pub(crate) fn finish(mut self) {
        // The window and each control read keep the per-step product deadline. The
        // host reap starts after LINK_EOF and includes the UI end, so it reads that
        // rule's one owner (`ui_end_kill_switch`), which stays below the nextest
        // per-test terminate timeout (slow-timeout period x terminate-after,
        // .config/nextest.toml).
        let host_pid = self.process.host_pid();
        let _window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
        self.reader
            .get_mut()
            .write_all(b"QUIT\n")
            .expect("state host Quit");
        self.reader
            .get_mut()
            .flush()
            .expect("flush state host Quit");
        assert_eq!(read_control_line(&mut self.reader), "QUIT_REPLY");
        assert_eq!(read_control_line(&mut self.reader), "LINK_EOF");
        let status = self.process.wait(Instant::now() + ui_end_kill_switch());
        assert!(
            status.success(),
            "{} host exited with {status}",
            self.case_name
        );
        assert!(
            !process_exists(self.bun_pid),
            "{} Bun survived exit",
            self.case_name
        );
    }
}

/// Run-owned directory selected only by the debug signed-host profile fixture.
pub(crate) struct ProfileTestRoot {
    temp: tempfile::TempDir,
    root: PathBuf,
}

impl ProfileTestRoot {
    pub(crate) fn new() -> Self {
        let temp = tempfile::Builder::new()
            .prefix("keld135-profile-")
            .tempdir()
            .expect("create run-owned directory in OS temp");
        let root = temp.path().join("profile-test-root");
        fs::create_dir(&root).expect("create isolated profile root");
        println!(
            "KELD_KEL135_TEST_PROFILE_ROOT {}",
            serde_json::json!({"root": root, "os_temp": temp.path()})
        );
        Self { temp, root }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.root
    }

    pub(crate) fn remove(self) {
        fs::remove_dir_all(&self.root).expect("remove run-owned test profiles");
        self.temp.close().expect("remove run-owned temp directory");
    }
}

pub(crate) fn assert_profile_test_root_feature(host: &Path, root: &Path) {
    let temp = root.parent().expect("test profile root has a temp parent");
    let output = Command::new(host)
        .arg("--keld-profile-test-root-probe-v1")
        .env("KELD_PROFILE_TEST_ROOT", root)
        .env("TEMP", temp)
        .env("TMP", temp)
        .output()
        .expect("probe signed host's debug profile-root feature");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "signed installed host lacks the profile-root fixture feature: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    assert!(
        stdout.lines().any(|line| {
            line.strip_prefix("KELD_PROFILE_TEST_ROOT_FEATURE ")
                .is_some_and(|selected| same_windows_path(Path::new(selected), root))
        }),
        "signed host did not attest the requested profile root: {stdout}"
    );
}

pub(crate) fn user_data_folder(root: &Path, namespace: &str) -> PathBuf {
    root.join("Keld")
        .join("profiles")
        .join("v1")
        .join(namespace)
        .join("webview2")
}

pub(crate) fn actual_webview_process(
    host_pid: u32,
    expected_host: &Path,
    expected_udf: &Path,
    deadline: Instant,
) -> Value {
    const PROCESS_QUERY: &str = r#"
$ErrorActionPreference = 'Stop'
$hostPid = [int]$env:KELD_PROFILE_HOST_PID
$expected = [IO.Path]::GetFullPath($env:KELD_EXPECTED_UDF).TrimEnd('\')
$all = @(Get-CimInstance Win32_Process)
$observedHost = $all | Where-Object { [int]$_.ProcessId -eq $hostPid } | Select-Object -First 1
if ($null -eq $observedHost) { [Console]::WriteLine('{}'); exit 0 }
$candidates = @()
foreach ($process in $all) {
  if ($process.Name -ne 'msedgewebview2.exe' -or [string]::IsNullOrEmpty($process.CommandLine)) { continue }
  if ([int]$process.ParentProcessId -ne $hostPid) { continue }
  $match = [regex]::Match($process.CommandLine, '--user-data-dir(?:=|\s+)(?:"([^"]+)"|(\S+))')
  $actual = if ($match.Groups[1].Success) { $match.Groups[1].Value } elseif ($match.Groups[2].Success) { $match.Groups[2].Value } else { '' }
  $candidates += @{ browser_pid = [int]$process.ProcessId; parent_pid = [int]$process.ParentProcessId; user_data_folder = $actual }
  $folder = if ([string]::IsNullOrEmpty($actual)) { '' } else { [IO.Path]::GetFullPath($actual).TrimEnd('\') }
  $nested = [IO.Path]::GetFullPath((Join-Path $expected 'EBWebView')).TrimEnd('\')
  if ([string]::IsNullOrEmpty($folder) -or ($folder -ine $expected -and $folder -ine $nested)) { continue }
  [Console]::WriteLine((@{ host_image = $observedHost.ExecutablePath; browser_pid = [int]$process.ProcessId; parent_pid = [int]$process.ParentProcessId; user_data_folder = $actual; candidates = $candidates } | ConvertTo-Json -Compress))
  exit 0
}
[Console]::WriteLine((@{ host_image = $observedHost.ExecutablePath; candidates = $candidates } | ConvertTo-Json -Compress))
"#;
    loop {
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", PROCESS_QUERY])
            .env("KELD_PROFILE_HOST_PID", host_pid.to_string())
            .env("KELD_EXPECTED_UDF", expected_udf)
            .output()
            .expect("query actual WebView2 process and command line");
        assert!(
            output.status.success(),
            "WebView2 process query failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("process query output UTF-8");
        let evidence: Value = serde_json::from_str(stdout.trim()).expect("process query JSON");
        if let (Some(host_image), Some(browser_pid), Some(parent_pid), Some(folder)) = (
            evidence["host_image"].as_str(),
            evidence["browser_pid"].as_u64(),
            evidence["parent_pid"].as_u64(),
            evidence["user_data_folder"].as_str(),
        ) {
            assert!(browser_pid > 0, "invalid browser PID: {evidence}");
            assert_eq!(
                parent_pid,
                u64::from(host_pid),
                "wrong WebView2 parent: {evidence}"
            );
            assert!(
                same_windows_path(Path::new(host_image), expected_host),
                "observed host image differs from installed signed host: {evidence}"
            );
            assert!(
                same_windows_path(Path::new(folder), expected_udf)
                    || same_windows_path(Path::new(folder), &expected_udf.join("EBWebView")),
                "observed WebView2 UDF differs from expected path: {evidence}"
            );
            return evidence;
        }
        assert!(
            Instant::now() < deadline,
            "actual WebView2 process using the selected UDF did not appear: {evidence}"
        );
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

fn same_windows_path(left: &Path, right: &Path) -> bool {
    fn normalize(path: &Path) -> String {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let text = path.to_string_lossy();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        text.trim_end_matches(['\\', '/']).to_ascii_lowercase()
    }
    normalize(left) == normalize(right)
}
