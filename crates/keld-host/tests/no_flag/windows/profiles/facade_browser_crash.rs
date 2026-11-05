//! Native browser failure, first fault, historical admission fence and recovery.

use super::facade_browser::{
    HOST_ABNORMAL_FAULT, assert_ready_empty, assert_stopping, create, healthy_recovery,
};
use super::facade_browser_fixture::{InstalledFacade, field};
use crate::PRODUCT_DEADLINE;
use crate::facade_window::fixture::assert_retired;
use crate::support::process::{assert_process_signaled, terminate_test_process};
use std::time::Instant;

#[test]
#[ignore = "signed exact-head profile-hook/root Host and installer required"]
fn windows_installed_browser_crash_stops_admission_without_starting_a_successor() {
    let fixture = InstalledFacade::new();
    let mut eligible = 0;
    let mut void = 0;
    for attempt in 1..=3 {
        let mut run = fixture.start(&[]);
        assert_ready_empty(&fixture, &run);
        let window = create(&mut run);
        let browser = run.browser(None, true);
        run.marker(
            &format!("REBIND gen=1 pid={}", browser.pid),
            0,
            Instant::now() + PRODUCT_DEADLINE,
        );
        let baseline = run.settled_accounting(window.id);
        let bun_pid = run.bun_pid;
        run.capture_control();

        // The native query validated the exact installed Host parent and exclusive
        // profile before this retained handle was granted the terminate right.
        // This is the sole intentionally killed process, never a name/PID-wide kill.
        terminate_test_process(&browser.handle);
        assert_process_signaled(&browser.handle, "owned browser intentionally failed");
        let killed_at = browser.exit();
        assert_ne!(killed_at, 0);
        let abnormal =
            run.marker_if_delivered("ABNORMAL source=", 0, Instant::now() + PRODUCT_DEADLINE);
        let send = abnormal.as_ref().map(|(line, _)| {
            assert_eq!(field(line, "gen"), 1);
            assert_eq!(field(line, "pid"), browser.pid);
            assert!(
                line.contains("source=browser-exited") || line.contains("source=process-failed"),
                "unrecognized native browser failure source: {line}"
            );
            // CREATE_ATTEMPT is emitted immediately before the real public
            // constructor. Merely writing to a dying TCP link is not eligibility.
            run.try_send("CREATE_CHECKED")
        });
        let finished = run.finish();
        assert_stopping(&fixture, &finished, HOST_ABNORMAL_FAULT);
        assert_retired(&window.native);
        assert_process_signaled(&browser.handle, "failed browser remains retired");
        assert!(
            !finished
                .stderr
                .lines()
                .any(|line| line.starts_with("REBIND gen=2 ")),
            "first browser fault admitted a new binding: {}",
            finished.stderr
        );
        assert!(
            !finished
                .unread_control
                .lines()
                .any(|line| line.starts_with("PUBLIC_CREATED ")),
            "public BrowserWindow succeeded after the observed fault: {}",
            finished.unread_control
        );

        // TotalProcesses retains short-lived process history. The fixture has
        // held the existing non-breakaway Job since before any app resource.
        // A final empty process snapshot alone could not prove this assertion.
        assert_eq!(finished.accounting.0, 0);
        assert_eq!(
            finished.accounting.1, baseline.1,
            "no-successor proof failed: the retained Job admitted a process after the settled pre-kill baseline; a larger total cannot identify that process as a browser"
        );
        let attempts = finished
            .unread_control
            .lines()
            .filter_map(|line| line.strip_prefix("CREATE_ATTEMPT "))
            .map(|json| {
                serde_json::from_str::<serde_json::Value>(json)
                    .expect("public constructor attempt JSON")
            })
            .collect::<Vec<_>>();
        assert!(
            attempts.len() <= 1,
            "duplicate constructor attempt: {attempts:?}"
        );
        let constructor_attempted = attempts.first().is_some_and(|value| {
            assert_eq!(
                value["pid"], bun_pid,
                "the same retained Bun made the attempt"
            );
            true
        });
        let at_risk = matches!(send, Some(Ok(()))) && constructor_attempted;
        let first_source = abnormal
            .as_ref()
            .map_or("not-delivered-before-stimulus", |(line, _)| line);
        if at_risk {
            eligible += 1;
        } else {
            void += 1;
        }
        println!(
            "KELD_GH1188_AC21_NC5 attempt={attempt} at_risk={} source={first_source} browser_pid={} killed_at={} baseline_active={} baseline_total={} final_active={} final_total={} constructor_attempted={constructor_attempted}",
            u8::from(at_risk),
            browser.pid,
            killed_at,
            baseline.0,
            baseline.1,
            finished.accounting.0,
            finished.accounting.1,
        );
        // Stopping was observed above before this operation can quarantine the
        // dirty predecessor. The same signed identity and profile path recover.
        healthy_recovery(&fixture);
        if at_risk {
            break;
        }
    }
    println!("KELD_GH1188_AC21_NC5_COUNTS eligible={eligible} void={void}");
    assert!(
        eligible > 0,
        "AC21 NC5 has zero trials with the native abnormal receipt observed before an actual public constructor attempt"
    );
    fixture.close();
}
