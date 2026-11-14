//! Shared expected-failure finish on the existing CLI/HANDLE/profile owners.
//! Kernel retirement permits fixture disposal, never product native-release credit.
use super::OriginalRun;
use crate::facade_window::fixture::assert_retired;
use crate::support::process::{
    PIPE_DRAIN_KILL_SWITCH, assert_process_signaled, pipe_text_by, retained_process_exit_code,
    wait_process_gone,
};
use crate::support::window::NativeWindowSnapshot;
use crate::{PRODUCT_DEADLINE, ui_end_kill_switch, wait_for_dev_stage_count};
use std::fs;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::process::ExitStatus;
use std::time::Instant;

impl OriginalRun {
    pub(super) fn finish_retained_failure(
        self,
        profile: &Path,
        windows: &[NativeWindowSnapshot],
        tags: (&'static str, &'static str),
        classify: impl FnOnce(ExitStatus, &str, &str, &str),
        inspect: impl FnOnce(&str, &str),
    ) {
        let Self {
            cli,
            listener,
            writer,
            control,
            stdout,
            stderr,
            retained,
            profiles,
            project,
            sentinel_pid,
            ..
        } = self;
        let profiles = ManuallyDrop::new(profiles);
        let status = cli.wait(Instant::now() + ui_end_kill_switch());
        wait_for_dev_stage_count(&project, 0, Instant::now() + PRODUCT_DEADLINE);
        wait_process_gone(sentinel_pid, Instant::now() + PRODUCT_DEADLINE);
        for (pid, handle) in &retained {
            assert_process_signaled(handle, &format!("actual retained-failure descendant {pid}"));
        }
        let (_, sentinel) = retained
            .iter()
            .find(|(pid, _)| *pid == sentinel_pid)
            .expect("same original cleanup sentinel HANDLE");
        let sentinel_exit = retained_process_exit_code(sentinel);
        assert_eq!(
            sentinel_exit, 0,
            "actual existing sentinel success follows exact non-breakaway attempt Job zero and stage deletion"
        );
        for window in windows {
            assert_retired(window);
        }
        crate::support::control::assert_no_control_connection(
            &listener,
            "no successor after an explicitly unqualified retained failure",
        );
        drop(writer);
        let deadline = Instant::now() + PIPE_DRAIN_KILL_SWITCH;
        let out = pipe_text_by(&stdout, deadline);
        let trace = pipe_text_by(&stderr, deadline);
        let control = pipe_text_by(&control, deadline);
        println!(
            "{} status={status}\nstdout:\n{out}\nstderr:\n{trace}\ncontrol:\n{control}",
            tags.0
        );
        classify(status, &out, &trace, &control);
        let retained_profile = fs::symlink_metadata(profile)
            .expect("same captured nonce path must remain after retained failure");
        assert!(
            retained_profile.is_dir(),
            "retained nonce path changed type"
        );
        println!(
            "{} {}",
            tags.1,
            serde_json::json!({
                "profile":profile, "productDeletion":false,
                "filesystemIdentityCapability":false, "observedPhysicalOwnersRetired":true,
                "sentinelPid":sentinel_pid, "sentinelExitCode":sentinel_exit,
            })
        );
        // Validate the complete independent source/native oracle before fixture
        // disposal. Any refusal above or here retains the original profile evidence.
        inspect(&trace, &control);
        ManuallyDrop::into_inner(profiles).remove();
    }
}
