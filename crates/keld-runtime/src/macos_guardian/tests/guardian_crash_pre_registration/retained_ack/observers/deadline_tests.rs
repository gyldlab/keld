//! Native marker receipt clocks, independently owned by this fixture.
use super::{GUARDIAN_SHUTDOWN_DEADLINE, await_executed_bun_until, kernel_process_image};
use crate::macos_guardian::process_group::{
    GroupIdentity, terminate_attested_group, validate_guardian_child_group,
};
use crate::macos_guardian::tests::test_support::NativeChildOwner;
use nix::unistd::{Pid, getpgid, getpgrp};
use std::any::Any;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

// Both children have original, retained std::process::Child handles. Never
// infer a kill right from a text marker's numeric PID. On assertion failure,
// Drop may signal ONLY the kernel-birth-attested original group, then rescues
// either exact direct Child handle and waits for both native exit statuses.
struct OwnedMarkerGroup {
    leader: NativeChildOwner,
    member: NativeChildOwner,
    original_birth: GroupIdentity,
    path: tempfile::TempDir,
}

impl OwnedMarkerGroup {
    fn start() -> Self {
        let leader = NativeChildOwner::new(
            Command::new("/bin/sleep")
                .arg("30")
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("test owns actual native group leader"),
        );
        let leader_pid = leader.id();
        let raw = i32::try_from(leader_pid).expect("native group PID is positive c_int");
        assert_ne!(
            Pid::from_raw(raw),
            getpgrp(),
            "do not own runner's process group"
        );
        let original_birth = validate_guardian_child_group(leader_pid, std::process::id())
            .expect("the test runner is the kernel-reported parent of this original group");
        // The already-owned leader is automatically killed/reaped if any
        // subsequent fixture assertion, member spawn or tempdir creation fails.
        let member = NativeChildOwner::new(
            Command::new("/bin/sleep")
                .arg("30")
                .process_group(raw)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("own actual native same-group member"),
        );
        let owned = Self {
            leader,
            member,
            original_birth,
            path: tempfile::tempdir().expect("private real marker directory"),
        };
        assert_eq!(
            getpgid(Some(Pid::from_raw(
                i32::try_from(owned.member.id()).expect("member PID")
            ))),
            Ok(Pid::from_raw(raw)),
            "kernel must confirm member of this exact, test-owned group"
        );
        owned
    }

    fn group(&self) -> i32 {
        i32::try_from(self.leader.id()).expect("positive original leader PID")
    }

    fn marker(&self) -> std::path::PathBuf {
        let path = self.path.path().join("real-owned-image-and-member.pid");
        std::fs::write(
            &path,
            format!("{} {}\n", self.leader.id(), self.member.id()),
        )
        .expect("write actual retained Child identities to a private marker file");
        path
    }

    fn pre_exec_image(&self) -> String {
        let image =
            kernel_process_image(i32::try_from(std::process::id()).expect("test runner PID"));
        let actual_native_image = kernel_process_image(self.group());
        assert_eq!(
            actual_native_image.rsplit('/').next(),
            Some("sleep"),
            "real /bin/sleep process image must be read from original native Child"
        );
        assert_ne!(
            actual_native_image, image,
            "actual native /bin/sleep image must differ from pre-exec test executable"
        );
        image
    }
}

impl Drop for OwnedMarkerGroup {
    fn drop(&mut self) {
        let _ = terminate_attested_group(self.original_birth);
        // Each retained NativeChildOwner performs exact Child kill and reap
        // even when the attested whole-group signal has been refused.
    }
}

fn native_panic_text(value: Box<dyn Any + Send>) -> String {
    match value.downcast::<String>() {
        Ok(text) => *text,
        Err(other) => match other.downcast::<&str>() {
            Ok(text) => (*text).to_owned(),
            Err(_) => "non-string native observer panic".to_owned(),
        },
    }
}

#[test]
fn real_native_marker_read_success_after_original_clock_must_fail() {
    let owned = OwnedMarkerGroup::start();
    let marker = owned.marker();
    let image = owned.pre_exec_image();
    // The exact original five-second budget begins before the actual marker
    // read. No synthetic clock injection or extra phase timeout is accepted.
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let (observed_tx, observed_rx) = mpsc::sync_channel(1);
    let (resume_tx, resume_rx) = mpsc::sync_channel(1);
    let coordinator = thread::spawn(move || {
        observed_rx
            .recv_timeout(GUARDIAN_SHUTDOWN_DEADLINE)
            .expect("observer must complete a genuine native marker read");
        while Instant::now() <= deadline {
            thread::yield_now();
        }
        resume_tx
            .send(())
            .expect("release actual read after original clock");
    });

    // A callback invoked immediately AFTER std::fs::read_to_string succeeds
    // witnesses the real read and fences only the subsequent success branch.
    // Old source returns its verified child PID even when released too late.
    let mut read_receipts = 0usize;
    let mut validated_native_receipts = 0usize;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        await_executed_bun_until(
            &marker,
            owned.group(),
            &image,
            deadline,
            || {
                read_receipts += 1;
                observed_tx
                    .send(())
                    .expect("one actual native marker-read receipt");
                resume_rx
                    .recv()
                    .expect("coordinator passed original deadline");
            },
            || validated_native_receipts += 1,
        )
    }));
    coordinator
        .join()
        .expect("native clock-gate coordinator joins");
    assert_eq!(
        read_receipts, 1,
        "one successful real marker read, not a poll timeout"
    );
    assert_eq!(
        validated_native_receipts, 1,
        "real same-group and executable-image assertions must execute before deadline rejection"
    );
    let error = outcome.expect_err(
        "a complete native group/image marker read after original 5s must never return success",
    );
    assert!(
        native_panic_text(error).contains("native marker accepted after original deadline"),
        "only the post-success original-clock guard may reject this late native observation"
    );
}

#[test]
fn real_native_marker_within_original_clock_succeeds() {
    let owned = OwnedMarkerGroup::start();
    let marker = owned.marker();
    let image = owned.pre_exec_image();
    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let mut read_receipts = 0usize;
    let mut validated_native_receipts = 0usize;
    let native_descendant = await_executed_bun_until(
        &marker,
        owned.group(),
        &image,
        deadline,
        || read_receipts += 1,
        || validated_native_receipts += 1,
    );
    assert_eq!(
        read_receipts, 1,
        "actual marker read completed exactly once"
    );
    assert_eq!(
        validated_native_receipts, 1,
        "kernel group and image validation completed exactly once"
    );
    assert_eq!(
        native_descendant,
        i32::try_from(owned.member.id()).expect("actual retained native descendant"),
        "valid marker only names the actual kernel-verified same-group Child"
    );
    assert!(
        Instant::now() < deadline,
        "positive result must arrive inside original budget"
    );
}
