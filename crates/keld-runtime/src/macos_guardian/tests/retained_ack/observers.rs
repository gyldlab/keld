//! Kernel/process and private-socket observers for both native KGR1 ACK cuts.
use std::io::{self, Read as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;

use super::GUARDIAN_SHUTDOWN_DEADLINE;
use super::kernel_process_image;

/// The actual child image is still the native libtest preexec reporter.
/// Unlike a JS marker-only oracle, a premature exec becomes visible here.
pub(super) fn observe_pre_ack_image(group: i32, marker: &Path) -> String {
    let original_image = kernel_process_image(i32::try_from(std::process::id()).expect("PID"));
    let observe_until = Instant::now() + Duration::from_millis(100);
    loop {
        assert_eq!(
            kernel_process_image(group),
            original_image,
            "pre-ACK child already replaced its executable image"
        );
        assert!(!marker.exists(), "Bun ran before KGR1 host ACK");
        if Instant::now() >= observe_until {
            break;
        }
        thread::yield_now();
    }
    original_image
}

pub(super) fn receive_event(listener: &UnixListener, deadline: Instant) -> (UnixStream, String) {
    receive_event_with_hooks(listener, deadline, |_| {}, |_| {})
}

// Preserve the shipping native Unix listener/peer while allowing the test to
// independently hold a byte exactly AFTER a successful pre-read clock probe.
// No mock peer, forged event, or new production control channel is introduced.
fn receive_event_with_hooks(
    listener: &UnixListener,
    deadline: Instant,
    mut on_prechecked_read: impl FnMut(usize),
    mut on_native_byte: impl FnMut(u8),
) -> (UnixStream, String) {
    loop {
        assert!(
            Instant::now() < deadline,
            "original retained-ACK event deadline expired before native accept"
        );
        match listener.accept() {
            Ok((mut stream, _)) => {
                // A connection queued before accept() is not permission to
                // consume it after the caller's original absolute deadline.
                assert!(
                    Instant::now() < deadline,
                    "original retained-ACK event deadline expired after native accept"
                );
                stream
                    .set_nonblocking(true)
                    .expect("nonblocking event read");
                let mut bytes = Vec::new();
                loop {
                    assert!(
                        Instant::now() < deadline,
                        "original retained-ACK event deadline expired before native read"
                    );
                    on_prechecked_read(bytes.len());
                    let mut byte = [0_u8; 1];
                    match stream.read(&mut byte) {
                        Ok(1) => {
                            on_native_byte(byte[0]);
                            // One successful kernel byte is not a completed
                            // on-time record. Include LF in the same clock.
                            assert!(
                                Instant::now() < deadline,
                                "original retained-ACK event deadline expired after native read"
                            );
                            if byte[0] == b'\n' {
                                break;
                            }
                            bytes.push(byte[0]);
                            assert!(bytes.len() <= 100, "bounded cut receipt");
                        }
                        Ok(0) => panic!("event closed before its newline"),
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "partial event timed out");
                            thread::yield_now();
                        }
                        Err(error) => panic!("read native process receipt: {error}"),
                        Ok(_) => unreachable!("one-byte event receipt"),
                    }
                }
                return (
                    stream,
                    String::from_utf8(bytes).expect("ASCII process receipt"),
                );
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "real KGR1 custody event timed out"
                );
                thread::yield_now();
            }
            Err(error) => panic!("accept cut control: {error}"),
        }
    }
}

#[cfg(test)]
#[path = "observers/deadline_tests.rs"]
mod deadline_tests;

pub(super) fn group_gone(group: i32, descendant: Option<i32>, guardian: i32) {
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    group_gone_with_probe_hook(group, descendant, guardian, deadline, |_| {});
}

// One exact original expiry; test-only native-kernel observation hook. This
// source module is compiled solely by macOS Guardian native tests, never into
// the production Guardian, and the hook grants no signal authority.
fn group_gone_with_probe_hook(
    group: i32,
    descendant: Option<i32>,
    guardian: i32,
    deadline: Instant,
    mut on_native_probe: impl FnMut(bool),
) {
    loop {
        assert!(
            Instant::now() < deadline,
            "real Guardian or Bun group remained unverified at original shutdown deadline"
        );
        let absent = kill(Pid::from_raw(-group), None) == Err(Errno::ESRCH)
            && kill(Pid::from_raw(group), None) == Err(Errno::ESRCH)
            && kill(Pid::from_raw(guardian), None) == Err(Errno::ESRCH)
            && descendant.is_none_or(|pid| kill(Pid::from_raw(pid), None) == Err(Errno::ESRCH));
        on_native_probe(absent);
        if absent {
            // Four read-only native probes describe one observation, not a
            // new deadline. After the original clock expires, even genuine
            // absence cannot retroactively certify timely cleanup.
            assert!(
                Instant::now() < deadline,
                "native group absence was not confirmed by original shutdown deadline"
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "real Guardian or Bun group leaked after host death"
        );
        thread::yield_now();
    }
}

pub(super) fn read_marker(marker: &Path, deadline: Instant) -> (i32, i32) {
    read_marker_with_hook(marker, deadline, |_| {})
}

fn read_marker_with_hook(
    marker: &Path,
    deadline: Instant,
    mut on_real_file_read: impl FnMut(&str),
) -> (i32, i32) {
    loop {
        assert!(
            Instant::now() < deadline,
            "real Bun and descendant marker was unavailable at original ACK deadline"
        );
        if let Ok(bytes) = std::fs::read_to_string(marker) {
            on_real_file_read(&bytes);
            let ids: Vec<i32> = bytes
                .split_whitespace()
                .map(|value| value.parse().expect("numeric Bun/descendant PID"))
                .collect();
            if let [leader, descendant] = ids.as_slice() {
                // Do not turn a file record consumed before the deadline into
                // a successful KGR1 witness after unbounded parsing/scheduling.
                assert!(
                    Instant::now() < deadline,
                    "Bun and descendant marker was accepted after original ACK deadline"
                );
                return (*leader, *descendant);
            }
        }
        assert!(
            Instant::now() < deadline,
            "real Bun and descendant did not execute after ACK"
        );
        thread::yield_now();
    }
}

#[cfg(test)]
#[path = "observers/arrival_deadline_tests.rs"]
mod arrival_deadline_tests;
