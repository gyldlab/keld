//! BOOT8 lease refusal against the stage held by the existing shipping CLI owner.
//! This covers only missing/unsupported lease routing, not graph or recovery hostility.
use super::OriginalRun;
use crate::support::control::assert_no_control_connection;
use crate::support::process::lineage::terminal_descendant_candidates;
use crate::support::process::{
    PIPE_DRAIN_KILL_SWITCH, PipeText, assert_process_signaled, open_process_for_wait, pipe_text_by,
    process_times, process_tree_report, read_pipe_on_thread,
};
use crate::support::profile_run::{ProfileTestRoot, assert_profile_test_root_feature};
use crate::support::signed_process::SignedStateProcessGuard;
use crate::support::window::app_top_level_windows;
use crate::{PRODUCT_DEADLINE, dev_stage_count};
use serde_json::Value;
use std::fs;
use std::mem::ManuallyDrop;
use std::net::TcpListener;
use std::os::windows::io::AsRawHandle as _;
use std::process::{Command, Stdio};
use std::time::Instant;

impl OriginalRun {
    pub(crate) fn assert_boot8_lease_refusals(&mut self) {
        let stage = self
            .stage_host
            .parent()
            .expect("actual CLI-owned stage")
            .to_owned();
        let boot = fs::read(stage.join("keld.boot.json")).expect("actual public descriptor");
        let descriptor: Value = serde_json::from_slice(&boot).expect("public descriptor JSON");
        assert_eq!(
            descriptor["schema"], 8,
            "do not substitute the generic public stage helper"
        );
        let host = fs::read(&self.stage_host).expect("actual staged Host bytes");
        for (label, lease, code, detail) in [
            ("missing", None, "KELD-WV-009", "WinVerifyTrust rejected"),
            (
                "unsupported",
                Some("stdin-v2"),
                "KELD-CORE-037",
                "unsupported KELD_DEV_LEASE value",
            ),
        ] {
            self.assert_live(
                "before BOOT8 refusal probe",
                Instant::now() + PRODUCT_DEADLINE,
            );
            assert_eq!(
                dev_stage_count(&self.project),
                1,
                "same retained public stage"
            );
            self.assert_boot8_lease_refusal(label, lease, code, detail);
            assert_eq!(
                fs::read(stage.join("keld.boot.json")).expect("retained descriptor"),
                boot
            );
            assert_eq!(
                fs::read(&self.stage_host).expect("retained Host bytes"),
                host
            );
            self.assert_live(
                "after BOOT8 refusal probe",
                Instant::now() + PRODUCT_DEADLINE,
            );
            assert_eq!(
                dev_stage_count(&self.project),
                1,
                "probe did not retire the real stage"
            );
        }
        println!(
            "KELD_BOOT8_LEASE_INPUT {}",
            serde_json::json!({
                "stage":stage, "schema":descriptor["schema"], "descriptor_bytes":boot.len(),
                "host_bytes":host.len(), "descriptor_and_host_unchanged":true,
            })
        );
    }

    fn assert_boot8_lease_refusal(
        &mut self,
        label: &str,
        lease: Option<&str>,
        code: &str,
        detail: &str,
    ) {
        // Keep failed profile evidence; dispose only after actual exit and an empty-root check.
        let profiles = ManuallyDrop::new(ProfileTestRoot::new());
        assert_profile_test_root_feature(&self.stage_host, profiles.path());
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("probe control observer");
        let mut command = Command::new(&self.stage_host);
        command
            .current_dir(self.stage_host.parent().expect("stage"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("KELD_PROFILE_TEST_ROOT", profiles.path())
            .env("TEMP", profiles.path().parent().expect("probe temp"))
            .env("TMP", profiles.path().parent().expect("probe temp"))
            .env(
                "KELD_T1B_CONTROL",
                listener.local_addr().expect("port").port().to_string(),
            )
            .env_remove("KELD_DEV_LEASE")
            .env_remove("KELD_TEST_WINDOWS_ADMIT_FACADE_BOOT")
            .env_remove("KELD_TEST_WINDOWS_FAIL_BUILD")
            .env_remove("KELD_TEST_WINDOWS_NATIVE_HOLD")
            .env_remove("KELD_TEST_WINDOWS_HANG_GUARD_SECS")
            .env_remove("KELD_TEST_WINDOWS_BARRIER_GRACE_EXPIRY")
            .env_remove("KELD_TEST_WINDOWS_WITHHOLD_LATER_GENERATION_RECEIPT");
        if let Some(value) = lease {
            command.env("KELD_DEV_LEASE", value);
        }
        let mut probe = SignedStateProcessGuard::new(command.spawn().expect("staged no-flag Host"));
        let pid = probe.host_pid();
        let retained = open_process_for_wait(pid, false);
        let birth = process_times(retained.as_raw_handle().cast()).creation;
        let stdout = read_pipe_on_thread(probe.child_mut().stdout.take().expect("probe stdout"));
        let stderr = read_pipe_on_thread(probe.child_mut().stderr.take().expect("probe stderr"));
        let status = probe.wait(Instant::now() + PRODUCT_DEADLINE);
        assert_process_signaled(&retained, "actual lease-refused Host");
        let deadline = Instant::now() + PIPE_DRAIN_KILL_SWITCH;
        let out = ended(&stdout, deadline);
        let trace = ended(&stderr, deadline);
        println!(
            "KELD_BOOT8_LEASE_PROBE {label} pid={pid} birth={birth} status={status}\nstdout:\n{out}\nstderr:\n{trace}"
        );
        assert!(
            !status.success(),
            "{label} unexpectedly entered the application"
        );
        assert!(
            trace.contains(code) && trace.contains(detail),
            "{label}: {trace}"
        );
        assert!(
            !trace.contains("KELD-CORE-035"),
            "the stage parser must not supply this refusal"
        );
        assert!(trace.contains("listener=0 child=0 window=0"), "{trace}");
        assert_no_control_connection(&listener, "refused Host must not start an app control peer");
        let tree = process_tree_report(pid);
        assert!(tree.contains("host_alive=False"), "{tree}");
        let remaining = terminal_descendant_candidates(&tree, pid, birth);
        assert!(
            remaining.is_empty(),
            "refused Host left descendant candidates: {tree}"
        );
        assert!(
            app_top_level_windows(pid).is_empty(),
            "refused Host owns a native app window"
        );
        assert!(
            fs::read_dir(profiles.path())
                .expect("actual isolated profile root")
                .next()
                .is_none(),
            "refused Host created profile state; retain it for root inspection"
        );
        println!(
            "KELD_BOOT8_LEASE_REFUSAL {}",
            serde_json::json!({
                "case":label, "pid":pid, "birth":birth, "exit_code":status.code(),
                "profile_root":profiles.path(), "profile_entries":0, "terminal_process_census":tree,
                "control_connections":0, "terminal_app_windows":0,
            })
        );
        ManuallyDrop::into_inner(profiles).remove();
    }
}

fn ended(pipe: &PipeText, deadline: Instant) -> String {
    let text = pipe_text_by(pipe, deadline);
    assert!(
        !text.contains("<pipe still open"),
        "probe capture did not reach EOF: {text}"
    );
    assert!(
        !text.contains("<pipe read failed:"),
        "probe capture failed: {text}"
    );
    text
}
