//! Independent process identities and native receipt parsing for installed facade rows.

pub(super) use crate::support::process::lineage::{descendants, retain_native_descendants};

use crate::PRODUCT_DEADLINE;
use crate::support::process::{
    open_process_for_wait, process_times, process_tree_report, wait_for_process_signal,
};
use crate::support::profile_run::actual_webview_process_for_pid;
use serde_json::Value;
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::path::Path;
use std::time::Instant;
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};

pub(super) struct BrowserObservation {
    pub(super) pid: u32,
    pub(super) handle: OwnedHandle,
}

impl BrowserObservation {
    pub(super) fn creation(&self) -> u64 {
        process_times(self.handle.as_raw_handle().cast()).creation
    }

    /// For callers that already require termination, reject a live object.
    pub(super) fn exit(&self) -> u64 {
        self.exit_if_signaled()
            .expect("browser must be signaled before reading its exit time")
    }

    pub(super) fn exit_if_signaled(&self) -> Option<u64> {
        exit_after_wait(wait_for_process_signal(&self.handle, 0), || {
            process_times(self.handle.as_raw_handle().cast()).exit
        })
    }
}

// GetProcessTimes leaves the exit field undefined while the process is live.
// Only the retained kernel object's signal permits interpreting that field.
// https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes
fn exit_after_wait(wait: u32, read_exit: impl FnOnce() -> u64) -> Option<u64> {
    match wait {
        WAIT_TIMEOUT => None,
        WAIT_OBJECT_0 => {
            let exit = read_exit();
            assert_ne!(exit, 0, "terminated browser has no kernel exit time");
            Some(exit)
        }
        other => panic!("browser termination observation failed: {other}"),
    }
}

#[test]
fn live_browser_does_not_read_an_undefined_exit_time() {
    let reads = std::cell::Cell::new(0);
    let observed = exit_after_wait(WAIT_TIMEOUT, || {
        reads.set(reads.get() + 1);
        42 // A possible nonzero output is still undefined for a live process.
    });
    assert_eq!(observed, None);
    assert_eq!(
        reads.get(),
        0,
        "a live process must never consult its exit field"
    );
}

#[test]
fn terminated_browser_preserves_its_exact_kernel_exit_time() {
    assert_eq!(exit_after_wait(WAIT_OBJECT_0, || 42), Some(42));
}

#[test]
fn invalid_wait_cannot_supply_a_browser_exit_time() {
    for wait in [
        windows_sys::Win32::Foundation::WAIT_FAILED,
        windows_sys::Win32::Foundation::WAIT_ABANDONED,
    ] {
        let outcome = std::panic::catch_unwind(|| exit_after_wait(wait, || 42));
        assert!(
            outcome.is_err(),
            "unrecognized wait {wait} earned exit credit"
        );
    }
}

#[test]
fn terminated_browser_cannot_use_the_live_sentinel() {
    assert!(std::panic::catch_unwind(|| exit_after_wait(WAIT_OBJECT_0, || 0)).is_err());
}

pub(super) fn observe_browser(
    host_pid: u32,
    expected_host: &Path,
    expected_udf: &Path,
    previous: Option<u32>,
    terminate: bool,
) -> BrowserObservation {
    let tree = process_tree_report(host_pid);
    let direct = descendants(&tree)
        .into_iter()
        .filter_map(|(name, pid, parent)| {
            (name.eq_ignore_ascii_case("msedgewebview2.exe") && parent == host_pid).then_some(pid)
        })
        .collect::<Vec<_>>();
    let newer = direct
        .iter()
        .copied()
        .filter(|pid| Some(*pid) != previous)
        .collect::<Vec<_>>();
    let pid = match newer.as_slice() {
        [pid] => *pid,
        [] => previous
            .filter(|pid| direct.contains(pid))
            .expect("one current browser in census"),
        _ => panic!("ambiguous independent browser census: {tree}"),
    };
    // Retain the kernel object before validating the query. PID reuse cannot
    // substitute another process while this handle remains open.
    let handle = open_process_for_wait(pid, terminate);
    let observation = actual_webview_process_for_pid(
        host_pid,
        expected_host,
        expected_udf,
        pid,
        Instant::now() + PRODUCT_DEADLINE,
    );
    assert_eq!(
        number(&observation, "browser_pid"),
        pid,
        "exact queried native process"
    );
    assert_eq!(
        wait_for_process_signal(&handle, 0),
        WAIT_TIMEOUT,
        "current browser is live"
    );
    assert_ne!(process_times(handle.as_raw_handle().cast()).creation, 0);
    BrowserObservation { pid, handle }
}

pub(super) fn number(value: &Value, field: &str) -> u32 {
    u32::try_from(
        value[field]
            .as_u64()
            .unwrap_or_else(|| panic!("missing {field}: {value}")),
    )
    .expect("Win32 identity fits u32")
}

pub(super) fn field(line: &str, name: &str) -> u32 {
    line.split_whitespace()
        .find_map(|part| part.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("missing {name} in {line}"))
        .parse()
        .expect("numeric native receipt")
}

pub(super) fn browser_count(tree: &str) -> u32 {
    field(tree, "browser_processes")
}
