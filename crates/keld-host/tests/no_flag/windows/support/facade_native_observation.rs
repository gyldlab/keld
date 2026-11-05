//! Independent native window, process and profile observations for facade cells.
//! Reuses the reviewed window observer and existing process handle primitives.

use crate::PRODUCT_DEADLINE;
use crate::support::process::lineage::retain_native_descendants;
use crate::support::process::process_times;
use crate::support::window::{NativeWindowSnapshot, app_top_level_windows};
use std::fs;
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) fn census(pid: u32, count: usize) -> Vec<isize> {
    let deadline = Instant::now() + PRODUCT_DEADLINE;
    loop {
        let windows = app_top_level_windows(pid);
        if windows.len() == count {
            return windows.into_iter().map(|window| window.hwnd).collect();
        }
        assert!(
            Instant::now() < deadline,
            "expected {count} native windows, found {windows:?}"
        );
        thread::park_timeout(Duration::from_millis(10));
    }
}

pub(crate) fn capture(hwnd: isize, pid: u32) -> NativeWindowSnapshot {
    capture_observed(hwnd, pid, "capture")
}

fn capture_observed(hwnd: isize, pid: u32, phase: &str) -> NativeWindowSnapshot {
    let snapshot =
        NativeWindowSnapshot::capture(u64::try_from(hwnd).expect("observed positive HWND"), pid)
            .expect("native HWND/controller tree");
    println!(
        "KELD_GH1188_NATIVE_CHILDREN phase={phase} hwnd={hwnd} host={pid} children={:?} details={:?}",
        snapshot.children, snapshot.child_details
    );
    snapshot
}

pub(crate) fn same_tree(before: &NativeWindowSnapshot, pid: u32) {
    let after = capture_observed(
        isize::try_from(before.hwnd).expect("HWND"),
        pid,
        "same-tree",
    );
    let mut old = before.children.clone();
    let mut new = after.children;
    old.sort_unstable();
    new.sort_unstable();
    assert!(!old.is_empty(), "at-risk controller hosting windows absent");
    assert_eq!(old, new, "the original controller child HWND set changed");
    assert_eq!(
        before.outer_rect, after.outer_rect,
        "native frame changed without a native operation"
    );
    assert_eq!(
        before.live_handles().len(),
        before.children.len() + 1,
        "an original native HWND retired while its facade remained live"
    );
}

pub(crate) fn assert_retired(before: &NativeWindowSnapshot) {
    assert_eq!(
        before.live_handles(),
        Vec::<usize>::new(),
        "Host answered before native HWND/child teardown"
    );
}

pub(crate) fn ephemeral_profile_dir(profile_root: &Path) -> PathBuf {
    let root = profile_root.join("Keld/ephemeral/v1");
    let entries = fs::read_dir(&root)
        .expect("isolated ephemeral profiles")
        .map(|entry| entry.expect("ephemeral profile entry").path())
        .collect::<Vec<_>>();
    assert_eq!(
        entries.len(),
        1,
        "one session owns one ephemeral profile: {entries:?}"
    );
    entries[0].clone()
}

pub(crate) fn retained_descendants(root: &mut Child) -> Vec<(u32, OwnedHandle)> {
    // The guard has retained this original Child since spawn; its kernel handle,
    // not a fresh PID lookup, supplies the expected root generation.
    let birth = process_times(root.as_raw_handle().cast()).creation;
    let (_, descendants) = retain_native_descendants(root, birth);
    descendants
        .into_iter()
        .map(|child| (child.pid, child.handle))
        .collect()
}
