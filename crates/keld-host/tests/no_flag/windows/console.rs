//! Isolated-console selector, readiness, timeout and native broadcast observations.

use crate::support::control::{
    accept_control_until, accept_ready_generation_with_descendant, try_read_control_line,
};
use crate::support::process::{
    assert_process_signaled, open_process_for_wait, wait_child, wait_for_child_process,
    wait_for_cleanup_sentinel, wait_for_process_signal,
};
use crate::support::product::ProductFixture;
use crate::support::product_cycle::run_product_cycle;
use crate::support::renderer::{expect_renderer_beacon, spawn_renderer_beacon};
use crate::support::window::wait_for_host_window;
use crate::{
    DARK_BG, PRODUCT_DEADLINE, PRODUCT_TITLE, prepare_keld_dev_helper, wait_for_dev_stage_count,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::process::{Command, Stdio};
use std::time::Instant;
use std::{env, fs};
use windows_sys::Win32::Foundation::WAIT_TIMEOUT;
use windows_sys::Win32::System::Threading::GetExitCodeProcess;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
    fn GenerateConsoleCtrlEvent(event: u32, process_group: u32) -> i32;
    fn GetConsoleProcessList(processes: *mut u32, capacity: u32) -> u32;
}

#[test]
fn isolated_console_rejects_a_missing_exact_selector() {
    let error = run_isolated_console_case("kel271_known_absent_console_selector", None)
        .expect_err("a successful zero-test child must not satisfy console acceptance");
    assert!(
        error.contains("isolated console missing completed Ctrl+C/relaunch observations"),
        "an unrelated subprocess failure is not the selector regression: {error}"
    );
    assert!(error.contains("running 0 tests"), "{error}");
}

pub(crate) fn accept_console_timeout_readiness(
    listener: &TcpListener,
    controls: &mut Vec<(String, BufReader<TcpStream>)>,
    processes: &mut Vec<(String, u32, OwnedHandle)>,
) -> u32 {
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    let mut launcher_pid = None;
    for _ in 0..4 {
        let stream = accept_control_until(listener, None, deadline);
        let mut reader = BufReader::new(stream);
        let record = try_read_control_line(&mut reader, deadline).expect("timeout readiness");
        let (role, pid) = record.split_once(' ').expect("role and exact native PID");
        assert!(matches!(
            role,
            "launcher" | "observer" | "direct" | "descendant"
        ));
        assert!(!controls.iter().any(|(seen, _)| seen == role), "{record}");
        let pid = pid.parse::<u32>().expect("numeric timeout fixture PID");
        println!("KELD_CONSOLE_TIMEOUT_READY role={role} pid={pid}");
        if role == "launcher" {
            launcher_pid = Some(pid);
        } else {
            let process = open_process_for_wait(pid, false);
            assert_eq!(
                wait_for_process_signal(&process, 0),
                WAIT_TIMEOUT,
                "{record}"
            );
            processes.push((role.to_owned(), pid, process));
        }
        controls.push((role.to_owned(), reader));
    }
    launcher_pid.expect("ready timeout launcher")
}

pub(crate) fn run_console_timeout_fixture(role: &str, port: u16) {
    let next = match role {
        "observer" => Some("direct"),
        "direct" => Some("descendant"),
        "descendant" => None,
        _ => panic!("unknown console timeout fixture role: {role}"),
    };
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, PRODUCT_DEADLINE)
        .expect("connect timeout fixture readiness");
    stream
        .set_write_timeout(Some(PRODUCT_DEADLINE))
        .expect("readiness write deadline");
    writeln!(stream, "{role} {}", std::process::id()).expect("publish exact fixture PID");
    stream.flush().expect("flush timeout fixture readiness");
    let mut child = next.map(|next| {
        Command::new(env::current_exe().expect("timeout fixture executable"))
            .args([
                "isolated_console_timeout_reaps_ready_descendants",
                "--exact",
                "--nocapture",
            ])
            .env("KELD_T4_CONSOLE_TIMEOUT_ROLE", next)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn timeout fixture descendant")
    });
    // The surviving controller owns this socket lease. It stays open through all
    // death assertions; EOF is solely emergency cleanup after a failed observation.
    let mut released = String::new();
    BufReader::new(stream)
        .read_line(&mut released)
        .expect("controller release or EOF");
    if let Some(child) = child.as_mut() {
        let status = wait_child(child, Instant::now() + PRODUCT_DEADLINE);
        assert!(status.success(), "timeout fixture cleanup child: {status}");
    }
}

pub(crate) fn run_isolated_console_case(
    selector: &str,
    timeout_probe: Option<u16>,
) -> Result<String, String> {
    let capture = tempfile::tempdir().expect("console test captures");
    let stdout = capture.path().join("stdout");
    let stderr = capture.path().join("stderr");
    let script = r"
$ErrorActionPreference='Stop'
$p=Start-Process -FilePath $env:KELD_T4_CONSOLE_EXE -ArgumentList @($env:KELD_T4_CONSOLE_SELECTOR,'--exact','--nocapture') -WindowStyle Hidden -RedirectStandardOutput $env:KELD_T4_CONSOLE_STDOUT -RedirectStandardError $env:KELD_T4_CONSOLE_STDERR -PassThru
$null=$p.Handle
if ($env:KELD_T4_CONSOLE_TIMEOUT_PORT) {
  $control=New-Object System.Net.Sockets.TcpClient
  try {
    $control.Connect('127.0.0.1',[int]$env:KELD_T4_CONSOLE_TIMEOUT_PORT)
    $stream=$control.GetStream()
    $stream.ReadTimeout=[int]$env:KELD_T4_CONSOLE_CONTROL_MS
    $stream.WriteTimeout=[int]$env:KELD_T4_CONSOLE_CONTROL_MS
    $writer=New-Object System.IO.StreamWriter($stream)
    $writer.NewLine=[string][char]10
    $writer.AutoFlush=$true
    $writer.WriteLine('launcher '+$p.Id)
    $reader=New-Object System.IO.StreamReader($stream)
    if ($reader.ReadLine() -ne 'TIMEOUT') { throw 'missing timeout fixture command' }
    $finished=$p.WaitForExit(0)
  } catch {
    [Console]::Error.WriteLine('timeout fixture control failed: '+$_)
    $finished=$false
  } finally {
    $control.Dispose()
  }
} else {
  $finished=$p.WaitForExit(90000)
}
if (!$finished) {
  [Console]::Error.WriteLine('KELD_CONSOLE_WAIT_EXPIRED pid='+$p.Id)
  $p.Kill()
  if (!$p.WaitForExit(10000)) { throw 'isolated console did not exit after termination' }
  [Console]::Error.WriteLine('KELD_CONSOLE_TIMEOUT_REAPED pid='+$p.Id+' exit_code='+$p.ExitCode)
  throw 'isolated console regression timed out'
}
if ($null -eq $p.ExitCode) { throw 'missing isolated console exit status' }
exit $p.ExitCode
";
    let mut command = Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("KELD_T4_CONSOLE_CASE", "1")
        .env("KELD_T4_CONSOLE_SELECTOR", selector)
        .env("KELD_T4_CONSOLE_PARENT", std::process::id().to_string())
        .env(
            "KELD_T4_CONSOLE_EXE",
            std::env::current_exe().expect("test executable"),
        )
        .env("KELD_T4_CONSOLE_STDOUT", &stdout)
        .env("KELD_T4_CONSOLE_STDERR", &stderr)
        .env(
            "KELD_T4_CONSOLE_CONTROL_MS",
            PRODUCT_DEADLINE.as_millis().to_string(),
        )
        .env_remove("KELD_T4_CONSOLE_TIMEOUT_PORT")
        .env_remove("KELD_T4_CONSOLE_TIMEOUT_ROLE");
    if let Some(port) = timeout_probe {
        command.env("KELD_T4_CONSOLE_TIMEOUT_PORT", port.to_string());
    }
    let output = command.output().expect("start isolated console regression");
    let stdout = fs::read_to_string(stdout).expect("console stdout");
    let stderr = fs::read_to_string(stderr).expect("console stderr");
    if !output.status.success() {
        return Err(format!(
            "isolated console failed: {}\n{stdout}\n{stderr}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    // Libtest also exits successfully when --exact selects zero tests. Require
    // the existing native observation and healthy-relaunch effects as well.
    if !stdout
        .lines()
        .any(|line| line.starts_with("KELD_WINDOWS_CTRL_C cli="))
        || !stdout
            .lines()
            .any(|line| line.starts_with("KELD_WINDOWS_CTRL_C_RELAUNCH host="))
    {
        return Err(format!(
            "isolated console missing completed Ctrl+C/relaunch observations: {}\n{stdout}\n{stderr}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(stdout)
}

pub(crate) fn run_console_ctrl_c_case() {
    // SAFETY: this disposable console observer may inherit nextest's Ctrl+C
    // ignore attribute. Establish the ordinary terminal disposition before the
    // CLI inherits it; null changes the attribute without installing a callback.
    assert_ne!(unsafe { SetConsoleCtrlHandler(None, 0) }, 0);
    let fixture = ProductFixture::new();
    let control = TcpListener::bind(("127.0.0.1", 0)).expect("console control");
    let beacon_listener = TcpListener::bind(("127.0.0.1", 0)).expect("console beacon");
    let beacon_port = beacon_listener.local_addr().expect("beacon address").port();
    let beacon = spawn_renderer_beacon(beacon_listener);
    fs::write(fixture.project.join("index.html"), format!(
        "<!doctype html>{DARK_BG}<title>{PRODUCT_TITLE}</title><img src=\"http://127.0.0.1:{beacon_port}/ready.png\">\n"
    )).expect("console renderer");
    let helper = prepare_keld_dev_helper(&fixture);
    let mut cli = Command::new(helper)
        .args(["keld_dev_windows_helper", "--exact", "--nocapture"])
        .env("KELD_T4_HELPER_PROJECT", &fixture.project)
        .env(
            "KELD_T1B_CONTROL",
            control
                .local_addr()
                .expect("control address")
                .port()
                .to_string(),
        )
        .env("KELD_T4_JOB_DESCENDANT", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("console CLI");
    let observation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let host_pid =
            wait_for_child_process(cli.id(), "keld-host.exe", Instant::now() + PRODUCT_DEADLINE);
        let sentinel_pid = wait_for_cleanup_sentinel(cli.id(), Instant::now() + PRODUCT_DEADLINE);
        let (reader, _writer, bun_pid, _, descendant_pid) =
            accept_ready_generation_with_descendant(&control, &mut cli);
        expect_renderer_beacon(beacon, "console rendered state");
        let _window = wait_for_host_window(host_pid, Instant::now() + PRODUCT_DEADLINE);
        let host = open_process_for_wait(host_pid, false);
        let bun = open_process_for_wait(bun_pid, false);
        let descendant = open_process_for_wait(descendant_pid, false);
        let sentinel = open_process_for_wait(sentinel_pid, false);
        assert_console_broadcast_scope(cli.id(), host_pid);

        // SAFETY: null selects the documented per-process ignore attribute. The CLI
        // already inherited the enabled disposition; only this disposable observer
        // changes. No callback or borrowed pointer crosses the call.
        assert_ne!(unsafe { SetConsoleCtrlHandler(None, 1) }, 0);
        // SAFETY: CTRL_C_EVENT (0), group 0 broadcasts to this isolated console.
        // The parent test runner is attached to a different console.
        assert_ne!(unsafe { GenerateConsoleCtrlEvent(0, 0) }, 0);
        let cli_status = wait_child(&mut cli, Instant::now() + PRODUCT_DEADLINE);
        assert_process_signaled(&host, "Ctrl+C host");
        assert_process_signaled(&bun, "Ctrl+C Bun");
        assert_process_signaled(&descendant, "Ctrl+C descendant");
        assert_process_signaled(&sentinel, "Ctrl+C sentinel");
        wait_for_dev_stage_count(&fixture.project, 0, Instant::now() + PRODUCT_DEADLINE);
        drop(reader);
        let mut host_status = 0;
        // SAFETY: the retained, signaled process handle has query access and the
        // output points to a live u32 for the duration of the call.
        assert_ne!(
            unsafe { GetExitCodeProcess(host.as_raw_handle().cast(), &raw mut host_status) },
            0
        );
        let mut stdout = String::new();
        let mut stderr = String::new();
        cli.stdout
            .take()
            .expect("console stdout")
            .read_to_string(&mut stdout)
            .expect("read console stdout");
        cli.stderr
            .take()
            .expect("console stderr")
            .read_to_string(&mut stderr)
            .expect("read console stderr");
        println!(
            "KELD_WINDOWS_CTRL_C cli={} host={host_pid} bun={bun_pid} descendant={descendant_pid} sentinel={sentinel_pid} cli_status={cli_status} host_status={host_status} stages=0 stdout={stdout:?} stderr={stderr:?}",
            cli.id()
        );
        assert_eq!(
            cli_status.code(),
            Some(-1_073_741_510),
            "native CLI interrupt classification"
        );
        assert_eq!(
            host_status, 0,
            "Ctrl+C must close the CLI lease and preserve the host shutdown tail; stdout={stdout:?}, stderr={stderr:?}"
        );
        assert!(
            stdout.contains("KEL96_T2_FORWARDED_LOG"),
            "host capture lost: {stdout:?}"
        );
        let relaunched = run_product_cycle(&fixture, "post-ctrl-c");
        println!(
            "KELD_WINDOWS_CTRL_C_RELAUNCH host={} bun={}",
            relaunched.host_pid, relaunched.bun_pid
        );
    }));
    if let Err(failure) = observation {
        // Keep precondition failures from orphaning the terminal-facing helper.
        // Killing this exact fixture child closes the existing host lease.
        let _ = cli.kill();
        let _ = cli.wait();
        std::panic::resume_unwind(failure);
    }
}

fn assert_console_broadcast_scope(cli_pid: u32, host_pid: u32) {
    let mut processes = [0_u32; 64];
    // SAFETY: the writable array holds exactly the advertised number of PIDs.
    let count = unsafe { GetConsoleProcessList(processes.as_mut_ptr(), 64) };
    assert!(
        count > 0 && count <= 64,
        "console census failed or exceeded capacity: {count}"
    );
    let attached = &processes[..usize::try_from(count).expect("bounded console count")];
    let parent: u32 = std::env::var("KELD_T4_CONSOLE_PARENT")
        .expect("outer test process identity")
        .parse()
        .expect("outer test PID");
    assert!(
        !attached.contains(&parent),
        "broadcast would reach outer runner: {attached:?}"
    );
    assert!(
        attached.contains(&std::process::id()),
        "observer absent: {attached:?}"
    );
    assert!(attached.contains(&cli_pid), "CLI absent: {attached:?}");
    assert!(attached.contains(&host_pid), "host absent: {attached:?}");
    println!("KELD_WINDOWS_CTRL_C_CONSOLE outer={parent} attached={attached:?}");
}
