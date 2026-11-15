//! Ordinary-process regressions for bounded native child capture and reaping.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::PathBuf;

use super::support::{CUT_ENV, ROOT_ENV, child_with_timeout};

const HELPER: &str = "windows_baseline::tests::capture::capture_child";
const BURST_BYTES: usize = 1024 * 1024;
const CAPTURE_TIMEOUT_MS: u32 = 10_000;

#[test]
fn child_capture_preserves_both_pipe_sized_bursts() {
    let root = tempfile::tempdir().expect("ordinary capture fixture");
    let result = std::panic::catch_unwind(|| {
        child_with_timeout(HELPER, root.path(), "", "burst", 0, CAPTURE_TIMEOUT_MS)
    });
    assert_eq!(
        fs::read(root.path().join("burst-started")).expect("child reached burst write"),
        b"started"
    );
    println!("KELD_KEL266_CAPTURE_CHILD_REACHED_BURST");
    let (stdout, stderr) = result.expect("finite burst must finish after the child starts writing");
    assert_burst(&stdout, "CAPTURE_STDOUT", b'A');
    assert_burst(&stderr, "CAPTURE_STDERR", b'B');
}

fn assert_burst(observed: &str, label: &str, byte: u8) {
    let (_, tail) = observed
        .split_once(&format!("{label}_BEGIN\n"))
        .expect("begin marker");
    let (body, _) = tail
        .split_once(&format!("\n{label}_END\n"))
        .expect("tail marker");
    assert_eq!(body.len(), BURST_BYTES, "exact stream byte count");
    assert!(
        body.as_bytes().iter().all(|actual| *actual == byte),
        "exact stream bytes"
    );
}

#[test]
fn child_status_failure_preserves_stderr_and_stdout() {
    let root = tempfile::tempdir().expect("ordinary status fixture");
    let failure = std::panic::catch_unwind(|| {
        child_with_timeout(HELPER, root.path(), "", "status", 0, CAPTURE_TIMEOUT_MS)
    })
    .expect_err("unexpected exit status must fail the real helper");
    let text = panic_text(&failure);
    assert!(text.contains("CAPTURE_STATUS_STDOUT"), "{text}");
    assert!(text.contains("CAPTURE_STATUS_STDERR"), "{text}");
    assert!(text.contains("Some(7)"), "actual exit code: {text}");
}

#[test]
fn child_timeout_preserves_diagnostics_and_reaps_parked_process() {
    let root = tempfile::tempdir().expect("ordinary timeout fixture");
    let failure = std::panic::catch_unwind(|| {
        child_with_timeout(HELPER, root.path(), "", "park", 0, CAPTURE_TIMEOUT_MS)
    })
    .expect_err("parked child must hit the bounded timeout");
    let text = panic_text(&failure);
    assert!(text.contains("kill-switch bound"), "{text}");
    assert!(text.contains("CAPTURE_PARK_STDOUT"), "{text}");
    assert!(text.contains("CAPTURE_PARK_STDERR"), "{text}");
    let held = root.path().join("child-held");
    assert_eq!(
        fs::read(&held).expect("ready witness remains after timeout"),
        b"held"
    );
    drop(
        OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(held)
            .expect("killed and reaped child released its exclusive file handle"),
    );
}

fn panic_text(value: &Box<dyn std::any::Any + Send>) -> &str {
    value
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| value.downcast_ref::<&str>().copied())
        .expect("text panic diagnostic")
}

#[test]
#[ignore = "private ordinary subprocess endpoint of native capture regressions"]
fn capture_child() {
    let mode = std::env::var(CUT_ENV).expect("explicit capture mode");
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    match mode.as_str() {
        "burst" => {
            let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("capture fixture root"));
            fs::write(root.join("burst-started"), b"started").expect("independent started witness");
            writeln!(stdout, "CAPTURE_STDOUT_BEGIN").expect("stdout begin");
            stdout
                .write_all(&vec![b'A'; BURST_BYTES])
                .expect("stdout burst");
            writeln!(stdout, "\nCAPTURE_STDOUT_END").expect("stdout tail");
            writeln!(stderr, "CAPTURE_STDERR_BEGIN").expect("stderr begin");
            stderr
                .write_all(&vec![b'B'; BURST_BYTES])
                .expect("stderr burst");
            writeln!(stderr, "\nCAPTURE_STDERR_END").expect("stderr tail");
        }
        "status" => {
            writeln!(stdout, "CAPTURE_STATUS_STDOUT").expect("stdout failure witness");
            writeln!(stderr, "CAPTURE_STATUS_STDERR").expect("stderr failure witness");
        }
        "park" => {
            let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("capture fixture root"));
            let mut held = OpenOptions::new()
                .write(true)
                .create_new(true)
                .share_mode(0)
                .open(root.join("child-held"))
                .expect("exclusive child lifetime witness");
            held.write_all(b"held").expect("write ready witness");
            writeln!(stdout, "CAPTURE_PARK_STDOUT").expect("stdout park witness");
            writeln!(stderr, "CAPTURE_PARK_STDERR").expect("stderr park witness");
            stdout.flush().expect("flush stdout before parking");
            stderr.flush().expect("flush stderr before parking");
            loop {
                std::thread::park();
            }
        }
        _ => panic!("unknown capture mode: {mode}"),
    }
    stdout.flush().expect("flush stdout before exit");
    stderr.flush().expect("flush stderr before exit");
    std::process::exit(if mode == "status" { 7 } else { 0 });
}
