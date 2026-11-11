//! A real exited child remains visible in the retained Job's lifetime total.

use super::super::{WindowsProcessJob, create_job_completion_port};
use super::{CONTINUE_BYTE, ObservedProcess, RELEASE_WAIT, blocked_child_command};
use std::io::Write as _;
use std::os::windows::io::AsRawHandle as _;
use std::os::windows::process::CommandExt as _;
use std::time::Instant;
use windows_sys::Win32::System::Threading::DETACHED_PROCESS;

#[test]
fn lifetime_accounting_retains_a_child_after_active_zero() {
    let mut job = WindowsProcessJob::create().expect("create an isolated observer-control Job");
    let raw = job.handle.as_raw_handle().cast();
    // Associate while empty so the zero notice cannot be lost after child exit.
    let completion =
        create_job_completion_port(raw).expect("associate the existing Job completion owner");
    let before = job.process_accounting().expect("empty Job accounting");
    assert_eq!(before, (0, 0), "the test owns a fresh Job");

    // Reuse the exact assignment-gate role and its pipe. No console host may
    // become an incidental member: this cell has exactly one test-owned child.
    let mut child = blocked_child_command()
        .creation_flags(DETACHED_PROCESS)
        .spawn()
        .expect("spawn the existing gated child without a console");
    let observed = ObservedProcess::open(child.id());
    let child_pid = child.id();
    job.assign_child(&child)
        .expect("the same owner assigns and proves exact child membership");
    let live = !observed.exited_now();
    let during = job.process_accounting().expect("live child accounting");

    let mut start = child.stdin.take().expect("existing assignment-gate pipe");
    start
        .write_all(&[CONTINUE_BYTE])
        .expect("release the existing child to its normal exit");
    drop(start);
    observed.assert_exited_within(RELEASE_WAIT, "the released observer-control child");
    let output = child.wait_with_output().expect("reap the exited child");
    job.wait_for_active_zero(&completion, Instant::now() + RELEASE_WAIT)
        .expect("native completion plus accounting prove the Job has no active member");
    let after = job.process_accounting().expect("post-exit Job accounting");
    let retained = job.process_accounting().expect("retained Job accounting");

    // Cleanup precedes the disputed lifetime assertion, including for the
    // active-as-total mutant. A watchdog or a leaked child is not its oracle.
    assert!(
        live,
        "the independently opened child was live at the first query"
    );
    assert!(
        output.status.success(),
        "the actual gated child did not exit normally: {:?}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("START_RELEASED"),
        "the existing fixture must acknowledge its actual release",
    );
    assert_eq!(
        during,
        (1, 1),
        "the live child increments both kernel counters"
    );
    assert_eq!(after.0, 0, "all active Job members have retired");
    assert_eq!(
        after.1, during.1,
        "lifetime total must retain the exited child after active count reaches zero",
    );
    assert_eq!(
        retained, after,
        "querying again must preserve the retained history"
    );
    println!(
        "KELD_WINDOWS_JOB_ACCOUNTING_CONTROL pid={child_pid} before={before:?} during={during:?} after={after:?} retained={retained:?}"
    );
}
