//! Selected-diagnostic operations reuse the same `OriginalRun` owners.
use super::{Fixture, OriginalRun, QueuedCloseTrace, ephemeral_profile_dir};
use crate::PRODUCT_DEADLINE;
use crate::facade_window::fixture::{capture, census};
use crate::support::process::wait_for_process_signal;
use crate::support::window::NativeWindowSnapshot;
use std::net::Shutdown;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;

pub(crate) struct DiagnosticPhysicalOwners {
    profile: PathBuf,
    window: NativeWindowSnapshot,
}

impl OriginalRun {
    pub(crate) fn start_selected_diagnostic(cli: &Path, project: &Path, directory: &Path) -> Self {
        Self::start_with_native_hold(cli, project, Fixture::SelectedDiagnostic, Some(directory))
    }

    /// Capture the actual browser family before `LOAD` starts the `Packet` deadline.
    pub(crate) fn retain_selected_diagnostic_owners(&mut self) -> DiagnosticPhysicalOwners {
        self.retain_resource_native_tree();
        let windows = census(self.host_pid, 1);
        let window = capture(windows[0], self.host_pid);
        assert!(
            !window.children.is_empty(),
            "actual retained native controller children"
        );
        let profile = ephemeral_profile_dir(self.profiles.path());
        println!(
            "KELD_ORIGINAL_DIAGNOSTIC_AT_RISK {}",
            serde_json::json!({
                "host":self.host_pid, "sentinel":self.sentinel_pid,
                "bun":self.hello["bunPid"], "hwnd":window.hwnd,
                "children":window.children, "profile":profile,
                "retainedProcessHandles":self.retained.len(),
                "filesystemIdentityCapability":false,
            })
        );
        DiagnosticPhysicalOwners { profile, window }
    }

    /// A saved actual `HANDLE` must signal; `CRASH_ACK` alone is not process death.
    pub(crate) fn selected_diagnostic_child_exited(&self) {
        // The caller already validated CRASH_ACK. Keep capture Read alive while
        // finishing the parent send direction before waiting for actual death.
        self.writer
            .shutdown(Shutdown::Write)
            .expect("finish the existing fixture control send direction");
        let bun = u32::try_from(self.hello["bunPid"].as_u64().expect("actual Bun pid"))
            .expect("actual pid width");
        let (_, handle) = self
            .retained
            .iter()
            .find(|(pid, _)| *pid == bun)
            .expect("original Bun HANDLE retained at startup");
        let budget = u32::try_from(PRODUCT_DEADLINE.as_millis()).expect("bounded wait width");
        assert_eq!(
            wait_for_process_signal(handle, budget),
            WAIT_OBJECT_0,
            "actual original Bun exit"
        );
        println!("KELD_ORIGINAL_DIAGNOSTIC_CHILD_RETIRED bun={bun}");
    }

    pub(crate) fn finish_selected_diagnostic_failure(
        self,
        owners: DiagnosticPhysicalOwners,
        inspect: impl FnOnce(&str, &str),
    ) {
        let hwnd = owners.window.hwnd;
        self.finish_retained_failure(
            &owners.profile,
            &[owners.window],
            (
                "KELD_ORIGINAL_DIAGNOSTIC_END",
                "KELD_ORIGINAL_DIAGNOSTIC_PROFILE_RETAINED",
            ),
            |status, out, trace, control| {
                assert_selected_failure(status, out, trace, control, hwnd);
            },
            inspect,
        );
    }
}

fn assert_selected_failure(status: ExitStatus, out: &str, trace: &str, control: &str, hwnd: usize) {
    for (name, captured) in [("stdout", out), ("stderr", trace), ("control", control)] {
        assert!(
            !captured.contains("<pipe still open"),
            "actual {name} EOF missing"
        );
        assert!(
            !captured.contains("<pipe read failed:"),
            "actual {name} read failure"
        );
    }
    assert!(
        !status.success(),
        "selected diagnostic ambiguity remains unqualified"
    );
    assert!(
        selected_native_tail_matches(trace, hwnd),
        "unrelated native failure cannot qualify the exact retained-source tail: {trace}"
    );
    assert!(
        trace.contains("KELD_T2_MAIN_CUSTODY retained=true"),
        "actual retained run owner"
    );
    assert!(
        !trace.contains("KELD_T2_RELEASE_WITNESS completed=true"),
        "unqualified source grants no release credit"
    );
    assert!(
        !trace.contains("packet-hold-failed"),
        "original physical Packet deadline/metadata must not fail"
    );
    assert!(
        !control.contains("FIXTURE_ERROR "),
        "fixed source stimulus failed"
    );
    assert!(
        !control.contains("IPC_EFFECT "),
        "held original packet must not reach its dead Bun consumer"
    );
}

// The selected error remains in the retained Core owner. Match only the actual
// same-window native tail, with the direct Core refusal observed before it.
fn selected_native_tail_matches(trace: &str, hwnd: usize) -> bool {
    if trace.contains("KELD_WV_NATIVE_OWNER_FAILED")
        || trace.contains("original bridge wake failed")
    {
        return false;
    }
    let records = QueuedCloseTrace::from_complete(trace);
    let roles: Vec<_> = records
        .records
        .iter()
        .filter(|row| row.kind == "selected" && row.value["phase"] == "role-retirement-return")
        .collect();
    let [role] = roles.as_slice() else {
        return false;
    };
    if role.value["retirement_failed"] != true
        || role.value["retirement_refusal"] != "selected-diagnostic"
    {
        return false;
    }
    let expiries: Vec<_> = trace
        .match_indices("KELD_WV_RELEASING_EXPIRED ")
        .filter(|(at, _)| *at == 0 || trace.as_bytes()[*at - 1] == b'\n')
        .collect();
    let [(expired_at, _)] = expiries.as_slice() else {
        return false;
    };
    let expected = format!("KELD_WV_RELEASING_EXPIRED hwnd={hwnd}");
    if trace[*expired_at..].lines().next() != Some(expected.as_str()) {
        return false;
    }
    let tails: Vec<_> = trace
        .lines()
        .filter(|line| line.starts_with("KELD-CORE-"))
        .collect();
    let [tail] = tails.as_slice() else {
        return false;
    };
    let Some(tail_at) = trace.find(*tail) else {
        return false;
    };
    tail.starts_with("KELD-CORE-037: no-flag host failed during Windows main presentation tail \u{2014} KELD-WV-009:")
        && tail.contains("Observed: a host window's native teardown was not confirmed within the releasing bound.")
        && trace.matches("KELD-WV-").count() == 1
        && role.end <= *expired_at
        && *expired_at < tail_at
}

#[test]
fn saved_native_tail_rejects_missing_foreign_and_unrelated_failures() {
    // Saved actual v4 projection; this component unit grants no native credit.
    const SAVED: &str = r#"KELD_X10_SELECTED_DIAGNOSTIC {"main_to_renderer":{"consumer_done":true,"event_release_sent":false,"message":6,"native_done":true,"native_retirement":"outcome-unknown","native_storage_id":2330016362992,"native_storage_released":false,"navigation":2,"pending_reply":6,"primary_write":false,"reader_stop_id":2329965636624,"reader_stopped":true,"retained":2026,"retired":true,"retirement_write":false,"selected":{"canonical_code":true,"code_bytes":15,"code_id":2329998601888,"message":6,"message_bytes":26,"message_id":2330016354720,"navigation":2,"reader_stop_id":2329965636624,"webview":1},"terminal_preparing":false,"webview":1},"phase":"role-retirement-return","renderer_to_main":{"consumer_done":false,"event_release_sent":false,"message":1,"native_done":true,"native_retirement":null,"native_storage_id":null,"native_storage_released":null,"navigation":2,"pending_reply":null,"primary_write":true,"reader_stop_id":2329965636624,"reader_stopped":true,"retained":308,"retired":true,"retirement_write":false,"selected":null,"terminal_preparing":false,"webview":1},"retirement_failed":true,"retirement_refusal":"selected-diagnostic","selected":null,"sequence":12,"value_full":false,"value_position":{"packet_in_slot":false,"phase":"complete","receipt_live":true,"serial":5,"target_id":2329965448352}}
KELD_WV_RELEASING_EXPIRED hwnd=49481786
KELD_T2_MAIN_CUSTODY retained=true
KELD-CORE-037: no-flag host failed during Windows main presentation tail — KELD-WV-009: profile selection failed: durable profile lifecycle state cannot prove safe reuse because recovery state cannot be proven on this platform/version. If persistent profiles are unsupported here, use explicit ephemeral mode; otherwise complete engine-release or boot recovery before lookup. Observed: a host window's native teardown was not confirmed within the releasing bound.. Fix the app session failure and relaunch the no-flag host. [startup-resource-attempts listener=1 child=1 window=0]
"#;
    let [role, expiry, custody, tail]: [&str; 4] = SAVED
        .split_inclusive('\n')
        .collect::<Vec<_>>()
        .try_into()
        .expect("four complete saved lines");
    let hwnd = 49_481_786;
    assert!(selected_native_tail_matches(SAVED, hwnd));
    assert!(!selected_native_tail_matches(SAVED, hwnd + 1));
    for changed in [
        SAVED.replace("\"retirement_refusal\":\"selected-diagnostic\"", "\"retirement_refusal\":null"),
        SAVED.replace("\"retirement_refusal\":\"selected-diagnostic\"", "\"retirement_refusal\":\"selected-terminal\""),
        SAVED.replace("KELD-WV-009:", "KELD-WV-004:"),
        SAVED.replace("Observed: a host window's native teardown was not confirmed within the releasing bound.", "Observed: an unrelated native failure."),
        format!("{SAVED}KELD_WV_NATIVE_OWNER_FAILED original bridge wake failed\n"),
        format!("{SAVED}KELD-WV-004: an unrelated native failure\n"),
        format!("{SAVED}KELD_WV_RELEASING_EXPIRED hwnd={hwnd}\n"),
        format!("{expiry}{role}{custody}{tail}"),
        format!("{role}{tail}{custody}{expiry}"),
    ] {
        assert!(!selected_native_tail_matches(&changed, hwnd));
    }
}
