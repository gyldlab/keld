//! Independent process identities and native receipt parsing for installed facade rows.

use crate::PRODUCT_DEADLINE;
use crate::support::process::{
    open_process_for_wait, process_times, process_tree_report, wait_for_process_signal,
};
use crate::support::profile_run::actual_webview_process_for_pid;
use serde_json::Value;
use std::os::windows::io::{AsRawHandle as _, OwnedHandle};
use std::path::Path;
use std::time::Instant;
use windows_sys::Win32::Foundation::WAIT_TIMEOUT;

pub(super) struct BrowserObservation {
    pub(super) pid: u32,
    pub(super) handle: OwnedHandle,
}

impl BrowserObservation {
    pub(super) fn creation(&self) -> u64 {
        process_times(self.handle.as_raw_handle().cast()).creation
    }

    pub(super) fn exit(&self) -> u64 {
        process_times(self.handle.as_raw_handle().cast()).exit
    }
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

pub(super) fn descendants(tree: &str) -> Vec<(String, u32, u32)> {
    tree.split_once(" descendants=")
        .expect("native census descendants")
        .1
        .split(',')
        .filter(|value| !value.is_empty())
        .map(|entry| {
            let (name, rest) = entry.rsplit_once(':').expect("descendant image and PID");
            let (pid, parent) = rest.split_once('<').expect("descendant parent PID");
            (
                name.to_owned(),
                pid.parse().expect("descendant PID"),
                parent.parse().expect("parent PID"),
            )
        })
        .collect()
}
