//! Native wait regressions; the outer process retains emergency cleanup authority.

use crate::support::EVENT_DEADLINE;
use crate::support::control::{accept_before, wait_child_output};
use crate::support::dev_cycle::ShippingLaunchCleanup;
use crate::support::process::{await_process_gone, process_exists, signal_process_group};
use crate::support::product::LiveCycle;
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const HELPER: &str = "wait_capture::wait_fixture";
const MODE: &str = "KELD_WAIT_REGRESSION_MODE";

#[test]
fn live_wait_captures_both_pipe_sized_bursts() {
    run_isolated("burst");
}

#[test]
fn live_wait_timeout_reaps_before_unwinding() {
    run_isolated("timeout");
}

#[test]
fn live_wait_observer_panic_reaps_before_unwinding() {
    run_isolated("panic");
}

#[test]
fn inherited_writer_cannot_hold_capture_past_deadline() {
    run_isolated("inherited");
}

fn run_isolated(mode: &str) {
    let root = tempfile::tempdir().expect("isolated wait regression root");
    let mut stdout = tempfile::tempfile().expect("outer stdout capture");
    let mut stderr = tempfile::tempfile().expect("outer stderr capture");
    let child = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", HELPER, "--ignored", "--nocapture"])
        .env(MODE, mode)
        .env("KELD_WAIT_REGRESSION_ROOT", root.path())
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().expect("outer stdout writer"))
        .stderr(stderr.try_clone().expect("outer stderr writer"))
        .process_group(0)
        .spawn()
        .expect("isolated wait regression child");
    let pid = child.id();
    let mut cleanup = ShippingLaunchCleanup::new(child);
    cleanup.host_group = Some(pid);
    let deadline = Instant::now() + EVENT_DEADLINE + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = cleanup
            .cli
            .as_mut()
            .expect("outer child")
            .try_wait()
            .expect("outer exit")
        {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::yield_now();
    };
    // This is emergency cleanup only. The child records its own cleanup oracle
    // before reporting success, so this cannot turn a leaked subject into a pass.
    let _ = signal_process_group("-KILL", pid);
    drop(cleanup);
    let mut diagnostics = String::new();
    for capture in [&mut stdout, &mut stderr] {
        capture.rewind().expect("rewind outer diagnostics");
        capture
            .read_to_string(&mut diagnostics)
            .expect("outer diagnostics");
    }
    assert_eq!(
        fs::read(root.path().join("entered")).expect("exact helper was selected"),
        mode.as_bytes()
    );
    assert!(
        status.is_some_and(|status| status.success()),
        "{mode}: {status:?}\n{diagnostics}"
    );
    assert_eq!(
        fs::read(root.path().join("passed")).expect("child completed independent oracles"),
        b"passed"
    );
}

#[test]
#[ignore = "private subprocess endpoint of bounded native wait regressions"]
fn wait_fixture() {
    let mode = std::env::var(MODE).expect("explicit wait regression mode");
    let root =
        PathBuf::from(std::env::var_os("KELD_WAIT_REGRESSION_ROOT").expect("regression root"));
    fs::write(root.join("entered"), &mode).expect("exact helper selection witness");
    match mode.as_str() {
        "burst" => burst(),
        "timeout" | "panic" => failed_wait(&mode),
        "inherited" => inherited_writer(),
        _ => panic!("unknown wait regression mode: {mode}"),
    }
    // Every failure path must leave the same wait usable by a healthy child.
    let output = live_cycle(
        Command::new("/usr/bin/printf")
            .arg("FOLLOWUP")
            .stdout(Stdio::piped())
            .spawn()
            .expect("follow-up child"),
    )
    .wait_host();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"FOLLOWUP");
    assert!(output.stderr.is_empty());
    fs::write(root.join("passed"), b"passed").expect("completed regression witness");
}

fn live_cycle(mut child: Child) -> LiveCycle {
    let (reader, writer) = UnixStream::pair().expect("unused live-cycle controls");
    LiveCycle {
        host_pid: child.id(),
        dev_lease_writer: child.stdin.take(),
        host: Some(child),
        guardian_pid: 0,
        bun_pid: 0,
        descendant_pid: 0,
        session_dir: PathBuf::new(),
        control_reader: BufReader::new(reader),
        control_writer: writer,
        beacon: None,
        presentation: None,
        group_gone: true,
    }
}

fn burst() {
    let child = Command::new("/usr/bin/python3")
        .args(["-c", "import os\nfor fd, byte in [(1,b'A'),(2,b'B')]:\n data=byte*1048576\n while data:\n  data=data[os.write(fd,data):]\n"])
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("finite burst child");
    let output = live_cycle(child).wait_host();
    assert!(output.status.success(), "{output:?}");
    // Literal external contract: neither expectation derives from capture buffers.
    assert_eq!(output.stdout.len(), 1_048_576);
    assert!(output.stdout.iter().all(|byte| *byte == b'A'));
    assert_eq!(output.stderr.len(), 1_048_576);
    assert!(output.stderr.iter().all(|byte| *byte == b'B'));
}

fn failed_wait(mode: &str) {
    let child = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("blocked live subject");
    let mut cycle = live_cycle(child);
    let pid = cycle.host_pid;
    assert!(process_exists(pid), "subject starts live");
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cycle.wait_host_observing(|| {
            assert!(mode != "panic", "KEL272_OBSERVER_PANIC");
        })
    }));
    assert!(failure.is_err(), "wait must reject {mode}");
    // Observe before dropping LiveCycle or invoking the outer emergency owner.
    assert!(
        !process_exists(pid),
        "wait failure escaped with live or unreaped subject {pid}"
    );
    let failure = failure.expect_err("wait failure");
    let message = panic_message(&failure);
    if mode == "panic" {
        assert_eq!(message, "KEL272_OBSERVER_PANIC");
    } else {
        assert!(
            message.contains("child exceeded exit/output deadline"),
            "{message}"
        );
    }
}

fn inherited_writer() {
    const PROGRAM: &str = r"
import os, socket, sys
control = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
control.connect(sys.argv[1])
pid = os.fork()
if pid:
    os._exit(0)
control.sendall((str(os.getpid()) + '\n').encode())
assert control.recv(1) == b'R'
os._exit(0)
";
    let root = tempfile::tempdir().expect("inherited writer control root");
    let path = root.path().join("writer.sock");
    let listener = UnixListener::bind(&path).expect("writer control listener");
    listener
        .set_nonblocking(true)
        .expect("bounded writer accept");
    let child = Command::new("/usr/bin/python3")
        .args(["-c", PROGRAM])
        .arg(&path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("inherited writer subject");
    let pid = child.id();
    let control = accept_before(&listener, Instant::now() + EVENT_DEADLINE);
    control
        .set_read_timeout(Some(EVENT_DEADLINE))
        .expect("writer control deadline");
    let mut control = BufReader::new(control);
    let mut line = String::new();
    control.read_line(&mut line).expect("writer PID witness");
    let writer_pid = line.trim().parse().expect("writer PID");
    let failure = std::panic::catch_unwind(|| wait_child_output(child, Duration::from_millis(100)));
    let failure = failure.expect_err("direct exit is not inherited-writer EOF");
    assert!(panic_message(&failure).contains("child exceeded exit/output deadline"));
    assert!(
        !process_exists(pid),
        "direct subject must already be reaped"
    );
    assert!(
        process_exists(writer_pid),
        "writer stays independently live until release"
    );
    control
        .get_mut()
        .write_all(b"R")
        .expect("release inherited writer");
    await_process_gone(writer_pid);
}

fn panic_message(value: &Box<dyn std::any::Any + Send>) -> &str {
    value
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| value.downcast_ref::<&str>().copied())
        .expect("text wait failure")
}
