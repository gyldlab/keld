//! Installed facade browser-instance generations and persistent release credit.

use super::facade_browser_fixture::{
    FinishedRun, InstalledFacade, InstalledRun, LATER_RECEIPT_WITHHELD, browser_count, field,
    number,
};
use super::sessions::LifecycleObservation;
use crate::facade_window::fixture::{assert_retired, capture, census};
use crate::support::process::{
    assert_process_signaled, assert_process_signaled_within, process_tree_report,
};
use crate::support::window::{NativeWindowSnapshot, post_close};
use crate::{PRODUCT_DEADLINE, ui_end_kill_switch};
use serde_json::Value;
use std::time::{Duration, Instant};

pub(super) const HOST_GRACE_FAULT: &str = "the host browser process exited without raising BrowserProcessExited within the post-exit grace";
pub(super) const HOST_ABNORMAL_FAULT: &str = "the host browser process failed or exited abnormally";
pub(super) const HOST_UNIDENTIFIED_FAULT: &str =
    "a host WebView2 controller exists but its browser process could not be identified";

pub(super) struct LiveWindow {
    pub(super) id: u32,
    pub(super) value: Value,
    pub(super) native: NativeWindowSnapshot,
}

pub(super) fn create(run: &mut InstalledRun<'_>) -> LiveWindow {
    run.send("CREATE");
    let value = run.read("PUBLIC_CREATED");
    assert_eq!(value["live"], true);
    assert_eq!(value["liveCount"], 1);
    assert_eq!(value["creation"], serde_json::json!(["contents", "window"]));
    let native = capture(census(run.pid(), 1)[0], run.pid());
    assert!(
        !native.children.is_empty(),
        "real WebView2 child HWNDs must exist"
    );
    LiveWindow {
        id: number(&value, "id"),
        value,
        native,
    }
}

pub(super) fn destroy(run: &mut InstalledRun<'_>, window: &LiveWindow) {
    run.send(&format!("DESTROY {}", window.id));
    let closed = run.read("CLOSED");
    assert_eq!(number(&closed, "id"), window.id);
    assert_eq!(closed["count"], 1);
    let returned = run.read("DESTROY_RETURN");
    let finished = run.read("DESTROY_FINISHED");
    assert_eq!(returned["live"], false);
    assert_eq!(finished["liveCount"], 0);
    assert_eq!(finished["closed"], 1);
    assert_eq!(finished["contentsDestroyed"], 1);
    assert_retired(&window.native);
    assert_eq!(census(run.pid(), 0), Vec::<isize>::new());
}

pub(super) fn allow_close(run: &mut InstalledRun<'_>, window: &LiveWindow) {
    run.send(&format!("ALLOW {}", window.id));
    assert_eq!(number(&run.read("ALLOW"), "id"), window.id);
    post_close(u64::try_from(window.native.hwnd).expect("observed HWND"));
    let requested = run.read("CLOSE_EVENT");
    assert_eq!(number(&requested, "id"), window.id);
    assert_eq!(requested["count"], 1);
    let closed = run.read("CLOSED");
    assert_eq!(number(&closed, "id"), window.id);
    assert_eq!(closed["count"], 1);
    assert_retired(&window.native);
    assert_eq!(census(run.pid(), 0), Vec::<isize>::new());
}

pub(super) fn healthy_recovery(fixture: &InstalledFacade) {
    let profile = fixture.profile_dir();
    let mut run = fixture.start(&[]);
    assert_ready_empty(fixture, &run);
    let window = create(&mut run);
    let browser = run.browser(None, false);
    run.marker(
        &format!("REBIND gen=1 pid={}", browser.pid),
        0,
        Instant::now() + PRODUCT_DEADLINE,
    );
    run.retain_descendants();
    allow_close(&mut run, &window);
    run.send("QUIT");
    run.read("QUIT_ACCEPTED");
    let finished = run.finish();
    assert_process_signaled(&browser.handle, "healthy recovery browser");
    assert_retired(&window.native);
    assert_idle(fixture, &finished);
    assert_eq!(
        fixture.profile_dir(),
        profile,
        "recovery uses the same authenticated profile path"
    );
}

#[test]
#[ignore = "signed exact-head profile-hook/root Host and installer required"]
fn windows_installed_managed_browser_instances_preserve_independent_generation_credit() {
    let fixture = InstalledFacade::new();
    let mut same_pid = 0;
    let mut overlap = 0;
    let mut clean_new = 0;
    // Timing selects the specified stimulus, not the classification. Actual
    // process objects and kernel creation/exit times determine each regime.
    for attempt in 1..=4 {
        for gap in [0, 150, 400] {
            let mut run = fixture.start(&[]);
            assert_ready_empty(&fixture, &run);
            let first = create(&mut run);
            let first_browser = run.browser(None, false);
            let (_, first_cursor) = run.marker(
                &format!("REBIND gen=1 pid={}", first_browser.pid),
                0,
                Instant::now() + PRODUCT_DEADLINE,
            );
            run.retain_descendants();
            run.send(&format!("REPLACE {} {gap}", first.id));
            assert_eq!(number(&run.read("CLOSED"), "id"), first.id);
            let value = run.read("REPLACED");
            assert_eq!(number(&value, "previous"), first.id);
            assert_eq!(value["previousGeneration"], first.value["generation"]);
            assert_eq!(value["gapMs"], gap);
            assert_eq!(value["live"], true);
            assert_eq!(value["liveCount"], 1);
            let id = number(&value, "id");
            assert_ne!(
                (id, value["generation"].clone()),
                (first.id, first.value["generation"].clone())
            );
            let second = LiveWindow {
                id,
                value,
                native: capture(census(run.pid(), 1)[0], run.pid()),
            };
            assert!(!second.native.children.is_empty());
            let second_browser = run.browser(Some(first_browser.pid), false);
            let first_exit = first_browser.exit();
            let generation = if second_browser.pid == first_browser.pid {
                1
            } else {
                2
            };
            let regime = if generation == 1 {
                assert_eq!(
                    first_exit, 0,
                    "same PID must still name the retained live object"
                );
                same_pid += 1;
                "same-live-instance"
            } else if first_exit != 0 && first_exit < second_browser.creation() {
                clean_new += 1;
                "clean-new-instance"
            } else {
                overlap += 1;
                "new-instance-overlap"
            };
            if generation == 2 {
                run.marker(
                    &format!("REBIND gen=2 pid={}", second_browser.pid),
                    first_cursor,
                    Instant::now() + PRODUCT_DEADLINE,
                );
            }
            run.retain_descendants();
            allow_close(&mut run, &second);
            run.send("QUIT");
            run.read("QUIT_ACCEPTED");
            let finished = run.finish();
            assert_idle(&fixture, &finished);
            assert_process_signaled(
                &first_browser.handle,
                "first browser object after replacement",
            );
            assert_process_signaled(
                &second_browser.handle,
                "second browser object after replacement",
            );
            assert_retired(&second.native);
            let bindings = finished
                .stderr
                .lines()
                .filter(|line| line.starts_with("REBIND "))
                .collect::<Vec<_>>();
            assert_eq!(
                bindings.len(),
                generation,
                "one checked binding per actual instance: {}",
                finished.stderr
            );
            assert_eq!(field(bindings[0], "gen"), 1);
            assert!(
                finished.stderr.contains(&format!(
                    "RECEIPT_CREDITED gen={generation} pid={}",
                    second_browser.pid,
                )),
                "current browser instance never earned release credit: {}",
                finished.stderr
            );
            println!(
                "KELD_GH1188_AC21_REGIME attempt={attempt} requested_gap_ms={gap} regime={regime} at_risk=1 first_pid={} first_exit={} second_pid={} second_creation={} hwnd_reused={}",
                first_browser.pid,
                first_exit,
                second_browser.pid,
                second_browser.creation(),
                first.native.hwnd == second.native.hwnd,
            );
        }
        if same_pid > 0 && clean_new > 0 {
            break;
        }
    }
    println!(
        "KELD_GH1188_AC21_REGIME_COUNTS same_live={same_pid} overlap={overlap} clean_new={clean_new}"
    );
    assert!(
        same_pid > 0,
        "AC21 regime (i) has zero eligible native observations"
    );
    assert!(
        clean_new > 0,
        "AC21 regime (iii) has zero eligible native observations"
    );
    // A zero overlap count is reported, not relabeled. The approved owner-level
    // AC18 NC4 census test remains the decisive proof for that nondeterministic half.
    healthy_recovery(&fixture);
    fixture.close();
}

#[test]
#[ignore = "signed exact-head profile-hook/root Host and installer required"]
fn windows_installed_current_browser_requires_its_own_normal_receipt() {
    let fixture = InstalledFacade::new();
    // The positive controls bracket the actual generation-two withholding cell.
    for withheld in [false, true, false] {
        let mut eligible = 0;
        let mut void = 0;
        for attempt in 1..=3 {
            let hooks = if withheld {
                vec![(LATER_RECEIPT_WITHHELD, "1")]
            } else {
                Vec::new()
            };
            let mut run = fixture.start(&hooks);
            assert_ready_empty(&fixture, &run);
            let first = create(&mut run);
            let first_browser = run.browser(None, false);
            run.marker(
                &format!("REBIND gen=1 pid={}", first_browser.pid),
                0,
                Instant::now() + PRODUCT_DEADLINE,
            );
            run.retain_descendants();
            destroy(&mut run, &first);
            let receipt = run.marker_if_delivered(
                &format!("RECEIPT_CREDITED gen=1 pid={}", first_browser.pid),
                0,
                Instant::now() + ui_end_kill_switch(),
            );
            let Some((_, credited)) = receipt else {
                void += 1;
                run.capture_control();
                run.send("QUIT");
                let finished = run.finish();
                println!(
                    "KELD_GH1188_AC21_NC3_VOID withheld={withheld} attempt={attempt} cause=prior-receipt-not-observed phase={} status={}",
                    fixture.lifecycle().phase,
                    finished.status
                );
                healthy_recovery(&fixture);
                continue;
            };
            assert_process_signaled_within(
                &first_browser.handle,
                "first browser before successor",
                PRODUCT_DEADLINE,
            );
            let census_deadline = Instant::now() + PRODUCT_DEADLINE;
            loop {
                let tree = process_tree_report(run.pid());
                if browser_count(&tree) == 0 {
                    break;
                }
                assert!(
                    Instant::now() < census_deadline,
                    "old browser family did not retire: {tree}"
                );
                std::thread::park_timeout(Duration::from_millis(10));
            }
            let second = create(&mut run);
            let second_browser = run.browser(Some(first_browser.pid), false);
            assert_ne!(
                second_browser.pid, first_browser.pid,
                "retained exited object cannot be recycled"
            );
            run.marker(
                &format!("REBIND gen=2 pid={}", second_browser.pid),
                credited,
                Instant::now() + PRODUCT_DEADLINE,
            );
            eligible += 1;
            println!(
                "KELD_GH1188_AC21_NC3 withheld={withheld} attempt={attempt} at_risk=1 old_pid={} new_pid={}",
                first_browser.pid, second_browser.pid
            );
            run.retain_descendants();
            allow_close(&mut run, &second);
            run.capture_control();
            run.send("QUIT");
            let finished = run.finish();
            assert_process_signaled(&second_browser.handle, "second browser after terminal");
            assert_retired(&second.native);
            if withheld {
                assert_stopping(&fixture, &finished, HOST_GRACE_FAULT);
                assert!(!finished.stderr.contains(&format!(
                    "RECEIPT_CREDITED gen=2 pid={}",
                    second_browser.pid
                )));
            } else {
                assert_idle(&fixture, &finished);
                assert!(finished.stderr.contains(&format!(
                    "RECEIPT_CREDITED gen=2 pid={}",
                    second_browser.pid
                )));
            }
            break;
        }
        println!("KELD_GH1188_AC21_NC3_COUNTS withheld={withheld} eligible={eligible} void={void}");
        assert!(
            eligible > 0,
            "AC21 NC3 has zero eligible prior-credit -> new-instance trials"
        );
    }
    healthy_recovery(&fixture);
    fixture.close();
}
pub(super) fn assert_ready_empty(fixture: &InstalledFacade, run: &InstalledRun<'_>) {
    assert_eq!(run.ready["ready"], true, "actual public Ready");
    assert_eq!(run.ready["readyEvents"], 1);
    assert_eq!(run.ready["windows"], serde_json::json!([]));
    assert_eq!(
        fixture.lifecycle(),
        LifecycleObservation::running_under(run.pid())
    );
}

pub(super) fn assert_idle(fixture: &InstalledFacade, finished: &FinishedRun) {
    assert!(
        finished.status.success(),
        "healthy facade failed: {}",
        finished.stderr
    );
    assert_eq!(
        fixture.lifecycle(),
        LifecycleObservation::idle(),
        "read Idle before any recovery can quarantine a dirty record"
    );
    assert!(
        finished
            .stderr
            .contains("KELD_T2_RELEASE_WITNESS completed=true")
    );
    assert_eq!(finished.accounting.0, 0);
}

pub(super) fn assert_stopping(fixture: &InstalledFacade, finished: &FinishedRun, reason: &str) {
    assert!(
        !finished.status.success(),
        "unsafe release exited normally: {}",
        finished.stderr
    );
    assert!(
        finished.stderr.contains("KELD-WV-009"),
        "missing lifecycle refusal: {}",
        finished.stderr
    );
    assert!(
        finished.stderr.contains(reason),
        "wrong first lifecycle fault: {}",
        finished.stderr
    );
    assert!(
        !finished
            .stderr
            .contains("KELD_T2_RELEASE_WITNESS completed=true")
    );
    assert_eq!(
        fixture.lifecycle().phase,
        "stopping",
        "failure must retain Stopping before a healthy successor recovers"
    );
}
