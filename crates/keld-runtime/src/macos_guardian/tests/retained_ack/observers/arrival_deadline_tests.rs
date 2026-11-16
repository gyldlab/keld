//! Actual native cut-time evidence for marker and whole-group observers.
//! An expired absolute phase may not be converted to a timely success by a
//! late available file or a completed, but post-deadline, kernel census.
use super::{group_gone_with_probe_hook, read_marker_with_hook};
use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid};
use std::os::unix::process::CommandExt as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

// Only this exact native Child is ever signaled, rather than an observed or
// event-reported PID/PGID. Stdin is not needed for a disposable /bin/sleep.
struct OwnedSleep(Child);

impl OwnedSleep {
    fn spawn(isolated_group: bool) -> Self {
        let mut command = Command::new("/bin/sleep");
        command
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if isolated_group {
            command.process_group(0);
        }
        Self(command.spawn().expect("own a real disposable native child"))
    }
}

impl Drop for OwnedSleep {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn observed_native_group_absence_cannot_complete_after_original_phase_deadline() {
    let mut owned = OwnedSleep::spawn(true);
    let group = i32::try_from(owned.0.id()).expect("original owned native leader pid_t");
    assert_ne!(Pid::from_raw(group), nix::unistd::getpgrp());
    assert_eq!(
        getpgid(Some(Pid::from_raw(group))),
        Ok(Pid::from_raw(group))
    );
    owned
        .0
        .kill()
        .expect("kill only the exact owned native Child");
    owned.0.wait().expect("reap exact original native leader");
    assert_eq!(
        kill(Pid::from_raw(-group), None),
        Err(Errno::ESRCH),
        "native group absence must be real before the call"
    );

    let deadline = Instant::now() + Duration::from_millis(250);
    let (observed_tx, observed_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        catch_unwind(AssertUnwindSafe(|| {
            group_gone_with_probe_hook(group, None, group, deadline, |absent| {
                let _ = observed_tx.send((absent, Instant::now()));
                let _ = release_rx.recv_timeout(Duration::from_secs(1));
            });
        }))
    });
    let receipt = observed_rx.recv_timeout(Duration::from_millis(200));
    while Instant::now() < deadline {
        thread::yield_now();
    }
    let _ = release_tx.send(());
    let result = reader.join().expect("join exact native group reader");
    assert!(
        receipt.is_ok_and(|(absent, when)| absent && when < deadline),
        "read-only kernel SIG0 must observe actual absence before delayed verdict"
    );
    assert!(
        result.is_err(),
        "an expired original clock may not become a new timely group-clear result"
    );
}

#[test]
fn real_native_marker_read_before_cutoff_cannot_return_after_original_deadline() {
    let original = OwnedSleep::spawn(false);
    let descendant = OwnedSleep::spawn(false);
    assert_ne!(original.0.id(), descendant.0.id());
    let root = tempfile::tempdir().expect("own private native marker root");
    let marker = root.path().join("real-roles.pid");
    std::fs::write(
        &marker,
        format!("{} {}\n", original.0.id(), descendant.0.id()),
    )
    .expect("write marker containing two actual owned native Child IDs");
    let deadline = Instant::now() + Duration::from_millis(250);
    let (received_tx, received_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let read_marker_path = marker.clone();
    let reader = thread::spawn(move || {
        catch_unwind(AssertUnwindSafe(|| {
            read_marker_with_hook(&read_marker_path, deadline, |bytes| {
                let _ = received_tx.send((bytes.contains(' '), Instant::now()));
                let _ = release_rx.recv_timeout(Duration::from_secs(1));
            })
        }))
    });
    let read = received_rx.recv_timeout(Duration::from_millis(200));
    while Instant::now() < deadline {
        thread::yield_now();
    }
    let _ = release_tx.send(());
    let result = reader.join().expect("join exact native marker reader");
    assert!(
        read.is_ok_and(|(complete, when)| complete && when < deadline),
        "real marker data must actually be read before original caller deadline"
    );
    assert!(
        result.is_err(),
        "late returned marker cannot be treated as a timely native Bun/descendant sighting"
    );
}

#[test]
fn real_owned_native_marker_is_accepted_within_original_budget() {
    let original = OwnedSleep::spawn(false);
    let descendant = OwnedSleep::spawn(false);
    let root = tempfile::tempdir().expect("own valid native marker root");
    let marker = root.path().join("valid-roles.pid");
    std::fs::write(
        &marker,
        format!("{} {}\n", original.0.id(), descendant.0.id()),
    )
    .expect("write two actual owned Child IDs");
    let (leader, child) =
        read_marker_with_hook(&marker, Instant::now() + Duration::from_secs(1), |_| {});
    assert_eq!(leader, i32::try_from(original.0.id()).expect("pid_t"));
    assert_eq!(child, i32::try_from(descendant.0.id()).expect("pid_t"));
}

#[test]
fn real_reaped_native_group_can_be_confirmed_within_original_budget() {
    let mut owned = OwnedSleep::spawn(true);
    let group = i32::try_from(owned.0.id()).expect("real original group");
    owned.0.kill().expect("kill only exact owned Child");
    owned.0.wait().expect("reap only exact owned Child");
    group_gone_with_probe_hook(
        group,
        None,
        group,
        Instant::now() + Duration::from_secs(1),
        |_| {},
    );
}
