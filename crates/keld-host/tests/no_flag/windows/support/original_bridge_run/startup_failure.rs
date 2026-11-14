//! Expected startup failure on the same actual CLI, pipe, HANDLE and profile owners.
//! Kernel/process teardown is physical proof only, never a native release receipt.
use super::{OriginalRun, ephemeral_profile_dir, retained_descendants};
use crate::PRODUCT_DEADLINE;
use crate::facade_window::fixture::{capture, census};
use crate::support::profile_run::actual_webview_process;
use crate::support::window::NativeWindowSnapshot;
use std::fs;
use std::mem::ManuallyDrop;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Instant;

impl OriginalRun {
    fn startup_physical_owners(
        &mut self,
        view: u64,
    ) -> (PathBuf, NativeWindowSnapshot, NativeWindowSnapshot) {
        let profile = ephemeral_profile_dir(self.profiles.path());
        assert!(
            fs::symlink_metadata(&profile)
                .expect("actual nonce leaf before LOAD")
                .is_dir(),
            "captured nonce path is not a directory"
        );
        let browser = actual_webview_process(
            self.host_pid,
            &self.stage_host,
            &profile.join("webview2"),
            Instant::now() + PRODUCT_DEADLINE,
        );
        let browser_pid =
            u32::try_from(browser["browser_pid"].as_u64().expect("actual browser pid"))
                .expect("browser pid fits u32");
        let windows = census(self.host_pid, 1);
        let before = capture(windows[0], self.host_pid);
        assert!(
            !before.children.is_empty(),
            "actual controller hosting children"
        );
        self.retained
            .extend(retained_descendants(self.cli.child_mut()));
        println!(
            "KELD_ORIGINAL_STARTUP_AT_RISK {}",
            serde_json::json!({
                "phase":"before-load","host":self.host_pid,"browser":browser,
                "hwnd":before.hwnd,"children":before.children,"profile":profile,
                "association":"captured nonce path and browser command",
                "filesystemIdentityCapability":false,
            })
        );
        self.send("LOAD");
        // Existing sole readback owner/poll; no new timer or native completion.
        // Retain the actual post-navigation descendants before the real deadline.
        self.page_when(view, |page| {
            page["bindMessagesSent"] == 2 && page["domObserved"] == true
        });
        let loaded = capture(
            isize::try_from(before.hwnd).expect("original HWND"),
            self.host_pid,
        );
        self.retained
            .extend(retained_descendants(self.cli.child_mut()));
        let bun_pid = u32::try_from(self.hello["bunPid"].as_u64().expect("actual Bun pid"))
            .expect("Bun pid fits u32");
        for expected in [self.host_pid, self.sentinel_pid, browser_pid, bun_pid] {
            assert!(
                self.retained.iter().any(|(pid, _)| *pid == expected),
                "actual descendant HANDLE missing for {expected}"
            );
        }
        println!(
            "KELD_ORIGINAL_STARTUP_AT_RISK {}",
            serde_json::json!({
                "phase":"after-probe","host":self.host_pid,"sentinel":self.sentinel_pid,
                "bun":bun_pid,"browserPid":browser_pid,"hwnd":loaded.hwnd,
                "children":loaded.children,"retainedProcessHandles":self.retained.len(),
            })
        );
        (profile, before, loaded)
    }

    /// The caller created the actual view without navigating. This same owner
    /// captures physical observations before and after LOAD without replacing it.
    pub(crate) fn finish_missing_registration(
        mut self,
        view: u64,
        inspect: impl FnOnce(&str, &str),
    ) {
        let observed = catch_unwind(AssertUnwindSafe(|| self.startup_physical_owners(view)));
        let (profile, before, loaded) = match observed {
            Ok(value) => value,
            Err(cause) => {
                let Self { profiles, .. } = self;
                let profiles = ManuallyDrop::new(profiles);
                eprintln!(
                    "KELD_ORIGINAL_STARTUP_UNPROVED retained_fixture={}",
                    profiles.path().display()
                );
                // Remaining original fields keep their existing failure Drop
                // order; the signed Child guard is emergency cleanup, never proof.
                resume_unwind(cause);
            }
        };
        self.finish_retained_failure(
            &profile,
            &[before, loaded],
            (
                "KELD_ORIGINAL_STARTUP_END",
                "KELD_ORIGINAL_STARTUP_PROFILE_RETAINED",
            ),
            assert_retained_failure,
            inspect,
        );
    }
}

fn assert_retained_failure(status: ExitStatus, out: &str, trace: &str, control: &str) {
    for (name, captured) in [("stdout", out), ("stderr", trace), ("control", control)] {
        assert!(
            !captured.contains("<pipe still open"),
            "actual {name} EOF missing"
        );
        assert!(
            !captured.contains("<pipe read failed:"),
            "actual {name} read failed"
        );
    }
    assert!(
        !status.success(),
        "missing registration must fail actual CLI startup"
    );
    let failures: Vec<_> = trace
        .lines()
        .filter(|line| line.starts_with("KELD_WV_NATIVE_OWNER_FAILED "))
        .collect();
    assert_eq!(failures.len(), 1, "one actual typed native owner failure");
    assert!(
        failures[0].contains("actual document binding receipt timed out"),
        "unrelated startup failure: {failures:?}"
    );
    assert_eq!(
        trace
            .lines()
            .filter(|line| *line == "KELD_T2_MAIN_CUSTODY retained=true")
            .count(),
        1,
        "actual same retained-run assignment"
    );
    assert!(
        !trace.contains("KELD_T2_RELEASE_WITNESS completed=true"),
        "retained startup fault must not claim native release"
    );
}
