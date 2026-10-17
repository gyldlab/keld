//! Real authenticated KGC1 shutdown with an already executing Bun generation.
//! The actual Guardian must retire both Bun and its original process group
//! descendant after its host-only S/KSA exchange, before test rescue.
use std::io;
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Instant;

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::{Pid, getpgid};

use super::super::process_group::{
    GroupIdentity, observe_group_identity, terminate_attested_group,
};
use super::super::{GUARDIAN_SHUTDOWN_DEADLINE, GuardianBootstrap, run_guarded_primary};
use super::test_support::verified_real_bun;

const ROLE_ENV: &str = "KELD_TEST_ACTIVE_KGC1_GUARDIAN_ROLE";
const BUN_ENV: &str = "KELD_TEST_ACTIVE_KGC1_BUN_PATH";
const MARKER_ENV: &str = "KELD_TEST_ACTIVE_KGC1_DESCENDANT_MARKER";
const ROLE_TEST: &str =
    "macos_guardian::tests::active_generation_shutdown::active_generation_guardian_role";

struct OwnedBunGroup(Option<GroupIdentity>);

impl Drop for OwnedBunGroup {
    fn drop(&mut self) {
        if let Some(identity) = self.0.take() {
            // An emergency source-birth-attested rescue, only after product
            // cleanup assertions. Never signal a raw potentially reused PGID.
            let _ = terminate_attested_group(identity);
        }
    }
}

#[test]
fn active_generation_guardian_role() {
    let Some(marker) = std::env::var_os(ROLE_ENV) else {
        return;
    };
    let bun = std::env::var_os(BUN_ENV).expect("test parent verified real Bun");
    let ack = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/fd/3")
        .expect("host-exclusive original KSA output, never libtest stdout");
    let script = r"
const {spawn} = require('node:child_process');
const {writeFileSync, renameSync} = require('node:fs');
const child = spawn('/bin/sleep', ['60'], {stdio:'ignore'});
if (!child.pid) process.exit(72);
// Publish the whole independent PID oracle atomically. A reader must never
// observe a half-written numeric descendant identity and misreport a group.
const receipt = process.env.KELD_TEST_ACTIVE_KGC1_DESCENDANT_MARKER;
writeFileSync(`${receipt}.partial`, `${process.pid} ${child.pid}\n`);
renameSync(`${receipt}.partial`, receipt);
setInterval(()=>{}, 1000);
";
    let result = run_guarded_primary(
        io::stdin().lock(),
        move |_app_link| {
            let mut command = Command::new(&bun);
            command.args(["-e", script]).env(MARKER_ENV, &marker);
            Ok(command)
        },
        ack,
    );
    assert!(
        result.is_ok(),
        "genuine orderly S must retire a registered live Bun generation: {result:?}"
    );
}

#[test]
fn native_orderly_s_reaps_executing_bun_and_same_group_descendant() {
    let temp = tempfile::tempdir().expect("owned native live KGC1 fixture");
    let marker = temp.path().join("live-group-pids");
    let bun = verified_real_bun();
    // The test runner's banners must never enter the original OS KSA pipe.
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "exec 3>&1; exec 1>&2; exec \"$@\"",
            "guardian-test-proxy",
        ])
        .arg(std::env::current_exe().expect("real native libtest executable"))
        .args(["--exact", ROLE_TEST, "--nocapture"])
        .env(ROLE_ENV, &marker)
        .env(BUN_ENV, &bun)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let pending = GuardianBootstrap::spawn_supervised(command).expect("genuine KGC1 Guardian");
    let mut primary = pending
        .register_guarded_primary_until(Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE)
        .expect("host retained real authenticated Guardian");

    let deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    let (bun_pid, descendant) = loop {
        if let Ok(receipt) = std::fs::read_to_string(&marker) {
            let ids: Vec<u32> = receipt
                .split_whitespace()
                .map(|id| id.parse().expect("real Bun/descendant numeric PID"))
                .collect();
            if let [bun, member] = ids.as_slice() {
                break (*bun, *member);
            }
        }
        assert!(
            Instant::now() < deadline,
            "real Bun did not execute its descendant script"
        );
        thread::yield_now();
    };
    let original_birth = observe_group_identity(bun_pid).expect("real registered Bun birth");
    let _owner = OwnedBunGroup(Some(original_birth));
    let group = Pid::from_raw(i32::try_from(bun_pid).expect("positive Bun group PID"));
    assert_eq!(
        getpgid(Some(Pid::from_raw(
            i32::try_from(descendant).expect("descendant PID")
        ))),
        Ok(group)
    );
    assert_eq!(
        primary.group_pid(),
        Some(bun_pid),
        "host owns native registered group"
    );
    assert_eq!(kill(group, None), Ok(()), "actual Bun leader is still live");
    // The host alone performs the existing authenticated S/KSA exchange. The
    // Guardian must then revoke the native lease before reporting success.
    let status = primary
        .shutdown()
        .expect("orderly S must retire running Bun and descendant");
    assert!(
        status.success(),
        "original Guardian successful exit: {status:?}"
    );
    assert!(
        primary.group_pid().is_none(),
        "host relinquished group only after retirement"
    );
    assert!(
        primary
            .child
            .try_wait()
            .expect("reaped real Guardian")
            .is_some(),
        "owned Guardian was reaped"
    );
    let absence_deadline = Instant::now() + GUARDIAN_SHUTDOWN_DEADLINE;
    while kill(Pid::from_raw(-group.as_raw()), None) != Err(Errno::ESRCH)
        || kill(
            Pid::from_raw(i32::try_from(descendant).expect("descendant PID")),
            None,
        ) != Err(Errno::ESRCH)
    {
        assert!(
            Instant::now() < absence_deadline,
            "same-group Bun descendant remains after S"
        );
        thread::yield_now();
    }
    // The product disappearance was observed BEFORE OwnedBunGroup Drop.
    assert_eq!(
        status.signal(),
        None,
        "Guardian shutdown is orderly, not SIGKILL"
    );
}
