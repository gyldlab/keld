//! AC25: real Win32 close and FIFO native release across a nested controller pump.

use super::destroy;
use super::fixture::{
    FacadeFixture, FacadeRun, NATIVE_HOLD, assert_retired, capture, census, marker_number, number,
    ordered, same_tree,
};
use crate::support::window::native_control::PhysicalCoordinates;
use crate::support::window::{app_top_level_windows, post_close};
use crate::{PRODUCT_DEADLINE, ui_end_kill_switch};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

#[path = "facade_window_pump_observation.rs"]
mod observation;
use observation::{held_build_or_terminal, later_bounds_after_pump, pump_record};

fn release(hold: &Path, build: u64) {
    fs::write(hold.join(format!("{build}.release")), b"release\n")
        .expect("release exactly the observed held native build");
}

fn initial_window(run: &mut FacadeRun, hold: &Path) -> (Value, usize) {
    run.send("CREATE");
    let (marker, cursor) = run.marker("BUILD_PUMP_HELD ", 0);
    let build = marker_number(&marker);
    release(hold, build);
    let (_, cursor) = run.marker(&format!("BUILD_PUMP_EXIT {build}"), cursor);
    let created = run.read("PUBLIC_CREATED");
    println!("KELD_GH1188_CREATE {created}");
    (created, cursor)
}

#[test]
#[allow(clippy::too_many_lines)] // Preserve the native FIFO acknowledgment and post-cleanup missing-event assertion.
fn windows_close_buffered_during_a_controller_pump_is_delivered_once() {
    let _coordinates = PhysicalCoordinates::enter();
    let fixture = FacadeFixture::new();
    let hold = fixture.project.root.path().join("pump-hold");
    fs::create_dir(&hold).expect("owned build-pump hold directory");
    let (mut run, _) = fixture.spawn(&[(NATIVE_HOLD, format!("build-pump:{}", hold.display()))]);
    let (created, cursor) = initial_window(&mut run, &hold);
    let first_id = number(&created, "id");
    let first_native = capture(census(run.pid(), 1)[0], run.pid());
    run.retain_descendants();
    let profile = fixture.profile_dir();

    run.send("RAW_CREATE second");
    assert_eq!(run.read("RAW_SENT")["token"], "second");
    let (marker, cursor) = run.marker("BUILD_PUMP_HELD ", cursor);
    let second_build = marker_number(&marker);
    post_close(first_native.hwnd as u64);
    let (_, close_cursor) = run.marker(&format!("PUMP_SAW_CLOSE {}", first_native.hwnd), cursor);
    println!(
        "KELD_GH1188_AC25A at_risk=1 hwnd={} build={second_build}",
        first_native.hwnd
    );
    release(&hold, second_build);
    run.marker(&format!("BUILD_PUMP_EXIT {second_build}"), close_cursor);
    // A dropped CloseRequested must not turn this cell into a missing-record
    // timeout. RAW_CREATED still arrives, and a subsequent independently moved
    // Bounds user fact proves the preceding native/Core FIFO was consumed.
    let mut closes = Vec::new();
    let built = pump_record(
        &mut run,
        "RAW_CREATED",
        &mut closes,
        Instant::now() + PRODUCT_DEADLINE,
    );
    assert_eq!(built["reply"]["variant"], "Created");
    let second_id = number(&built["reply"]["window"], "id");
    assert_ne!(first_id, second_id);
    let state = later_bounds_after_pump(&mut run, first_id, first_native.hwnd, &mut closes);
    let prior_close_count = state["close"]
        .as_u64()
        .expect("public close callback count");
    let veto = if prior_close_count > 0 {
        run.send(&format!("VERIFY_VETO {first_id}"));
        Some(pump_record(
            &mut run,
            "VETO_VERIFIED",
            &mut closes,
            Instant::now() + PRODUCT_DEADLINE,
        ))
    } else {
        None
    };
    let first_after_pump = capture(
        isize::try_from(first_native.hwnd).expect("original HWND"),
        run.pid(),
    );
    let mut original_children = first_native.children.clone();
    let mut retained_children = first_after_pump.children.clone();
    original_children.sort_unstable();
    retained_children.sort_unstable();
    assert_eq!(
        original_children, retained_children,
        "the original controller must survive"
    );
    same_tree(&first_after_pump, run.pid());
    let second_hwnd = census(run.pid(), 2)
        .into_iter()
        .find(|hwnd| usize::try_from(*hwnd).expect("HWND") != first_native.hwnd)
        .expect("independent second HWND");
    let second_native = capture(second_hwnd, run.pid());
    run.retain_descendants();
    run.send(&format!("ALLOW {first_id}"));
    run.read("ALLOW");
    post_close(first_native.hwnd as u64);
    assert_eq!(run.read("CLOSE_EVENT")["count"], prior_close_count + 1);
    assert_eq!(run.read("CLOSED")["count"], 1);
    assert_retired(&first_native);
    same_tree(&second_native, run.pid());
    destroy(&mut run, second_id);
    assert_retired(&second_native);
    let trace = run.quit();
    assert_retired(&first_native);
    assert_retired(&first_after_pump);
    assert!(!profile.exists());
    fixture.close();
    // First disputed assertion. The drop-first-post-build-close mutant reaches
    // this with its later accepted close and normal native/profile cleanup done.
    assert_eq!(
        closes.len(),
        1,
        "buffered native CloseRequested was lost or duplicated: {closes:?}; settled={state}"
    );
    assert_eq!(closes[0]["id"], first_id);
    assert_eq!(closes[0]["count"], 1);
    assert_eq!(state["close"], 1);
    assert_eq!(
        veto.expect("one actual close was vetoed")["reply"],
        "ReplyStale"
    );
    ordered(
        &trace,
        &[
            format!("BUILD_PUMP_EXIT {second_build}"),
            format!("CLOSE_DISPATCHED {}", first_native.hwnd),
        ],
    );
    let dispatched = format!("CLOSE_DISPATCHED {}", first_native.hwnd);
    assert_eq!(
        trace.lines().filter(|line| *line == dispatched).count(),
        2,
        "each actual WM_CLOSE must dispatch once:\n{trace}"
    );
}

#[test]
fn windows_builds_wait_for_a_pending_release_in_order() {
    backpressure_case(false);
}

/// AC25 NC3's timing control: under the no-backpressure mutant, one 12 s hold
/// must not create the two-hold expiry. FIFO admission itself is the test above.
#[test]
fn windows_single_held_build_stays_inside_the_release_bound() {
    backpressure_case(true);
}

#[allow(clippy::too_many_lines)] // Keep ordered native stimuli, oracles and cleanup auditable together.
fn backpressure_case(single_hold: bool) {
    let fixture = FacadeFixture::new();
    let hold = fixture.project.root.path().join("fifo-pump-hold");
    fs::create_dir(&hold).expect("owned FIFO pump hold directory");
    let (mut run, _) = fixture.spawn(&[(NATIVE_HOLD, format!("build-pump:{}", hold.display()))]);
    let (created, cursor) = initial_window(&mut run, &hold);
    let first_id = number(&created, "id");
    let first_native = capture(census(run.pid(), 1)[0], run.pid());
    run.retain_descendants();
    let profile = fixture.profile_dir();

    run.send("RAW_CREATE carrier");
    run.read("RAW_SENT");
    let (carrier, mut cursor) = run.marker("BUILD_PUMP_HELD ", cursor);
    let carrier = marker_number(&carrier);
    run.send(&format!("RAW_DESTROY {first_id}"));
    run.read("RAW_DESTROY_SENT");
    run.send("RAW_CREATE second");
    run.read("RAW_SENT");
    run.send("RAW_CREATE third");
    run.read("RAW_SENT");
    for command in ["DestroyWindow", "CreateWindow", "CreateWindow"] {
        let (posted, next) = run.marker("BRIDGE_POSTED ", cursor);
        assert!(
            posted.starts_with(&format!("BRIDGE_POSTED {command}")),
            "fixture command did not enter the live Tao bridge FIFO: {posted}"
        );
        cursor = next;
    }
    run.capture_control();
    release(&hold, carrier);
    let (_, next) = run.marker(&format!("BUILD_PUMP_EXIT {carrier}"), cursor);
    cursor = next;

    let mut held_builds = Vec::new();
    let mut hold_times = Vec::new();
    for index in 0..2 {
        let Some((marker, next)) = held_build_or_terminal(&mut run, cursor) else {
            break;
        };
        let build = marker_number(&marker);
        cursor = next;
        let trace = run.trace();
        let parent_gone = !first_native.live_handles().contains(&first_native.hwnd);
        let destroyed = format!("DESTROYED {}", first_native.hwnd);
        let deferred_destroy = !trace.lines().any(|line| line == destroyed);
        println!(
            "KELD_GH1188_AC25_NC3 build={build} native_parent_gone={parent_gone} destroyed_not_dispatched={deferred_destroy}"
        );
        let started = Instant::now();
        // The approved backlog schedule: each separate build holds 12 s of its
        // existing 15 s budget. The 5 s raw-call timeout is deliberately not an oracle.
        if index == 0 || !single_hold {
            thread::sleep(Duration::from_secs(12));
        }
        hold_times.push(started.elapsed());
        release(&hold, build);
        held_builds.push(build);
        // Defer product assertions until Host exit. The no-backpressure mutant
        // must first fail its native fault/status assertion, not an incidental reply.
        if let Some((_, next)) = run.marker_or_exit(&format!("BUILD_PUMP_EXIT {build}"), cursor) {
            cursor = next;
        }
    }
    let host_pid = run.pid();
    let observed_census = app_top_level_windows(host_pid).len();
    let request = if run
        .host
        .child_mut()
        .try_wait()
        .expect("native Host status")
        .is_none()
    {
        Some(run.try_send("QUIT_RAW"))
    } else {
        None
    };
    let (status, trace, control) = run.finish_observed(Instant::now() + ui_end_kill_switch());
    assert_retired(&first_native);
    assert_eq!(app_top_level_windows(host_pid), []);
    // Retain product deletion as an observation before removing the isolated
    // fixture. An expected failed release keeps its profile; test cleanup must
    // not turn that removal into a product success.
    let profile_retained = profile.exists();
    println!(
        "KELD_GH1188_AC25_CLEANUP native_windows=0 profile_retained={profile_retained} held_builds={held_builds:?}",
    );
    fixture.close();
    assert!(
        status.success(),
        "native release backlog faulted:\n{trace}\n{control}"
    );
    assert!(
        !trace.contains("KELD-WV-009"),
        "native release fault was swallowed:\n{trace}"
    );
    assert!(
        request.is_some_and(|result| result.is_ok()),
        "healthy Host did not receive raw Quit"
    );
    assert_eq!(held_builds.len(), 2);
    assert!(hold_times[0] >= Duration::from_secs(12));
    let second = held_builds[0];
    let third = held_builds[1];
    if single_hold {
        assert!(
            hold_times[1] < Duration::from_secs(12),
            "INVALID single-hold control: scheduler created a second long hold"
        );
        println!(
            "KELD_GH1188_AC25_SINGLE_HOLD no_release_fault=true first_ms={} second_ms={}",
            hold_times[0].as_millis(),
            hold_times[1].as_millis()
        );
    } else {
        assert!(hold_times[1] >= Duration::from_secs(12));
        ordered(
            &trace,
            &[
                format!("BUILD_PUMP_EXIT {carrier}"),
                format!("BUILD_DEFERRED {second}"),
                format!("BUILD_DEFERRED {third}"),
                format!("DESTROYED {}", first_native.hwnd),
                format!("TEARDOWN_CONFIRMED {}", first_native.hwnd),
                "RELEASED ".to_owned(),
                format!("BUILD_PUMP_HELD {second}"),
                format!("BUILD_PUMP_EXIT {second}"),
                format!("BUILD_PUMP_HELD {third}"),
                format!("BUILD_PUMP_EXIT {third}"),
            ],
        );
        println!(
            "KELD_GH1188_AC25B at_risk=1 second_hold_ms={} third_hold_ms={}",
            hold_times[0].as_millis(),
            hold_times[1].as_millis()
        );
    }
    let closed = control
        .lines()
        .filter_map(|line| line.strip_prefix("CLOSED "))
        .map(|json| serde_json::from_str::<Value>(json).expect("closed receipt JSON"))
        .filter(|value| value["id"] == first_id)
        .count();
    assert_eq!(
        closed, 1,
        "A closed more than once or never closed:\n{control}"
    );
    assert_eq!(
        observed_census, 3,
        "carrier Z, B and C must be independent live native windows"
    );
    assert!(
        !profile_retained,
        "healthy product release kept the ephemeral profile"
    );
}
