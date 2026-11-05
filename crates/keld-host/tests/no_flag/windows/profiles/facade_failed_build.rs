//! AC18 NC5: a failed managed construction cannot erase native release debt.

use super::facade_browser::{
    HOST_GRACE_FAULT, HOST_UNIDENTIFIED_FAULT, assert_idle, assert_ready_empty, assert_stopping,
    healthy_recovery,
};
use super::facade_browser_fixture::{ALL_RECEIPTS_WITHHELD, FAIL_BUILD, InstalledFacade, field};
use crate::PRODUCT_DEADLINE;
use crate::facade_window::fixture::census;
use std::time::Instant;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;

#[test]
#[ignore = "signed exact-head profile-hook/root Host and installer required"]
fn windows_installed_managed_zero_window_quit_releases_profile() {
    let fixture = InstalledFacade::new();
    let mut run = fixture.start(&[]);
    assert_ready_empty(&fixture, &run);
    assert_eq!(census(run.pid(), 0), []);
    run.retain_descendants();
    run.send("QUIT");
    run.read("QUIT_ACCEPTED");
    let finished = run.finish();
    assert_idle(&fixture, &finished);
    assert!(
        finished
            .stderr
            .contains("environments=1 controllers=0 armed_by=Quit")
    );
    println!("KELD_GH1188_AC18_ZERO at_risk=1 phase=idle");
    healthy_recovery(&fixture);
    fixture.close();
}

#[test]
#[ignore = "signed exact-head profile-hook/root Host and installer required"]
fn windows_installed_failed_view_keeps_its_browser_release_obligation() {
    let fixture = InstalledFacade::new();
    for withheld in [false, true, false] {
        let mut hooks = vec![(FAIL_BUILD, "view")];
        if withheld {
            hooks.push((ALL_RECEIPTS_WITHHELD, "1"));
        }
        let mut run = fixture.start(&hooks);
        assert_ready_empty(&fixture, &run);
        run.retain_descendants();
        run.send("CREATE");
        let refused = run.read("CREATE_ERROR");
        assert_eq!(refused["code"], "KELD-CORE-041");
        assert_eq!(
            refused["windows"], 0,
            "failed construction published a public window"
        );
        assert_eq!(census(run.pid(), 0), []);
        run.capture_control();
        run.send("QUIT");
        // A checked release failure may terminate the link before Quit replies.
        let finished = run.finish();
        let (hwnd, build) = failed_native_construction(&finished.stderr, "view");
        assert_failed_hwnd_retired(hwnd);
        if withheld {
            // Check the actual incorrect Idle/exit-zero behavior before looking
            // for REBIND. A skip-binding mutant must die on this policy assertion,
            // not become an ineligible trial by removing its own diagnostic.
            assert_stopping(&fixture, &finished, HOST_GRACE_FAULT);
        } else {
            assert_idle(&fixture, &finished);
        }
        let bindings = finished
            .stderr
            .lines()
            .filter(|line| line.starts_with("REBIND "))
            .collect::<Vec<_>>();
        assert_eq!(
            bindings.len(),
            1,
            "the failed View still binds its native browser"
        );
        assert_eq!(field(bindings[0], "gen"), 1);
        assert_ne!(field(bindings[0], "pid"), 0);
        if !withheld {
            assert!(
                finished.stderr.contains(&format!(
                    "RECEIPT_CREDITED gen=1 pid={}",
                    field(bindings[0], "pid"),
                )),
                "healthy failed construction never earned its release receipt"
            );
        }
        println!(
            "KELD_GH1188_AC18_NC5 point=view withheld={withheld} at_risk=1 hwnd={hwnd} build={build} phase={} job_total={}",
            fixture.lifecycle().phase,
            finished.accounting.1,
        );
        healthy_recovery(&fixture);
    }
    fixture.close();
}

#[test]
#[ignore = "signed exact-head profile-hook/root Host and installer required"]
fn windows_installed_unidentified_controller_never_gets_zero_window_release_credit() {
    let fixture = InstalledFacade::new();
    let mut run = fixture.start(&[(FAIL_BUILD, "webview")]);
    assert_ready_empty(&fixture, &run);
    run.retain_descendants();
    run.capture_control();
    run.send("CREATE");
    // This hook is reached only after actual controller completion. The marker
    // survives the intended mutation that removes the UnidentifiedController
    // latch. Do not predicate eligibility on that latch or on a REBIND line.
    let (failed, _) = run.marker("FAILED_BUILD_HWND ", 0, Instant::now() + PRODUCT_DEADLINE);
    assert!(failed.ends_with("point=webview"), "{failed}");
    // Always try the ordinary public Quit. In the missing-latch mutant the Host
    // is still live, and must finish with the wrong Idle rather than time out.
    let quit_sent = run.try_send("QUIT").is_ok();
    let finished = run.finish();
    let (hwnd, build) = failed_native_construction(&finished.stderr, "webview");
    assert_failed_hwnd_retired(hwnd);
    assert!(
        !finished
            .unread_control
            .lines()
            .any(|line| line.starts_with("PUBLIC_CREATED ")),
        "unidentified controller became a public window: {}",
        finished.unread_control
    );
    assert_stopping(&fixture, &finished, HOST_UNIDENTIFIED_FAULT);
    println!(
        "KELD_GH1188_AC18_NC5 point=webview withheld=false at_risk=1 hwnd={hwnd} build={build} phase=stopping quit_sent={quit_sent} job_total={}",
        finished.accounting.1,
    );
    healthy_recovery(&fixture);
    fixture.close();
}

fn failed_native_construction(trace: &str, point: &str) -> (usize, u32) {
    let lines = trace
        .lines()
        .filter(|line| line.starts_with("FAILED_BUILD_HWND "))
        .collect::<Vec<_>>();
    assert_eq!(
        lines.len(),
        1,
        "exactly one actually reached native fault point: {trace}"
    );
    let line = lines[0];
    assert!(line.ends_with(&format!("point={point}")), "{line}");
    let hwnd: usize = line
        .split_whitespace()
        .nth(1)
        .expect("failed HWND")
        .parse()
        .expect("native HWND");
    assert_ne!(hwnd, 0);
    let build = field(line, "build");
    assert_eq!(
        trace
            .lines()
            .filter(|line| *line == format!("FAILED_BUILD_DESTROYED {hwnd}"))
            .count(),
        1
    );
    assert_eq!(
        trace
            .lines()
            .filter(|line| *line == format!("FAILED_BUILD_ANSWERED {build}"))
            .count(),
        1
    );
    let marker = trace.find(line).expect("point marker in trace");
    let destroyed = trace
        .find(&format!("FAILED_BUILD_DESTROYED {hwnd}"))
        .expect("Destroyed receipt");
    let answered = trace
        .find(&format!("FAILED_BUILD_ANSWERED {build}"))
        .expect("failed build answer");
    assert!(
        marker < destroyed && destroyed < answered,
        "native custody order: {trace}"
    );
    assert!(
        !trace.lines().any(|line| line.starts_with("RELEASED ")),
        "a failed build was published as a live view release: {trace}"
    );
    (hwnd, build)
}

fn assert_failed_hwnd_retired(hwnd: usize) {
    // SAFETY: this is an independent query after the retained Host has exited.
    // It does not signal a possibly recycled HWND or infer window authority.
    // https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-iswindow
    assert_eq!(
        unsafe { IsWindow(hwnd as HWND) },
        0,
        "failed-build HWND survived native teardown"
    );
}
