//! Approved U7/U18 fixture consumers. These are native acceptance cells, not
//! evidence that the unrun production paths satisfy the T3 admission gate.

use super::fixture::{
    BUILD_FAILURE, FacadeFixture, NATIVE_HOLD, capture, census, marker_number, number, ordered,
};
use super::{create, destroy};
use crate::support::window::{NativeWindowSnapshot, app_top_level_windows, post_close};
use crate::{PRODUCT_DEADLINE, ui_end_kill_switch};
use std::fs;
use std::time::{Duration, Instant};

/// `IsWindow` only, using the reviewed observer's conservative liveness query.
/// A failed HWND is disclosed before a complete controller tree ever exists;
/// the unused rectangle here is not a geometry observation or product fact.
fn failed_hwnd_live(hwnd: u64) -> bool {
    let observation = NativeWindowSnapshot {
        hwnd: usize::try_from(hwnd).expect("failed native HWND"),
        outer_rect: [0; 4],
        children: Vec::new(),
    };
    !observation.live_handles().is_empty()
}

fn failed_build(line: &str) -> u64 {
    line.split_whitespace()
        .find_map(|field| field.strip_prefix("build="))
        .expect("U7 failed-build token")
        .parse()
        .expect("numeric build token")
}

#[test]
fn windows_controller_failure_after_minting_releases_the_hwnd() {
    let fixture = FacadeFixture::new();
    let (mut run, _) = fixture.spawn(&[(BUILD_FAILURE, "hwnd".to_owned())]);
    run.retain_descendants();
    run.send("CREATE");
    let refused = run.read("CREATE_ERROR");
    assert_eq!(
        refused["code"], "KELD-CORE-041",
        "injected admitted build must fail at Webview"
    );
    assert_eq!(
        refused["windows"], 0,
        "failed construction published a partial facade"
    );
    let (failed, _) = run.marker("FAILED_BUILD_HWND ", 0);
    let hwnd = marker_number(&failed);
    let build = failed_build(&failed);
    assert!(
        !failed_hwnd_live(hwnd),
        "failed HWND still live at the constructor error"
    );
    assert_eq!(app_top_level_windows(run.pid()), []);
    let retry = create(&mut run);
    assert_eq!(
        retry["id"], 2,
        "failed canonical Create consumes its identity"
    );
    let native = capture(census(run.pid(), 1)[0], run.pid());
    assert!(
        !native.children.is_empty(),
        "healthy retry has no native controller"
    );
    fixture.assert_lease_held();
    run.retain_descendants();
    let profile = fixture.profile_dir();
    destroy(&mut run, number(&retry, "id"));
    assert!(native.live_handles().is_empty());
    let trace = run.quit();
    ordered(
        &trace,
        &[
            format!("FAILED_BUILD_DESTROYED {hwnd}"),
            format!("TEARDOWN_CONFIRMED {hwnd}"),
            format!("FAILED_BUILD_ANSWERED {build}"),
        ],
    );
    let answered = format!("FAILED_BUILD_ANSWERED {build}");
    assert_eq!(trace.lines().filter(|line| *line == answered).count(), 1);
    assert!(
        !profile.exists(),
        "healthy retry/quit failed to release its profile"
    );
    fixture.close();
}

#[test]
fn windows_lost_destroyed_after_a_failed_build_ends_in_a_bounded_fatal() {
    let fixture = FacadeFixture::new();
    let (mut run, _) = fixture.spawn(&[(BUILD_FAILURE, "hwnd:swallow".to_owned())]);
    run.retain_descendants();
    let started = Instant::now();
    run.send("RAW_CREATE first");
    run.read("RAW_SENT");
    let (failed, _) = run.marker("FAILED_BUILD_HWND ", 0);
    let hwnd = marker_number(&failed);
    println!("KELD_GH1188_AC16_NC3 at_risk=1 hwnd={hwnd}");
    run.send("RAW_CREATE second");
    run.read("RAW_SENT");
    let (deferred, _) = run.marker("BUILD_DEFERRED ", 0);
    let second_build = marker_number(&deferred);
    let profile = fixture.profile_dir();
    // Independent assertion of the approved 20 s releasing bound. The raw KIPC
    // call's existing 5 s deadline is not the oracle and is never extended.
    let bound = Duration::from_secs(20);
    let (status, trace) = run.finish(started + bound + PRODUCT_DEADLINE);
    assert!(
        !status.success(),
        "lost Destroyed became a successful Host retirement:\n{trace}"
    );
    assert!(
        started.elapsed() >= bound,
        "fatal occurred before the releasing bound:\n{trace}"
    );
    assert!(
        trace.contains("KELD-WV-009"),
        "bounded native-release failure absent:\n{trace}"
    );
    assert!(
        trace.contains(&format!("BUILD_REFUSED {second_build}")),
        "fault reaction lost the deferred build:\n{trace}"
    );
    assert!(
        !trace.contains("KELD_T2_RELEASE_WITNESS completed=true"),
        "unproven release credited normal shutdown:\n{trace}"
    );
    assert!(
        !failed_hwnd_live(hwnd),
        "swallow mode leaked the actual native HWND"
    );
    assert!(
        profile.exists(),
        "failed retirement falsely removed its profile"
    );
    // Removing a run-owned fault fixture is harness cleanup, never product success.
    fixture.close();
}

#[test]
fn windows_terminal_waits_for_pending_releases() {
    for add_deferred in [false, true] {
        let fixture = FacadeFixture::new();
        let (mut run, _) = fixture.spawn(&[(BUILD_FAILURE, "hwnd:defer".to_owned())]);
        run.retain_descendants();
        run.send("RAW_CREATE first");
        run.read("RAW_SENT");
        let (failed, _) = run.marker("FAILED_BUILD_HWND ", 0);
        let hwnd = marker_number(&failed);
        let first_build = failed_build(&failed);
        let second_build = if add_deferred {
            run.send("RAW_CREATE second");
            run.read("RAW_SENT");
            let (deferred, _) = run.marker("BUILD_DEFERRED ", 0);
            Some(marker_number(&deferred))
        } else {
            None
        };
        let profile = fixture.profile_dir();
        run.send("QUIT");
        // The in-flight raw Create can be interrupted by the admitted Quit tail.
        // Its TCP response is not the native release oracle (AC16 NC4).
        let (status, trace) = run.finish(Instant::now() + ui_end_kill_switch());
        assert!(
            trace.contains("TERMINAL Quit releasing=1"),
            "INVALID AC16 NC4: Quit did not meet a parked release; zero at-risk runs:\n{trace}"
        );
        println!("KELD_GH1188_AC16_NC4 deferred={add_deferred} at_risk=1 hwnd={hwnd}");
        assert!(status.success(), "terminal failed:\n{trace}");
        ordered(
            &trace,
            &[
                format!("FAILED_BUILD_HWND {hwnd}"),
                "TERMINAL Quit releasing=1".to_owned(),
                format!("DEFERRED_CONSUMED {hwnd}"),
                format!("FAILED_BUILD_ANSWERED {first_build}"),
                "KELD_T2_LOOP_EXIT ".to_owned(),
            ],
        );
        assert!(
            trace.contains(&format!("DEFERRED_DESTROYED {hwnd}")),
            "{trace}"
        );
        assert!(
            trace.contains("releasing=0"),
            "terminal exited with an owed answer:\n{trace}"
        );
        if let Some(second) = second_build {
            ordered(
                &trace,
                &[
                    format!("BUILD_DEFERRED {second}"),
                    "TERMINAL Quit releasing=1".to_owned(),
                    format!("BUILD_REFUSED {second}"),
                    format!("DEFERRED_CONSUMED {hwnd}"),
                ],
            );
            let answered = format!("FAILED_BUILD_ANSWERED {second}");
            assert_eq!(trace.lines().filter(|line| *line == answered).count(), 1);
        }
        assert!(!failed_hwnd_live(hwnd));
        assert!(
            !profile.exists(),
            "proven terminal release left the owned profile"
        );
        fixture.close();
    }
}

#[test]
fn windows_release_waits_for_native_teardown() {
    for release in ["close", "destroy", "failed-build"] {
        let fixture = FacadeFixture::new();
        let hold = fixture.project.root.path().join("native-hold");
        fs::create_dir(&hold).expect("run-owned native hold directory");
        let mut hooks = vec![(NATIVE_HOLD, format!("destroyed:{}", hold.display()))];
        if release == "failed-build" {
            hooks.push((BUILD_FAILURE, "hwnd".to_owned()));
        }
        let (mut run, _) = fixture.spawn(&hooks);
        let native = if release == "failed-build" {
            None
        } else {
            create(&mut run);
            Some(capture(census(run.pid(), 1)[0], run.pid()))
        };
        run.retain_descendants();
        let profile = fixture.profile_dir();
        match release {
            "close" => {
                run.send("ALLOW 1");
                run.read("ALLOW");
                post_close(native.as_ref().expect("created close HWND").hwnd as u64);
                assert_eq!(run.read("CLOSE_EVENT")["count"], 1);
            }
            "destroy" => {
                run.send("RAW_DESTROY 1");
                run.read("RAW_DESTROY_SENT");
            }
            _ => {
                run.send("RAW_CREATE failed");
                run.read("RAW_SENT");
            }
        }
        let (held, _) = run.marker("DESTROY_HELD ", 0);
        let hwnd = marker_number(&held);
        assert!(
            failed_hwnd_live(hwnd),
            "INVALID AC24 {release}: no TRUE IsWindow at DESTROY_HELD; zero at-risk runs"
        );
        if let Some(native) = &native {
            assert_eq!(hwnd, native.hwnd as u64);
        }
        println!("KELD_GH1188_AC24 case={release} at_risk=1 hwnd={hwnd}");
        run.assert_no_release_during_hold();
        fs::write(hold.join("release"), b"release\n").expect("release owned native hold");
        match release {
            "close" => {
                assert_eq!(run.read("CLOSED")["count"], 1);
            }
            "destroy" => {
                // Raw async Destroy's fact dispatch and promise continuations can
                // have either JS scheduling order; both must follow native teardown.
                let first = run.next();
                let second = run.next();
                let records = [first, second];
                assert!(
                    records
                        .iter()
                        .any(|(kind, value)| kind == "CLOSED" && value["count"] == 1)
                );
                assert!(
                    records
                        .iter()
                        .any(|(kind, value)| kind == "DESTROY_REPLY"
                            && value["reply"] == "Destroyed")
                );
            }
            _ => {
                assert_eq!(
                    run.read("RAW_ERROR")["code"],
                    "KELD-CORE-041",
                    "U7 fault must answer the actual build error, not a call timeout"
                );
            }
        }
        assert!(
            !failed_hwnd_live(hwnd),
            "reply published with the parent HWND still live"
        );
        if let Some(native) = &native {
            assert!(native.live_handles().is_empty());
        }
        assert_eq!(app_top_level_windows(run.pid()), []);
        let trace = run.quit();
        let before = if release == "failed-build" {
            format!("FAILED_BUILD_DESTROYED {hwnd}")
        } else {
            format!("DESTROYED {hwnd}")
        };
        let after = if release == "failed-build" {
            "FAILED_BUILD_ANSWERED ".to_owned()
        } else {
            "RELEASED ".to_owned()
        };
        ordered(
            &trace,
            &[
                before,
                format!("DESTROY_HELD {hwnd}"),
                format!("DESTROY_HOLD_END {hwnd}"),
                format!("TEARDOWN_CONFIRMED {hwnd}"),
                after,
            ],
        );
        assert!(!profile.exists());
        fixture.close();
    }
}
