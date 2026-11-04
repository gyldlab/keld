//! An exited native child can still have unread receipt bytes in its stderr pipe.

use super::SeamedHost;
use crate::support::process::{PIPE_DRAIN_KILL_SWITCH, read_pipe_on_thread, wait_child};
use std::io::{self, Read};
use std::process::{ChildStderr, Command, Stdio};
use std::sync::mpsc;
use std::time::Instant;

struct HeldReader {
    pipe: ChildStderr,
    release: Option<mpsc::Receiver<()>>,
}

impl Read for HeldReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if let Some(release) = self.release.take() {
            release
                .recv_timeout(PIPE_DRAIN_KILL_SWITCH)
                .map_err(io::Error::other)?;
        }
        self.pipe.read(bytes)
    }
}

#[test]
#[ignore = "private native stderr-tail subprocess helper"]
fn stderr_tail_helper() {
    assert_eq!(
        std::env::var("KELD_TEST_STDERR_TAIL_HELPER").as_deref(),
        Ok("1")
    );
    eprintln!("KELD_TEST_TAIL first");
    eprintln!("KELD_TEST_TAIL second");
}

fn exited_host_with_held_reader() -> (SeamedHost, mpsc::Sender<()>) {
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "profiles::sessions::capture_receipts::stderr_tail_helper",
            "--ignored",
            "--nocapture",
        ])
        .env("KELD_TEST_STDERR_TAIL_HELPER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("native receipt helper");
    assert!(wait_child(&mut child, Instant::now() + PIPE_DRAIN_KILL_SWITCH).success());
    let pipe = child.stderr.take().expect("real unread stderr pipe");
    let (release, held) = mpsc::channel();
    let mut host = SeamedHost::from_child(child);
    host.live_stderr = Some(read_pipe_on_thread(HeldReader {
        pipe,
        release: Some(held),
    }));
    (host, release)
}

#[test]
fn exited_host_tail_receipt_waits_for_the_reader() {
    let (mut host, release) = exited_host_with_held_reader();
    let (first, cursor) = host.wait_stderr_line_observed(
        "KELD_TEST_TAIL ",
        0,
        Instant::now() + PIPE_DRAIN_KILL_SWITCH,
        || {
            release
                .send(())
                .expect("allow capture only after the first missing observation");
        },
    );
    assert_eq!(first, "KELD_TEST_TAIL first");
    let (second, next) = host.wait_stderr_line_observed(
        "KELD_TEST_TAIL ",
        cursor,
        Instant::now() + PIPE_DRAIN_KILL_SWITCH,
        || panic!("a buffered line must return without the missing-line path"),
    );
    assert_eq!(second, "KELD_TEST_TAIL second");
    assert!(next > cursor);
    let (status, _, stderr) = host.finish();
    assert!(status.success());
    assert!(
        stderr.contains("KELD_TEST_TAIL second"),
        "final diagnostics were consumed"
    );
}

#[test]
fn exited_host_without_the_receipt_still_fails_after_drain() {
    let (mut host, release) = exited_host_with_held_reader();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host.wait_stderr_line_observed(
            "MISSING_RECEIPT",
            0,
            Instant::now() + PIPE_DRAIN_KILL_SWITCH,
            || release.send(()).expect("release actual pipe capture"),
        )
    }));
    let panic = result.expect_err("exit and pipe completion are not a receipt");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("diagnostic panic text");
    assert!(
        message.contains("host exited before MISSING_RECEIPT"),
        "{message}"
    );
    assert!(
        message.contains("KELD_TEST_TAIL second"),
        "missing drained output: {message}"
    );
    assert!(host.finish().0.success());
}
