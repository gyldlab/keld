//! Baseline regression cleanup: an unsupported constructor remains a failure.
//! Retire that known zero-window T2 session normally before the intended assertion.

use super::fixture::{FacadeFixture, FacadeRun};
use crate::support::window::app_top_level_windows;
use crate::ui_end_kill_switch;
use serde_json::Value;
use std::time::Instant;

pub(super) fn create_or_retire_unsupported(
    mut run: FacadeRun,
    fixture: &FacadeFixture,
) -> (FacadeRun, Value) {
    run.send("CREATE");
    let (kind, created) = run.next();
    if kind == "CREATE_ERROR" && created["code"] == "KELD-CORE-042" {
        let pid = run.pid();
        let profile = fixture.profile_dir();
        assert_eq!(
            app_top_level_windows(pid),
            [],
            "unsupported constructor published a native window"
        );
        assert_eq!(
            created["windows"], 0,
            "unsupported constructor published a partial facade"
        );
        run.retain_descendants();
        run.send("QUIT");
        run.read("QUIT_ACCEPTED");
        let (status, trace) = run.finish(Instant::now() + ui_end_kill_switch());
        assert!(
            status.success(),
            "unsupported baseline failed its ordinary Quit cleanup: {status}\n{trace}"
        );
        assert!(
            trace.contains("KELD_T2_LOOP_EXIT environments=1 controllers=0 armed_by=Quit"),
            "baseline unexpectedly constructed a controller or missed normal Quit:\n{trace}"
        );
        assert!(
            trace.contains("KELD_T2_RELEASE_WITNESS completed=true"),
            "baseline cleanup lacked its qualified native release witness:\n{trace}"
        );
        assert_eq!(app_top_level_windows(pid), []);
        assert!(
            !profile.exists(),
            "baseline cleanup left its owned ephemeral profile"
        );
        println!(
            "KELD_GH1188_BASELINE_RED_CLEANUP host_pid={pid} windows=0 retained_descendants_signaled=true profile_removed=true"
        );
        panic!(
            "first post-Ready BrowserWindow must construct a real native window; actual {kind} {created}"
        );
    }
    assert_eq!(
        kind, "PUBLIC_CREATED",
        "actual first constructor response: {kind} {created}"
    );
    println!("KELD_GH1188_CREATE {created}");
    (run, created)
}
