//! AC18 NC2 and AC27(e): actual managed veto and persistent release witnesses.
//! Uses the installed facade and `release_witness` owners; no new launch policy.

use super::super::facade_browser::{
    allow_close, assert_idle, assert_ready_empty, create, healthy_recovery,
};
use super::super::facade_browser_fixture::{InstalledFacade, InstalledRun, number};
use super::super::sessions::LifecycleObservation;
use super::{ENVIRONMENT_DESCENDANT, HOLD, observe_environment_child, observe_loop_end};
use crate::PRODUCT_DEADLINE;
use crate::facade_window::fixture::{assert_retired, census, ordered, same_tree};
use crate::support::installed::fixture_env;
use crate::support::process::{
    assert_process_signaled, open_process_for_wait, terminate_test_process, wait_for_process_signal,
};
use crate::support::window::{NativeWindowSnapshot, post_close};
use serde_json::Value;
use std::os::windows::io::OwnedHandle;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};

#[test]
#[ignore = "signed exact-head profile-hook/root Host and installer required"]
fn windows_installed_veto_keeps_the_live_profile_running() {
    let fixture = InstalledFacade::new();
    let mut run = fixture.start(&[]);
    assert_ready_empty(&fixture, &run);
    let host_pid = run.pid();
    let window = create(&mut run);
    let browser = run.browser(None, false);
    run.retain_descendants();

    println!(
        "KELD_GH1188_VETO_PRE_STIMULUS hwnd={} native={:?}",
        window.native.hwnd,
        NativeWindowSnapshot::capture(
            u64::try_from(window.native.hwnd).expect("retained owned HWND"),
            run.pid(),
        ),
    );

    post_close(u64::try_from(window.native.hwnd).expect("retained owned HWND"));
    let requested = run.read("CLOSE_EVENT");
    assert_eq!(number(&requested, "id"), window.id);
    assert_eq!(requested["count"], 1);
    run.send(&format!("VERIFY_VETO {}", window.id));
    let veto = run.read("VETO_VERIFIED");
    assert_eq!(veto["reply"], "ReplyStale");
    assert_eq!(veto["pending"], Value::Null);
    assert_eq!(veto["live"], true);
    assert_eq!(veto["contentsLive"], true);
    assert_eq!(veto["closed"], 0);
    assert_eq!(veto["contentsDestroyed"], 0);
    assert_eq!(veto["generation"], window.value["generation"]);
    // Cancel and the subsequent stale Allow have both been consumed. No terminal
    // or build is pending, so the persistent record is quiescent, not polled.
    let veto_record = fixture.lifecycle();
    same_tree(&window.native, run.pid());
    assert_eq!(wait_for_process_signal(&browser.handle, 0), WAIT_TIMEOUT);
    let retained_browser = run.browser(Some(browser.pid), false);
    assert_eq!(retained_browser.pid, browser.pid);

    run.send(&format!("ALLOW {}", window.id));
    assert_eq!(number(&run.read("ALLOW"), "id"), window.id);
    post_close(u64::try_from(window.native.hwnd).expect("same retained HWND"));
    let accepted = run.read("CLOSE_EVENT");
    assert_eq!(number(&accepted, "id"), window.id);
    assert_eq!(accepted["count"], 2);
    let closed = run.read("CLOSED");
    assert_eq!(number(&closed, "id"), window.id);
    assert_eq!(closed["count"], 1);
    assert_eq!(closed["contentsDestroyed"], true);
    assert_retired(&window.native);
    assert_eq!(census(run.pid(), 0), Vec::<isize>::new());
    run.send("QUIT");
    run.read("QUIT_ACCEPTED");
    let finished = run.finish();
    assert_process_signaled(&browser.handle, "vetoed installed browser after Quit");
    assert_process_signaled(&retained_browser.handle, "retained original browser");
    assert_idle(&fixture, &finished);
    println!(
        "KELD_GH1188_AC18_NC2 at_risk=1 host={host_pid} hwnd={} id={} generation={} veto_phase={} after=idle",
        window.native.hwnd, window.id, window.value["generation"], veto_record.phase,
    );
    healthy_recovery(&fixture);
    fixture.close();
    // The Stopping-on-CloseRequested mutant must fail this actual record
    // assertion after the owned native family and profile resources are retired.
    assert_eq!(
        veto_record,
        LifecycleObservation::running_under(host_pid),
        "an actual veto must preserve Running while the original controller lives"
    );
}

#[test]
#[ignore = "signed exact-head profile-hook/root Host, installer and native D16 helper required"]
fn windows_installed_managed_release_waits_for_non_family_retirement() {
    let fixture = InstalledFacade::new();
    let helper = fixture_env("KELD_T2_HELD_DESCENDANT_HELPER");
    let helper = helper.to_str().expect("native D16 helper path is Unicode");
    assert!(
        std::path::Path::new(helper).is_file(),
        "D16 helper is absent"
    );
    let mut eligible = None;
    let mut void = 0;
    for attempt in 1..=3 {
        let mut run = fixture.start(&[(ENVIRONMENT_DESCENDANT, helper)]);
        assert_ready_empty(&fixture, &run);
        let window = create(&mut run);
        let browser = run.browser(None, false);
        run.marker(
            &format!("REBIND gen=1 pid={}", browser.pid),
            0,
            Instant::now() + PRODUCT_DEADLINE,
        );
        let (helper_pid, helper_handle, cursor) = observe_environment_child(&mut run.host, 0);
        run.retain_descendants();
        allow_close(&mut run, &window);
        // Keep the original control reader draining while the held member delays
        // release. A later Winsock reset cannot erase an unread terminal record.
        run.capture_control();
        run.send("QUIT");
        let held = observe_managed_hold(&mut run, &fixture, &helper_handle, cursor);
        let finished = run.finish();
        assert_process_signaled(&helper_handle, "owned D16 helper after release");
        assert_process_signaled(&browser.handle, "managed browser after release");
        assert_retired(&window.native);
        assert_idle(&fixture, &finished);
        ordered(
            &finished.stderr,
            &[
                format!("REBIND gen=1 pid={}", browser.pid),
                format!("RECEIPT_CREDITED gen=1 pid={}", browser.pid),
                "KELD_T2_LOOP_EXIT ".to_owned(),
            ],
        );
        println!(
            "KELD_GH1188_AC27_MANAGED attempt={attempt} at_risk={} helper={helper_pid} browser={} hold_ms={} host_wait={} helper_after={} phase={:?} counts={}",
            u8::from(held.helper_at_readback),
            browser.pid,
            held.elapsed.as_millis(),
            held.host_wait,
            held.helper_after,
            held.phase,
            held.counts,
        );
        healthy_recovery(&fixture);
        if held.helper_at_readback {
            eligible = Some(held);
            break;
        }
        void += 1;
        println!(
            "KELD_GH1188_AC27_MANAGED_VOID attempt={attempt} cause=helper-not-live-at-loop-readback"
        );
    }
    fixture.close();
    println!(
        "KELD_GH1188_AC27_MANAGED_COUNTS eligible={} void={void}",
        u8::from(eligible.is_some()),
    );
    let held = eligible.expect("AC27(e) requires a live helper at actual loop-exit readback");
    // All normal cleanup and Idle checks above precede the disputed observations.
    assert_eq!(
        held.phase.as_ref().map(|record| record.phase.as_str()),
        Some("stopping"),
        "managed release credited Idle while the owned non-family helper lived"
    );
    assert_eq!(
        held.host_wait, WAIT_TIMEOUT,
        "managed release completed before the held native member retired"
    );
    assert!(
        held.elapsed >= HOLD,
        "the specified two-second hold was not observed"
    );
    assert_eq!(
        held.helper_after, WAIT_TIMEOUT,
        "the helper did not survive the hold"
    );
}

struct HeldRelease {
    counts: String,
    helper_at_readback: bool,
    phase: Option<LifecycleObservation>,
    host_wait: u32,
    helper_after: u32,
    elapsed: Duration,
}

fn observe_managed_hold(
    run: &mut InstalledRun<'_>,
    fixture: &InstalledFacade,
    helper: &OwnedHandle,
    cursor: usize,
) -> HeldRelease {
    let host = open_process_for_wait(run.pid(), false);
    // The existing witness observer reads the exact actual arming cause and
    // waits for witness entry. The managed kind has one created controller.
    let (counts, _) = observe_loop_end(&mut run.host, cursor, 1, "Quit");
    let helper_at_readback = wait_for_process_signal(helper, 0) == WAIT_TIMEOUT;
    let start = Instant::now();
    let host_wait = wait_for_process_signal(
        &host,
        if helper_at_readback {
            u32::try_from(HOLD.as_millis()).expect("hold fits native wait")
        } else {
            0
        },
    );
    let elapsed = start.elapsed();
    let helper_after = wait_for_process_signal(helper, 0);
    // Healthy: the retained live helper blocks the witness, so no record writer
    // can run. Bypass mutant: an exited Host is also write-free. No repeated
    // opening of a record races its atomic rename.
    let phase = if helper_after == WAIT_TIMEOUT || host_wait == WAIT_OBJECT_0 {
        Some(fixture.lifecycle())
    } else {
        None
    };
    if helper_after == WAIT_TIMEOUT {
        terminate_test_process(helper);
    }
    assert_process_signaled(helper, "D16 helper after explicit owned release");
    HeldRelease {
        counts,
        helper_at_readback,
        phase,
        host_wait,
        helper_after,
        elapsed,
    }
}
