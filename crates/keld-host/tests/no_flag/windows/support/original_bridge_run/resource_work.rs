//! Resource-work operations on the existing CLI/control/profile owner.
//! Old captures and exact process handles stay in `OriginalRun` through final EOF.

use super::{Fixture, OriginalRun};
use crate::facade_window::fixture::{ephemeral_profile_dir, retained_descendants};
use crate::support::control::accept_control_or_host_failure;
use crate::support::process::{
    PIPE_DRAIN_KILL_SWITCH, PipeText, assert_process_signaled, open_process_for_wait,
    pipe_line_after, pipe_text_by, read_pipe_on_thread, wait_for_process_signal,
};
use crate::support::profile_run::actual_webview_process;
use crate::{PRODUCT_DEADLINE, startup_kill_switch};
use serde_json::Value;
use std::path::Path;
use std::time::Instant;
use windows_sys::Win32::Foundation::WAIT_TIMEOUT;

impl OriginalRun {
    pub(crate) fn start_resource(cli: &Path, project: &Path, directory: &Path) -> Self {
        Self::start_with_native_hold(cli, project, Fixture::ResourceHold, Some(directory))
    }

    pub(crate) fn crash_and_reconnect_resource(&mut self, window: u64) -> u32 {
        let old_pid = u32::try_from(self.hello["bunPid"].as_u64().expect("actual old Bun"))
            .expect("old Bun pid width");
        self.send("CRASH");
        let ack = self.record("CRASH_ACK ");
        assert_eq!(ack["bunPid"], old_pid);
        assert_eq!(ack["id"], window);
        let old = self
            .retained
            .iter()
            .find(|(pid, _)| *pid == old_pid)
            .expect("old Bun HANDLE retained before requesting its exit");
        assert_process_signaled(&old.1, "actual previous resource-fixture Bun");
        let old_text = pipe_text_by(&self.control, Instant::now() + PIPE_DRAIN_KILL_SWITCH);
        assert!(
            !old_text.contains("<pipe still open") && !old_text.contains("<pipe read failed:"),
            "old control must reach clean EOF before replacement: {old_text}"
        );

        let deadline = Instant::now() + startup_kill_switch();
        let socket = accept_control_or_host_failure(&self.listener, self.cli.child_mut(), deadline);
        let writer = socket.try_clone().expect("replacement control writer");
        let control = read_pipe_on_thread(socket);
        let old_control = std::mem::replace(&mut self.control, control);
        self.retired_controls.push((old_pid, old_control));
        self.writer = writer;
        self.control_after = 0;
        let hello = self.record_after("HELLO ", 0, deadline);
        let next_pid = u32::try_from(hello["bunPid"].as_u64().expect("actual replacement Bun"))
            .expect("replacement Bun pid width");
        assert_ne!(next_pid, old_pid, "actual replacement process identity");
        self.cli.observe_bun(next_pid);
        self.retained
            .push((next_pid, open_process_for_wait(next_pid, false)));
        self.hello = hello;
        let ready = self.record_after("READY ", 0, deadline);
        assert_eq!(
            ready["windows"], 1,
            "same retained Host window after Bun loss"
        );
        assert_eq!(ready["windowIds"], serde_json::json!([window]));
        let host = self
            .retained
            .iter()
            .find(|(pid, _)| *pid == self.host_pid)
            .expect("same original Host HANDLE");
        assert_eq!(
            wait_for_process_signal(&host.1, 0),
            WAIT_TIMEOUT,
            "replacement Bun uses the still-live retained Host"
        );
        next_pid
    }

    pub(crate) fn retain_resource_native_tree(&mut self) {
        assert!(self.preclosed_browser_pid.is_none());
        let profile = ephemeral_profile_dir(self.profiles.path());
        let browser = actual_webview_process(
            self.host_pid,
            &self.stage_host,
            &profile.join("webview2"),
            Instant::now() + PRODUCT_DEADLINE,
        );
        let pid = u32::try_from(browser["browser_pid"].as_u64().expect("actual browser pid"))
            .expect("browser pid width");
        self.retained
            .extend(retained_descendants(self.cli.child_mut()));
        assert!(
            self.retained.iter().any(|(actual, _)| *actual == pid),
            "actual browser HANDLE must be retained before the last controller Close"
        );
        self.preclosed_browser_pid = Some(pid);
    }

    pub(crate) fn resource_control_record(&mut self, prefix: &str) -> serde_json::Value {
        // Independent tags may precede one another. Preserve the existing
        // parser/diagnostics without consuming an earlier genuine observation.
        self.record_after(prefix, 0, Instant::now() + startup_kill_switch())
    }

    pub(crate) fn resource_record(
        &mut self,
        observed: &mut ResourceTrace,
        kind: &str,
        phase: &str,
    ) -> serde_json::Value {
        let deadline = Instant::now() + startup_kill_switch();
        loop {
            observed.poll(&self.stderr);
            if let Some(record) = observed
                .records
                .iter()
                .find(|record| record.kind == kind && record.value["phase"] == phase)
            {
                return record.value.clone();
            }
            self.assert_live("actual resource owner observation", deadline);
            std::thread::park_timeout(std::time::Duration::from_millis(10));
        }
    }
}

const HOLD: &str = "KELD_ORIGINAL_RESOURCE_HOLD ";
const CUSTODY: &str = "KELD_ORIGINAL_RESOURCE_CUSTODY ";

/// Bounded copies of existing stderr rows; this owns no process, ticket or native object.
pub(crate) struct ResourceRecord {
    pub(crate) end: usize,
    pub(crate) kind: &'static str,
    pub(crate) value: Value,
}

#[derive(Default)]
pub(crate) struct ResourceTrace {
    hold_after: usize,
    custody_after: usize,
    pub(crate) records: Vec<ResourceRecord>,
}

fn checked_row(kind: &'static str, prefix: &str, text: &str, end: usize) -> ResourceRecord {
    let json = text
        .strip_prefix(prefix)
        .expect("actual resource trace prefix");
    assert!(json.len() <= 32768, "bounded resource observation");
    let value: Value = serde_json::from_str(json).expect("actual resource trace JSON");
    if kind == "custody"
        && value["phase"] == "pending-dropped"
        && value["prior"]["ledger"]["workerActive"] == true
    {
        panic!(
            "X14_EARLY_PENDING_DROP: actual Pending was dropped while its same worker ticket remained active: {value}"
        );
    }
    assert!(
        kind != "hold" || value["phase"] != "expired",
        "X14_RESOURCE_HOLD_EXPIRED: {value}"
    );
    ResourceRecord { end, kind, value }
}

impl ResourceTrace {
    fn poll(&mut self, capture: &PipeText) {
        // Observe the decisive forbidden disposal before a generic process/liveness assertion.
        for (kind, prefix, cursor) in [
            ("custody", CUSTODY, &mut self.custody_after),
            ("hold", HOLD, &mut self.hold_after),
        ] {
            while let Some((line, end)) = pipe_line_after(capture, prefix, *cursor) {
                *cursor = end;
                assert!(
                    self.records.len() < 64,
                    "bounded same-ticket resource trace"
                );
                self.records.push(checked_row(kind, prefix, &line, end));
            }
        }
    }

    pub(crate) fn from_complete(trace: &str) -> Self {
        let mut observed = Self::default();
        let mut end = 0;
        for line in trace.split_inclusive('\n') {
            end += line.len();
            let text = line.trim_end_matches(['\r', '\n']);
            for (kind, prefix) in [("custody", CUSTODY), ("hold", HOLD)] {
                if text.starts_with(prefix) {
                    assert!(
                        observed.records.len() < 64,
                        "bounded complete resource trace"
                    );
                    observed.records.push(checked_row(kind, prefix, text, end));
                }
            }
        }
        observed
    }
}
